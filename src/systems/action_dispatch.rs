//! Action type determination and duration calculation.
//!
//! Converts input into action types and calculates action durations.
//! Extracted from time_system.rs to separate action semantics from time management.

use crate::components::{ActionType, BlocksMovement, Container, Door, FriendlyNPC, Position, Attackable};
use crate::constants::*;
use crate::events::StairDirection;
use crate::grid::Grid;
use crate::tile::TileType;
use hecs::{Entity, World};

// =============================================================================
// ACTION DURATION CALCULATION
// =============================================================================

/// Calculate the duration of an action for a given actor's speed.
pub fn calculate_action_duration(action_type: &ActionType, speed: f32) -> f32 {
    let base_duration = match action_type {
        ActionType::Move { is_diagonal, .. } => {
            let base = ACTION_WALK_DURATION;
            if *is_diagonal {
                base * DIAGONAL_MOVEMENT_MULTIPLIER
            } else {
                base
            }
        }
        ActionType::Attack { .. } => ACTION_ATTACK_DURATION,
        ActionType::AttackDirection { .. } => ACTION_ATTACK_DURATION,
        ActionType::OpenDoor { .. } => ACTION_DOOR_DURATION,
        ActionType::OpenChest { .. } => ACTION_CHEST_DURATION,
        ActionType::Wait => ACTION_WAIT_DURATION,
        ActionType::ShootBow { .. } => ACTION_SHOOT_DURATION,
        ActionType::UseStairs { .. } => ACTION_WALK_DURATION, // Same as walking
        ActionType::TalkTo { .. } => ACTION_DOOR_DURATION, // Quick interaction
        ActionType::ThrowPotion { .. } => ACTION_SHOOT_DURATION, // Same as shooting
        ActionType::Blink { .. } => ACTION_WAIT_DURATION, // Instant teleport
        ActionType::CastFireball { .. } => ACTION_SHOOT_DURATION, // Same as shooting
        ActionType::EquipWeapon { .. } => 0.0, // Instant (free action)
        ActionType::UnequipWeapon => 0.0, // Instant (free action)
    };

    // Speed modifies duration: higher speed = shorter duration
    base_duration / speed
}

// =============================================================================
// ACTION TYPE DETERMINATION
// =============================================================================

/// Determine what action type results from a movement input.
/// Checks for targets at the destination: NPCs, enemies, doors, chests, stairs.
pub fn determine_action_type(
    world: &World,
    grid: &Grid,
    entity: Entity,
    dx: i32,
    dy: i32,
) -> ActionType {
    let is_diagonal = dx != 0 && dy != 0;

    // Get entity position
    let pos = match world.get::<&Position>(entity) {
        Ok(p) => p,
        Err(_) => return ActionType::Wait,
    };

    let target_x = pos.x + dx;
    let target_y = pos.y + dy;

    // Check for friendly NPC at target (must check before attackable!)
    for (id, (npc_pos, _)) in world.query::<(&Position, &FriendlyNPC)>().iter() {
        if npc_pos.x == target_x && npc_pos.y == target_y {
            return ActionType::TalkTo { npc: id };
        }
    }

    // Check for attackable entity at target
    for (id, (enemy_pos, _)) in world.query::<(&Position, &Attackable)>().iter() {
        if id != entity && enemy_pos.x == target_x && enemy_pos.y == target_y {
            return ActionType::Attack { target: id };
        }
    }

    // Check for closed door at target
    for (id, (door_pos, door)) in world.query::<(&Position, &Door)>().iter() {
        if door_pos.x == target_x && door_pos.y == target_y && !door.is_open {
            return ActionType::OpenDoor { door: id };
        }
    }

    // Check for chest at target (closed, or open with items still inside)
    for (id, (chest_pos, container, _)) in
        world.query::<(&Position, &Container, &BlocksMovement)>().iter()
    {
        if chest_pos.x == target_x && chest_pos.y == target_y {
            // Interact if closed OR if open but still has items
            if !container.is_open || !container.is_empty() {
                return ActionType::OpenChest { chest: id };
            }
        }
    }

    // Check for stairs at target
    if let Some(tile) = grid.get(target_x, target_y) {
        match tile.tile_type {
            TileType::StairsDown => {
                return ActionType::UseStairs {
                    x: target_x,
                    y: target_y,
                    direction: StairDirection::Down,
                };
            }
            TileType::StairsUp => {
                return ActionType::UseStairs {
                    x: target_x,
                    y: target_y,
                    direction: StairDirection::Up,
                };
            }
            _ => {}
        }
    }

    // Default to move
    ActionType::Move { dx, dy, is_diagonal }
}
