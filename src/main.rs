mod camera;
mod components;
mod dungeon_gen;
mod fov;
mod grid;
mod renderer;
mod tile;
mod tileset;

use camera::Camera;
use components::{Container, Health, Inventory, ItemType, Player, Position, Sprite, Stats};
use fov::FOV;
use glam::Vec2;
use grid::Grid;
use hecs::World;
use renderer::Renderer;
use tile::tile_ids;
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
    window: Window,
    gl_surface: glutin::surface::Surface<WindowSurface>,
    gl_context: glutin::context::PossiblyCurrentContext,
    gl: Arc<glow::Context>,
    egui_glow: EguiGlow,
    camera: Camera,
    grid: Grid,
    renderer: Renderer,
    tileset: Tileset,
    world: World,
    player_entity: hecs::Entity,
    last_frame_time: Instant,
    show_inventory: bool,
    // Input state
    keys_pressed: std::collections::HashSet<KeyCode>,
    mouse_pos: (f32, f32),
    mouse_down: bool,
    last_mouse_pos: (f32, f32),
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
            .with_inner_size(PhysicalSize::new(1280u32, 720u32))
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
        let egui_glow = EguiGlow::new(event_loop, gl.clone(), None, None, false);

        // Initialize game state
        let mut camera = Camera::new(size.width as f32, size.height as f32);
        let grid = Grid::new(100, 100);
        let renderer = Renderer::new(gl.clone()).expect("Failed to create renderer");

        // Load tileset
        let tileset = Tileset::load(gl.clone(), std::path::Path::new("assets/minirogue-all.tsj"))
            .expect("Failed to load tileset");

        // Create ECS world
        let mut world = World::new();

        // Find a walkable tile to spawn the player
        let mut player_start = Position::new(50, 50);
        'find_spawn: for y in 0..grid.height as i32 {
            for x in 0..grid.width as i32 {
                if let Some(tile) = grid.get(x, y) {
                    if tile.tile_type.is_walkable() {
                        player_start = Position::new(x, y);
                        break 'find_spawn;
                    }
                }
            }
        }

        // Spawn player
        let player_entity = world.spawn((
            player_start,
            Sprite::new(tile_ids::PLAYER),
            Player,
            {
                let mut health = Health::new(100);
                health.current = 50;
                health
            },
            Stats::new(10, 8, 12),
            Inventory::new(),
        ));

        camera.set_tracking_target(Vec2::new(player_start.x as f32, player_start.y as f32));

        // Spawn chests
        for (x, y) in &grid.chest_positions.clone() {
            world.spawn((
                Position::new(*x, *y),
                Sprite::new(tile_ids::CHEST_CLOSED),
                Container::new(vec![ItemType::HealthPotion]),
            ));
        }

        self.state = Some(AppState {
            window,
            gl_surface,
            gl_context,
            gl,
            egui_glow,
            camera,
            grid,
            renderer,
            tileset,
            world,
            player_entity,
            last_frame_time: Instant::now(),
            show_inventory: false,
            keys_pressed: std::collections::HashSet::new(),
            mouse_pos: (0.0, 0.0),
            mouse_down: false,
            last_mouse_pos: (0.0, 0.0),
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
                                state.keys_pressed.insert(key);
                            }
                            ElementState::Released => {
                                state.keys_pressed.remove(&key);
                            }
                        }
                    }
                }
            }
            WindowEvent::CursorMoved { position, .. } => {
                state.last_mouse_pos = state.mouse_pos;
                state.mouse_pos = (position.x as f32, position.y as f32);
            }
            WindowEvent::MouseInput { state: btn_state, button, .. } => {
                if !egui_consumed.consumed && button == MouseButton::Left {
                    state.mouse_down = btn_state == ElementState::Pressed;
                    if btn_state == ElementState::Released {
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
                    state.camera.add_zoom_impulse(scroll, state.mouse_pos.0, state.mouse_pos.1);
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

        // Update camera (pass mouse_down so momentum doesn't apply while dragging)
        self.camera.update(dt, self.mouse_down);

        // Update FOV
        if let Ok(player_pos) = self.world.get::<&Position>(self.player_entity) {
            for tile in &mut self.grid.tiles {
                tile.visible = false;
            }

            let visible_tiles = FOV::calculate(&self.grid, player_pos.x, player_pos.y, 10);
            for (x, y) in visible_tiles {
                if let Some(tile) = self.grid.get_mut(x, y) {
                    tile.visible = true;
                    tile.explored = true;
                }
            }
        }

        // Collect entities for rendering (only if in explored tiles, with fog)
        let mut entities_to_render = Vec::new();
        let mut player_render: Option<(Position, Sprite, f32)> = None;

        for (id, (pos, sprite)) in self.world.query::<(&Position, &Sprite)>().iter() {
            // Check tile visibility for fog of war
            let (is_explored, is_visible) = self.grid.get(pos.x, pos.y)
                .map(|tile| (tile.explored, tile.visible))
                .unwrap_or((false, false));

            if id == self.player_entity {
                player_render = Some((*pos, *sprite, 1.0)); // Player always full brightness
            } else if is_explored {
                let fog = if is_visible { 1.0 } else { 0.5 };
                entities_to_render.push((*pos, *sprite, fog));
            }
        }

        // Player is always rendered (they're always in an explored tile)
        if let Some(player) = player_render {
            entities_to_render.push(player);
        }

        // Get player health
        let health_percent = if let Ok(health) = self.world.get::<&Health>(self.player_entity) {
            health.percentage()
        } else {
            1.0
        };

        // Begin egui frame
        let mut item_to_use: Option<usize> = None;

        self.egui_glow.run(&self.window, |ctx| {
            // Health bar
            egui::Window::new("Health")
                .fixed_pos([10.0, 10.0])
                .fixed_size([200.0, 40.0])
                .title_bar(false)
                .show(ctx, |ui| {
                    ui.add(egui::ProgressBar::new(health_percent)
                        .text(format!("HP: {:.0}%", health_percent * 100.0)));
                });

            // Inventory window
            if self.show_inventory {
                egui::Window::new("Character")
                    .default_pos([self.camera.viewport_width / 2.0 - 300.0, self.camera.viewport_height / 2.0 - 250.0])
                    .default_size([600.0, 500.0])
                    .collapsible(false)
                    .resizable(true)
                    .show(ctx, |ui| {
                        if let Ok(stats) = self.world.get::<&Stats>(self.player_entity) {
                            ui.columns(2, |columns| {
                                // Left column: Stats
                                columns[0].vertical(|ui| {
                                    ui.heading("CHARACTER STATS");
                                    ui.separator();
                                    ui.add_space(10.0);
                                    ui.label(format!("Strength: {}", stats.strength));
                                    ui.add_space(5.0);
                                    ui.label(format!("Intelligence: {}", stats.intelligence));
                                    ui.add_space(5.0);
                                    ui.label(format!("Agility: {}", stats.agility));
                                    ui.add_space(10.0);
                                    ui.separator();

                                    let carry_capacity = stats.strength as f32 * 2.0;
                                    if let Ok(inventory) = self.world.get::<&Inventory>(self.player_entity) {
                                        ui.label(format!("Weight: {:.1} / {:.1} kg",
                                            inventory.current_weight_kg, carry_capacity));
                                    }
                                });

                                // Right column: Inventory
                                columns[1].vertical(|ui| {
                                    ui.heading("INVENTORY");
                                    ui.separator();
                                    ui.add_space(10.0);

                                    if let Ok(inventory) = self.world.get::<&Inventory>(self.player_entity) {
                                        if inventory.items.is_empty() {
                                            ui.label(egui::RichText::new("(empty)")
                                                .italics()
                                                .color(egui::Color32::GRAY));
                                        } else {
                                            ui.horizontal_wrapped(|ui| {
                                                for (i, item_type) in inventory.items.iter().enumerate() {
                                                    let (icon, color) = match item_type {
                                                        ItemType::HealthPotion => ("HP", egui::Color32::from_rgb(255, 100, 100)),
                                                    };

                                                    let button = egui::Button::new(
                                                        egui::RichText::new(icon)
                                                            .size(24.0)
                                                            .color(color)
                                                    )
                                                    .min_size(egui::vec2(60.0, 60.0));

                                                    if ui.add(button)
                                                        .on_hover_text(format!("{}\n\nClick to use", item_type.name()))
                                                        .clicked()
                                                    {
                                                        item_to_use = Some(i);
                                                    }
                                                }
                                            });
                                        }
                                    }
                                });
                            });
                        }
                    });
            }
        });

        // Use item if clicked
        if let Some(item_index) = item_to_use {
            if let Ok(mut inv) = self.world.get::<&mut Inventory>(self.player_entity) {
                if let Some(item) = inv.remove_item(item_index) {
                    if let Ok(mut health) = self.world.get::<&mut Health>(self.player_entity) {
                        health.heal(item.heal_amount());
                    }
                }
            }
        }

        // Render
        unsafe {
            use glow::HasContext;
            self.gl.clear_color(0.1, 0.1, 0.1, 1.0);
            self.gl.clear(glow::COLOR_BUFFER_BIT);
        }

        self.renderer.render(&self.camera, &self.grid, &self.tileset).unwrap();
        self.renderer.render_entities(&self.camera, &entities_to_render, &self.tileset).unwrap();

        // Render egui
        self.egui_glow.paint(&self.window);

        // Swap buffers
        self.gl_surface.swap_buffers(&self.gl_context).unwrap();
    }

    fn handle_input(&mut self) {
        // Toggle inventory
        if self.keys_pressed.remove(&KeyCode::KeyI) {
            self.show_inventory = !self.show_inventory;
        }

        // Movement (only process once per key press)
        let mut movement: Option<(i32, i32)> = None;

        if self.keys_pressed.remove(&KeyCode::KeyW) || self.keys_pressed.remove(&KeyCode::ArrowUp) {
            movement = Some((0, 1));
        } else if self.keys_pressed.remove(&KeyCode::KeyS) || self.keys_pressed.remove(&KeyCode::ArrowDown) {
            movement = Some((0, -1));
        } else if self.keys_pressed.remove(&KeyCode::KeyA) || self.keys_pressed.remove(&KeyCode::ArrowLeft) {
            movement = Some((-1, 0));
        } else if self.keys_pressed.remove(&KeyCode::KeyD) || self.keys_pressed.remove(&KeyCode::ArrowRight) {
            movement = Some((1, 0));
        }

        if let Some((dx, dy)) = movement {
            // Get current position (copy it to avoid borrow issues)
            let current_pos = self.world.get::<&Position>(self.player_entity).ok().map(|p| *p);

            if let Some(pos) = current_pos {
                let target_pos = Position::new(pos.x + dx, pos.y + dy);

                if let Some(tile) = self.grid.get(target_pos.x, target_pos.y) {
                    if tile.tile_type.is_walkable() {
                        // Move player
                        if let Ok(mut pos) = self.world.get::<&mut Position>(self.player_entity) {
                            *pos = target_pos;
                        }
                        self.camera.set_tracking_target(Vec2::new(target_pos.x as f32, target_pos.y as f32));

                        // Check for chest and collect items
                        let mut collected_items = Vec::new();
                        for (_id, (chest_pos, container)) in self.world.query_mut::<(&Position, &mut Container)>() {
                            if chest_pos.x == target_pos.x && chest_pos.y == target_pos.y && !container.is_open {
                                container.is_open = true;
                                collected_items = container.take_all();
                                break;
                            }
                        }

                        // Add collected items to inventory
                        if !collected_items.is_empty() {
                            if let Ok(mut inventory) = self.world.get::<&mut Inventory>(self.player_entity) {
                                for item in collected_items {
                                    inventory.add_item(item);
                                }
                            }
                        }
                    }
                }
            }
        }

        // Mouse drag for panning (when inventory is closed)
        if self.mouse_down && !self.show_inventory {
            let dx = self.mouse_pos.0 - self.last_mouse_pos.0;
            let dy = self.mouse_pos.1 - self.last_mouse_pos.1;
            if dx.abs() > 0.1 || dy.abs() > 0.1 {
                self.camera.pan(dx, dy);
            }
        }
        // Consume the mouse delta so it's not applied again next frame
        self.last_mouse_pos = self.mouse_pos;
    }
}
