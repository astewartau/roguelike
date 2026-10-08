//! Rendering context - owns rendering resources separate from game state.

use crate::camera::Camera;
use crate::grid::Grid;
use crate::multi_tileset::MultiTileset;
use crate::renderer::Renderer;
use crate::systems::RenderEntity;
use crate::tile::SpriteSheet;
use crate::ui::UiIcons;
use crate::vfx::{FireEffect, VisualEffect};

use std::sync::Arc;

/// How a frame is lit: the player's own light and every other light source.
///
/// Shared by `RenderContext::render_frame` and `Renderer::render`, which both
/// need the whole trio.
pub struct SceneLighting<'a> {
    pub player_pos: (f32, f32),
    pub player_light_radius: f32,
    /// `(x, y, radius, intensity)` per source.
    pub light_sources: &'a [(f32, f32, f32, f32)],
}

/// What a frame draws: the floor plus everything standing on or over it.
pub struct SceneContents<'a> {
    pub grid: &'a Grid,
    pub entities: &'a [RenderEntity],
    pub vfx_effects: &'a [VisualEffect],
    pub fires: &'a [FireEffect],
}

/// Rendering resources - lives in the application shell (main.rs).
/// Separate from game state to maintain clear boundaries.
pub struct RenderContext {
    pub camera: Camera,
    pub renderer: Renderer,
    pub tileset: MultiTileset,
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
        let tileset = MultiTileset::load(gl, std::path::Path::new("assets/32rogues"))
            .expect("Failed to load tileset");

        // Register tileset textures with egui_glow so they can be used in UI
        let tiles_egui_id = egui_glow
            .painter
            .register_native_texture(tileset.get_native_texture(SpriteSheet::Tiles));
        let rogues_egui_id = egui_glow
            .painter
            .register_native_texture(tileset.get_native_texture(SpriteSheet::Rogues));
        let monsters_egui_id = egui_glow
            .painter
            .register_native_texture(tileset.get_native_texture(SpriteSheet::Monsters));
        let items_egui_id = egui_glow
            .painter
            .register_native_texture(tileset.get_native_texture(SpriteSheet::Items));
        let animated_tiles_egui_id = egui_glow
            .painter
            .register_native_texture(tileset.get_native_texture(SpriteSheet::AnimatedTiles));

        let ui_icons = UiIcons::new(&tileset, tiles_egui_id, rogues_egui_id, monsters_egui_id, items_egui_id, animated_tiles_egui_id);

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
        scene: SceneContents<'_>,
        lighting: SceneLighting<'_>,
        show_grid_lines: bool,
    ) {
        profile_function!();

        let SceneContents { grid, entities, vfx_effects, fires } = scene;

        unsafe {
            use glow::HasContext;
            gl.clear_color(0.0, 0.0, 0.0, 1.0);
            gl.clear(glow::COLOR_BUFFER_BIT);
        }

        {
            profile_scope!("render_tiles");
            self.renderer
                .render(&self.camera, grid, &self.tileset, lighting, show_grid_lines)
                .unwrap();
        }
        {
            profile_scope!("render_decals");
            self.renderer
                .render_decals(&self.camera, grid, &self.tileset)
                .unwrap();
        }
        {
            profile_scope!("render_entities");
            self.renderer
                .render_entities(&self.camera, entities, &self.tileset)
                .unwrap();
        }
        {
            // Tall-grass blades drawn above entities so characters look concealed.
            profile_scope!("render_grass_canopy");
            self.renderer
                .render_grass_canopy(&self.camera, grid, &self.tileset)
                .unwrap();
        }
        {
            profile_scope!("render_vfx");
            self.renderer.render_vfx(&self.camera, vfx_effects);
            self.renderer.render_fire(&self.camera, fires);
        }
    }
}
