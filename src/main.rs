mod active_ai_tracker;
mod app;
mod camera;
mod components;
mod constants;
mod dungeon_gen;
mod engine;
mod events;
mod fov;
mod game;
mod grid;
mod input;
mod multi_tileset;
mod pathfinding;
mod queries;
mod render;
mod renderer;
mod spatial_cache;
mod spawning;
mod systems;
mod tile;
mod tileset;
mod time_system;
mod ui;
mod vfx;

use engine::{GameEngine, WindowAction};
use render::RenderContext;
use std::sync::Arc;
use std::time::Instant;

use glutin::prelude::*;
use glutin::surface::WindowSurface;
use winit::application::ApplicationHandler;
use winit::event::WindowEvent;
use winit::event_loop::{ActiveEventLoop, EventLoop};
use winit::window::{Fullscreen, Window, WindowId};

use egui_glow::EguiGlow;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize puffin profiler
    puffin::set_scopes_on(true);

    // Start puffin HTTP server for viewing profiler in browser
    // Open http://localhost:8585 in your browser to view the profiler
    let server_addr = format!("127.0.0.1:{}", puffin_http::DEFAULT_PORT);
    let _puffin_server = puffin_http::Server::new(&server_addr).ok();
    eprintln!("Profiler server running at http://{}", server_addr);
    eprintln!("Run `puffin_viewer` or open in browser to view profiler");

    let event_loop = EventLoop::new()?;
    let mut app = App::new();
    event_loop.run_app(&mut app)?;
    Ok(())
}

struct App {
    state: Option<AppState>,
}

struct AppState {
    // Platform/GL
    window: Window,
    gl_surface: glutin::surface::Surface<WindowSurface>,
    gl_context: glutin::context::PossiblyCurrentContext,
    gl: Arc<glow::Context>,
    egui_glow: EguiGlow,

    // Rendering
    render_ctx: RenderContext,

    // Game engine (owns all game state)
    engine: GameEngine,

    // Frame timing
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

        let size = window.inner_size();

        // Create render context
        let render_ctx = RenderContext::new(
            gl.clone(),
            &mut egui_glow,
            size.width as f32,
            size.height as f32,
        );

        // Create game engine (starts in StartScreen mode)
        let engine = GameEngine::new();

        self.state = Some(AppState {
            window,
            gl_surface,
            gl_context,
            gl,
            egui_glow,
            render_ctx,
            engine,
            last_frame_time: Instant::now(),
        });
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        let state = match &mut self.state {
            Some(s) => s,
            None => return,
        };

        // Let egui handle first
        let egui_consumed = state.egui_glow.on_window_event(&state.window, &event);

        match event {
            WindowEvent::CloseRequested => {
                event_loop.exit();
            }
            WindowEvent::Resized(size) => {
                app::resize_surface(&state.gl_surface, &state.gl_context, size.width, size.height);
                state.render_ctx.camera.viewport_width = size.width as f32;
                state.render_ctx.camera.viewport_height = size.height as f32;
            }
            WindowEvent::RedrawRequested => {
                state.update_and_render();
                state.window.request_redraw();
            }
            _ => {
                // Forward to engine
                if let Some(action) = state.engine.handle_event(
                    &event,
                    &mut state.render_ctx.camera,
                    egui_consumed.consumed,
                ) {
                    match action {
                        WindowAction::Exit => event_loop.exit(),
                        WindowAction::ToggleFullscreen => state.toggle_fullscreen(),
                    }
                }
            }
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
        puffin::GlobalProfiler::lock().new_frame();
        puffin::profile_function!();

        let current_time = Instant::now();
        let raw_dt = (current_time - self.last_frame_time).as_secs_f32();
        self.last_frame_time = current_time;

        let dt = raw_dt.min(constants::MAX_ANIMATION_DT);

        // Tick game engine
        let tick_result = {
            puffin::profile_scope!("engine_tick");
            self.engine.tick(dt, &mut self.render_ctx.camera)
        };

        // Handle window actions from tick
        if let Some(action) = tick_result.window_action {
            match action {
                WindowAction::ToggleFullscreen => self.toggle_fullscreen(),
                WindowAction::Exit => {
                    // Can't exit from here, but this shouldn't happen from tick
                }
            }
        }

        // Run UI
        let ui_actions = {
            puffin::profile_scope!("run_ui");
            self.engine.run_ui(
                &mut self.egui_glow,
                &self.window,
                &self.render_ctx.camera,
                &self.render_ctx.tileset,
                &self.render_ctx.ui_icons,
            )
        };

        // Process UI actions
        self.engine.process_ui_actions(&ui_actions);

        // Handle start game action (from class selection screen)
        if let Some(class) = ui_actions.start_game {
            self.engine.start_game(class, &mut self.render_ctx.camera);
        }

        // Render game world (only when playing)
        if let Some(grid) = self.engine.grid() {
            puffin::profile_scope!("render_frame");
            let light_sources = self.engine.light_sources();
            self.render_ctx.render_frame(
                &self.gl,
                grid,
                &tick_result.entities,
                self.engine.vfx_effects(),
                self.engine.fires(),
                self.engine.player_visual_pos(),
                self.engine.player_light_radius(),
                &light_sources,
                self.engine.show_grid_lines(),
            );
        }

        // Render egui
        {
            puffin::profile_scope!("egui_paint");
            self.egui_glow.paint(&self.window);
        }

        // Swap buffers
        self.gl_surface.swap_buffers(&self.gl_context).unwrap();
    }

    fn toggle_fullscreen(&mut self) {
        let fullscreen = if self.window.fullscreen().is_some() {
            None
        } else {
            Some(Fullscreen::Borderless(None))
        };
        self.window.set_fullscreen(fullscreen);
    }
}
