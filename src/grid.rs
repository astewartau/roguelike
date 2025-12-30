use crate::dungeon_gen::{DungeonGenerator, Rect, RoomTheme, ThemedRoom};
use crate::tile::{SpriteSheet, Tile};

/// A decorative decal placed on a tile
#[derive(Debug, Clone, Copy)]
pub struct Decal {
    pub x: i32,
    pub y: i32,
    pub sheet: SpriteSheet,
    pub tile_id: u32,
}

pub struct Grid {
    pub width: usize,
    pub height: usize,
    pub tiles: Vec<Tile>,
    pub chest_positions: Vec<(i32, i32)>,
    pub door_positions: Vec<((i32, i32), RoomTheme)>,
    pub brazier_positions: Vec<(i32, i32)>,
    pub decals: Vec<Decal>,
    pub stairs_up_pos: Option<(i32, i32)>,
    pub stairs_down_pos: Option<(i32, i32)>,
    /// The starting room where the player spawns (for NPC placement and enemy exclusion)
    pub starting_room: Option<Rect>,
    /// Per-tile illumination values (computed each frame for visible tiles)
    pub illumination: Vec<f32>,
    /// Themed rooms for wall/door styling
    pub themed_rooms: Vec<ThemedRoom>,
    /// Water tile positions for animated water entities
    pub water_positions: Vec<(i32, i32)>,
    /// Coffin positions in Crypt rooms
    pub coffin_positions: Vec<(i32, i32)>,
    /// Barrel positions in Storage rooms
    pub barrel_positions: Vec<(i32, i32)>,
    /// Shop vendor spawn position
    pub shop_position: Option<(i32, i32)>,
    /// Shop decoration positions (jars, sacks, etc.)
    pub shop_decor_positions: Vec<(i32, i32)>,
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
            brazier_positions: result.brazier_positions,
            decals: result.decals,
            stairs_up_pos: result.stairs_up_pos,
            stairs_down_pos: result.stairs_down_pos,
            starting_room: result.starting_room,
            illumination: vec![0.0; width * height],
            themed_rooms: result.themed_rooms,
            water_positions: result.water_positions,
            coffin_positions: result.coffin_positions,
            barrel_positions: result.barrel_positions,
            shop_position: result.shop_position,
            shop_decor_positions: result.shop_decor_positions,
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

    /// Check if a tile is walkable (exists and has a walkable tile type)
    pub fn is_walkable(&self, x: i32, y: i32) -> bool {
        self.get(x, y).map(|t| t.tile_type.is_walkable()).unwrap_or(false)
    }
}
