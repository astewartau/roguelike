#![allow(dead_code)]

mod actions;
mod camera;
mod components;
mod constants;
mod dungeon_gen;
mod events;
mod fov;
mod game;
mod grid;
mod input;
mod pathfinding;
mod renderer;
mod spawning;
mod systems;
mod tile;
mod tileset;
mod ui;
mod vfx;

use camera::Camera;
use constants::*;
use grid::Grid;
use hecs::World;
use renderer::Renderer;
use tileset::Tileset;
use std::ffi::CString;
use std::num::NonZeroU32;
use std::sync::Arc;
use std::time::Instant;

use glutin::config::ConfigTemplateBuilder;
use glutin::context::{ContextApi, ContextAttributesBuilder, Version};
use glutin::display::GetGlDisplay;
use glutin::prelude::*;
use glutin::surface::{SurfaceAttributesBuilder, WindowSurface};
use glutin_winit::DisplayBuilder;
use raw_window_handle::HasWindowHandle;
use winit::application::ApplicationHandler;
use winit::dpi::PhysicalSize;
use winit::event::{ElementState, MouseButton, MouseScrollDelta, WindowEvent};
use winit::event_loop::{ActiveEventLoop, EventLoop};
use winit::keyboard::{KeyCode, PhysicalKey};
use winit::window::{Window, WindowAttributes, WindowId};

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

    // UI state
    show_inventory: bool,
    open_chest: Option<hecs::Entity>,
    show_grid_lines: bool,
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

        // Create window
        let window_attrs = WindowAttributes::default()
            .with_title("Grid Roguelike")
            .with_inner_size(PhysicalSize::new(WINDOW_DEFAULT_WIDTH, WINDOW_DEFAULT_HEIGHT))
            .with_resizable(true);

        let template = ConfigTemplateBuilder::new().with_alpha_size(8);
        let display_builder = DisplayBuilder::new().with_window_attributes(Some(window_attrs));

        let (window, gl_config) = display_builder
            .build(event_loop, template, |configs| {
                configs
                    .reduce(|accum, config| {
                        if config.num_samples() > accum.num_samples() {
                            config
                        } else {
                            accum
                        }
                    })
                    .unwrap()
            })
            .expect("Failed to create window");

        let window = window.expect("Failed to create window");
        let window_handle = window.window_handle().unwrap();
        let gl_display = gl_config.display();

        let context_attrs = ContextAttributesBuilder::new()
            .with_context_api(ContextApi::OpenGl(Some(Version::new(3, 3))))
            .build(Some(window_handle.as_raw()));

        let gl_context = unsafe {
            gl_display
                .create_context(&gl_config, &context_attrs)
                .expect("Failed to create OpenGL context")
        };

        let size = window.inner_size();
        let surface_attrs = SurfaceAttributesBuilder::<WindowSurface>::new().build(
            window_handle.as_raw(),
            NonZeroU32::new(size.width).unwrap(),
            NonZeroU32::new(size.height).unwrap(),
        );

        let gl_surface = unsafe {
            gl_display
                .create_window_surface(&gl_config, &surface_attrs)
                .expect("Failed to create surface")
        };

        let gl_context = gl_context
            .make_current(&gl_surface)
            .expect("Failed to make context current");

        let gl = Arc::new(unsafe {
            glow::Context::from_loader_function(|s| {
                let s = CString::new(s).unwrap();
                gl_display.get_proc_address(&s) as *const _
            })
        });

        // Initialize egui
        let mut egui_glow = EguiGlow::new(event_loop, gl.clone(), None, None, false);

        // Initialize game state
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
        let (world, player_entity, player_start) = game::init_world(&grid);
        game::setup_camera(&mut camera, &player_start);

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
            events: events::EventQueue::new(),
            show_inventory: false,
            open_chest: None,
            show_grid_lines: false,
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
                if size.width > 0 && size.height > 0 {
                    state.gl_surface.resize(
                        &state.gl_context,
                        NonZeroU32::new(size.width).unwrap(),
                        NonZeroU32::new(size.height).unwrap(),
                    );
                    state.camera.viewport_width = size.width as f32;
                    state.camera.viewport_height = size.height as f32;
                }
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
        );
        input::process_events(&mut self.events, &mut self.vfx);

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
            .render(&self.camera, &self.grid, &self.tileset, self.show_grid_lines)
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
            self.open_chest,
            self.camera.viewport_width,
            self.camera.viewport_height,
        );

        let icons = &self.ui_icons;
        let show_inventory = self.show_inventory;
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
        if let Some(chest_id) = self.open_chest {
            if actions.chest_take_all || actions.close_chest {
                if actions.chest_take_all {
                    systems::take_all_from_container(&mut self.world, self.player_entity, chest_id);
                }
                self.open_chest = None;
            } else if actions.chest_take_gold {
                systems::take_gold_from_container(&mut self.world, self.player_entity, chest_id);
            } else if let Some(item_index) = actions.chest_item_to_take {
                systems::take_item_from_container(
                    &mut self.world,
                    self.player_entity,
                    chest_id,
                    item_index,
                );
            }
        }

        // Use item if clicked
        if let Some(item_index) = actions.item_to_use {
            game::use_item(&mut self.world, self.player_entity, item_index);
        }
    }

    fn handle_input(&mut self) {
        // Process keyboard input
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
            self.show_inventory = !self.show_inventory;
        }

        if result.toggle_grid_lines {
            self.show_grid_lines = !self.show_grid_lines;
        }

        // Enter key: Take All if chest open, otherwise loot at player position
        if result.enter_pressed {
            if let Some(chest_id) = self.open_chest {
                systems::take_all_from_container(&mut self.world, self.player_entity, chest_id);
                self.open_chest = None;
            } else if let Some(container_id) =
                systems::find_container_at_player(&self.world, self.player_entity)
            {
                self.open_chest = Some(container_id);
            }
        }

        // Check if player is dead - no movement allowed
        // If Health component is missing (removed on death), treat as dead
        let is_dead = self
            .world
            .get::<&components::Health>(self.player_entity)
            .map(|h| h.is_dead())
            .unwrap_or(true);

        if is_dead {
            // Clear any pending path and skip movement processing
            self.input.clear_path();
        } else if let Some((dx, dy)) = result.movement {
            // Keyboard movement cancels click-to-move path
            self.input.clear_path();

            // Get player position (copy to avoid borrow issues)
            let player_pos = self
                .world
                .get::<&components::Position>(self.player_entity)
                .ok()
                .map(|p| (p.x, p.y));

            if let Some((px, py)) = player_pos {
                let target_x = px + dx;
                let target_y = py + dy;
                if let Some(tile) = self.grid.get(target_x, target_y) {
                    if tile.tile_type.is_walkable() {
                        let result = input::run_ticks_until_player_acts(
                            &mut self.world,
                            &self.grid,
                            self.player_entity,
                            dx,
                            dy,
                            &mut self.events,
                            &mut self.vfx,
                        );
                        if let systems::MoveResult::OpenedChest(chest) = result {
                            self.open_chest = Some(chest);
                        } else if let systems::MoveResult::Moved = result {
                            self.open_chest = None;
                        }
                    }
                }
            }
        } else {
            // No keyboard movement - follow click-to-move path
            if let Some(container) = input::follow_player_path(
                &mut self.input,
                &mut self.world,
                &self.grid,
                self.player_entity,
                &mut self.events,
                &mut self.vfx,
            ) {
                self.open_chest = Some(container);
            }
        }

        // Process mouse drag for camera panning
        input::process_mouse_drag(&mut self.input, &mut self.camera, self.show_inventory);
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
                spawning::enemies::SKELETON.spawn(&mut self.world, tile_x, tile_y);
            }
        }
    }
}
