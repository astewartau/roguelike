//! Rendering context - owns rendering resources separate from game state.

use crate::camera::Camera;
use crate::grid::Grid;
use crate::renderer::Renderer;
use crate::systems::RenderEntity;
use crate::tileset::Tileset;
use crate::ui::UiIcons;
use crate::vfx::{FireEffect, VisualEffect};

use std::sync::Arc;

/// Rendering resources - lives in the application shell (main.rs).
/// Separate from game state to maintain clear boundaries.
pub struct RenderContext {
    pub camera: Camera,
    pub renderer: Renderer,
    pub tileset: Tileset,
    pub ui_icons: UiIcons,
}

impl RenderContext {
    /// Create a new render context with the given GL context.
    pub fn new(
        gl: Arc<glow::Context>,
        egui_glow: &mut egui_glow::EguiGlow,
        viewport_width: f32,
        viewport_height: f32,
    ) -> Self {
        let camera = Camera::new(viewport_width, viewport_height);
        let renderer = Renderer::new(gl.clone()).expect("Failed to create renderer");
        let tileset = Tileset::load(gl, std::path::Path::new("assets/minirogue-all.tsj"))
            .expect("Failed to load tileset");

        let tileset_egui_id = egui_glow.painter.register_native_texture(tileset.texture);
        let ui_icons = UiIcons::new(&tileset, tileset_egui_id);

        Self {
            camera,
            renderer,
            tileset,
            ui_icons,
        }
    }

    /// Render a frame with all game content.
    pub fn render_frame(
        &mut self,
        gl: &glow::Context,
        grid: &Grid,
        entities: &[RenderEntity],
        vfx_effects: &[VisualEffect],
        fires: &[FireEffect],
        show_grid_lines: bool,
    ) {
        unsafe {
            use glow::HasContext;
            gl.clear_color(0.1, 0.1, 0.1, 1.0);
            gl.clear(glow::COLOR_BUFFER_BIT);
        }

        self.renderer
            .render(&self.camera, grid, &self.tileset, show_grid_lines)
            .unwrap();
        self.renderer
            .render_decals(&self.camera, grid, &self.tileset)
            .unwrap();
        self.renderer
            .render_entities(&self.camera, entities, &self.tileset)
            .unwrap();
        self.renderer.render_vfx(&self.camera, vfx_effects);
        self.renderer.render_fire(&self.camera, fires);
    }
}
