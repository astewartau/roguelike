/// Tile IDs matching the tileset (minirogue-all.png)
/// These can be adjusted to match different tilesets
pub mod tile_ids {
    pub const EMPTY: u32 = 0;
    pub const FLOOR: u32 = 2;      // Stone floor tile
    pub const WALL: u32 = 3;       // Wall tile
    pub const WATER: u32 = 4;      // Water tile
    pub const GRASS: u32 = 5;      // Grass tile
    pub const STONE: u32 = 2;      // Stone floor
    pub const DOOR: u32 = 27;      // Door tile

    // Entity tiles
    pub const PLAYER: u32 = 85;    // character_1
    pub const SKELETON: u32 = 105; // Skeleton enemy
    pub const RAT: u32 = 111;      // Rat enemy
    pub const CHEST_CLOSED: u32 = 32;  // Closed chest
    pub const CHEST_OPEN: u32 = 33;    // Open chest (if available)
    pub const BONES: u32 = 22;     // bones_1 - remains of defeated enemies

    // Item tiles
    pub const SWORD: u32 = 65;
    pub const BOW: u32 = 69;
    pub const ARROW: u32 = 70;
    pub const RED_POTION: u32 = 52;
    pub const SCROLL: u32 = 58;  // Book tile for scrolls
    pub const COINS: u32 = 36;

    // UI icons
    pub const HEART: u32 = 151;
    pub const DIAMOND: u32 = 38;

    // Decorative decals
    pub const BONES_1: u32 = 22;
    pub const BONES_2: u32 = 23;
    pub const BONES_3: u32 = 24;
    pub const BONES_4: u32 = 25;
    pub const ROCKS: u32 = 26;
    pub const PLANT: u32 = 13;
    pub const MUSHROOM: u32 = 48;
    pub const FLOWERS: u32 = 51;
    pub const SKULL: u32 = 148;

    // Stairs
    pub const STAIRS_DOWN: u32 = 4;  // Stairs going down
    pub const STAIRS_UP: u32 = 5;    // Stairs going up
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum TileType {
    Empty,
    Floor,
    Wall,
    Water,
    Grass,
    Stone,
    StairsDown,
    StairsUp,
}

impl TileType {
    /// Get the tile ID for this tile type (maps to tileset)
    pub fn tile_id(&self) -> u32 {
        match self {
            TileType::Empty => tile_ids::EMPTY,
            TileType::Floor => tile_ids::FLOOR,
            TileType::Wall => tile_ids::WALL,
            TileType::Water => tile_ids::WATER,
            TileType::Grass => tile_ids::GRASS,
            TileType::Stone => tile_ids::STONE,
            TileType::StairsDown => tile_ids::STAIRS_DOWN,
            TileType::StairsUp => tile_ids::STAIRS_UP,
        }
    }

    pub fn is_walkable(&self) -> bool {
        matches!(self, TileType::Floor | TileType::Grass | TileType::StairsDown | TileType::StairsUp)
    }

    pub fn blocks_vision(&self) -> bool {
        matches!(self, TileType::Wall | TileType::Empty)
    }
}

#[derive(Debug, Clone)]
pub struct Tile {
    pub tile_type: TileType,
    pub explored: bool,
    pub visible: bool,
}

impl Tile {
    pub fn new(tile_type: TileType) -> Self {
        Self {
            tile_type,
            explored: false,
            visible: false,
        }
    }
}

impl Default for Tile {
    fn default() -> Self {
        Self::new(TileType::Empty)
    }
}
