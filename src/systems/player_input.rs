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
    /// Interact with something in a direction (Ctrl+move)
    InteractDirection { dx: i32, dy: i32 },
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
    /// Start draining life from a target (Necromancer channeled ability)
    StartLifeDrain { target: Entity },
    /// Ranger: Tumble to target position with invulnerability
    Tumble { target_x: i32, target_y: i32 },
    /// Ranger: Place snare trap at target position
    PlaceSnareTrap { target_x: i32, target_y: i32 },
    /// Ranger: Shoot crippling shot that slows target
    ShootCripplingShot { target_x: i32, target_y: i32 },
    /// Cast a learned (studied) spell at a target position (Blink/Fireball;
    /// untargeted learned casts are activated directly from the hotbar)
    CastLearnedSpell {
        ability: crate::components::AbilityType,
        target_x: i32,
        target_y: i32,
    },
    /// Necromancer: start channeling Raise Dead on a bones container
    StartRaiseDead { target: Entity },
    /// Necromancer: swap places with one of your raised skeletons
    Sacrifice { skeleton: Entity },
    /// Necromancer: detonate a corpse
    CorpseExplosion { corpse: Entity },
    /// Druid: root hostiles around a tile
    Entangle { target_x: i32, target_y: i32 },
    CallRain { target_x: i32, target_y: i32 },
    /// Push the pushable object on the adjacent tile (dx, dy) one tile on.
    /// Ctrl+direction resolves to this on its own (see
    /// `actions::resolve_interact_direction`); the explicit intent is for the
    /// context menu.
    #[allow(dead_code)] // Reserved for the upcoming right-click context menu
    Push { dx: i32, dy: i32 },
    /// Close an open door (context menu; Ctrl+direction also resolves to it).
    #[allow(dead_code)] // Reserved for the upcoming right-click context menu
    CloseDoor { door: Entity },
    /// Fighter: Shield Bash the creature or pushable on an adjacent tile.
    ShieldBash { target_x: i32, target_y: i32 },
    /// Necromancer: Grave Bolt at a tile in range and line of sight.
    GraveBolt { target_x: i32, target_y: i32 },
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
/// or if required validation fails. A kit ability still on cooldown is
/// refused here, whatever route its intent took (hotbar click, context menu):
/// abilities are gated by their cooldown alone, so this gate must hold.
pub fn intent_to_action(
    world: &World,
    grid: &Grid,
    player_entity: Entity,
    intent: &PlayerIntent,
) -> Option<ActionType> {
    let action = intent_to_action_unchecked(world, grid, player_entity, intent)?;
    if let Some(ability) = crate::systems::actions::kit_ability_for_action(&action) {
        if !crate::systems::actions::kit_ability_ready(world, player_entity, ability) {
            return None;
        }
    }
    Some(action)
}

fn intent_to_action_unchecked(
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

        PlayerIntent::InteractDirection { dx, dy } => Some(
            crate::systems::actions::resolve_interact_direction(world, player_entity, *dx, *dy),
        ),

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
                // Throwable potions (and the water flask, which splash-douses)
                ItemType::HealthPotion
                | ItemType::RegenerationPotion
                | ItemType::StrengthPotion
                | ItemType::ConfusionPotion
                | ItemType::WaterFlaskFull => Some(ActionType::ThrowPotion {
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

        PlayerIntent::StartLifeDrain { target } => {
            Some(ActionType::StartLifeDrain { target: *target })
        }

        PlayerIntent::Tumble { target_x, target_y } => Some(ActionType::Tumble {
            target_x: *target_x,
            target_y: *target_y,
        }),

        PlayerIntent::PlaceSnareTrap { target_x, target_y } => Some(ActionType::PlaceSnareTrap {
            target_x: *target_x,
            target_y: *target_y,
        }),

        PlayerIntent::ShootCripplingShot { target_x, target_y } => Some(ActionType::ShootCripplingShot {
            target_x: *target_x,
            target_y: *target_y,
        }),

        PlayerIntent::CastLearnedSpell { ability, target_x, target_y } => {
            Some(ActionType::CastLearnedSpell {
                ability: *ability,
                target_x: *target_x,
                target_y: *target_y,
            })
        }

        PlayerIntent::StartRaiseDead { target } => {
            Some(ActionType::StartRaiseDead { target: *target })
        }

        PlayerIntent::Sacrifice { skeleton } => {
            Some(ActionType::Sacrifice { skeleton: *skeleton })
        }

        PlayerIntent::CorpseExplosion { corpse } => {
            Some(ActionType::CorpseExplosion { corpse: *corpse })
        }

        PlayerIntent::Entangle { target_x, target_y } => Some(ActionType::Entangle {
            target_x: *target_x,
            target_y: *target_y,
        }),

        PlayerIntent::CallRain { target_x, target_y } => Some(ActionType::CallRain {
            target_x: *target_x,
            target_y: *target_y,
        }),

        PlayerIntent::Push { dx, dy } => Some(ActionType::Push { dx: *dx, dy: *dy }),

        // An open door next to the player. Something standing in the doorway
        // is reported by the action itself ("the door won't close").
        PlayerIntent::CloseDoor { door } => {
            let open = world
                .get::<&crate::components::Door>(*door)
                .map(|d| d.is_open)
                .unwrap_or(false);
            let adjacent = match (
                crate::queries::get_entity_position(world, player_entity),
                crate::queries::get_entity_position(world, *door),
            ) {
                (Some(a), Some(b)) => (a.0 - b.0).abs().max((a.1 - b.1).abs()) == 1,
                _ => false,
            };
            (open && adjacent).then_some(ActionType::CloseDoor { door: *door })
        }

        PlayerIntent::ShieldBash { target_x, target_y } => {
            crate::systems::actions::can_shield_bash(world, player_entity, (*target_x, *target_y))
                .then_some(ActionType::ShieldBash { target_x: *target_x, target_y: *target_y })
        }

        PlayerIntent::GraveBolt { target_x, target_y } => {
            crate::systems::actions::can_grave_bolt(
                world,
                grid,
                player_entity,
                (*target_x, *target_y),
            )
            .then_some(ActionType::GraveBolt { target_x: *target_x, target_y: *target_y })
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
