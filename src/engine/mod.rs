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

use crate::camera::Camera;
use crate::events::EventQueue;
use crate::input::{self, InputState, TargetingMode};
use crate::systems;
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

/// Result of a game tick - contains everything needed for rendering
pub struct TickResult {
    /// Entities to render
    pub entities: Vec<crate::systems::RenderEntity>,
    /// Window action to perform (if any)
    pub window_action: Option<WindowAction>,
}

/// The game engine - owns all game state and simulation logic.
pub struct GameEngine {
    /// Core game state (world, grid, floors, time)
    pub state: GameState,

    /// Visual effects manager
    pub vfx: VfxManager,

    /// Event queue for game events
    pub events: EventQueue,

    /// Input state tracking
    pub input: InputState,

    /// UI state (inventory open, chest open, etc.)
    pub ui_state: GameUiState,

    /// Developer menu state
    pub dev_menu: DevMenu,
}

impl GameEngine {
    /// Create a new game engine with an initialized world.
    pub fn new() -> Self {
        let state = GameState::new();
        let ui_state = GameUiState::new(state.player_entity);

        Self {
            state,
            vfx: VfxManager::new(),
            events: EventQueue::new(),
            input: InputState::new(),
            ui_state,
            dev_menu: DevMenu::new(),
        }
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

                        if !was_drag {
                            if self.input.is_targeting() {
                                self.input.pending_left_click = true;
                            } else if self.dev_menu.has_active_tool() {
                                self.handle_dev_spawn(camera);
                            } else {
                                input::handle_click_to_move(
                                    &mut self.input,
                                    camera,
                                    &self.state.world,
                                    &self.state.grid,
                                    self.state.player_entity,
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
    pub fn tick(&mut self, dt: f32, camera: &mut Camera) -> TickResult {
        let mut window_action = None;

        // Handle input
        let input_result = self.process_input(camera);
        if input_result.toggle_fullscreen {
            window_action = Some(WindowAction::ToggleFullscreen);
        }

        // Update animations
        systems::update_lunge_animations(&mut self.state.world, dt);
        self.vfx.update(dt);

        // Remove dead entities
        let mut rng = rand::thread_rng();
        systems::remove_dead_entities(
            &mut self.state.world,
            self.state.player_entity,
            &mut rng,
            &mut self.events,
            Some(&mut self.state.action_scheduler),
        );

        // Process events from remove_dead_entities
        let event_result = process_events(
            &mut self.events,
            &mut self.state.world,
            &self.state.grid,
            &mut self.vfx,
            &mut self.ui_state,
            self.state.player_entity,
        );
        if let Some(direction) = event_result.floor_transition {
            self.handle_floor_transition(direction, camera);
        }
        if event_result.should_interrupt_path() {
            self.input.clear_path();
        }

        // Visual lerping
        systems::visual_lerp(&mut self.state.world, dt);
        systems::lerp_projectiles_realtime(
            &mut self.state.world,
            dt,
            crate::constants::ARROW_SPEED,
        );

        // Projectile cleanup
        let finished = systems::cleanup_finished_projectiles(&self.state.world);
        systems::despawn_projectiles(&mut self.state.world, finished);

        // Update camera tracking
        if let Ok(vis_pos) = self.state.world.get::<&crate::components::VisualPosition>(self.state.player_entity) {
            camera.set_tracking_target(glam::Vec2::new(vis_pos.x, vis_pos.y));
        }

        // Update camera
        camera.update(dt, self.input.mouse_down);

        // Update FOV
        systems::update_fov(
            &self.state.world,
            &mut self.state.grid,
            self.state.player_entity,
            crate::constants::FOV_RADIUS,
            self.state.game_clock.time,
        );

        // Collect renderables
        let entities = systems::collect_renderables(
            &self.state.world,
            &self.state.grid,
            self.state.player_entity,
        );

        TickResult {
            entities,
            window_action,
        }
    }

    /// Process UI actions from the UI layer.
    pub fn process_ui_actions(&mut self, actions: &UiActions) {
        let ui_result = process_ui_actions(
            &mut self.state.world,
            &mut self.state.grid,
            self.state.player_entity,
            actions,
            &mut self.dev_menu,
            &self.ui_state,
            &mut self.events,
            self.state.game_clock.time,
        );

        // Apply UI state changes
        if let Some(targeting) = ui_result.enter_targeting {
            self.input.targeting_mode = Some(targeting);
        }
        if ui_result.close_inventory {
            self.ui_state.show_inventory = false;
        }
        if ui_result.close_context_menu {
            self.ui_state.close_context_menu();
        }
        if ui_result.close_chest {
            self.ui_state.close_chest();
        }
        if ui_result.close_dialogue {
            self.ui_state.close_dialogue();
        }
    }

    /// Get the grid for rendering.
    pub fn grid(&self) -> &crate::grid::Grid {
        &self.state.grid
    }

    /// Get VFX effects for rendering.
    pub fn vfx_effects(&self) -> &[VisualEffect] {
        &self.vfx.effects
    }

    /// Get fire effects for rendering.
    pub fn fires(&self) -> &[FireEffect] {
        &self.vfx.fires
    }

    /// Get the current game time.
    #[allow(dead_code)] // Public API for external callers
    pub fn game_time(&self) -> f32 {
        self.state.game_clock.time
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

    /// Get the player entity.
    #[allow(dead_code)] // Public API for external callers
    pub fn player_entity(&self) -> Entity {
        self.state.player_entity
    }

    /// Get a reference to the ECS world.
    #[allow(dead_code)] // Public API for external callers
    pub fn world(&self) -> &hecs::World {
        &self.state.world
    }

    /// Should show grid lines?
    pub fn show_grid_lines(&self) -> bool {
        self.ui_state.show_grid_lines
    }

    /// Run the UI and return actions. This handles the borrowing internally.
    pub fn run_ui(
        &mut self,
        egui_glow: &mut egui_glow::EguiGlow,
        window: &winit::window::Window,
        camera: &crate::camera::Camera,
        tileset: &crate::tileset::Tileset,
        ui_icons: &crate::ui::UiIcons,
    ) -> crate::ui::UiActions {
        crate::ui::run_ui(
            egui_glow,
            window,
            &self.state.world,
            self.state.player_entity,
            &self.state.grid,
            &mut self.ui_state,
            &mut self.dev_menu,
            camera,
            tileset,
            ui_icons,
            &self.vfx.effects,
            self.input.targeting_mode.as_ref(),
            self.input.mouse_pos,
            self.state.game_clock.time,
        )
    }

    // --- Private methods ---

    fn process_input(&mut self, camera: &mut Camera) -> InputResult {
        let frame = input::process_frame(
            &mut self.input,
            &self.state.world,
            &self.state.grid,
            camera,
            self.state.player_entity,
        );

        let mut result = InputResult::default();

        // UI toggles
        result.toggle_fullscreen = frame.toggle_fullscreen;
        if frame.toggle_inventory {
            self.ui_state.toggle_inventory();
        }
        if frame.toggle_grid_lines {
            self.ui_state.toggle_grid_lines();
        }

        // Enter key: container interaction
        if frame.enter_pressed {
            match crate::game::handle_enter_key_container(
                &mut self.state.world,
                self.state.player_entity,
                self.ui_state.open_chest,
                &mut self.events,
            ) {
                crate::game::ContainerAction::TookAll(_) => {
                    self.ui_state.close_chest();
                }
                crate::game::ContainerAction::Opened(_) => {
                    let _ = process_events(
                        &mut self.events,
                        &mut self.state.world,
                        &self.state.grid,
                        &mut self.vfx,
                        &mut self.ui_state,
                        self.state.player_entity,
                    );
                }
                crate::game::ContainerAction::None => {}
            }
        }

        // Player dead - just handle drag
        if frame.player_dead {
            input::process_mouse_drag(&mut self.input, camera, self.ui_state.show_inventory);
            return result;
        }

        // Execute player intent
        if let Some(intent) = frame.player_intent {
            if let Some(item_index) = frame.item_to_remove {
                systems::remove_item_from_inventory(
                    &mut self.state.world,
                    self.state.player_entity,
                    item_index,
                );
            }

            let turn_result = execute_player_intent(
                &mut self.state.world,
                &self.state.grid,
                self.state.player_entity,
                intent,
                &mut self.state.game_clock,
                &mut self.state.action_scheduler,
                &mut self.events,
                &mut self.vfx,
                &mut self.ui_state,
            );

            match turn_result.turn_result {
                TurnResult::Started => {
                    if !frame.from_keyboard {
                        self.input.consume_step();
                        if self.input.has_arrived() {
                            self.input.clear_destination();
                            if let Some(container_id) = systems::find_container_at_player(
                                &self.state.world,
                                self.state.player_entity,
                            ) {
                                self.events.push(crate::events::GameEvent::ContainerOpened {
                                    container: container_id,
                                    opener: self.state.player_entity,
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
        }

        // Mouse drag for camera
        input::process_mouse_drag(&mut self.input, camera, self.ui_state.show_inventory);

        result
    }

    fn handle_dev_spawn(&mut self, camera: &Camera) {
        let Some(tool) = self.dev_menu.selected_tool else {
            return;
        };

        let needs_vfx = dev_spawning::spawn_at_cursor(
            tool,
            self.input.mouse_pos,
            camera,
            &mut self.state.world,
            &mut self.state.grid,
            self.state.player_entity,
            &self.state.game_clock,
            &mut self.state.action_scheduler,
            &mut self.events,
        );

        if needs_vfx {
            let world_pos = camera.screen_to_world(self.input.mouse_pos.0, self.input.mouse_pos.1);
            let tile_x = world_pos.x.round() as i32;
            let tile_y = world_pos.y.round() as i32;
            dev_spawning::spawn_vfx_for_tool(tool, tile_x, tile_y, &mut self.vfx);
        }
    }

    fn handle_floor_transition(
        &mut self,
        direction: crate::events::StairDirection,
        camera: &mut Camera,
    ) {
        if !can_transition_floor(self.state.current_floor, direction) {
            return;
        }

        // Take ownership of grid for transition
        let current_grid = std::mem::replace(
            &mut self.state.grid,
            crate::grid::Grid::new(1, 1),
        );

        let result = handle_floor_transition(
            &mut self.state.world,
            current_grid,
            &mut self.state.floors,
            self.state.current_floor,
            direction,
            self.state.player_entity,
            &self.state.game_clock,
            &mut self.state.action_scheduler,
            &mut self.events,
        );

        self.state.grid = result.new_grid;
        self.state.current_floor = result.new_floor;

        self.input.clear_path();

        camera.set_tracking_target(glam::Vec2::new(
            result.player_visual_pos.0,
            result.player_visual_pos.1,
        ));
    }
}

/// Result of input processing (internal)
#[derive(Default)]
struct InputResult {
    toggle_fullscreen: bool,
}
