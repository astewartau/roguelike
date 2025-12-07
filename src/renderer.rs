use crate::camera::Camera;
use crate::components::Sprite;
use crate::grid::Grid;
use crate::tileset::Tileset;
use glow::*;
use std::mem;
use std::sync::Arc;

const VERTEX_SHADER_SRC: &str = r#"#version 330 core
layout (location = 0) in vec2 aPos;
layout (location = 1) in vec2 aInstancePos;
layout (location = 2) in vec4 aInstanceUV;  // u0, v0, u1, v1
layout (location = 3) in float aFogMult;
layout (location = 4) in float aBorder;

uniform mat4 uProjection;

out vec2 vTexCoord;
out vec2 vLocalPos;
out float vFog;
out float vBorder;

void main() {
    vec2 worldPos = aInstancePos + aPos;
    gl_Position = uProjection * vec4(worldPos, 0.0, 1.0);

    // Interpolate UV based on vertex position (0-1)
    vTexCoord = mix(aInstanceUV.xy, aInstanceUV.zw, aPos);
    vLocalPos = aPos;
    vFog = aFogMult;
    vBorder = aBorder;
}
"#;

const FRAGMENT_SHADER_SRC: &str = r#"#version 330 core
in vec2 vTexCoord;
in vec2 vLocalPos;
in float vFog;
in float vBorder;  // 0.0 = normal, 1.0 = red border, 2.0 = hit flash

uniform sampler2D uTileset;

out vec4 FragColor;

void main() {
    vec4 texColor = texture(uTileset, vTexCoord);
    if (texColor.a < 0.1) discard;  // Discard transparent pixels

    vec3 color = texColor.rgb * vFog;

    // Hit flash effect (white overlay)
    if (vBorder > 1.5) {
        color = mix(color, vec3(1.0, 1.0, 1.0), 0.7);  // White flash
    }
    // Draw red border if flagged
    else if (vBorder > 0.5) {
        float borderWidth = 0.08;
        bool onBorder = vLocalPos.x < borderWidth || vLocalPos.x > (1.0 - borderWidth) ||
                        vLocalPos.y < borderWidth || vLocalPos.y > (1.0 - borderWidth);
        if (onBorder) {
            color = vec3(0.9, 0.2, 0.2);  // Red border
        }
    }

    FragColor = vec4(color, texColor.a);
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
            // Layout: pos(2) + uv(4) + fog(1) + border(1) = 8 floats = 32 bytes per instance
            let instance_vbo = gl
                .create_buffer()
                .map_err(|e| format!("Failed to create instance VBO: {}", e))?;
            gl.bind_buffer(ARRAY_BUFFER, Some(instance_vbo));

            let stride = 32; // 8 floats * 4 bytes

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

            // Border attribute (1 float)
            gl.enable_vertex_attrib_array(4);
            gl.vertex_attrib_pointer_f32(4, 1, FLOAT, false, stride, 28);
            gl.vertex_attrib_divisor(4, 1);

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
            })
        }
    }

    pub fn render(&mut self, camera: &Camera, grid: &Grid, tileset: &Tileset) -> Result<(), String> {
        unsafe {
            self.gl.clear(COLOR_BUFFER_BIT);

            self.gl.use_program(Some(self.program));
            self.gl.bind_vertex_array(Some(self.vao));

            // Bind tileset texture
            tileset.bind(&self.gl, 0);
            self.gl.uniform_1_i32(Some(&self.tileset_loc), 0);

            // Get visible bounds
            let (min_x, max_x, min_y, max_y) = camera.get_visible_bounds();

            // Build instance data for visible tiles
            // Layout: pos(2) + uv(4) + fog(1) + border(1) = 8 floats per instance
            let mut instance_data = Vec::new();

            for y in min_y..=max_y {
                for x in min_x..=max_x {
                    if let Some(tile) = grid.get(x, y) {
                        // Only render explored tiles
                        if !tile.explored {
                            continue;
                        }

                        let uv = tileset.get_uv(tile.tile_type.tile_id());
                        let fog = if tile.visible { 1.0 } else { 0.5 };

                        instance_data.push(x as f32);
                        instance_data.push(y as f32);
                        instance_data.push(uv.u0);
                        instance_data.push(uv.v0);
                        instance_data.push(uv.u1);
                        instance_data.push(uv.v1);
                        instance_data.push(fog);
                        instance_data.push(0.0);  // no border for tiles
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

                let instance_count = instance_data.len() / 8;
                self.gl.draw_arrays_instanced(TRIANGLES, 0, 6, instance_count as i32);
            }

            self.gl.bind_vertex_array(None);

            // Render grid lines on top
            self.render_grid_lines(camera, min_x, max_x, min_y, max_y);
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

    /// Render entities. Tuple: (x, y, sprite, fog, has_border, has_hit_flash)
    pub fn render_entities(&mut self, camera: &Camera, entities: &[(f32, f32, Sprite, f32, bool, bool)], tileset: &Tileset) -> Result<(), String> {
        if entities.is_empty() {
            return Ok(());
        }

        unsafe {
            self.gl.use_program(Some(self.program));
            self.gl.bind_vertex_array(Some(self.vao));

            // Bind tileset texture
            tileset.bind(&self.gl, 0);
            self.gl.uniform_1_i32(Some(&self.tileset_loc), 0);

            // Build instance data for entities
            let mut instance_data = Vec::new();

            for (x, y, sprite, fog, has_border, has_hit_flash) in entities {
                let uv = tileset.get_uv(sprite.tile_id);

                instance_data.push(*x);
                instance_data.push(*y);
                instance_data.push(uv.u0);
                instance_data.push(uv.v0);
                instance_data.push(uv.u1);
                instance_data.push(uv.v1);
                instance_data.push(*fog);
                // Encode border (1.0) and hit flash (2.0) in the same field
                let effect = if *has_hit_flash { 2.0 } else if *has_border { 1.0 } else { 0.0 };
                instance_data.push(effect);
            }

            self.gl.bind_buffer(ARRAY_BUFFER, Some(self.instance_vbo));
            self.gl.buffer_data_u8_slice(ARRAY_BUFFER, as_u8_slice(&instance_data), DYNAMIC_DRAW);

            let projection = camera.projection_matrix();
            self.gl.uniform_matrix_4_f32_slice(Some(&self.projection_loc), false, projection.as_ref());

            self.gl.draw_arrays_instanced(TRIANGLES, 0, 6, entities.len() as i32);

            self.gl.bind_vertex_array(None);
        }

        Ok(())
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
