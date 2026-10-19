//! Rat packs.
//!
//! Rats (`PackHunter`) spawn in packs of `RAT_PACK_MIN..=RAT_PACK_MAX`
//! (`spawning::SpawnConfig::spawn_all`) and are only brave together:
//!
//! - A pack rat with no living packmate within `RAT_PACK_RADIUS` is *alone*.
//!   Alone and aware of a foe, it runs (`ai` reuses the ordinary flee step,
//!   not the Feared status) — unless it is cornered, when it turns and
//!   fights.
//! - With its pack about it, it is immune to the wounded-morale panic in
//!   `combat::apply_damage`.
//! - When it becomes aware of a foe, or is hit, every packmate within
//!   `RAT_PACK_RADIUS` becomes aware too.
//!
//! "Packmate" means any other untamed pack rat: packs are not tracked by
//! identity, so two packs that meet simply become one bigger pack. A tamed
//! rat has left the pack and neither counts nor is counted.

use hecs::{Entity, World};

use crate::components::{AIState, Asleep, ChaseAI, Health, PackHunter, Player, Position, TamedBy};
use crate::constants::*;

/// Whether `entity` is an untamed pack rat.
fn is_wild_pack_rat(world: &World, entity: Entity) -> bool {
    world.get::<&PackHunter>(entity).is_ok() && world.get::<&TamedBy>(entity).is_err()
}

/// Living untamed pack rats within `RAT_PACK_RADIUS` (Chebyshev) of `rat`,
/// excluding `rat` itself.
fn packmates_near(world: &World, rat: Entity) -> Vec<Entity> {
    let Some(center) = crate::queries::get_entity_position(world, rat) else {
        return Vec::new();
    };
    world
        .query::<(&Position, &Health, &PackHunter)>()
        .without::<&TamedBy>()
        .iter()
        .filter(|(id, (p, h, _))| {
            *id != rat
                && h.current > 0
                && (p.x - center.0).abs().max((p.y - center.1).abs()) <= RAT_PACK_RADIUS
        })
        .map(|(id, _)| id)
        .collect()
}

/// Whether the pack rat `rat` is alone: no living packmate within
/// `RAT_PACK_RADIUS`. False for anything that is not a wild pack rat.
pub fn is_alone(world: &World, rat: Entity) -> bool {
    is_wild_pack_rat(world, rat) && packmates_near(world, rat).is_empty()
}

/// Whether `entity` is a pack rat with its pack around it, and so shrugs off
/// the wounded-morale panic.
pub fn steadied_by_pack(world: &World, entity: Entity) -> bool {
    is_wild_pack_rat(world, entity) && !packmates_near(world, entity).is_empty()
}

/// `rat` knows about `threat` (last seen at `threat_pos`): every packmate
/// within `RAT_PACK_RADIUS` that is not already hunting wakes and goes to
/// investigate, carrying the same threat. No-op for anything that is not a
/// wild pack rat. Already-aware packmates are left alone (their own
/// perception is better than a squeak).
pub fn alert_packmates(world: &mut World, rat: Entity, threat: Entity, threat_pos: (i32, i32)) {
    if !is_wild_pack_rat(world, rat) {
        return;
    }
    for mate in packmates_near(world, rat) {
        let unaware = world
            .get::<&ChaseAI>(mate)
            .map(|ai| matches!(ai.state, AIState::Unaware | AIState::Idle))
            .unwrap_or(false);
        if !unaware {
            continue;
        }
        let _ = world.remove_one::<Asleep>(mate);
        if let Ok(mut ai) = world.get::<&mut ChaseAI>(mate) {
            ai.state = AIState::Investigating;
            ai.alertness = 0.0;
            ai.add_threat(threat, WAKE_THREAT);
            ai.update_target_pos(threat, threat_pos);
        }
    }
}

/// A pack rat was hit: its packmates come running toward the player (damage
/// carries no attacker here, and the player is who rats are hunting; an
/// attacking companion is picked up by sight once they arrive).
pub fn alert_pack_of_attack(world: &mut World, rat: Entity) {
    if !is_wild_pack_rat(world, rat) {
        return;
    }
    let player = world.query::<&Player>().iter().next().map(|(id, _)| id);
    let Some(player) = player else { return };
    let Some(toward) = crate::queries::get_entity_position(world, player) else {
        return;
    };
    alert_packmates(world, rat, player, toward);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::components::{ActionType, Actor, EffectType};
    use crate::systems::actions::TestArena as Arena;

    fn rat(arena: &mut Arena, x: i32, y: i32) -> Entity {
        crate::spawning::enemies::RAT.spawn(&mut arena.world, x, y, &mut arena.rng)
    }

    fn state(arena: &Arena, e: Entity) -> AIState {
        arena.world.get::<&ChaseAI>(e).unwrap().state
    }

    #[test]
    fn rats_are_pack_hunters_and_alone_only_without_packmates_nearby() {
        let mut arena = Arena::new((1, 1));
        let a = rat(&mut arena, 8, 8);
        assert!(arena.world.get::<&PackHunter>(a).is_ok(), "rats hunt in packs");
        assert!(is_alone(&arena.world, a));
        let b = rat(&mut arena, 8 + RAT_PACK_RADIUS, 8);
        assert!(!is_alone(&arena.world, a), "a packmate at the edge of the radius counts");
        assert!(steadied_by_pack(&arena.world, a));
        arena.world.get::<&mut Health>(b).unwrap().current = 0;
        assert!(is_alone(&arena.world, a), "a dead packmate does not");
    }

    /// A pack rat that spots the player rouses its sleeping packmates.
    #[test]
    fn an_alert_spreads_through_the_pack() {
        let mut arena = Arena::new((4, 4));
        let scout = rat(&mut arena, 6, 4);
        let sleeper = rat(&mut arena, 9, 6);
        let far = rat(&mut arena, 6 + RAT_PACK_RADIUS + 4, 12);
        let _ = arena.world.insert_one(sleeper, Asleep);
        arena.hunt(scout, 1.0);
        assert_eq!(state(&arena, sleeper), AIState::Unaware);

        crate::systems::ai::decide_action(&mut arena.ctx(), scout);
        assert!(
            matches!(state(&arena, sleeper), AIState::Investigating | AIState::Chasing),
            "the packmate is roused"
        );
        assert!(arena.world.get::<&Asleep>(sleeper).is_err(), "and awake");
        assert_eq!(state(&arena, far), AIState::Unaware, "a rat out of earshot sleeps on");
    }

    /// Hitting a pack rat brings its packmates too.
    #[test]
    fn a_hit_pack_rat_alerts_its_packmates() {
        let mut arena = Arena::new((4, 4));
        let bitten = rat(&mut arena, 5, 4);
        let mate = rat(&mut arena, 7, 7);
        let player = arena.player;
        let _ = crate::systems::actions::apply_attack(&mut arena.ctx().effects(), player, bitten);
        assert!(matches!(state(&arena, mate), AIState::Investigating | AIState::Chasing));
    }

    /// Alone and aware, a rat runs instead of fighting — without the Feared
    /// status.
    #[test]
    fn a_lone_rat_flees_when_aware() {
        let mut arena = Arena::new((5, 8));
        let r = arena.rat(7, 8, 1.0);
        crate::systems::ai::decide_action(&mut arena.ctx(), r);
        let action = arena.world.get::<&Actor>(r).unwrap().current_action.map(|a| a.action_type);
        match action {
            Some(ActionType::Move { dx, .. }) => assert_eq!(dx, 1, "it runs away from the player"),
            other => panic!("expected a flee step, got {other:?}"),
        }
        assert!(!crate::queries::has_status_effect(&arena.world, r, EffectType::Feared));
    }

    /// With its pack, the same rat closes in.
    #[test]
    fn a_rat_with_its_pack_closes_in() {
        let mut arena = Arena::new((5, 8));
        let r = arena.rat(7, 8, 1.0);
        let _mate = arena.rat(8, 10, 1.0);
        crate::systems::ai::decide_action(&mut arena.ctx(), r);
        let action = arena.world.get::<&Actor>(r).unwrap().current_action.map(|a| a.action_type);
        match action {
            Some(ActionType::Move { dx, .. }) => assert_eq!(dx, -1, "it advances on the player"),
            other => panic!("expected a step toward the player, got {other:?}"),
        }
    }

    /// Cornered, a lone rat fights.
    #[test]
    fn a_cornered_lone_rat_fights() {
        let mut arena = Arena::new((1, 0));
        let r = arena.rat(0, 0, 1.0);
        // Wall it into the corner: the only open neighbour is the player's tile.
        for t in [(0, 1), (1, 1)] {
            if let Some(tile) = arena.grid.get_mut(t.0, t.1) {
                *tile = crate::tile::Tile::new(crate::tile::TileType::Wall);
            }
        }
        crate::systems::ai::decide_action(&mut arena.ctx(), r);
        let action = arena.world.get::<&Actor>(r).unwrap().current_action.map(|a| a.action_type);
        assert!(
            matches!(action, Some(ActionType::Attack { target }) if target == arena.player),
            "got {action:?}"
        );
    }

    /// With its pack, a badly wounded rat never breaks (over many rolls);
    /// alone, it does.
    #[test]
    fn packmates_do_not_break_from_morale_fear() {
        use rand::SeedableRng;
        let mut arena = Arena::new((1, 1));
        let a = rat(&mut arena, 8, 8);
        let _b = rat(&mut arena, 9, 9);
        let mut rng = rand::rngs::StdRng::seed_from_u64(3);
        let mut events = crate::events::EventQueue::new();
        for _ in 0..40 {
            arena.world.get::<&mut Health>(a).unwrap().current = 5;
            crate::systems::combat::apply_damage_dot(&mut arena.world, a, 1, &mut rng, &mut events);
        }
        assert!(!crate::queries::has_status_effect(&arena.world, a, EffectType::Feared));

        let lone = rat(&mut arena, 1, 14);
        let mut broke = false;
        for _ in 0..40 {
            arena.world.get::<&mut Health>(lone).unwrap().current = 5;
            crate::systems::combat::apply_damage_dot(&mut arena.world, lone, 1, &mut rng, &mut events);
            broke |= crate::queries::has_status_effect(&arena.world, lone, EffectType::Feared);
        }
        assert!(broke, "a lone wounded rat can still panic");
    }
}
