mod camera;
mod components;
mod dungeon_gen;
mod fov;
mod grid;
mod renderer;
mod tile;

use camera::Camera;
use components::{Container, Health, Inventory, ItemType, Player, Position, Sprite, Stats};
use fov::FOV;
use glam::Vec2;
use grid::Grid;
use hecs::World;
use renderer::Renderer;
use sdl2::event::Event;
use sdl2::keyboard::Keycode;
use std::sync::Arc;
use std::time::Instant;
use egui_sdl2_gl::egui;

fn main() -> Result<(), String> {
    let sdl_context = sdl2::init()?;
    let video_subsystem = sdl_context.video()?;

    let gl_attr = video_subsystem.gl_attr();
    gl_attr.set_context_profile(sdl2::video::GLProfile::Core);
    gl_attr.set_context_version(3, 3);

    let window = video_subsystem
        .window("Grid Roguelike", 1280, 720)
        .opengl()
        .resizable()
        .build()
        .map_err(|e| e.to_string())?;

    let _gl_context = window.gl_create_context()?;
    let gl = Arc::new(unsafe {
        glow::Context::from_loader_function(|s| video_subsystem.gl_get_proc_address(s) as *const _)
    });

    let mut camera = Camera::new(1280.0, 720.0);
    let mut grid = Grid::new(100, 100);
    let mut renderer = Renderer::new(gl.clone())?;

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

    // Spawn player at first walkable position with 50% health for testing
    let player_entity = world.spawn((
        player_start,
        Sprite::new(glam::Vec3::new(1.0, 1.0, 0.0)), // Yellow player
        Player,
        {
            let mut health = Health::new(100);
            health.current = 50; // Start with 50% health for testing
            health
        },
        Stats::new(10, 8, 12), // STR: 10, INT: 8, AGI: 12
        Inventory::new(),
    ));

    // Set camera to track player initially at their actual spawn position
    camera.set_tracking_target(Vec2::new(player_start.x as f32, player_start.y as f32));

    // Spawn chests at room centers with health potions
    for (x, y) in &grid.chest_positions {
        world.spawn((
            Position::new(*x, *y),
            Sprite::new(glam::Vec3::new(0.6, 0.4, 0.2)), // Brown chest
            Container::new(vec![ItemType::HealthPotion]),
        ));
    }

    let mut event_pump = sdl_context.event_pump()?;

    // Initialize egui with SDL2 backend
    let (mut painter, mut egui_state) = egui_sdl2_gl::with_sdl2(
        &window,
        egui_sdl2_gl::ShaderVersion::Default,
        egui_sdl2_gl::DpiScaling::Default,
    );
    let egui_ctx = egui::Context::default();
    egui_ctx.set_visuals(egui::Visuals::dark());

    let mut last_frame_time = Instant::now();
    let mut show_inventory = false;
    let start_time = Instant::now();

    'running: loop {
        let current_time = Instant::now();
        let dt = (current_time - last_frame_time).as_secs_f32();
        last_frame_time = current_time;

        // Process SDL events and pass to egui
        for event in event_pump.poll_iter() {
            match event {
                Event::Quit { .. } | Event::KeyDown { keycode: Some(Keycode::Escape), .. } => {
                    break 'running;
                }
                _ => {
                    egui_state.process_input(&window, event, &mut painter);
                }
            }
        }

        // Update camera (smooth interpolation handled internally)
        camera.update(dt, false);

        // Update field of view based on player position
        if let Ok(player_pos) = world.get::<&Position>(player_entity) {
            // Clear all visibility
            for tile in &mut grid.tiles {
                tile.visible = false;
            }

            // Calculate visible tiles with shadowcasting (10 tile radius)
            let visible_tiles = FOV::calculate(&grid, player_pos.x, player_pos.y, 10);

            // Mark tiles as visible and explored
            for (x, y) in visible_tiles {
                if let Some(tile) = grid.get_mut(x, y) {
                    tile.visible = true;
                    tile.explored = true;
                }
            }
        }

        // Collect entities for rendering (non-player entities first, then player on top)
        let mut entities_to_render = Vec::new();
        let mut player_render: Option<(Position, Sprite)> = None;

        for (id, (pos, sprite)) in world.query::<(&Position, &Sprite)>().iter() {
            if id == player_entity {
                player_render = Some((*pos, *sprite));
            } else {
                entities_to_render.push((*pos, *sprite));
            }
        }

        // Add player last so it renders on top
        if let Some(player) = player_render {
            entities_to_render.push(player);
        }

        // Get player health for UI
        let health_percent = if let Ok(health) = world.get::<&Health>(player_entity) {
            health.percentage()
        } else {
            1.0
        };

        // Begin egui frame
        let mut item_to_use: Option<usize> = None;
        egui_state.input.time = Some(start_time.elapsed().as_secs_f64());
        egui_ctx.begin_pass(egui_state.input.take());

        // Handle game controls (only if egui doesn't want the input)
        if !egui_ctx.wants_keyboard_input() {
            egui_ctx.input(|i| {
                // Toggle inventory with I key
                if i.key_pressed(egui::Key::I) {
                    show_inventory = !show_inventory;
                }

                // WASD/Arrow movement
                let mut new_pos: Option<Position> = None;
                if let Ok(pos) = world.get::<&Position>(player_entity) {
                    if i.key_pressed(egui::Key::W) || i.key_pressed(egui::Key::ArrowUp) {
                        new_pos = Some(Position::new(pos.x, pos.y + 1));
                    } else if i.key_pressed(egui::Key::S) || i.key_pressed(egui::Key::ArrowDown) {
                        new_pos = Some(Position::new(pos.x, pos.y - 1));
                    } else if i.key_pressed(egui::Key::A) || i.key_pressed(egui::Key::ArrowLeft) {
                        new_pos = Some(Position::new(pos.x - 1, pos.y));
                    } else if i.key_pressed(egui::Key::D) || i.key_pressed(egui::Key::ArrowRight) {
                        new_pos = Some(Position::new(pos.x + 1, pos.y));
                    }
                }

                // Try to move to new position if walkable
                if let Some(target_pos) = new_pos {
                    if let Some(tile) = grid.get(target_pos.x, target_pos.y) {
                        if tile.tile_type.is_walkable() {
                            // Move player
                            if let Ok(mut pos) = world.get::<&mut Position>(player_entity) {
                                *pos = target_pos;
                            }
                            camera.set_tracking_target(Vec2::new(target_pos.x as f32, target_pos.y as f32));

                            // Check for chest at new position and collect items
                            for (_id, (chest_pos, container)) in world.query_mut::<(&Position, &mut Container)>() {
                                if chest_pos.x == target_pos.x && chest_pos.y == target_pos.y && !container.is_open {
                                    container.is_open = true;
                                    let items = container.take_all();
                                    if let Ok(mut inventory) = world.get::<&mut Inventory>(player_entity) {
                                        for item in items {
                                            inventory.add_item(item);
                                        }
                                    }
                                    break;
                                }
                            }
                        }
                    }
                }
            });
        }

        // Handle camera controls (mouse drag/zoom) only if egui doesn't want pointer and inventory is closed
        if !egui_ctx.wants_pointer_input() && !show_inventory {
            egui_ctx.input(|i| {
                // Mouse drag for panning
                if i.pointer.primary_down() {
                    let delta = i.pointer.delta();
                    if delta.length() > 0.1 {
                        camera.pan(delta.x, delta.y);
                    }
                } else if i.pointer.primary_released() {
                    camera.release_pan();
                }

                // Mouse wheel for zoom
                if i.smooth_scroll_delta.y != 0.0 {
                    if let Some(pos) = i.pointer.hover_pos() {
                        camera.add_zoom_impulse(i.smooth_scroll_delta.y, pos.x, pos.y);
                    }
                }
            });
        }

        // Health bar (top-left)
        egui::Window::new("Health")
            .fixed_pos([10.0, 10.0])
            .fixed_size([200.0, 40.0])
            .title_bar(false)
            .show(&egui_ctx, |ui| {
                ui.add(egui::ProgressBar::new(health_percent)
                    .text(format!("HP: {:.0}%", health_percent * 100.0)));
            });

        // Inventory window
        if show_inventory {
            egui::Window::new("Character")
                .fixed_pos([camera.viewport_width / 2.0 - 200.0, camera.viewport_height / 2.0 - 200.0])
                .fixed_size([400.0, 400.0])
                .collapsible(false)
                .resizable(false)
                .show(&egui_ctx, |ui| {
                ui.heading("CHARACTER STATS");
                ui.separator();

                if let Ok(stats) = world.get::<&Stats>(player_entity) {
                    ui.label(format!("💪 Strength: {}", stats.strength));
                    ui.label(format!("🧠 Intelligence: {}", stats.intelligence));
                    ui.label(format!("⚡ Agility: {}", stats.agility));
                    ui.separator();

                    let carry_capacity = stats.strength as f32 * 2.0;
                    if let Ok(inventory) = world.get::<&Inventory>(player_entity) {
                        ui.label(format!("🎒 Weight: {:.1} / {:.1} kg",
                            inventory.current_weight_kg, carry_capacity));
                        ui.separator();

                        ui.heading("Items");
                        if inventory.items.is_empty() {
                            ui.label("(empty)");
                        } else {
                            for (i, item_type) in inventory.items.iter().enumerate() {
                                if ui.button(format!("🧪 {} (click to use)", item_type.name())).clicked() {
                                    item_to_use = Some(i);
                                }
                            }
                        }
                    }
                }
            });
        }

        let egui::FullOutput {
            platform_output,
            textures_delta,
            shapes,
            pixels_per_point,
            ..
        } = egui_ctx.end_pass();

        egui_state.process_output(&window, &platform_output);

        // Use item if clicked
        if let Some(item_index) = item_to_use {
            if let Ok(mut inv) = world.get::<&mut Inventory>(player_entity) {
                if let Some(item) = inv.remove_item(item_index) {
                    if let Ok(mut health) = world.get::<&mut Health>(player_entity) {
                        health.heal(item.heal_amount());
                    }
                }
            }
        }

        // Render game
        renderer.render(&camera, &grid)?;
        renderer.render_entities(&camera, &entities_to_render)?;

        // Render egui on top
        let paint_jobs = egui_ctx.tessellate(shapes, pixels_per_point);
        painter.paint_jobs(None, textures_delta, paint_jobs);

        window.gl_swap_window();
    }

    Ok(())
}
