/// Sprite sheet identifiers for the 32rogues tileset
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SpriteSheet {
    Tiles,         // tiles.png - terrain, doors, stairs, decals
    Rogues,        // rogues.png - player characters, NPCs
    Monsters,      // monsters.png - enemies
    Items,         // items.png - weapons, armor, potions, etc.
    AnimatedTiles, // animated-tiles.png - fire pits, torches, etc.
}

/// Helper to convert row.letter notation to tile ID
/// Row is 1-indexed, letter is 0-indexed (a=0, b=1, etc.)
const fn rc(row: u32, col: u32, columns: u32) -> u32 {
    (row - 1) * columns + col
}

/// Tile IDs for the 32rogues tileset
/// Format: (SpriteSheet, tile_id)
///
/// Sheet dimensions (32x32 tiles):
/// - Tiles: 17 columns (tiles.png 544x832)
/// - Rogues: 7 columns (rogues.png 224x224)
/// - Monsters: 12 columns (monsters.png 384x416)
/// - Items: 11 columns (items.png 352x832)
pub mod tile_ids {
    use super::{rc, SpriteSheet};

    // Column counts for each sheet
    pub const TILES_COLS: u32 = 17;
    pub const ROGUES_COLS: u32 = 7;
    pub const MONSTERS_COLS: u32 = 12;
    pub const ITEMS_COLS: u32 = 11;
    pub const ANIMATED_TILES_COLS: u32 = 11;

    // ===== TILES SHEET (terrain, structures, decals) =====

    // Terrain
    pub const EMPTY: (SpriteSheet, u32) = (SpriteSheet::Tiles, 0);
    // Stone floor variants (row 10)
    pub const FLOOR: (SpriteSheet, u32) = (SpriteSheet::Tiles, rc(10, 1, TILES_COLS)); // 10.b stone floor 1
    pub const FLOOR_2: (SpriteSheet, u32) = (SpriteSheet::Tiles, rc(10, 2, TILES_COLS)); // 10.c stone floor 2
    pub const FLOOR_3: (SpriteSheet, u32) = (SpriteSheet::Tiles, rc(10, 3, TILES_COLS)); // 10.d stone floor 3
    pub const FLOOR_VARIANTS: [(SpriteSheet, u32); 3] = [FLOOR, FLOOR_2, FLOOR_3];
    pub const WALL: (SpriteSheet, u32) = (SpriteSheet::Tiles, rc(3, 1, TILES_COLS)); // 3.b stone brick wall (side)
    pub const WALL_TOP: (SpriteSheet, u32) = (SpriteSheet::Tiles, rc(3, 0, TILES_COLS)); // 3.a stone brick wall (top)

    // Rough stone walls for Overgrown rooms
    pub const WALL_ROUGH: (SpriteSheet, u32) = (SpriteSheet::Tiles, rc(2, 1, TILES_COLS)); // 2.b rough stone wall (side)
    pub const WALL_ROUGH_TOP: (SpriteSheet, u32) = (SpriteSheet::Tiles, rc(2, 0, TILES_COLS)); // 2.a rough stone wall (top)

    // Catacombs/skull walls for Crypt rooms
    pub const WALL_CRYPT: (SpriteSheet, u32) = (SpriteSheet::Tiles, rc(6, 1, TILES_COLS)); // 6.b skull wall (side)
    pub const WALL_CRYPT_TOP: (SpriteSheet, u32) = (SpriteSheet::Tiles, rc(6, 0, TILES_COLS)); // 6.a skull wall (top)
    pub const WATER: (SpriteSheet, u32) = (SpriteSheet::Tiles, rc(7, 1, TILES_COLS)); // Use floor, tinted blue
    // Grass floor variants (row 8)
    pub const GRASS: (SpriteSheet, u32) = (SpriteSheet::Tiles, rc(8, 1, TILES_COLS)); // 8.b grass 1
    pub const GRASS_2: (SpriteSheet, u32) = (SpriteSheet::Tiles, rc(8, 2, TILES_COLS)); // 8.c grass 2
    pub const GRASS_3: (SpriteSheet, u32) = (SpriteSheet::Tiles, rc(8, 3, TILES_COLS)); // 8.d grass 3
    pub const GRASS_VARIANTS: [(SpriteSheet, u32); 3] = [GRASS, GRASS_2, GRASS_3];
    pub const TALL_GRASS: (SpriteSheet, u32) = (SpriteSheet::Tiles, rc(20, 7, TILES_COLS)); // 20.h wheat
    pub const STONE: (SpriteSheet, u32) = (SpriteSheet::Tiles, rc(10, 1, TILES_COLS)); // 10.b stone floor

    // Structures
    pub const STAIRS_DOWN: (SpriteSheet, u32) = (SpriteSheet::Tiles, rc(17, 7, TILES_COLS)); // 17.h staircase down
    pub const STAIRS_UP: (SpriteSheet, u32) = (SpriteSheet::Tiles, rc(17, 8, TILES_COLS)); // 17.i staircase up
    pub const DOOR: (SpriteSheet, u32) = (SpriteSheet::Tiles, rc(17, 2, TILES_COLS)); // 17.c framed door (shut)
    pub const DOOR_OPEN: (SpriteSheet, u32) = (SpriteSheet::Tiles, rc(17, 3, TILES_COLS)); // 17.d framed door (open)

    // Green door for Overgrown rooms
    pub const DOOR_GREEN: (SpriteSheet, u32) = (SpriteSheet::Tiles, rc(17, 1, TILES_COLS)); // 17.b door 2 (green)
    pub const DOOR_GREEN_OPEN: (SpriteSheet, u32) = (SpriteSheet::Tiles, rc(17, 3, TILES_COLS)); // 17.d uses same open sprite

    // Grated door for Crypt rooms
    pub const DOOR_GRATED: (SpriteSheet, u32) = (SpriteSheet::Tiles, rc(17, 6, TILES_COLS)); // 17.g grated door
    pub const CHEST_CLOSED: (SpriteSheet, u32) = (SpriteSheet::Tiles, rc(18, 0, TILES_COLS)); // 18.a chest (closed)
    pub const CHEST_OPEN: (SpriteSheet, u32) = (SpriteSheet::Tiles, rc(18, 1, TILES_COLS)); // 18.b chest (open)

    // Decorative decals
    pub const BONES: (SpriteSheet, u32) = (SpriteSheet::Tiles, rc(11, 1, TILES_COLS)); // 11.b bone 1
    pub const BONES_1: (SpriteSheet, u32) = (SpriteSheet::Tiles, rc(11, 1, TILES_COLS)); // 11.b bone 1
    pub const BONES_2: (SpriteSheet, u32) = (SpriteSheet::Tiles, rc(11, 2, TILES_COLS)); // 11.c bone 2
    pub const BONES_3: (SpriteSheet, u32) = (SpriteSheet::Tiles, rc(11, 3, TILES_COLS)); // 11.d bone 3
    pub const BONES_4: (SpriteSheet, u32) = (SpriteSheet::Tiles, rc(22, 0, TILES_COLS)); // 22.a corpse bones 1
    pub const ROCKS: (SpriteSheet, u32) = (SpriteSheet::Tiles, rc(19, 0, TILES_COLS)); // 19.a large rock 1
    pub const ROCKS_2: (SpriteSheet, u32) = (SpriteSheet::Tiles, rc(19, 1, TILES_COLS)); // 19.b large rock 2
    pub const SKULL: (SpriteSheet, u32) = (SpriteSheet::Tiles, rc(22, 1, TILES_COLS)); // 22.b corpse bones 2
    pub const PLANT: (SpriteSheet, u32) = (SpriteSheet::Tiles, rc(20, 0, TILES_COLS)); // 20.a buckwheat
    pub const MUSHROOM: (SpriteSheet, u32) = (SpriteSheet::Tiles, rc(21, 0, TILES_COLS)); // 21.a small mushrooms
    pub const MUSHROOM_LARGE: (SpriteSheet, u32) = (SpriteSheet::Tiles, rc(21, 1, TILES_COLS)); // 21.b large mushroom
    pub const FLOWERS: (SpriteSheet, u32) = (SpriteSheet::Tiles, rc(8, 3, TILES_COLS)); // 8.d grass 3

    // Additional plants (row 20)
    pub const PLANT_FLAX: (SpriteSheet, u32) = (SpriteSheet::Tiles, rc(20, 1, TILES_COLS)); // 20.b flax
    pub const PLANT_PAPYRUS: (SpriteSheet, u32) = (SpriteSheet::Tiles, rc(20, 2, TILES_COLS)); // 20.c papyrus
    pub const PLANT_RICE: (SpriteSheet, u32) = (SpriteSheet::Tiles, rc(20, 6, TILES_COLS)); // 20.g rice
    pub const PLANT_CORN: (SpriteSheet, u32) = (SpriteSheet::Tiles, rc(20, 8, TILES_COLS)); // 20.i maize/corn

    // Blood and slime decals (row 23)
    pub const BLOOD_1: (SpriteSheet, u32) = (SpriteSheet::Tiles, rc(23, 0, TILES_COLS)); // 23.a blood spatter 1
    pub const BLOOD_2: (SpriteSheet, u32) = (SpriteSheet::Tiles, rc(23, 1, TILES_COLS)); // 23.b blood spatter 2
    pub const SLIME_SMALL: (SpriteSheet, u32) = (SpriteSheet::Tiles, rc(23, 2, TILES_COLS)); // 23.c slime small
    pub const SLIME_LARGE: (SpriteSheet, u32) = (SpriteSheet::Tiles, rc(23, 3, TILES_COLS)); // 23.d slime large

    // Coffins (row 24)
    pub const COFFIN_CLOSED: (SpriteSheet, u32) = (SpriteSheet::Tiles, rc(24, 0, TILES_COLS)); // 24.a coffin closed
    pub const COFFIN_OPEN: (SpriteSheet, u32) = (SpriteSheet::Tiles, rc(24, 2, TILES_COLS)); // 24.c coffin open

    // Barrel (row 18)
    pub const BARREL: (SpriteSheet, u32) = (SpriteSheet::Tiles, rc(18, 4, TILES_COLS)); // 18.e barrel

    // Shop room tiles
    // Red stone floor variants (row 12)
    pub const FLOOR_SHOP: (SpriteSheet, u32) = (SpriteSheet::Tiles, rc(12, 1, TILES_COLS)); // 12.b red stone 1
    pub const FLOOR_SHOP_2: (SpriteSheet, u32) = (SpriteSheet::Tiles, rc(12, 2, TILES_COLS)); // 12.c red stone 2
    pub const FLOOR_SHOP_3: (SpriteSheet, u32) = (SpriteSheet::Tiles, rc(12, 3, TILES_COLS)); // 12.d red stone 3
    pub const FLOOR_SHOP_VARIANTS: [(SpriteSheet, u32); 3] = [FLOOR_SHOP, FLOOR_SHOP_2, FLOOR_SHOP_3];
    // Shop doors (row 17)
    pub const DOOR_SHOP: (SpriteSheet, u32) = (SpriteSheet::Tiles, rc(17, 4, TILES_COLS)); // 17.e framed door 2 (shut)
    pub const DOOR_SHOP_OPEN: (SpriteSheet, u32) = (SpriteSheet::Tiles, rc(17, 5, TILES_COLS)); // 17.f framed door 2 (open)
    // Shop decorations (row 18)
    pub const JAR_CLOSED: (SpriteSheet, u32) = (SpriteSheet::Tiles, rc(18, 2, TILES_COLS)); // 18.c jar closed
    pub const JAR_OPEN: (SpriteSheet, u32) = (SpriteSheet::Tiles, rc(18, 3, TILES_COLS)); // 18.d jar open
    pub const ORE_SACK: (SpriteSheet, u32) = (SpriteSheet::Tiles, rc(18, 5, TILES_COLS)); // 18.f ore sack

    // ===== ROGUES SHEET (player characters, NPCs) =====

    pub const PLAYER: (SpriteSheet, u32) = (SpriteSheet::Rogues, rc(1, 3, ROGUES_COLS)); // 1.d rogue
    pub const FIGHTER: (SpriteSheet, u32) = (SpriteSheet::Rogues, rc(2, 1, ROGUES_COLS)); // 2.b male fighter
    pub const RANGER: (SpriteSheet, u32) = (SpriteSheet::Rogues, rc(1, 2, ROGUES_COLS)); // 1.c ranger
    pub const WIZARD: (SpriteSheet, u32) = (SpriteSheet::Rogues, rc(5, 1, ROGUES_COLS)); // 5.b male wizard
    pub const KNIGHT: (SpriteSheet, u32) = (SpriteSheet::Rogues, rc(2, 0, ROGUES_COLS)); // 2.a knight
    pub const ELF: (SpriteSheet, u32) = (SpriteSheet::Rogues, rc(1, 1, ROGUES_COLS)); // 1.b elf
    pub const DWARF: (SpriteSheet, u32) = (SpriteSheet::Rogues, rc(1, 0, ROGUES_COLS)); // 1.a dwarf

    // ===== MONSTERS SHEET (enemies) =====

    pub const SKELETON: (SpriteSheet, u32) = (SpriteSheet::Monsters, rc(5, 0, MONSTERS_COLS)); // 5.a skeleton
    pub const SKELETON_ARCHER: (SpriteSheet, u32) = (SpriteSheet::Monsters, rc(5, 1, MONSTERS_COLS)); // 5.b skeleton archer
    pub const RAT: (SpriteSheet, u32) = (SpriteSheet::Monsters, rc(7, 11, MONSTERS_COLS)); // 7.l giant rat
    pub const GOBLIN: (SpriteSheet, u32) = (SpriteSheet::Monsters, rc(1, 2, MONSTERS_COLS)); // 1.c goblin
    pub const ORC: (SpriteSheet, u32) = (SpriteSheet::Monsters, rc(1, 0, MONSTERS_COLS)); // 1.a orc
    pub const ZOMBIE: (SpriteSheet, u32) = (SpriteSheet::Monsters, rc(5, 4, MONSTERS_COLS)); // 5.e zombie
    pub const SLIME: (SpriteSheet, u32) = (SpriteSheet::Monsters, rc(3, 0, MONSTERS_COLS)); // 3.a small slime
    pub const SPIDER: (SpriteSheet, u32) = (SpriteSheet::Monsters, rc(7, 8, MONSTERS_COLS)); // 7.i giant spider
    pub const BAT: (SpriteSheet, u32) = (SpriteSheet::Monsters, rc(7, 6, MONSTERS_COLS)); // 7.g giant bat

    // ===== ITEMS SHEET (weapons, armor, potions, etc.) =====

    // Weapons
    pub const SWORD: (SpriteSheet, u32) = (SpriteSheet::Items, rc(1, 3, ITEMS_COLS)); // 1.d long sword
    pub const DAGGER: (SpriteSheet, u32) = (SpriteSheet::Items, rc(1, 0, ITEMS_COLS)); // 1.a dagger
    pub const BOW: (SpriteSheet, u32) = (SpriteSheet::Items, rc(10, 2, ITEMS_COLS)); // 10.c long bow
    pub const ARROW: (SpriteSheet, u32) = (SpriteSheet::Items, rc(24, 0, ITEMS_COLS)); // 24.a arrow
    pub const AXE: (SpriteSheet, u32) = (SpriteSheet::Items, rc(4, 1, ITEMS_COLS)); // 4.b battle axe
    pub const STAFF: (SpriteSheet, u32) = (SpriteSheet::Items, rc(11, 0, ITEMS_COLS)); // 11.a crystal staff

    // Potions
    pub const RED_POTION: (SpriteSheet, u32) = (SpriteSheet::Items, rc(20, 1, ITEMS_COLS)); // 20.b red potion
    pub const BLUE_POTION: (SpriteSheet, u32) = (SpriteSheet::Items, rc(21, 3, ITEMS_COLS)); // 21.d blue potion
    pub const GREEN_POTION: (SpriteSheet, u32) = (SpriteSheet::Items, rc(20, 4, ITEMS_COLS)); // 20.e green potion
    pub const AMBER_POTION: (SpriteSheet, u32) = (SpriteSheet::Items, rc(21, 4, ITEMS_COLS)); // 21.e orange potion

    // Other items
    pub const COINS: (SpriteSheet, u32) = (SpriteSheet::Items, rc(25, 1, ITEMS_COLS)); // 25.b small stacks
    pub const SCROLL: (SpriteSheet, u32) = (SpriteSheet::Items, rc(22, 0, ITEMS_COLS)); // 22.a scroll
    pub const KEY: (SpriteSheet, u32) = (SpriteSheet::Items, rc(23, 0, ITEMS_COLS)); // 23.a gold key

    // Food (row 26)
    pub const CHEESE: (SpriteSheet, u32) = (SpriteSheet::Items, rc(26, 0, ITEMS_COLS)); // 26.a cheese
    pub const BREAD: (SpriteSheet, u32) = (SpriteSheet::Items, rc(26, 1, ITEMS_COLS)); // 26.b bread
    pub const APPLE: (SpriteSheet, u32) = (SpriteSheet::Items, rc(26, 2, ITEMS_COLS)); // 26.c apple

    // Traps (inventory item uses flame sword icon)
    pub const FIRE_TRAP: (SpriteSheet, u32) = (SpriteSheet::Items, rc(1, 10, ITEMS_COLS)); // 1.k flame sword

    // ===== TILES SHEET - TRAPS =====

    // Traps (row 17 of tiles.png)
    pub const PRESSURE_PLATE: (SpriteSheet, u32) = (SpriteSheet::Tiles, rc(17, 9, TILES_COLS)); // 17.j pressure plate (up)
    pub const PRESSURE_PLATE_DOWN: (SpriteSheet, u32) = (SpriteSheet::Tiles, rc(17, 10, TILES_COLS)); // 17.k pressure plate (down)

    // UI Icons (using items that work as icons)
    pub const HEART: (SpriteSheet, u32) = (SpriteSheet::Items, rc(17, 0, ITEMS_COLS)); // 17.a red pendant
    pub const DIAMOND: (SpriteSheet, u32) = (SpriteSheet::Items, rc(17, 2, ITEMS_COLS)); // 17.c crystal pendant

    // ===== ANIMATED TILES SHEET (light sources, environmental effects) =====

    /// Brazier (lit) - first frame, row 2 in animated-tiles.png
    pub const BRAZIER: (SpriteSheet, u32) = (SpriteSheet::AnimatedTiles, rc(2, 0, ANIMATED_TILES_COLS));
    /// Fire pit (lit) - first frame, row 4 in animated-tiles.png
    pub const FIRE_PIT: (SpriteSheet, u32) = (SpriteSheet::AnimatedTiles, rc(4, 0, ANIMATED_TILES_COLS));
    /// Animated water - first frame, row 11 in animated-tiles.png (11 frames)
    pub const WATER_ANIMATED: (SpriteSheet, u32) = (SpriteSheet::AnimatedTiles, rc(11, 0, ANIMATED_TILES_COLS));
    /// Fire effect - first frame, row 9 in animated-tiles.png (for burning entities)
    pub const FIRE_EFFECT: (SpriteSheet, u32) = (SpriteSheet::AnimatedTiles, rc(9, 0, ANIMATED_TILES_COLS));
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum TileType {
    Empty,
    Floor,
    Wall,
    Water,
    Grass,
    TallGrass, // Blocks vision but is walkable
    #[allow(dead_code)] // Reserved for future terrain
    Stone,
    StairsDown,
    StairsUp,
}

impl TileType {
    /// Get the sprite sheet and tile ID for this tile type
    pub fn sprite_ref(&self) -> (SpriteSheet, u32) {
        match self {
            TileType::Empty => tile_ids::EMPTY,
            TileType::Floor => tile_ids::FLOOR,
            TileType::Wall => tile_ids::WALL,
            TileType::Water => tile_ids::WATER,
            TileType::Grass => tile_ids::GRASS,
            TileType::TallGrass => tile_ids::TALL_GRASS,
            TileType::Stone => tile_ids::STONE,
            TileType::StairsDown => tile_ids::STAIRS_DOWN,
            TileType::StairsUp => tile_ids::STAIRS_UP,
        }
    }

    /// Get just the tile ID (for backwards compatibility during migration)
    pub fn tile_id(&self) -> u32 {
        self.sprite_ref().1
    }

    pub fn is_walkable(&self) -> bool {
        matches!(
            self,
            TileType::Floor
                | TileType::Grass
                | TileType::TallGrass
                | TileType::Water
                | TileType::StairsDown
                | TileType::StairsUp
        )
    }

    pub fn blocks_vision(&self) -> bool {
        matches!(self, TileType::Wall | TileType::Empty | TileType::TallGrass)
    }
}

#[derive(Debug, Clone)]
pub struct Tile {
    pub tile_type: TileType,
    pub explored: bool,
    pub visible: bool,
    /// Game time until which this tile is magically revealed (Scroll of Reveal)
    pub revealed_until: Option<f32>,
    /// Optional sprite override (for oriented walls, etc.)
    pub sprite_override: Option<(SpriteSheet, u32)>,
}

impl Tile {
    pub fn new(tile_type: TileType) -> Self {
        Self {
            tile_type,
            explored: false,
            visible: false,
            revealed_until: None,
            sprite_override: None,
        }
    }

    /// Get the sprite to render for this tile
    pub fn sprite(&self) -> (SpriteSheet, u32) {
        self.sprite_override.unwrap_or_else(|| self.tile_type.sprite_ref())
    }
}

impl Default for Tile {
    fn default() -> Self {
        Self::new(TileType::Empty)
    }
}
