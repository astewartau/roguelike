//! Game engine - owns all game state and provides a clean API to the application shell.
//!
//! The engine handles:
//! - Game state (world, grid, floors, time system)
//! - Input processing
//! - Simulation advancement
//! - Event processing
//!
//! The application shell (main.rs) only handles:
//! - Window creation and event loop
//! - Forwarding events to the engine
//! - Rendering what the engine returns

mod dev_spawning;
pub mod floor_transition;
mod game_state;
pub mod initialization;
mod simulation;

pub use floor_transition::{can_transition_floor, handle_floor_transition};
pub use game_state::GameState;
pub use initialization::initialize_single_ai_actor;
pub use simulation::*;

use crate::components::{AbilityType, ActionType, Actor, ClassAbility, PlayerClass};

use crate::camera::Camera;
use crate::events::EventQueue;
use crate::input::{self, InputState, TargetingMode};
use crate::spawning;
use crate::systems;
use crate::time_system;
use crate::ui::{DevMenu, GameUiState, UiActions};
use crate::vfx::{FireEffect, VfxManager, VisualEffect};

use hecs::Entity;
use winit::event::{ElementState, MouseButton, MouseScrollDelta, WindowEvent};
use winit::keyboard::{KeyCode, PhysicalKey};

/// Actions the engine wants the window to perform
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WindowAction {
    Exit,
    ToggleFullscreen,
}

/// Game mode - whether we're on the start screen or playing
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GameMode {
    /// Class selection screen
    StartScreen,
    /// Playing the game
    Playing,
}

/// Result of a game tick - contains everything needed for rendering
pub struct TickResult {
    /// Entities to render
    pub entities: Vec<crate::systems::RenderEntity>,
    /// Window action to perform (if any)
    pub window_action: Option<WindowAction>,
}

/// The game engine - owns all game state and simulation logic.
pub struct GameEngine {
    /// Current game mode (start screen or playing)
    pub game_mode: GameMode,

    /// Selected player class (for start screen)
    pub selected_class: Option<PlayerClass>,

    /// Core game state (world, grid, floors, time) - None on start screen
    pub state: Option<GameState>,

    /// Visual effects manager
    pub vfx: VfxManager,

    /// Event queue for game events
    pub events: EventQueue,

    /// Input state tracking
    pub input: InputState,

    /// UI state (inventory open, chest open, etc.) - needs player entity, created with state
    ui_state: Option<GameUiState>,

    /// Developer menu state
    pub dev_menu: DevMenu,

    /// Accumulated real time (for animations)
    pub real_time: f32,
}

impl GameEngine {
    /// Create a new game engine on the start screen.
    pub fn new() -> Self {
        Self {
            game_mode: GameMode::StartScreen,
            selected_class: Some(PlayerClass::Fighter), // Default selection
            state: None,
            vfx: VfxManager::new(),
            events: EventQueue::new(),
            input: InputState::new(),
            ui_state: None,
            dev_menu: DevMenu::new(),
            real_time: 0.0,
        }
    }

    /// Start the game with the selected class.
    pub fn start_game(&mut self, class: PlayerClass, camera: &mut Camera) {
        let mut state = GameState::new(class);

        // Initialize AI actors
        state.initialize_ai(&mut self.events);

        // Spawn campfire in starting room near wizard
        if let Some(starting_room) = &state.grid.starting_room {
            // Find a position for the campfire (offset from center)
            let (cx, cy) = starting_room.center();
            // Try to place it to the right of center, or find first available spot
            let campfire_positions = [
                (cx + 2, cy),
                (cx - 2, cy),
                (cx, cy + 2),
                (cx, cy - 2),
                (cx + 1, cy + 1),
            ];
            for (x, y) in campfire_positions {
                if state.grid.is_walkable(x, y) {
                    spawning::spawn_campfire(&mut state.world, x, y);
                    break;
                }
            }
        }

        // Set up camera to track player
        if let Some((x, y)) = state.player_start_position() {
            camera.set_tracking_target(glam::Vec2::new(x, y));
        }

        let ui_state = GameUiState::new(state.player_entity);
        self.state = Some(state);
        self.ui_state = Some(ui_state);
        self.game_mode = GameMode::Playing;
    }

    /// Check if we're currently playing (not on start screen).
    pub fn is_playing(&self) -> bool {
        self.game_mode == GameMode::Playing
    }

    /// Get a reference to the UI state (panics if not playing).
    pub fn ui_state(&self) -> &GameUiState {
        self.ui_state.as_ref().expect("UI state not initialized - game not started")
    }

    /// Get a mutable reference to the UI state (panics if not playing).
    pub fn ui_state_mut(&mut self) -> &mut GameUiState {
        self.ui_state.as_mut().expect("UI state not initialized - game not started")
    }

    /// Handle a window event.
    /// Returns a WindowAction if the engine wants the window to do something.
    pub fn handle_event(
        &mut self,
        event: &WindowEvent,
        camera: &mut Camera,
        egui_consumed: bool,
    ) -> Option<WindowAction> {
        match event {
            WindowEvent::KeyboardInput { event: key_event, .. } => {
                if !egui_consumed {
                    if let PhysicalKey::Code(key) = key_event.physical_key {
                        match key_event.state {
                            ElementState::Pressed => {
                                if key == KeyCode::Escape {
                                    if self.input.is_targeting() {
                                        self.input.cancel_targeting();
                                        return None;
                                    } else {
                                        return Some(WindowAction::Exit);
                                    }
                                }
                                if key == KeyCode::Backquote {
                                    self.dev_menu.toggle();
                                }
                                self.input.keys_pressed.insert(key);
                            }
                            ElementState::Released => {
                                self.input.keys_pressed.remove(&key);
                            }
                        }
                    }
                }
            }
            WindowEvent::CursorMoved { position, .. } => {
                self.input.last_mouse_pos = self.input.mouse_pos;
                self.input.mouse_pos = (position.x as f32, position.y as f32);
            }
            WindowEvent::MouseInput { state: btn_state, button, .. } => {
                if !egui_consumed && *button == MouseButton::Left {
                    let was_down = self.input.mouse_down;
                    self.input.mouse_down = *btn_state == ElementState::Pressed;

                    if *btn_state == ElementState::Pressed {
                        self.input.mouse_down_pos = self.input.mouse_pos;
                        camera.start_pan(self.input.mouse_pos.0, self.input.mouse_pos.1);
                    } else if *btn_state == ElementState::Released {
                        camera.release_pan();
                        let dx = self.input.mouse_pos.0 - self.input.mouse_down_pos.0;
                        let dy = self.input.mouse_pos.1 - self.input.mouse_down_pos.1;
                        let was_drag = was_down
                            && (dx.abs() > crate::constants::CLICK_DRAG_THRESHOLD
                                || dy.abs() > crate::constants::CLICK_DRAG_THRESHOLD);

                        if !was_drag && self.is_playing() {
                            if self.input.is_targeting() {
                                self.input.pending_left_click = true;
                            } else if self.dev_menu.has_active_tool() {
                                self.handle_dev_spawn(camera);
                            } else if let Some(ref state) = self.state {
                                input::handle_click_to_move(
                                    &mut self.input,
                                    camera,
                                    &state.world,
                                    &state.grid,
                                    state.player_entity,
                                );
                            }
                        }
                    }
                }
                if !egui_consumed && *button == MouseButton::Right {
                    if *btn_state == ElementState::Released {
                        if self.input.is_targeting() {
                            self.input.cancel_targeting();
                        } else {
                            self.input.pending_right_click = true;
                        }
                    }
                }
            }
            WindowEvent::MouseWheel { delta, .. } => {
                if !egui_consumed {
                    let scroll = match delta {
                        MouseScrollDelta::LineDelta(_, y) => *y * 2.0,
                        MouseScrollDelta::PixelDelta(pos) => pos.y as f32 * 0.1,
                    };
                    camera.add_zoom_impulse(scroll, self.input.mouse_pos.0, self.input.mouse_pos.1);
                }
            }
            _ => {}
        }
        None
    }

    /// Process a frame tick - advances simulation, returns render data.
    /// Returns empty results if not in playing mode.
    pub fn tick(&mut self, dt: f32, camera: &mut Camera) -> TickResult {
        // Accumulate real time for animations
        self.real_time += dt;

        // Only run game simulation when playing
        if self.state.is_none() {
            camera.update(dt, self.input.mouse_down);
            return TickResult {
                entities: vec![],
                window_action: None,
            };
        }

        // Handle input first (needs full &mut self access)
        let input_result = self.process_input(camera);
        let mut window_action = None;
        if input_result.toggle_fullscreen {
            window_action = Some(WindowAction::ToggleFullscreen);
        }

        // Now extract state references for the rest
        let state = self.state.as_mut().expect("State checked above");
        let ui_state = self.ui_state.as_mut().expect("UI state should exist when state exists");

        // Update animations
        systems::update_lunge_animations(&mut state.world, dt);
        self.vfx.update(dt);

        // Remove dead entities
        let mut rng = rand::thread_rng();
        systems::remove_dead_entities(
            &mut state.world,
            state.player_entity,
            &mut rng,
            &mut self.events,
            Some(&mut state.action_scheduler),
        );

        // Process events from remove_dead_entities
        let event_result = process_events(
            &mut self.events,
            &mut state.world,
            &state.grid,
            &mut self.vfx,
            ui_state,
            state.player_entity,
        );

        // Collect skeleton spawn positions before floor transition might invalidate state
        let skeleton_spawns = event_result.skeleton_spawns.clone();

        if let Some(direction) = event_result.floor_transition {
            self.handle_floor_transition(direction, camera);
        }
        if event_result.should_interrupt_path() {
            self.input.clear_path();
        }

        // Re-borrow state after floor transition (which may have modified it)
        let state = self.state.as_mut().expect("State should still exist after floor transition");

        // Spawn skeletons from opened coffins
        if !skeleton_spawns.is_empty() {
            for (x, y) in &skeleton_spawns {
                let skeleton = spawning::enemies::SKELETON.spawn(&mut state.world, *x, *y);
                let mut rng = rand::thread_rng();
                initialization::initialize_single_ai_actor(
                    &mut state.world,
                    &state.grid,
                    skeleton,
                    state.player_entity,
                    &state.game_clock,
                    &mut state.action_scheduler,
                    &mut self.events,
                    &mut rng,
                );
            }
            // Close loot UI - player must deal with skeleton first
            if let Some(ui_state) = self.ui_state.as_mut() {
                ui_state.close_chest();
            }
        }

        // Visual lerping
        systems::visual_lerp(&mut state.world, dt);
        systems::lerp_projectiles_realtime(
            &mut state.world,
            dt,
            crate::constants::ARROW_SPEED,
        );

        // Projectile cleanup
        let finished = systems::cleanup_finished_projectiles(&state.world);
        systems::despawn_projectiles(&mut state.world, finished);

        // Update camera tracking
        if let Ok(vis_pos) = state.world.get::<&crate::components::VisualPosition>(state.player_entity) {
            camera.set_tracking_target(glam::Vec2::new(vis_pos.x, vis_pos.y));
        }

        // Update camera
        camera.update(dt, self.input.mouse_down);

        // Update visibility based on LOS and illumination
        // Player's light + any visible light sources (campfires) determine what you can see
        systems::update_fov(
            &state.world,
            &mut state.grid,
            state.player_entity,
            crate::constants::FOV_RADIUS,
            state.game_clock.time,
        );

        // Calculate per-tile illumination (must be after FOV update)
        systems::calculate_illumination(
            &state.world,
            &mut state.grid,
            state.player_entity,
            crate::constants::FOV_RADIUS,
        );

        // Collect renderables
        let entities = systems::collect_renderables(
            &state.world,
            &state.grid,
            state.player_entity,
            self.real_time,
        );

        TickResult {
            entities,
            window_action,
        }
    }

    /// Process UI actions from the UI layer.
    /// Does nothing if not playing.
    pub fn process_ui_actions(&mut self, actions: &UiActions) {
        let Some(ref mut state) = self.state else { return };
        let ui_state = self.ui_state.as_ref().expect("UI state should exist when state exists");

        let ui_result = process_ui_actions(
            &mut state.world,
            &mut state.grid,
            state.player_entity,
            actions,
            &mut self.dev_menu,
            ui_state,
            &mut self.events,
            state.game_clock.time,
        );

        // Handle ability button click from UI
        if actions.use_ability {
            self.try_use_class_ability();
        }

        let ui_state = self.ui_state.as_mut().expect("UI state should exist");
        // Apply UI state changes
        if let Some(targeting) = ui_result.enter_targeting {
            self.input.targeting_mode = Some(targeting);
        }
        if ui_result.close_inventory {
            ui_state.show_inventory = false;
        }
        if ui_result.close_context_menu {
            ui_state.close_context_menu();
        }
        if ui_result.close_chest {
            ui_state.close_chest();
        }
        if ui_result.close_dialogue {
            ui_state.close_dialogue();
        }
    }

    /// Get the grid for rendering (returns None if not playing).
    pub fn grid(&self) -> Option<&crate::grid::Grid> {
        self.state.as_ref().map(|s| &s.grid)
    }

    /// Get VFX effects for rendering.
    pub fn vfx_effects(&self) -> &[VisualEffect] {
        &self.vfx.effects
    }

    /// Get fire effects for rendering.
    pub fn fires(&self) -> &[FireEffect] {
        &self.vfx.fires
    }

    /// Get the current game time (0.0 if not playing).
    #[allow(dead_code)] // Public API for external callers
    pub fn game_time(&self) -> f32 {
        self.state.as_ref().map(|s| s.game_clock.time).unwrap_or(0.0)
    }

    /// Get the targeting mode if active.
    #[allow(dead_code)] // Public API for external callers
    pub fn targeting_mode(&self) -> Option<&TargetingMode> {
        self.input.targeting_mode.as_ref()
    }

    /// Get mouse position.
    #[allow(dead_code)] // Public API for external callers
    pub fn mouse_pos(&self) -> (f32, f32) {
        self.input.mouse_pos
    }

    /// Get the player entity (returns None if not playing).
    #[allow(dead_code)] // Public API for external callers
    pub fn player_entity(&self) -> Option<Entity> {
        self.state.as_ref().map(|s| s.player_entity)
    }

    /// Get a reference to the ECS world (returns None if not playing).
    #[allow(dead_code)] // Public API for external callers
    pub fn world(&self) -> Option<&hecs::World> {
        self.state.as_ref().map(|s| &s.world)
    }

    /// Should show grid lines?
    pub fn show_grid_lines(&self) -> bool {
        self.ui_state.as_ref().map(|u| u.show_grid_lines).unwrap_or(false)
    }

    /// Run the UI and return actions. This handles the borrowing internally.
    /// When on start screen, shows class selection. Returns start_game action if player clicks Start.
    pub fn run_ui(
        &mut self,
        egui_glow: &mut egui_glow::EguiGlow,
        window: &winit::window::Window,
        camera: &crate::camera::Camera,
        tileset: &crate::multi_tileset::MultiTileset,
        ui_icons: &crate::ui::UiIcons,
    ) -> crate::ui::UiActions {
        match self.game_mode {
            GameMode::StartScreen => {
                // Show class selection screen
                let start_result = crate::ui::run_start_screen(
                    egui_glow,
                    window,
                    tileset,
                    ui_icons,
                    &mut self.selected_class,
                );

                // Return start_game action if player clicked Start
                let mut actions = crate::ui::UiActions::default();
                actions.start_game = start_result;
                actions
            }
            GameMode::Playing => {
                let state = self.state.as_ref().expect("State should exist when playing");
                let ui_state = self.ui_state.as_mut().expect("UI state should exist when playing");

                crate::ui::run_ui(
                    egui_glow,
                    window,
                    &state.world,
                    state.player_entity,
                    &state.grid,
                    ui_state,
                    &mut self.dev_menu,
                    camera,
                    tileset,
                    ui_icons,
                    &self.vfx.effects,
                    self.input.targeting_mode.as_ref(),
                    self.input.mouse_pos,
                    state.game_clock.time,
                )
            }
        }
    }

    // --- Private methods ---

    /// Process input. Only called when playing (state must exist).
    fn process_input(&mut self, camera: &mut Camera) -> InputResult {
        let state = self.state.as_mut().expect("process_input called without state");
        let ui_state = self.ui_state.as_mut().expect("process_input called without ui_state");

        let frame = input::process_frame(
            &mut self.input,
            &state.world,
            &state.grid,
            camera,
            state.player_entity,
        );

        let mut result = InputResult::default();

        // UI toggles
        result.toggle_fullscreen = frame.toggle_fullscreen;
        if frame.toggle_inventory {
            ui_state.toggle_inventory();
        }
        if frame.toggle_grid_lines {
            ui_state.toggle_grid_lines();
        }

        // Enter key: container interaction (chests, bones, ground items)
        if frame.enter_pressed {
            match crate::game::handle_enter_key_container(
                &mut state.world,
                state.player_entity,
                ui_state.open_chest,
                &mut self.events,
            ) {
                crate::game::ContainerAction::TookAll(_) => {
                    ui_state.close_chest();
                    // Clean up empty ground item piles
                    systems::cleanup_empty_ground_piles(&mut state.world);
                }
                crate::game::ContainerAction::Opened(_) => {
                    let _ = process_events(
                        &mut self.events,
                        &mut state.world,
                        &state.grid,
                        &mut self.vfx,
                        ui_state,
                        state.player_entity,
                    );
                }
                crate::game::ContainerAction::None => {}
            }
        }

        // Player dead - just handle drag
        if frame.player_dead {
            input::process_mouse_drag(&mut self.input, camera, ui_state.show_inventory);
            return result;
        }

        // Class ability activation (Q key)
        if frame.ability_pressed {
            activate_class_ability(
                &mut state.world,
                &state.grid,
                state.player_entity,
                &mut state.game_clock,
                &mut state.action_scheduler,
                &mut self.events,
                &mut self.vfx,
                ui_state,
            );
        }

        // Execute player intent
        if let Some(intent) = frame.player_intent {
            if let Some(item_index) = frame.item_to_remove {
                systems::remove_item_from_inventory(
                    &mut state.world,
                    state.player_entity,
                    item_index,
                );
            }

            let turn_result = execute_player_intent(
                &mut state.world,
                &state.grid,
                state.player_entity,
                intent,
                &mut state.game_clock,
                &mut state.action_scheduler,
                &mut self.events,
                &mut self.vfx,
                ui_state,
            );

            // Collect skeleton spawn positions before floor transition might invalidate state
            let skeleton_spawns = turn_result.skeleton_spawns.clone();

            match turn_result.turn_result {
                TurnResult::Started => {
                    if !frame.from_keyboard {
                        self.input.consume_step();
                        if self.input.has_arrived() {
                            self.input.clear_destination();
                            if let Some(container_id) = systems::find_container_at_player(
                                &state.world,
                                state.player_entity,
                            ) {
                                self.events.push(crate::events::GameEvent::ContainerOpened {
                                    container: container_id,
                                    opener: state.player_entity,
                                });
                            }
                        }
                    }
                    if let Some(direction) = turn_result.floor_transition {
                        self.handle_floor_transition(direction, camera);
                    }
                }
                TurnResult::Blocked | TurnResult::NotReady => {
                    self.input.clear_path();
                }
            }

            if turn_result.should_interrupt_path() {
                self.input.clear_path();
            }

            // Spawn skeletons from opened coffins
            if !skeleton_spawns.is_empty() {
                let state = self.state.as_mut().expect("State should exist");
                for (x, y) in &skeleton_spawns {
                    let skeleton = spawning::enemies::SKELETON.spawn(&mut state.world, *x, *y);
                    let mut rng = rand::thread_rng();
                    initialization::initialize_single_ai_actor(
                        &mut state.world,
                        &state.grid,
                        skeleton,
                        state.player_entity,
                        &state.game_clock,
                        &mut state.action_scheduler,
                        &mut self.events,
                        &mut rng,
                    );
                }
                // Close loot UI - player must deal with skeleton first
                if let Some(ui_state) = self.ui_state.as_mut() {
                    ui_state.close_chest();
                }
            }
        }

        // Mouse drag for camera
        let show_inv = self.ui_state.as_ref().map(|u| u.show_inventory).unwrap_or(false);
        input::process_mouse_drag(&mut self.input, camera, show_inv);

        result
    }

    fn handle_dev_spawn(&mut self, camera: &Camera) {
        let Some(tool) = self.dev_menu.selected_tool else {
            return;
        };
        let Some(ref mut state) = self.state else {
            return;
        };

        let needs_vfx = dev_spawning::spawn_at_cursor(
            tool,
            self.input.mouse_pos,
            camera,
            &mut state.world,
            &mut state.grid,
            state.player_entity,
            &state.game_clock,
            &mut state.action_scheduler,
            &mut self.events,
        );

        if needs_vfx {
            let world_pos = camera.screen_to_world(self.input.mouse_pos.0, self.input.mouse_pos.1);
            let tile_x = world_pos.x.round() as i32;
            let tile_y = world_pos.y.round() as i32;
            dev_spawning::spawn_vfx_for_tool(tool, tile_x, tile_y, &mut self.vfx);
        }
    }

    /// Try to use the player's class ability (called from UI button)
    fn try_use_class_ability(&mut self) {
        let Some(ref mut state) = self.state else {
            return;
        };
        let Some(ref mut ui_state) = self.ui_state else {
            return;
        };

        activate_class_ability(
            &mut state.world,
            &state.grid,
            state.player_entity,
            &mut state.game_clock,
            &mut state.action_scheduler,
            &mut self.events,
            &mut self.vfx,
            ui_state,
        );
    }

    fn handle_floor_transition(
        &mut self,
        direction: crate::events::StairDirection,
        camera: &mut Camera,
    ) {
        let Some(ref mut state) = self.state else {
            return;
        };

        if !can_transition_floor(state.current_floor, direction) {
            return;
        }

        // Take ownership of grid for transition
        let current_grid = std::mem::replace(
            &mut state.grid,
            crate::grid::Grid::new(1, 1),
        );

        let result = handle_floor_transition(
            &mut state.world,
            current_grid,
            &mut state.floors,
            state.current_floor,
            direction,
            state.player_entity,
            &state.game_clock,
            &mut state.action_scheduler,
            &mut self.events,
        );

        state.grid = result.new_grid;
        state.current_floor = result.new_floor;

        self.input.clear_path();

        camera.set_tracking_target(glam::Vec2::new(
            result.player_visual_pos.0,
            result.player_visual_pos.1,
        ));
    }
}

/// Try to activate the player's class ability.
/// Returns true if the ability was successfully activated.
fn activate_class_ability(
    world: &mut hecs::World,
    grid: &crate::grid::Grid,
    player: Entity,
    game_clock: &mut crate::time_system::GameClock,
    action_scheduler: &mut crate::time_system::ActionScheduler,
    events: &mut EventQueue,
    vfx: &mut VfxManager,
    ui_state: &mut GameUiState,
) -> bool {
    // Check if player is idle
    let is_idle = world
        .get::<&Actor>(player)
        .map(|a| a.current_action.is_none())
        .unwrap_or(false);

    if !is_idle {
        return false;
    }

    // Get ability info
    let ability_info = world
        .get::<&ClassAbility>(player)
        .ok()
        .map(|a| (a.ability_type, a.is_ready(), a.ability_type.energy_cost()));

    let Some((ability_type, is_ready, energy_cost)) = ability_info else {
        return false;
    };

    if !is_ready {
        return false;
    }

    // Check if player can ever afford this (max_energy >= cost)
    let can_afford = world
        .get::<&Actor>(player)
        .map(|a| a.max_energy >= energy_cost)
        .unwrap_or(false);

    if !can_afford {
        return false;
    }

    // Wait for enough energy (this advances time, enemies may act)
    let mut rng = rand::thread_rng();
    let got_energy = simulation::wait_for_energy(
        world,
        grid,
        player,
        energy_cost,
        game_clock,
        action_scheduler,
        events,
        &mut rng,
    );

    if !got_energy {
        // Player died or something went wrong during wait
        let _ = process_events(events, world, grid, vfx, ui_state, player);
        return false;
    }

    // Determine action type based on ability
    let action_type = match ability_type {
        AbilityType::Cleave => ActionType::Cleave,
        AbilityType::Sprint => ActionType::ActivateSprint,
    };

    // Start the action
    let start_result = time_system::start_action(
        world,
        player,
        action_type,
        game_clock,
        action_scheduler,
    );

    if start_result.is_ok() {
        // Start cooldown
        if let Ok(mut ability) = world.get::<&mut ClassAbility>(player) {
            ability.start_cooldown();
        }

        // Advance time and process events
        simulation::advance_until_player_ready(
            world,
            grid,
            player,
            game_clock,
            action_scheduler,
            events,
            &mut rng,
        );
    }

    let _ = process_events(events, world, grid, vfx, ui_state, player);

    start_result.is_ok()
}

/// Result of input processing (internal)
#[derive(Default)]
struct InputResult {
    toggle_fullscreen: bool,
}
