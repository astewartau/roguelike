//! Input handling and player control.
//!
//! Processes keyboard and mouse input, manages click-to-move pathing.
//! This module is purely about input state - it does NOT execute game logic.

use crate::camera::Camera;
use crate::components::{Actor, BlocksMovement, Position};
use crate::grid::Grid;
use crate::pathfinding;
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

    /// Get the next step in the path, if any
    pub fn peek_next_step(&self) -> Option<(i32, i32)> {
        self.player_path.front().copied()
    }

    /// Remove the front step from the path (call after successfully moving)
    pub fn consume_step(&mut self) {
        self.player_path.pop_front();
    }

    /// Check if we've arrived at our destination
    pub fn has_arrived(&self) -> bool {
        self.player_path.is_empty() && self.player_path_destination.is_some()
    }

    /// Clear the destination (call after handling arrival)
    pub fn clear_destination(&mut self) {
        self.player_path_destination = None;
    }
}

impl Default for InputState {
    fn default() -> Self {
        Self::new()
    }
}

/// Result of processing keyboard input
pub struct InputResult {
    /// Player wants to toggle fullscreen
    pub toggle_fullscreen: bool,
    /// Player wants to toggle inventory
    pub toggle_inventory: bool,
    /// Player wants to toggle grid lines
    pub toggle_grid_lines: bool,
    /// Player pressed Enter (take all / loot)
    pub enter_pressed: bool,
    /// Movement intent (dx, dy)
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

/// Process keyboard input and return actions to take.
/// Does NOT execute any game logic - just returns intents.
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

/// Handle click-to-move: calculate path to clicked tile.
/// Does NOT execute movement - just calculates and stores the path.
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

/// Get the next movement from click-to-move path, if player can act.
/// Returns the (dx, dy) movement intent, or None if no path or can't act.
pub fn get_path_movement(
    input: &InputState,
    world: &World,
    player_entity: Entity,
) -> Option<(i32, i32)> {
    // Check if player can act
    let player_can_act = world
        .get::<&Actor>(player_entity)
        .map(|a| a.can_act())
        .unwrap_or(false);

    if !player_can_act {
        return None;
    }

    // Get the next step
    let (next_x, next_y) = input.peek_next_step()?;

    // Get player position
    let player_pos = match world.get::<&Position>(player_entity) {
        Ok(p) => (p.x, p.y),
        Err(_) => return None,
    };

    // Calculate movement direction
    let dx = next_x - player_pos.0;
    let dy = next_y - player_pos.1;

    Some((dx, dy))
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
