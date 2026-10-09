//! Class-kit abilities: Guard (Fighter), Bone Ward / Sacrifice / Corpse
//! Explosion (Necromancer), Thorns / Entangle / Call Rain (Druid).
//!
//! # Reactive abilities act at action START
//!
//! Everything else in `systems::actions` applies its effect when the action
//! *completes*. That is wrong for a reactive tool: an enemy swing takes 0.36s
//! (bat) to ~1.45s (zombie) and lands at its own completion, so a guard that
//! only went up when the guard action completed would usually arrive after the
//! blow it was meant to answer. Guard, Bone Ward and Sacrifice therefore do
//! their work in [`apply_action_start_effects`], which
//! `time_system::start_action_with_start_effects` calls the moment the action
//! is scheduled. Their completion handlers only tidy up.

use hecs::Entity;

use crate::components::{
    AbilityType, ActionType, Actor, BoneWard, ChaseAI, Container, ContainerType, EffectType,
    Health, Player, Position, RaisedUndead, TamedBy, VisualPosition,
};
use crate::constants::*;
use crate::engine::EffectCtx;
use crate::events::GameEvent;
use crate::queries;
use crate::systems::effects;
use crate::tile::TileType;

use super::{consume_corpse, interrupt_life_drain_on_damage, ActionResult};

/// The kit ability a *targeted* action belongs to, so the cooldown can be
/// started when the action starts (targeted abilities are activated by a click,
/// not through the hotbar slot, so the slot can't start it). `None` for
/// actions that are not kit abilities or that start their cooldown elsewhere.
pub fn kit_ability_for_action(action: &ActionType) -> Option<AbilityType> {
    match action {
        ActionType::Tumble { .. } => Some(AbilityType::Tumble),
        ActionType::PlaceSnareTrap { .. } => Some(AbilityType::SnareTrap),
        ActionType::ShootCripplingShot { .. } => Some(AbilityType::CripplingShot),
        ActionType::Sacrifice { .. } => Some(AbilityType::Sacrifice),
        ActionType::CorpseExplosion { .. } => Some(AbilityType::CorpseExplosion),
        ActionType::Entangle { .. } => Some(AbilityType::Entangle),
        ActionType::CallRain { .. } => Some(AbilityType::CallRain),
        _ => None,
    }
}

/// Apply the part of an action that has to happen the instant it starts.
///
/// Called by `time_system::start_action_with_start_effects` right after the
/// action is recorded and scheduled. A no-op for every action that is not a
/// reactive ability.
pub fn apply_action_start_effects(ctx: &mut EffectCtx, entity: Entity, action: &ActionType) {
    match action {
        ActionType::Guard => start_guard(ctx, entity),
        ActionType::BoneWard => start_bone_ward(ctx, entity),
        ActionType::Sacrifice { skeleton } => start_sacrifice(ctx, entity, *skeleton),
        _ => {}
    }
}

// =============================================================================
// GUARD (Fighter)
// =============================================================================

/// Raise the guard: `Guarding` for exactly as long as the Guard action runs
/// (normally `GUARD_DURATION`; longer if the fighter is slowed), so the window
/// and the commitment always match. The block itself is resolved in
/// `apply_attack` / `apply_boss_ground_slam`.
fn start_guard(ctx: &mut EffectCtx, entity: Entity) {
    let duration = ctx
        .world
        .get::<&Actor>(entity)
        .ok()
        .and_then(|a| a.current_action)
        .map(|a| (a.completion_time - a.start_time).max(0.0))
        .unwrap_or(GUARD_DURATION);
    effects::add_effect_to_entity(ctx.world, entity, EffectType::Guarding, duration);
    ctx.events.push(GameEvent::AbilityActivated { entity, ability: AbilityType::Guard });
}

/// The Guard action ends: drop the guard (the effect would also time out on
/// its own; removing it here keeps the window exactly the action's span).
pub fn apply_guard_complete(ctx: &mut EffectCtx, entity: Entity) -> ActionResult {
    effects::remove_effect_from_entity(ctx.world, entity, EffectType::Guarding);
    ActionResult::Completed
}

/// Whether `target` is guarding right now.
pub fn is_guarding(world: &hecs::World, target: Entity) -> bool {
    queries::has_status_effect(world, target, EffectType::Guarding)
}

/// A blow landed on a guarding `defender`: stagger the attacker and announce
/// the block. Returns the raw damage that gets through.
pub fn resolve_guard_block(
    world: &mut hecs::World,
    events: &mut crate::events::EventQueue,
    attacker: Entity,
    defender: Entity,
    raw: i32,
) -> i32 {
    // Staggered: a brief Stun, which makes the AI skip its next decisions.
    effects::add_effect_to_entity(world, attacker, EffectType::Stunned, GUARD_STAGGER_DURATION);
    let defender_pos = queries::get_entity_position(world, defender)
        .map(|(x, y)| (x as f32 + 0.5, y as f32 + 0.5))
        .unwrap_or((0.0, 0.0));
    events.push(GameEvent::AttackBlocked { attacker, defender, defender_pos });
    (raw as f32 * (1.0 - GUARD_DAMAGE_REDUCTION)) as i32
}

// =============================================================================
// BONE WARD (Necromancer)
// =============================================================================

/// Charges a Bone Ward cast at `center` would get: one, plus one per corpse
/// within `BONE_WARD_CORPSE_RADIUS`, capped at `BONE_WARD_MAX_CHARGES`.
pub fn bone_ward_charges(world: &hecs::World, center: (i32, i32)) -> u32 {
    let corpses = world
        .query::<(&Position, &Container)>()
        .iter()
        .filter(|(_, (p, c))| {
            matches!(c.container_type, ContainerType::Corpse)
                && (p.x - center.0).abs().max((p.y - center.1).abs()) <= BONE_WARD_CORPSE_RADIUS
        })
        .count() as u32;
    (1 + corpses).min(BONE_WARD_MAX_CHARGES)
}

/// Raise the ward (replacing any ward still up).
fn start_bone_ward(ctx: &mut EffectCtx, entity: Entity) {
    let Some(center) = queries::get_entity_position(ctx.world, entity) else {
        return;
    };
    let charges = bone_ward_charges(ctx.world, center);
    let _ = ctx.world.insert_one(entity, BoneWard { charges });
    effects::add_effect_to_entity(ctx.world, entity, EffectType::BoneWard, BONE_WARD_DURATION);
    ctx.events.push(GameEvent::BoneWardRaised { entity, charges });
}

// =============================================================================
// SACRIFICE (Necromancer)
// =============================================================================

/// Whether `skeleton` is a living raised skeleton owned by `owner` within
/// `SACRIFICE_RANGE`. Shared by click validation and the action itself.
pub fn is_valid_sacrifice_target(world: &hecs::World, owner: Entity, skeleton: Entity) -> bool {
    let owned = world.get::<&RaisedUndead>(skeleton).is_ok()
        && world.get::<&TamedBy>(skeleton).map(|t| t.owner == owner).unwrap_or(false)
        && world.get::<&Health>(skeleton).map(|h| h.current > 0).unwrap_or(false);
    let (Some(a), Some(b)) = (
        queries::get_entity_position(world, owner),
        queries::get_entity_position(world, skeleton),
    ) else {
        return false;
    };
    owned && (a.0 - b.0).abs().max((a.1 - b.1).abs()) <= SACRIFICE_RANGE
}

/// Swap the caster and the skeleton. Swings already locked onto the caster
/// now find an empty tile (phase-1 reach check) and whiff; those attackers
/// also take a strong dislike to the skeleton that stepped in.
fn start_sacrifice(ctx: &mut EffectCtx, caster: Entity, skeleton: Entity) {
    if !is_valid_sacrifice_target(ctx.world, caster, skeleton) {
        return;
    }
    let (Some(from), Some(to)) = (
        queries::get_entity_position(ctx.world, caster),
        queries::get_entity_position(ctx.world, skeleton),
    ) else {
        return;
    };

    for (entity, old, new) in [(caster, from, to), (skeleton, to, from)] {
        if let Ok(mut p) = ctx.world.get::<&mut Position>(entity) {
            p.x = new.0;
            p.y = new.1;
        }
        // Snap, like Blink: this is a swap, not a walk.
        if let Ok(mut v) = ctx.world.get::<&mut VisualPosition>(entity) {
            v.x = new.0 as f32;
            v.y = new.1 as f32;
        }
        ctx.spatial.update_position(entity, old, new);
        ctx.events.push(GameEvent::EntityMoved { entity, from: old, to: new });
    }
    // Both arrive on the other's tile; each tile acts on its new occupant.
    for (entity, tile) in [(caster, to), (skeleton, from)] {
        crate::systems::tile_effects::on_enter_tile(
            ctx.world, ctx.grid, entity, tile, ctx.events, ctx.rng,
        );
    }

    // Enemies mid-swing at the caster turn on the skeleton.
    let swinging: Vec<Entity> = ctx
        .world
        .query::<(&Actor, &ChaseAI)>()
        .iter()
        .filter(|(_, (a, _))| {
            matches!(
                a.current_action.map(|c| c.action_type),
                Some(ActionType::Attack { target }) if target == caster
            )
        })
        .map(|(e, _)| e)
        .collect();
    for enemy in swinging {
        if let Ok(mut ai) = ctx.world.get::<&mut ChaseAI>(enemy) {
            let on_caster = ai
                .threat_table
                .iter()
                .find(|t| t.entity == caster)
                .map(|t| t.threat)
                .unwrap_or(0.0);
            ai.add_threat(skeleton, on_caster + SACRIFICE_TAUNT_THREAT);
        }
    }

    ctx.events.push(GameEvent::SacrificeSwapped { caster, skeleton });
}

// =============================================================================
// CORPSE EXPLOSION (Necromancer)
// =============================================================================

/// Whether `corpse` is a corpse within `CORPSE_EXPLOSION_RANGE` of `caster`.
pub fn is_valid_corpse_target(world: &hecs::World, caster: Entity, corpse: Entity) -> bool {
    let is_corpse = world
        .get::<&Container>(corpse)
        .map(|c| matches!(c.container_type, ContainerType::Corpse))
        .unwrap_or(false);
    let (Some(a), Some(b)) = (
        queries::get_entity_position(world, caster),
        queries::get_entity_position(world, corpse),
    ) else {
        return false;
    };
    is_corpse && (a.0 - b.0).abs().max((a.1 - b.1).abs()) <= CORPSE_EXPLOSION_RANGE
}

/// Living hostiles (enemy AI, not anyone's companion) within `radius` of
/// `center`. The player, companions and NPCs are never included.
fn hostiles_near(world: &hecs::World, center: (i32, i32), radius: i32) -> Vec<(Entity, (i32, i32))> {
    world
        .query::<(&Position, &Health, &ChaseAI)>()
        .without::<&TamedBy>()
        .without::<&Player>()
        .iter()
        .filter(|(_, (p, h, _))| {
            h.current > 0 && (p.x - center.0).abs().max((p.y - center.1).abs()) <= radius
        })
        .map(|(e, (p, _, _))| (e, (p.x, p.y)))
        .collect()
}

/// Detonate a corpse: its loot drops to the floor, and every hostile within
/// `CORPSE_EXPLOSION_RADIUS` takes INT-scaled damage.
pub fn apply_corpse_explosion(ctx: &mut EffectCtx, caster: Entity, corpse: Entity) -> ActionResult {
    if !is_valid_corpse_target(ctx.world, caster, corpse) {
        return ActionResult::Invalid;
    }
    let Some(center) = queries::get_entity_position(ctx.world, corpse) else {
        return ActionResult::Invalid;
    };

    ctx.spatial.remove_entity(corpse);
    consume_corpse(ctx.world, corpse, center);

    let damage = ((CORPSE_EXPLOSION_DAMAGE as f32 * queries::int_power(ctx.world, caster)).round()
        as i32)
        .max(1);

    // Explosion VFX, sound and camera shake all key off this event.
    ctx.events.push(GameEvent::FireballExplosion {
        x: center.0,
        y: center.1,
        radius: CORPSE_EXPLOSION_RADIUS,
    });
    crate::systems::ai::wake_enemies_in_radius(ctx.world, center, EXPLOSION_NOISE_RADIUS);

    let victims = hostiles_near(ctx.world, center, CORPSE_EXPLOSION_RADIUS);
    for &(victim, (x, y)) in &victims {
        let dealt =
            crate::systems::combat::apply_damage(ctx.world, victim, damage, ctx.rng, ctx.events);
        interrupt_life_drain_on_damage(ctx.world, victim, ctx.events);
        let threat = dealt as f32 * THREAT_PER_DAMAGE;
        crate::systems::ai::generate_threat(ctx.world, victim, caster, threat);
        ctx.events.push(GameEvent::AttackHit {
            attacker: caster,
            target: victim,
            target_pos: (x as f32 + 0.5, y as f32 + 0.5),
            damage: dealt,
            kind: crate::events::DamageKind::CorpseExplosion,
            crit: false,
        });
    }

    ctx.events.push(GameEvent::CorpseExploded {
        caster,
        position: center,
        hits: victims.len() as u32,
    });
    ActionResult::Completed
}

// =============================================================================
// THORNS / ENTANGLE (Druid)
// =============================================================================

/// Wreathe the caster in thorns for `THORNS_DURATION` x INT power. The
/// reflection is resolved in `apply_attack` via [`reflect_thorns`].
pub fn apply_activate_thorns(ctx: &mut EffectCtx, entity: Entity) -> ActionResult {
    let duration = THORNS_DURATION * queries::int_power(ctx.world, entity);
    effects::add_effect_to_entity(ctx.world, entity, EffectType::Thorns, duration);
    ctx.events.push(GameEvent::AbilityActivated { entity, ability: AbilityType::Thorns });
    ActionResult::Completed
}

/// A melee hit by `attacker` connected with `defender`: if the defender is
/// thorny, `THORNS_DAMAGE` goes back at the attacker through `apply_damage`.
///
/// This deals damage directly rather than through `apply_attack`, so two
/// thorny combatants cannot ping-pong: only the original swing reflects.
pub fn reflect_thorns(ctx: &mut EffectCtx, attacker: Entity, defender: Entity) {
    if attacker == defender || !queries::has_status_effect(ctx.world, defender, EffectType::Thorns)
    {
        return;
    }
    let alive = ctx.world.get::<&Health>(attacker).map(|h| h.current > 0).unwrap_or(false);
    let Some((x, y)) = queries::get_entity_position(ctx.world, attacker) else {
        return;
    };
    if !alive {
        return;
    }
    let dealt =
        crate::systems::combat::apply_damage(ctx.world, attacker, THORNS_DAMAGE, ctx.rng, ctx.events);
    interrupt_life_drain_on_damage(ctx.world, attacker, ctx.events);
    crate::systems::ai::generate_threat(ctx.world, attacker, defender, dealt as f32 * THREAT_PER_DAMAGE);
    ctx.events.push(GameEvent::AttackHit {
        attacker: defender,
        target: attacker,
        target_pos: (x as f32 + 0.5, y as f32 + 0.5),
        damage: dealt,
        kind: crate::events::DamageKind::Thorns,
        crit: false,
    });
}

/// Vines burst from the ground around `(tx, ty)`: every hostile within
/// `ENTANGLE_RADIUS` is Rooted, for longer if it stands in grass.
pub fn apply_entangle(ctx: &mut EffectCtx, caster: Entity, tx: i32, ty: i32) -> ActionResult {
    let Some(from) = queries::get_entity_position(ctx.world, caster) else {
        return ActionResult::Invalid;
    };
    if (tx - from.0).abs().max((ty - from.1).abs()) > ENTANGLE_RANGE {
        return ActionResult::Blocked;
    }

    let victims = hostiles_near(ctx.world, (tx, ty), ENTANGLE_RADIUS);
    for &(victim, (x, y)) in &victims {
        let grassy = ctx
            .grid
            .get(x, y)
            .map(|t| matches!(t.tile_type, TileType::Grass | TileType::TallGrass))
            .unwrap_or(false);
        let duration = if grassy { ENTANGLE_ROOT_DURATION_GRASS } else { ENTANGLE_ROOT_DURATION };
        effects::add_effect_to_entity(ctx.world, victim, EffectType::Rooted, duration);
    }

    let tiles: Vec<(i32, i32)> = (-ENTANGLE_RADIUS..=ENTANGLE_RADIUS)
        .flat_map(|dy| (-ENTANGLE_RADIUS..=ENTANGLE_RADIUS).map(move |dx| (tx + dx, ty + dy)))
        .filter(|&(x, y)| ctx.grid.is_walkable(x, y))
        .collect();
    ctx.events.push(GameEvent::EntangleCast {
        caster,
        position: (tx, ty),
        rooted: victims.len() as u32,
        tiles,
    });
    ActionResult::Completed
}

/// A downpour over `(tx, ty)`: everything within `CALL_RAIN_RADIUS` is
/// doused and soaked through the same path as a thrown water flask
/// (`fire::splash_water`) — creature fires, grass fires, burning oil and webs
/// go out, grass is soaked (`WetGrass`), and every creature in the area,
/// friend or foe (the druid too, if inside), becomes Wet.
pub fn apply_call_rain(ctx: &mut EffectCtx, caster: Entity, tx: i32, ty: i32) -> ActionResult {
    let Some(from) = queries::get_entity_position(ctx.world, caster) else {
        return ActionResult::Invalid;
    };
    if (tx - from.0).abs().max((ty - from.1).abs()) > CALL_RAIN_RANGE {
        return ActionResult::Blocked;
    }

    let doused =
        crate::systems::fire::splash_water(ctx.world, ctx.grid, tx, ty, CALL_RAIN_RADIUS, ctx.events);

    let tiles: Vec<(i32, i32)> = (-CALL_RAIN_RADIUS..=CALL_RAIN_RADIUS)
        .flat_map(|dy| (-CALL_RAIN_RADIUS..=CALL_RAIN_RADIUS).map(move |dx| (tx + dx, ty + dy)))
        .filter(|&(x, y)| ctx.grid.is_walkable(x, y))
        .collect();
    ctx.events.push(GameEvent::RainCalled { caster, position: (tx, ty), tiles, doused });
    ActionResult::Completed
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::components::{
        CompanionAI, Equipment, ItemInstance, ItemType, Sprite, StatusEffects, Weapon,
    };
    use crate::events::{DamageKind, MissReason};
    use crate::systems::actions::combat::tests::Arena;
    use crate::tile::{tile_ids, Tile};

    fn effect_remaining(world: &hecs::World, e: Entity, effect: EffectType) -> Option<f32> {
        world
            .get::<&StatusEffects>(e)
            .ok()?
            .effects
            .iter()
            .find(|x| x.effect_type == effect)
            .map(|x| x.remaining_duration)
    }

    fn spawn_corpse(arena: &mut Arena, x: i32, y: i32, items: Vec<ItemInstance>, gold: u32) -> Entity {
        let pos = Position::new(x, y);
        let mut container = Container::corpse(items, gold);
        container.is_open = true;
        arena.world.spawn((
            pos,
            VisualPosition::from_position(&pos),
            Sprite::from_ref(tile_ids::BONES_4),
            container,
        ))
    }

    /// A raised skeleton owned by the arena's player (mirrors the engine's
    /// `spawn_raised_skeleton`, minus scheduling: it never acts here).
    fn raised_skeleton(arena: &mut Arena, x: i32, y: i32) -> Entity {
        let owner = arena.player;
        let s = crate::spawning::enemies::SKELETON.spawn(&mut arena.world, x, y, &mut arena.rng);
        let _ = arena.world.remove_one::<ChaseAI>(s);
        let _ = arena.world.remove_one::<crate::components::Asleep>(s);
        let _ = arena.world.insert(
            s,
            (
                TamedBy { owner },
                CompanionAI { owner, follow_distance: 2, threat_table: Vec::new() },
                RaisedUndead,
            ),
        );
        arena.cache.rebuild_in_place(&arena.world);
        s
    }

    /// Build an arena where a hard-hitting rat starts a 0.6s swing at the
    /// player at t=0 and the player Waits until t=0.5. The swing then lands
    /// 0.1s into whatever the player does next.
    fn swing_arena() -> (Arena, Entity) {
        let mut arena = Arena::new((5, 5));
        let player = arena.player;
        arena.world.get::<&mut Health>(player).unwrap().current = 30;
        let rat = arena.rat(6, 5, 0.8 / 0.6);
        let _ = arena.world.insert_one(rat, Equipment::with_weapon(Weapon::claws(20)));
        arena.start(rat, ActionType::Attack { target: player });
        arena.player_does(ActionType::Wait);
        assert!((arena.clock.time - 0.5).abs() < 1e-4);
        assert_eq!(arena.hp(player), 30, "the swing has not landed yet");
        (arena, rat)
    }

    /// Guard is up from the instant the action starts: a blow landing 0.1s
    /// into the guard is cut to 25% and the attacker is staggered.
    #[test]
    fn guard_blocks_a_hit_that_lands_just_after_it_starts() {
        // Control: same seed, same swing, the player just keeps waiting.
        let (mut control, _) = swing_arena();
        control.player_does(ActionType::Wait);
        let unguarded = 30 - control.hp(control.player);
        assert!(unguarded > 4, "the control hit is a real hit ({unguarded})");

        let (mut arena, rat) = swing_arena();
        let player = arena.player;

        // Starting the action is enough to be guarding: nothing has advanced.
        time_system_start(&mut arena, ActionType::Guard);
        assert!(queries::has_status_effect(&arena.world, player, EffectType::Guarding));

        crate::engine::advance_until_player_ready(&mut arena.ctx());
        arena.seen.extend(arena.events.drain().collect::<Vec<_>>());
        assert!(arena.clock.time > 0.6, "the guard outlasted the swing");

        let guarded = 30 - arena.hp(player);
        let expected = ((unguarded as f32 * (1.0 - GUARD_DAMAGE_REDUCTION)) as i32).max(1);
        assert_eq!(guarded, expected, "guarded {guarded} vs unguarded {unguarded}");
        assert!(arena.stunned(rat), "the blocked attacker is staggered");
        assert!(arena.seen.iter().any(|e| matches!(e,
            GameEvent::AttackBlocked { attacker, defender, .. } if *attacker == rat && *defender == player)));
        assert!(
            !queries::has_status_effect(&arena.world, player, EffectType::Guarding),
            "the guard drops when the action ends"
        );

        let mut log = crate::ui::MessageLog::new(player);
        for ev in &arena.seen {
            log.record_event(ev, &arena.world);
        }
        assert!(log.lines().iter().any(|l| l == "You block the Rat's blow!"), "{:?}", log.lines());
    }

    /// Start a player action through the engine's start-effects path without
    /// advancing time.
    fn time_system_start(arena: &mut Arena, action: ActionType) {
        let player = arena.player;
        crate::time_system::start_action_with_start_effects(&mut arena.ctx(), player, action)
            .expect("starts");
    }

    /// Guarding through Gnash's ground slam: most of it is blocked, the guard
    /// is not stunned, and the boss is staggered instead.
    #[test]
    fn guard_blunts_the_boss_slam() {
        let mut arena = Arena::new((5, 5));
        let player = arena.player;
        let boss = crate::spawning::spawn_boss(&mut arena.world, 3, 6, 5, &mut arena.rng)
            .expect("floor 3 has a boss");
        effects::add_effect_to_entity(&mut arena.world, player, EffectType::Guarding, 1.0);

        {
            let mut ctx = arena.ctx();
            crate::systems::actions::apply_boss_ground_slam(&mut ctx.effects(), boss);
        }
        let expected = ((BOSS_SLAM_DAMAGE as f32 * (1.0 - GUARD_DAMAGE_REDUCTION)) as i32).max(1);
        assert_eq!(30 - arena.hp(player), expected);
        assert!(!arena.stunned(player), "a braced guard keeps their feet");
        assert!(arena.stunned(boss), "the boss is staggered");
    }

    /// Bone Ward: one charge, plus one per corpse within the radius, capped.
    #[test]
    fn bone_ward_charges_count_nearby_corpses() {
        let mut arena = Arena::new((5, 5));
        assert_eq!(bone_ward_charges(&arena.world, (5, 5)), 1);
        spawn_corpse(&mut arena, 5 + BONE_WARD_CORPSE_RADIUS, 5, vec![], 0);
        spawn_corpse(&mut arena, 5 + BONE_WARD_CORPSE_RADIUS + 1, 5, vec![], 0); // too far
        assert_eq!(bone_ward_charges(&arena.world, (5, 5)), 2);
        for i in 0..4 {
            spawn_corpse(&mut arena, 4, 4 + i, vec![], 0);
        }
        assert_eq!(bone_ward_charges(&arena.world, (5, 5)), BONE_WARD_MAX_CHARGES);
    }

    /// The ward goes up at action start and swallows exactly N whole hits,
    /// N = 1 + nearby corpses; the next hit lands.
    #[test]
    fn bone_ward_absorbs_one_hit_per_charge() {
        let mut arena = Arena::new((5, 5));
        let player = arena.player;
        spawn_corpse(&mut arena, 6, 6, vec![], 0);

        time_system_start(&mut arena, ActionType::BoneWard);
        assert_eq!(arena.world.get::<&BoneWard>(player).map(|w| w.charges).ok(), Some(2));
        assert!(queries::has_status_effect(&arena.world, player, EffectType::BoneWard));

        for _ in 0..2 {
            let dealt = crate::systems::combat::apply_damage(
                &mut arena.world, player, 10, &mut arena.rng, &mut arena.events,
            );
            assert_eq!(dealt, 0);
            assert_eq!(arena.hp(player), 30);
        }
        assert!(arena.world.get::<&BoneWard>(player).is_err(), "spent wards are removed");
        let dealt = crate::systems::combat::apply_damage(
            &mut arena.world, player, 10, &mut arena.rng, &mut arena.events,
        );
        assert_eq!(dealt, 10);
        assert_eq!(arena.hp(player), 20);

        let absorbed: Vec<u32> = arena
            .events
            .drain()
            .filter_map(|e| match e {
                GameEvent::BoneWardAbsorbed { charges_left, .. } => Some(charges_left),
                _ => None,
            })
            .collect();
        assert_eq!(absorbed, vec![1, 0]);
    }

    /// Sacrifice swaps the necromancer with their skeleton at action start,
    /// so an enemy swing already locked on the necromancer whiffs, and the
    /// enemy turns on the skeleton.
    #[test]
    fn sacrifice_swaps_places_and_the_swing_at_the_player_misses() {
        let mut arena = Arena::new((5, 5));
        let player = arena.player;
        // Slow rat: its swing lands at 1.6s.
        let rat = arena.rat(6, 5, 0.5);
        let skeleton = raised_skeleton(&mut arena, 5, 9);
        arena.start(rat, ActionType::Attack { target: player });

        arena.player_does(ActionType::Sacrifice { skeleton });
        assert!(arena.clock.time < 0.5, "a very short action");
        assert_eq!(arena.pos(player), (5, 9));
        assert_eq!(arena.pos(skeleton), (5, 5));
        arena.cache.assert_coherent_with_world(&arena.world, "after the swap");
        assert!(arena.seen.iter().any(|e| matches!(e, GameEvent::SacrificeSwapped { .. })));

        let top = arena
            .world
            .get::<&ChaseAI>(rat)
            .ok()
            .and_then(|ai| ai.highest_threat().map(|t| t.entity));
        assert_eq!(top, Some(skeleton), "the rat now wants the skeleton");

        arena.wait_until(1.7);
        assert_eq!(arena.hp(player), 30, "the swing found an empty tile");
        assert!(arena.missed(rat, player));
        assert!(arena.seen.iter().any(|e| matches!(e,
            GameEvent::AttackMissed { target, reason: MissReason::OutOfReach, .. } if *target == player)));
    }

    /// Sacrifice refuses skeletons that aren't yours or are out of range.
    #[test]
    fn sacrifice_needs_your_own_skeleton_in_range() {
        let mut arena = Arena::new((5, 5));
        let player = arena.player;
        let mine_far = raised_skeleton(&mut arena, 5 + SACRIFICE_RANGE + 1, 5);
        assert!(!is_valid_sacrifice_target(&arena.world, player, mine_far));
        let hostile = crate::spawning::enemies::SKELETON.spawn(&mut arena.world, 6, 6, &mut arena.rng);
        assert!(!is_valid_sacrifice_target(&arena.world, player, hostile));
        let mine = raised_skeleton(&mut arena, 7, 7);
        assert!(is_valid_sacrifice_target(&arena.world, player, mine));
    }

    /// Corpse Explosion consumes the corpse (its loot survives on the floor)
    /// and damages only hostiles within radius 1 of it.
    #[test]
    fn corpse_explosion_hits_only_nearby_hostiles_and_consumes_the_corpse() {
        let mut arena = Arena::new((5, 5));
        let player = arena.player;
        let loot = vec![ItemInstance::plain(ItemType::HealthPotion)];
        let corpse = spawn_corpse(&mut arena, 6, 5, loot, 9);
        let near = arena.rat(7, 6, 1.0); // diagonal to the corpse: inside
        let far = arena.rat(8, 5, 1.0); // two tiles from the corpse: outside
        let pet = raised_skeleton(&mut arena, 6, 4); // adjacent, but an ally
        let (near_hp, far_hp, pet_hp) = (arena.hp(near), arena.hp(far), arena.hp(pet));

        let result = {
            let mut ctx = arena.ctx();
            apply_corpse_explosion(&mut ctx.effects(), player, corpse)
        };
        assert_eq!(result, ActionResult::Completed);
        let events: Vec<GameEvent> = arena.events.drain().collect();

        assert!(arena.hp(near) < near_hp, "the hostile beside the corpse is hit");
        assert_eq!(arena.hp(far), far_hp, "outside the radius");
        assert_eq!(arena.hp(pet), pet_hp, "companions are spared");
        assert_eq!(arena.hp(player), 30, "the caster (adjacent) is spared");
        assert!(!arena.world.contains(corpse), "the corpse is consumed");

        let pile: Vec<(i32, i32, u32, usize)> = arena
            .world
            .query::<(&Position, &Container, &crate::components::GroundItemPile)>()
            .iter()
            .map(|(_, (p, c, _))| (p.x, p.y, c.gold, c.items.len()))
            .collect();
        assert_eq!(pile, vec![(6, 5, 9, 1)], "the corpse's loot drops where it lay");

        assert!(events.iter().any(|e| matches!(e, GameEvent::FireballExplosion { x: 6, y: 5, .. })));
        assert!(events.iter().any(|e| matches!(e, GameEvent::CorpseExploded { hits: 1, .. })));
        assert!(events.iter().any(|e| matches!(e,
            GameEvent::AttackHit { target, kind: DamageKind::CorpseExplosion, .. } if *target == near)));

        // Out of range or not a corpse: nothing happens.
        let far_corpse = spawn_corpse(&mut arena, 5 + CORPSE_EXPLOSION_RANGE + 1, 5, vec![], 0);
        let mut ctx = arena.ctx();
        assert_eq!(
            apply_corpse_explosion(&mut ctx.effects(), player, far_corpse),
            ActionResult::Invalid
        );
    }

    /// Thorns: a melee hit on the druid pricks the attacker back, once —
    /// even when the attacker is thorny too.
    #[test]
    fn thorns_reflect_melee_without_recursing() {
        let mut arena = Arena::new((5, 5));
        let player = arena.player;
        let rat = arena.rat(6, 5, 1.0);
        let _ = arena.world.insert_one(rat, Equipment::with_weapon(Weapon::claws(4)));
        arena.world.get::<&mut Health>(rat).unwrap().current = 50;
        crate::systems::effects::add_effect_to_entity(&mut arena.world, player, EffectType::Thorns, 10.0);
        crate::systems::effects::add_effect_to_entity(&mut arena.world, rat, EffectType::Thorns, 10.0);

        {
            let mut ctx = arena.ctx();
            assert_eq!(
                crate::systems::actions::apply_attack(&mut ctx.effects(), rat, player),
                ActionResult::Completed
            );
        }
        let events: Vec<GameEvent> = arena.events.drain().collect();
        let thorns: Vec<(Entity, Entity, i32)> = events
            .iter()
            .filter_map(|e| match e {
                GameEvent::AttackHit { attacker, target, damage, kind: DamageKind::Thorns, .. } => {
                    Some((*attacker, *target, *damage))
                }
                _ => None,
            })
            .collect();
        assert_eq!(thorns.len(), 1, "exactly one reflection: {thorns:?}");
        assert_eq!((thorns[0].0, thorns[0].1), (player, rat));
        assert_eq!(arena.hp(rat), 50 - thorns[0].2);

        let melee: i32 = events
            .iter()
            .filter_map(|e| match e {
                GameEvent::AttackHit { target, damage, kind: DamageKind::Melee, .. } if *target == player => {
                    Some(*damage)
                }
                _ => None,
            })
            .sum();
        assert_eq!(arena.hp(player), 30 - melee, "no thorns bounced back at the druid");
    }

    /// Entangle roots hostiles around the tile; grass roots for longer.
    #[test]
    fn entangle_roots_longer_on_grass() {
        let mut arena = Arena::new((5, 5));
        let player = arena.player;
        if let Some(tile) = arena.grid.get_mut(9, 9) {
            *tile = Tile::new(TileType::Grass);
        }
        let on_grass = arena.rat(9, 9, 1.0);
        let on_floor = arena.rat(8, 8, 1.0);
        let outside = arena.rat(10, 6, 1.0);
        let pet = raised_skeleton(&mut arena, 9, 8);

        let result = {
            let mut ctx = arena.ctx();
            apply_entangle(&mut ctx.effects(), player, 8, 9)
        };
        assert_eq!(result, ActionResult::Completed);

        assert_eq!(
            effect_remaining(&arena.world, on_grass, EffectType::Rooted),
            Some(ENTANGLE_ROOT_DURATION_GRASS)
        );
        assert_eq!(
            effect_remaining(&arena.world, on_floor, EffectType::Rooted),
            Some(ENTANGLE_ROOT_DURATION)
        );
        assert_eq!(effect_remaining(&arena.world, outside, EffectType::Rooted), None);
        assert_eq!(effect_remaining(&arena.world, pet, EffectType::Rooted), None);
        assert!(arena
            .events
            .drain()
            .any(|e| matches!(e, GameEvent::EntangleCast { rooted: 2, .. })));

        // Beyond ENTANGLE_RANGE the cast fizzles.
        let mut ctx = arena.ctx();
        assert_eq!(
            apply_entangle(&mut ctx.effects(), player, 5 + ENTANGLE_RANGE + 1, 5),
            ActionResult::Blocked
        );
    }

    /// Every class starts with the kit the design calls for.
    #[test]
    fn every_class_kit_has_the_expected_abilities() {
        use crate::components::{ClassKit, PlayerClass};
        let kit = |c| -> Vec<AbilityType> {
            ClassKit::for_class(c).abilities.iter().map(|k| k.ability).collect()
        };
        assert_eq!(kit(PlayerClass::Fighter), vec![AbilityType::Guard]);
        assert_eq!(
            kit(PlayerClass::Ranger),
            vec![
                AbilityType::Disengage,
                AbilityType::Tumble,
                AbilityType::SnareTrap,
                AbilityType::CripplingShot
            ]
        );
        assert_eq!(
            kit(PlayerClass::Druid),
            vec![AbilityType::Thorns, AbilityType::Entangle, AbilityType::CallRain]
        );
        assert_eq!(
            kit(PlayerClass::Necromancer),
            vec![AbilityType::BoneWard, AbilityType::Sacrifice, AbilityType::CorpseExplosion]
        );
        // All start ready, each with its own cooldown.
        for class in PlayerClass::ALL {
            for k in ClassKit::for_class(class).abilities {
                assert_eq!(k.cooldown_remaining, 0.0);
                assert!(k.cooldown_total > 0.0);
            }
        }
    }

    /// Both ends of a Sacrifice swap run the arrival hook: the skeleton lands
    /// in the caster's water and comes up Wet, the caster lands in the
    /// skeleton's oil and comes up Oiled.
    #[test]
    fn sacrifice_swap_runs_tile_effects_for_both() {
        let mut arena = Arena::new((5, 5));
        let player = arena.player;
        arena.grid.water_positions.push((5, 5));
        let skeleton = raised_skeleton(&mut arena, 5, 7);
        crate::spawning::spawn_oil_puddle(&mut arena.world, 5, 7);

        arena.player_does(ActionType::Sacrifice { skeleton });
        assert_eq!(arena.pos(player), (5, 7));
        assert!(queries::has_status_effect(&arena.world, player, EffectType::Oiled));
        assert!(queries::has_status_effect(&arena.world, skeleton, EffectType::Wet));
    }

    /// Call Rain puts out everything burning in the area — creatures, grass,
    /// oil — soaks the grass, and leaves every creature in it Wet, the druid
    /// included. A creature outside the radius is untouched.
    #[test]
    fn call_rain_douses_and_soaks_everyone_in_the_area() {
        use crate::components::{BurningGrass, BurningOil, WetGrass};
        let mut arena = Arena::new((5, 5));
        let player = arena.player;
        for x in 7..=9 {
            if let Some(t) = arena.grid.get_mut(x, 7) {
                t.tile_type = TileType::TallGrass;
            }
        }
        crate::spawning::spawn_burning_grass(&mut arena.world, 8, 7);
        let puddle = crate::spawning::spawn_oil_puddle(&mut arena.world, 6, 6);
        crate::systems::fire::ignite_oil_puddle(&mut arena.world, puddle);
        let rat = arena.rat(7, 6, 0.1);
        effects::add_effect_to_entity(&mut arena.world, rat, EffectType::Burning, 30.0);
        effects::add_effect_to_entity(&mut arena.world, player, EffectType::Oiled, 30.0);
        let far = arena.rat(13, 13, 0.1);

        arena.player_does(ActionType::CallRain { target_x: 7, target_y: 7 });

        assert!(arena.world.get::<&BurningOil>(puddle).is_err(), "oil fire out");
        assert!(arena.world.get::<&crate::components::OilPuddle>(puddle).is_ok(), "puddle stays");
        assert_eq!(arena.world.query::<&BurningGrass>().iter().count(), 0, "grass fire out");
        let soaked: Vec<(i32, i32)> = arena
            .world
            .query::<(&Position, &WetGrass)>()
            .iter()
            .map(|(_, (p, _))| (p.x, p.y))
            .collect();
        assert!(soaked.contains(&(9, 7)), "grass in the area is soaked: {soaked:?}");
        assert!(!queries::has_status_effect(&arena.world, rat, EffectType::Burning));
        assert!(queries::has_status_effect(&arena.world, rat, EffectType::Wet));
        assert!(queries::has_status_effect(&arena.world, player, EffectType::Wet), "druid in range");
        assert!(!queries::has_status_effect(&arena.world, player, EffectType::Oiled), "oil washed off");
        assert!(!queries::has_status_effect(&arena.world, far, EffectType::Wet), "outside the rain");

        let mut log = crate::ui::MessageLog::new(player);
        for ev in &arena.seen {
            log.record_event(ev, &arena.world);
        }
        let lines = log.lines();
        assert!(lines.iter().any(|l| l == "You are soaked."), "{lines:?}");
        assert!(lines.iter().any(|l| l == "Rain pours down and puts out 3 fires."), "{lines:?}");
    }

    /// Out of range is refused.
    #[test]
    fn call_rain_respects_its_range() {
        let mut arena = Arena::new((1, 1));
        let r = apply_call_rain(
            &mut EffectCtx {
                world: &mut arena.world,
                grid: &mut arena.grid,
                spatial: &mut arena.cache,
                events: &mut arena.events,
                rng: &mut arena.rng,
            },
            arena.player,
            1 + CALL_RAIN_RANGE + 1,
            1,
        );
        assert_eq!(r, ActionResult::Blocked);
    }
}
