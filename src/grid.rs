use crate::dungeon_gen::DungeonGenerator;
use crate::tile::Tile;

pub struct Grid {
    pub width: usize,
    pub height: usize,
    pub tiles: Vec<Tile>,
    pub chest_positions: Vec<(i32, i32)>,
}

impl Grid {
    pub fn new(width: usize, height: usize) -> Self {
        // Generate mine/cave dungeon using cellular automata + constructed rooms
        let (tiles, chest_positions) = DungeonGenerator::generate(width, height);

        Self {
            width,
            height,
            tiles,
            chest_positions,
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
