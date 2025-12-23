//! Input handling and player control.
//!
//! Processes keyboard and mouse input, manages click-to-move pathing.
//! This module is purely about input state - it does NOT execute game logic.

use crate::camera::Camera;
use crate::components::{Actor, Attackable, BlocksMovement, Container, Door, ItemType, Position};
use crate::grid::Grid;
use crate::pathfinding;
use hecs::{Entity, World};
use std::collections::{HashSet, VecDeque};
use winit::keyboard::KeyCode;

/// Maximum distance an enemy can move from the click origin before pursuit is abandoned
pub const MAX_PURSUIT_DISTANCE: i32 = 8;

/// Targeting mode for items that require click-to-target
#[derive(Clone, Debug)]
pub struct TargetingMode {
    /// The item type being used
    pub item_type: ItemType,
    /// The index of the item in inventory
    pub item_index: usize,
    /// Maximum range for targeting
    pub max_range: i32,
    /// Radius of effect (0 for single-tile effects like Blink)
    pub radius: i32,
}

/// What the player clicked on - determines interaction behavior
#[derive(Debug, Clone, Copy)]
pub enum ClickTarget {
    /// Empty ground - just walk there
    Ground { x: i32, y: i32 },
    /// Enemy - pursue and attack
    Enemy { entity: Entity, x: i32, y: i32 },
    /// Closed door - path to and bump to open
    Door { entity: Entity, x: i32, y: i32 },
    /// Chest - path to and bump to open
    Chest { entity: Entity, x: i32, y: i32 },
    /// Walkable container (bones) - walk onto and auto-loot
    WalkableContainer { entity: Entity, x: i32, y: i32 },
    /// Blocked terrain or entity we can't interact with
    Blocked,
}

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
    /// Enemy being pursued (for click-to-attack)
    pub pursuit_target: Option<Entity>,
    /// Original click position when pursuit started (bounds how far we'll chase)
    pub pursuit_origin: Option<(i32, i32)>,
    /// Targeting mode for items that require click-to-target (Blink, Fireball)
    pub targeting_mode: Option<TargetingMode>,
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
            pursuit_target: None,
            pursuit_origin: None,
            targeting_mode: None,
        }
    }

    /// Clear the current path and any pursuit state
    pub fn clear_path(&mut self) {
        self.player_path.clear();
        self.player_path_destination = None;
        self.pursuit_target = None;
        self.pursuit_origin = None;
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

    /// Enter targeting mode for an item
    pub fn enter_targeting_mode(&mut self, item_type: ItemType, item_index: usize, max_range: i32, radius: i32) {
        self.targeting_mode = Some(TargetingMode {
            item_type,
            item_index,
            max_range,
            radius,
        });
        // Clear any movement path when entering targeting mode
        self.clear_path();
    }

    /// Exit targeting mode
    pub fn cancel_targeting(&mut self) {
        self.targeting_mode = None;
    }

    /// Check if in targeting mode
    pub fn is_targeting(&self) -> bool {
        self.targeting_mode.is_some()
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
    /// Attack direction intent (dx, dy) - Shift+movement
    pub attack_direction: Option<(i32, i32)>,
    /// Right-click shoot intent (target_x, target_y)
    pub shoot_target: Option<(i32, i32)>,
    /// Player pressed Escape (cancel targeting, close menus, etc.)
    pub escape_pressed: bool,
}

impl Default for InputResult {
    fn default() -> Self {
        Self {
            toggle_fullscreen: false,
            toggle_inventory: false,
            toggle_grid_lines: false,
            enter_pressed: false,
            movement: None,
            attack_direction: None,
            shoot_target: None,
            escape_pressed: false,
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

    // Check if shift is held (don't consume - it's a modifier)
    let shift_held = input.keys_pressed.contains(&KeyCode::ShiftLeft)
        || input.keys_pressed.contains(&KeyCode::ShiftRight);

    // Movement or attack direction (only process once per key press)
    // Shift+direction = attack direction, plain direction = movement
    let direction = if input.keys_pressed.remove(&KeyCode::KeyW)
        || input.keys_pressed.remove(&KeyCode::ArrowUp)
    {
        Some((0, 1))
    } else if input.keys_pressed.remove(&KeyCode::KeyS)
        || input.keys_pressed.remove(&KeyCode::ArrowDown)
    {
        Some((0, -1))
    } else if input.keys_pressed.remove(&KeyCode::KeyA)
        || input.keys_pressed.remove(&KeyCode::ArrowLeft)
    {
        Some((-1, 0))
    } else if input.keys_pressed.remove(&KeyCode::KeyD)
        || input.keys_pressed.remove(&KeyCode::ArrowRight)
    {
        Some((1, 0))
    } else {
        None
    };

    if let Some(dir) = direction {
        if shift_held {
            result.attack_direction = Some(dir);
        } else {
            result.movement = Some(dir);
        }
    }

    result
}

/// Identify what the player clicked on at a given tile position.
/// Checks entities at the tile and terrain type to determine interaction.
pub fn identify_click_target(
    world: &World,
    grid: &Grid,
    tile_x: i32,
    tile_y: i32,
) -> ClickTarget {
    // Check terrain first - is it even a valid tile?
    let tile = match grid.get(tile_x, tile_y) {
        Some(t) => t,
        None => return ClickTarget::Blocked,
    };

    // Check for entities at this tile (order matters - check most specific first)

    // 1. Check for attackable enemy
    for (id, (pos, _)) in world.query::<(&Position, &Attackable)>().iter() {
        if pos.x == tile_x && pos.y == tile_y {
            return ClickTarget::Enemy { entity: id, x: tile_x, y: tile_y };
        }
    }

    // 2. Check for closed door
    for (id, (pos, door)) in world.query::<(&Position, &Door)>().iter() {
        if pos.x == tile_x && pos.y == tile_y && !door.is_open {
            return ClickTarget::Door { entity: id, x: tile_x, y: tile_y };
        }
    }

    // 3. Check for chest (container that blocks movement)
    for (id, (pos, _container)) in world.query::<(&Position, &Container)>().iter() {
        if pos.x == tile_x && pos.y == tile_y {
            // Is it a blocking container (chest) or walkable (bones)?
            if world.get::<&BlocksMovement>(id).is_ok() {
                return ClickTarget::Chest { entity: id, x: tile_x, y: tile_y };
            } else {
                return ClickTarget::WalkableContainer { entity: id, x: tile_x, y: tile_y };
            }
        }
    }

    // 4. Check for other blocking entities we can't interact with
    for (id, (pos, _)) in world.query::<(&Position, &BlocksMovement)>().iter() {
        if pos.x == tile_x && pos.y == tile_y {
            // Something is blocking but not interactive
            let _ = id;
            return ClickTarget::Blocked;
        }
    }

    // 5. Check terrain walkability
    if !tile.tile_type.is_walkable() {
        return ClickTarget::Blocked;
    }

    // Nothing special - just ground
    ClickTarget::Ground { x: tile_x, y: tile_y }
}

/// Handle click-to-move: calculate path to clicked tile based on what was clicked.
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

    // Identify what was clicked
    let target = identify_click_target(world, grid, tile_x, tile_y);

    // Handle based on target type
    match target {
        ClickTarget::Enemy { entity, x, y } => {
            // Set up pursuit mode for enemies
            input.pursuit_target = Some(entity);
            input.pursuit_origin = Some((x, y));

            // Check if already adjacent - queue attack immediately
            let dx = (x - player_pos.0).abs();
            let dy = (y - player_pos.1).abs();
            if dx <= 1 && dy <= 1 {
                input.player_path.clear();
                input.player_path.push_back((x, y));
                input.player_path_destination = Some((x, y));
                return;
            }

            // Path to adjacent tile (will attack when we arrive)
            if let Some(path) = path_to_adjacent(grid, world, player_entity, player_pos, (x, y)) {
                input.player_path = VecDeque::from(path);
                input.player_path_destination = Some((x, y));
            }
        }

        ClickTarget::Door { x, y, .. } | ClickTarget::Chest { x, y, .. } => {
            // Path to adjacent tile, then bump into target to open
            input.pursuit_target = None;
            input.pursuit_origin = None;

            if let Some(mut path) = path_to_adjacent(grid, world, player_entity, player_pos, (x, y))
            {
                // Append the target tile - walking "into" it triggers the open action
                path.push((x, y));
                input.player_path = VecDeque::from(path);
                input.player_path_destination = Some((x, y));
            }
        }

        ClickTarget::WalkableContainer { x, y, .. } | ClickTarget::Ground { x, y } => {
            // Path directly to the tile (arrival handles auto-loot for containers)
            input.pursuit_target = None;
            input.pursuit_origin = None;

            let blocked: HashSet<(i32, i32)> = world
                .query::<(&Position, &BlocksMovement)>()
                .iter()
                .filter(|(id, _)| *id != player_entity)
                .map(|(_, (pos, _))| (pos.x, pos.y))
                .collect();

            if let Some(path) = pathfinding::find_path(grid, player_pos, (x, y), &blocked) {
                input.player_path = VecDeque::from(path);
                input.player_path_destination = Some((x, y));
            }
        }

        ClickTarget::Blocked => {
            // Can't interact - do nothing
        }
    }
}

/// Calculate a path to a tile adjacent to the target (for attacking)
fn path_to_adjacent(
    grid: &Grid,
    world: &World,
    player_entity: Entity,
    player_pos: (i32, i32),
    target_pos: (i32, i32),
) -> Option<Vec<(i32, i32)>> {
    // Collect blocking positions, excluding the target tile itself
    let blocked: HashSet<(i32, i32)> = world
        .query::<(&Position, &BlocksMovement)>()
        .iter()
        .filter(|(id, _)| *id != player_entity)
        .map(|(_, (pos, _))| (pos.x, pos.y))
        .filter(|pos| *pos != target_pos)
        .collect();

    // Try to find path to any adjacent tile
    let adjacents = [
        (target_pos.0 - 1, target_pos.1),
        (target_pos.0 + 1, target_pos.1),
        (target_pos.0, target_pos.1 - 1),
        (target_pos.0, target_pos.1 + 1),
    ];

    let mut best_path: Option<Vec<(i32, i32)>> = None;

    for adj in adjacents {
        // Skip if blocked or not walkable
        if blocked.contains(&adj) {
            continue;
        }
        if !grid.get(adj.0, adj.1).map(|t| t.tile_type.is_walkable()).unwrap_or(false) {
            continue;
        }

        if let Some(path) = pathfinding::find_path(grid, player_pos, adj, &blocked) {
            if best_path.as_ref().map(|p| path.len() < p.len()).unwrap_or(true) {
                best_path = Some(path);
            }
        }
    }

    best_path
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

/// Update pursuit path if we're chasing an enemy.
/// Call this before get_path_movement to recalculate path to moving targets.
/// Returns true if still pursuing, false if pursuit ended.
pub fn update_pursuit(
    input: &mut InputState,
    world: &World,
    grid: &Grid,
    player_entity: Entity,
) -> bool {
    let (target, origin) = match (input.pursuit_target, input.pursuit_origin) {
        (Some(t), Some(o)) => (t, o),
        _ => return false, // Not in pursuit mode
    };

    // Check if target still exists and is attackable
    let target_pos = match world.get::<&Position>(target) {
        Ok(pos) => (pos.x, pos.y),
        Err(_) => {
            // Target no longer exists (dead?)
            input.clear_path();
            return false;
        }
    };

    // Check if target is still attackable
    if world.get::<&Attackable>(target).is_err() {
        input.clear_path();
        return false;
    }

    // Check if target moved too far from origin
    let dist_from_origin = (target_pos.0 - origin.0).abs() + (target_pos.1 - origin.1).abs();
    if dist_from_origin > MAX_PURSUIT_DISTANCE {
        input.clear_path();
        return false;
    }

    // Get player position
    let player_pos = match world.get::<&Position>(player_entity) {
        Ok(p) => (p.x, p.y),
        Err(_) => {
            input.clear_path();
            return false;
        }
    };

    // Check if we're already adjacent to target - queue movement into enemy to trigger attack
    let dx = (target_pos.0 - player_pos.0).abs();
    let dy = (target_pos.1 - player_pos.1).abs();
    if dx <= 1 && dy <= 1 {
        // Adjacent - set path to enemy's tile, moving "into" them triggers attack
        input.player_path.clear();
        input.player_path.push_back(target_pos);
        input.player_path_destination = Some(target_pos);
        return true;
    }

    // Only recalculate path if current path is empty or next step is blocked
    let needs_recalc = if let Some(next_step) = input.player_path.front() {
        // Check if next step is blocked (by another entity, not the target)
        let step_blocked = world
            .query::<(&Position, &BlocksMovement)>()
            .iter()
            .filter(|(id, _)| *id != player_entity && *id != target)
            .any(|(_, (pos, _))| pos.x == next_step.0 && pos.y == next_step.1);

        // Check if next step is not walkable terrain
        let step_unwalkable = !grid
            .get(next_step.0, next_step.1)
            .map(|t| t.tile_type.is_walkable())
            .unwrap_or(false);

        step_blocked || step_unwalkable
    } else {
        // Path is empty, need to recalculate
        true
    };

    if needs_recalc {
        // Recalculate path to target's current position
        if let Some(path) = path_to_adjacent(grid, world, player_entity, player_pos, target_pos) {
            input.player_path = VecDeque::from(path);
            input.player_path_destination = Some(target_pos);
            true
        } else {
            // Can't reach target
            input.clear_path();
            false
        }
    } else {
        // Keep following current path
        true
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

/// Convert right-click position to a shoot target (tile coordinates)
pub fn get_shoot_target(
    input: &InputState,
    camera: &Camera,
) -> (i32, i32) {
    let world_pos = camera.screen_to_world(input.mouse_pos.0, input.mouse_pos.1);
    let tile_x = world_pos.x.floor() as i32;
    let tile_y = world_pos.y.floor() as i32;
    (tile_x, tile_y)
}
