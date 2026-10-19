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

use crate::components::{EffectType, GrabbedBy, Grabber, Health, RootStruggleNoticed};
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

/// Whether `entity` is held in place and cannot take a step: Grabbed by a
/// zombie (see [`is_held`]) or Rooted (a web, a snare, Entangle). The shared
/// movement gate for walking and stairs — the player, companions and enemies
/// alike. Attacks and abilities are not gated: a pinned creature can still
/// swing, and Blink/Tumble/Disengage teleport it out.
///
/// Reports the struggle: a grab every attempt (`GrabStruggle`), a root once
/// per application (`RootStruggle`; remembered in [`RootStruggleNoticed`], so
/// holding a direction key does not flood the log).
pub fn pinned_in_place(world: &mut World, entity: Entity, events: &mut EventQueue) -> bool {
    if is_held(world, entity, events) {
        events.push(GameEvent::GrabStruggle { entity });
        return true;
    }
    let root_left = world
        .get::<&crate::components::StatusEffects>(entity)
        .ok()
        .and_then(|s| {
            s.effects
                .iter()
                .find(|e| e.effect_type == EffectType::Rooted)
                .map(|e| e.remaining_duration)
        });
    let Some(remaining) = root_left else {
        let _ = world.remove_one::<RootStruggleNoticed>(entity);
        return false;
    };
    // A root applied (or refreshed) since the last notice has more time on
    // it than was left then.
    let noticed = world
        .get::<&RootStruggleNoticed>(entity)
        .map(|n| remaining <= n.remaining)
        .unwrap_or(false);
    if !noticed {
        events.push(GameEvent::RootStruggle { entity });
    }
    let _ = world.insert_one(entity, RootStruggleNoticed { remaining });
    true
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

    // =========================================================================
    // Roots hold the player (and anyone else) in place
    // =========================================================================

    fn root_struggles(arena: &Arena) -> usize {
        arena.seen.iter().filter(|e| matches!(e, GameEvent::RootStruggle { .. })).count()
    }

    #[test]
    fn a_rooted_player_cannot_step_but_can_still_attack() {
        let mut arena = Arena::new((5, 5));
        let player = arena.player;
        effects::add_effect_to_entity(&mut arena.world, player, EffectType::Rooted, 4.0);

        arena.player_does(ActionType::Move { dx: -1, dy: 0, is_diagonal: false });
        assert_eq!(arena.pos(player), (5, 5), "stuck fast");
        arena.player_does(ActionType::Move { dx: -1, dy: 0, is_diagonal: false });
        assert_eq!(arena.pos(player), (5, 5));
        assert_eq!(root_struggles(&arena), 1, "said once per root, not per attempt");

        let mut log = crate::ui::MessageLog::new(player);
        for ev in &arena.seen {
            log.record_event(ev, &arena.world);
        }
        assert_eq!(
            log.lines().iter().filter(|l| *l == "You're stuck fast!").count(),
            1,
            "{:?}",
            log.lines()
        );

        // A fresh root is announced again.
        effects::add_effect_to_entity(&mut arena.world, player, EffectType::Rooted, 4.0);
        arena.player_does(ActionType::Move { dx: -1, dy: 0, is_diagonal: false });
        assert_eq!(root_struggles(&arena), 2);

        // Swinging is fine.
        let rat = arena.rat(6, 5, 0.1);
        arena.player_does(ActionType::Attack { target: rat });
        assert!(arena.hit(player, rat), "a rooted player still attacks");

        // Once the root is gone, the step goes through.
        effects::remove_effect_from_entity(&mut arena.world, player, EffectType::Rooted);
        arena.player_does(ActionType::Move { dx: -1, dy: 0, is_diagonal: false });
        assert_eq!(arena.pos(player), (4, 5));
        assert!(arena.world.get::<&RootStruggleNoticed>(player).is_err(), "notice cleared");
        arena.cache.assert_coherent_with_world(&arena.world, "rooted player");
    }

    #[test]
    fn a_web_roots_the_player() {
        let mut arena = Arena::new((5, 5));
        let player = arena.player;
        crate::spawning::spawn_web(&mut arena.world, 6, 5, None);
        arena.player_does(ActionType::Move { dx: 1, dy: 0, is_diagonal: false });
        assert_eq!(arena.pos(player), (6, 5), "walked into the web");
        assert!(queries::has_status_effect(&arena.world, player, EffectType::Rooted));
        arena.player_does(ActionType::Move { dx: 1, dy: 0, is_diagonal: false });
        assert_eq!(arena.pos(player), (6, 5), "and cannot walk out of it");
        assert_eq!(root_struggles(&arena), 1);
    }

    #[test]
    fn a_rooted_companion_cannot_walk_either() {
        let mut arena = Arena::new((5, 5));
        let player = arena.player;
        let pet = crate::spawning::enemies::RAT.spawn(&mut arena.world, 8, 8, &mut arena.rng);
        arena.world.insert_one(pet, crate::components::TamedBy { owner: player }).unwrap();
        effects::add_effect_to_entity(&mut arena.world, pet, EffectType::Rooted, 4.0);
        let r = crate::systems::actions::apply_move(&mut arena.ctx().effects(), pet, -1, 0);
        assert_eq!(r, ActionResult::Blocked);
        assert_eq!(arena.pos(pet), (8, 8));
        let _ = Position::new(0, 0);
    }
}
