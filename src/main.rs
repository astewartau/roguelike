#![allow(dead_code)]

mod app;
mod camera;
mod components;
mod constants;
mod dungeon_gen;
mod events;
mod fov;
mod game;
mod game_loop;
mod grid;
mod input;
mod pathfinding;
mod renderer;
mod spawning;
mod systems;
mod tile;
mod tileset;
mod time_system;
mod ui;
mod vfx;

use camera::Camera;
use constants::*;
use grid::Grid;
use hecs::World;
use renderer::Renderer;
use tileset::Tileset;
use std::sync::Arc;
use std::time::Instant;

use glutin::prelude::*;
use glutin::surface::WindowSurface;
use winit::application::ApplicationHandler;
use winit::event::{ElementState, MouseButton, MouseScrollDelta, WindowEvent};
use winit::event_loop::{ActiveEventLoop, EventLoop};
use winit::keyboard::{KeyCode, PhysicalKey};
use winit::window::{Window, WindowId};

use egui_glow::EguiGlow;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let event_loop = EventLoop::new()?;
    let mut app = App::new();
    event_loop.run_app(&mut app)?;
    Ok(())
}

struct App {
    state: Option<AppState>,
}

struct AppState {
    // Window and GL
    window: Window,
    gl_surface: glutin::surface::Surface<WindowSurface>,
    gl_context: glutin::context::PossiblyCurrentContext,
    gl: Arc<glow::Context>,
    egui_glow: EguiGlow,

    // Rendering
    camera: Camera,
    renderer: Renderer,
    tileset: Tileset,
    ui_icons: ui::UiIcons,

    // Game state
    grid: Grid,
    world: World,
    player_entity: hecs::Entity,
    vfx: vfx::VfxManager,
    events: events::EventQueue,

    // Time system
    game_clock: time_system::GameClock,
    action_scheduler: time_system::ActionScheduler,

    // UI state
    ui_state: ui::GameUiState,
    dev_menu: ui::DevMenu,

    // Input state
    input: input::InputState,

    // Timing
    last_frame_time: Instant,
}

impl App {
    fn new() -> Self {
        Self { state: None }
    }
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.state.is_some() {
            return;
        }

        // Create window and GL context
        let app::WindowContext {
            window,
            gl_surface,
            gl_context,
            gl,
            mut egui_glow,
        } = app::create_window(event_loop);

        // Initialize game state
        let size = window.inner_size();
        let mut camera = Camera::new(size.width as f32, size.height as f32);
        let grid = Grid::new(DUNGEON_DEFAULT_WIDTH, DUNGEON_DEFAULT_HEIGHT);
        let renderer = Renderer::new(gl.clone()).expect("Failed to create renderer");

        // Load tileset
        let tileset = Tileset::load(gl.clone(), std::path::Path::new("assets/minirogue-all.tsj"))
            .expect("Failed to load tileset");

        // Register tileset texture with egui and create UI icons
        let tileset_egui_id = egui_glow.painter.register_native_texture(tileset.texture);
        let ui_icons = ui::UiIcons::new(&tileset, tileset_egui_id);

        // Initialize game world
        let (mut world, player_entity, player_start) = game::init_world(&grid);
        game::setup_camera(&mut camera, &player_start);

        // Initialize time system
        let game_clock = time_system::GameClock::new();
        let mut action_scheduler = time_system::ActionScheduler::new();
        let mut event_queue = events::EventQueue::new();

        // Initialize AI entities with their first actions
        let mut rng = rand::thread_rng();
        game::initialize_ai_actors(
            &mut world,
            &grid,
            player_entity,
            &game_clock,
            &mut action_scheduler,
            &mut event_queue,
            &mut rng,
        );

        self.state = Some(AppState {
            window,
            gl_surface,
            gl_context,
            gl,
            egui_glow,
            camera,
            renderer,
            tileset,
            ui_icons,
            grid,
            world,
            player_entity,
            vfx: vfx::VfxManager::new(),
            events: event_queue,
            game_clock,
            action_scheduler,
            ui_state: ui::GameUiState::new(player_entity),
            dev_menu: ui::DevMenu::new(),
            input: input::InputState::new(),
            last_frame_time: Instant::now(),
        });
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        let state = match &mut self.state {
            Some(s) => s,
            None => return,
        };

        // Let egui handle the event first
        let egui_consumed = state.egui_glow.on_window_event(&state.window, &event);

        match event {
            WindowEvent::CloseRequested => {
                event_loop.exit();
            }
            WindowEvent::Resized(size) => {
                app::resize_surface(&state.gl_surface, &state.gl_context, size.width, size.height);
                state.camera.viewport_width = size.width as f32;
                state.camera.viewport_height = size.height as f32;
            }
            WindowEvent::KeyboardInput { event, .. } => {
                if !egui_consumed.consumed {
                    if let PhysicalKey::Code(key) = event.physical_key {
                        match event.state {
                            ElementState::Pressed => {
                                if key == KeyCode::Escape {
                                    event_loop.exit();
                                }
                                if key == KeyCode::Backquote {
                                    state.dev_menu.toggle();
                                }
                                state.input.keys_pressed.insert(key);
                            }
                            ElementState::Released => {
                                state.input.keys_pressed.remove(&key);
                            }
                        }
                    }
                }
            }
            WindowEvent::CursorMoved { position, .. } => {
                state.input.last_mouse_pos = state.input.mouse_pos;
                state.input.mouse_pos = (position.x as f32, position.y as f32);
            }
            WindowEvent::MouseInput { state: btn_state, button, .. } => {
                if !egui_consumed.consumed && button == MouseButton::Left {
                    let was_down = state.input.mouse_down;
                    state.input.mouse_down = btn_state == ElementState::Pressed;

                    if btn_state == ElementState::Released {
                        // Check if this was a click (not a drag)
                        let dx = state.input.mouse_pos.0 - state.input.last_mouse_pos.0;
                        let dy = state.input.mouse_pos.1 - state.input.last_mouse_pos.1;
                        let was_drag = was_down
                            && (dx.abs() > CLICK_DRAG_THRESHOLD || dy.abs() > CLICK_DRAG_THRESHOLD);

                        if !was_drag {
                            // Check if a dev tool is active
                            if state.dev_menu.has_active_tool() {
                                state.handle_dev_spawn();
                            } else {
                                input::handle_click_to_move(
                                    &mut state.input,
                                    &state.camera,
                                    &state.world,
                                    &state.grid,
                                    state.player_entity,
                                );
                            }
                        }

                        state.camera.release_pan();
                    }
                }
            }
            WindowEvent::MouseWheel { delta, .. } => {
                if !egui_consumed.consumed {
                    let scroll = match delta {
                        MouseScrollDelta::LineDelta(_, y) => y * 2.0,
                        MouseScrollDelta::PixelDelta(pos) => pos.y as f32 * 0.1,
                    };
                    state.camera.add_zoom_impulse(
                        scroll,
                        state.input.mouse_pos.0,
                        state.input.mouse_pos.1,
                    );
                }
            }
            WindowEvent::RedrawRequested => {
                state.update_and_render();
                state.window.request_redraw();
            }
            _ => {}
        }
    }

    fn about_to_wait(&mut self, _event_loop: &ActiveEventLoop) {
        if let Some(state) = &self.state {
            state.window.request_redraw();
        }
    }
}

impl AppState {
    fn update_and_render(&mut self) {
        let current_time = Instant::now();
        let dt = (current_time - self.last_frame_time).as_secs_f32();
        self.last_frame_time = current_time;

        // Handle input
        self.handle_input();

        // Update animations
        systems::update_lunge_animations(&mut self.world, dt);
        self.vfx.update(dt);

        // Remove dead entities (turn into lootable bones, grant XP)
        let mut rng = rand::thread_rng();
        systems::remove_dead_entities(
            &mut self.world,
            self.player_entity,
            &mut rng,
            &mut self.events,
            Some(&mut self.action_scheduler),
        );
        // Process any events from remove_dead_entities (death VFX, etc.)
        game_loop::process_events(&mut self.events, &mut self.world, &mut self.vfx, &mut self.ui_state);

        // Lerp all visual positions toward logical positions
        systems::visual_lerp(&mut self.world, dt);

        // Update camera to follow player's visual position
        if let Ok(vis_pos) = self.world.get::<&components::VisualPosition>(self.player_entity) {
            self.camera
                .set_tracking_target(glam::Vec2::new(vis_pos.x, vis_pos.y));
        }

        // Update camera (pass mouse_down so momentum doesn't apply while dragging)
        self.camera.update(dt, self.input.mouse_down);

        // Update FOV
        systems::update_fov(&self.world, &mut self.grid, self.player_entity, FOV_RADIUS);

        // Collect entities for rendering
        let entities_to_render =
            systems::collect_renderables(&self.world, &self.grid, self.player_entity);

        // Run UI
        let ui_actions = self.run_ui();

        // Handle UI actions
        self.process_ui_actions(ui_actions);

        // Render
        unsafe {
            use glow::HasContext;
            self.gl.clear_color(0.1, 0.1, 0.1, 1.0);
            self.gl.clear(glow::COLOR_BUFFER_BIT);
        }

        self.renderer
            .render(&self.camera, &self.grid, &self.tileset, self.ui_state.show_grid_lines)
            .unwrap();
        self.renderer
            .render_entities(&self.camera, &entities_to_render, &self.tileset)
            .unwrap();
        self.renderer.render_vfx(&self.camera, &self.vfx.effects);

        // Render egui
        self.egui_glow.paint(&self.window);

        // Swap buffers
        self.gl_surface.swap_buffers(&self.gl_context).unwrap();
    }

    fn run_ui(&mut self) -> ui::UiActions {
        let mut actions = ui::UiActions::default();

        // Get status bar data
        let status_data = ui::get_status_bar_data(&self.world, self.player_entity);

        // Get loot window data if chest is open
        let loot_data = ui::get_loot_window_data(
            &self.world,
            self.ui_state.open_chest,
            self.camera.viewport_width,
            self.camera.viewport_height,
        );

        let icons = &self.ui_icons;
        let show_inventory = self.ui_state.show_inventory;
        let world = &self.world;
        let player_entity = self.player_entity;
        let viewport_width = self.camera.viewport_width;
        let viewport_height = self.camera.viewport_height;
        let vfx_effects = &self.vfx.effects;
        let camera = &self.camera;
        let tileset = &self.tileset;
        let dev_menu = &mut self.dev_menu;

        self.egui_glow.run(&self.window, |ctx| {
            // Status bar (always visible)
            ui::draw_status_bar(ctx, &status_data, icons.tileset_texture_id, icons.coins_uv);

            // Floating damage numbers
            ui::draw_damage_numbers(ctx, vfx_effects, camera);

            // Developer menu
            ui::draw_dev_menu(ctx, dev_menu, icons.tileset_texture_id, tileset);

            // Loot window (if chest is open)
            if let Some(ref data) = loot_data {
                ui::draw_loot_window(
                    ctx,
                    data,
                    icons.tileset_texture_id,
                    icons.coins_uv,
                    icons.potion_uv,
                    &mut actions,
                );
            }

            // Inventory window (if toggled)
            if show_inventory {
                let inv_data = ui::InventoryWindowData {
                    viewport_width,
                    viewport_height,
                };
                ui::draw_inventory_window(
                    ctx,
                    world,
                    player_entity,
                    &inv_data,
                    icons.tileset_texture_id,
                    icons.sword_uv,
                    icons.coins_uv,
                    icons.potion_uv,
                    &mut actions,
                );
            }
        });

        actions
    }

    fn process_ui_actions(&mut self, actions: ui::UiActions) {
        // Handle chest/loot interactions
        if let Some(chest_id) = self.ui_state.open_chest {
            if actions.chest_take_all || actions.close_chest {
                if actions.chest_take_all {
                    systems::take_all_from_container(&mut self.world, self.player_entity, chest_id, Some(&mut self.events));
                }
                self.ui_state.close_chest();
            } else if actions.chest_take_gold {
                systems::take_gold_from_container(&mut self.world, self.player_entity, chest_id, Some(&mut self.events));
            } else if let Some(item_index) = actions.chest_item_to_take {
                systems::take_item_from_container(
                    &mut self.world,
                    self.player_entity,
                    chest_id,
                    item_index,
                    Some(&mut self.events),
                );
            }
        }

        // Use item if clicked
        if let Some(item_index) = actions.item_to_use {
            systems::use_item(&mut self.world, self.player_entity, item_index);
        }
    }

    fn handle_input(&mut self) {
        // Process keyboard input (pure input handling - no game logic)
        let result = input::process_keyboard(&mut self.input);

        // Handle toggle actions
        if result.toggle_fullscreen {
            use winit::window::Fullscreen;
            let fullscreen = if self.window.fullscreen().is_some() {
                None
            } else {
                Some(Fullscreen::Borderless(None))
            };
            self.window.set_fullscreen(fullscreen);
        }

        if result.toggle_inventory {
            self.ui_state.toggle_inventory();
        }

        if result.toggle_grid_lines {
            self.ui_state.toggle_grid_lines();
        }

        // Enter key: Take All if chest open, otherwise open chest at player position
        if result.enter_pressed {
            if let Some(chest_id) = self.ui_state.open_chest {
                systems::take_all_from_container(&mut self.world, self.player_entity, chest_id, Some(&mut self.events));
                self.ui_state.close_chest();
            } else if let Some(container_id) =
                systems::find_container_at_player(&self.world, self.player_entity)
            {
                self.events.push(crate::events::GameEvent::ContainerOpened {
                    container: container_id,
                    opener: self.player_entity,
                });
                // Process immediately so UI updates this frame
                game_loop::process_events(&mut self.events, &mut self.world, &mut self.vfx, &mut self.ui_state);
            }
        }

        // Check if player is dead - no movement allowed
        let is_dead = self
            .world
            .get::<&components::Health>(self.player_entity)
            .map(|h| h.is_dead())
            .unwrap_or(true);

        if is_dead {
            self.input.clear_path();
            input::process_mouse_drag(&mut self.input, &mut self.camera, self.ui_state.show_inventory);
            return;
        }

        // Determine movement intent (keyboard takes priority over click-to-move)
        let (movement_intent, from_keyboard) = if let Some((dx, dy)) = result.movement {
            // Keyboard movement cancels click-to-move path
            self.input.clear_path();
            (Some((dx, dy)), true)
        } else {
            // Try click-to-move path
            (input::get_path_movement(&self.input, &self.world, self.player_entity), false)
        };

        // Execute movement if we have an intent
        if let Some((dx, dy)) = movement_intent {
            // Validate target tile is walkable
            let player_pos = self
                .world
                .get::<&components::Position>(self.player_entity)
                .ok()
                .map(|p| (p.x, p.y));

            let tile_walkable = player_pos
                .and_then(|(px, py)| self.grid.get(px + dx, py + dy))
                .map(|t| t.tile_type.is_walkable())
                .unwrap_or(false);

            // For click-to-move, check if this would open a chest and stop instead
            // (chests require explicit keyboard interaction)
            if !from_keyboard {
                let action_type = game_loop::peek_action_type(
                    &self.world,
                    &self.grid,
                    self.player_entity,
                    dx,
                    dy,
                );
                if matches!(action_type, components::ActionType::OpenChest { .. }) {
                    // Stop at the chest, don't auto-open it
                    self.input.clear_path();
                    input::process_mouse_drag(&mut self.input, &mut self.camera, self.ui_state.show_inventory);
                    return;
                }
            }

            if tile_walkable {
                // Execute the turn via game_loop (handles time advancement, AI, events, UI state)
                let turn_result = game_loop::execute_player_turn(
                    &mut self.world,
                    &self.grid,
                    self.player_entity,
                    dx,
                    dy,
                    &mut self.game_clock,
                    &mut self.action_scheduler,
                    &mut self.events,
                    &mut self.vfx,
                    &mut self.ui_state,
                );

                // Handle path consumption based on result
                match turn_result {
                    game_loop::TurnResult::Started => {
                        // For path-following, consume the step we just took
                        if !from_keyboard {
                            self.input.consume_step();
                        }
                    }
                    game_loop::TurnResult::Blocked | game_loop::TurnResult::NotReady => {
                        // Clear path on blocked movement
                        self.input.clear_path();
                    }
                }
            }
        }

        // Process mouse drag for camera panning
        input::process_mouse_drag(&mut self.input, &mut self.camera, self.ui_state.show_inventory);
    }

    fn handle_dev_spawn(&mut self) {
        let Some(tool) = self.dev_menu.selected_tool else {
            return;
        };

        // Convert mouse position to world coordinates
        let world_pos = self.camera.screen_to_world(
            self.input.mouse_pos.0,
            self.input.mouse_pos.1,
        );

        // Round to get tile coordinates
        let tile_x = world_pos.x.round() as i32;
        let tile_y = world_pos.y.round() as i32;

        // Check if the tile is walkable
        let Some(tile) = self.grid.get(tile_x, tile_y) else {
            return;
        };
        if !tile.tile_type.is_walkable() {
            return;
        }

        // Check if something is already blocking this tile
        let is_blocked = self.world.query::<(&components::Position, &components::BlocksMovement)>()
            .iter()
            .any(|(_, (pos, _))| pos.x == tile_x && pos.y == tile_y);
        if is_blocked {
            return;
        }

        // Spawn the entity based on the selected tool
        match tool {
            ui::DevTool::SpawnChest => {
                let pos = components::Position::new(tile_x, tile_y);
                self.world.spawn((
                    pos,
                    components::VisualPosition::from_position(&pos),
                    components::Sprite::new(tile::tile_ids::CHEST_CLOSED),
                    components::Container::new(vec![components::ItemType::HealthPotion]),
                    components::BlocksMovement,
                ));
            }
            ui::DevTool::SpawnEnemy => {
                let enemy = spawning::enemies::SKELETON.spawn(&mut self.world, tile_x, tile_y);
                // Initialize the AI actor's first action
                let mut rng = rand::thread_rng();
                game::initialize_single_ai_actor(
                    &mut self.world,
                    &self.grid,
                    enemy,
                    self.player_entity,
                    &self.game_clock,
                    &mut self.action_scheduler,
                    &mut self.events,
                    &mut rng,
                );
            }
        }
    }
}
