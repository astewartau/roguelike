//! Melee attacks, cleave, and stun.

use hecs::{Entity, World};
use rand::Rng;

use crate::components::{
    CompanionAI, EffectType, Equipment, Health, LungeAnimation, Player, Position, SecondaryAbility,
    TamedBy,
};
use crate::constants::*;
use crate::events::{EventQueue, GameEvent};
use crate::grid::Grid;
use crate::queries;
use crate::spatial_cache::SpatialCache;

use super::{interrupt_life_drain_on_damage, ActionResult};

/// Apply attack effect
pub fn apply_attack(
    world: &mut World,
    grid: &Grid,
    spatial_cache: &mut SpatialCache,
    attacker: Entity,
    target: Entity,
    events: &mut EventQueue,
    rng: &mut impl Rng,
) -> ActionResult {
    // Get target position for VFX
    let target_pos = match queries::get_entity_position(world, target) {
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

    // Apply damage to target (handles invulnerability, armor defense, Protected/Barkskin)
    let damage = crate::systems::combat::apply_damage(world, target, raw);

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
        world, grid, spatial_cache, attacker, target, damage, events, rng,
    );

    // Venomous natural attacks (Giant Spider): a connecting bite Slows the
    // target. Applied directly here since enemy claws/fangs are not item
    // instances with on-hit affixes.
    if damage > 0 {
        let venom = world
            .get::<&crate::components::Venomous>(attacker)
            .ok()
            .map(|v| v.slow_duration);
        if let Some(duration) = venom {
            let target_alive = world
                .get::<&Health>(target)
                .map(|h| h.current > 0)
                .unwrap_or(false);
            if target_alive {
                crate::systems::effects::add_effect_to_entity(
                    world, target, EffectType::Slowed, duration,
                );
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

    ActionResult::Completed
}

/// Apply attack direction effect - attacks whatever is at the target tile, or whiffs
pub fn apply_attack_direction(
    world: &mut World,
    grid: &Grid,
    spatial_cache: &mut SpatialCache,
    attacker: Entity,
    dx: i32,
    dy: i32,
    events: &mut EventQueue,
    rng: &mut impl Rng,
) -> ActionResult {
    // Get attacker position
    let attacker_pos = match queries::get_entity_position(world, attacker) {
        Some(p) => p,
        None => return ActionResult::Invalid,
    };

    let target_x = attacker_pos.0 + dx;
    let target_y = attacker_pos.1 + dy;

    // Find any Attackable entity at the target position
    if let Some(target) = queries::get_attackable_at(world, target_x, target_y, Some(attacker)) {
        apply_attack(world, grid, spatial_cache, attacker, target, events, rng)
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
pub fn apply_cleave(
    world: &mut World,
    grid: &Grid,
    spatial_cache: &mut SpatialCache,
    attacker: Entity,
    events: &mut EventQueue,
    rng: &mut impl Rng,
) -> ActionResult {
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
        let damage = crate::systems::combat::apply_damage(world, *target, raw);

        // Resolve weapon on-hit affixes through the shared chokepoint
        crate::systems::combat::resolve_weapon_on_hit(
            world, grid, spatial_cache, attacker, *target, damage, events, rng,
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

#[cfg(test)]
mod tests {
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

    fn make_grid(width: usize, height: usize) -> Grid {
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
        let grid = make_grid(10, 10);
        let mut events = EventQueue::new();

        let ppos = Position::new(1, 1);
        let player = world.spawn((
            ppos,
            VisualPosition::from_position(&ppos),
            Player,
            Actor::new(3, 1.0),
            Stats::new(14, 10, 10),
            Health::new(30),
            Equipment::with_weapon(Weapon::claws(50)),
            Experience::new(),
            StatusEffects::new(),
        ));
        let rat = crate::spawning::enemies::RAT.spawn(&mut world, 1, 2);
        // 1 HP: apply_damage always deals at least 1, so the hit is lethal
        // regardless of the damage variance roll.
        world.get::<&mut Health>(rat).unwrap().current = 1;

        let mut cache = crate::spatial_cache::SpatialCache::rebuild_from_world(&world);
        let mut rng = StdRng::seed_from_u64(7);
        let result = apply_attack(&mut world, &grid, &mut cache, player, rat, &mut events, &mut rng);
        assert_eq!(result, ActionResult::Completed);
        assert!(world.get::<&Health>(rat).unwrap().current <= 0, "hit is lethal");

        let mut tracker = crate::active_ai_tracker::ActiveAITracker::new();
        tracker.register_entity(rat);
        let kills = crate::systems::combat::remove_dead_entities(
            &mut world,
            player,
            0,
            &mut rng,
            &mut events,
            None,
            &mut cache,
            &mut tracker,
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
}
