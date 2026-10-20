//! Trap placement, trap triggering, and container opening.

use hecs::{Entity, World};

use rand::Rng;

use crate::components::{
    Container, ContainerType, EffectType, Position, Sprite, VisualPosition,
};
use crate::constants::*;
use crate::events::{EventQueue, GameEvent};
use crate::systems::effects;
use crate::tile::tile_ids;

use super::ActionResult;

/// Whatever steps on a trap sets it off, and the trap is used up. A trap
/// someone set spares them and their companions, and credits them with the
/// threat for any damage it does; the dungeon's own go off under anyone,
/// spotted or not.
pub(crate) fn check_trap_trigger(
    world: &mut World,
    grid: &crate::grid::Grid,
    victim: Entity,
    target_x: i32,
    target_y: i32,
    events: &mut EventQueue,
    rng: &mut impl Rng,
) {
    use crate::components::{Concealed, Follower, Trap, TrapEffect};

    let trap_info: Option<(Entity, Trap)> = world
        .query::<(&Position, &Trap)>()
        .iter()
        .find(|(_, (pos, _))| pos.x == target_x && pos.y == target_y)
        .map(|(id, (_, trap))| (id, *trap));
    let Some((trap_entity, trap)) = trap_info else {
        return;
    };

    if let Some(owner) = trap.owner {
        let owners_pet = world.get::<&Follower>(victim).is_ok_and(|t| t.leader == owner);
        if victim == owner || owners_pet {
            return;
        }
    }
    let was_hidden = world.get::<&Concealed>(trap_entity).is_ok();

    let mut damage = 0;
    match trap.effect {
        TrapEffect::Spike { damage: amount } | TrapEffect::Fire { damage: amount } => {
            damage = crate::systems::combat::apply_damage(world, grid, victim, amount, rng, events);
            crate::systems::ai::wake_on_attacked(world, victim);
            if let Some(owner) = trap.owner {
                let threat = damage as f32 * THREAT_PER_DAMAGE;
                crate::systems::ai::generate_threat(world, victim, owner, threat);
                crate::systems::ai::generate_companion_threat(world, victim, owner, threat);
            }
            if matches!(trap.effect, TrapEffect::Fire { .. }) {
                // Hook into the fire ecosystem: sets the victim Burning (it
                // is standing on the tile) and ignites any grass/oil there.
                crate::systems::fire::spill_fire_at(world, grid, target_x, target_y, events);
            }
        }
        TrapEffect::Snare { seconds } => {
            effects::add_effect_to_entity(world, victim, EffectType::Rooted, seconds);
        }
        TrapEffect::Alarm { radius } => {
            crate::systems::noise::make_noise(world, grid, (target_x, target_y), radius);
        }
    }

    events.push(GameEvent::TrapTriggered {
        effect: trap.effect,
        victim,
        position: (target_x, target_y),
        damage,
        was_hidden,
    });

    let _ = world.despawn(trap_entity);
}

/// Set a trap with `effect` at the target tile: a visible pressure plate
/// that the placer and their companions can cross safely.
pub fn apply_place_trap(
    world: &mut World,
    placer: Entity,
    effect: crate::components::TrapEffect,
    target_x: i32,
    target_y: i32,
    events: &mut EventQueue,
) -> ActionResult {
    use crate::components::{FlameOverlay, Trap, TrapEffect};
    let pos = Position::new(target_x, target_y);
    let trap = world.spawn((
        pos,
        VisualPosition::from_position(&pos),
        Sprite::from_ref(tile_ids::PRESSURE_PLATE),
        Trap { effect, owner: Some(placer) },
    ));
    if matches!(effect, TrapEffect::Fire { .. }) {
        let _ = world.insert_one(trap, FlameOverlay);
    }
    events.push(GameEvent::TrapPlaced { trap, placer, position: (target_x, target_y) });
    ActionResult::Completed
}

/// Apply open chest effect
pub fn apply_open_chest(
    world: &mut World,
    opener: Entity,
    chest: Entity,
    events: &mut EventQueue,
) -> ActionResult {
    // Check if this is a coffin that might spawn a skeleton
    let spawn_skeleton = {
        if let Ok(container) = world.get::<&Container>(chest) {
            if container.container_type == ContainerType::Coffin && !container.is_open {
                // Roll for skeleton spawn
                let roll: f32 = rand::random();
                roll < container.spawn_chance
            } else {
                false
            }
        } else {
            false
        }
    };

    // Get position for skeleton spawn if needed
    let spawn_pos = if spawn_skeleton {
        world.get::<&Position>(chest).ok().map(|p| (p.x, p.y))
    } else {
        None
    };

    // Mark container as open and get type
    let container_type = if let Ok(mut container) = world.get::<&mut Container>(chest) {
        container.is_open = true;
        Some(container.container_type)
    } else {
        None
    };

    // Get container position for audio
    let container_pos = world
        .get::<&Position>(chest)
        .map(|p| (p.x, p.y))
        .unwrap_or((0, 0));

    // If skeleton spawns, only emit the spawn event (player must deal with skeleton first)
    // Otherwise, emit ContainerOpened to show loot UI
    if let Some(position) = spawn_pos {
        // Skeleton spawning - emit spawn event but skip loot UI
        // Still emit ContainerOpened for sprite change, but skeleton takes priority
        events.push(GameEvent::ContainerOpened { container: chest, opener, container_type, position: container_pos });
        events.push(GameEvent::CoffinSkeletonSpawn { position });
    } else {
        // No skeleton - normal loot behavior
        events.push(GameEvent::ContainerOpened { container: chest, opener, container_type, position: container_pos });
    }

    ActionResult::Completed
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::components::{Attackable, Health, StatusEffects, Follower, TrapEffect};
    use crate::grid::Grid;
    use crate::tile::{Tile, TileType};
    use rand::rngs::StdRng;
    use rand::SeedableRng;

    /// Traps roll only for the morale check inside `apply_damage`; a fixed seed
    /// keeps these tests deterministic.
    fn test_rng() -> StdRng {
        StdRng::seed_from_u64(1)
    }

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

    #[test]
    fn test_spike_trap_damages_and_is_consumed() {
        let mut world = hecs::World::new();
        let grid = make_grid(7, 7);
        let mut events = EventQueue::new();

        let victim = world.spawn((
            Position::new(3, 3),
            Health::new(30),
            StatusEffects::new(),
            Attackable,
        ));
        let trap =
            crate::spawning::spawn_dungeon_trap(&mut world, 3, 3, TrapEffect::spike());

        check_trap_trigger(
            &mut world, &grid, victim, 3, 3, &mut events, &mut test_rng(),
        );

        let hp = world.get::<&Health>(victim).expect("victim alive").current;
        assert_eq!(hp, 30 - crate::constants::DUNGEON_SPIKE_TRAP_DAMAGE);
        assert!(!world.contains(trap), "triggered traps are consumed");
        assert!(events.drain().any(|e| matches!(
            e,
            GameEvent::TrapTriggered { effect: TrapEffect::Spike { .. }, was_hidden: true, .. }
        )));
    }

    #[test]
    fn test_snare_trap_roots_victim() {
        let mut world = hecs::World::new();
        let grid = make_grid(7, 7);
        let mut events = EventQueue::new();

        let victim = world.spawn((Position::new(2, 2), StatusEffects::new()));
        let trap =
            crate::spawning::spawn_dungeon_trap(&mut world, 2, 2, TrapEffect::snare());

        check_trap_trigger(
            &mut world, &grid, victim, 2, 2, &mut events, &mut test_rng(),
        );

        let rooted = world
            .get::<&StatusEffects>(victim)
            .map(|s| s.effects.iter().any(|e| e.effect_type == EffectType::Rooted))
            .unwrap_or(false);
        assert!(rooted, "snare traps root whoever steps on them");
        assert!(!world.contains(trap));
    }

    #[test]
    fn test_empty_tile_triggers_nothing() {
        let mut world = hecs::World::new();
        let grid = make_grid(5, 5);
        let mut events = EventQueue::new();

        let victim = world.spawn((Position::new(1, 1), StatusEffects::new()));
        check_trap_trigger(
            &mut world, &grid, victim, 1, 1, &mut events, &mut test_rng(),
        );
        assert_eq!(events.drain().count(), 0);
    }

    #[test]
    fn a_set_trap_spares_its_owner_and_their_pets() {
        let mut world = hecs::World::new();
        let grid = make_grid(7, 7);
        let mut events = EventQueue::new();
        let owner = world.spawn((Position::new(1, 1), StatusEffects::new()));
        let pet = world.spawn((Position::new(1, 1), StatusEffects::new(), Follower { leader: owner }));
        let stranger = world.spawn((Position::new(1, 1), StatusEffects::new()));
        apply_place_trap(&mut world, owner, TrapEffect::snare(), 1, 1, &mut events);

        for friend in [owner, pet] {
            check_trap_trigger(&mut world, &grid, friend, 1, 1, &mut events, &mut test_rng());
        }
        assert_eq!(world.query::<&crate::components::Trap>().iter().count(), 1, "still armed");

        check_trap_trigger(&mut world, &grid, stranger, 1, 1, &mut events, &mut test_rng());
        assert_eq!(world.query::<&crate::components::Trap>().iter().count(), 0, "sprung");
        assert!(crate::systems::effects::entity_has_effect(&world, stranger, EffectType::Rooted));
    }
}
