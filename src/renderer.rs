use crate::camera::Camera;
use crate::grid::Grid;
use crate::multi_tileset::MultiTileset;
use crate::systems::RenderEntity;
use crate::tile::SpriteSheet;
use glow::*;
use std::mem;
use std::sync::Arc;

const VERTEX_SHADER_SRC: &str = r#"#version 330 core
layout (location = 0) in vec2 aPos;
layout (location = 1) in vec2 aInstancePos;
layout (location = 2) in vec4 aInstanceUV;  // u0, v0, u1, v1
layout (location = 3) in float aFogMult;
layout (location = 4) in float aAlpha;      // Transparency (1.0 = opaque)
layout (location = 5) in vec3 aTint;        // Color tint (for water, etc.)

uniform mat4 uProjection;

out vec2 vTexCoord;
out vec2 vLocalPos;
out float vFog;
out float vAlpha;
out vec3 vTint;

void main() {
    vec2 worldPos = aInstancePos + aPos;
    gl_Position = uProjection * vec4(worldPos, 0.0, 1.0);

    // Interpolate UV based on vertex position (0-1)
    vTexCoord = mix(aInstanceUV.xy, aInstanceUV.zw, aPos);
    vLocalPos = aPos;
    vFog = aFogMult;
    vAlpha = aAlpha;
    vTint = aTint;
}
"#;

const FRAGMENT_SHADER_SRC: &str = r#"#version 330 core
in vec2 vTexCoord;
in float vFog;
in float vAlpha;
in vec3 vTint;

uniform sampler2D uTileset;

out vec4 FragColor;

void main() {
    vec4 texColor = texture(uTileset, vTexCoord);
    if (texColor.a < 0.1) discard;  // Discard transparent pixels

    vec3 color = texColor.rgb * vFog * vTint;
    FragColor = vec4(color, texColor.a * vAlpha);
}
"#;

const GRID_LINE_VERTEX_SHADER: &str = r#"#version 330 core
layout (location = 0) in vec2 aPos;

uniform mat4 uProjection;

void main() {
    gl_Position = uProjection * vec4(aPos, 0.0, 1.0);
}
"#;

const GRID_LINE_FRAGMENT_SHADER: &str = r#"#version 330 core
out vec4 FragColor;

void main() {
    // Subtle dark grid lines
    FragColor = vec4(0.05, 0.1, 0.2, 0.015);
}
"#;

// VFX shaders for slash effects
const VFX_VERTEX_SHADER: &str = r#"#version 330 core
layout (location = 0) in vec2 aPos;
layout (location = 1) in vec2 aInstancePos;
layout (location = 2) in float aProgress;
layout (location = 3) in float aAngle;

uniform mat4 uProjection;

out vec2 vLocalPos;
out float vProgress;
out float vAngle;

void main() {
    vec2 worldPos = aInstancePos + aPos;
    gl_Position = uProjection * vec4(worldPos, 0.0, 1.0);
    vLocalPos = aPos;
    vProgress = aProgress;
    vAngle = aAngle;
}
"#;

const VFX_FRAGMENT_SHADER: &str = r#"#version 330 core
in vec2 vLocalPos;
in float vProgress;
in float vAngle;

out vec4 FragColor;

void main() {
    // Center UV at (0,0) and scale to -1..1
    vec2 uv = vLocalPos * 2.0 - 1.0;

    // Rotate UV by angle to get diagonal slash
    float c = cos(vAngle);
    float s = sin(vAngle);
    vec2 rotUV = vec2(uv.x * c - uv.y * s, uv.x * s + uv.y * c);

    // Slash is a horizontal line in rotated space (distance from y=0)
    float dist = abs(rotUV.y);

    // Slash thickness
    float thickness = 0.18;
    float edge = smoothstep(thickness, thickness * 0.2, dist);

    // Position along the slash (-1 to 1 in rotated x)
    float alongSlash = rotUV.x;

    // Animated sweep: slash extends from one end to the other
    float sweepStart = mix(-1.8, 0.2, vProgress);
    float sweepEnd = mix(-0.2, 1.8, vProgress);
    float sweep = smoothstep(sweepStart, sweepStart + 0.4, alongSlash) *
                  smoothstep(sweepEnd, sweepEnd - 0.4, alongSlash);

    // Fade out over time
    float fadeOut = 1.0 - smoothstep(0.6, 1.0, vProgress);

    float alpha = edge * sweep * fadeOut;

    // Deep red core, brighter red edge
    vec3 coreColor = vec3(0.4, 0.0, 0.0);   // Deep dark red core
    vec3 edgeColor = vec3(1.0, 0.2, 0.1);   // Bright red edge
    float coreMask = smoothstep(thickness, 0.0, dist);
    vec3 color = mix(edgeColor, coreColor, coreMask);

    if (alpha < 0.01) discard;
    FragColor = vec4(color, alpha);
}
"#;

// Fire particle shader
const FIRE_VERTEX_SHADER: &str = r#"#version 330 core
layout (location = 0) in vec2 aPos;
layout (location = 1) in vec2 aInstancePos;
layout (location = 2) in float aTime;
layout (location = 3) in float aSeed;

uniform mat4 uProjection;

out vec2 vLocalPos;
out float vTime;
out float vSeed;

void main() {
    vec2 worldPos = aInstancePos + aPos;
    gl_Position = uProjection * vec4(worldPos, 0.0, 1.0);
    vLocalPos = aPos;
    vTime = aTime;
    vSeed = aSeed;
}
"#;

const FIRE_FRAGMENT_SHADER: &str = r#"#version 330 core
in vec2 vLocalPos;
in float vTime;
in float vSeed;

out vec4 FragColor;

// Simple pseudo-random noise
float hash(vec2 p) {
    return fract(sin(dot(p, vec2(127.1, 311.7))) * 43758.5453);
}

// Value noise
float noise(vec2 p) {
    vec2 i = floor(p);
    vec2 f = fract(p);
    f = f * f * (3.0 - 2.0 * f); // smoothstep

    float a = hash(i);
    float b = hash(i + vec2(1.0, 0.0));
    float c = hash(i + vec2(0.0, 1.0));
    float d = hash(i + vec2(1.0, 1.0));

    return mix(mix(a, b, f.x), mix(c, d, f.x), f.y);
}

// Fractal brownian motion
float fbm(vec2 p) {
    float sum = 0.0;
    float amp = 0.5;
    for (int i = 0; i < 4; i++) {
        sum += noise(p) * amp;
        p *= 2.0;
        amp *= 0.5;
    }
    return sum;
}

void main() {
    // Center UV and scale
    vec2 uv = vLocalPos * 2.0 - 1.0;

    // Fire rises upward - offset y by time
    float time = vTime * 2.0;
    vec2 fireUV = uv;
    fireUV.y += time * 0.8; // Fire rises
    fireUV += vSeed; // Per-fire variation

    // Create turbulent fire shape using layered noise
    // Base large turbulence
    float n = fbm(fireUV * 3.0 + vec2(0.0, -time * 1.5));
    // Medium detail
    n += fbm(fireUV * 6.0 + vec2(vSeed, -time * 2.0)) * 0.5;
    // Fine detail - faster moving small wisps
    n += fbm(fireUV * 12.0 + vec2(-vSeed * 0.5, -time * 3.0)) * 0.25;
    // Extra fine grain for texture
    float grain = noise(fireUV * 20.0 + vec2(time * 2.0, -time * 4.0)) * 0.15;

    // Fire shape: wider at bottom, narrower at top
    float shape = 1.0 - abs(uv.x) * (1.0 + uv.y * 0.5);
    shape *= 1.0 - uv.y; // Fade toward top
    shape = max(0.0, shape);

    // Combine noise with shape
    float fire = shape * n * 2.0;
    fire = smoothstep(0.1, 0.8, fire);

    // Add grain texture to the body (stronger in middle, fades at edges)
    fire += grain * fire * (1.0 - fire) * 2.0;

    // Flicker effect (reduced intensity)
    float flicker = 0.9 + 0.1 * sin(time * 8.0 + vSeed * 100.0);
    fire *= flicker;

    // Fire colors: orange core -> red-orange -> deep red -> transparent
    // More red/orange focused, less yellow/white
    vec3 color;
    if (fire > 0.75) {
        // Hot core: bright orange-yellow (not white)
        color = mix(vec3(1.0, 0.5, 0.1), vec3(1.0, 0.7, 0.2), (fire - 0.75) / 0.25);
    } else if (fire > 0.45) {
        // Mid flame: orange to bright orange
        color = mix(vec3(0.9, 0.25, 0.0), vec3(1.0, 0.5, 0.1), (fire - 0.45) / 0.3);
    } else if (fire > 0.2) {
        // Outer flame: red-orange
        color = mix(vec3(0.6, 0.1, 0.0), vec3(0.9, 0.25, 0.0), (fire - 0.2) / 0.25);
    } else {
        // Edge: deep red to dark
        color = mix(vec3(0.3, 0.05, 0.0), vec3(0.6, 0.1, 0.0), fire / 0.2);
    }

    // Subtle emissive boost (reduced from before)
    color *= 1.0 + fire * 0.3;

    float alpha = fire * shape * 1.5;
    alpha = clamp(alpha, 0.0, 1.0);

    if (alpha < 0.01) discard;
    FragColor = vec4(color, alpha);
}
"#;

pub struct Renderer {
    gl: Arc<glow::Context>,
    program: NativeProgram,
    vao: NativeVertexArray,
    vbo: NativeBuffer,
    instance_vbo: NativeBuffer,
    projection_loc: NativeUniformLocation,
    tileset_loc: NativeUniformLocation,
    // Grid line rendering
    grid_program: NativeProgram,
    grid_vao: NativeVertexArray,
    grid_vbo: NativeBuffer,
    grid_projection_loc: NativeUniformLocation,
    // VFX rendering
    vfx_program: NativeProgram,
    vfx_vao: NativeVertexArray,
    vfx_vbo: NativeBuffer,
    vfx_instance_vbo: NativeBuffer,
    vfx_projection_loc: NativeUniformLocation,
    // Fire particle rendering
    fire_program: NativeProgram,
    fire_vao: NativeVertexArray,
    fire_vbo: NativeBuffer,
    fire_instance_vbo: NativeBuffer,
    fire_projection_loc: NativeUniformLocation,
}

impl Renderer {
    pub fn new(gl: Arc<glow::Context>) -> Result<Self, String> {
        unsafe {
            // Compile shaders
            let vertex_shader = gl
                .create_shader(VERTEX_SHADER)
                .map_err(|e| format!("Failed to create vertex shader: {}", e))?;
            gl.shader_source(vertex_shader, VERTEX_SHADER_SRC);
            gl.compile_shader(vertex_shader);
            if !gl.get_shader_compile_status(vertex_shader) {
                return Err(gl.get_shader_info_log(vertex_shader));
            }

            let fragment_shader = gl
                .create_shader(FRAGMENT_SHADER)
                .map_err(|e| format!("Failed to create fragment shader: {}", e))?;
            gl.shader_source(fragment_shader, FRAGMENT_SHADER_SRC);
            gl.compile_shader(fragment_shader);
            if !gl.get_shader_compile_status(fragment_shader) {
                return Err(gl.get_shader_info_log(fragment_shader));
            }

            let program = gl
                .create_program()
                .map_err(|e| format!("Failed to create program: {}", e))?;
            gl.attach_shader(program, vertex_shader);
            gl.attach_shader(program, fragment_shader);
            gl.link_program(program);
            if !gl.get_program_link_status(program) {
                return Err(gl.get_program_info_log(program));
            }

            gl.delete_shader(vertex_shader);
            gl.delete_shader(fragment_shader);

            let projection_loc = gl
                .get_uniform_location(program, "uProjection")
                .ok_or("Failed to get projection uniform location")?;
            let tileset_loc = gl
                .get_uniform_location(program, "uTileset")
                .ok_or("Failed to get tileset uniform location")?;

            // Create quad vertices (0,0 to 1,1)
            let vertices: [f32; 12] = [
                0.0, 0.0, // bottom-left
                1.0, 0.0, // bottom-right
                1.0, 1.0, // top-right
                0.0, 0.0, // bottom-left
                1.0, 1.0, // top-right
                0.0, 1.0, // top-left
            ];

            let vao = gl
                .create_vertex_array()
                .map_err(|e| format!("Failed to create VAO: {}", e))?;
            gl.bind_vertex_array(Some(vao));

            let vbo = gl
                .create_buffer()
                .map_err(|e| format!("Failed to create VBO: {}", e))?;
            gl.bind_buffer(ARRAY_BUFFER, Some(vbo));
            gl.buffer_data_u8_slice(
                ARRAY_BUFFER,
                as_u8_slice(&vertices),
                STATIC_DRAW,
            );

            gl.enable_vertex_attrib_array(0);
            gl.vertex_attrib_pointer_f32(0, 2, FLOAT, false, 8, 0);

            // Create instance buffer
            // Layout: pos(2) + uv(4) + fog(1) + alpha(1) + tint(3) = 11 floats = 44 bytes per instance
            let instance_vbo = gl
                .create_buffer()
                .map_err(|e| format!("Failed to create instance VBO: {}", e))?;
            gl.bind_buffer(ARRAY_BUFFER, Some(instance_vbo));

            let stride = 44; // 11 floats * 4 bytes

            // Position attribute (2 floats)
            gl.enable_vertex_attrib_array(1);
            gl.vertex_attrib_pointer_f32(1, 2, FLOAT, false, stride, 0);
            gl.vertex_attrib_divisor(1, 1);

            // UV attribute (4 floats: u0, v0, u1, v1)
            gl.enable_vertex_attrib_array(2);
            gl.vertex_attrib_pointer_f32(2, 4, FLOAT, false, stride, 8);
            gl.vertex_attrib_divisor(2, 1);

            // Fog multiplier attribute (1 float)
            gl.enable_vertex_attrib_array(3);
            gl.vertex_attrib_pointer_f32(3, 1, FLOAT, false, stride, 24);
            gl.vertex_attrib_divisor(3, 1);

            // Alpha attribute (1 float)
            gl.enable_vertex_attrib_array(4);
            gl.vertex_attrib_pointer_f32(4, 1, FLOAT, false, stride, 28);
            gl.vertex_attrib_divisor(4, 1);

            // Tint color attribute (3 floats: r, g, b)
            gl.enable_vertex_attrib_array(5);
            gl.vertex_attrib_pointer_f32(5, 3, FLOAT, false, stride, 32);
            gl.vertex_attrib_divisor(5, 1);

            gl.bind_vertex_array(None);

            // Create grid line shader program
            let grid_vertex_shader = gl
                .create_shader(VERTEX_SHADER)
                .map_err(|e| format!("Failed to create grid vertex shader: {}", e))?;
            gl.shader_source(grid_vertex_shader, GRID_LINE_VERTEX_SHADER);
            gl.compile_shader(grid_vertex_shader);
            if !gl.get_shader_compile_status(grid_vertex_shader) {
                return Err(gl.get_shader_info_log(grid_vertex_shader));
            }

            let grid_fragment_shader = gl
                .create_shader(FRAGMENT_SHADER)
                .map_err(|e| format!("Failed to create grid fragment shader: {}", e))?;
            gl.shader_source(grid_fragment_shader, GRID_LINE_FRAGMENT_SHADER);
            gl.compile_shader(grid_fragment_shader);
            if !gl.get_shader_compile_status(grid_fragment_shader) {
                return Err(gl.get_shader_info_log(grid_fragment_shader));
            }

            let grid_program = gl
                .create_program()
                .map_err(|e| format!("Failed to create grid program: {}", e))?;
            gl.attach_shader(grid_program, grid_vertex_shader);
            gl.attach_shader(grid_program, grid_fragment_shader);
            gl.link_program(grid_program);
            if !gl.get_program_link_status(grid_program) {
                return Err(gl.get_program_info_log(grid_program));
            }

            gl.delete_shader(grid_vertex_shader);
            gl.delete_shader(grid_fragment_shader);

            let grid_projection_loc = gl
                .get_uniform_location(grid_program, "uProjection")
                .ok_or("Failed to get grid projection uniform location")?;

            // Create grid line VAO and VBO
            let grid_vao = gl
                .create_vertex_array()
                .map_err(|e| format!("Failed to create grid VAO: {}", e))?;
            gl.bind_vertex_array(Some(grid_vao));

            let grid_vbo = gl
                .create_buffer()
                .map_err(|e| format!("Failed to create grid VBO: {}", e))?;
            gl.bind_buffer(ARRAY_BUFFER, Some(grid_vbo));

            gl.enable_vertex_attrib_array(0);
            gl.vertex_attrib_pointer_f32(0, 2, FLOAT, false, 8, 0);

            gl.bind_vertex_array(None);

            // Create VFX shader program
            let vfx_vertex_shader = gl
                .create_shader(VERTEX_SHADER)
                .map_err(|e| format!("Failed to create VFX vertex shader: {}", e))?;
            gl.shader_source(vfx_vertex_shader, VFX_VERTEX_SHADER);
            gl.compile_shader(vfx_vertex_shader);
            if !gl.get_shader_compile_status(vfx_vertex_shader) {
                return Err(gl.get_shader_info_log(vfx_vertex_shader));
            }

            let vfx_fragment_shader = gl
                .create_shader(FRAGMENT_SHADER)
                .map_err(|e| format!("Failed to create VFX fragment shader: {}", e))?;
            gl.shader_source(vfx_fragment_shader, VFX_FRAGMENT_SHADER);
            gl.compile_shader(vfx_fragment_shader);
            if !gl.get_shader_compile_status(vfx_fragment_shader) {
                return Err(gl.get_shader_info_log(vfx_fragment_shader));
            }

            let vfx_program = gl
                .create_program()
                .map_err(|e| format!("Failed to create VFX program: {}", e))?;
            gl.attach_shader(vfx_program, vfx_vertex_shader);
            gl.attach_shader(vfx_program, vfx_fragment_shader);
            gl.link_program(vfx_program);
            if !gl.get_program_link_status(vfx_program) {
                return Err(gl.get_program_info_log(vfx_program));
            }

            gl.delete_shader(vfx_vertex_shader);
            gl.delete_shader(vfx_fragment_shader);

            let vfx_projection_loc = gl
                .get_uniform_location(vfx_program, "uProjection")
                .ok_or("Failed to get VFX projection uniform location")?;

            // Create VFX VAO and VBOs
            let vfx_vao = gl
                .create_vertex_array()
                .map_err(|e| format!("Failed to create VFX VAO: {}", e))?;
            gl.bind_vertex_array(Some(vfx_vao));

            // Quad vertices for VFX (same as entity quad)
            let vfx_vbo = gl
                .create_buffer()
                .map_err(|e| format!("Failed to create VFX VBO: {}", e))?;
            gl.bind_buffer(ARRAY_BUFFER, Some(vfx_vbo));
            let vfx_vertices: [f32; 12] = [
                0.0, 0.0,
                1.0, 0.0,
                1.0, 1.0,
                0.0, 0.0,
                1.0, 1.0,
                0.0, 1.0,
            ];
            gl.buffer_data_u8_slice(ARRAY_BUFFER, as_u8_slice(&vfx_vertices), STATIC_DRAW);

            gl.enable_vertex_attrib_array(0);
            gl.vertex_attrib_pointer_f32(0, 2, FLOAT, false, 8, 0);

            // VFX instance buffer: pos(2) + progress(1) + angle(1) = 4 floats = 16 bytes
            let vfx_instance_vbo = gl
                .create_buffer()
                .map_err(|e| format!("Failed to create VFX instance VBO: {}", e))?;
            gl.bind_buffer(ARRAY_BUFFER, Some(vfx_instance_vbo));

            let vfx_stride = 16;

            // Position (2 floats)
            gl.enable_vertex_attrib_array(1);
            gl.vertex_attrib_pointer_f32(1, 2, FLOAT, false, vfx_stride, 0);
            gl.vertex_attrib_divisor(1, 1);

            // Progress (1 float)
            gl.enable_vertex_attrib_array(2);
            gl.vertex_attrib_pointer_f32(2, 1, FLOAT, false, vfx_stride, 8);
            gl.vertex_attrib_divisor(2, 1);

            // Angle (1 float)
            gl.enable_vertex_attrib_array(3);
            gl.vertex_attrib_pointer_f32(3, 1, FLOAT, false, vfx_stride, 12);
            gl.vertex_attrib_divisor(3, 1);

            gl.bind_vertex_array(None);

            // Create Fire shader program
            let fire_vertex_shader = gl
                .create_shader(VERTEX_SHADER)
                .map_err(|e| format!("Failed to create Fire vertex shader: {}", e))?;
            gl.shader_source(fire_vertex_shader, FIRE_VERTEX_SHADER);
            gl.compile_shader(fire_vertex_shader);
            if !gl.get_shader_compile_status(fire_vertex_shader) {
                return Err(gl.get_shader_info_log(fire_vertex_shader));
            }

            let fire_fragment_shader = gl
                .create_shader(FRAGMENT_SHADER)
                .map_err(|e| format!("Failed to create Fire fragment shader: {}", e))?;
            gl.shader_source(fire_fragment_shader, FIRE_FRAGMENT_SHADER);
            gl.compile_shader(fire_fragment_shader);
            if !gl.get_shader_compile_status(fire_fragment_shader) {
                return Err(gl.get_shader_info_log(fire_fragment_shader));
            }

            let fire_program = gl
                .create_program()
                .map_err(|e| format!("Failed to create Fire program: {}", e))?;
            gl.attach_shader(fire_program, fire_vertex_shader);
            gl.attach_shader(fire_program, fire_fragment_shader);
            gl.link_program(fire_program);
            if !gl.get_program_link_status(fire_program) {
                return Err(gl.get_program_info_log(fire_program));
            }

            gl.delete_shader(fire_vertex_shader);
            gl.delete_shader(fire_fragment_shader);

            let fire_projection_loc = gl
                .get_uniform_location(fire_program, "uProjection")
                .ok_or("Failed to get Fire projection uniform location")?;

            // Create Fire VAO and VBOs
            let fire_vao = gl
                .create_vertex_array()
                .map_err(|e| format!("Failed to create Fire VAO: {}", e))?;
            gl.bind_vertex_array(Some(fire_vao));

            // Quad vertices for fire (same as other quads)
            let fire_vbo = gl
                .create_buffer()
                .map_err(|e| format!("Failed to create Fire VBO: {}", e))?;
            gl.bind_buffer(ARRAY_BUFFER, Some(fire_vbo));
            let fire_vertices: [f32; 12] = [
                0.0, 0.0,
                1.0, 0.0,
                1.0, 1.0,
                0.0, 0.0,
                1.0, 1.0,
                0.0, 1.0,
            ];
            gl.buffer_data_u8_slice(ARRAY_BUFFER, as_u8_slice(&fire_vertices), STATIC_DRAW);

            gl.enable_vertex_attrib_array(0);
            gl.vertex_attrib_pointer_f32(0, 2, FLOAT, false, 8, 0);

            // Fire instance buffer: pos(2) + time(1) + seed(1) = 4 floats = 16 bytes
            let fire_instance_vbo = gl
                .create_buffer()
                .map_err(|e| format!("Failed to create Fire instance VBO: {}", e))?;
            gl.bind_buffer(ARRAY_BUFFER, Some(fire_instance_vbo));

            let fire_stride = 16;

            // Position (2 floats)
            gl.enable_vertex_attrib_array(1);
            gl.vertex_attrib_pointer_f32(1, 2, FLOAT, false, fire_stride, 0);
            gl.vertex_attrib_divisor(1, 1);

            // Time (1 float)
            gl.enable_vertex_attrib_array(2);
            gl.vertex_attrib_pointer_f32(2, 1, FLOAT, false, fire_stride, 8);
            gl.vertex_attrib_divisor(2, 1);

            // Seed (1 float)
            gl.enable_vertex_attrib_array(3);
            gl.vertex_attrib_pointer_f32(3, 1, FLOAT, false, fire_stride, 12);
            gl.vertex_attrib_divisor(3, 1);

            gl.bind_vertex_array(None);

            // Navy blue background
            gl.clear_color(0.05, 0.08, 0.15, 1.0);

            // Enable blending for transparency
            gl.enable(BLEND);
            gl.blend_func(SRC_ALPHA, ONE_MINUS_SRC_ALPHA);

            Ok(Self {
                gl,
                program,
                vao,
                vbo,
                instance_vbo,
                projection_loc,
                tileset_loc,
                grid_program,
                grid_vao,
                grid_vbo,
                grid_projection_loc,
                vfx_program,
                vfx_vao,
                vfx_vbo,
                vfx_instance_vbo,
                vfx_projection_loc,
                fire_program,
                fire_vao,
                fire_vbo,
                fire_instance_vbo,
                fire_projection_loc,
            })
        }
    }

    pub fn render(&mut self, camera: &Camera, grid: &Grid, tileset: &MultiTileset, show_grid_lines: bool) -> Result<(), String> {
        unsafe {
            self.gl.clear(COLOR_BUFFER_BIT);

            self.gl.use_program(Some(self.program));
            self.gl.bind_vertex_array(Some(self.vao));

            // Bind Tiles sheet texture (all terrain is from Tiles sheet)
            tileset.bind(&self.gl, SpriteSheet::Tiles, 0);
            self.gl.uniform_1_i32(Some(&self.tileset_loc), 0);

            // Get visible bounds
            let (min_x, max_x, min_y, max_y) = camera.get_visible_bounds();

            // Build instance data for visible tiles
            // Layout: pos(2) + uv(4) + fog(1) + alpha(1) + tint(3) = 11 floats per instance
            let mut instance_data = Vec::new();

            for y in min_y..=max_y {
                for x in min_x..=max_x {
                    if let Some(tile) = grid.get(x, y) {
                        // Only render explored tiles
                        if !tile.explored {
                            continue;
                        }

                        let (sheet, tile_id) = tile.sprite();
                        let uv = tileset.get_uv(sheet, tile_id);
                        // Use pre-computed illumination values for smooth lighting
                        let idx = y as usize * grid.width + x as usize;
                        let fog = grid.illumination.get(idx).copied().unwrap_or(0.5);

                        // Water tiles get a blue tint
                        let tint = match tile.tile_type {
                            crate::tile::TileType::Water => (0.6, 0.8, 1.0),
                            _ => (1.0, 1.0, 1.0),
                        };

                        instance_data.push(x as f32);
                        instance_data.push(y as f32);
                        instance_data.push(uv.u0);
                        instance_data.push(uv.v0);
                        instance_data.push(uv.u1);
                        instance_data.push(uv.v1);
                        instance_data.push(fog);
                        instance_data.push(1.0); // alpha (opaque for tiles)
                        instance_data.push(tint.0);
                        instance_data.push(tint.1);
                        instance_data.push(tint.2);
                    }
                }
            }

            if !instance_data.is_empty() {
                self.gl.bind_buffer(ARRAY_BUFFER, Some(self.instance_vbo));
                self.gl.buffer_data_u8_slice(
                    ARRAY_BUFFER,
                    as_u8_slice(&instance_data),
                    DYNAMIC_DRAW,
                );

                let projection = camera.projection_matrix();
                self.gl.uniform_matrix_4_f32_slice(
                    Some(&self.projection_loc),
                    false,
                    projection.as_ref(),
                );

                let instance_count = instance_data.len() / 11;
                self.gl.draw_arrays_instanced(TRIANGLES, 0, 6, instance_count as i32);
            }

            self.gl.bind_vertex_array(None);

            // Render grid lines on top (if enabled)
            if show_grid_lines {
                self.render_grid_lines(camera, min_x, max_x, min_y, max_y);
            }
        }

        Ok(())
    }

    fn render_grid_lines(&self, camera: &Camera, min_x: i32, max_x: i32, min_y: i32, max_y: i32) {
        unsafe {
            self.gl.use_program(Some(self.grid_program));
            self.gl.bind_vertex_array(Some(self.grid_vao));

            let projection = camera.projection_matrix();
            self.gl.uniform_matrix_4_f32_slice(
                Some(&self.grid_projection_loc),
                false,
                projection.as_ref(),
            );

            let mut line_vertices = Vec::new();

            // Vertical lines
            for x in min_x..=max_x {
                line_vertices.push(x as f32);
                line_vertices.push(min_y as f32);
                line_vertices.push(x as f32);
                line_vertices.push((max_y + 1) as f32);
            }

            // Horizontal lines
            for y in min_y..=max_y {
                line_vertices.push(min_x as f32);
                line_vertices.push(y as f32);
                line_vertices.push((max_x + 1) as f32);
                line_vertices.push(y as f32);
            }

            if !line_vertices.is_empty() {
                self.gl.bind_buffer(ARRAY_BUFFER, Some(self.grid_vbo));
                self.gl.buffer_data_u8_slice(
                    ARRAY_BUFFER,
                    as_u8_slice(&line_vertices),
                    DYNAMIC_DRAW,
                );

                self.gl.draw_arrays(LINES, 0, (line_vertices.len() / 2) as i32);
            }

            self.gl.bind_vertex_array(None);
        }
    }

    /// Render decorative decals (on top of floor, below entities)
    pub fn render_decals(&mut self, camera: &Camera, grid: &Grid, tileset: &MultiTileset) -> Result<(), String> {
        if grid.decals.is_empty() {
            return Ok(());
        }

        unsafe {
            self.gl.use_program(Some(self.program));
            self.gl.bind_vertex_array(Some(self.vao));

            // Bind Tiles sheet texture (all decals are from Tiles sheet)
            tileset.bind(&self.gl, SpriteSheet::Tiles, 0);
            self.gl.uniform_1_i32(Some(&self.tileset_loc), 0);

            // Get visible bounds
            let (min_x, max_x, min_y, max_y) = camera.get_visible_bounds();

            // Build instance data for visible decals
            let mut instance_data = Vec::new();

            for decal in &grid.decals {
                // Skip decals outside visible bounds
                if decal.x < min_x || decal.x > max_x || decal.y < min_y || decal.y > max_y {
                    continue;
                }

                // Check if tile is explored (decals should respect fog of war)
                let is_visible = grid.get(decal.x, decal.y)
                    .map(|t| t.explored)
                    .unwrap_or(false);

                if !is_visible {
                    continue;
                }

                // Use pre-computed illumination values for smooth lighting
                let idx = decal.y as usize * grid.width + decal.x as usize;
                let fog = grid.illumination.get(idx).copied().unwrap_or(0.5);

                let uv = tileset.get_uv(decal.sheet, decal.tile_id);

                instance_data.push(decal.x as f32);
                instance_data.push(decal.y as f32);
                instance_data.push(uv.u0);
                instance_data.push(uv.v0);
                instance_data.push(uv.u1);
                instance_data.push(uv.v1);
                instance_data.push(fog);
                instance_data.push(1.0); // alpha (opaque)
                instance_data.push(1.0); // tint R
                instance_data.push(1.0); // tint G
                instance_data.push(1.0); // tint B
            }

            if !instance_data.is_empty() {
                self.gl.bind_buffer(ARRAY_BUFFER, Some(self.instance_vbo));
                self.gl.buffer_data_u8_slice(
                    ARRAY_BUFFER,
                    as_u8_slice(&instance_data),
                    DYNAMIC_DRAW,
                );

                let projection = camera.projection_matrix();
                self.gl.uniform_matrix_4_f32_slice(
                    Some(&self.projection_loc),
                    false,
                    projection.as_ref(),
                );

                let instance_count = instance_data.len() / 11;
                self.gl.draw_arrays_instanced(TRIANGLES, 0, 6, instance_count as i32);
            }

            self.gl.bind_vertex_array(None);
        }

        Ok(())
    }

    /// Render entities - groups by sprite sheet for multi-texture rendering
    pub fn render_entities(&mut self, camera: &Camera, entities: &[RenderEntity], tileset: &MultiTileset) -> Result<(), String> {
        if entities.is_empty() {
            return Ok(());
        }

        unsafe {
            self.gl.use_program(Some(self.program));
            self.gl.bind_vertex_array(Some(self.vao));

            let projection = camera.projection_matrix();
            self.gl.uniform_matrix_4_f32_slice(Some(&self.projection_loc), false, projection.as_ref());
            self.gl.uniform_1_i32(Some(&self.tileset_loc), 0);

            // Group entities by sprite sheet (order matters for layering)
            // AnimatedTiles first (ground effects like fire), then terrain, characters, monsters, items on top
            let sheets = [SpriteSheet::AnimatedTiles, SpriteSheet::Tiles, SpriteSheet::Rogues, SpriteSheet::Monsters, SpriteSheet::Items];

            for sheet in sheets {
                let mut instance_data = Vec::new();

                for entity in entities {
                    if entity.sprite.sheet != sheet {
                        continue;
                    }

                    let uv = tileset.get_uv(entity.sprite.sheet, entity.sprite.tile_id);

                    instance_data.push(entity.x);
                    instance_data.push(entity.y);
                    instance_data.push(uv.u0);
                    instance_data.push(uv.v0);
                    instance_data.push(uv.u1);
                    instance_data.push(uv.v1);
                    instance_data.push(entity.brightness);
                    instance_data.push(entity.alpha);
                    instance_data.push(1.0); // tint R
                    instance_data.push(1.0); // tint G
                    instance_data.push(1.0); // tint B
                }

                if !instance_data.is_empty() {
                    tileset.bind(&self.gl, sheet, 0);
                    self.gl.bind_buffer(ARRAY_BUFFER, Some(self.instance_vbo));
                    self.gl.buffer_data_u8_slice(ARRAY_BUFFER, as_u8_slice(&instance_data), DYNAMIC_DRAW);

                    let instance_count = instance_data.len() / 11;
                    self.gl.draw_arrays_instanced(TRIANGLES, 0, 6, instance_count as i32);
                }
            }

            // Second pass: render overlay sprites on top (also grouped by sheet)
            for sheet in sheets {
                let mut overlay_data = Vec::new();

                for entity in entities {
                    if let Some(overlay) = &entity.overlay {
                        if overlay.sheet != sheet {
                            continue;
                        }

                        let uv = tileset.get_uv(overlay.sheet, overlay.tile_id);
                        overlay_data.push(entity.x);
                        overlay_data.push(entity.y);
                        overlay_data.push(uv.u0);
                        overlay_data.push(uv.v0);
                        overlay_data.push(uv.u1);
                        overlay_data.push(uv.v1);
                        overlay_data.push(entity.brightness);
                        overlay_data.push(entity.alpha);
                        overlay_data.push(1.0); // tint R
                        overlay_data.push(1.0); // tint G
                        overlay_data.push(1.0); // tint B
                    }
                }

                if !overlay_data.is_empty() {
                    tileset.bind(&self.gl, sheet, 0);
                    self.gl.bind_buffer(ARRAY_BUFFER, Some(self.instance_vbo));
                    self.gl.buffer_data_u8_slice(ARRAY_BUFFER, as_u8_slice(&overlay_data), DYNAMIC_DRAW);

                    let overlay_count = overlay_data.len() / 11;
                    self.gl.draw_arrays_instanced(TRIANGLES, 0, 6, overlay_count as i32);
                }
            }

            self.gl.bind_vertex_array(None);
        }

        Ok(())
    }

    /// Render visual effects (slashes, particles, etc.)
    pub fn render_vfx(&mut self, camera: &Camera, effects: &[crate::vfx::VisualEffect]) {
        if effects.is_empty() {
            return;
        }

        unsafe {
            self.gl.use_program(Some(self.vfx_program));
            self.gl.bind_vertex_array(Some(self.vfx_vao));

            // Build instance data: pos(2) + progress(1) + angle(1)
            let mut instance_data = Vec::new();
            let mut slash_count = 0;

            for effect in effects {
                let angle = match &effect.effect_type {
                    crate::vfx::VfxType::Slash { angle } => *angle,
                    crate::vfx::VfxType::DamageNumber { .. } => continue, // Rendered via egui
                    crate::vfx::VfxType::Fire { .. } => continue, // Rendered separately
                    crate::vfx::VfxType::Alert => continue, // Rendered via egui
                    crate::vfx::VfxType::Explosion { .. } => continue, // Rendered via egui
                    crate::vfx::VfxType::PotionSplash { .. } => continue, // Rendered via egui
                };

                // Center the effect on the tile (effect.x/y is already centered)
                instance_data.push(effect.x - 0.5);
                instance_data.push(effect.y - 0.5);
                instance_data.push(effect.progress());
                instance_data.push(angle);
                slash_count += 1;
            }

            if slash_count == 0 {
                self.gl.bind_vertex_array(None);
                return;
            }

            self.gl.bind_buffer(ARRAY_BUFFER, Some(self.vfx_instance_vbo));
            self.gl.buffer_data_u8_slice(ARRAY_BUFFER, as_u8_slice(&instance_data), DYNAMIC_DRAW);

            let projection = camera.projection_matrix();
            self.gl.uniform_matrix_4_f32_slice(Some(&self.vfx_projection_loc), false, projection.as_ref());

            self.gl.draw_arrays_instanced(TRIANGLES, 0, 6, slash_count);

            self.gl.bind_vertex_array(None);
        }
    }

    /// Render fire particle effects
    pub fn render_fire(&mut self, camera: &Camera, fires: &[crate::vfx::FireEffect]) {
        if fires.is_empty() {
            return;
        }

        unsafe {
            // Use additive blending for fire glow
            self.gl.blend_func(SRC_ALPHA, ONE);

            self.gl.use_program(Some(self.fire_program));
            self.gl.bind_vertex_array(Some(self.fire_vao));

            // Build instance data: pos(2) + time(1) + seed(1)
            let mut instance_data = Vec::new();

            for fire in fires {
                // Center the fire on the tile
                instance_data.push(fire.x - 0.5);
                instance_data.push(fire.y - 0.5);
                instance_data.push(fire.time);
                instance_data.push(fire.seed);
            }

            self.gl.bind_buffer(ARRAY_BUFFER, Some(self.fire_instance_vbo));
            self.gl.buffer_data_u8_slice(ARRAY_BUFFER, as_u8_slice(&instance_data), DYNAMIC_DRAW);

            let projection = camera.projection_matrix();
            self.gl.uniform_matrix_4_f32_slice(Some(&self.fire_projection_loc), false, projection.as_ref());

            self.gl.draw_arrays_instanced(TRIANGLES, 0, 6, fires.len() as i32);

            self.gl.bind_vertex_array(None);

            // Restore normal alpha blending
            self.gl.blend_func(SRC_ALPHA, ONE_MINUS_SRC_ALPHA);
        }
    }
}

impl Drop for Renderer {
    fn drop(&mut self) {
        unsafe {
            self.gl.delete_program(self.program);
            self.gl.delete_vertex_array(self.vao);
            self.gl.delete_buffer(self.vbo);
            self.gl.delete_buffer(self.instance_vbo);
            self.gl.delete_program(self.grid_program);
            self.gl.delete_vertex_array(self.grid_vao);
            self.gl.delete_buffer(self.grid_vbo);
            self.gl.delete_program(self.vfx_program);
            self.gl.delete_vertex_array(self.vfx_vao);
            self.gl.delete_buffer(self.vfx_vbo);
            self.gl.delete_buffer(self.vfx_instance_vbo);
            self.gl.delete_program(self.fire_program);
            self.gl.delete_vertex_array(self.fire_vao);
            self.gl.delete_buffer(self.fire_vbo);
            self.gl.delete_buffer(self.fire_instance_vbo);
        }
    }
}

fn as_u8_slice<T>(data: &[T]) -> &[u8] {
    unsafe {
        std::slice::from_raw_parts(
            data.as_ptr() as *const u8,
            data.len() * mem::size_of::<T>(),
        )
    }
}
