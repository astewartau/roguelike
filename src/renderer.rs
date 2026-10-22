use crate::camera::Camera;
use crate::constants::{LIGHT_AMBIENT_TINT, LIGHT_EDITOR_AMBIENT, LIGHT_PLAYER_AMBIENT, LIGHT_PLAYER_FOCUSED};
use crate::constants::HOVER_TILE_BRIGHTEN;
use crate::grid::Grid;
use crate::multi_tileset::MultiTileset;
use crate::systems::RenderEntity;
use crate::tile::SpriteSheet;
use glow::*;
use std::sync::Arc;

/// Texture unit the light map is bound to (sprites use unit 0).
const LIGHT_MAP_UNIT: u32 = 1;
/// Texture unit the map of how seen each tile is lives on (`RevealFade`).
const SEEN_MAP_UNIT: u32 = 2;

const VERTEX_SHADER_SRC: &str = r#"#version 330 core
layout (location = 0) in vec2 aPos;
layout (location = 1) in vec2 aInstancePos;
layout (location = 2) in vec4 aInstanceUV;  // u0, v0, u1, v1
layout (location = 3) in float aFogMult;    // The tile's illumination (unused: see uSeen)
layout (location = 4) in float aAlpha;      // Transparency (1.0 = opaque)
layout (location = 5) in vec3 aTint;        // Color tint (for water, etc.)

uniform mat4 uProjection;

out vec2 vTexCoord;
out vec2 vWorldPos;
out float vFogBase;
out float vAlpha;
out vec3 vTint;

void main() {
    vec2 worldPos = aInstancePos + aPos;
    gl_Position = uProjection * vec4(worldPos, 0.0, 1.0);

    // Interpolate UV based on vertex position (0-1)
    vTexCoord = mix(aInstanceUV.xy, aInstanceUV.zw, aPos);
    vWorldPos = worldPos;
    vFogBase = aFogMult;
    vAlpha = aAlpha;
    vTint = aTint;
}
"#;

/// Tile fragment shader. Light from the world arrives pre-computed in
/// `uLightMap` (see `systems::light::LightMap`), one RGB texel per tile,
/// sampled with linear filtering so it still blends smoothly across tiles.
/// Occlusion lives in that map, which is why walls and closed doors now stop
/// light here too: a per-fragment distance test cannot know about them.
const FRAGMENT_SHADER_SRC: &str = r#"#version 330 core
in vec2 vTexCoord;
in vec2 vWorldPos;
in float vFogBase;
in float vAlpha;
in vec3 vTint;

uniform sampler2D uTileset;
uniform vec2 uPlayerPos;
uniform float uPlayerLightRadius;
uniform vec3 uPlayerLightColor;
uniform float uPlayerAmbient;  // brightness everywhere you can see
uniform float uPlayerFocused;  // extra brightness of the pool at your feet
uniform float uAmbientTint;
uniform sampler2D uLightMap;  // RGB light from the world, one texel per tile
uniform vec2 uGridSize;       // grid size in tiles, to map world -> texel
uniform vec3 uSkyLight;       // the sky's light over an outdoor map; 0 underground
uniform float uFogShare;      // remembered tiles get this share of the ambient light
uniform sampler2D uSeen;      // how seen each tile is, 0 (remembered) to 1 (in view)

out vec4 FragColor;

void main() {
    vec4 texColor = texture(uTileset, vTexCoord);
    if (texColor.a < 0.1) discard;  // Discard transparent pixels

    // Accumulated light is a colour, not a scalar: each source adds its own
    // tint, so a tile lit by a brazier and a crystal at once gets both.
    // The ambient light: the player's soft light everywhere in view and the
    // sky's (daylight, dusk or moonlight). The focused falloff below always
    // carries the player's light's full colour; this flat term only takes
    // `uAmbientTint` of it. Tinting the ambient fully washes every surface in
    // the room with the light's hue, and a warm wash over this tileset's
    // blue-grey stone reads as khaki rather than as firelight.
    vec3 ambient = mix(vec3(1.0), uPlayerLightColor, uAmbientTint) * uPlayerAmbient + uSkyLight;
    // A remembered tile (explored, out of sight) is lit by a share of that
    // and nothing else: not by a lamp or a fire it cannot see.
    vec3 remembered = ambient * uFogShare;

    // What a tile in view is lit by: the ambient, the pool of the player's
    // light at their feet, and every lamp and fire.
    float distToPlayer = distance(vWorldPos, uPlayerPos);
    float t = clamp(1.0 - distToPlayer / uPlayerLightRadius, 0.0, 1.0);
    vec3 lit = ambient + uPlayerLightColor * (uPlayerFocused * t * t);
    // Light from braziers, campfires, fires, fungus...: traced on the CPU
    // through walls, doors and objects, tile by tile. Tile (x, y) spans
    // world [x, x+1), so dividing by the grid size lands each tile's centre
    // on its texel's centre.
    lit += texture(uLightMap, vWorldPos / uGridSize).rgb;
    lit = min(lit, vec3(1.5));  // Allow slight overbrightness

    // How seen this spot is (`RevealFade::seen_map`): 1 in view, 0
    // remembered, between where a tile fades in or out of view and across
    // the soft edge of your sight. Sampled smoothly, so it's a gradient,
    // not a tile's hard edge.
    float seen = texture(uSeen, vWorldPos / uGridSize).r;
    vec3 light = mix(remembered, lit, seen);

    vec3 color = texColor.rgb * light * vTint;
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
    // Per-pixel lighting uniforms
    player_pos_loc: NativeUniformLocation,
    player_light_radius_loc: NativeUniformLocation,
    player_light_color_loc: NativeUniformLocation,
    player_ambient_loc: NativeUniformLocation,
    player_focused_loc: NativeUniformLocation,
    ambient_tint_loc: NativeUniformLocation,
    sky_light_loc: NativeUniformLocation,
    fog_level_loc: NativeUniformLocation,
    light_map_loc: NativeUniformLocation,
    grid_size_loc: NativeUniformLocation,
    seen_map_loc: NativeUniformLocation,
    /// How seen each tile is (`RevealFade::seen_map`), uploaded every frame
    /// on texture unit `SEEN_MAP_UNIT`.
    seen_map_texture: NativeTexture,
    /// Per-frame light map (see `systems::light::LightMap`), re-uploaded
    /// every frame on texture unit `LIGHT_MAP_UNIT`.
    light_map_texture: NativeTexture,
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
            let player_pos_loc = gl
                .get_uniform_location(program, "uPlayerPos")
                .ok_or("Failed to get player pos uniform location")?;
            let player_light_radius_loc = gl
                .get_uniform_location(program, "uPlayerLightRadius")
                .ok_or("Failed to get player light radius uniform location")?;
            let player_light_color_loc = gl
                .get_uniform_location(program, "uPlayerLightColor")
                .ok_or("Failed to get player light color uniform location")?;
            let player_ambient_loc = gl
                .get_uniform_location(program, "uPlayerAmbient")
                .ok_or("Failed to get player ambient uniform location")?;
            let player_focused_loc = gl
                .get_uniform_location(program, "uPlayerFocused")
                .ok_or("Failed to get player focused uniform location")?;
            let ambient_tint_loc = gl
                .get_uniform_location(program, "uAmbientTint")
                .ok_or("Failed to get ambient tint uniform location")?;
            let sky_light_loc = gl
                .get_uniform_location(program, "uSkyLight")
                .ok_or("Failed to get sky light uniform location")?;
            let fog_level_loc = gl
                .get_uniform_location(program, "uFogShare")
                .ok_or("Failed to get fog level uniform location")?;
            let light_map_loc = gl
                .get_uniform_location(program, "uLightMap")
                .ok_or("Failed to get light map uniform location")?;
            let grid_size_loc = gl
                .get_uniform_location(program, "uGridSize")
                .ok_or("Failed to get grid size uniform location")?;

            // Light map texture: float RGB so light can exceed 1.0 near a
            // source, linear filtering so it blends between tile centres, and
            // clamped edges so the map's border doesn't wrap round.
            let seen_map_loc = gl
                .get_uniform_location(program, "uSeen")
                .ok_or("Failed to get seen map uniform location")?;
            let seen_map_texture = gl
                .create_texture()
                .map_err(|e| format!("Failed to create seen map texture: {}", e))?;
            gl.bind_texture(TEXTURE_2D, Some(seen_map_texture));
            gl.tex_parameter_i32(TEXTURE_2D, TEXTURE_MIN_FILTER, LINEAR as i32);
            gl.tex_parameter_i32(TEXTURE_2D, TEXTURE_MAG_FILTER, LINEAR as i32);
            gl.tex_parameter_i32(TEXTURE_2D, TEXTURE_WRAP_S, CLAMP_TO_EDGE as i32);
            gl.tex_parameter_i32(TEXTURE_2D, TEXTURE_WRAP_T, CLAMP_TO_EDGE as i32);
            gl.bind_texture(TEXTURE_2D, None);

            let light_map_texture = gl
                .create_texture()
                .map_err(|e| format!("Failed to create light map texture: {}", e))?;
            gl.bind_texture(TEXTURE_2D, Some(light_map_texture));
            gl.tex_parameter_i32(TEXTURE_2D, TEXTURE_MIN_FILTER, LINEAR as i32);
            gl.tex_parameter_i32(TEXTURE_2D, TEXTURE_MAG_FILTER, LINEAR as i32);
            gl.tex_parameter_i32(TEXTURE_2D, TEXTURE_WRAP_S, CLAMP_TO_EDGE as i32);
            gl.tex_parameter_i32(TEXTURE_2D, TEXTURE_WRAP_T, CLAMP_TO_EDGE as i32);
            gl.bind_texture(TEXTURE_2D, None);

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

            // Black background for unexplored areas
            gl.clear_color(0.0, 0.0, 0.0, 1.0);

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
                player_pos_loc,
                player_light_radius_loc,
                player_light_color_loc,
                player_ambient_loc,
                player_focused_loc,
                ambient_tint_loc,
                sky_light_loc,
                fog_level_loc,
                light_map_loc,
                grid_size_loc,
                seen_map_loc,
                seen_map_texture,
                light_map_texture,
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

    pub fn render(
        &mut self,
        camera: &Camera,
        grid: &Grid,
        tileset: &MultiTileset,
        lighting: crate::render::SceneLighting<'_>,
        reveal: &crate::render::RevealFade,
        show_grid_lines: bool,
    ) -> Result<(), String> {
        let crate::render::SceneLighting {
            player_pos,
            even,
            player_light_radius,
            player_light_color,
            light_map,
            highlight_tile,
            sky,
            fog,
        } = lighting;
        unsafe {
            self.gl.clear(COLOR_BUFFER_BIT);

            self.gl.use_program(Some(self.program));
            self.gl.bind_vertex_array(Some(self.vao));

            self.gl.uniform_1_i32(Some(&self.tileset_loc), 0);

            // Set lighting uniforms. Without the player's light (the editor,
            // player or no player): an even, untinted brightness and no pool
            // anywhere, or under an open sky, only the sky's light, so the
            // editor shows the time of day as it will look. The player's own
            // light fades as daylight comes up: by day the sky lights you.
            // As in play, editor or not: the player's light (an ambient
            // everywhere in view and a pool at their feet) fading as daylight
            // comes up. With no player there's no pool, the ambient the same.
            // Lit evenly (the editor's toggle), a flat bright ambient instead,
            // or only the sky outdoors.
            let night = 1.0 - sky.map_or(0.0, |s| s.daylight);
            let (pos, color, ambient, focused) = match (even, player_pos, sky) {
                (true, _, Some(_)) => ((0.0, 0.0), (1.0, 1.0, 1.0), 0.0, 0.0),
                (true, _, None) => ((0.0, 0.0), (1.0, 1.0, 1.0), LIGHT_EDITOR_AMBIENT, 0.0),
                (false, Some(pos), _) => {
                    (pos, player_light_color, LIGHT_PLAYER_AMBIENT * night, LIGHT_PLAYER_FOCUSED * night)
                }
                (false, None, _) => ((0.0, 0.0), player_light_color, LIGHT_PLAYER_AMBIENT * night, 0.0),
            };
            let sky_rgb = sky.map_or((0.0, 0.0, 0.0), |s| s.rgb());
            self.gl.uniform_3_f32(Some(&self.sky_light_loc), sky_rgb.0, sky_rgb.1, sky_rgb.2);
            // How remembered tiles are lit: a share of the ambient light.
            self.gl.uniform_1_f32(Some(&self.fog_level_loc), fog);
            self.gl.uniform_2_f32(Some(&self.player_pos_loc), pos.0, pos.1);
            self.gl.uniform_1_f32(Some(&self.player_light_radius_loc), player_light_radius);
            self.gl.uniform_3_f32(Some(&self.player_light_color_loc), color.0, color.1, color.2);
            self.gl.uniform_1_f32(Some(&self.player_ambient_loc), ambient);
            self.gl.uniform_1_f32(Some(&self.player_focused_loc), focused);

            self.gl
                .uniform_1_f32(Some(&self.ambient_tint_loc), LIGHT_AMBIENT_TINT);

            // Upload this frame's light map to its own texture unit. Every
            // later pass with this program binds sprites to unit 0 only, so
            // the map stays bound for decals, entities and grass too.
            self.gl.active_texture(TEXTURE0 + LIGHT_MAP_UNIT);
            self.gl.bind_texture(TEXTURE_2D, Some(self.light_map_texture));
            if light_map.width > 0 && light_map.height > 0 {
                self.gl.tex_image_2d(
                    TEXTURE_2D,
                    0,
                    RGB32F as i32,
                    light_map.width as i32,
                    light_map.height as i32,
                    0,
                    RGB,
                    FLOAT,
                    Some(as_u8_slice(&light_map.rgb)),
                );
            }
            self.gl.active_texture(TEXTURE0);
            self.gl.uniform_1_i32(Some(&self.light_map_loc), LIGHT_MAP_UNIT as i32);

            // And how seen each tile is, on its own unit: in view, fading,
            // remembered, softened at the edge of sight. Every pass with this
            // program reads it, so sprites fade with the tiles they're on.
            let seen = reveal.seen_map(grid.width, grid.height);
            self.gl.active_texture(TEXTURE0 + SEEN_MAP_UNIT);
            self.gl.bind_texture(TEXTURE_2D, Some(self.seen_map_texture));
            if grid.width > 0 && grid.height > 0 {
                self.gl.tex_image_2d(
                    TEXTURE_2D,
                    0,
                    R32F as i32,
                    grid.width as i32,
                    grid.height as i32,
                    0,
                    RED,
                    FLOAT,
                    Some(as_u8_slice(&seen)),
                );
            }
            self.gl.active_texture(TEXTURE0);
            self.gl.uniform_1_i32(Some(&self.seen_map_loc), SEEN_MAP_UNIT as i32);
            self.gl.uniform_2_f32(
                Some(&self.grid_size_loc),
                light_map.width.max(1) as f32,
                light_map.height.max(1) as f32,
            );

            // Get visible bounds
            let (min_x, max_x, min_y, max_y) = camera.get_visible_bounds();

            // Build instance data for visible tiles, one batch per sheet
            // (terrain is mostly Tiles; outdoor ground is on its own sheet),
            // in two layers: the ground, then whatever stands on it.
            // Layout: pos(2) + uv(4) + fog(1) + alpha(1) + tint(3) = 11 floats per instance
            let mut ground_batches: Vec<(SpriteSheet, Vec<f32>)> = Vec::new();
            let mut top_batches: Vec<(SpriteSheet, Vec<f32>)> = Vec::new();
            let push = |batches: &mut Vec<(SpriteSheet, Vec<f32>)>, sheet: SpriteSheet, instance: [f32; 11]| {
                match batches.iter_mut().find(|(s, _)| *s == sheet) {
                    Some((_, data)) => data.extend_from_slice(&instance),
                    None => batches.push((sheet, instance.to_vec())),
                }
            };

            for y in min_y..=max_y {
                for x in min_x..=max_x {
                    if let Some(tile) = grid.get(x, y) {
                        // Only render explored tiles (or ones in view)
                        if !tile.known() {
                            continue;
                        }

                        // What's drawn: the ground, and what stands on it if
                        // that has ground of its own beneath (`Tile::under`).
                        // Tall grass draws only its ground here; the grass
                        // blades themselves are drawn above entities in
                        // render_grass_canopy (so characters appear concealed
                        // in the grass). The dungeon's own tall grass has no
                        // ground recorded, and grows on grassy floor.
                        let on_top = tile.tile_type.stands_on_ground();
                        let (ground, top) = match tile.under.filter(|_| on_top) {
                            Some(under) => {
                                let top = (tile.tile_type != crate::tile::TileType::TallGrass).then(|| tile.sprite());
                                ((under.sprite(), under.tile_type), top)
                            }
                            None if tile.tile_type == crate::tile::TileType::TallGrass => {
                                let variants = &crate::tile::tile_ids::GRASS_VARIANTS;
                                let sprite = variants[((x + y).rem_euclid(variants.len() as i32)) as usize];
                                ((sprite, crate::tile::TileType::Grass), None)
                            }
                            None => ((tile.sprite(), tile.tile_type), None),
                        };
                        // Use pre-computed illumination for fog of war (low values)
                        // Per-pixel lighting handles visible tiles
                        let idx = y as usize * grid.width + x as usize;
                        let fog = grid.illumination.get(idx).copied().unwrap_or(0.5);
                        // Fading into view or out of it (`RevealFade`).
                        // The tile under the cursor is drawn a little lighter.
                        let lift = if highlight_tile == Some((x, y)) { HOVER_TILE_BRIGHTEN } else { 1.0 };

                        let layers = [Some((ground, false)), top.map(|sprite| ((sprite, tile.tile_type), true))];
                        for ((sprite, terrain), is_top) in layers.into_iter().flatten() {
                            // A sheet that isn't there (the outdoor one is
                            // optional) draws the terrain's stock look instead.
                            let (sheet, tile_id) =
                                if tileset.has_tile(sprite.0, sprite.1) { sprite } else { terrain.fallback_sprite() };
                            let uv = tileset.get_uv(sheet, tile_id);
                            // Water tiles get a blue tint
                            let tint = match terrain {
                                crate::tile::TileType::Water => (0.6, 0.8, 1.0),
                                _ => (1.0, 1.0, 1.0),
                            };
                            let instance = [
                                x as f32,
                                y as f32,
                                uv.u0,
                                uv.v0,
                                uv.u1,
                                uv.v1,
                                fog,
                                1.0, // alpha (opaque for tiles)
                                tint.0 * lift,
                                tint.1 * lift,
                                tint.2 * lift,
                            ];
                            push(if is_top { &mut top_batches } else { &mut ground_batches }, sheet, instance);
                        }
                    }
                }
            }

            let projection = camera.projection_matrix();
            self.gl.uniform_matrix_4_f32_slice(
                Some(&self.projection_loc),
                false,
                projection.as_ref(),
            );
            for (sheet, instance_data) in ground_batches.iter().chain(&top_batches) {
                self.draw_sprite_batch(*sheet, instance_data, tileset);
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
    pub fn render_decals(
        &mut self,
        camera: &Camera,
        grid: &Grid,
        tileset: &MultiTileset,
    ) -> Result<(), String> {
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
                    .map(|t| t.known())
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

    /// Render the tall-grass "canopy" — the grass blades drawn on top of entities
    /// so characters standing in grass appear concealed by it. The grassy ground
    /// itself is drawn in the base tile pass (see `render`).
    pub fn render_grass_canopy(
        &mut self,
        camera: &Camera,
        grid: &Grid,
        tileset: &MultiTileset,
    ) -> Result<(), String> {
        unsafe {
            self.gl.use_program(Some(self.program));
            self.gl.bind_vertex_array(Some(self.vao));

            tileset.bind(&self.gl, SpriteSheet::Tiles, 0);
            self.gl.uniform_1_i32(Some(&self.tileset_loc), 0);

            let (min_x, max_x, min_y, max_y) = camera.get_visible_bounds();
            let grass_uv = {
                let (sheet, id) = crate::tile::tile_ids::TALL_GRASS;
                tileset.get_uv(sheet, id)
            };

            let mut instance_data = Vec::new();
            for y in min_y..=max_y {
                for x in min_x..=max_x {
                    let Some(tile) = grid.get(x, y) else { continue };
                    if tile.tile_type != crate::tile::TileType::TallGrass || !tile.known() {
                        continue;
                    }
                    let idx = y as usize * grid.width + x as usize;
                    let fog = grid.illumination.get(idx).copied().unwrap_or(0.5);

                    instance_data.push(x as f32);
                    instance_data.push(y as f32);
                    instance_data.push(grass_uv.u0);
                    instance_data.push(grass_uv.v0);
                    instance_data.push(grass_uv.u1);
                    instance_data.push(grass_uv.v1);
                    instance_data.push(fog);
                    instance_data.push(1.0); // alpha
                    instance_data.push(1.0); // tint r
                    instance_data.push(1.0); // tint g
                    instance_data.push(1.0); // tint b
                }
            }

            if !instance_data.is_empty() {
                self.gl.bind_buffer(ARRAY_BUFFER, Some(self.instance_vbo));
                self.gl.buffer_data_u8_slice(ARRAY_BUFFER, as_u8_slice(&instance_data), DYNAMIC_DRAW);

                let projection = camera.projection_matrix();
                self.gl.uniform_matrix_4_f32_slice(Some(&self.projection_loc), false, projection.as_ref());

                let instance_count = instance_data.len() / 11;
                self.gl.draw_arrays_instanced(TRIANGLES, 0, 6, instance_count as i32);
            }

            self.gl.bind_vertex_array(None);
        }

        Ok(())
    }

    /// Render entities - groups by sprite sheet for multi-texture rendering
    pub fn render_entities(
        &mut self,
        camera: &Camera,
        entities: &[RenderEntity],
        tileset: &MultiTileset,
    ) -> Result<(), String> {
        if entities.is_empty() {
            return Ok(());
        }

        unsafe {
            self.gl.use_program(Some(self.program));
            self.gl.bind_vertex_array(Some(self.vao));

            let projection = camera.projection_matrix();
            self.gl.uniform_matrix_4_f32_slice(Some(&self.projection_loc), false, projection.as_ref());
            self.gl.uniform_1_i32(Some(&self.tileset_loc), 0);

            // Draw in the order `collect_renderables` produced (ground, then
            // actors, then the player), with each entity's overlay (a held
            // weapon, say) straight after it. Consecutive sprites on the same
            // sheet share one instanced draw; a texture switch starts a new
            // batch. This used to draw sheet-by-sheet in a fixed order with
            // Items last, so an item dropped on the floor drew over whoever
            // was standing on it.
            let mut batch_sheet: Option<SpriteSheet> = None;
            let mut instance_data: Vec<f32> = Vec::new();

            for entity in entities {
                // Fire overlays render in a final pass, below.
                if entity.effects & crate::systems::rendering::effects::BURNING != 0 {
                    continue;
                }

                let layers = std::iter::once((entity.sprite, entity.tint, (0.0, 0.0), false))
                    .chain(entity.overlays.iter().map(|o| (o.sprite, (1.0, 1.0, 1.0), o.offset, o.mirrored)));
                for (sprite, tint, (dx, dy), mirrored) in layers {
                    // A held torch or a badge on a sheet that predates them
                    // is left out rather than drawn as a smear.
                    if !tileset.has_tile(sprite.sheet, sprite.tile_id) {
                        continue;
                    }
                    if batch_sheet != Some(sprite.sheet) {
                        if let Some(sheet) = batch_sheet {
                            self.draw_sprite_batch(sheet, &instance_data, tileset);
                        }
                        instance_data.clear();
                        batch_sheet = Some(sprite.sheet);
                    }

                    let uv = tileset.get_uv(sprite.sheet, sprite.tile_id);
                    // Swapping the u ends draws the cell flipped left to right.
                    let (u0, u1) = if mirrored { (uv.u1, uv.u0) } else { (uv.u0, uv.u1) };
                    // Fading into view with the tile it's on.
                    instance_data.extend_from_slice(&[
                        entity.x + dx,
                        entity.y + dy,
                        u0,
                        uv.v0,
                        u1,
                        uv.v1,
                        entity.brightness,
                        entity.alpha,
                        tint.0,
                        tint.1,
                        tint.2,
                    ]);
                }
            }
            if let Some(sheet) = batch_sheet {
                self.draw_sprite_batch(sheet, &instance_data, tileset);
            }

            // Third pass: render fire overlays on top of everything
            {
                let mut fire_data = Vec::new();

                for entity in entities {
                    if entity.effects & crate::systems::rendering::effects::BURNING == 0 {
                        continue;
                    }

                    let uv = tileset.get_uv(entity.sprite.sheet, entity.sprite.tile_id);

                    fire_data.push(entity.x);
                    fire_data.push(entity.y);
                    fire_data.push(uv.u0);
                    fire_data.push(uv.v0);
                    fire_data.push(uv.u1);
                    fire_data.push(uv.v1);
                    fire_data.push(entity.brightness);
                    fire_data.push(entity.alpha);
                    fire_data.push(1.0); // tint R
                    fire_data.push(1.0); // tint G
                    fire_data.push(1.0); // tint B
                }

                if !fire_data.is_empty() {
                    tileset.bind(&self.gl, SpriteSheet::AnimatedTiles, 0);
                    self.gl.bind_buffer(ARRAY_BUFFER, Some(self.instance_vbo));
                    self.gl.buffer_data_u8_slice(ARRAY_BUFFER, as_u8_slice(&fire_data), DYNAMIC_DRAW);

                    let fire_count = fire_data.len() / 11;
                    self.gl.draw_arrays_instanced(TRIANGLES, 0, 6, fire_count as i32);
                }
            }

            self.gl.bind_vertex_array(None);
        }

        Ok(())
    }

    /// Upload one batch of sprite instances (11 floats each, as laid out in
    /// `render_entities`) and draw it from `sheet`. Expects the sprite program
    /// and VAO to be bound already.
    fn draw_sprite_batch(&self, sheet: SpriteSheet, instance_data: &[f32], tileset: &MultiTileset) {
        if instance_data.is_empty() {
            return;
        }
        unsafe {
            tileset.bind(&self.gl, sheet, 0);
            self.gl.bind_buffer(ARRAY_BUFFER, Some(self.instance_vbo));
            self.gl.buffer_data_u8_slice(ARRAY_BUFFER, as_u8_slice(instance_data), DYNAMIC_DRAW);
            let instance_count = instance_data.len() / 11;
            self.gl.draw_arrays_instanced(TRIANGLES, 0, 6, instance_count as i32);
        }
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
                    crate::vfx::VfxType::HealNumber { .. } => continue, // Rendered via egui
                    crate::vfx::VfxType::MissText { .. } => continue, // Rendered via egui
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
            std::mem::size_of_val(data),
        )
    }
}
