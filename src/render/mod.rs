//! Rendering context - owns rendering resources separate from game state.

mod post;

use crate::camera::Camera;
use crate::grid::Grid;
use crate::multi_tileset::MultiTileset;
use crate::renderer::Renderer;
use crate::systems::RenderEntity;
use crate::tile::SpriteSheet;
use crate::ui::UiIcons;
use crate::vfx::{FireEffect, VisualEffect};
use post::PostProcess;

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
    /// Offscreen bloom/vignette pass. `None` when the GPU would not give us
    /// the framebuffers or would not compile the shaders, in which case the
    /// scene is drawn straight to the screen - a dungeon without bloom, not a
    /// crash on someone else's driver.
    post: Option<PostProcess>,
}

impl RenderContext {
    /// Create a new render context with the given GL context.
    pub fn new(
        gl: Arc<glow::Context>,
        egui_glow: &mut egui_glow::EguiGlow,
        viewport_width: u32,
        viewport_height: u32,
    ) -> Self {
        let camera = Camera::new(viewport_width as f32, viewport_height as f32);
        let renderer = Renderer::new(gl.clone()).expect("Failed to create renderer");
        let tileset = MultiTileset::load(gl.clone(), std::path::Path::new("assets/32rogues"))
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

        // Post-processing is a visual nicety, not a requirement: if the driver
        // refuses a half-float framebuffer or one of the passes will not
        // compile, say so once and play without it.
        let post = match PostProcess::new(gl, viewport_width, viewport_height) {
            Ok(post) => Some(post),
            Err(e) => {
                eprintln!("Post-processing disabled: {}", e);
                None
            }
        };

        Self {
            camera,
            renderer,
            tileset,
            ui_icons,
            post,
        }
    }

    /// Follow a window resize: the camera's viewport and the offscreen targets
    /// both have to match the new surface.
    ///
    /// A zero-area size (a minimised window) is passed straight through - the
    /// pass drops its targets and reports that there is nothing to draw into,
    /// rather than asking the driver for a framebuffer with no pixels.
    pub fn resize(&mut self, width: u32, height: u32) {
        self.camera.viewport_width = width as f32;
        self.camera.viewport_height = height as f32;

        if let Some(post) = &mut self.post {
            if let Err(e) = post.resize(width, height) {
                eprintln!("Post-processing disabled: {}", e);
                self.post = None;
            }
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

        // Draw the scene into the offscreen target when the post-process pass
        // is available, so bloom and vignette can be applied to the game world
        // without touching the egui pass that paints after us. `Some(false)`
        // means the pass exists but has no pixels to render into - a minimised
        // window, where the right thing to do is nothing at all.
        let post_bound = self.post.as_ref().map(|post| post.begin_scene());
        if post_bound == Some(false) {
            return;
        }

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

        // Resolve the offscreen scene to the screen. This has to land before
        // egui paints, which it does: `main.rs` calls `egui_glow.paint` after
        // `render_frame` returns, so the UI is composited on top of the
        // finished scene and is itself neither bloomed nor vignetted.
        if post_bound == Some(true) {
            if let Some(post) = &self.post {
                post.resolve();
            }
        }
    }
}
