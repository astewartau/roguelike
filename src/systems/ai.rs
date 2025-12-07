//! AI behavior systems.

use crate::actions::Action;
use crate::components::{Actor, AIState, BlocksMovement, BlocksVision, ChaseAI, Position};
use crate::events::EventQueue;
use crate::fov::FOV;
use crate::grid::Grid;
use crate::pathfinding;
use hecs::{Entity, World};
use rand::Rng;
use std::collections::HashSet;

/// Give all actors +1 energy per tick
pub fn tick_energy(world: &mut World) {
    for (_id, actor) in world.query_mut::<&mut Actor>() {
        actor.energy += 1;
    }
}

/// AI state machine: Idle (wander) -> Chasing (sees player) -> Investigating (lost sight)
/// Uses execution-time validation to prevent invalid moves like entity swaps.
pub fn ai_chase(
    world: &mut World,
    grid: &Grid,
    player_entity: Entity,
    rng: &mut impl Rng,
    events: &mut EventQueue,
) {
    // Get player position
    let player_pos = match world.get::<&Position>(player_entity) {
        Ok(p) => (p.x, p.y),
        Err(_) => return,
    };

    // Collect AI decisions first to avoid borrow conflicts
    // We store: (entity, dx, dy, new_state, last_known_pos)
    // Note: We now store deltas (dx, dy) not absolute positions
    let mut ai_decisions: Vec<(Entity, i32, i32, AIState, Option<(i32, i32)>)> = Vec::new();

    // Collect blocking positions for enemy FOV (used for planning, not execution)
    let vision_blocking: HashSet<(i32, i32)> = world
        .query::<(&Position, &BlocksVision)>()
        .iter()
        .map(|(_, (pos, _))| (pos.x, pos.y))
        .collect();

    // Collect movement blocking for pathfinding (soft check for planning)
    let movement_blocking: HashSet<(i32, i32)> = world
        .query::<(&Position, &BlocksMovement)>()
        .iter()
        .map(|(_, (pos, _))| (pos.x, pos.y))
        .collect();

    for (id, (pos, actor, chase)) in world.query::<(&Position, &Actor, &ChaseAI)>().iter() {
        if actor.energy < actor.speed {
            continue; // Not enough energy to act
        }

        // Calculate FOV from this enemy's position
        let visible_tiles: HashSet<(i32, i32)> = FOV::calculate(
            grid,
            pos.x,
            pos.y,
            chase.sight_radius,
            Some(|x: i32, y: i32| vision_blocking.contains(&(x, y))),
        )
        .into_iter()
        .collect();

        let can_see_player = visible_tiles.contains(&player_pos);

        // Determine new state and target
        let (new_state, target, last_known) = match chase.state {
            AIState::Idle => {
                if can_see_player {
                    (AIState::Chasing, Some(player_pos), Some(player_pos))
                } else {
                    (AIState::Idle, None, None)
                }
            }
            AIState::Chasing => {
                if can_see_player {
                    (AIState::Chasing, Some(player_pos), Some(player_pos))
                } else {
                    (
                        AIState::Investigating,
                        chase.last_known_pos,
                        chase.last_known_pos,
                    )
                }
            }
            AIState::Investigating => {
                if can_see_player {
                    (AIState::Chasing, Some(player_pos), Some(player_pos))
                } else if let Some(last_pos) = chase.last_known_pos {
                    if pos.x == last_pos.0 && pos.y == last_pos.1 {
                        (AIState::Idle, None, None)
                    } else {
                        (AIState::Investigating, Some(last_pos), Some(last_pos))
                    }
                } else {
                    (AIState::Idle, None, None)
                }
            }
        };

        // Determine intended movement (as delta, not absolute)
        let (dx, dy) = if let Some((tx, ty)) = target {
            // Move toward target using A* pathfinding
            if let Some((nx, ny)) =
                pathfinding::next_step_toward(grid, (pos.x, pos.y), (tx, ty), &movement_blocking)
            {
                (nx - pos.x, ny - pos.y)
            } else {
                (0, 0) // Can't pathfind, stay in place
            }
        } else {
            // Idle: wander randomly
            let dirs = [(0, 1), (0, -1), (1, 0), (-1, 0)];
            let (dx, dy) = dirs[rng.gen_range(0..4)];
            let new_x = pos.x + dx;
            let new_y = pos.y + dy;

            // Soft check for planning - actual validation happens at execution
            if let Some(tile) = grid.get(new_x, new_y) {
                if tile.tile_type.is_walkable() {
                    (dx, dy)
                } else {
                    (0, 0)
                }
            } else {
                (0, 0)
            }
        };

        ai_decisions.push((id, dx, dy, new_state, last_known));
    }

    // Execute each action with real-time validation
    // This is the key fix: each action checks the CURRENT world state
    for (id, dx, dy, new_state, last_known) in ai_decisions {
        // Update chase state first
        if let Ok(mut chase) = world.get::<&mut ChaseAI>(id) {
            chase.state = new_state;
            chase.last_known_pos = last_known;
        }

        // Execute the move action with real-time validation
        // This will:
        // - Convert to attack if there's an attackable entity at target
        // - Block if something is in the way
        // - Move if the path is clear
        if dx != 0 || dy != 0 {
            let action = Action::Move { dx, dy };
            let _result = action.execute(world, grid, id, events);
            // Note: We don't need to handle the result specially here.
            // If blocked or attacked, that's fine - the action system handles it.
        } else {
            // No movement intended, just wait (spend energy)
            let action = Action::Wait;
            let _result = action.execute(world, grid, id, events);
        }
    }
}
