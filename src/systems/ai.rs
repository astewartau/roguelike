//! AI decision-making and behavior systems.
//!
//! This module handles AI state machines, perception, and action selection
//! for non-player entities.

use std::collections::HashSet;

use hecs::{Entity, World};
use rand::Rng;

use crate::components::{ActionType, Actor, AIState, ChaseAI, EffectType, Equipment, Position};
use crate::constants::AI_ACTIVE_RADIUS;
use crate::events::{EventQueue, GameEvent};
use crate::grid::Grid;
use crate::pathfinding::{self, BresenhamLineIter};
use crate::queries;
use crate::systems::action_dispatch;
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
    puffin::profile_function!();

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

    // Performance optimization: Skip AI for distant enemies
    // Get positions early to check distance before expensive operations
    let entity_pos = world.get::<&Position>(entity).ok().map(|p| (p.x, p.y));
    let player_pos = world.get::<&Position>(player_entity).ok().map(|p| (p.x, p.y));

    if let (Some(epos), Some(ppos)) = (entity_pos, player_pos) {
        let distance = (epos.0 - ppos.0).abs() + (epos.1 - ppos.1).abs();
        if distance > AI_ACTIVE_RADIUS {
            // Too far from player - schedule a wakeup check later and skip this turn
            let wakeup_delay = 0.5; // Check again in half a second of game time
            scheduler.schedule(entity, clock.time + wakeup_delay);
            return;
        }
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

    // Check for status effects that override normal AI behavior
    let is_confused = queries::has_status_effect(world, entity, EffectType::Confused);
    let is_feared = queries::has_status_effect(world, entity, EffectType::Feared);

    // Confused: move randomly, ignore player entirely
    if is_confused {
        let movement_blocking = queries::get_blocking_positions(world, Some(entity));

        let (dx, dy) = random_wander(grid, entity_pos, &movement_blocking, rng);
        if dx == 0 && dy == 0 {
            return ActionType::Wait;
        }
        return action_dispatch::determine_action_type(world, grid, entity, dx, dy);
    }

    // Feared: flee from player
    if is_feared {
        let movement_blocking = queries::get_blocking_positions(world, Some(entity));

        let (dx, dy) = flee_from_target(grid, entity_pos, player_pos, &movement_blocking, rng);
        if dx == 0 && dy == 0 {
            return ActionType::Wait;
        }
        return action_dispatch::determine_action_type(world, grid, entity, dx, dy);
    }

    // Get AI state, sight radius, and ranged parameters
    let (sight_radius, current_state, last_known, ranged_min, ranged_max) =
        match world.get::<&ChaseAI>(entity) {
            Ok(ai) => (
                ai.sight_radius,
                ai.state,
                ai.last_known_pos,
                ai.ranged_min,
                ai.ranged_max,
            ),
            Err(_) => return ActionType::Wait,
        };

    // Check if entity has a bow equipped (for ranged attacks)
    let has_ranged_weapon = world
        .get::<&Equipment>(entity)
        .map(|e| e.has_bow())
        .unwrap_or(false);

    // Calculate visibility (checks for invisibility effect on player)
    let can_see_player = can_see_target(world, grid, entity_pos, player_pos, sight_radius, Some(player_entity));

    // Calculate distance to player (Chebyshev distance for ranged check)
    let distance = (entity_pos.0 - player_pos.0)
        .abs()
        .max((entity_pos.1 - player_pos.1).abs());

    // Always update last_known_pos when we can see the player.
    // This ensures we have the correct position even if we return early for ranged attacks.
    // Without this, archers who keep shooting would have a stale last_known_pos from when
    // they first spotted the player, causing them to investigate the wrong location.
    if can_see_player {
        if let Ok(mut ai) = world.get::<&mut ChaseAI>(entity) {
            ai.last_known_pos = Some(player_pos);
        }
    }

    // Check for ranged attack opportunity
    if can_see_player && has_ranged_weapon && ranged_max > 0 {
        // In range for ranged attack?
        if distance >= ranged_min && distance <= ranged_max {
            // Check line of sight for projectile (no blocking entities in the way)
            if has_clear_shot(world, entity_pos, player_pos) {
                return ActionType::ShootBow {
                    target_x: player_pos.0,
                    target_y: player_pos.1,
                };
            }
        }
    }

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
    action_dispatch::determine_action_type(world, grid, entity, dx, dy)
}

/// Check if there's a clear line of sight for a projectile (no blocking entities)
fn has_clear_shot(world: &World, from: (i32, i32), to: (i32, i32)) -> bool {
    let blocking = queries::get_blocking_positions(world, None);

    for (x, y) in BresenhamLineIter::new(from.0, from.1, to.0, to.1) {
        // Skip the target position (we want to shoot AT the target)
        if (x, y) == to {
            continue;
        }
        if blocking.contains(&(x, y)) {
            return false;
        }
    }

    true
}

/// Check if an entity can see a target position.
/// Uses proximity-based detection: target must be within sight_radius AND have clear line of sight.
/// If target_entity is provided, also checks if the target is invisible.
fn can_see_target(
    world: &World,
    grid: &Grid,
    from: (i32, i32),
    target: (i32, i32),
    sight_radius: i32,
    target_entity: Option<Entity>,
) -> bool {
    // Check if target entity is invisible
    if let Some(entity) = target_entity {
        if queries::has_status_effect(world, entity, EffectType::Invisible) {
            return false;
        }
    }

    // Check distance (Chebyshev distance for more natural "sight" radius)
    let dx = (from.0 - target.0).abs();
    let dy = (from.1 - target.1).abs();
    let distance = dx.max(dy);
    if distance > sight_radius {
        return false;
    }

    // Collect vision-blocking positions
    let vision_blocking = queries::get_vision_blocking_positions(world);

    // Check line of sight using Bresenham's algorithm
    has_line_of_sight(grid, &vision_blocking, from.0, from.1, target.0, target.1)
}

/// Check if there's a clear line of sight between two points.
/// Uses Bresenham's line algorithm to check for blocking tiles.
fn has_line_of_sight(
    grid: &Grid,
    blocking_entities: &std::collections::HashSet<(i32, i32)>,
    x0: i32,
    y0: i32,
    x1: i32,
    y1: i32,
) -> bool {
    let dx = (x1 - x0).abs();
    let dy = (y1 - y0).abs();
    let sx = if x0 < x1 { 1 } else { -1 };
    let sy = if y0 < y1 { 1 } else { -1 };
    let mut err = dx - dy;

    let mut x = x0;
    let mut y = y0;

    while x != x1 || y != y1 {
        let e2 = 2 * err;
        if e2 > -dy {
            err -= dy;
            x += sx;
        }
        if e2 < dx {
            err += dx;
            y += sy;
        }

        // Don't check the destination tile itself
        if x == x1 && y == y1 {
            break;
        }

        // Check if this tile blocks vision
        if let Some(tile) = grid.get(x, y) {
            if tile.tile_type.blocks_vision() {
                return false;
            }
        }

        // Check if an entity blocks vision here
        if blocking_entities.contains(&(x, y)) {
            return false;
        }
    }

    true
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
    // Collect movement-blocking positions (used by both pathfinding and wandering)
    let movement_blocking = queries::get_blocking_positions(world, Some(entity));

    if let Some((tx, ty)) = target {
        // Pathfind to target
        pathfinding::next_step_toward(grid, entity_pos, (tx, ty), &movement_blocking)
            .map(|(nx, ny)| (nx - entity_pos.0, ny - entity_pos.1))
            .unwrap_or((0, 0))
    } else {
        // Idle: wander randomly
        random_wander(grid, entity_pos, &movement_blocking, rng)
    }
}

/// Pick a random adjacent walkable tile for wandering.
fn random_wander(
    grid: &Grid,
    pos: (i32, i32),
    blocked: &HashSet<(i32, i32)>,
    rng: &mut impl Rng,
) -> (i32, i32) {
    let dirs = [(0, 1), (0, -1), (1, 0), (-1, 0)];
    let (dx, dy) = dirs[rng.gen_range(0..4)];
    let target = (pos.0 + dx, pos.1 + dy);

    if grid.is_walkable(target.0, target.1) && !blocked.contains(&target) {
        (dx, dy)
    } else {
        (0, 0)
    }
}

/// Flee from a target position - move in the opposite direction.
/// Falls back to random movement if direct flee path is blocked.
fn flee_from_target(
    grid: &Grid,
    pos: (i32, i32),
    target: (i32, i32),
    blocked: &HashSet<(i32, i32)>,
    rng: &mut impl Rng,
) -> (i32, i32) {
    // Calculate direction away from target
    let flee_dx = (pos.0 - target.0).signum();
    let flee_dy = (pos.1 - target.1).signum();

    // Try to move directly away
    if flee_dx != 0 || flee_dy != 0 {
        let nx = pos.0 + flee_dx;
        let ny = pos.1 + flee_dy;
        if grid.is_walkable(nx, ny) && !blocked.contains(&(nx, ny)) {
            return (flee_dx, flee_dy);
        }

        // Try fleeing in just X direction
        if flee_dx != 0 {
            let nx = pos.0 + flee_dx;
            let ny = pos.1;
            if grid.is_walkable(nx, ny) && !blocked.contains(&(nx, ny)) {
                return (flee_dx, 0);
            }
        }

        // Try fleeing in just Y direction
        if flee_dy != 0 {
            let nx = pos.0;
            let ny = pos.1 + flee_dy;
            if grid.is_walkable(nx, ny) && !blocked.contains(&(nx, ny)) {
                return (0, flee_dy);
            }
        }
    }

    // Fallback to random movement if can't flee directly
    random_wander(grid, pos, blocked, rng)
}
