//! Slimes split in two when badly hurt.
//!
//! A creature with [`Splits`] that is alive at or below
//! `SLIME_SPLIT_HP_FRACTION` of its max HP, has not split before (`spent`)
//! and is below `SLIME_MAX_SPLITS` generations, buds off a child onto a free
//! adjacent tile. HP is conserved: the child takes half the parent's current
//! HP (rounded down), the parent keeps the rest, and both halves' max HP is
//! halved to match. If no adjacent tile is free the chance is lost (it does
//! not retry once room opens up).
//!
//! The split is not done inside `combat::apply_damage`: damage arrives from
//! melee, arrows, fire, DoTs and explosions, several of them mid-query, and a
//! spawn needs the scheduler and AI tracker. Instead [`process_splits`] scans
//! for slimes that have crossed the threshold; it runs after every completed
//! action in `advance_until_player_ready` and once per engine frame (after
//! fire), so the split lands before the slime acts again whatever hurt it.
//!
//! **XP:** each split generation halves a slime's XP (see
//! `combat::remove_dead_entities`), so killing both halves pays what the
//! original would have.
//!
//! The child is spawned through `EnemyDef::spawn` from a copy of the parent's
//! template with `split_generation` bumped, so floor save/load restores it as
//! a split half (tinted, non-splitting, reduced XP). The parent's stored
//! template is bumped the same way.

use hecs::Entity;
use rand::seq::SliceRandom;

use crate::components::{Asleep, ChaseAI, Health, Splits, SpriteTint};
use crate::constants::*;
use crate::engine::ActorCtx;
use crate::events::GameEvent;
use crate::spawning::EnemyDef;

/// Whether a creature with this split state and health should split now.
fn wants_split(splits: &Splits, health: &Health) -> bool {
    !splits.spent
        && splits.generation < SLIME_MAX_SPLITS
        && health.current > 0
        && (health.current as f32) <= health.max as f32 * SLIME_SPLIT_HP_FRACTION
}

/// Split every slime that has crossed its threshold since the last call.
pub fn process_splits(ctx: &mut ActorCtx) {
    let candidates: Vec<Entity> = ctx
        .world
        .query::<(&Splits, &Health)>()
        .iter()
        .filter(|(_, (s, h))| wants_split(s, h))
        .map(|(id, _)| id)
        .collect();
    for parent in candidates {
        split_one(ctx, parent);
    }
}

/// Free tiles next to `pos` a new creature could stand on (all 8 neighbours).
fn free_neighbours(ctx: &ActorCtx, pos: (i32, i32)) -> Vec<(i32, i32)> {
    let mut free = Vec::new();
    for dy in -1..=1 {
        for dx in -1..=1 {
            if dx == 0 && dy == 0 {
                continue;
            }
            let t = (pos.0 + dx, pos.1 + dy);
            if ctx.grid.is_walkable(t.0, t.1) && !ctx.spatial.is_blocked(t) {
                free.push(t);
            }
        }
    }
    free
}

fn split_one(ctx: &mut ActorCtx, parent: Entity) {
    // The chance is used up whether or not there is room: mark it first.
    if let Ok(mut s) = ctx.world.get::<&mut Splits>(parent) {
        s.spent = true;
    }

    let Some(pos) = crate::queries::get_entity_position(ctx.world, parent) else { return };
    let (current, max) = match ctx.world.get::<&Health>(parent) {
        Ok(h) => (h.current, h.max),
        Err(_) => return,
    };
    let template = ctx.world.get::<&EnemyDef>(parent).map(|d| (*d).clone()).ok();

    let free = free_neighbours(ctx, pos);
    let spot = free.choose(ctx.rng).copied();
    let (Some(spot), Some(template), true) = (spot, template, current >= 2) else {
        // No room (or nothing to share, or no template to spawn from): no
        // split. Persist that on the stored template so a revisited floor
        // does not hand the slime a second chance.
        if let Ok(mut def) = ctx.world.get::<&mut EnemyDef>(parent) {
            def.splits = false;
        }
        return;
    };

    // Halve the HP between the two, conserving the total.
    let child_hp = current / 2;
    let parent_hp = current - child_hp;
    let half_max = ((max + 1) / 2).max(parent_hp);

    let mut child_def = template;
    child_def.split_generation = child_def.split_generation.saturating_add(1);

    // The parent becomes one of the two halves: next generation, tinted,
    // template updated so save/load agrees.
    if let Ok(mut h) = ctx.world.get::<&mut Health>(parent) {
        h.current = parent_hp;
        h.max = half_max;
    }
    if let Ok(mut s) = ctx.world.get::<&mut Splits>(parent) {
        s.generation = child_def.split_generation;
    }
    if let Ok(mut def) = ctx.world.get::<&mut EnemyDef>(parent) {
        *def = child_def.clone();
    }
    let (r, g, b) = SLIME_SPLIT_TINT;
    let _ = ctx.world.insert_one(parent, SpriteTint { r, g, b });

    // The child, through the normal enemy spawn path (save/loadable).
    let child = child_def.spawn(ctx.world, spot.0, spot.1, ctx.rng);
    if let Ok(mut h) = ctx.world.get::<&mut Health>(child) {
        h.current = child_hp;
        h.max = half_max;
    }
    // The split already happened; the child is not a fresh splitter.
    if let Ok(mut s) = ctx.world.get::<&mut Splits>(child) {
        s.spent = true;
    }
    // Same mind as the parent: awake and after the same target.
    let _ = ctx.world.remove_one::<Asleep>(child);
    let parent_ai = ctx.world.get::<&ChaseAI>(parent).map(|ai| (*ai).clone()).ok();
    if let (Some(ai), Ok(mut child_ai)) = (parent_ai, ctx.world.get::<&mut ChaseAI>(child)) {
        *child_ai = ai;
    }

    ctx.spatial.register_entity(child, spot, true, false);
    ctx.events.push(GameEvent::SlimeSplit { parent, child, position: spot });
    crate::engine::initialize_single_ai_actor(ctx, child);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::components::{BlocksMovement, Position};
    use crate::systems::actions::TestArena as Arena;

    fn slime(arena: &mut Arena, x: i32, y: i32) -> Entity {
        let s = crate::spawning::enemies::SLIME.spawn(&mut arena.world, x, y, &mut arena.rng);
        arena.hunt(s, SLIME_SPEED);
        s
    }

    fn slimes(arena: &Arena) -> Vec<Entity> {
        arena.world.query::<&Splits>().iter().map(|(id, _)| id).collect()
    }

    fn hurt(arena: &mut Arena, e: Entity, raw: i32) {
        crate::systems::combat::apply_damage(&mut arena.world, e, raw, &mut arena.rng, &mut arena.events);
        process_splits(&mut arena.ctx());
    }

    #[test]
    fn slime_splits_once_at_half_hp_conserving_hp() {
        let mut arena = Arena::new((2, 2));
        let parent = slime(&mut arena, 8, 8);

        // A scratch above half: no split.
        hurt(&mut arena, parent, 4);
        assert_eq!(arena.hp(parent), SLIME_HEALTH - 4);
        assert_eq!(slimes(&arena).len(), 1);

        // Down to 10/24 (at or below half): splits.
        hurt(&mut arena, parent, 10);
        let hp_before_split = SLIME_HEALTH - 14;
        let all = slimes(&arena);
        assert_eq!(all.len(), 2, "one slime became two");
        let child = *all.iter().find(|&&e| e != parent).unwrap();
        assert_eq!(arena.hp(parent) + arena.hp(child), hp_before_split, "HP is conserved");
        assert_eq!(arena.hp(child), hp_before_split / 2);

        // Adjacent, awake, after the same target, fireproof like its parent.
        let (cx, cy) = arena.pos(child);
        assert!((cx - 8).abs().max((cy - 8).abs()) == 1, "child is adjacent");
        let (state, target) = arena
            .world
            .get::<&ChaseAI>(child)
            .map(|ai| (ai.state, ai.current_target))
            .unwrap();
        assert!(
            matches!(state, crate::components::AIState::Chasing | crate::components::AIState::Investigating),
            "the child is aware, not asleep or patrolling: {state:?}"
        );
        assert_eq!(target, Some(arena.player), "the child goes after the parent's quarry");
        assert!(arena.world.get::<&Asleep>(child).is_err());
        assert!(arena.world.get::<&crate::components::Combustible>(child).is_err(), "fireproof");
        // Both halves are the next generation, tinted, with a matching template.
        for e in [parent, child] {
            assert_eq!(arena.world.get::<&Splits>(e).unwrap().generation, 1);
            assert!(arena.world.get::<&SpriteTint>(e).is_ok());
            assert_eq!(arena.world.get::<&EnemyDef>(e).unwrap().split_generation, 1);
        }
        arena.cache.assert_coherent_with_world(&arena.world, "after a split");
        assert!(arena.events.drain().any(|e| matches!(e, GameEvent::SlimeSplit { .. })));

        // The halves never split again, however hurt.
        let (p_hp, c_hp) = (arena.hp(parent), arena.hp(child));
        hurt(&mut arena, parent, p_hp - 1);
        hurt(&mut arena, child, c_hp - 1);
        assert_eq!(slimes(&arena).len(), 2, "children don't split");
    }

    #[test]
    fn no_split_when_surrounded() {
        let mut arena = Arena::new((2, 2));
        let s = slime(&mut arena, 8, 8);
        for dy in -1..=1 {
            for dx in -1..=1 {
                if dx != 0 || dy != 0 {
                    arena.world.spawn((Position::new(8 + dx, 8 + dy), BlocksMovement));
                }
            }
        }
        arena.cache.rebuild_in_place(&arena.world);
        hurt(&mut arena, s, 14);
        assert_eq!(slimes(&arena).len(), 1, "no room, no split");
        assert_eq!(arena.hp(s), SLIME_HEALTH - 14, "and it keeps all its HP");
        assert!(arena.world.get::<&Splits>(s).unwrap().spent, "the chance is used up");
        assert!(!arena.world.get::<&EnemyDef>(s).unwrap().splits, "persisted for save/load");
    }

    /// A split half's template respawns as a split half (floor save/load).
    #[test]
    fn split_template_respawns_as_a_half() {
        let mut arena = Arena::new((2, 2));
        let mut def = crate::spawning::enemies::SLIME.clone();
        def.split_generation = 1;
        let e = def.spawn(&mut arena.world, 5, 5, &mut arena.rng);
        let s = *arena.world.get::<&Splits>(e).unwrap();
        assert_eq!(s.generation, 1);
        assert!(arena.world.get::<&SpriteTint>(e).is_ok());
        arena.world.get::<&mut Health>(e).unwrap().current = 2;
        process_splits(&mut arena.ctx());
        assert_eq!(slimes(&arena).len(), 1);
    }

    /// The halves pay half XP each, so the pair is worth the original.
    #[test]
    fn split_halves_pay_half_xp() {
        let mut arena = Arena::new((2, 2));
        let whole = slime(&mut arena, 8, 8);
        let mut def = crate::spawning::enemies::SLIME.clone();
        def.split_generation = 1;
        let half = def.spawn(&mut arena.world, 10, 10, &mut arena.rng);
        let xp_of = |arena: &mut Arena, e: Entity| {
            let before = arena.world.get::<&crate::components::Experience>(arena.player).unwrap().current;
            let level_before = arena.world.get::<&crate::components::Experience>(arena.player).unwrap().level;
            arena.world.get::<&mut Health>(e).unwrap().current = 0;
            crate::systems::combat::remove_dead_entities(&mut arena.ctx(), 0);
            let exp = arena.world.get::<&crate::components::Experience>(arena.player).unwrap();
            assert_eq!(exp.level, level_before, "no level-up muddying the count");
            exp.current - before
        };
        let full = xp_of(&mut arena, whole);
        let halved = xp_of(&mut arena, half);
        assert_eq!(halved, (full / 2).max(1));
    }
}
