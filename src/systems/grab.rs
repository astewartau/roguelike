//! Zombie grabs: a connecting hit holds the victim in place.
//!
//! A `Grabber` (zombie) that lands a melee hit applies [`EffectType::Grabbed`]
//! for `ZOMBIE_GRAB_DURATION` and records itself in [`GrabbedBy`]. While
//! Grabbed the victim cannot walk (`apply_move` refuses the step and reports
//! a struggle; hostile AI treats it like Rooted), but can still attack and use
//! abilities. The grab ends:
//! - when the effect's timer runs out (`tick_status_effects`);
//! - early, when the grabber dies, is stunned, or is no longer adjacent
//!   ([`tick_grabs`], run after every completed action, and lazily by
//!   [`is_held`] when the victim tries to move).
//!
//! A slippery (Oiled) target cannot be grabbed at all.

use hecs::{Entity, World};

use crate::components::{EffectType, GrabbedBy, Grabber, Health};
use crate::constants::*;
use crate::events::{EventQueue, GameEvent};
use crate::queries;
use crate::systems::effects;

/// `grabber` just landed a damaging melee hit on `target`: if it grabs, take
/// hold (or slip off an Oiled target). No-op for non-grabbers and for a
/// target the hit killed.
pub fn try_grab(world: &mut World, events: &mut EventQueue, grabber: Entity, target: Entity) {
    if world.get::<&Grabber>(grabber).is_err() {
        return;
    }
    let alive = world.get::<&Health>(target).map(|h| h.current > 0).unwrap_or(false);
    if !alive {
        return;
    }
    if queries::is_slippery(world, target) {
        events.push(GameEvent::GrabSlipped { grabber, target });
        return;
    }
    if effects::add_effect_to_entity(world, target, EffectType::Grabbed, ZOMBIE_GRAB_DURATION) {
        let _ = world.insert_one(target, GrabbedBy { grabber });
        events.push(GameEvent::Grabbed { grabber, target });
    }
}

/// Whether the grab on `victim` (held by `grabber`) should end early: the
/// grabber is gone or dead, stunned, or no longer within reach.
fn grab_broken(world: &World, victim: Entity, grabber: Entity) -> bool {
    let grabber_alive = world.get::<&Health>(grabber).map(|h| h.current > 0).unwrap_or(false);
    if !grabber_alive {
        return true;
    }
    if queries::has_status_effect(world, grabber, EffectType::Stunned) {
        return true;
    }
    match (
        queries::get_entity_position(world, grabber),
        queries::get_entity_position(world, victim),
    ) {
        (Some(g), Some(v)) => (g.0 - v.0).abs().max((g.1 - v.1).abs()) > MELEE_REACH,
        _ => true,
    }
}

/// Let go of `victim`: drop the effect and the holder link, and announce it.
fn release(world: &mut World, events: &mut EventQueue, victim: Entity) {
    effects::remove_effect_from_entity(world, victim, EffectType::Grabbed);
    let _ = world.remove_one::<GrabbedBy>(victim);
    events.push(GameEvent::GrabReleased { entity: victim });
}

/// End every grab whose holder died, was stunned or drifted out of reach,
/// and tidy up the holder link of grabs whose timer ran out. Cheap (only
/// grabbed entities are visited); run after each completed action.
pub fn tick_grabs(world: &mut World, events: &mut EventQueue) {
    let held: Vec<(Entity, Entity)> = world
        .query::<&GrabbedBy>()
        .iter()
        .map(|(id, g)| (id, g.grabber))
        .collect();
    for (victim, grabber) in held {
        if !effects::entity_has_effect(world, victim, EffectType::Grabbed) {
            // Timed out: the status tick already removed the effect.
            let _ = world.remove_one::<GrabbedBy>(victim);
            events.push(GameEvent::GrabReleased { entity: victim });
        } else if grab_broken(world, victim, grabber) {
            release(world, events, victim);
        }
    }
}

/// Whether `entity` is currently held fast. Checks the grab is still valid
/// first (releasing it if not), so a victim whose zombie just died walks
/// away on the very next step.
pub fn is_held(world: &mut World, entity: Entity, events: &mut EventQueue) -> bool {
    if !effects::entity_has_effect(world, entity, EffectType::Grabbed) {
        return false;
    }
    let grabber = world.get::<&GrabbedBy>(entity).map(|g| g.grabber).ok();
    match grabber {
        Some(g) if grab_broken(world, entity, g) => {
            release(world, events, entity);
            false
        }
        // No recorded holder: honour the effect until its timer ends.
        _ => true,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::components::{ActionType, Position};
    use crate::systems::actions::{apply_attack, ActionResult, TestArena as Arena};

    /// A zombie next to the arena's player, hunting it (not scheduled: it
    /// only swings when a test tells it to).
    fn zombie(arena: &mut Arena, x: i32, y: i32) -> Entity {
        let z = crate::spawning::enemies::ZOMBIE.spawn(&mut arena.world, x, y, &mut arena.rng);
        arena.hunt(z, ZOMBIE_SPEED);
        z
    }

    fn zombie_hits(arena: &mut Arena, z: Entity) {
        let player = arena.player;
        let r = apply_attack(&mut arena.ctx().effects(), z, player);
        assert_eq!(r, ActionResult::Completed);
        let drained: Vec<GameEvent> = arena.events.drain().collect();
        arena.seen.extend(drained);
    }

    fn grabbed(arena: &Arena) -> bool {
        queries::has_status_effect(&arena.world, arena.player, EffectType::Grabbed)
    }

    #[test]
    fn zombie_grab_blocks_movement_but_not_attacks() {
        let mut arena = Arena::new((5, 5));
        let z = zombie(&mut arena, 6, 5);
        zombie_hits(&mut arena, z);
        assert!(grabbed(&arena), "a connecting hit grabs");
        assert!(arena.seen.iter().any(|e| matches!(e, GameEvent::Grabbed { target, .. } if *target == arena.player)));

        // Walking away goes nowhere.
        arena.player_does(ActionType::Move { dx: -1, dy: 0, is_diagonal: false });
        assert_eq!(arena.pos(arena.player), (5, 5), "held in place");
        assert!(arena.seen.iter().any(|e| matches!(e, GameEvent::GrabStruggle { .. })));

        // Swinging back works.
        assert!(grabbed(&arena), "still held when the swing starts");
        let before = arena.hp(z);
        arena.player_does(ActionType::Attack { target: z });
        assert!(arena.hp(z) < before, "a grabbed player can still attack");
    }

    #[test]
    fn grab_ends_when_the_zombie_dies() {
        let mut arena = Arena::new((5, 5));
        let z = zombie(&mut arena, 6, 5);
        zombie_hits(&mut arena, z);
        assert!(grabbed(&arena));

        arena.world.get::<&mut Health>(z).unwrap().current = 1;
        arena.player_does(ActionType::Attack { target: z });
        assert!(arena.hp(z) <= 0, "the zombie is dead");
        assert!(!grabbed(&arena), "its grip died with it");
        assert!(arena.world.get::<&GrabbedBy>(arena.player).is_err());

        arena.player_does(ActionType::Move { dx: -1, dy: 0, is_diagonal: false });
        assert_eq!(arena.pos(arena.player), (4, 5), "free to walk");
    }

    #[test]
    fn grab_ends_when_the_zombie_is_no_longer_adjacent_or_stunned() {
        let mut arena = Arena::new((5, 5));
        let z = zombie(&mut arena, 6, 5);
        zombie_hits(&mut arena, z);
        assert!(grabbed(&arena));

        // Shoved away (knockback, say): no longer within reach.
        let from = arena.pos(z);
        if let Ok(mut p) = arena.world.get::<&mut Position>(z) {
            p.x = 9;
        }
        arena.cache.update_position(z, from, (9, 5));
        tick_grabs(&mut arena.world, &mut arena.events);
        assert!(!grabbed(&arena), "out of reach lets go");
        assert!(arena.events.drain().any(|e| matches!(e, GameEvent::GrabReleased { .. })));

        // Back next to the player and grabbing again; then stunned.
        if let Ok(mut p) = arena.world.get::<&mut Position>(z) {
            p.x = 6;
        }
        arena.cache.update_position(z, (9, 5), (6, 5));
        zombie_hits(&mut arena, z);
        assert!(grabbed(&arena));
        effects::add_effect_to_entity(&mut arena.world, z, EffectType::Stunned, 2.0);
        tick_grabs(&mut arena.world, &mut arena.events);
        assert!(!grabbed(&arena), "a stunned zombie loses its grip");
    }

    #[test]
    fn grab_times_out() {
        let mut arena = Arena::new((5, 5));
        let z = zombie(&mut arena, 6, 5);
        zombie_hits(&mut arena, z);
        arena.wait_until(ZOMBIE_GRAB_DURATION + 0.1);
        assert!(!grabbed(&arena));
        assert!(arena.world.get::<&GrabbedBy>(arena.player).is_err(), "the holder link is tidied");
        arena.player_does(ActionType::Move { dx: -1, dy: 0, is_diagonal: false });
        assert_eq!(arena.pos(arena.player), (4, 5));
    }

    #[test]
    fn slippery_target_cannot_be_grabbed() {
        let mut arena = Arena::new((5, 5));
        let z = zombie(&mut arena, 6, 5);
        effects::add_effect_to_entity(&mut arena.world, arena.player, EffectType::Oiled, 10.0);
        zombie_hits(&mut arena, z);
        assert!(arena.hp(arena.player) < 30, "the hit still lands");
        assert!(!grabbed(&arena), "but the oiled player slips the grab");
        assert!(arena.seen.iter().any(|e| matches!(e, GameEvent::GrabSlipped { .. })));
        arena.player_does(ActionType::Move { dx: -1, dy: 0, is_diagonal: false });
        assert_eq!(arena.pos(arena.player), (4, 5));
    }

    #[test]
    fn only_grabbers_grab() {
        let mut arena = Arena::new((5, 5));
        let rat = arena.rat(6, 5, 1.0);
        let player = arena.player;
        let _ = apply_attack(&mut arena.ctx().effects(), rat, player);
        assert!(!grabbed(&arena));
    }
}
