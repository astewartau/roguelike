//! Input handling and player control.
//!
//! Processes keyboard and mouse input, manages click-to-move pathing.

use crate::camera::Camera;
use crate::components::{BlocksMovement, Position};
use crate::grid::Grid;
use crate::pathfinding;
use crate::systems::{self, MoveResult};
use crate::events::EventQueue;
use crate::vfx::VfxManager;
use hecs::{Entity, World};
use std::collections::{HashSet, VecDeque};
use winit::keyboard::KeyCode;

/// Input state tracking
pub struct InputState {
    pub keys_pressed: HashSet<KeyCode>,
    pub mouse_pos: (f32, f32),
    pub mouse_down: bool,
    pub last_mouse_pos: (f32, f32),
    /// Click-to-move path (VecDeque for O(1) pop_front)
    pub player_path: VecDeque<(i32, i32)>,
    /// Destination of click-to-move (for auto-interact on arrival)
    pub player_path_destination: Option<(i32, i32)>,
}

impl InputState {
    pub fn new() -> Self {
        Self {
            keys_pressed: HashSet::new(),
            mouse_pos: (0.0, 0.0),
            mouse_down: false,
            last_mouse_pos: (0.0, 0.0),
            player_path: VecDeque::new(),
            player_path_destination: None,
        }
    }

    /// Clear the current path
    pub fn clear_path(&mut self) {
        self.player_path.clear();
        self.player_path_destination = None;
    }
}

impl Default for InputState {
    fn default() -> Self {
        Self::new()
    }
}

/// Result of processing input
pub struct InputResult {
    /// Player wants to toggle fullscreen
    pub toggle_fullscreen: bool,
    /// Player wants to toggle inventory
    pub toggle_inventory: bool,
    /// Player wants to toggle grid lines
    pub toggle_grid_lines: bool,
    /// Player pressed Enter (take all / loot)
    pub enter_pressed: bool,
    /// Movement to execute (dx, dy)
    pub movement: Option<(i32, i32)>,
}

impl Default for InputResult {
    fn default() -> Self {
        Self {
            toggle_fullscreen: false,
            toggle_inventory: false,
            toggle_grid_lines: false,
            enter_pressed: false,
            movement: None,
        }
    }
}

/// Process keyboard input and return actions to take
pub fn process_keyboard(input: &mut InputState) -> InputResult {
    let mut result = InputResult::default();

    // Toggle fullscreen
    if input.keys_pressed.remove(&KeyCode::F11) {
        result.toggle_fullscreen = true;
    }

    // Toggle inventory
    if input.keys_pressed.remove(&KeyCode::KeyI) {
        result.toggle_inventory = true;
    }

    // Toggle grid lines
    if input.keys_pressed.remove(&KeyCode::BracketRight) {
        result.toggle_grid_lines = true;
    }

    // Enter key
    if input.keys_pressed.remove(&KeyCode::Enter) {
        result.enter_pressed = true;
    }

    // Movement (only process once per key press)
    if input.keys_pressed.remove(&KeyCode::KeyW) || input.keys_pressed.remove(&KeyCode::ArrowUp) {
        result.movement = Some((0, 1));
    } else if input.keys_pressed.remove(&KeyCode::KeyS)
        || input.keys_pressed.remove(&KeyCode::ArrowDown)
    {
        result.movement = Some((0, -1));
    } else if input.keys_pressed.remove(&KeyCode::KeyA)
        || input.keys_pressed.remove(&KeyCode::ArrowLeft)
    {
        result.movement = Some((-1, 0));
    } else if input.keys_pressed.remove(&KeyCode::KeyD)
        || input.keys_pressed.remove(&KeyCode::ArrowRight)
    {
        result.movement = Some((1, 0));
    }

    result
}

/// Handle click-to-move: calculate path to clicked tile
pub fn handle_click_to_move(
    input: &mut InputState,
    camera: &Camera,
    world: &World,
    grid: &Grid,
    player_entity: Entity,
) {
    // Convert screen position to world position
    let world_pos = camera.screen_to_world(input.mouse_pos.0, input.mouse_pos.1);

    // Convert to tile coordinates
    let tile_x = world_pos.x.floor() as i32;
    let tile_y = world_pos.y.floor() as i32;

    // Get player position
    let player_pos = match world.get::<&Position>(player_entity) {
        Ok(p) => (p.x, p.y),
        Err(_) => return,
    };

    // Don't path to current position
    if tile_x == player_pos.0 && tile_y == player_pos.1 {
        return;
    }

    // Collect blocking positions (other entities)
    let blocked: HashSet<(i32, i32)> = world
        .query::<(&Position, &BlocksMovement)>()
        .iter()
        .filter(|(id, _)| *id != player_entity)
        .map(|(_, (pos, _))| (pos.x, pos.y))
        .collect();

    // Calculate path
    if let Some(path) = pathfinding::find_path(grid, player_pos, (tile_x, tile_y), &blocked) {
        input.player_path = VecDeque::from(path);
        input.player_path_destination = Some((tile_x, tile_y));
    }
}

/// Follow the click-to-move path, executing one step
/// Returns Some(container_id) if player arrived at a lootable container
pub fn follow_player_path(
    input: &mut InputState,
    world: &mut World,
    grid: &Grid,
    player_entity: Entity,
    events: &mut EventQueue,
    vfx: &mut VfxManager,
) -> Option<Entity> {
    // Get the next step (peek at front)
    let (next_x, next_y) = *input.player_path.front()?;

    // Get player position
    let player_pos = match world.get::<&Position>(player_entity) {
        Ok(p) => (p.x, p.y),
        Err(_) => {
            input.clear_path();
            return None;
        }
    };

    // Calculate movement direction
    let dx = next_x - player_pos.0;
    let dy = next_y - player_pos.1;

    // Execute the move
    let result = run_ticks_until_player_acts(world, grid, player_entity, dx, dy, events, vfx);

    match result {
        MoveResult::Moved => {
            // Remove the step we just took (O(1) with VecDeque)
            input.player_path.pop_front();

            // If path is now empty, we've arrived - check for auto-interact
            if input.player_path.is_empty() {
                if let Some(_dest) = input.player_path_destination.take() {
                    // Check for lootable container at current position
                    return systems::find_container_at_player(world, player_entity);
                }
            }
            None
        }
        MoveResult::Attacked(_) | MoveResult::OpenedChest(_) => {
            // Stop pathing if we attacked or opened something
            input.clear_path();
            if let MoveResult::OpenedChest(chest) = result {
                Some(chest)
            } else {
                None
            }
        }
        MoveResult::Blocked => {
            // Path is blocked, clear it
            input.clear_path();
            None
        }
    }
}

/// Run game ticks until the player can act, then execute their move
pub fn run_ticks_until_player_acts(
    world: &mut World,
    grid: &Grid,
    player_entity: Entity,
    dx: i32,
    dy: i32,
    events: &mut EventQueue,
    vfx: &mut VfxManager,
) -> MoveResult {
    let mut rng = rand::thread_rng();

    loop {
        // Check if player can act
        let player_can_act = world
            .get::<&crate::components::Actor>(player_entity)
            .map(|a| a.energy >= a.speed)
            .unwrap_or(true);

        if player_can_act {
            let result = systems::player_move(world, grid, player_entity, dx, dy, events);
            // Process events (VFX spawning, etc.)
            process_events(events, vfx);
            return result;
        }

        // Player can't act yet - run one tick
        systems::tick_energy(world);
        systems::ai_chase(world, grid, player_entity, &mut rng, events);
        // Process events from AI actions too
        process_events(events, vfx);
    }
}

/// Process all pending events, dispatching to appropriate handlers
pub fn process_events(events: &mut EventQueue, vfx: &mut VfxManager) {
    for event in events.drain() {
        vfx.handle_event(&event);
        // Future: audio.handle_event(&event), ui.handle_event(&event), etc.
    }
}

/// Process mouse drag for camera panning
pub fn process_mouse_drag(input: &mut InputState, camera: &mut Camera, show_inventory: bool) {
    if input.mouse_down && !show_inventory {
        let dx = input.mouse_pos.0 - input.last_mouse_pos.0;
        let dy = input.mouse_pos.1 - input.last_mouse_pos.1;
        if dx.abs() > 0.1 || dy.abs() > 0.1 {
            camera.pan(dx, dy);
        }
    }
    // Consume the mouse delta so it's not applied again next frame
    input.last_mouse_pos = input.mouse_pos;
}
