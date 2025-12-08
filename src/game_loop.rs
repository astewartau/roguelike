//! Game loop and time advancement.
//!
//! This module owns the game simulation loop, advancing time and processing
//! actions. It separates game logic from input handling and rendering.

use crate::components::{ActionType, Actor, BlocksMovement, ChaseAI, Position};
use crate::events::{EventQueue, GameEvent};
use crate::grid::Grid;
use crate::pathfinding;
use crate::systems;
use crate::time_system::{self, ActionScheduler, GameClock};
use crate::vfx::VfxManager;
use hecs::{Entity, World};
use rand::Rng;
use std::collections::HashSet;

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
) -> TurnResult {
    // Check if player can act
    let can_act = world
        .get::<&Actor>(player_entity)
        .map(|a| a.can_act())
        .unwrap_or(false);

    if !can_act {
        return TurnResult::NotReady;
    }

    // Determine what action the player is attempting
    let action_type = time_system::determine_action_type(world, grid, player_entity, dx, dy);

    // Try to start the action
    if time_system::start_action(world, player_entity, action_type, clock, scheduler).is_err() {
        return TurnResult::Blocked;
    }

    // Advance time until player can act again
    let mut rng = rand::thread_rng();
    advance_until_player_ready(world, grid, player_entity, clock, scheduler, events, &mut rng);

    // Process events (VFX, UI state, world state changes)
    process_events(events, world, vfx, ui_state);

    TurnResult::Started
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
        if let Ok(actor) = world.get::<&Actor>(player_entity) {
            if actor.can_act() {
                return;
            }
        } else {
            // Player has no Actor component (shouldn't happen)
            return;
        }

        // Get next completion from scheduler
        let Some((next_entity, completion_time)) = scheduler.pop_next() else {
            // No pending completions but player can't act - shouldn't happen
            return;
        };

        // Advance time to the completion
        clock.advance_to(completion_time);

        // Process time-based effects (HP regen, energy regen)
        time_system::tick_health_regen(world, clock.time, Some(events));
        time_system::tick_energy_regen(world, clock.time);

        // Complete the action
        time_system::complete_action(world, grid, next_entity, events);

        // If not player, have AI decide next action
        if next_entity != player_entity {
            ai_decide_action(world, grid, next_entity, player_entity, clock, scheduler, rng);
        }
    }
}

/// Have an AI entity decide and start its next action.
pub fn ai_decide_action(
    world: &mut World,
    grid: &Grid,
    entity: Entity,
    player_entity: Entity,
    clock: &GameClock,
    scheduler: &mut ActionScheduler,
    rng: &mut impl Rng,
) {
    // Check if entity has Actor component
    let (can_act, energy_regen_interval, last_regen_time) = match world.get::<&Actor>(entity) {
        Ok(a) => (a.can_act(), a.energy_regen_interval, a.last_energy_regen_time),
        Err(_) => return,
    };

    // If can't act (usually out of energy), schedule to wake up when energy regens
    if !can_act {
        if energy_regen_interval > 0.0 {
            let next_regen_time = last_regen_time + energy_regen_interval;
            if next_regen_time > clock.time {
                scheduler.schedule(entity, next_regen_time);
            } else {
                scheduler.schedule(entity, clock.time + energy_regen_interval);
            }
        }
        return;
    }

    // Check if entity has AI
    let has_ai = world.get::<&ChaseAI>(entity).is_ok();
    if !has_ai {
        return;
    }

    // Determine AI action
    let action_type = determine_ai_action(world, grid, entity, player_entity, rng);

    // Start the action
    let _ = time_system::start_action(world, entity, action_type, clock, scheduler);
}

/// Determine what action an AI entity should take.
fn determine_ai_action(
    world: &mut World,
    grid: &Grid,
    entity: Entity,
    player_entity: Entity,
    rng: &mut impl Rng,
) -> ActionType {
    use crate::components::AIState;
    use crate::fov::FOV;

    // Get entity position
    let entity_pos = match world.get::<&Position>(entity) {
        Ok(p) => (p.x, p.y),
        Err(_) => return ActionType::Wait,
    };

    // Get player position
    let player_pos = match world.get::<&Position>(player_entity) {
        Ok(p) => (p.x, p.y),
        Err(_) => return ActionType::Wait,
    };

    // Get AI state and sight radius
    let (sight_radius, current_state, last_known) = match world.get::<&ChaseAI>(entity) {
        Ok(ai) => (ai.sight_radius, ai.state, ai.last_known_pos),
        Err(_) => return ActionType::Wait,
    };

    // Collect vision-blocking positions
    let vision_blocking: HashSet<(i32, i32)> = world
        .query::<(&Position, &crate::components::BlocksVision)>()
        .iter()
        .map(|(_, (pos, _))| (pos.x, pos.y))
        .collect();

    // Calculate FOV from entity position
    let visible_tiles = FOV::calculate(
        grid,
        entity_pos.0,
        entity_pos.1,
        sight_radius,
        Some(|x, y| vision_blocking.contains(&(x, y))),
    );

    let can_see_player = visible_tiles.contains(&player_pos);

    // Determine new state and target
    let (new_state, target, new_last_known) = match current_state {
        AIState::Idle => {
            if can_see_player {
                (AIState::Chasing, Some(player_pos), Some(player_pos))
            } else {
                (AIState::Idle, None, last_known)
            }
        }
        AIState::Chasing => {
            if can_see_player {
                (AIState::Chasing, Some(player_pos), Some(player_pos))
            } else {
                (AIState::Investigating, last_known, last_known)
            }
        }
        AIState::Investigating => {
            if can_see_player {
                (AIState::Chasing, Some(player_pos), Some(player_pos))
            } else if last_known.map(|lk| lk == entity_pos).unwrap_or(true) {
                (AIState::Idle, None, None)
            } else {
                (AIState::Investigating, last_known, last_known)
            }
        }
    };

    // Update AI state
    if let Ok(mut ai) = world.get::<&mut ChaseAI>(entity) {
        ai.state = new_state;
        ai.last_known_pos = new_last_known;
    }

    // Determine movement
    let (dx, dy) = if let Some((tx, ty)) = target {
        // Collect movement-blocking positions
        let movement_blocking: HashSet<(i32, i32)> = world
            .query::<(&Position, &BlocksMovement)>()
            .iter()
            .filter(|(id, _)| *id != entity)
            .map(|(_, (pos, _))| (pos.x, pos.y))
            .collect();

        // Pathfind to target
        pathfinding::next_step_toward(grid, entity_pos, (tx, ty), &movement_blocking)
            .map(|(nx, ny)| (nx - entity_pos.0, ny - entity_pos.1))
            .unwrap_or((0, 0))
    } else {
        // Idle: wander randomly
        let dirs = [(0, 1), (0, -1), (1, 0), (-1, 0)];
        let (dx, dy) = dirs[rng.gen_range(0..4)];
        let target_x = entity_pos.0 + dx;
        let target_y = entity_pos.1 + dy;

        if grid
            .get(target_x, target_y)
            .map(|t| t.tile_type.is_walkable())
            .unwrap_or(false)
        {
            (dx, dy)
        } else {
            (0, 0)
        }
    };

    // If no movement, wait
    if dx == 0 && dy == 0 {
        return ActionType::Wait;
    }

    // Determine action type (may convert to attack)
    time_system::determine_action_type(world, grid, entity, dx, dy)
}

use crate::ui::GameUiState;

/// Process all pending events, dispatching to appropriate handlers.
pub fn process_events(
    events: &mut EventQueue,
    world: &mut World,
    vfx: &mut VfxManager,
    ui_state: &mut GameUiState,
) {
    for event in events.drain() {
        // Visual effects
        vfx.handle_event(&event);

        // UI state
        ui_state.handle_event(&event);

        // World state changes in response to events
        match &event {
            GameEvent::ChestOpened { chest, .. } => {
                systems::handle_chest_opened(world, *chest);
            }
            _ => {}
        }

        // Future: audio.handle_event(&event), etc.
    }
}
