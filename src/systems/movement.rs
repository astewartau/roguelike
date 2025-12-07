//! Player movement system.

use crate::actions::{Action, ActionResult};
use crate::events::EventQueue;
use crate::grid::Grid;
use hecs::{Entity, World};

/// Result of a player move attempt
pub enum MoveResult {
    Moved,
    OpenedChest(Entity),
    Attacked(Entity),
    Blocked,
}

/// Handle player movement, door interaction, chest interaction, and combat.
/// Uses the unified Action system for execution-time validation.
pub fn player_move(
    world: &mut World,
    grid: &Grid,
    player_entity: Entity,
    dx: i32,
    dy: i32,
    events: &mut EventQueue,
) -> MoveResult {
    let action = Action::Move { dx, dy };
    let result = action.execute(world, grid, player_entity, events);

    // Convert ActionResult to MoveResult for backwards compatibility
    match result {
        ActionResult::Moved => MoveResult::Moved,
        ActionResult::Attacked(entity) => MoveResult::Attacked(entity),
        ActionResult::OpenedDoor(_) => MoveResult::Moved,
        ActionResult::OpenedChest(entity) => MoveResult::OpenedChest(entity),
        ActionResult::Blocked | ActionResult::Invalid => MoveResult::Blocked,
    }
}
