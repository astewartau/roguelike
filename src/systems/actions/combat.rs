//! Melee attacks, cleave, and stun.

use crate::engine::EffectCtx;
use hecs::{Entity, World};
use rand::Rng;

use crate::components::{
    Attackable, CompanionAI, EffectType, Equipment, Health, LungeAnimation, Player, Position,
    SecondaryAbility, TamedBy,
};
use crate::constants::*;
use crate::events::{EventQueue, GameEvent, MissReason};
use crate::queries;

use super::{interrupt_life_drain_on_damage, ActionResult};

/// Whether a melee attack from `attacker` on `target` connects *now*: the
/// target still exists, is still attackable and alive, and stands within
/// `MELEE_REACH` (Chebyshev, so diagonals count) of the attacker.
///
/// `ActionType::Attack` locks its target when the swing starts, but the
/// effect is applied when it completes; anything can happen in between.
/// Returns `Err(reason)` describing why the swing would miss.
pub fn melee_reach_check(world: &World, attacker: Entity, target: Entity) -> Result<(), MissReason> {
    let attackable = world.get::<&Attackable>(target).is_ok();
    let alive = world.get::<&Health>(target).map(|h| h.current > 0).unwrap_or(true);
    let (Some(a), Some(t)) = (
        queries::get_entity_position(world, attacker),
        queries::get_entity_position(world, target),
    ) else {
        return Err(MissReason::TargetGone);
    };
    if !attackable || !alive {
        return Err(MissReason::TargetGone);
    }
    let distance = (a.0 - t.0).abs().max((a.1 - t.1).abs());
    if distance > MELEE_REACH {
        return Err(MissReason::OutOfReach);
    }
    Ok(())
}

/// Apply attack effect.
///
/// The target was chosen when the attack started; if it has since moved out
/// of reach or gone, the swing whiffs (see [`melee_reach_check`]).
pub fn apply_attack(ctx: &mut EffectCtx, attacker: Entity, target: Entity) -> ActionResult {
    let EffectCtx { world, grid, spatial: spatial_cache, events, rng } = ctx;
    let (world, grid) = (&mut **world, &mut **grid);
    let (spatial_cache, events, rng) = (&mut **spatial_cache, &mut **events, &mut **rng);

    if queries::get_entity_position(world, attacker).is_none() {
        return ActionResult::Invalid;
    }

    // Get target position for VFX
    let target_tile = queries::get_entity_position(world, target);

    if let Err(reason) = melee_reach_check(world, attacker, target) {
        let target_pos = target_tile.map(|p| (p.0 as f32 + 0.5, p.1 as f32 + 0.5));
        // Swing at the air toward where the target is (or was) now, like the
        // AttackDirection whiff. A target with no position left has no
        // direction to swing at.
        if let Some((tx, ty)) = target_pos {
            let _ = world.insert_one(attacker, LungeAnimation::new(tx, ty));
        }
        events.push(GameEvent::AttackMissed { attacker, target, target_pos, reason });
        return ActionResult::Completed;
    }

    let target_pos = match target_tile {
        Some(p) => (p.0 as f32, p.1 as f32),
        None => return ActionResult::Invalid,
    };

    // Attacker-side buff (defender-side reductions are handled centrally by
    // combat::apply_damage).
    let has_strength_boost = queries::has_status_effect(world, attacker, EffectType::Strengthened);

    // Calculate damage (effective stats include stat affixes on equipped gear)
    let base_damage = {
        let strength = queries::effective_stats(world, attacker).strength;
        let (weapon_damage, affix_damage) = world
            .get::<&Equipment>(attacker)
            .ok()
            .map(|e| {
                (
                    e.get_melee().map(|w| w.base_damage + w.damage_bonus).unwrap_or(UNARMED_DAMAGE),
                    e.affix_damage_bonus(),
                )
            })
            .unwrap_or((UNARMED_DAMAGE, 0));

        weapon_damage + affix_damage + (strength - 10) / 2
    };

    // Apply damage variance and crit (attacker side)
    let damage_mult = rng.gen_range(COMBAT_DAMAGE_MIN_MULT..=COMBAT_DAMAGE_MAX_MULT);
    let is_crit = rng.gen::<f32>() < COMBAT_CRIT_CHANCE;
    let mut raw = (base_damage as f32 * damage_mult) as i32;
    if is_crit {
        raw = (raw as f32 * COMBAT_CRIT_MULTIPLIER) as i32;
    }
    if has_strength_boost {
        raw = (raw as f32 * STRENGTH_DAMAGE_MULTIPLIER) as i32;
    }
    // Conditional weapon affixes (LowHealthDamage)
    raw = (raw as f32 * crate::systems::combat::attacker_conditional_damage_mult(world, attacker))
        as i32;

    // A guarding defender blocks most of the blow and staggers the attacker.
    // (Defender-side, but melee-only, so it lives here rather than in the
    // source-agnostic apply_damage: arrows, fire and DoTs are not blocked.)
    if super::is_guarding(world, target) {
        raw = super::resolve_guard_block(world, events, attacker, target, raw);
    }

    // Apply damage to target (handles invulnerability, armor defense, Protected/Barkskin)
    let damage = crate::systems::combat::apply_damage(world, target, raw, rng, events);

    // CursedLoud gear rings out: wake enemies in a doubled radius on top of
    // the standard melee-noise wake inside apply_damage.
    let noise = crate::systems::combat::attack_noise_radius(world, attacker, MELEE_NOISE_RADIUS);
    if noise > MELEE_NOISE_RADIUS {
        let pos = (target_pos.0 as i32, target_pos.1 as i32);
        crate::systems::ai::wake_enemies_in_radius(world, pos, noise);
    }

    // A burning combatant can set the other alight on a melee hit.
    crate::systems::fire::try_combat_ignite(world, attacker, target, rng);
    crate::systems::fire::try_combat_ignite(world, target, attacker, rng);

    // Resolve weapon on-hit affixes (ignite/slow/fear/lifesteal/knockback/kill-heal)
    crate::systems::combat::resolve_weapon_on_hit(
        &mut EffectCtx { world, grid, spatial: spatial_cache, events, rng },
        attacker,
        target,
        damage,
    );

    // Natural-weapon on-hit effects: venom (Giant Spider slows and poisons,
    // Lesser Giant Spider poisons) and wounds (rats may cause Bleeding).
    // Applied directly here since enemy claws/fangs are not item instances
    // with on-hit affixes. Newly gained DoTs are announced after the hit line.
    let mut gained: Vec<EffectType> = Vec::new();
    if damage > 0 {
        let target_alive = world
            .get::<&Health>(target)
            .map(|h| h.current > 0)
            .unwrap_or(false);
        let venom = world
            .get::<&crate::components::Venomous>(attacker)
            .ok()
            .map(|v| *v);
        let bleed_chance = world
            .get::<&crate::components::Lacerating>(attacker)
            .ok()
            .map(|l| l.bleed_chance);
        let mut afflict = |world: &mut hecs::World, effect: EffectType, duration: f32| {
            let had = crate::systems::effects::entity_has_effect(world, target, effect);
            if crate::systems::effects::add_effect_to_entity(world, target, effect, duration) && !had {
                gained.push(effect);
            }
        };
        if let (true, Some(v)) = (target_alive, venom) {
            if v.slow_duration > 0.0 {
                crate::systems::effects::add_effect_to_entity(
                    world, target, EffectType::Slowed, v.slow_duration,
                );
            }
            if v.poison_duration > 0.0 {
                afflict(world, EffectType::Poisoned, v.poison_duration);
            }
        }
        if let (true, Some(chance)) = (target_alive, bleed_chance) {
            if rng.gen::<f32>() < chance {
                afflict(world, EffectType::Bleeding, BLEED_DURATION);
            }
        }
    }

    // Interrupt life drain if target was channeling
    interrupt_life_drain_on_damage(world, target, events);

    // Generate threat on the target for the attacker (enemy gains threat on whoever hit it)
    let threat_amount = damage as f32 * THREAT_PER_DAMAGE;
    crate::systems::ai::generate_threat(world, target, attacker, threat_amount);
    crate::systems::ai::generate_companion_threat(world, target, attacker, threat_amount);

    // If target is the player, all companions become aware of the attacker
    if world.get::<&Player>(target).is_ok() {
        let comp_ids: Vec<Entity> = world.query::<&CompanionAI>().iter().map(|(id, _)| id).collect();
        for comp_id in comp_ids {
            crate::systems::ai::generate_companion_threat(world, comp_id, attacker, threat_amount);
        }
    }
    // If attacker is the player, all companions gain awareness of the target
    if world.get::<&Player>(attacker).is_ok() {
        let comp_ids: Vec<Entity> = world.query::<&CompanionAI>().iter().map(|(id, _)| id).collect();
        for comp_id in comp_ids {
            crate::systems::ai::generate_companion_threat(world, comp_id, target, threat_amount * THREAT_COMPANION_ASSIST_MULT);
        }
    }

    // Add lunge animation to attacker
    let _ = world.insert_one(attacker, LungeAnimation::new(target_pos.0 + 0.5, target_pos.1 + 0.5));

    // Emit attack event
    events.push(GameEvent::AttackHit {
        attacker,
        target,
        target_pos: (target_pos.0 + 0.5, target_pos.1 + 0.5),
        damage,
        kind: crate::events::DamageKind::Melee,
        crit: is_crit && damage > 0,
    });
    for effect in gained {
        events.push(GameEvent::StatusEffectGained { entity: target, effect });
    }

    // A thorny defender bites back (after the hit, so the log reads in order).
    super::reflect_thorns(
        &mut EffectCtx { world, grid, spatial: spatial_cache, events, rng },
        attacker,
        target,
    );

    ActionResult::Completed
}

/// Apply attack direction effect - attacks whatever is at the target tile, or whiffs
pub fn apply_attack_direction(
    ctx: &mut EffectCtx,
    attacker: Entity,
    dx: i32,
    dy: i32,
) -> ActionResult {
    let EffectCtx { world, grid, spatial: spatial_cache, events, rng } = ctx;
    let (world, grid) = (&mut **world, &mut **grid);
    let (spatial_cache, events, rng) = (&mut **spatial_cache, &mut **events, &mut **rng);

    // Get attacker position
    let attacker_pos = match queries::get_entity_position(world, attacker) {
        Some(p) => p,
        None => return ActionResult::Invalid,
    };

    let target_x = attacker_pos.0 + dx;
    let target_y = attacker_pos.1 + dy;

    // Find any Attackable entity at the target position
    if let Some(target) = queries::get_attackable_at(world, target_x, target_y, Some(attacker)) {
        apply_attack(
            &mut EffectCtx { world, grid, spatial: spatial_cache, events, rng },
            attacker,
            target,
        )
    } else {
        // No target - whiff (swing at air), but still add lunge animation
        let _ = world.insert_one(
            attacker,
            LungeAnimation::new(target_x as f32 + 0.5, target_y as f32 + 0.5),
        );
        ActionResult::Completed
    }
}

/// Apply cleave attack - attacks all enemies within radius 2 (24 tiles)
pub fn apply_cleave(ctx: &mut EffectCtx, attacker: Entity) -> ActionResult {
    let EffectCtx { world, grid, spatial: spatial_cache, events, rng } = ctx;
    let (world, grid) = (&mut **world, &mut **grid);
    let (spatial_cache, events, rng) = (&mut **spatial_cache, &mut **events, &mut **rng);

    // Get attacker position
    let attacker_pos = match queries::get_entity_position(world, attacker) {
        Some(p) => p,
        None => return ActionResult::Invalid,
    };

    // Emit cleave event for VFX
    events.push(GameEvent::CleavePerformed {
        center: attacker_pos,
    });

    // Get attacker stats for damage calculation (includes stat affixes)
    let strength = queries::effective_stats(world, attacker).strength;
    let (weapon_damage, affix_damage) = world
        .get::<&Equipment>(attacker)
        .ok()
        .map(|e| {
            (
                e.get_melee().map(|w| w.base_damage + w.damage_bonus).unwrap_or(UNARMED_DAMAGE),
                e.affix_damage_bonus(),
            )
        })
        .unwrap_or((UNARMED_DAMAGE, 0));
    let base_damage = weapon_damage + affix_damage + (strength - 10) / 2;

    // Check for status effects on attacker
    let has_strength_boost = queries::has_status_effect(world, attacker, EffectType::Strengthened);

    // Collect all attackable entities within radius 2 (5x5 area minus center = 24 tiles)
    let mut targets: Vec<(Entity, i32, i32)> = Vec::new();
    for dx in -2..=2 {
        for dy in -2..=2 {
            if dx == 0 && dy == 0 {
                continue; // Skip self
            }
            let tx = attacker_pos.0 + dx;
            let ty = attacker_pos.1 + dy;
            if let Some(target) = queries::get_attackable_at(world, tx, ty, Some(attacker)) {
                targets.push((target, tx, ty));
            }
        }
    }

    // Apply damage to each target
    for (target, tx, ty) in &targets {
        // Apply damage variance and crit (attacker side)
        let damage_mult = rng.gen_range(COMBAT_DAMAGE_MIN_MULT..=COMBAT_DAMAGE_MAX_MULT);
        let is_crit = rng.gen::<f32>() < COMBAT_CRIT_CHANCE;
        let mut raw = (base_damage as f32 * damage_mult) as i32;
        if is_crit {
            raw = (raw as f32 * COMBAT_CRIT_MULTIPLIER) as i32;
        }
        if has_strength_boost {
            raw = (raw as f32 * STRENGTH_DAMAGE_MULTIPLIER) as i32;
        }
        // Conditional weapon affixes (LowHealthDamage)
        raw = (raw as f32
            * crate::systems::combat::attacker_conditional_damage_mult(world, attacker))
            as i32;

        // Apply damage to target (handles invulnerability, armor defense, Protected/Barkskin)
        let damage = crate::systems::combat::apply_damage(world, *target, raw, rng, events);

        // Resolve weapon on-hit affixes through the shared chokepoint
        crate::systems::combat::resolve_weapon_on_hit(
            &mut EffectCtx { world, grid, spatial: spatial_cache, events, rng },
            attacker,
            *target,
            damage,
        );

        // Interrupt life drain if target was channeling
        interrupt_life_drain_on_damage(world, *target, events);

        // Generate threat on cleave targets
        let cleave_threat = damage as f32 * THREAT_PER_DAMAGE;
        crate::systems::ai::generate_threat(world, *target, attacker, cleave_threat);
        crate::systems::ai::generate_companion_threat(world, *target, attacker, cleave_threat);

        // Emit attack event for VFX
        events.push(GameEvent::AttackHit {
            attacker,
            target: *target,
            target_pos: (*tx as f32 + 0.5, *ty as f32 + 0.5),
            damage,
            kind: crate::events::DamageKind::Cleave,
            crit: is_crit,
        });
    }

    // CursedLoud gear rings out: wake enemies in a doubled radius around the
    // cleave (on top of the per-hit melee-noise wake inside apply_damage).
    let noise = crate::systems::combat::attack_noise_radius(world, attacker, MELEE_NOISE_RADIUS);
    if noise > MELEE_NOISE_RADIUS && !targets.is_empty() {
        crate::systems::ai::wake_enemies_in_radius(world, attacker_pos, noise);
    }

    // Add a small lunge animation (to center, since we're hitting all around)
    // Just do a small pulse effect by lunging to self
    let _ = world.insert_one(
        attacker,
        LungeAnimation::new(attacker_pos.0 as f32 + 0.5, attacker_pos.1 as f32 + 0.5),
    );

    ActionResult::Completed
}

/// Activate Stun (Fighter): stun all nearby enemies for a few seconds.
pub fn apply_activate_stun(
    world: &mut World,
    entity: Entity,
    events: &mut EventQueue,
) -> ActionResult {
    use crate::constants::STUN_ABILITY_DURATION;

    // Get entity position
    let entity_pos = crate::queries::get_entity_position(world, entity);
    if entity_pos.is_none() {
        return ActionResult::Invalid;
    }
    let (ex, ey) = entity_pos.unwrap();

    // Find all living enemies in range (exclude the player and tamed companions)
    let targets: Vec<hecs::Entity> = world
        .query::<(&Position, &Health)>()
        .without::<&Player>()
        .without::<&TamedBy>()
        .iter()
        .filter(|(_, (pos, health))| {
            let dx = (pos.x - ex).abs();
            let dy = (pos.y - ey).abs();
            dx <= crate::constants::STUN_ABILITY_RADIUS
                && dy <= crate::constants::STUN_ABILITY_RADIUS
                && health.current > 0
        })
        .map(|(e, _)| e)
        .collect();

    // Apply the stun effect to each target
    for target in targets {
        crate::systems::effects::add_effect_to_entity(
            world,
            target,
            EffectType::Stunned,
            STUN_ABILITY_DURATION,
        );
    }

    // Start the ability cooldown
    if let Ok(mut ability) = world.get::<&mut SecondaryAbility>(entity) {
        ability.start_cooldown();
    }

    // Emit event for sound/VFX
    events.push(GameEvent::StunActivated {
        entity,
        position: (ex, ey),
    });

    ActionResult::Completed
}

/// Land a boss ground slam (the end of `ActionType::BossGroundSlam`).
///
/// Everything player-side — the player and living companions — within
/// `BOSS_SLAM_RADIUS` (Chebyshev) of the boss *at this moment* takes
/// `BOSS_SLAM_DAMAGE` and is Stunned. Fellow enemies are spared; the
/// shockwave is aimed. Whoever got clear during the wind-up is untouched.
pub fn apply_boss_ground_slam(ctx: &mut EffectCtx, boss: Entity) -> ActionResult {
    let EffectCtx { world, events, rng, .. } = ctx;
    let (world, events, rng) = (&mut **world, &mut **events, &mut **rng);

    let Some(center) = queries::get_entity_position(world, boss) else {
        return ActionResult::Invalid;
    };
    let ability = world
        .get::<&crate::components::Boss>(boss)
        .map(|b| b.ability)
        .unwrap_or(crate::components::BossAbility::GroundSlam);
    events.push(GameEvent::BossAbilityUsed { boss, ability, position: center });

    let in_radius = |p: &Position| {
        (p.x - center.0).abs().max((p.y - center.1).abs()) <= BOSS_SLAM_RADIUS
    };
    let mut victims: Vec<(Entity, (i32, i32))> = world
        .query::<(&Position, &Player)>()
        .iter()
        .filter(|(_, (p, _))| in_radius(p))
        .map(|(id, (p, _))| (id, (p.x, p.y)))
        .collect();
    victims.extend(
        world
            .query::<(&Position, &CompanionAI, &Health)>()
            .iter()
            .filter(|(_, (p, _, h))| h.current > 0 && in_radius(p))
            .map(|(id, (p, _, _))| (id, (p.x, p.y))),
    );

    for (victim, vpos) in victims {
        // Guarding through the slam: most of it is blocked, the guard keeps
        // their feet (no stun), and the boss is staggered like any attacker.
        let guarded = super::is_guarding(world, victim);
        let raw = if guarded {
            super::resolve_guard_block(world, events, boss, victim, BOSS_SLAM_DAMAGE)
        } else {
            BOSS_SLAM_DAMAGE
        };
        let damage = crate::systems::combat::apply_damage(world, victim, raw, rng, events);
        if !guarded {
            crate::systems::effects::add_effect_to_entity(
                world, victim, EffectType::Stunned, BOSS_SLAM_STUN_DURATION,
            );
        }
        events.push(GameEvent::AttackHit {
            attacker: boss,
            target: victim,
            target_pos: (vpos.0 as f32 + 0.5, vpos.1 as f32 + 0.5),
            damage,
            kind: crate::events::DamageKind::Slam,
            crit: false,
        });
    }

    ActionResult::Completed
}

#[cfg(test)]
pub(super) mod tests {
    use super::*;
    use crate::components::{
        Actor, Container, ContainerType, Experience, Sprite, Stats, StatusEffects, VisualPosition,
        Weapon,
    };
    use crate::grid::Grid;
    use crate::tile::{tile_ids, Tile, TileType};
    use hecs::World;
    use rand::rngs::StdRng;
    use rand::SeedableRng;

    pub(crate) fn make_grid(width: usize, height: usize) -> Grid {
        Grid {
            width,
            height,
            tiles: vec![Tile::new(TileType::Floor); width * height],
            chest_positions: vec![],
            door_positions: vec![],
            brazier_positions: vec![],
            decals: vec![],
            stairs_up_pos: None,
            stairs_down_pos: None,
            starting_room: None,
            illumination: vec![0.0; width * height],
            themed_rooms: vec![],
            water_positions: vec![],
            coffin_positions: vec![],
            barrel_positions: vec![],
            shop_position: None,
            shop_decor_positions: vec![],
            trap_positions: vec![],
            furniture_positions: vec![],
            secret_room: None,
            secret_door_pos: None,
            stalagmite_positions: Vec::new(),
            mushroom_positions: Vec::new(),
            crystal_positions: Vec::new(),
        }
    }

    /// Full kill chain: melee attack -> apply_damage -> death conversion ->
    /// walkable bones container holding gold, with the kill counted and XP
    /// granted to the player.
    #[test]
    fn test_melee_kill_leaves_lootable_bones() {
        let mut world = World::new();
        let mut grid = make_grid(10, 10);
        let mut events = EventQueue::new();

        let ppos = Position::new(1, 1);
        let player = world.spawn((
            ppos,
            VisualPosition::from_position(&ppos),
            Player,
            Actor::new(1.0),
            Stats::new(14, 10, 10),
            Health::new(30),
            Equipment::with_weapon(Weapon::claws(50)),
            Experience::new(),
            StatusEffects::new(),
        ));
        let mut rng = StdRng::seed_from_u64(7);
        let rat = crate::spawning::enemies::RAT.spawn(&mut world, 1, 2, &mut rng);
        // 1 HP: apply_damage always deals at least 1, so the hit is lethal
        // regardless of the damage variance roll.
        world.get::<&mut Health>(rat).unwrap().current = 1;

        let mut cache = crate::spatial_cache::SpatialCache::rebuild_from_world(&world);
        let result = apply_attack(
            &mut EffectCtx {
                world: &mut world,
                grid: &mut grid,
                spatial: &mut cache,
                events: &mut events,
                rng: &mut rng,
            },
            player,
            rat,
        );
        assert_eq!(result, ActionResult::Completed);
        assert!(world.get::<&Health>(rat).unwrap().current <= 0, "hit is lethal");

        let mut tracker = crate::active_ai_tracker::ActiveAITracker::new();
        tracker.register_entity(rat);
        let mut clock = crate::time_system::GameClock::new();
        let mut scheduler = crate::time_system::ActionScheduler::new();
        let kills = crate::systems::combat::remove_dead_entities(
            &mut crate::engine::ActorCtx {
                world: &mut world,
                grid: &mut grid,
                player,
                clock: &mut clock,
                scheduler: &mut scheduler,
                tracker: &mut tracker,
                spatial: &mut cache,
                events: &mut events,
                rng: &mut rng,
            },
            0,
        );
        assert_eq!(kills, 1, "hostile death increments the kill counter");

        // The rat is now bones: no combat components, walkable, lootable.
        assert!(world.get::<&Health>(rat).is_err());
        assert!(world.get::<&crate::components::ChaseAI>(rat).is_err());
        assert!(world.get::<&crate::components::BlocksMovement>(rat).is_err());
        let sprite = world.get::<&Sprite>(rat).map(|s| (s.sheet, s.tile_id)).unwrap();
        assert_eq!(sprite, tile_ids::BONES_4, "corpse renders as bones");
        let container = world.get::<&Container>(rat).expect("bones are a container");
        assert!(matches!(container.container_type, ContainerType::Corpse));
        assert!(
            (crate::constants::ENEMY_GOLD_DROP_MIN..=crate::constants::ENEMY_GOLD_DROP_MAX)
                .contains(&container.gold),
            "gold drop in range, got {}",
            container.gold
        );
        drop(container);

        // The kill paid XP.
        let xp = world.get::<&Experience>(player).unwrap().current;
        assert!(xp > 0, "player gains XP from the kill");
    }

    // =========================================================================
    // Reach at completion, death mid-swing, and the telegraphed ground slam.
    // These drive the real scheduler (`advance_until_player_ready`), so time
    // only moves while the player has an action in progress — exactly as in
    // play.
    // =========================================================================

    use crate::active_ai_tracker::ActiveAITracker;
    use crate::components::{ActionType, AIState, Boss, ChaseAI};
    use crate::engine::ActorCtx;
    use crate::events::MissReason;
    use crate::spatial_cache::SpatialCache;
    use crate::time_system::{self, ActionScheduler, GameClock};

    /// A floor-only arena with a player and the full simulation context.
    pub(crate) struct Arena {
        pub(crate) world: World,
        pub(crate) grid: Grid,
        pub(crate) clock: GameClock,
        pub(crate) scheduler: ActionScheduler,
        pub(crate) tracker: ActiveAITracker,
        pub(crate) cache: SpatialCache,
        pub(crate) events: EventQueue,
        pub(crate) rng: StdRng,
        pub(crate) player: Entity,
        /// Every event drained so far, in order.
        pub(crate) seen: Vec<GameEvent>,
    }

    impl Arena {
        pub(crate) fn new(player_at: (i32, i32)) -> Self {
            let mut world = World::new();
            let ppos = Position::new(player_at.0, player_at.1);
            let player = world.spawn((
                ppos,
                VisualPosition::from_position(&ppos),
                Player,
                Actor::new(1.0),
                Stats::new(14, 10, 10),
                Health::new(30),
                Equipment::with_weapon(Weapon::claws(3)),
                Experience::new(),
                StatusEffects::new(),
                Attackable,
                crate::components::BlocksMovement,
            ));
            let cache = SpatialCache::rebuild_from_world(&world);
            Self {
                world,
                grid: make_grid(16, 16),
                clock: GameClock::new(),
                scheduler: ActionScheduler::new(),
                tracker: ActiveAITracker::new(),
                cache,
                events: EventQueue::new(),
                rng: StdRng::seed_from_u64(11),
                player,
                seen: Vec::new(),
            }
        }

        /// Spawn a rat at (x, y) with the given speed, already hunting the player.
        pub(crate) fn rat(&mut self, x: i32, y: i32, speed: f32) -> Entity {
            let rat = crate::spawning::enemies::RAT.spawn(&mut self.world, x, y, &mut self.rng);
            self.hunt(rat, speed);
            rat
        }

        /// Wake `e`, point it at the player, fix its speed, and refresh the
        /// caches that hand-placed entities bypass.
        pub(crate) fn hunt(&mut self, e: Entity, speed: f32) {
            let _ = self.world.remove_one::<crate::components::Asleep>(e);
            let ppos = self.pos(self.player);
            if let Ok(mut ai) = self.world.get::<&mut ChaseAI>(e) {
                ai.state = AIState::Chasing;
                ai.add_threat(self.player, WAKE_THREAT);
                ai.update_target_pos(self.player, ppos);
            }
            self.world.get::<&mut Actor>(e).unwrap().speed = speed;
            self.cache.rebuild_in_place(&self.world);
            self.tracker.initialize_from_world(&self.world, ppos);
        }

        pub(crate) fn ctx(&mut self) -> ActorCtx<'_> {
            ActorCtx {
                world: &mut self.world,
                grid: &mut self.grid,
                player: self.player,
                clock: &mut self.clock,
                scheduler: &mut self.scheduler,
                tracker: &mut self.tracker,
                spatial: &mut self.cache,
                events: &mut self.events,
                rng: &mut self.rng,
            }
        }

        /// Start `action` for a non-player entity, as the AI would.
        pub(crate) fn start(&mut self, e: Entity, action: ActionType) {
            time_system::start_action(&mut self.world, e, action, &self.clock, &mut self.scheduler)
                .expect("action starts");
        }

        /// The player takes `action`; the world runs until they can act again.
        pub(crate) fn player_does(&mut self, action: ActionType) {
            let player = self.player;
            // The engine's player path: start effects (reactive abilities) apply.
            time_system::start_action_with_start_effects(&mut self.ctx(), player, action)
                .expect("player action starts");
            crate::engine::advance_until_player_ready(&mut self.ctx());
            let drained: Vec<GameEvent> = self.events.drain().collect();
            self.seen.extend(drained);
        }

        /// Wait in place until game time passes `t`.
        pub(crate) fn wait_until(&mut self, t: f32) {
            while self.clock.time <= t {
                self.player_does(ActionType::Wait);
            }
        }

        pub(crate) fn hp(&self, e: Entity) -> i32 {
            self.world.get::<&Health>(e).map(|h| h.current).unwrap_or(i32::MIN)
        }

        pub(crate) fn pos(&self, e: Entity) -> (i32, i32) {
            queries::get_entity_position(&self.world, e).expect("has a position")
        }

        pub(crate) fn action(&self, e: Entity) -> Option<ActionType> {
            self.world.get::<&Actor>(e).ok()?.current_action.map(|a| a.action_type)
        }

        pub(crate) fn stunned(&self, e: Entity) -> bool {
            queries::has_status_effect(&self.world, e, EffectType::Stunned)
        }

        pub(crate) fn missed(&self, attacker: Entity, target: Entity) -> bool {
            self.seen.iter().any(|ev| {
                matches!(ev, GameEvent::AttackMissed { attacker: a, target: t, reason: MissReason::OutOfReach, .. }
                    if *a == attacker && *t == target)
            })
        }

        pub(crate) fn hit(&self, attacker: Entity, target: Entity) -> bool {
            self.seen.iter().any(|ev| {
                matches!(ev, GameEvent::AttackHit { attacker: a, target: t, .. }
                    if *a == attacker && *t == target)
            })
        }
    }

    /// (a) An enemy's swing is aimed when it starts but lands when it
    /// completes. A player who steps out of reach in between is not hit.
    #[test]
    fn enemy_attack_whiffs_if_the_player_stepped_out_of_reach() {
        let mut arena = Arena::new((5, 5));
        // Slow rat: its 0.8s swing takes 1.6s, longer than the player's step.
        let rat = arena.rat(6, 5, 0.5);
        let player = arena.player;
        arena.start(rat, ActionType::Attack { target: player });

        arena.player_does(ActionType::Move { dx: -1, dy: 0, is_diagonal: false });
        assert_eq!(arena.pos(player), (4, 5), "the step resolved first");
        assert!(
            matches!(arena.action(rat), Some(ActionType::Attack { .. })),
            "the rat is still mid-swing"
        );

        arena.wait_until(1.7);
        assert_eq!(arena.hp(player), 30, "a swing at an empty tile deals nothing");
        assert!(arena.missed(rat, player), "the swing reports a miss");
        assert!(!arena.hit(rat, player));
        assert!(
            arena.world.get::<&LungeAnimation>(rat).is_ok(),
            "the rat still lunges at the air"
        );

        let mut log = crate::ui::MessageLog::new(player);
        for ev in &arena.seen {
            log.record_event(ev, &arena.world);
        }
        assert!(
            log.lines().iter().any(|l| l == "You dodge the Rat's attack."),
            "got {:?}",
            log.lines()
        );
    }

    /// (b) The same rule for the player: an enemy that got away before the
    /// player's swing landed is not hit.
    #[test]
    fn player_attack_whiffs_if_the_enemy_moved_away() {
        let mut arena = Arena::new((5, 5));
        // Fast rat: steps away in 0.5s, inside the player's 0.8s swing, and
        // cannot step back before the swing lands.
        let rat = arena.rat(6, 5, 2.0);
        let player = arena.player;
        let rat_hp = arena.hp(rat);

        arena.start(rat, ActionType::Move { dx: 1, dy: 0, is_diagonal: false });
        arena.player_does(ActionType::Attack { target: rat });

        assert_eq!(arena.pos(rat), (7, 5), "the rat stepped away");
        assert_eq!(arena.hp(rat), rat_hp, "and took no damage");
        assert!(arena.missed(player, rat));
        assert!(!arena.hit(player, rat));

        let mut log = crate::ui::MessageLog::new(player);
        for ev in &arena.seen {
            log.record_event(ev, &arena.world);
        }
        assert!(
            log.lines().iter().any(|l| l == "The Rat evades your attack."),
            "got {:?}",
            log.lines()
        );
    }

    /// (c) Reach is Chebyshev: a target that moved but is still diagonally
    /// adjacent is still hit.
    #[test]
    fn diagonal_adjacency_is_still_in_reach() {
        let mut arena = Arena::new((5, 5));
        let rat = arena.rat(6, 5, 0.5);
        let player = arena.player;
        arena.start(rat, ActionType::Attack { target: player });

        // Step from beside the rat to diagonal from it.
        arena.player_does(ActionType::Move { dx: 0, dy: 1, is_diagonal: false });
        assert_eq!(arena.pos(player), (5, 6));
        arena.wait_until(1.7);

        assert!(arena.hp(player) < 30, "the swing landed");
        assert!(arena.hit(rat, player));
        assert!(!arena.missed(rat, player));
    }

    /// (d) A rat killed mid-swing does not finish the swing, through the
    /// normal per-frame death processing.
    #[test]
    fn an_enemy_killed_mid_swing_does_not_land_its_attack() {
        let mut arena = Arena::new((5, 5));
        let rat = arena.rat(6, 5, 0.5);
        let player = arena.player;
        arena.world.get::<&mut Health>(rat).unwrap().current = 1;
        arena.start(rat, ActionType::Attack { target: player });

        // The player's 0.8s swing lands well before the rat's 1.6s one.
        arena.player_does(ActionType::Attack { target: rat });
        assert!(arena.hp(rat) <= 0, "the player's hit was lethal");

        // What the engine does once per frame after the simulation step.
        let kills = crate::systems::combat::remove_dead_entities(&mut arena.ctx(), 0);
        assert_eq!(kills, 1);

        arena.wait_until(2.5);
        assert_eq!(arena.hp(player), 30, "the dead rat's swing never landed");
        assert!(!arena.hit(rat, player));
    }

    /// The same, for a death inside a single advance (a companion kill or a
    /// burn tick), before the per-frame death processing has run: the
    /// completion is still queued, and must do nothing.
    #[test]
    fn a_dead_attacker_still_queued_does_not_land_its_attack() {
        let mut arena = Arena::new((5, 5));
        let rat = arena.rat(6, 5, 0.5);
        let player = arena.player;
        arena.start(rat, ActionType::Attack { target: player });
        arena.world.get::<&mut Health>(rat).unwrap().current = 0;

        arena.wait_until(2.5);
        assert_eq!(arena.hp(player), 30);
        assert!(!arena.hit(rat, player));
        assert!(arena.action(rat).is_none(), "the dead rat picked no new action");
    }

    /// Spawn Gnash at (x, y), slam off cooldown, hunting the player.
    fn gnash(arena: &mut Arena, x: i32, y: i32, speed: f32) -> Entity {
        let boss = crate::spawning::spawn_boss(&mut arena.world, 3, x, y, &mut arena.rng)
            .expect("floor 3 has a boss");
        arena.world.get::<&mut Boss>(boss).unwrap().cooldown = 0.0;
        arena.hunt(boss, speed);
        boss
    }

    /// The slam winds up first and lands at completion on whoever is still
    /// inside the radius.
    #[test]
    fn ground_slam_stuns_only_when_the_wind_up_completes() {
        let mut arena = Arena::new((5, 5));
        let boss = gnash(&mut arena, 5 + BOSS_SLAM_RADIUS, 5, 1.0);
        let player = arena.player;

        crate::systems::ai::decide_action(&mut arena.ctx(), boss);
        assert!(matches!(arena.action(boss), Some(ActionType::BossGroundSlam)), "winding up");
        assert!(!arena.stunned(player), "nothing lands at the start of the wind-up");
        assert_eq!(arena.hp(player), 30);

        // Halfway through the wind-up: still nothing.
        arena.player_does(ActionType::Wait);
        assert!(arena.clock.time < BOSS_SLAM_WINDUP);
        assert!(!arena.stunned(player));
        assert_eq!(arena.hp(player), 30);

        arena.wait_until(BOSS_SLAM_WINDUP + 0.1);
        assert!(arena.stunned(player), "inside the radius at completion: stunned");
        assert!(arena.hp(player) < 30, "and hurt");
        assert!(arena.seen.iter().any(|e| matches!(e, GameEvent::BossAbilityUsed { .. })));
    }

    /// Getting clear during the wind-up avoids the slam entirely.
    #[test]
    fn leaving_the_radius_during_the_wind_up_avoids_the_slam() {
        let mut arena = Arena::new((5, 5));
        // Slow boss: a 2s wind-up, time for a 1s step out of range.
        let boss = gnash(&mut arena, 5 + BOSS_SLAM_RADIUS, 5, 0.5);
        let player = arena.player;

        crate::systems::ai::decide_action(&mut arena.ctx(), boss);
        assert!(matches!(arena.action(boss), Some(ActionType::BossGroundSlam)));

        arena.player_does(ActionType::Move { dx: -1, dy: 0, is_diagonal: false });
        assert_eq!(arena.pos(player), (4, 5), "now outside the radius");

        arena.wait_until(2.0 * BOSS_SLAM_WINDUP + 0.1);
        assert!(
            arena.seen.iter().any(|e| matches!(e, GameEvent::BossAbilityUsed { .. })),
            "the slam went off"
        );
        assert!(!arena.stunned(player), "but the player was out of range");
        assert_eq!(arena.hp(player), 30);
    }

    /// A giant spider's bite slows and poisons; the poison is announced after
    /// the hit line, and then ticks on game time, ignoring armor.
    #[test]
    fn a_spider_bite_poisons_and_the_poison_ticks() {
        let mut arena = Arena::new((5, 5));
        let player = arena.player;
        arena.world.get::<&mut Health>(player).unwrap().current = 30;
        let spider =
            crate::spawning::enemies::GIANT_SPIDER.spawn(&mut arena.world, 6, 5, &mut arena.rng);
        arena.hunt(spider, 1.0);
        let _ = arena.world.insert_one(spider, Equipment::with_weapon(Weapon::claws(2)));

        let mut ctx = arena.ctx();
        let result = apply_attack(&mut ctx.effects(), spider, player);
        assert_eq!(result, ActionResult::Completed);
        let events: Vec<GameEvent> = arena.events.drain().collect();
        assert!(queries::has_status_effect(&arena.world, player, EffectType::Poisoned));
        assert!(queries::has_status_effect(&arena.world, player, EffectType::Slowed));
        let hit = events.iter().position(|e| matches!(e, GameEvent::AttackHit { .. }));
        let gained = events.iter().position(|e| matches!(e,
            GameEvent::StatusEffectGained { effect: EffectType::Poisoned, .. }));
        assert!(hit.is_some() && gained > hit, "poison announced after the hit: {events:?}");

        let mut log = crate::ui::MessageLog::new(player);
        for ev in &events {
            log.record_event(ev, &arena.world);
        }
        assert!(log.lines().iter().any(|l| l == "You are poisoned."), "{:?}", log.lines());

        // The poison bites over the next few seconds of game time.
        let before = arena.hp(player);
        let mut rng = StdRng::seed_from_u64(2);
        for i in 1..=4 {
            crate::time_system::tick_dot_damage(&mut arena.world, 100.0 + i as f32, &mut rng, &mut arena.events);
        }
        assert_eq!(before - arena.hp(player), 4 * POISON_DAMAGE);
    }

    /// Rats carry the wound-opening trait (Bleeding on a chance roll).
    #[test]
    fn rats_can_open_wounds() {
        let mut arena = Arena::new((5, 5));
        let rat = arena.rat(6, 5, 1.0);
        let chance = arena
            .world
            .get::<&crate::components::Lacerating>(rat)
            .map(|l| l.bleed_chance)
            .expect("rats lacerate");
        assert_eq!(chance, RAT_BLEED_CHANCE);
    }
}
