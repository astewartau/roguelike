//! AI decision-making and behavior systems.
//!
//! This module handles AI state machines, perception, and action selection
//! for non-player entities.

use std::collections::HashSet;

use hecs::{Entity, World};
use rand::Rng;

use crate::components::{ActionType, Actor, AIState, BlocksMovement, BlocksVision, ChaseAI, Position};
use crate::events::{EventQueue, GameEvent};
use crate::fov::FOV;
use crate::grid::Grid;
use crate::pathfinding;
use crate::time_system::{self, ActionScheduler, GameClock};

/// Have an AI entity decide and start its next action.
pub fn decide_action(
    world: &mut World,
    grid: &Grid,
    entity: Entity,
    player_entity: Entity,
    clock: &GameClock,
    scheduler: &mut ActionScheduler,
    events: &mut EventQueue,
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

    // Determine AI action (and emit state change events)
    let action_type = determine_action(world, grid, entity, player_entity, events, rng);

    // Start the action (with energy events)
    let _ = time_system::start_action_with_events(world, entity, action_type, clock, scheduler, Some(events));
}

/// Determine what action an AI entity should take based on its state and perception.
fn determine_action(
    world: &mut World,
    grid: &Grid,
    entity: Entity,
    player_entity: Entity,
    events: &mut EventQueue,
    rng: &mut impl Rng,
) -> ActionType {
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

    // Calculate visibility
    let can_see_player = can_see_target(world, grid, entity_pos, player_pos, sight_radius);

    // Update state machine
    let (new_state, target, new_last_known) = update_state_machine(
        current_state,
        entity_pos,
        player_pos,
        last_known,
        can_see_player,
    );

    // Emit state change event if state changed
    if new_state != current_state {
        events.push(GameEvent::AIStateChanged {
            entity,
            new_state,
        });
    }

    // Update AI state
    if let Ok(mut ai) = world.get::<&mut ChaseAI>(entity) {
        ai.state = new_state;
        ai.last_known_pos = new_last_known;
    }

    // Determine movement direction
    let (dx, dy) = calculate_movement(world, grid, entity, entity_pos, target, rng);

    // If no movement, wait
    if dx == 0 && dy == 0 {
        return ActionType::Wait;
    }

    // Determine action type (may convert to attack)
    time_system::determine_action_type(world, grid, entity, dx, dy)
}

/// Check if an entity can see a target position.
fn can_see_target(
    world: &World,
    grid: &Grid,
    from: (i32, i32),
    target: (i32, i32),
    sight_radius: i32,
) -> bool {
    // Collect vision-blocking positions
    let vision_blocking: HashSet<(i32, i32)> = world
        .query::<(&Position, &BlocksVision)>()
        .iter()
        .map(|(_, (pos, _))| (pos.x, pos.y))
        .collect();

    // Calculate FOV from entity position
    let visible_tiles = FOV::calculate(
        grid,
        from.0,
        from.1,
        sight_radius,
        Some(|x, y| vision_blocking.contains(&(x, y))),
    );

    visible_tiles.contains(&target)
}

/// Update the AI state machine based on perception.
fn update_state_machine(
    current_state: AIState,
    entity_pos: (i32, i32),
    player_pos: (i32, i32),
    last_known: Option<(i32, i32)>,
    can_see_player: bool,
) -> (AIState, Option<(i32, i32)>, Option<(i32, i32)>) {
    match current_state {
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
                // Reached last known position, go idle
                (AIState::Idle, None, None)
            } else {
                (AIState::Investigating, last_known, last_known)
            }
        }
    }
}

/// Calculate the movement direction for an AI entity.
fn calculate_movement(
    world: &World,
    grid: &Grid,
    entity: Entity,
    entity_pos: (i32, i32),
    target: Option<(i32, i32)>,
    rng: &mut impl Rng,
) -> (i32, i32) {
    if let Some((tx, ty)) = target {
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
        random_wander(grid, entity_pos, rng)
    }
}

/// Pick a random adjacent walkable tile for wandering.
fn random_wander(grid: &Grid, pos: (i32, i32), rng: &mut impl Rng) -> (i32, i32) {
    let dirs = [(0, 1), (0, -1), (1, 0), (-1, 0)];
    let (dx, dy) = dirs[rng.gen_range(0..4)];
    let target_x = pos.0 + dx;
    let target_y = pos.1 + dy;

    if grid
        .get(target_x, target_y)
        .map(|t| t.tile_type.is_walkable())
        .unwrap_or(false)
    {
        (dx, dy)
    } else {
        (0, 0)
    }
}
