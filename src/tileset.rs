use glow::HasContext;
use serde::Deserialize;
use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;

/// UV coordinates for a tile (normalized 0-1)
#[derive(Clone, Copy, Debug)]
pub struct TileUV {
    pub u0: f32,
    pub v0: f32,
    pub u1: f32,
    pub v1: f32,
}

/// Tile metadata from the .tsj file
#[derive(Deserialize)]
struct TsjTile {
    id: u32,
    #[serde(rename = "type")]
    tile_type: Option<String>,
}

/// Raw .tsj file format
#[derive(Deserialize)]
struct TsjFile {
    columns: u32,
    image: String,
    imagewidth: u32,
    imageheight: u32,
    tilewidth: u32,
    tileheight: u32,
    tilecount: u32,
    #[serde(default)]
    tiles: Vec<TsjTile>,
}

pub struct Tileset {
    pub texture: glow::Texture,
    pub tile_width: u32,
    pub tile_height: u32,
    pub columns: u32,
    #[allow(dead_code)] // Reserved for future tileset expansion
    pub rows: u32,
    #[allow(dead_code)] // Reserved for future tileset expansion
    pub tile_count: u32,
    image_width: u32,
    image_height: u32,
    /// Map from tile type name to tile ID
    #[allow(dead_code)] // Reserved for type-based tile lookup
    type_to_id: HashMap<String, u32>,
}

impl Tileset {
    /// Load a tileset from a .tsj file and its associated PNG
    pub fn load(gl: Arc<glow::Context>, tsj_path: &Path) -> Result<Self, String> {
        // Parse the JSON
        let json_str = std::fs::read_to_string(tsj_path)
            .map_err(|e| format!("Failed to read {}: {}", tsj_path.display(), e))?;
        let tsj: TsjFile = serde_json::from_str(&json_str)
            .map_err(|e| format!("Failed to parse {}: {}", tsj_path.display(), e))?;

        // Load the PNG (relative to the .tsj file)
        let png_path = tsj_path.parent().unwrap_or(Path::new(".")).join(&tsj.image);
        let mut img = image::open(&png_path)
            .map_err(|e| format!("Failed to load {}: {}", png_path.display(), e))?
            .into_rgba8();

        // Convert to premultiplied alpha (required by egui)
        for pixel in img.pixels_mut() {
            let a = pixel[3] as f32 / 255.0;
            pixel[0] = (pixel[0] as f32 * a) as u8;
            pixel[1] = (pixel[1] as f32 * a) as u8;
            pixel[2] = (pixel[2] as f32 * a) as u8;
        }

        let (width, height) = img.dimensions();

        // Create OpenGL texture
        let texture = unsafe {
            let tex = gl.create_texture().map_err(|e| format!("Failed to create texture: {}", e))?;
            gl.bind_texture(glow::TEXTURE_2D, Some(tex));

            // Use NEAREST for crisp pixel art
            gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_MIN_FILTER, glow::NEAREST as i32);
            gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_MAG_FILTER, glow::NEAREST as i32);
            gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_WRAP_S, glow::CLAMP_TO_EDGE as i32);
            gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_WRAP_T, glow::CLAMP_TO_EDGE as i32);

            gl.tex_image_2d(
                glow::TEXTURE_2D,
                0,
                glow::RGBA as i32,
                width as i32,
                height as i32,
                0,
                glow::RGBA,
                glow::UNSIGNED_BYTE,
                Some(img.as_raw()),
            );

            gl.bind_texture(glow::TEXTURE_2D, None);
            tex
        };

        // Build type -> ID mapping
        let mut type_to_id = HashMap::new();
        for tile in &tsj.tiles {
            if let Some(ref tile_type) = tile.tile_type {
                type_to_id.insert(tile_type.clone(), tile.id);
            }
        }

        let rows = tsj.imageheight / tsj.tileheight;

        Ok(Self {
            texture,
            tile_width: tsj.tilewidth,
            tile_height: tsj.tileheight,
            columns: tsj.columns,
            rows,
            tile_count: tsj.tilecount,
            image_width: tsj.imagewidth,
            image_height: tsj.imageheight,
            type_to_id,
        })
    }

    /// Get UV coordinates for a tile by ID
    pub fn get_uv(&self, tile_id: u32) -> TileUV {
        let col = tile_id % self.columns;
        let row = tile_id / self.columns;

        // Tiny inset to prevent texture bleeding at tile edges
        // Using 0.1 pixel instead of 0.5 to minimize visual stretching
        let inset_u = 0.1 / self.image_width as f32;
        let inset_v = 0.1 / self.image_height as f32;

        let u0 = (col * self.tile_width) as f32 / self.image_width as f32 + inset_u;
        let u1 = ((col + 1) * self.tile_width) as f32 / self.image_width as f32 - inset_u;

        // Flip V coordinates (OpenGL has origin at bottom-left, PNG at top-left)
        let v0 = ((row + 1) * self.tile_height) as f32 / self.image_height as f32 - inset_v;
        let v1 = (row * self.tile_height) as f32 / self.image_height as f32 + inset_v;

        TileUV { u0, v0, u1, v1 }
    }

    /// Get UV coordinates for a tile by type name (e.g., "wall", "chest_gold_closed")
    #[allow(dead_code)] // Reserved for type-based tile lookup
    pub fn get_uv_by_type(&self, tile_type: &str) -> Option<TileUV> {
        self.type_to_id.get(tile_type).map(|&id| self.get_uv(id))
    }

    /// Get tile ID by type name
    #[allow(dead_code)] // Reserved for type-based tile lookup
    pub fn get_id(&self, tile_type: &str) -> Option<u32> {
        self.type_to_id.get(tile_type).copied()
    }

    /// Bind this tileset's texture to a texture unit
    pub fn bind(&self, gl: &glow::Context, unit: u32) {
        unsafe {
            gl.active_texture(glow::TEXTURE0 + unit);
            gl.bind_texture(glow::TEXTURE_2D, Some(self.texture));
        }
    }

    /// Get the egui TextureId for this tileset (requires prior registration)
    #[allow(dead_code)] // Reserved for egui tile rendering
    pub fn egui_texture_id(&self) -> egui::TextureId {
        egui::TextureId::User(self.texture.0.get() as u64)
    }

    /// Get UV rect for egui (note: egui uses top-left origin, OpenGL uses bottom-left)
    pub fn get_egui_uv(&self, tile_id: u32) -> egui::Rect {
        let col = tile_id % self.columns;
        let row = tile_id / self.columns;

        let u0 = (col * self.tile_width) as f32 / self.image_width as f32;
        let u1 = ((col + 1) * self.tile_width) as f32 / self.image_width as f32;

        // egui uses top-left origin like PNG, no flip needed
        let v0 = (row * self.tile_height) as f32 / self.image_height as f32;
        let v1 = ((row + 1) * self.tile_height) as f32 / self.image_height as f32;

        egui::Rect::from_min_max(egui::pos2(u0, v0), egui::pos2(u1, v1))
    }
}
