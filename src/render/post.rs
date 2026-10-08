//! Offscreen post-processing: the game scene is drawn into a framebuffer and
//! then resolved to the screen through fullscreen-quad shader passes.
//!
//! Only the game scene goes through here. egui paints straight to the default
//! framebuffer after [`PostProcess::resolve`] has run, so the UI is never
//! bloomed or vignetted — see the frame order in `main.rs`.
//!
//! The chain is: scene -> bright pass -> horizontal blur -> vertical blur ->
//! composite. The scene target is `RGBA16F` on purpose: the tile shader lets
//! lit pixels reach 1.5 and the fire shader blends additively, and an 8-bit
//! target would clamp exactly the overbright values the bloom exists to find.
//!
//! Adding another screen-space effect (per-floor colour grading, low-HP
//! desaturation, hit distortion, time warp) means adding a uniform group to
//! [`CompositeUniforms`] and a block to [`COMPOSITE_FRAGMENT_SHADER`]. The
//! targets, the quad and the blur chain do not need to change.

use crate::constants::*;
use glow::*;
use std::sync::Arc;

/// Shared by every pass: a quad already in clip space, so no projection.
const POST_VERTEX_SHADER: &str = r#"#version 330 core
layout (location = 0) in vec2 aPos;  // clip space, -1..1

out vec2 vUV;

void main() {
    vUV = aPos * 0.5 + 0.5;
    gl_Position = vec4(aPos, 0.0, 1.0);
}
"#;

/// Keeps only what is brighter than the threshold, with a soft knee below it so
/// flickering flames fade in rather than popping.
const BRIGHT_PASS_FRAGMENT_SHADER: &str = r#"#version 330 core
in vec2 vUV;

uniform sampler2D uScene;
uniform float uThreshold;
uniform float uSoftKnee;

out vec4 FragColor;

void main() {
    vec3 color = texture(uScene, vUV).rgb;

    // Max channel rather than luma: fire is saturated orange, and a luma
    // weighting would read it as much darker than it looks.
    float brightness = max(max(color.r, color.g), color.b);

    float knee = uThreshold * uSoftKnee + 1e-5;
    float soft = clamp(brightness - uThreshold + knee, 0.0, 2.0 * knee);
    soft = soft * soft * 0.25 / knee;

    float contribution = max(soft, brightness - uThreshold) / max(brightness, 1e-5);
    FragColor = vec4(color * contribution, 1.0);
}
"#;

/// One axis of a separable 9-tap Gaussian. `uStep` carries both the direction
/// and the tap spacing, so the same program runs horizontally and vertically.
const BLUR_FRAGMENT_SHADER: &str = r#"#version 330 core
in vec2 vUV;

uniform sampler2D uSource;
uniform vec2 uStep;  // texel size * direction * radius

out vec4 FragColor;

void main() {
    // Standard sigma-2 9-tap weights; these sum to 1.0.
    vec3 sum = texture(uSource, vUV).rgb * 0.2270270270;
    sum += (texture(uSource, vUV + uStep).rgb
          + texture(uSource, vUV - uStep).rgb) * 0.1945945946;
    sum += (texture(uSource, vUV + uStep * 2.0).rgb
          + texture(uSource, vUV - uStep * 2.0).rgb) * 0.1216216216;
    sum += (texture(uSource, vUV + uStep * 3.0).rgb
          + texture(uSource, vUV - uStep * 3.0).rgb) * 0.0540540541;
    sum += (texture(uSource, vUV + uStep * 4.0).rgb
          + texture(uSource, vUV - uStep * 4.0).rgb) * 0.0162162162;

    FragColor = vec4(sum, 1.0);
}
"#;

/// The one pass that writes to the screen. Every screen-space effect lands
/// here, as its own block against its own uniform group.
const COMPOSITE_FRAGMENT_SHADER: &str = r#"#version 330 core
in vec2 vUV;

uniform sampler2D uScene;

// --- Bloom ---
uniform sampler2D uBloom;
uniform float uBloomIntensity;

// --- Vignette ---
uniform float uVignetteStrength;
uniform float uVignetteRadius;

out vec4 FragColor;

void main() {
    vec3 color = texture(uScene, vUV).rgb;

    // Bloom: additive, so bright things get brighter and dark areas are left
    // exactly as the lighting shader left them.
    color += texture(uBloom, vUV).rgb * uBloomIntensity;

    // Vignette: circular in UV space, so it is an ellipse on screen and does
    // not change shape with the window's aspect ratio. Scaling by sqrt(2) puts
    // 0.0 at the centre of the screen and 1.0 exactly in the corners, which is
    // what makes uVignetteRadius a readable "fraction of the way out".
    float dist = length(vUV - vec2(0.5)) * 1.4142136;
    color *= 1.0 - uVignetteStrength * smoothstep(uVignetteRadius, 1.0, dist);

    FragColor = vec4(color, 1.0);
}
"#;

/// Uniform locations for the composite program, grouped by the effect that owns
/// them. A new effect appends a group; existing groups are untouched.
struct CompositeUniforms {
    scene: NativeUniformLocation,
    // Bloom
    bloom: NativeUniformLocation,
    bloom_intensity: NativeUniformLocation,
    // Vignette
    vignette_strength: NativeUniformLocation,
    vignette_radius: NativeUniformLocation,
}

/// The render targets, which are the only part of the pass that depends on the
/// window size. Absent when the window has no area to render into.
struct Targets {
    /// Full-resolution `RGBA16F` target the game scene is drawn into.
    scene_fbo: NativeFramebuffer,
    scene_tex: NativeTexture,
    /// Downsampled ping-pong pair: bright pass and horizontal blur write [0],
    /// the vertical blur writes [1].
    bloom_fbo: [NativeFramebuffer; 2],
    bloom_tex: [NativeTexture; 2],
    bloom_width: u32,
    bloom_height: u32,
}

/// Owns the offscreen targets and the fullscreen passes that resolve them.
///
/// Rendering resources only - no gameplay state, matching the boundary
/// `RenderContext` draws.
pub struct PostProcess {
    gl: Arc<glow::Context>,

    quad_vao: NativeVertexArray,
    quad_vbo: NativeBuffer,

    bright_program: NativeProgram,
    bright_scene_loc: NativeUniformLocation,
    bright_threshold_loc: NativeUniformLocation,
    bright_knee_loc: NativeUniformLocation,

    blur_program: NativeProgram,
    blur_source_loc: NativeUniformLocation,
    blur_step_loc: NativeUniformLocation,

    composite_program: NativeProgram,
    composite: CompositeUniforms,

    targets: Option<Targets>,
    width: u32,
    height: u32,
}

impl PostProcess {
    /// Compile the passes and allocate targets for a `width` x `height` window.
    ///
    /// A zero-area window is not an error: the pass is created with no targets
    /// and [`Self::begin_scene`] reports that there is nowhere to draw, so a
    /// minimised window costs nothing and panics nowhere.
    pub fn new(gl: Arc<glow::Context>, width: u32, height: u32) -> Result<Self, String> {
        unsafe {
            let bright_program = link_post_program(&gl, BRIGHT_PASS_FRAGMENT_SHADER, "bright pass")?;
            let blur_program = link_post_program(&gl, BLUR_FRAGMENT_SHADER, "blur")?;
            let composite_program = link_post_program(&gl, COMPOSITE_FRAGMENT_SHADER, "composite")?;

            let bright_scene_loc = uniform(&gl, bright_program, "uScene")?;
            let bright_threshold_loc = uniform(&gl, bright_program, "uThreshold")?;
            let bright_knee_loc = uniform(&gl, bright_program, "uSoftKnee")?;

            let blur_source_loc = uniform(&gl, blur_program, "uSource")?;
            let blur_step_loc = uniform(&gl, blur_program, "uStep")?;

            let composite = CompositeUniforms {
                scene: uniform(&gl, composite_program, "uScene")?,
                bloom: uniform(&gl, composite_program, "uBloom")?,
                bloom_intensity: uniform(&gl, composite_program, "uBloomIntensity")?,
                vignette_strength: uniform(&gl, composite_program, "uVignetteStrength")?,
                vignette_radius: uniform(&gl, composite_program, "uVignetteRadius")?,
            };

            // Two triangles covering clip space.
            let vertices: [f32; 12] = [
                -1.0, -1.0, //
                1.0, -1.0, //
                1.0, 1.0, //
                -1.0, -1.0, //
                1.0, 1.0, //
                -1.0, 1.0, //
            ];

            let quad_vao = gl
                .create_vertex_array()
                .map_err(|e| format!("Failed to create post-process VAO: {}", e))?;
            gl.bind_vertex_array(Some(quad_vao));

            let quad_vbo = gl
                .create_buffer()
                .map_err(|e| format!("Failed to create post-process VBO: {}", e))?;
            gl.bind_buffer(ARRAY_BUFFER, Some(quad_vbo));
            gl.buffer_data_u8_slice(ARRAY_BUFFER, as_u8_slice(&vertices), STATIC_DRAW);

            gl.enable_vertex_attrib_array(0);
            gl.vertex_attrib_pointer_f32(0, 2, FLOAT, false, 8, 0);

            gl.bind_vertex_array(None);

            let mut post = Self {
                gl,
                quad_vao,
                quad_vbo,
                bright_program,
                bright_scene_loc,
                bright_threshold_loc,
                bright_knee_loc,
                blur_program,
                blur_source_loc,
                blur_step_loc,
                composite_program,
                composite,
                targets: None,
                width: 0,
                height: 0,
            };

            post.resize(width, height)?;
            Ok(post)
        }
    }

    /// Reallocate the targets for a new window size.
    ///
    /// A no-op when the size has not changed. A zero-area size (minimised
    /// window) drops the targets rather than asking the driver for an invalid
    /// framebuffer; the next non-zero resize builds them again.
    pub fn resize(&mut self, width: u32, height: u32) -> Result<(), String> {
        if self.width == width && self.height == height && self.targets.is_some() {
            return Ok(());
        }

        self.width = width;
        self.height = height;
        self.discard_targets();

        if width == 0 || height == 0 {
            return Ok(());
        }

        let (bloom_width, bloom_height) = bloom_target_size(width, height);

        unsafe {
            let (scene_fbo, scene_tex) = create_target(&self.gl, width, height, "scene")?;
            let (bloom_fbo_0, bloom_tex_0) =
                create_target(&self.gl, bloom_width, bloom_height, "bloom ping")?;
            let (bloom_fbo_1, bloom_tex_1) =
                create_target(&self.gl, bloom_width, bloom_height, "bloom pong")?;

            self.targets = Some(Targets {
                scene_fbo,
                scene_tex,
                bloom_fbo: [bloom_fbo_0, bloom_fbo_1],
                bloom_tex: [bloom_tex_0, bloom_tex_1],
                bloom_width,
                bloom_height,
            });
        }

        Ok(())
    }

    /// Bind the offscreen scene target and size the viewport to it.
    ///
    /// Returns `false` when there is nothing to render into, in which case the
    /// caller should skip the frame: nothing is bound and no state is touched.
    pub fn begin_scene(&self) -> bool {
        let Some(targets) = &self.targets else {
            return false;
        };

        unsafe {
            self.gl
                .bind_framebuffer(FRAMEBUFFER, Some(targets.scene_fbo));
            self.gl.viewport(0, 0, self.width as i32, self.height as i32);
        }
        true
    }

    /// Resolve the offscreen scene to the default framebuffer: bright pass,
    /// two-axis blur, then the composite that applies bloom and vignette.
    ///
    /// Leaves the default framebuffer bound and the viewport at window size, so
    /// egui can paint straight afterwards, and restores the renderer's resting
    /// alpha-blend state.
    pub fn resolve(&self) {
        profile_function!();

        let Some(targets) = &self.targets else {
            return;
        };

        unsafe {
            // The passes replace rather than blend; the scene's own alpha must
            // not leak into the composite.
            self.gl.disable(BLEND);
            self.gl.bind_vertex_array(Some(self.quad_vao));

            {
                profile_scope!("post_bright_pass");
                self.gl
                    .bind_framebuffer(FRAMEBUFFER, Some(targets.bloom_fbo[0]));
                self.gl.viewport(
                    0,
                    0,
                    targets.bloom_width as i32,
                    targets.bloom_height as i32,
                );
                self.gl.use_program(Some(self.bright_program));
                bind_texture_unit(&self.gl, 0, targets.scene_tex);
                self.gl.uniform_1_i32(Some(&self.bright_scene_loc), 0);
                self.gl
                    .uniform_1_f32(Some(&self.bright_threshold_loc), BLOOM_THRESHOLD);
                self.gl
                    .uniform_1_f32(Some(&self.bright_knee_loc), BLOOM_SOFT_KNEE);
                self.gl.draw_arrays(TRIANGLES, 0, 6);
            }

            {
                profile_scope!("post_bloom_blur");
                let texel_x = BLOOM_BLUR_RADIUS / targets.bloom_width as f32;
                let texel_y = BLOOM_BLUR_RADIUS / targets.bloom_height as f32;

                self.gl.use_program(Some(self.blur_program));
                self.gl.uniform_1_i32(Some(&self.blur_source_loc), 0);

                // Horizontal: bloom[0] -> bloom[1].
                self.gl
                    .bind_framebuffer(FRAMEBUFFER, Some(targets.bloom_fbo[1]));
                bind_texture_unit(&self.gl, 0, targets.bloom_tex[0]);
                self.gl
                    .uniform_2_f32(Some(&self.blur_step_loc), texel_x, 0.0);
                self.gl.draw_arrays(TRIANGLES, 0, 6);

                // Vertical: bloom[1] -> bloom[0].
                self.gl
                    .bind_framebuffer(FRAMEBUFFER, Some(targets.bloom_fbo[0]));
                bind_texture_unit(&self.gl, 0, targets.bloom_tex[1]);
                self.gl
                    .uniform_2_f32(Some(&self.blur_step_loc), 0.0, texel_y);
                self.gl.draw_arrays(TRIANGLES, 0, 6);
            }

            {
                profile_scope!("post_composite");
                self.gl.bind_framebuffer(FRAMEBUFFER, None);
                self.gl.viewport(0, 0, self.width as i32, self.height as i32);
                self.gl.use_program(Some(self.composite_program));

                bind_texture_unit(&self.gl, 0, targets.scene_tex);
                bind_texture_unit(&self.gl, 1, targets.bloom_tex[0]);
                self.gl.uniform_1_i32(Some(&self.composite.scene), 0);
                self.gl.uniform_1_i32(Some(&self.composite.bloom), 1);

                self.gl
                    .uniform_1_f32(Some(&self.composite.bloom_intensity), BLOOM_INTENSITY);
                self.gl.uniform_1_f32(
                    Some(&self.composite.vignette_strength),
                    VIGNETTE_STRENGTH,
                );
                self.gl
                    .uniform_1_f32(Some(&self.composite.vignette_radius), VIGNETTE_RADIUS);

                self.gl.draw_arrays(TRIANGLES, 0, 6);
            }

            // Leave the unit the rest of the renderer uses pointing at nothing
            // of ours, and put the blend state back how everything else expects
            // to find it.
            bind_texture_unit_none(&self.gl, 1);
            self.gl.active_texture(TEXTURE0);
            self.gl.bind_vertex_array(None);
            self.gl.enable(BLEND);
            self.gl.blend_func(SRC_ALPHA, ONE_MINUS_SRC_ALPHA);
        }
    }

    fn discard_targets(&mut self) {
        if let Some(targets) = self.targets.take() {
            unsafe {
                self.gl.delete_framebuffer(targets.scene_fbo);
                self.gl.delete_texture(targets.scene_tex);
                for fbo in targets.bloom_fbo {
                    self.gl.delete_framebuffer(fbo);
                }
                for tex in targets.bloom_tex {
                    self.gl.delete_texture(tex);
                }
            }
        }
    }
}

impl Drop for PostProcess {
    fn drop(&mut self) {
        self.discard_targets();
        unsafe {
            self.gl.delete_program(self.bright_program);
            self.gl.delete_program(self.blur_program);
            self.gl.delete_program(self.composite_program);
            self.gl.delete_vertex_array(self.quad_vao);
            self.gl.delete_buffer(self.quad_vbo);
        }
    }
}

/// Link one fullscreen pass: the shared vertex shader plus `fragment_src`.
///
/// `label` only ever appears in error messages, which is where a shader that
/// will not compile on someone else's driver has to show up.
unsafe fn link_post_program(
    gl: &glow::Context,
    fragment_src: &str,
    label: &str,
) -> Result<NativeProgram, String> {
    let vertex_shader = gl
        .create_shader(VERTEX_SHADER)
        .map_err(|e| format!("Failed to create {} vertex shader: {}", label, e))?;
    gl.shader_source(vertex_shader, POST_VERTEX_SHADER);
    gl.compile_shader(vertex_shader);
    if !gl.get_shader_compile_status(vertex_shader) {
        let log = gl.get_shader_info_log(vertex_shader);
        gl.delete_shader(vertex_shader);
        return Err(format!("{} vertex shader: {}", label, log));
    }

    let fragment_shader = gl
        .create_shader(FRAGMENT_SHADER)
        .map_err(|e| format!("Failed to create {} fragment shader: {}", label, e))?;
    gl.shader_source(fragment_shader, fragment_src);
    gl.compile_shader(fragment_shader);
    if !gl.get_shader_compile_status(fragment_shader) {
        let log = gl.get_shader_info_log(fragment_shader);
        gl.delete_shader(vertex_shader);
        gl.delete_shader(fragment_shader);
        return Err(format!("{} fragment shader: {}", label, log));
    }

    let program = gl
        .create_program()
        .map_err(|e| format!("Failed to create {} program: {}", label, e))?;
    gl.attach_shader(program, vertex_shader);
    gl.attach_shader(program, fragment_shader);
    gl.link_program(program);
    let linked = gl.get_program_link_status(program);
    gl.delete_shader(vertex_shader);
    gl.delete_shader(fragment_shader);
    if !linked {
        let log = gl.get_program_info_log(program);
        gl.delete_program(program);
        return Err(format!("{} program: {}", label, log));
    }

    Ok(program)
}

unsafe fn uniform(
    gl: &glow::Context,
    program: NativeProgram,
    name: &str,
) -> Result<NativeUniformLocation, String> {
    gl.get_uniform_location(program, name)
        .ok_or_else(|| format!("Failed to get uniform location for {}", name))
}

/// Allocate an `RGBA16F` colour target and the framebuffer that writes to it.
///
/// Half-float because the scene is deliberately overbright in places; linear
/// filtering because the blur samples the downsampled buffer between texels,
/// and clamp-to-edge so neither the blur nor the composite wraps a bright
/// screen edge round to the opposite side.
unsafe fn create_target(
    gl: &glow::Context,
    width: u32,
    height: u32,
    label: &str,
) -> Result<(NativeFramebuffer, NativeTexture), String> {
    let texture = gl
        .create_texture()
        .map_err(|e| format!("Failed to create {} texture: {}", label, e))?;
    gl.bind_texture(TEXTURE_2D, Some(texture));
    gl.tex_image_2d(
        TEXTURE_2D,
        0,
        RGBA16F as i32,
        width as i32,
        height as i32,
        0,
        RGBA,
        HALF_FLOAT,
        None,
    );
    gl.tex_parameter_i32(TEXTURE_2D, TEXTURE_MIN_FILTER, LINEAR as i32);
    gl.tex_parameter_i32(TEXTURE_2D, TEXTURE_MAG_FILTER, LINEAR as i32);
    gl.tex_parameter_i32(TEXTURE_2D, TEXTURE_WRAP_S, CLAMP_TO_EDGE as i32);
    gl.tex_parameter_i32(TEXTURE_2D, TEXTURE_WRAP_T, CLAMP_TO_EDGE as i32);
    gl.bind_texture(TEXTURE_2D, None);

    let framebuffer = match gl.create_framebuffer() {
        Ok(fbo) => fbo,
        Err(e) => {
            gl.delete_texture(texture);
            return Err(format!("Failed to create {} framebuffer: {}", label, e));
        }
    };
    gl.bind_framebuffer(FRAMEBUFFER, Some(framebuffer));
    gl.framebuffer_texture_2d(FRAMEBUFFER, COLOR_ATTACHMENT0, TEXTURE_2D, Some(texture), 0);

    let status = gl.check_framebuffer_status(FRAMEBUFFER);
    gl.bind_framebuffer(FRAMEBUFFER, None);
    if status != FRAMEBUFFER_COMPLETE {
        gl.delete_framebuffer(framebuffer);
        gl.delete_texture(texture);
        return Err(format!(
            "{} framebuffer incomplete (status 0x{:x}) at {}x{}",
            label, status, width, height
        ));
    }

    Ok((framebuffer, texture))
}

unsafe fn bind_texture_unit(gl: &glow::Context, unit: u32, texture: NativeTexture) {
    gl.active_texture(TEXTURE0 + unit);
    gl.bind_texture(TEXTURE_2D, Some(texture));
}

unsafe fn bind_texture_unit_none(gl: &glow::Context, unit: u32) {
    gl.active_texture(TEXTURE0 + unit);
    gl.bind_texture(TEXTURE_2D, None);
}

/// Size of the blur buffers for a given window size.
///
/// Separate and pure so the awkward edges are testable without a GL context:
/// the divisor is clamped in case [`BLOOM_DOWNSAMPLE`] is ever tuned to zero,
/// and the result is clamped to at least one pixel, because a window narrow
/// enough to divide down to nothing would otherwise ask the driver for a
/// zero-width framebuffer.
fn bloom_target_size(width: u32, height: u32) -> (u32, u32) {
    let divisor = BLOOM_DOWNSAMPLE.max(1);
    ((width / divisor).max(1), (height / divisor).max(1))
}

fn as_u8_slice<T>(data: &[T]) -> &[u8] {
    unsafe { std::slice::from_raw_parts(data.as_ptr() as *const u8, std::mem::size_of_val(data)) }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The blur runs at a fraction of the screen resolution; at the default
    /// window size that should be an exact division with nothing clamped.
    #[test]
    fn test_bloom_target_is_the_screen_divided_by_the_downsample() {
        let (w, h) = bloom_target_size(WINDOW_DEFAULT_WIDTH, WINDOW_DEFAULT_HEIGHT);
        assert_eq!(w, WINDOW_DEFAULT_WIDTH / BLOOM_DOWNSAMPLE);
        assert_eq!(h, WINDOW_DEFAULT_HEIGHT / BLOOM_DOWNSAMPLE);
    }

    /// A window small enough that the downsample divides it away must still
    /// produce a framebuffer with pixels in it. Zero would be a GL error, and
    /// the blur divides by these to get its texel size.
    #[test]
    fn test_bloom_target_never_collapses_to_zero() {
        for (width, height) in [(1, 1), (2, 3), (BLOOM_DOWNSAMPLE - 1, 1), (1, 10_000)] {
            let (w, h) = bloom_target_size(width, height);
            assert!(w >= 1, "{}x{} gave a zero-width blur target", width, height);
            assert!(h >= 1, "{}x{} gave a zero-height blur target", width, height);
        }
    }
}
