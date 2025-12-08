//! Game loop and time advancement.
//!
//! This module owns the game simulation loop, advancing time and processing
//! actions. It separates game logic from input handling and rendering.

use crate::components::{ActionType, Actor};
use crate::events::{EventQueue, GameEvent};
use crate::grid::Grid;
use crate::systems;
use crate::time_system::{self, ActionScheduler, GameClock};
use crate::vfx::VfxManager;
use hecs::{Entity, World};
use rand::Rng;

/// Result of attempting to start a player action
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TurnResult {
    /// Action started and completed successfully
    Started,
    /// Action was blocked (invalid target, etc.)
    Blocked,
    /// Player can't act (busy or no energy)
    NotReady,
}

/// Full result of executing a player turn, including any events that need handling
pub struct TurnExecutionResult {
    pub turn_result: TurnResult,
    pub floor_transition: Option<StairDirection>,
}

/// Start a player action and advance time until player can act again.
///
/// This is the main entry point for player turns. It:
/// 1. Determines the action type from movement input
/// 2. Starts the action (scheduling it)
/// 3. Advances time, completing actions and running AI
/// 4. Returns when the player can act again
pub fn execute_player_turn(
    world: &mut World,
    grid: &Grid,
    player_entity: Entity,
    dx: i32,
    dy: i32,
    clock: &mut GameClock,
    scheduler: &mut ActionScheduler,
    events: &mut EventQueue,
    vfx: &mut VfxManager,
    ui_state: &mut GameUiState,
) -> TurnExecutionResult {
    // Check if player can act
    let can_act = world
        .get::<&Actor>(player_entity)
        .map(|a| a.can_act())
        .unwrap_or(false);

    if !can_act {
        return TurnExecutionResult {
            turn_result: TurnResult::NotReady,
            floor_transition: None,
        };
    }

    // Determine what action the player is attempting
    let action_type = time_system::determine_action_type(world, grid, player_entity, dx, dy);

    // Try to start the action
    if time_system::start_action(world, player_entity, action_type, clock, scheduler).is_err() {
        return TurnExecutionResult {
            turn_result: TurnResult::Blocked,
            floor_transition: None,
        };
    }

    // Advance time until player can act again
    let mut rng = rand::thread_rng();
    advance_until_player_ready(world, grid, player_entity, clock, scheduler, events, &mut rng);

    // Process events (VFX, UI state, world state changes)
    let event_result = process_events(events, world, vfx, ui_state);

    TurnExecutionResult {
        turn_result: TurnResult::Started,
        floor_transition: event_result.floor_transition,
    }
}

/// Get the action type that would result from a movement input.
/// Useful for UI feedback (showing what will happen).
pub fn peek_action_type(
    world: &World,
    grid: &Grid,
    player_entity: Entity,
    dx: i32,
    dy: i32,
) -> ActionType {
    time_system::determine_action_type(world, grid, player_entity, dx, dy)
}

/// Advance game time until the player can act again.
///
/// This processes the simulation:
/// - Completes scheduled actions
/// - Runs AI for non-player entities
/// - Ticks regeneration
/// - Updates projectiles
fn advance_until_player_ready(
    world: &mut World,
    grid: &Grid,
    player_entity: Entity,
    clock: &mut GameClock,
    scheduler: &mut ActionScheduler,
    events: &mut EventQueue,
    rng: &mut impl Rng,
) {
    loop {
        // Check if player can act
        let player_can_act = world
            .get::<&Actor>(player_entity)
            .map(|a| a.can_act())
            .unwrap_or(false);

        if player_can_act {
            // Final projectile update before returning
            update_projectiles_at_time(world, grid, clock.time, events);
            return;
        }

        // If player has no Actor component, shouldn't happen but bail out
        if world.get::<&Actor>(player_entity).is_err() {
            return;
        }

        // Get next completion from scheduler
        let Some((next_entity, completion_time)) = scheduler.pop_next() else {
            // No pending completions but player can't act - shouldn't happen
            return;
        };

        // Advance time to the completion
        clock.advance_to(completion_time);

        // Update projectiles at this time point
        update_projectiles_at_time(world, grid, clock.time, events);

        // Process time-based effects (HP regen, energy regen)
        time_system::tick_health_regen(world, clock.time, Some(events));
        time_system::tick_energy_regen(world, clock.time, Some(events));

        // Complete the action
        time_system::complete_action(world, grid, next_entity, events, clock.time);

        // If not player, have AI decide next action
        if next_entity != player_entity {
            systems::ai::decide_action(world, grid, next_entity, player_entity, clock, scheduler, events, rng);
        }
    }
}

/// Update projectiles based on game time (marks finished but doesn't despawn)
fn update_projectiles_at_time(
    world: &mut World,
    grid: &Grid,
    current_time: f32,
    events: &mut EventQueue,
) {
    // Update projectiles - this marks them as finished but doesn't despawn
    // Despawning happens in the render loop after visual catch-up
    systems::update_projectiles(world, grid, current_time, events);
}

/// Public wrapper for advance_until_player_ready (used by bow shooting)
pub fn advance_until_player_ready_public(
    world: &mut World,
    grid: &Grid,
    player_entity: Entity,
    clock: &mut GameClock,
    scheduler: &mut ActionScheduler,
    events: &mut EventQueue,
    rng: &mut impl Rng,
) {
    advance_until_player_ready(world, grid, player_entity, clock, scheduler, events, rng);
}

use crate::events::StairDirection;
use crate::ui::GameUiState;

/// Result of processing events, contains any floor transitions that need handling
pub struct EventProcessingResult {
    pub floor_transition: Option<StairDirection>,
}

/// Process all pending events, dispatching to appropriate handlers.
/// Returns any floor transitions that need to be handled by the caller.
pub fn process_events(
    events: &mut EventQueue,
    world: &mut World,
    vfx: &mut VfxManager,
    ui_state: &mut GameUiState,
) -> EventProcessingResult {
    let mut result = EventProcessingResult {
        floor_transition: None,
    };

    for event in events.drain() {
        // Visual effects
        vfx.handle_event(&event);

        // UI state
        ui_state.handle_event(&event);

        // World state changes in response to events
        match &event {
            GameEvent::ContainerOpened { container, .. } => {
                systems::handle_container_opened(world, *container);
            }
            GameEvent::FloorTransition { direction, .. } => {
                // Capture floor transition for handling by caller
                result.floor_transition = Some(*direction);
            }
            _ => {}
        }

        // Future: audio.handle_event(&event), etc.
    }

    result
}
