//! Multi-texture tileset loader for 32rogues sprite sheets.

use crate::tile::SpriteSheet;
use crate::tileset::TileUV;
use glow::HasContext;
use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;

/// Data for a single sprite sheet
struct SheetData {
    texture: glow::Texture,
    columns: u32,
    rows: u32,
    image_width: u32,
    image_height: u32,
}

/// Multi-texture tileset manager for 32rogues
/// Manages 4 separate sprite sheets with unified UV lookup
pub struct MultiTileset {
    sheets: HashMap<SpriteSheet, SheetData>,
    tile_size: u32,
}

/// Sheet specifications for 32rogues (all 32x32 pixel tiles)
const TILE_SIZE: u32 = 32;

struct SheetSpec {
    sheet: SpriteSheet,
    filename: &'static str,
    columns: u32,
}

const SHEET_SPECS: &[SheetSpec] = &[
    SheetSpec {
        sheet: SpriteSheet::Tiles,
        filename: "tiles.png",
        columns: 17,
    },
    SheetSpec {
        sheet: SpriteSheet::Rogues,
        filename: "rogues.png",
        columns: 7,
    },
    SheetSpec {
        sheet: SpriteSheet::Monsters,
        filename: "monsters.png",
        columns: 12,
    },
    SheetSpec {
        sheet: SpriteSheet::Items,
        filename: "items.png",
        columns: 11,
    },
    SheetSpec {
        sheet: SpriteSheet::AnimatedTiles,
        filename: "animated-tiles.png",
        columns: 11,
    },
];

impl MultiTileset {
    /// Load all sprite sheets from the 32rogues directory
    pub fn load(gl: Arc<glow::Context>, base_path: &Path) -> Result<Self, String> {
        let mut sheets = HashMap::new();

        for spec in SHEET_SPECS {
            let png_path = base_path.join(spec.filename);
            let sheet_data = Self::load_sheet(&gl, &png_path, spec.columns)?;
            sheets.insert(spec.sheet, sheet_data);
        }

        Ok(Self {
            sheets,
            tile_size: TILE_SIZE,
        })
    }

    /// Load a single sprite sheet
    fn load_sheet(gl: &glow::Context, png_path: &Path, columns: u32) -> Result<SheetData, String> {
        // Load the PNG
        let mut img = image::open(png_path)
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
        let rows = height / TILE_SIZE;

        // Create OpenGL texture
        let texture = unsafe {
            let tex = gl
                .create_texture()
                .map_err(|e| format!("Failed to create texture: {}", e))?;
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

        Ok(SheetData {
            texture,
            columns,
            rows,
            image_width: width,
            image_height: height,
        })
    }

    /// Get UV coordinates for a tile by sheet and ID
    pub fn get_uv(&self, sheet: SpriteSheet, tile_id: u32) -> TileUV {
        let sheet_data = self
            .sheets
            .get(&sheet)
            .expect("Sheet not loaded");

        let col = tile_id % sheet_data.columns;
        let row = tile_id / sheet_data.columns;

        // Tiny inset to prevent texture bleeding at tile edges
        let inset_u = 0.1 / sheet_data.image_width as f32;
        let inset_v = 0.1 / sheet_data.image_height as f32;

        let u0 = (col * self.tile_size) as f32 / sheet_data.image_width as f32 + inset_u;
        let u1 = ((col + 1) * self.tile_size) as f32 / sheet_data.image_width as f32 - inset_u;

        // Flip V coordinates (OpenGL has origin at bottom-left, PNG at top-left)
        let v0 = ((row + 1) * self.tile_size) as f32 / sheet_data.image_height as f32 - inset_v;
        let v1 = (row * self.tile_size) as f32 / sheet_data.image_height as f32 + inset_v;

        TileUV { u0, v0, u1, v1 }
    }

    /// Bind a specific sheet's texture to a texture unit
    pub fn bind(&self, gl: &glow::Context, sheet: SpriteSheet, unit: u32) {
        let sheet_data = self
            .sheets
            .get(&sheet)
            .expect("Sheet not loaded");

        unsafe {
            gl.active_texture(glow::TEXTURE0 + unit);
            gl.bind_texture(glow::TEXTURE_2D, Some(sheet_data.texture));
        }
    }

    /// Get the tile size (32 for 32rogues)
    pub fn tile_size(&self) -> u32 {
        self.tile_size
    }

    /// Get UV rect for egui (note: egui uses top-left origin, OpenGL uses bottom-left)
    pub fn get_egui_uv(&self, sheet: SpriteSheet, tile_id: u32) -> egui::Rect {
        let sheet_data = self
            .sheets
            .get(&sheet)
            .expect("Sheet not loaded");

        let col = tile_id % sheet_data.columns;
        let row = tile_id / sheet_data.columns;

        let u0 = (col * self.tile_size) as f32 / sheet_data.image_width as f32;
        let u1 = ((col + 1) * self.tile_size) as f32 / sheet_data.image_width as f32;

        // egui uses top-left origin like PNG, no flip needed
        let v0 = (row * self.tile_size) as f32 / sheet_data.image_height as f32;
        let v1 = ((row + 1) * self.tile_size) as f32 / sheet_data.image_height as f32;

        egui::Rect::from_min_max(egui::pos2(u0, v0), egui::pos2(u1, v1))
    }

    /// Get the raw glow texture for a specific sheet (for registration with egui_glow)
    pub fn get_native_texture(&self, sheet: SpriteSheet) -> glow::Texture {
        self.sheets
            .get(&sheet)
            .expect("Sheet not loaded")
            .texture
    }

    /// Get column count for a sheet (needed for some calculations)
    pub fn columns(&self, sheet: SpriteSheet) -> u32 {
        self.sheets
            .get(&sheet)
            .expect("Sheet not loaded")
            .columns
    }

    /// Get row count for a sheet
    #[allow(dead_code)]
    pub fn rows(&self, sheet: SpriteSheet) -> u32 {
        self.sheets
            .get(&sheet)
            .expect("Sheet not loaded")
            .rows
    }
}
