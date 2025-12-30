//! Player input interpretation and intent processing.
//!
//! Converts raw input into PlayerIntent, validates targeting,
//! and provides intent-to-action conversion. This keeps game logic
//! out of main.rs and in proper ECS systems.

use hecs::{Entity, World};

use crate::components::{ActionType, BlocksMovement, Equipment, EquippedWeapon, ItemType, Position};
use crate::grid::Grid;
use crate::input::TargetingMode;

/// High-level player intent derived from input.
/// This represents what the player wants to do, before validation.
#[derive(Debug, Clone, PartialEq)]
pub enum PlayerIntent {
    /// No action this frame
    #[allow(dead_code)] // Default/fallback case
    None,
    /// Wait in place (skip turn)
    Wait,
    /// Move in a direction
    Move { dx: i32, dy: i32 },
    /// Force attack in a direction (Shift+move)
    AttackDirection { dx: i32, dy: i32 },
    /// Shoot equipped ranged weapon at target
    ShootRanged { target_x: i32, target_y: i32 },
    /// Use a targeted ability (blink, fireball)
    UseTargetedAbility {
        item_type: ItemType,
        item_index: usize,
        target_x: i32,
        target_y: i32,
    },
    /// Start taming an animal (Druid ability)
    StartTaming { target: Entity },
}

/// Result of validating a targeting action
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TargetingValidation {
    /// Target is valid
    Valid,
    /// Target is out of range
    OutOfRange,
    /// Target terrain is not walkable
    BlockedTerrain,
    /// Target is blocked by an entity
    BlockedByEntity,
    /// Item type doesn't support targeting
    InvalidItemType,
}

/// Validate a targeting action (range, walkability, etc.)
///
/// Returns `Valid` if the target is acceptable for the given targeting mode.
pub fn validate_targeting(
    world: &World,
    grid: &Grid,
    player_pos: (i32, i32),
    target_x: i32,
    target_y: i32,
    targeting: &TargetingMode,
) -> TargetingValidation {
    // Check range (Chebyshev distance for better diagonal targeting)
    let distance = (target_x - player_pos.0)
        .abs()
        .max((target_y - player_pos.1).abs());
    if distance > targeting.max_range {
        return TargetingValidation::OutOfRange;
    }

    // Item-specific validation
    match targeting.item_type {
        ItemType::ScrollOfBlink => {
            // Blink requires walkable, unblocked destination
            let walkable = grid
                .get(target_x, target_y)
                .map(|t| t.tile_type.is_walkable())
                .unwrap_or(false);
            if !walkable {
                return TargetingValidation::BlockedTerrain;
            }

            // Check no entity blocks this position
            let blocked = world
                .query::<(&Position, &BlocksMovement)>()
                .iter()
                .any(|(_, (pos, _))| pos.x == target_x && pos.y == target_y);
            if blocked {
                return TargetingValidation::BlockedByEntity;
            }

            TargetingValidation::Valid
        }
        ItemType::ScrollOfFireball => {
            // Fireball can target anywhere in range
            TargetingValidation::Valid
        }
        // Throwable potions can target anywhere in range
        ItemType::HealthPotion
        | ItemType::RegenerationPotion
        | ItemType::StrengthPotion
        | ItemType::ConfusionPotion => TargetingValidation::Valid,
        // Fire trap requires walkable, unblocked destination (adjacent only)
        ItemType::FireTrap => {
            let walkable = grid
                .get(target_x, target_y)
                .map(|t| t.tile_type.is_walkable())
                .unwrap_or(false);
            if !walkable {
                return TargetingValidation::BlockedTerrain;
            }

            // Check no entity blocks this position
            let blocked = world
                .query::<(&Position, &BlocksMovement)>()
                .iter()
                .any(|(_, (pos, _))| pos.x == target_x && pos.y == target_y);
            if blocked {
                return TargetingValidation::BlockedByEntity;
            }

            TargetingValidation::Valid
        }
        _ => TargetingValidation::InvalidItemType,
    }
}

/// Convert a PlayerIntent to an ActionType.
///
/// Returns `None` if the intent doesn't map to an action (e.g., `PlayerIntent::None`)
/// or if required validation fails.
pub fn intent_to_action(
    world: &World,
    grid: &Grid,
    player_entity: Entity,
    intent: &PlayerIntent,
) -> Option<ActionType> {
    match intent {
        PlayerIntent::None => None,

        PlayerIntent::Wait => Some(ActionType::Wait),

        PlayerIntent::Move { dx, dy } => {
            // Use action_dispatch for full movement logic
            // (handles attacks, doors, chests, etc.)
            Some(crate::systems::action_dispatch::determine_action_type(
                world,
                grid,
                player_entity,
                *dx,
                *dy,
            ))
        }

        PlayerIntent::AttackDirection { dx, dy } => {
            Some(ActionType::AttackDirection { dx: *dx, dy: *dy })
        }

        PlayerIntent::ShootRanged { target_x, target_y } => {
            // Check if player has a bow equipped
            if !has_ranged_equipped(world, player_entity) {
                return None;
            }
            Some(ActionType::ShootBow {
                target_x: *target_x,
                target_y: *target_y,
            })
        }

        PlayerIntent::UseTargetedAbility {
            item_type,
            item_index: _,
            target_x,
            target_y,
        } => {
            match item_type {
                ItemType::ScrollOfBlink => Some(ActionType::Blink {
                    target_x: *target_x,
                    target_y: *target_y,
                }),
                ItemType::ScrollOfFireball => Some(ActionType::CastFireball {
                    target_x: *target_x,
                    target_y: *target_y,
                }),
                // Throwable potions
                ItemType::HealthPotion
                | ItemType::RegenerationPotion
                | ItemType::StrengthPotion
                | ItemType::ConfusionPotion => Some(ActionType::ThrowPotion {
                    potion_type: *item_type,
                    target_x: *target_x,
                    target_y: *target_y,
                }),
                // Fire trap placement
                ItemType::FireTrap => Some(ActionType::PlaceFireTrap {
                    target_x: *target_x,
                    target_y: *target_y,
                }),
                _ => None,
            }
        }

        PlayerIntent::StartTaming { target } => {
            Some(ActionType::StartTaming { target: *target })
        }
    }
}

/// Check if the player has a ranged weapon (bow) equipped.
pub fn has_ranged_equipped(world: &World, player_entity: Entity) -> bool {
    world
        .get::<&Equipment>(player_entity)
        .map(|e| matches!(e.weapon, Some(EquippedWeapon::Ranged(_))))
        .unwrap_or(false)
}
