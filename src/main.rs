mod camera;
mod components;
mod dungeon_gen;
mod fov;
mod grid;
mod renderer;
mod tile;

use camera::Camera;
use components::{Health, Inventory, Player, Position, Sprite, Stats};
use fov::FOV;
use glam::Vec2;
use grid::Grid;
use hecs::World;
use renderer::Renderer;
use sdl2::event::Event;
use sdl2::keyboard::Keycode;
use sdl2::mouse::MouseButton;
use std::time::Instant;

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
    let gl = unsafe {
        glow::Context::from_loader_function(|s| video_subsystem.gl_get_proc_address(s) as *const _)
    };

    let mut camera = Camera::new(1280.0, 720.0);
    let mut grid = Grid::new(100, 100);
    let mut renderer = Renderer::new(gl)?;

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

    // Spawn player at first walkable position
    let player_entity = world.spawn((
        player_start,
        Sprite::new(glam::Vec3::new(1.0, 1.0, 0.0)), // Yellow player
        Player,
        Health::new(100), // Start with 100 HP
        Stats::new(10, 8, 12), // STR: 10, INT: 8, AGI: 12
        Inventory::new(),
    ));

    // Set camera to track player initially at their actual spawn position
    camera.set_tracking_target(Vec2::new(player_start.x as f32, player_start.y as f32));

    let mut event_pump = sdl_context.event_pump()?;
    let mut mouse_down = false;
    let mut last_mouse_pos = (0, 0);
    let mut last_frame_time = Instant::now();
    let mut show_inventory = false;

    'running: loop {
        let current_time = Instant::now();
        let dt = (current_time - last_frame_time).as_secs_f32();
        last_frame_time = current_time;
        for event in event_pump.poll_iter() {
            match event {
                Event::Quit { .. }
                | Event::KeyDown {
                    keycode: Some(Keycode::Escape),
                    ..
                } => break 'running,

                Event::KeyDown {
                    keycode: Some(keycode),
                    ..
                } => {
                    // Toggle inventory
                    if keycode == Keycode::I {
                        show_inventory = !show_inventory;
                        continue;
                    }

                    // WASD movement
                    let mut moved = false;

                    // Get player position
                    if let Ok(mut pos) = world.get::<&mut Position>(player_entity) {
                        let old_pos = *pos;

                        let new_pos = match keycode {
                            Keycode::W | Keycode::Up => {
                                moved = true;
                                Position::new(old_pos.x, old_pos.y + 1)
                            }
                            Keycode::S | Keycode::Down => {
                                moved = true;
                                Position::new(old_pos.x, old_pos.y - 1)
                            }
                            Keycode::A | Keycode::Left => {
                                moved = true;
                                Position::new(old_pos.x - 1, old_pos.y)
                            }
                            Keycode::D | Keycode::Right => {
                                moved = true;
                                Position::new(old_pos.x + 1, old_pos.y)
                            }
                            _ => old_pos,
                        };

                        // Check collision before moving
                        if moved {
                            // Check if target tile is walkable
                            if let Some(tile) = grid.get(new_pos.x, new_pos.y) {
                                if tile.tile_type.is_walkable() {
                                    // Only move if tile is walkable
                                    *pos = new_pos;
                                    // Update camera tracking
                                    camera.set_tracking_target(Vec2::new(new_pos.x as f32, new_pos.y as f32));
                                }
                            }
                        }
                    }
                }

                Event::Window {
                    win_event: sdl2::event::WindowEvent::Resized(w, h),
                    ..
                } => {
                    camera.resize(w as f32, h as f32);
                    renderer.resize(w, h);
                }

                Event::MouseButtonDown {
                    mouse_btn: MouseButton::Left,
                    x,
                    y,
                    ..
                } => {
                    mouse_down = true;
                    last_mouse_pos = (x, y);
                }

                Event::MouseButtonUp {
                    mouse_btn: MouseButton::Left,
                    ..
                } => {
                    mouse_down = false;
                    camera.release_pan(); // Apply momentum on release
                }

                Event::MouseMotion { x, y, .. } => {
                    // Always update last mouse position
                    if mouse_down {
                        let dx = (x - last_mouse_pos.0) as f32;
                        let dy = (y - last_mouse_pos.1) as f32;
                        camera.pan(dx, dy); // Direct pan while dragging
                    }
                    last_mouse_pos = (x, y);
                }

                Event::MouseWheel { y, .. } => {
                    camera.add_zoom_impulse(y as f32, last_mouse_pos.0 as f32, last_mouse_pos.1 as f32);
                }

                _ => {}
            }
        }

        // Update camera with smooth interpolation
        camera.update(dt, mouse_down);

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

        // Collect entities for rendering
        let mut entities_to_render = Vec::new();
        for (_id, (pos, sprite)) in world.query::<(&Position, &Sprite)>().iter() {
            entities_to_render.push((*pos, *sprite));
        }

        // Get player health for UI
        let health_percent = if let Ok(health) = world.get::<&Health>(player_entity) {
            health.percentage()
        } else {
            1.0
        };

        // Render
        renderer.render(&camera, &grid)?;
        renderer.render_entities(&camera, &entities_to_render)?;
        renderer.render_ui(health_percent, camera.viewport_width, camera.viewport_height)?;

        // Render inventory if open
        if show_inventory {
            if let Ok(stats) = world.get::<&Stats>(player_entity) {
                if let Ok(inventory) = world.get::<&Inventory>(player_entity) {
                    renderer.render_inventory(
                        (stats.strength, stats.intelligence, stats.agility),
                        inventory.current_weight_kg,
                        camera.viewport_width,
                        camera.viewport_height,
                    )?;
                }
            }
        }

        window.gl_swap_window();
    }

    Ok(())
}
