/// Tile IDs matching the tileset (minirogue-all.png)
/// These can be adjusted to match different tilesets
pub mod tile_ids {
    pub const EMPTY: u32 = 0;
    pub const FLOOR: u32 = 1;      // Basic floor tile
    pub const WALL: u32 = 3;       // Wall tile
    pub const WATER: u32 = 4;      // Water tile
    pub const GRASS: u32 = 5;      // Grass tile
    pub const STONE: u32 = 2;      // Stone floor

    // Entity tiles
    pub const PLAYER: u32 = 87;    // Human character
    pub const CHEST_CLOSED: u32 = 32;  // Closed chest
    pub const CHEST_OPEN: u32 = 33;    // Open chest (if available)
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum TileType {
    Empty,
    Floor,
    Wall,
    Water,
    Grass,
    Stone,
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
        }
    }

    pub fn is_walkable(&self) -> bool {
        matches!(self, TileType::Floor | TileType::Grass)
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
