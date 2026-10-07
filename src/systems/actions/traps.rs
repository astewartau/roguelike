//! Trap placement, trap triggering, and container opening.

use hecs::{Entity, World};

use crate::components::{
    AbilityType, Container, ContainerType, EffectType, PlacedTrap, Position, Sprite, TrapType,
    VisualPosition,
};
use crate::constants::*;
use crate::events::{EventQueue, GameEvent};
use crate::systems::effects;
use crate::tile::tile_ids;

use super::{interrupt_life_drain_on_damage, ActionResult};

/// Check if an entity stepping on a fire trap should trigger it.
/// Fire traps ignore their owner and the owner's tamed pets.
pub(super) fn check_fire_trap_trigger(
    world: &mut World,
    victim: Entity,
    target_x: i32,
    target_y: i32,
    events: &mut EventQueue,
) {
    use crate::components::{PlacedFireTrap, TamedBy};
    use crate::constants::BURNING_DURATION;

    // Find fire trap at this position
    let trap_info: Option<(hecs::Entity, Entity, i32)> = world
        .query::<(&Position, &PlacedFireTrap)>()
        .iter()
        .find_map(|(trap_id, (pos, trap))| {
            if pos.x == target_x && pos.y == target_y {
                Some((trap_id, trap.owner, trap.burst_damage))
            } else {
                None
            }
        });

    let Some((trap_entity, trap_owner, burst_damage)) = trap_info else {
        return;
    };

    // Check if victim is the owner
    if victim == trap_owner {
        return;
    }

    // Check if victim is a tamed pet of the owner
    if let Ok(tamed_by) = world.get::<&TamedBy>(victim) {
        if tamed_by.owner == trap_owner {
            return;
        }
    }

    // Trap triggered! Apply burst damage (handles invulnerability, armor defense,
    // Protected/Barkskin) and a burning effect.
    crate::systems::combat::apply_damage(world, victim, burst_damage);

    // Interrupt life drain if victim was channeling
    interrupt_life_drain_on_damage(world, victim, events);

    // Generate threat on the victim for the trap owner
    crate::systems::ai::generate_threat(world, victim, trap_owner, burst_damage as f32 * THREAT_PER_DAMAGE);
    crate::systems::ai::generate_companion_threat(world, victim, trap_owner, burst_damage as f32 * THREAT_PER_DAMAGE);

    // Apply burning effect
    effects::add_effect_to_entity(world, victim, EffectType::Burning, BURNING_DURATION);

    // Emit events
    events.push(GameEvent::FireTrapTriggered {
        trap: trap_entity,
        victim,
        position: (target_x, target_y),
    });

    events.push(GameEvent::CaughtFire {
        entity: victim,
        position: (target_x, target_y),
    });

    // Destroy the trap after triggering
    let _ = world.despawn(trap_entity);
}

/// Check if an entity stepping on a snare trap should trigger it.
/// Snare traps ignore their owner and the owner's tamed pets.
pub(super) fn check_snare_trap_trigger(
    world: &mut World,
    victim: Entity,
    target_x: i32,
    target_y: i32,
    events: &mut EventQueue,
) {
    use crate::components::{PlacedTrap, TrapType, TamedBy};

    // Find snare trap at this position
    let trap_info: Option<(hecs::Entity, Entity, f32)> = world
        .query::<(&Position, &PlacedTrap)>()
        .iter()
        .find_map(|(trap_id, (pos, trap))| {
            if pos.x == target_x && pos.y == target_y {
                if let TrapType::Snare { root_duration } = trap.trap_type {
                    return Some((trap_id, trap.owner, root_duration));
                }
            }
            None
        });

    let Some((trap_entity, trap_owner, root_duration)) = trap_info else {
        return;
    };

    // Check if victim is the owner
    if victim == trap_owner {
        return;
    }

    // Check if victim is a tamed pet of the owner
    if let Ok(tamed_by) = world.get::<&TamedBy>(victim) {
        if tamed_by.owner == trap_owner {
            return;
        }
    }

    // Trap triggered! Apply rooted effect
    effects::add_effect_to_entity(world, victim, EffectType::Rooted, root_duration);

    // Emit event
    events.push(GameEvent::SnareTrapTriggered {
        trap: trap_entity,
        victim,
        position: (target_x, target_y),
    });

    // Destroy the trap after triggering
    let _ = world.despawn(trap_entity);
}

/// Check if an entity stepping on a dungeon-generated floor trap triggers it.
/// Unlike player-placed traps there is no owner exemption: everything that
/// steps on one (player, enemy, companion) sets it off, revealed or not.
/// Triggered traps are consumed.
pub(super) fn check_dungeon_trap_trigger(
    world: &mut World,
    grid: &crate::grid::Grid,
    victim: Entity,
    target_x: i32,
    target_y: i32,
    events: &mut EventQueue,
) {
    use crate::components::{DungeonTrap, DungeonTrapKind};

    let trap_info: Option<(Entity, DungeonTrapKind)> = world
        .query::<(&Position, &DungeonTrap)>()
        .iter()
        .find_map(|(trap_id, (pos, trap))| {
            if pos.x == target_x && pos.y == target_y {
                Some((trap_id, trap.kind))
            } else {
                None
            }
        });

    let Some((trap_entity, kind)) = trap_info else {
        return;
    };

    let mut damage = 0;
    match kind {
        DungeonTrapKind::Spike => {
            damage = crate::systems::combat::apply_damage(world, victim, DUNGEON_SPIKE_TRAP_DAMAGE);
            interrupt_life_drain_on_damage(world, victim, events);
            crate::systems::ai::wake_on_attacked(world, victim);
        }
        DungeonTrapKind::Fire => {
            damage = crate::systems::combat::apply_damage(world, victim, DUNGEON_FIRE_TRAP_DAMAGE);
            interrupt_life_drain_on_damage(world, victim, events);
            crate::systems::ai::wake_on_attacked(world, victim);
            // Hook into the fire ecosystem: sets the victim Burning (it is
            // standing on the tile) and ignites any grass/oil there.
            crate::systems::fire::spill_fire_at(world, grid, target_x, target_y, events);
        }
        DungeonTrapKind::Snare => {
            effects::add_effect_to_entity(
                world,
                victim,
                EffectType::Rooted,
                DUNGEON_SNARE_ROOT_DURATION,
            );
        }
        DungeonTrapKind::Alarm => {
            crate::systems::ai::wake_enemies_in_radius(
                world,
                (target_x, target_y),
                DUNGEON_ALARM_WAKE_RADIUS,
            );
        }
    }

    events.push(GameEvent::DungeonTrapTriggered {
        kind,
        victim,
        position: (target_x, target_y),
        damage,
    });

    // Triggered traps are consumed.
    let _ = world.despawn(trap_entity);
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

/// Place a fire trap at the target location
pub fn apply_place_fire_trap(
    world: &mut World,
    placer: Entity,
    target_x: i32,
    target_y: i32,
    events: &mut EventQueue,
) -> ActionResult {
    use crate::components::PlacedFireTrap;
    use crate::constants::FIRE_TRAP_BURST_DAMAGE;

    // Spawn the fire trap entity with pressure plate sprite
    // Fire animation is rendered as overlay in rendering.rs (like burning entities)
    let pos = Position::new(target_x, target_y);
    let trap = world.spawn((
        pos,
        VisualPosition::from_position(&pos),
        // Base sprite (pressure plate)
        Sprite::from_ref(tile_ids::PRESSURE_PLATE),
        // Trap data - tracks owner and damage
        PlacedFireTrap {
            owner: placer,
            burst_damage: FIRE_TRAP_BURST_DAMAGE,
        },
    ));

    events.push(GameEvent::FireTrapPlaced {
        trap,
        placer,
        position: (target_x, target_y),
    });

    ActionResult::Completed
}

/// Ranger ability: Place a snare trap that roots enemies
pub fn apply_place_snare_trap(
    world: &mut World,
    placer: Entity,
    target_x: i32,
    target_y: i32,
    events: &mut EventQueue,
) -> ActionResult {
    use crate::constants::SNARE_TRAP_ROOT_DURATION;

    // Spawn the trap entity
    let pos = Position::new(target_x, target_y);
    let trap = world.spawn((
        pos,
        VisualPosition::from_position(&pos),
        Sprite::from_ref(tile_ids::PRESSURE_PLATE),
        PlacedTrap {
            owner: placer,
            trap_type: TrapType::Snare { root_duration: SNARE_TRAP_ROOT_DURATION },
        },
    ));

    let _ = trap;

    events.push(GameEvent::AbilityActivated {
        entity: placer,
        ability: AbilityType::SnareTrap,
    });

    ActionResult::Completed
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::components::{Attackable, DungeonTrapKind, Health, StatusEffects};
    use crate::grid::Grid;
    use crate::tile::{Tile, TileType};

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
            crate::spawning::spawn_dungeon_trap(&mut world, 3, 3, DungeonTrapKind::Spike);

        check_dungeon_trap_trigger(&mut world, &grid, victim, 3, 3, &mut events);

        let hp = world.get::<&Health>(victim).expect("victim alive").current;
        assert_eq!(hp, 30 - crate::constants::DUNGEON_SPIKE_TRAP_DAMAGE);
        assert!(!world.contains(trap), "triggered traps are consumed");
        assert!(events.drain().any(|e| matches!(
            e,
            GameEvent::DungeonTrapTriggered { kind: DungeonTrapKind::Spike, .. }
        )));
    }

    #[test]
    fn test_snare_trap_roots_victim() {
        let mut world = hecs::World::new();
        let grid = make_grid(7, 7);
        let mut events = EventQueue::new();

        let victim = world.spawn((Position::new(2, 2), StatusEffects::new()));
        let trap =
            crate::spawning::spawn_dungeon_trap(&mut world, 2, 2, DungeonTrapKind::Snare);

        check_dungeon_trap_trigger(&mut world, &grid, victim, 2, 2, &mut events);

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
        check_dungeon_trap_trigger(&mut world, &grid, victim, 1, 1, &mut events);
        assert_eq!(events.drain().count(), 0);
    }
}
