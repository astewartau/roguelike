use crate::tile::{Tile, TileType};
use noise::{NoiseFn, Perlin};

pub struct Grid {
    pub width: usize,
    pub height: usize,
    pub tiles: Vec<Tile>,
}

impl Grid {
    pub fn new(width: usize, height: usize) -> Self {
        let mut tiles = vec![Tile::default(); width * height];

        // Generate procedural terrain using Perlin noise
        let perlin = Perlin::new(42);

        for y in 0..height {
            for x in 0..width {
                let nx = x as f64 / 50.0;
                let ny = y as f64 / 50.0;

                let value = perlin.get([nx, ny]);

                let tile_type = if value < -0.3 {
                    TileType::Water
                } else if value < 0.0 {
                    TileType::Grass
                } else if value < 0.4 {
                    TileType::Floor
                } else if value < 0.6 {
                    TileType::Stone
                } else {
                    TileType::Wall
                };

                tiles[y * width + x] = Tile::new(tile_type);
            }
        }

        Self {
            width,
            height,
            tiles,
        }
    }

    pub fn get(&self, x: i32, y: i32) -> Option<&Tile> {
        if x < 0 || y < 0 || x >= self.width as i32 || y >= self.height as i32 {
            return None;
        }
        Some(&self.tiles[y as usize * self.width + x as usize])
    }

    pub fn get_mut(&mut self, x: i32, y: i32) -> Option<&mut Tile> {
        if x < 0 || y < 0 || x >= self.width as i32 || y >= self.height as i32 {
            return None;
        }
        Some(&mut self.tiles[y as usize * self.width + x as usize])
    }
}
