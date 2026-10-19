//! Orc charges.
//!
//! A `Charger` (orc) whose target stands on one of the eight straight lines
//! from it, `ORC_CHARGE_MIN_RANGE..=ORC_CHARGE_MAX_RANGE` away down a clear
//! lane, lowers its head ([`try_start_charge`], from the AI) and winds up
//! (`ActionType::OrcChargeWindup`, telegraphed down the lane by
//! `systems::telegraph`). The direction is fixed when the wind-up starts. When
//! it completes ([`apply_orc_charge`]) the orc dashes up to
//! `ORC_CHARGE_MAX_RANGE + 1` tiles along it, stopping before the first thing
//! in the way:
//!
//! - a creature hostile to it: a charge hit (`ORC_CHARGE_DAMAGE_MULT` times a
//!   normal blow, through the shared melee path so Guard, wards, Thorns and
//!   flanking all apply) and a one-tile knockback down the lane;
//! - a wall or furniture: it slams into it, Stunned for `ORC_WALL_STUN`;
//! - nothing (or one of its own): it stumbles, Stunned for
//!   `ORC_STUMBLE_DURATION`.
//!
//! So stepping out of the lane during the wind-up dodges the charge, and
//! baiting it into a wall buys a free beating.

use hecs::{Entity, World};

use crate::components::{ActionType, Charger, EffectType, LungeAnimation, Position};
use crate::constants::*;
use crate::engine::EffectCtx;
use crate::events::{ChargeOutcome, DamageKind, EventQueue, GameEvent};
use crate::grid::Grid;
use crate::queries;
use crate::spatial_cache::SpatialCache;
use crate::systems::actions::{apply_melee_blow, is_guarding, ActionResult, MeleeBlow};
use crate::systems::combat::combat_side;

/// The unit step from `from` toward `to` if `to` lies on one of the eight
/// straight lines (orthogonal or exact diagonal) through `from`.
pub fn charge_direction(from: (i32, i32), to: (i32, i32)) -> Option<(i32, i32)> {
    let (dx, dy) = (to.0 - from.0, to.1 - from.1);
    if (dx, dy) == (0, 0) {
        return None;
    }
    if dx == 0 || dy == 0 || dx.abs() == dy.abs() {
        Some((dx.signum(), dy.signum()))
    } else {
        None
    }
}

/// Every tile strictly between `from` and the tile `dist` steps along `dir`
/// is walkable and holds nothing that blocks movement.
fn lane_is_clear(
    grid: &Grid,
    spatial: &SpatialCache,
    from: (i32, i32),
    dir: (i32, i32),
    dist: i32,
) -> bool {
    (1..dist).all(|i| {
        let t = (from.0 + dir.0 * i, from.1 + dir.1 * i);
        grid.is_walkable(t.0, t.1) && !spatial.is_blocked(t)
    })
}

/// Should the charger `orc` at `orc_pos` charge a target at `target_pos`?
/// If so, start its cooldown, announce the wind-up (when the player can see
/// the orc) and return the wind-up action. None for non-chargers, on
/// cooldown, out of the charge band, off the eight lines, or with anything in
/// the lane.
pub fn try_start_charge(
    world: &mut World,
    grid: &Grid,
    spatial: &SpatialCache,
    orc: Entity,
    orc_pos: (i32, i32),
    target_pos: (i32, i32),
    events: &mut EventQueue,
) -> Option<ActionType> {
    let ready = world.get::<&Charger>(orc).ok()?.cooldown <= 0.0;
    if !ready {
        return None;
    }
    let dist = (target_pos.0 - orc_pos.0).abs().max((target_pos.1 - orc_pos.1).abs());
    if !(ORC_CHARGE_MIN_RANGE..=ORC_CHARGE_MAX_RANGE).contains(&dist) {
        return None;
    }
    let dir = charge_direction(orc_pos, target_pos)?;
    if !lane_is_clear(grid, spatial, orc_pos, dir, dist) {
        return None;
    }
    if let Ok(mut charger) = world.get::<&mut Charger>(orc) {
        charger.cooldown = ORC_CHARGE_COOLDOWN;
    }
    if grid.get(orc_pos.0, orc_pos.1).map(|t| t.visible).unwrap_or(false) {
        events.push(GameEvent::ChargeWindup { attacker: orc, position: orc_pos, dir });
    }
    Some(ActionType::OrcChargeWindup { dx: dir.0, dy: dir.1 })
}

/// What stopped a dash.
enum Stop {
    /// A creature hostile to the charger.
    Foe(Entity),
    /// One of the charger's own side.
    Ally,
    /// Terrain or a non-creature blocker.
    Wall,
    /// Nothing: the full length was run.
    Ran,
}

/// The wind-up completed: dash along `(dx, dy)` and resolve the impact.
pub fn apply_orc_charge(ctx: &mut EffectCtx, orc: Entity, dx: i32, dy: i32) -> ActionResult {
    let Some(start) = queries::get_entity_position(ctx.world, orc) else {
        return ActionResult::Invalid;
    };
    if (dx, dy) == (0, 0) {
        return ActionResult::Invalid;
    }
    // Pinned or knocked senseless during the wind-up: the charge never goes.
    if [EffectType::Stunned, EffectType::Rooted, EffectType::Grabbed]
        .iter()
        .any(|&e| queries::has_status_effect(ctx.world, orc, e))
    {
        return ActionResult::Completed;
    }
    let my_side = combat_side(ctx.world, orc);

    let mut cur = start;
    let mut stop = Stop::Ran;
    for _ in 0..=ORC_CHARGE_MAX_RANGE {
        let next = (cur.0 + dx, cur.1 + dy);
        if let Some(creature) = queries::get_attackable_at(ctx.world, next.0, next.1, Some(orc)) {
            stop = match combat_side(ctx.world, creature) {
                Some(side) if Some(side) != my_side => Stop::Foe(creature),
                Some(_) => Stop::Ally,
                None => Stop::Wall, // an oil barrel and the like
            };
            break;
        }
        if !ctx.grid.is_walkable(next.0, next.1)
            || queries::is_position_blocked(ctx.spatial, next.0, next.1, Some(orc))
        {
            stop = Stop::Wall;
            break;
        }
        cur = next;
    }

    // The dash itself: one move from start to end. The visual position is
    // left behind on purpose, so the normal lerp plays it as a fast rush.
    if cur != start {
        if let Ok(mut pos) = ctx.world.get::<&mut Position>(orc) {
            pos.x = cur.0;
            pos.y = cur.1;
        }
        ctx.spatial.update_position(orc, start, cur);
        ctx.events.push(GameEvent::EntityMoved { entity: orc, from: start, to: cur });
        crate::systems::tile_effects::on_enter_tile(ctx.world, ctx.grid, orc, cur, ctx.events, ctx.rng);
    }

    let visible = ctx.grid.get(cur.0, cur.1).map(|t| t.visible).unwrap_or(false);
    let stun = |ctx: &mut EffectCtx, duration: f32, outcome: ChargeOutcome| {
        crate::systems::effects::add_effect_to_entity(ctx.world, orc, EffectType::Stunned, duration);
        if visible {
            ctx.events.push(GameEvent::ChargeMissed { attacker: orc, position: cur, outcome });
        }
    };
    match stop {
        Stop::Foe(victim) => {
            // A landing tile can kill (a fire trap): the dead do not hit.
            if crate::systems::combat::is_dead(ctx.world, orc) {
                return ActionResult::Completed;
            }
            let braced = is_guarding(ctx.world, victim);
            let blow = MeleeBlow { damage_mult: ORC_CHARGE_DAMAGE_MULT, kind: DamageKind::Charge };
            apply_melee_blow(ctx, orc, victim, blow);
            // The dash is the strike; a lunge would snap the visual onto the
            // landing tile and swallow it.
            let _ = ctx.world.remove_one::<LungeAnimation>(orc);
            // A guard keeps their feet, as against the boss's slam.
            let alive = ctx
                .world
                .get::<&crate::components::Health>(victim)
                .map(|h| h.current > 0)
                .unwrap_or(false);
            if alive && !braced {
                crate::systems::combat::try_knockback(
                    ctx.world, ctx.grid, ctx.spatial, orc, victim, ctx.events, ctx.rng,
                );
            }
        }
        Stop::Wall => stun(ctx, ORC_WALL_STUN, ChargeOutcome::Wall),
        Stop::Ally | Stop::Ran => stun(ctx, ORC_STUMBLE_DURATION, ChargeOutcome::Stumble),
    }
    ActionResult::Completed
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::components::{BlocksMovement, Health};
    use crate::events::GameEvent;
    use crate::systems::actions::TestArena as Arena;
    use crate::tile::{Tile, TileType};

    /// An orc at (x, y), charge ready, hunting the arena's player.
    fn orc(arena: &mut Arena, x: i32, y: i32) -> Entity {
        let o = crate::spawning::enemies::ORC.spawn(&mut arena.world, x, y, &mut arena.rng);
        arena.hunt(o, 1.0);
        o
    }

    fn decide(arena: &mut Arena, e: Entity) -> Option<ActionType> {
        crate::systems::ai::decide_action(&mut arena.ctx(), e);
        arena.action(e)
    }

    fn wall(arena: &mut Arena, x: i32, y: i32) {
        if let Some(t) = arena.grid.get_mut(x, y) {
            *t = Tile::new(TileType::Wall);
        }
    }

    fn charge_hits(arena: &Arena, orc: Entity) -> Vec<i32> {
        arena
            .seen
            .iter()
            .filter_map(|e| match e {
                GameEvent::AttackHit { attacker, kind: DamageKind::Charge, damage, .. }
                    if *attacker == orc =>
                {
                    Some(*damage)
                }
                _ => None,
            })
            .collect()
    }

    #[test]
    fn charge_direction_is_one_of_eight_lines() {
        assert_eq!(charge_direction((0, 0), (4, 0)), Some((1, 0)));
        assert_eq!(charge_direction((0, 0), (-3, 3)), Some((-1, 1)));
        assert_eq!(charge_direction((0, 0), (3, 1)), None);
        assert_eq!(charge_direction((2, 2), (2, 2)), None);
    }

    #[test]
    fn orcs_charge_but_gnash_slams() {
        let mut arena = Arena::new((1, 1));
        let o = orc(&mut arena, 8, 8);
        assert!(arena.world.get::<&Charger>(o).is_ok());
        let boss = crate::spawning::spawn_boss(&mut arena.world, 3, 3, 12, &mut arena.rng).unwrap();
        assert!(arena.world.get::<&Charger>(boss).is_err(), "Gnash keeps his slam");
    }

    #[test]
    fn winds_up_when_the_target_is_in_a_clear_lane_and_in_range() {
        let mut arena = Arena::new((2, 8));
        let o = orc(&mut arena, 2 + ORC_CHARGE_MIN_RANGE + 1, 8);
        assert!(matches!(decide(&mut arena, o), Some(ActionType::OrcChargeWindup { dx: -1, dy: 0 })));
        assert!(arena.world.get::<&Charger>(o).unwrap().cooldown > 0.0, "cooldown started");

        // Telegraphed down the lane.
        let t = crate::systems::telegraph::hostile_telegraphs(&arena.world, arena.clock.time);
        assert_eq!(t.len(), 1);
        let tiles = t[0].shape.tiles();
        assert!(tiles.contains(&(2, 8)), "the player's tile is threatened: {tiles:?}");
        assert_eq!(tiles.len() as i32, ORC_CHARGE_MAX_RANGE + 1);

        // Too close: no charge, just the walk in.
        let mut arena = Arena::new((2, 8));
        let o = orc(&mut arena, 2 + ORC_CHARGE_MIN_RANGE - 1, 8);
        assert!(!matches!(decide(&mut arena, o), Some(ActionType::OrcChargeWindup { .. })));

        // Off the eight lines: no charge.
        let mut arena = Arena::new((2, 8));
        let o = orc(&mut arena, 6, 9);
        assert!(!matches!(decide(&mut arena, o), Some(ActionType::OrcChargeWindup { .. })));
    }

    #[test]
    fn no_wind_up_when_the_lane_is_blocked() {
        let mut arena = Arena::new((2, 8));
        arena.world.spawn((Position::new(4, 8), BlocksMovement));
        arena.cache.rebuild_in_place(&arena.world);
        let o = orc(&mut arena, 6, 8);
        assert!(!matches!(decide(&mut arena, o), Some(ActionType::OrcChargeWindup { .. })));

        let mut arena = Arena::new((2, 8));
        wall(&mut arena, 4, 8);
        let o = orc(&mut arena, 6, 8);
        assert!(!matches!(decide(&mut arena, o), Some(ActionType::OrcChargeWindup { .. })));
    }

    /// Mark every tile visible, so player-visible-only events are emitted.
    fn reveal(arena: &mut Arena) {
        for t in arena.grid.tiles.iter_mut() {
            t.visible = true;
        }
    }

    fn log_of(arena: &Arena) -> Vec<String> {
        let mut log = crate::ui::MessageLog::new(arena.player);
        for ev in &arena.seen {
            log.record_event(ev, &arena.world);
        }
        log.lines()
    }

    /// Standing in the lane when the wind-up completes: the orc rushes up,
    /// hits for boosted damage and shoves the player one tile back.
    #[test]
    fn a_charge_that_connects_hits_hard_and_knocks_back() {
        let mut arena = Arena::new((2, 8));
        reveal(&mut arena);
        let player = arena.player;
        {
            let mut h = arena.world.get::<&mut Health>(player).unwrap();
            h.max = 200;
            h.current = 200;
        }
        let o = orc(&mut arena, 6, 8);
        assert!(matches!(decide(&mut arena, o), Some(ActionType::OrcChargeWindup { .. })));
        let drained: Vec<GameEvent> = arena.events.drain().collect();
        arena.seen.extend(drained);
        arena.wait_until(ORC_CHARGE_WINDUP + 0.05);

        let hits = charge_hits(&arena, o);
        assert_eq!(hits.len(), 1, "one charge hit: {:?}", arena.seen);
        assert_eq!(arena.pos(o), (3, 8), "the orc rushed up the lane");
        assert_eq!(arena.pos(player), (1, 8), "knocked back down the lane");
        arena.cache.assert_coherent_with_world(&arena.world, "after the charge");

        // Even the weakest charge roll beats the plain swing's average.
        let base = ORC_DAMAGE + (ORC_STRENGTH - 10) / 2;
        let min_charge =
            (((base as f32 * COMBAT_DAMAGE_MIN_MULT) as i32) as f32 * ORC_CHARGE_DAMAGE_MULT) as i32;
        assert!(hits[0] >= min_charge, "{} < {}", hits[0], min_charge);
        assert!(hits[0] > base, "more than a plain swing");
        assert!(!arena.stunned(o), "a connecting charge does not stun the orc");

        let lines = log_of(&arena);
        assert!(lines.iter().any(|l| l == "The Orc lowers its head to charge!"), "{lines:?}");
        assert!(lines.iter().any(|l| l.starts_with("The Orc's charge slams into you")), "{lines:?}");
    }

    /// Sidestepping out of the lane during the wind-up: the orc runs past and
    /// stumbles.
    #[test]
    fn sidestepping_the_lane_makes_it_stumble() {
        let mut arena = Arena::new((8, 8));
        reveal(&mut arena);
        let player = arena.player;
        // A slow orc, so the player's step resolves before the dash.
        let o = orc(&mut arena, 12, 8);
        arena.world.get::<&mut crate::components::Actor>(o).unwrap().speed = 0.5;
        assert!(matches!(decide(&mut arena, o), Some(ActionType::OrcChargeWindup { .. })));
        arena.player_does(ActionType::Move { dx: 0, dy: 1, is_diagonal: false });
        assert_eq!(arena.pos(player), (8, 9));

        arena.wait_until(2.0 * ORC_CHARGE_WINDUP + 0.05);
        assert!(charge_hits(&arena, o).is_empty());
        assert_eq!(arena.hp(player), 30);
        assert_eq!(arena.pos(o), (12 - (ORC_CHARGE_MAX_RANGE + 1), 8), "ran the full length");
        assert!(arena.stunned(o), "and stumbled");
        assert!(arena.seen.iter().any(|e| matches!(e,
            GameEvent::ChargeMissed { outcome: ChargeOutcome::Stumble, .. })));
        assert!(log_of(&arena).iter().any(|l| l == "The Orc stumbles past!"));
        arena.cache.assert_coherent_with_world(&arena.world, "stumble");
    }

    /// Charging into a wall leaves the orc stunned for the wall stun.
    #[test]
    fn charging_into_a_wall_self_stuns() {
        let mut arena = Arena::new((5, 2));
        reveal(&mut arena);
        let o = orc(&mut arena, 10, 8);
        wall(&mut arena, 7, 8);
        let r = apply_orc_charge(&mut arena.ctx().effects(), o, -1, 0);
        assert_eq!(r, ActionResult::Completed);
        assert_eq!(arena.pos(o), (8, 8), "stopped before the wall");
        let remaining = arena
            .world
            .get::<&crate::components::StatusEffects>(o)
            .unwrap()
            .effects
            .iter()
            .find(|e| e.effect_type == EffectType::Stunned)
            .map(|e| e.remaining_duration);
        assert_eq!(remaining, Some(ORC_WALL_STUN));
        let drained: Vec<GameEvent> = arena.events.drain().collect();
        arena.seen.extend(drained);
        assert!(log_of(&arena).iter().any(|l| l == "The Orc slams into the wall!"));
        arena.cache.assert_coherent_with_world(&arena.world, "wall charge");
    }

    /// A guarding player blocks most of the charge and keeps their feet.
    #[test]
    fn guard_reduces_the_charge_hit() {
        // Same seed twice: once guarding, once open.
        let mut guarded = Arena::new((2, 8));
        let o = orc(&mut guarded, 5, 8);
        let mut open = Arena::new((2, 8));
        let o2 = orc(&mut open, 5, 8);
        let player = guarded.player;
        crate::systems::effects::add_effect_to_entity(
            &mut guarded.world, player, EffectType::Guarding, 5.0,
        );
        apply_orc_charge(&mut guarded.ctx().effects(), o, -1, 0);
        apply_orc_charge(&mut open.ctx().effects(), o2, -1, 0);
        let g: Vec<GameEvent> = guarded.events.drain().collect();
        let u: Vec<GameEvent> = open.events.drain().collect();
        let dmg = |evs: &[GameEvent]| {
            evs.iter()
                .find_map(|e| match e {
                    GameEvent::AttackHit { kind: DamageKind::Charge, damage, .. } => Some(*damage),
                    _ => None,
                })
                .expect("the charge hit")
        };
        assert!(dmg(&g) < dmg(&u), "{} vs {}", dmg(&g), dmg(&u));
        assert!(g.iter().any(|e| matches!(e, GameEvent::AttackBlocked { .. })));
        assert_eq!(guarded.pos(player), (2, 8), "a guard keeps their feet");
        let p2 = open.player;
        assert!(open.hp(p2) <= 0 || open.pos(p2) == (1, 8), "an open target is knocked back");
    }
}
