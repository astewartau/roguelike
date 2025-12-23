use crate::dungeon_gen::{DungeonGenerator, Rect};
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
    pub stairs_up_pos: Option<(i32, i32)>,
    pub stairs_down_pos: Option<(i32, i32)>,
    /// The starting room where the player spawns (for NPC placement and enemy exclusion)
    pub starting_room: Option<Rect>,
}

impl Grid {
    pub fn new(width: usize, height: usize) -> Self {
        Self::new_floor(width, height, 0)
    }

    /// Generate a dungeon floor. floor_num 0 is the first floor (no stairs up).
    pub fn new_floor(width: usize, height: usize, floor_num: u32) -> Self {
        // Generate dungeon using BSP
        let result = DungeonGenerator::generate(width, height, floor_num);

        Self {
            width,
            height,
            tiles: result.tiles,
            chest_positions: result.chest_positions,
            door_positions: result.door_positions,
            decals: result.decals,
            stairs_up_pos: result.stairs_up_pos,
            stairs_down_pos: result.stairs_down_pos,
            starting_room: result.starting_room,
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
