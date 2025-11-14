use glam::Vec3;

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
    pub fn color(&self) -> Vec3 {
        match self {
            TileType::Empty => Vec3::new(0.0, 0.0, 0.0),
            TileType::Floor => Vec3::new(0.4, 0.3, 0.2),
            TileType::Wall => Vec3::new(0.3, 0.3, 0.3),
            TileType::Water => Vec3::new(0.2, 0.4, 0.8),
            TileType::Grass => Vec3::new(0.2, 0.6, 0.2),
            TileType::Stone => Vec3::new(0.5, 0.5, 0.5),
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
