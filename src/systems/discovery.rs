//! Passive discovery of hidden dungeon traps and secret doors.
//!
//! Dungeon generation hides floor traps (no sprite) and occasionally seals a
//! small room behind a `SecretDoor` that renders as a wall. Every time the
//! player finishes a step, each hidden trap / secret door within one tile
//! gets a detection roll scaled by the player's *effective* Agility:
//!
//! `chance = DETECT_BASE_CHANCE * (1 + (agi - 10) * DETECT_AGI_SCALING)`
//!
//! On success the trap gains a tinted trap-door sprite (it can now be stepped
//! around), or the secret door converts into a normal openable door with a
//! distinct tint. Hooked from `apply_move` (see `systems::actions::movement`).

use hecs::Entity;
use crate::world::World;
use rand::Rng;

use crate::components::{Concealed, Door, Position, SecretDoor, Sprite, SpriteTint};
use crate::constants::*;
use crate::events::{EventQueue, GameEvent};
use crate::tile::tile_ids;

/// Per-step chance to spot an adjacent hidden trap or secret door, scaled by
/// effective Agility and clamped so it is never impossible nor guaranteed.
pub fn detection_chance(agility: i32) -> f64 {
    (DETECT_BASE_CHANCE * (1.0 + (agility - 10) as f64 * DETECT_AGI_SCALING))
        .clamp(DETECT_CHANCE_MIN, DETECT_CHANCE_MAX)
}

/// After a player step: roll detection for each hidden trap and secret door
/// within one tile (Chebyshev), scaled by the player's effective Agility.
pub fn roll_player_discovery(
    world: &mut World,
    player: Entity,
    events: &mut EventQueue,
    rng: &mut impl Rng,
) {
    let Some((px, py)) = crate::queries::get_entity_position(world, player) else {
        return;
    };
    let chance = detection_chance(crate::queries::effective_stats(world, player).agility);

    // Hidden things within one tile.
    let hidden: Vec<(Entity, (i32, i32))> = world
        .query::<(&Position, &Concealed)>()
        .iter()
        .filter(|(_, (p, _))| (p.x - px).abs() <= 1 && (p.y - py).abs() <= 1)
        .map(|(id, (p, _))| (id, (p.x, p.y)))
        .collect();
    for (id, pos) in hidden {
        if rng.gen_bool(chance) {
            reveal(world, id);
            events.push(GameEvent::TrapSpotted { position: pos });
        }
    }

    // Secret doors within one tile.
    let secret_doors: Vec<(Entity, (i32, i32))> = world
        .query::<(&Position, &SecretDoor)>()
        .iter()
        .filter(|(_, (p, _))| (p.x - px).abs() <= 1 && (p.y - py).abs() <= 1)
        .map(|(id, (p, _))| (id, (p.x, p.y)))
        .collect();
    for (id, pos) in secret_doors {
        if rng.gen_bool(chance) {
            discover_secret_door(world, id);
            events.push(GameEvent::SecretDoorFound { position: pos });
        }
    }
}

/// Bring a `Concealed` entity into view, wearing the look it was hiding.
pub fn reveal(world: &mut World, entity: Entity) {
    if let Ok(hidden) = world.remove_one::<Concealed>(entity) {
        let _ = world.insert(entity, (hidden.sprite, hidden.tint));
    }
}

/// Convert a secret door into a normal closed (openable) door with a distinct
/// tint. It keeps blocking movement and vision until opened, like any closed
/// door; the existing DoorOpened event path clears the blocking flags.
pub fn discover_secret_door(world: &mut World, door: Entity) {
    let _ = world.remove_one::<SecretDoor>(door);
    let (r, g, b) = SECRET_DOOR_TINT;
    let _ = world.insert(
        door,
        (
            Door::new(),
            Sprite::from_ref(tile_ids::DOOR),
            SpriteTint { r, g, b },
        ),
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::components::BlocksMovement;

    #[test]
    fn test_detection_chance_scales_with_agility() {
        // Baseline AGI 10 = base chance.
        assert!((detection_chance(10) - DETECT_BASE_CHANCE).abs() < 1e-9);
        // AGI 14 (the default player) is ~32% better than base.
        let expected = DETECT_BASE_CHANCE * (1.0 + 4.0 * DETECT_AGI_SCALING);
        assert!((detection_chance(14) - expected).abs() < 1e-9);
        // Higher AGI is never worse.
        assert!(detection_chance(18) > detection_chance(12));
        // Extreme low AGI clamps at the floor instead of going negative
        // (gen_bool would panic on a negative probability).
        assert!((detection_chance(-100) - DETECT_CHANCE_MIN).abs() < 1e-9);
        // Extreme high AGI clamps below certainty.
        assert!((detection_chance(1000) - DETECT_CHANCE_MAX).abs() < 1e-9);
    }

    #[test]
    fn test_revealing_a_trap_gives_it_its_look() {
        use crate::components::TrapEffect;
        let mut world = World::new();
        let trap = crate::spawning::spawn_dungeon_trap(&mut world, 3, 3, TrapEffect::snare());
        assert!(world.get::<&Sprite>(trap).is_err(), "hidden traps must not render");

        reveal(&mut world, trap);

        assert!(world.get::<&Sprite>(trap).is_ok(), "revealed traps render");
        assert!(world.get::<&Concealed>(trap).is_err(), "no longer hidden");

        let spikes = crate::spawning::spawn_dungeon_trap(&mut world, 4, 3, TrapEffect::spike());
        reveal(&mut world, spikes);
        let sprite = world.get::<&Sprite>(spikes).map(|s| (s.sheet, s.tile_id)).ok();
        assert_eq!(sprite, Some(tile_ids::SPIKES), "a spike trap shows its spikes");
    }

    #[test]
    fn test_discover_secret_door_becomes_closed_door() {
        let mut world = World::new();
        let door =
            crate::spawning::spawn_secret_door(&mut world, 5, 5, crate::tile::tile_ids::WALL);

        discover_secret_door(&mut world, door);

        assert!(world.get::<&SecretDoor>(door).is_err(), "marker removed");
        let d = world.get::<&Door>(door).expect("now a real door");
        assert!(!d.is_open, "starts closed");
        // Still blocks movement until opened.
        assert!(world.get::<&BlocksMovement>(door).is_ok());
    }
}
