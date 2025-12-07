mod actions;
mod camera;
mod components;
mod dungeon_gen;
mod events;
mod fov;
mod grid;
mod pathfinding;
mod renderer;
mod systems;
mod tile;
mod tileset;
mod vfx;

use camera::Camera;
use components::{Actor, Attackable, BlocksMovement, BlocksVision, ChaseAI, Container, Door, Equipment, Experience, Health, Inventory, ItemType, Player, Position, Sprite, Stats, VisualPosition, Weapon};
use glam::Vec2;
use grid::Grid;
use hecs::World;
use renderer::Renderer;
use tile::tile_ids;
use tileset::Tileset;
use rand::Rng;
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
    tileset_egui_id: egui::TextureId,  // Registered texture for egui
    world: World,
    player_entity: hecs::Entity,
    last_frame_time: Instant,
    show_inventory: bool,
    // Currently open chest (for loot UI)
    open_chest: Option<hecs::Entity>,
    // Input state
    keys_pressed: std::collections::HashSet<KeyCode>,
    mouse_pos: (f32, f32),
    mouse_down: bool,
    last_mouse_pos: (f32, f32),
    // Click-to-move path for player
    player_path: Vec<(i32, i32)>,
    // Destination of click-to-move (for auto-interact on arrival)
    player_path_destination: Option<(i32, i32)>,
    // Whether to show grid lines
    show_grid_lines: bool,
    // Visual effects (slashes, particles, etc.)
    vfx: vfx::VfxManager,
    // Event queue for decoupled system communication
    events: events::EventQueue,
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
        let mut egui_glow = EguiGlow::new(event_loop, gl.clone(), None, None, false);

        // Initialize game state
        let mut camera = Camera::new(size.width as f32, size.height as f32);
        let grid = Grid::new(100, 100);
        let renderer = Renderer::new(gl.clone()).expect("Failed to create renderer");

        // Load tileset
        let tileset = Tileset::load(gl.clone(), std::path::Path::new("assets/minirogue-all.tsj"))
            .expect("Failed to load tileset");

        // Register tileset texture with egui
        let tileset_egui_id = egui_glow.painter.register_native_texture(tileset.texture);

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

        // Spawn player (speed 3 = slower than skeletons)
        let player_entity = world.spawn((
            player_start,
            VisualPosition::from_position(&player_start),
            Sprite::new(tile_ids::PLAYER),
            Player,
            Actor::new(3),
            Health::new(15),
            Stats::new(10, 8, 12),
            Inventory::new(),
            Equipment::with_weapon(Weapon::sword()),
            BlocksMovement,
            Experience::new(),
        ));

        camera.set_tracking_target(Vec2::new(player_start.x as f32, player_start.y as f32));

        // Spawn chests (block movement until opened)
        for (x, y) in &grid.chest_positions.clone() {
            let pos = Position::new(*x, *y);
            world.spawn((
                pos,
                VisualPosition::from_position(&pos),
                Sprite::new(tile_ids::CHEST_CLOSED),
                Container::new(vec![ItemType::HealthPotion]),
                BlocksMovement,
            ));
        }

        // Spawn doors (closed by default, block vision and movement)
        for (x, y) in &grid.door_positions.clone() {
            let pos = Position::new(*x, *y);
            world.spawn((
                pos,
                VisualPosition::from_position(&pos),
                Sprite::new(tile_ids::DOOR),
                Door::new(),
                BlocksVision,
                BlocksMovement,
            ));
        }

        // Spawn skeletons on random walkable tiles (speed 2 = faster than player)
        let mut rng = rand::thread_rng();
        let walkable_tiles: Vec<(i32, i32)> = (0..grid.height as i32)
            .flat_map(|y| (0..grid.width as i32).map(move |x| (x, y)))
            .filter(|&(x, y)| {
                grid.get(x, y).map(|t| t.tile_type.is_walkable()).unwrap_or(false)
                    && !(x == player_start.x && y == player_start.y)
            })
            .collect();

        // Spawn enemies (skeletons) - all have chase AI but start idle
        for _ in 0..10 {
            if let Some(&(x, y)) = walkable_tiles.get(rng.gen_range(0..walkable_tiles.len())) {
                let pos = Position::new(x, y);
                world.spawn((
                    pos,
                    VisualPosition::from_position(&pos),
                    Sprite::new(tile_ids::SKELETON),
                    Actor::new(2),
                    ChaseAI::new(8),  // sight radius of 8 tiles
                    Health::new(25),
                    Stats::new(4, 1, 3),  // Total: 8 -> 8 XP
                    Attackable,
                    BlocksMovement,
                ));
            }
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
            tileset_egui_id,
            world,
            player_entity,
            last_frame_time: Instant::now(),
            show_inventory: false,
            open_chest: None,
            keys_pressed: std::collections::HashSet::new(),
            mouse_pos: (0.0, 0.0),
            mouse_down: false,
            last_mouse_pos: (0.0, 0.0),
            player_path: Vec::new(),
            player_path_destination: None,
            show_grid_lines: false,
            vfx: vfx::VfxManager::new(),
            events: events::EventQueue::new(),
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
                    let was_down = state.mouse_down;
                    state.mouse_down = btn_state == ElementState::Pressed;

                    if btn_state == ElementState::Released {
                        // Check if this was a click (not a drag)
                        let dx = state.mouse_pos.0 - state.last_mouse_pos.0;
                        let dy = state.mouse_pos.1 - state.last_mouse_pos.1;
                        let was_drag = was_down && (dx.abs() > 5.0 || dy.abs() > 5.0);

                        if !was_drag {
                            // Click-to-move: calculate path to clicked tile
                            state.handle_click_to_move();
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

        // Update animations
        systems::update_lunge_animations(&mut self.world, dt);
        systems::update_hit_flashes(&mut self.world, dt);
        self.vfx.update(dt);

        // Remove dead entities (turn into lootable bones, grant XP)
        let mut rng = rand::thread_rng();
        systems::remove_dead_entities(&mut self.world, self.player_entity, &mut rng, &mut self.events);
        self.process_events();

        // Lerp all visual positions toward logical positions
        systems::visual_lerp(&mut self.world, dt);

        // Update camera to follow player's visual position
        if let Ok(vis_pos) = self.world.get::<&VisualPosition>(self.player_entity) {
            self.camera.set_tracking_target(Vec2::new(vis_pos.x, vis_pos.y));
        }

        // Update camera (pass mouse_down so momentum doesn't apply while dragging)
        self.camera.update(dt, self.mouse_down);

        // Update FOV
        systems::update_fov(&self.world, &mut self.grid, self.player_entity, 10);

        // Collect entities for rendering
        let entities_to_render = systems::collect_renderables(&self.world, &self.grid, self.player_entity);

        // Get player health, gold, and experience
        let health_percent = if let Ok(health) = self.world.get::<&Health>(self.player_entity) {
            (health.current as f32 / health.max as f32).clamp(0.0, 1.0)
        } else {
            1.0
        };
        let player_gold = self.world.get::<&Inventory>(self.player_entity)
            .map(|inv| inv.gold)
            .unwrap_or(0);
        let (xp_progress, xp_level) = self.world.get::<&Experience>(self.player_entity)
            .map(|exp| (systems::xp_progress(&exp), exp.level))
            .unwrap_or((0.0, 1));

        // Begin egui frame
        let mut item_to_use: Option<usize> = None;
        let mut chest_item_to_take: Option<usize> = None;
        let mut chest_take_all = false;
        let mut chest_take_gold = false;
        let mut close_chest = false;

        // Get chest contents for UI (if chest is open)
        let (chest_contents, chest_gold): (Vec<ItemType>, u32) = self.open_chest
            .and_then(|id| self.world.get::<&Container>(id).ok())
            .map(|c| (c.items.clone(), c.gold))
            .unwrap_or_default();

        // Pre-compute tileset info for UI icons
        let tileset_texture_id = self.tileset_egui_id;
        let sword_uv = self.tileset.get_egui_uv(tile_ids::SWORD);
        let potion_uv = self.tileset.get_egui_uv(tile_ids::RED_POTION);
        let coins_uv = self.tileset.get_egui_uv(tile_ids::COINS);

        self.egui_glow.run(&self.window, |ctx| {
            // Health, XP, and Gold bar
            egui::Window::new("Status")
                .fixed_pos([10.0, 10.0])
                .fixed_size([200.0, 90.0])
                .title_bar(false)
                .show(ctx, |ui| {
                    ui.add(egui::ProgressBar::new(health_percent)
                        .text(format!("HP: {:.0}%", health_percent * 100.0)));
                    ui.add(egui::ProgressBar::new(xp_progress)
                        .fill(egui::Color32::from_rgb(100, 149, 237))  // Cornflower blue
                        .text(format!("Lv {} - XP: {:.0}%", xp_level, xp_progress * 100.0)));
                    ui.horizontal(|ui| {
                        let coin_img = egui::Image::new(egui::load::SizedTexture::new(
                            tileset_texture_id,
                            egui::vec2(16.0, 16.0),
                        ))
                        .uv(coins_uv);
                        ui.add(coin_img);
                        ui.label(format!("{}", player_gold));
                    });
                });

            // Chest/bones loot window
            if self.open_chest.is_some() {
                egui::Window::new("Loot")
                    .default_pos([self.camera.viewport_width / 2.0 - 150.0, self.camera.viewport_height / 2.0 - 100.0])
                    .default_size([300.0, 200.0])
                    .collapsible(false)
                    .resizable(false)
                    .show(ctx, |ui| {
                        ui.heading("Contents");
                        ui.separator();
                        ui.add_space(10.0);

                        let has_contents = !chest_contents.is_empty() || chest_gold > 0;

                        if !has_contents {
                            ui.label(egui::RichText::new("(empty)")
                                .italics()
                                .color(egui::Color32::GRAY));
                        } else {
                            // Show gold if present
                            if chest_gold > 0 {
                                ui.horizontal(|ui| {
                                    let coin_img = egui::Image::new(egui::load::SizedTexture::new(
                                        tileset_texture_id,
                                        egui::vec2(32.0, 32.0),
                                    ))
                                    .uv(coins_uv);

                                    if ui.add(egui::ImageButton::new(coin_img))
                                        .on_hover_text(format!("{} Gold\n\nClick to take", chest_gold))
                                        .clicked()
                                    {
                                        chest_take_gold = true;
                                    }
                                    ui.label(format!("{} gold", chest_gold));
                                });
                                ui.add_space(5.0);
                            }

                            // Show items
                            ui.horizontal_wrapped(|ui| {
                                for (i, item_type) in chest_contents.iter().enumerate() {
                                    let uv = match item_type {
                                        ItemType::HealthPotion => potion_uv,
                                    };

                                    let image = egui::Image::new(egui::load::SizedTexture::new(
                                        tileset_texture_id,
                                        egui::vec2(48.0, 48.0),
                                    ))
                                    .uv(uv);

                                    let response = ui.add(egui::ImageButton::new(image));

                                    if response
                                        .on_hover_text(format!("{}\n\nClick to take", systems::item_name(*item_type)))
                                        .clicked()
                                    {
                                        chest_item_to_take = Some(i);
                                    }
                                }
                            });
                        }

                        ui.add_space(10.0);
                        ui.separator();
                        ui.horizontal(|ui| {
                            if has_contents {
                                if ui.button("Take All").clicked() {
                                    chest_take_all = true;
                                }
                            }
                            if ui.button("Close").clicked() {
                                close_chest = true;
                            }
                        });
                    });
            }

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
                                // Left column: Stats + Equipment
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

                                        ui.add_space(10.0);
                                        ui.horizontal(|ui| {
                                            let coin_img = egui::Image::new(egui::load::SizedTexture::new(
                                                tileset_texture_id,
                                                egui::vec2(24.0, 24.0),
                                            ))
                                            .uv(coins_uv);
                                            ui.add(coin_img);
                                            ui.label(format!("{} gold", inventory.gold));
                                        });
                                    }

                                    ui.add_space(20.0);
                                    ui.heading("EQUIPMENT");
                                    ui.separator();
                                    ui.add_space(10.0);

                                    // Weapon slot
                                    ui.horizontal(|ui| {
                                        ui.label("Weapon:");
                                        if let Ok(equipment) = self.world.get::<&Equipment>(self.player_entity) {
                                            if let Some(weapon) = &equipment.weapon {
                                                let image = egui::Image::new(egui::load::SizedTexture::new(
                                                    tileset_texture_id,
                                                    egui::vec2(48.0, 48.0),
                                                ))
                                                .uv(sword_uv);

                                                ui.add(egui::ImageButton::new(image))
                                                    .on_hover_text(format!(
                                                        "{}\n\nDamage: {} + {} = {}",
                                                        weapon.name,
                                                        weapon.base_damage,
                                                        weapon.damage_bonus,
                                                        systems::weapon_damage(weapon)
                                                    ));
                                            } else {
                                                ui.label(egui::RichText::new("(none)")
                                                    .italics()
                                                    .color(egui::Color32::GRAY));
                                            }
                                        }
                                    });
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
                                                    let uv = match item_type {
                                                        ItemType::HealthPotion => potion_uv,
                                                    };

                                                    let image = egui::Image::new(egui::load::SizedTexture::new(
                                                        tileset_texture_id,
                                                        egui::vec2(48.0, 48.0),
                                                    ))
                                                    .uv(uv);

                                                    let response = ui.add(egui::ImageButton::new(image));

                                                    if response
                                                        .on_hover_text(format!("{}\n\nClick to use", systems::item_name(*item_type)))
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

        // Handle chest/loot interactions
        if let Some(chest_id) = self.open_chest {
            if chest_take_all {
                systems::take_all_from_container(&mut self.world, self.player_entity, chest_id);
                self.open_chest = None;
            } else if chest_take_gold {
                systems::take_gold_from_container(&mut self.world, self.player_entity, chest_id);
            } else if let Some(item_index) = chest_item_to_take {
                systems::take_item_from_container(&mut self.world, self.player_entity, chest_id, item_index);
            } else if close_chest {
                self.open_chest = None;
            }
        }

        // Use item if clicked
        if let Some(item_index) = item_to_use {
            if let Ok(mut inv) = self.world.get::<&mut Inventory>(self.player_entity) {
                if item_index < inv.items.len() {
                    let item = inv.items.remove(item_index);
                    inv.current_weight_kg -= systems::item_weight(item);
                    if let Ok(mut health) = self.world.get::<&mut Health>(self.player_entity) {
                        health.current = (health.current + systems::item_heal_amount(item)).min(health.max);
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

        self.renderer.render(&self.camera, &self.grid, &self.tileset, self.show_grid_lines).unwrap();
        self.renderer.render_entities(&self.camera, &entities_to_render, &self.tileset).unwrap();
        self.renderer.render_vfx(&self.camera, &self.vfx.effects);

        // Render egui
        self.egui_glow.paint(&self.window);

        // Swap buffers
        self.gl_surface.swap_buffers(&self.gl_context).unwrap();
    }

    fn run_ticks_until_player_acts(&mut self, dx: i32, dy: i32) -> systems::MoveResult {
        let mut rng = rand::thread_rng();

        loop {
            // Check if player can act
            let player_can_act = self.world
                .get::<&Actor>(self.player_entity)
                .map(|a| a.energy >= a.speed)
                .unwrap_or(true);

            if player_can_act {
                let result = systems::player_move(&mut self.world, &self.grid, self.player_entity, dx, dy, &mut self.events);
                match result {
                    systems::MoveResult::OpenedChest(chest) => {
                        self.open_chest = Some(chest);
                    }
                    systems::MoveResult::Moved => {
                        // Close any open loot window when player moves away
                        self.open_chest = None;
                    }
                    _ => {}
                }
                // Process events (VFX spawning, etc.)
                self.process_events();
                return result;
            }

            // Player can't act yet - run one tick
            systems::tick_energy(&mut self.world);
            systems::ai_chase(&mut self.world, &self.grid, self.player_entity, &mut rng, &mut self.events);
            // Process events from AI actions too
            self.process_events();
        }
    }

    /// Process all pending events, dispatching to appropriate handlers
    fn process_events(&mut self) {
        for event in self.events.drain() {
            self.vfx.handle_event(&event);
            // Future: audio.handle_event(&event), ui.handle_event(&event), etc.
        }
    }

    fn handle_click_to_move(&mut self) {
        // Convert screen position to world position
        let world_pos = self.camera.screen_to_world(self.mouse_pos.0, self.mouse_pos.1);

        // Convert to tile coordinates
        let tile_x = world_pos.x.floor() as i32;
        let tile_y = world_pos.y.floor() as i32;

        // Get player position
        let player_pos = match self.world.get::<&Position>(self.player_entity) {
            Ok(p) => (p.x, p.y),
            Err(_) => return,
        };

        // Don't path to current position
        if tile_x == player_pos.0 && tile_y == player_pos.1 {
            return;
        }

        // Collect blocking positions (other entities)
        let blocked: std::collections::HashSet<(i32, i32)> = self.world
            .query::<(&Position, &BlocksMovement)>()
            .iter()
            .filter(|(id, _)| *id != self.player_entity)
            .map(|(_, (pos, _))| (pos.x, pos.y))
            .collect();

        // Calculate path
        if let Some(path) = pathfinding::find_path(&self.grid, player_pos, (tile_x, tile_y), &blocked) {
            self.player_path = path;
            self.player_path_destination = Some((tile_x, tile_y));
        }
    }

    fn follow_player_path(&mut self) {
        if self.player_path.is_empty() {
            return;
        }

        // Get the next step
        let (next_x, next_y) = self.player_path[0];

        // Get player position
        let player_pos = match self.world.get::<&Position>(self.player_entity) {
            Ok(p) => (p.x, p.y),
            Err(_) => {
                self.player_path.clear();
                return;
            }
        };

        // Calculate movement direction
        let dx = next_x - player_pos.0;
        let dy = next_y - player_pos.1;

        // Execute the move
        let result = self.run_ticks_until_player_acts(dx, dy);

        match result {
            systems::MoveResult::Moved => {
                // Remove the step we just took
                self.player_path.remove(0);

                // If path is now empty, we've arrived - check for auto-interact
                if self.player_path.is_empty() {
                    if let Some(_dest) = self.player_path_destination.take() {
                        // Check for lootable container at current position (bones, etc.)
                        if let Some(container_id) =
                            systems::find_container_at_player(&self.world, self.player_entity)
                        {
                            self.open_chest = Some(container_id);
                        }
                    }
                }
            }
            systems::MoveResult::Attacked(_) | systems::MoveResult::OpenedChest(_) => {
                // Stop pathing if we attacked or opened something
                self.player_path.clear();
                self.player_path_destination = None;
            }
            systems::MoveResult::Blocked => {
                // Path is blocked, recalculate or clear
                self.player_path.clear();
                self.player_path_destination = None;
            }
        }
    }

    fn handle_input(&mut self) {
        // Toggle fullscreen
        if self.keys_pressed.remove(&KeyCode::F11) {
            use winit::window::Fullscreen;
            let fullscreen = if self.window.fullscreen().is_some() {
                None
            } else {
                Some(Fullscreen::Borderless(None))
            };
            self.window.set_fullscreen(fullscreen);
        }

        // Toggle inventory
        if self.keys_pressed.remove(&KeyCode::KeyI) {
            self.show_inventory = !self.show_inventory;
        }

        // Toggle grid lines
        if self.keys_pressed.remove(&KeyCode::BracketRight) {
            self.show_grid_lines = !self.show_grid_lines;
        }

        // Enter key: Take All if chest open, otherwise loot bones at player position
        if self.keys_pressed.remove(&KeyCode::Enter) {
            if let Some(chest_id) = self.open_chest {
                // Take all from open chest
                systems::take_all_from_container(&mut self.world, self.player_entity, chest_id);
                self.open_chest = None;
            } else if let Some(container_id) = systems::find_container_at_player(&self.world, self.player_entity) {
                self.open_chest = Some(container_id);
            }
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

        // Execute movement immediately (run all ticks, visuals will catch up)
        if let Some((dx, dy)) = movement {
            // Keyboard movement cancels click-to-move path
            self.player_path.clear();

            let current_pos = self.world.get::<&Position>(self.player_entity).ok().map(|p| *p);
            if let Some(pos) = current_pos {
                let target_pos = Position::new(pos.x + dx, pos.y + dy);
                if let Some(tile) = self.grid.get(target_pos.x, target_pos.y) {
                    if tile.tile_type.is_walkable() {
                        // Run ticks until player can act, then execute move
                        self.run_ticks_until_player_acts(dx, dy);
                    }
                }
            }
        } else {
            // No keyboard movement - follow click-to-move path
            self.follow_player_path();
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
