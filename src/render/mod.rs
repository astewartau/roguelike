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

/// One light source as the tile shader wants it: already flickered, already
/// coloured, ready to pack straight into the uniform arrays.
///
/// The flicker is evaluated on the CPU, in [`flicker_offset`], rather than in
/// the fragment shader. Both give pixel-identical results, because the light
/// array is uploaded once per frame and the sine is per *light*, not per
/// pixel - but on the CPU it costs two `sin` calls per light per frame instead
/// of two per light per fragment, and it leaves the per-light-type flicker
/// scale free rather than needing a uniform channel of its own.
#[derive(Debug, Clone, Copy)]
pub struct SceneLight {
    /// Tile-space centre of the light.
    pub pos: (f32, f32),
    /// Reach in tiles, with flicker already applied.
    pub radius: f32,
    /// Brightness multiplier, with flicker already applied.
    pub intensity: f32,
    /// Linear RGB tint of the light.
    pub color: (f32, f32, f32),
}

/// How a frame is lit: the player's own light and every other light source.
///
/// Shared by `RenderContext::render_frame` and `Renderer::render`, which both
/// need the whole set.
pub struct SceneLighting<'a> {
    pub player_pos: (f32, f32),
    /// The player's light reach, with its own flicker already applied.
    pub player_light_radius: f32,
    /// Tint of the player's own light. Multiplies the ambient floor as well as
    /// the falloff, so it colours everything the player can see.
    pub player_light_color: (f32, f32, f32),
    pub light_sources: &'a [SceneLight],
    /// Tile to lighten slightly (the one under the mouse cursor), if any.
    /// See `HOVER_TILE_BRIGHTEN`.
    pub highlight_tile: Option<(i32, i32)>,
}

/// The flicker signal for one light at one instant, in `[-scale, scale]`.
///
/// Two sines at incommensurable frequencies (see
/// [`crate::constants::LIGHT_FLICKER_FREQ_SECONDARY`]), summed with weights
/// that add to 1.0 so the result stays inside `[-1, 1]` before `scale` is
/// applied. Because the ratio of the two frequencies is irrational the sum has
/// no period, so a torch never visibly repeats itself.
///
/// `phase` is the light's own seed, which keeps two adjacent braziers from
/// guttering in unison; it is applied to both sines, with the second scaled by
/// [`crate::constants::LIGHT_FLICKER_PHASE_SPREAD`] so the lights differ in
/// waveform and not just in offset.
pub fn flicker_offset(time: f32, phase: f32, scale: f32) -> f32 {
    use crate::constants::*;

    if scale == 0.0 {
        return 0.0;
    }
    let primary = (time * LIGHT_FLICKER_FREQ_PRIMARY + phase).sin();
    let secondary =
        (time * LIGHT_FLICKER_FREQ_SECONDARY + phase * LIGHT_FLICKER_PHASE_SPREAD).sin();
    let combined = primary * LIGHT_FLICKER_PRIMARY_WEIGHT
        + secondary * (1.0 - LIGHT_FLICKER_PRIMARY_WEIGHT);
    combined * scale
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::constants::*;

    /// The two weights sum to 1.0, so the combined signal has to stay inside
    /// `[-scale, scale]` - that is what lets the amplitude constants be read
    /// as "+/- this fraction".
    #[test]
    fn flicker_stays_within_its_scale() {
        for step in 0..20_000 {
            let t = step as f32 * 0.01;
            for phase in [0.0, 1.3, 2.7, 4.9, 6.1] {
                let f = flicker_offset(t, phase, 1.0);
                assert!(
                    (-1.0..=1.0).contains(&f),
                    "flicker {f} out of range at t={t} phase={phase}"
                );
            }
        }
    }

    #[test]
    fn zero_scale_means_dead_steady() {
        for step in 0..100 {
            assert_eq!(flicker_offset(step as f32 * 0.1, 2.0, 0.0), 0.0);
        }
    }

    /// A per-light phase seed is only worth having if it actually decorrelates
    /// two lights, so check that two seeds disagree across a whole sweep
    /// rather than merely at one instant.
    #[test]
    fn different_phases_do_not_gutter_in_unison() {
        let mut max_divergence: f32 = 0.0;
        for step in 0..1_000 {
            let t = step as f32 * 0.01;
            let a = flicker_offset(t, 0.4, 1.0);
            let b = flicker_offset(t, 3.9, 1.0);
            max_divergence = max_divergence.max((a - b).abs());
        }
        assert!(
            max_divergence > 0.5,
            "two phase seeds stayed within {max_divergence} of each other"
        );
    }

    /// The frequency ratio is irrational, so the sum has no period: a sample
    /// one primary-period later should not match the sample at t.
    #[test]
    fn flicker_does_not_repeat_on_the_primary_period() {
        let period = std::f32::consts::TAU / LIGHT_FLICKER_FREQ_PRIMARY;
        let mut matched_everywhere = true;
        for cycle in 1..50 {
            let t = 0.37;
            let later = t + period * cycle as f32;
            if (flicker_offset(t, 1.0, 1.0) - flicker_offset(later, 1.0, 1.0)).abs() > 1e-3 {
                matched_everywhere = false;
                break;
            }
        }
        assert!(!matched_everywhere, "flicker repeated on the primary period");
    }

    /// The bloom bright pass measures `max(r, g, b)`, so every light colour
    /// must peg one channel at 1.0 or it will quietly stop feeding the bloom
    /// that BLOOM_THRESHOLD was tuned for. See the invariant note in
    /// src/constants/lighting.rs.
    #[test]
    fn every_light_colour_pegs_a_channel_at_one() {
        for (name, (r, g, b)) in [
            ("fire", LIGHT_COLOR_FIRE),
            ("player", LIGHT_COLOR_PLAYER),
            ("fungus", LIGHT_COLOR_FUNGUS),
            ("crystal", LIGHT_COLOR_CRYSTAL),
            ("default", LIGHT_COLOR_DEFAULT),
        ] {
            let max = r.max(g).max(b);
            assert!(
                (max - 1.0).abs() < 1e-6,
                "{name} peaks at {max}, not 1.0, so it will under-feed the bloom"
            );
            for c in [r, g, b] {
                assert!((0.0..=1.0).contains(&c), "{name} has an out-of-range channel {c}");
            }
        }
    }
}
