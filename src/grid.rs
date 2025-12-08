use crate::dungeon_gen::DungeonGenerator;
use crate::tile::Tile;

/// A decorative decal placed on a tile
#[derive(Debug, Clone, Copy)]
pub struct Decal {
    pub x: i32,
    pub y: i32,
    pub tile_id: u32,
}

pub struct Grid {
    pub width: usize,
    pub height: usize,
    pub tiles: Vec<Tile>,
    pub chest_positions: Vec<(i32, i32)>,
    pub door_positions: Vec<(i32, i32)>,
    pub decals: Vec<Decal>,
}

impl Grid {
    pub fn new(width: usize, height: usize) -> Self {
        // Generate dungeon using BSP
        let (tiles, chest_positions, door_positions, decals) = DungeonGenerator::generate(width, height);

        Self {
            width,
            height,
            tiles,
            chest_positions,
            door_positions,
            decals,
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
