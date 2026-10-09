#[macro_use]
mod profiling;

mod active_ai_tracker;
mod app;
mod audio;
mod camera;
mod components;
mod constants;
mod dungeon_gen;
mod ease;
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
mod run_history;
mod spatial_cache;
mod spawning;
mod systems;
mod tile;
mod tile_occupancy;
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
    // Profiling is opt-in (cargo feature `profiling`); without it this does
    // nothing and no local port is opened. Held for the process lifetime: the
    // server stops when the handle drops.
    profiling::start();

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
        let render_ctx = RenderContext::new(gl.clone(), &mut egui_glow, size.width, size.height);

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
                // Camera viewport and the post-process targets both track the
                // surface size; a minimised window reports 0x0 and is handled
                // inside, not filtered out here.
                state.render_ctx.resize(size.width, size.height);
            }
            WindowEvent::RedrawRequested => {
                if state.update_and_render() {
                    event_loop.exit();
                }
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
    /// Runs one frame. Returns true if the app should exit (e.g. the player
    /// chose Exit in the pause menu).
    fn update_and_render(&mut self) -> bool {
        profiling::new_frame();
        profile_function!();

        let current_time = Instant::now();
        let raw_dt = (current_time - self.last_frame_time).as_secs_f32();
        self.last_frame_time = current_time;

        let dt = raw_dt.min(constants::MAX_ANIMATION_DT);

        // Tick game engine
        let tick_result = {
            profile_scope!("engine_tick");
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
            profile_scope!("run_ui");
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

        // Handle start game action (from class selection screen); the seed
        // comes from the start screen's seed field (random when left as-is)
        if let Some(class) = ui_actions.start_game {
            let seed = self.engine.next_run_seed();
            self.engine.start_game(class, seed, &mut self.render_ctx.camera);
        }

        // Handle retry action (from game over / pause screen) - restart with
        // the same class AND the same seed, so it's a true retry of the run
        if ui_actions.retry_game {
            if let Some(class) = self.engine.selected_class {
                let seed = self
                    .engine
                    .current_run_seed()
                    .unwrap_or_else(|| self.engine.next_run_seed());
                self.engine.start_game(class, seed, &mut self.render_ctx.camera);
            }
        }

        // Handle return to menu action (from game over / pause screen)
        if ui_actions.return_to_menu {
            self.engine.return_to_start_screen();
        }

        // Quit requested from the pause menu (propagated to the caller, which
        // has the event loop).
        let exit_requested = ui_actions.exit_game;

        // Render game world (only when playing)
        if let Some(grid) = self.engine.grid() {
            profile_scope!("render_frame");
            let light_sources = self.engine.light_sources();
            self.render_ctx.render_frame(
                &self.gl,
                crate::render::SceneContents {
                    grid,
                    entities: &tick_result.entities,
                    vfx_effects: self.engine.vfx_effects(),
                    fires: self.engine.fires(),
                },
                crate::render::SceneLighting {
                    player_pos: self.engine.player_visual_pos(),
                    player_light_radius: self.engine.player_light_radius(),
                    player_light_color: self.engine.player_light_color(),
                    light_sources: &light_sources,
                    highlight_tile: self.engine.hover_highlight_tile(),
                },
                self.engine.show_grid_lines(),
            );
        }

        // Render egui
        {
            profile_scope!("egui_paint");
            self.egui_glow.paint(&self.window);
        }

        // Swap buffers
        self.gl_surface.swap_buffers(&self.gl_context).unwrap();

        exit_requested
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
