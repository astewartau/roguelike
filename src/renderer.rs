use crate::camera::Camera;
use crate::grid::Grid;
use glow::*;
use std::mem;

const VERTEX_SHADER_SRC: &str = r#"#version 330 core
layout (location = 0) in vec2 aPos;
layout (location = 1) in vec2 aInstancePos;
layout (location = 2) in vec3 aInstanceColor;

uniform mat4 uProjection;

out vec3 vColor;

void main() {
    vec2 worldPos = aInstancePos + aPos;
    gl_Position = uProjection * vec4(worldPos, 0.0, 1.0);
    vColor = aInstanceColor;
}
"#;

const FRAGMENT_SHADER_SRC: &str = r#"#version 330 core
in vec3 vColor;
out vec4 FragColor;

void main() {
    FragColor = vec4(vColor, 1.0);
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
    // Bright blue grid lines for techy look
    FragColor = vec4(0.2, 0.4, 0.8, 0.3);
}
"#;

pub struct Renderer {
    gl: glow::Context,
    program: NativeProgram,
    vao: NativeVertexArray,
    vbo: NativeBuffer,
    instance_vbo: NativeBuffer,
    projection_loc: NativeUniformLocation,
    max_instances: usize,
    // Grid line rendering
    grid_program: NativeProgram,
    grid_vao: NativeVertexArray,
    grid_vbo: NativeBuffer,
    grid_projection_loc: NativeUniformLocation,
}

impl Renderer {
    pub fn new(gl: glow::Context) -> Result<Self, String> {
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
            let instance_vbo = gl
                .create_buffer()
                .map_err(|e| format!("Failed to create instance VBO: {}", e))?;
            gl.bind_buffer(ARRAY_BUFFER, Some(instance_vbo));

            // Position attribute (2 floats)
            gl.enable_vertex_attrib_array(1);
            gl.vertex_attrib_pointer_f32(1, 2, FLOAT, false, 20, 0);
            gl.vertex_attrib_divisor(1, 1);

            // Color attribute (3 floats)
            gl.enable_vertex_attrib_array(2);
            gl.vertex_attrib_pointer_f32(2, 3, FLOAT, false, 20, 8);
            gl.vertex_attrib_divisor(2, 1);

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

            // Navy blue background (techy look)
            gl.clear_color(0.05, 0.08, 0.15, 1.0);

            // Enable blending for semi-transparent grid lines
            gl.enable(BLEND);
            gl.blend_func(SRC_ALPHA, ONE_MINUS_SRC_ALPHA);

            Ok(Self {
                gl,
                program,
                vao,
                vbo,
                instance_vbo,
                projection_loc,
                max_instances: 100000,
                grid_program,
                grid_vao,
                grid_vbo,
                grid_projection_loc,
            })
        }
    }

    pub fn resize(&self, width: i32, height: i32) {
        unsafe {
            self.gl.viewport(0, 0, width, height);
        }
    }

    pub fn render(&mut self, camera: &Camera, grid: &Grid) -> Result<(), String> {
        unsafe {
            self.gl.clear(COLOR_BUFFER_BIT);

            self.gl.use_program(Some(self.program));
            self.gl.bind_vertex_array(Some(self.vao));

            // Get visible bounds
            let (min_x, max_x, min_y, max_y) = camera.get_visible_bounds();

            // Build instance data for visible tiles
            let mut instance_data = Vec::new();

            for y in min_y..=max_y {
                for x in min_x..=max_x {
                    if let Some(tile) = grid.get(x, y) {
                        let color = tile.tile_type.color();

                        // Position (2 floats) + Color (3 floats) = 5 floats per instance
                        instance_data.push(x as f32);
                        instance_data.push(y as f32);
                        instance_data.push(color.x);
                        instance_data.push(color.y);
                        instance_data.push(color.z);
                    }
                }
            }

            if !instance_data.is_empty() {
                // Upload instance data
                self.gl.bind_buffer(ARRAY_BUFFER, Some(self.instance_vbo));
                self.gl.buffer_data_u8_slice(
                    ARRAY_BUFFER,
                    as_u8_slice(&instance_data),
                    DYNAMIC_DRAW,
                );

                // Set projection matrix
                let projection = camera.projection_matrix();
                self.gl.uniform_matrix_4_f32_slice(
                    Some(&self.projection_loc),
                    false,
                    projection.as_ref(),
                );

                // Draw instances
                let instance_count = instance_data.len() / 5;
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

            // Set projection matrix
            let projection = camera.projection_matrix();
            self.gl.uniform_matrix_4_f32_slice(
                Some(&self.grid_projection_loc),
                false,
                projection.as_ref(),
            );

            // Build line vertices for visible grid
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
                // Upload line data
                self.gl.bind_buffer(ARRAY_BUFFER, Some(self.grid_vbo));
                self.gl.buffer_data_u8_slice(
                    ARRAY_BUFFER,
                    as_u8_slice(&line_vertices),
                    DYNAMIC_DRAW,
                );

                // Draw lines
                self.gl.draw_arrays(LINES, 0, (line_vertices.len() / 2) as i32);
            }

            self.gl.bind_vertex_array(None);
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
