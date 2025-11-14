mod camera;
mod grid;
mod renderer;
mod tile;

use camera::Camera;
use grid::Grid;
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
    let grid = Grid::new(100, 100);
    let mut renderer = Renderer::new(gl)?;

    let mut event_pump = sdl_context.event_pump()?;
    let mut mouse_down = false;
    let mut last_mouse_pos = (0, 0);
    let mut last_frame_time = Instant::now();

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

        renderer.render(&camera, &grid)?;
        window.gl_swap_window();
    }

    Ok(())
}
