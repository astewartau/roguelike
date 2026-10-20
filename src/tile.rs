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
/// - Tiles: 17 columns (tiles.png 544x864)
/// - Rogues: 7 columns (rogues.png 224x224)
/// - Monsters: 12 columns (monsters.png 384x416)
/// - Items: 11 columns (items.png 352x1280: stock is 26 rows, tools/make_tiles.py adds 27-40)
// A catalogue of what the sprite sheets contain, not a list of what the game
// currently draws. Entries for art nothing spawns yet (and alternates like
// SKELETON_ARCHER, superseded by SKELETON plus a BOW overlay) are kept so the
// available tiles stay discoverable from code.
#[allow(dead_code)]
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

    // Dirt walls for Cavern rooms. Warm brown, so a cave reads as clearly
    // different from the cold blue-grey brick of the built dungeon.
    pub const WALL_DIRT: (SpriteSheet, u32) = (SpriteSheet::Tiles, rc(1, 1, TILES_COLS)); // 1.b dirt wall (side)
    pub const WALL_DIRT_TOP: (SpriteSheet, u32) = (SpriteSheet::Tiles, rc(1, 0, TILES_COLS)); // 1.a dirt wall (top)
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
    /// Oil puddle: the large pool decal (23.d), rendered with a dark brown tint
    pub const OIL_PUDDLE: (SpriteSheet, u32) = (SpriteSheet::Tiles, rc(23, 3, TILES_COLS));

    // Coffins (row 24)
    pub const COFFIN_CLOSED: (SpriteSheet, u32) = (SpriteSheet::Tiles, rc(24, 0, TILES_COLS)); // 24.a coffin closed
    pub const COFFIN_OPEN: (SpriteSheet, u32) = (SpriteSheet::Tiles, rc(24, 2, TILES_COLS)); // 24.c coffin open

    // Barrel (row 18)
    pub const BARREL: (SpriteSheet, u32) = (SpriteSheet::Tiles, rc(18, 4, TILES_COLS)); // 18.e barrel

    // Cavern tiles (row 27, drawn by assets/custom/make_cave_tiles.py).
    /// Stalagmites: a transparent decal sprite. Spawned as a blocking entity --
    /// cover you can shoot over but not walk through.
    pub const CAVE_STALAGMITES: (SpriteSheet, u32) = (SpriteSheet::Tiles, rc(27, 0, TILES_COLS)); // 27.a
    /// Stalactites: drawn hanging from the top of the tile, so it is used as a
    /// decal laid over a cave wall rather than as a wall sprite of its own.
    pub const CAVE_STALACTITES: (SpriteSheet, u32) = (SpriteSheet::Tiles, rc(27, 1, TILES_COLS)); // 27.b
    /// Glowing mushrooms: a walkable decal entity that sheds light (and burns).
    pub const CAVE_GLOW_MUSHROOMS: (SpriteSheet, u32) = (SpriteSheet::Tiles, rc(27, 2, TILES_COLS)); // 27.c
    /// Crystal cluster: a walkable decal entity that sheds a cold light.
    pub const CAVE_CRYSTAL_CLUSTER: (SpriteSheet, u32) = (SpriteSheet::Tiles, rc(27, 3, TILES_COLS)); // 27.d
    /// Ore-veined cave wall: full-bleed, used as a wall sprite override.
    /// Decoration for now -- there is no mining action.
    pub const CAVE_ORE_WALL: (SpriteSheet, u32) = (SpriteSheet::Tiles, rc(27, 4, TILES_COLS)); // 27.e
    /// Rubble cave floor: full-bleed, used as the Cavern floor sprite override.
    pub const CAVE_RUBBLE_FLOOR: (SpriteSheet, u32) = (SpriteSheet::Tiles, rc(27, 5, TILES_COLS)); // 27.f

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
    pub const MONK: (SpriteSheet, u32) = (SpriteSheet::Rogues, rc(3, 5, ROGUES_COLS)); // 3.f schema monk
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
    pub const GIANT_SPIDER: (SpriteSheet, u32) = SPIDER; // 7.i giant spider
    pub const LESSER_GIANT_SPIDER: (SpriteSheet, u32) = (SpriteSheet::Monsters, rc(7, 9, MONSTERS_COLS)); // 7.j lesser giant spider
    pub const BAT: (SpriteSheet, u32) = (SpriteSheet::Monsters, rc(7, 6, MONSTERS_COLS)); // 7.g giant bat
    pub const GOBLIN_SHAMAN: (SpriteSheet, u32) = (SpriteSheet::Monsters, rc(1, 6, MONSTERS_COLS)); // 1.g goblin mage
    /// Spider web on the floor. No sheet has one, so it's custom
    /// (tools/make_tiles.py).
    pub const WEB: (SpriteSheet, u32) = (SpriteSheet::Items, rc(29, 9, ITEMS_COLS)); // 29.j web (custom)

    // ===== ITEMS SHEET (weapons, armor, potions, etc.) =====

    // Weapons
    pub const SWORD: (SpriteSheet, u32) = (SpriteSheet::Items, rc(1, 3, ITEMS_COLS)); // 1.d long sword
    pub const DAGGER: (SpriteSheet, u32) = (SpriteSheet::Items, rc(1, 0, ITEMS_COLS)); // 1.a dagger
    pub const BOW: (SpriteSheet, u32) = (SpriteSheet::Items, rc(10, 2, ITEMS_COLS)); // 10.c long bow
    pub const ARROW: (SpriteSheet, u32) = (SpriteSheet::Items, rc(24, 0, ITEMS_COLS)); // 24.a arrow
    pub const AXE: (SpriteSheet, u32) = (SpriteSheet::Items, rc(4, 1, ITEMS_COLS)); // 4.b battle axe
    pub const STAFF: (SpriteSheet, u32) = (SpriteSheet::Items, rc(11, 0, ITEMS_COLS)); // 11.a crystal staff

    // Armor (rows 13 & 16)
    pub const LEATHER_ARMOR: (SpriteSheet, u32) = (SpriteSheet::Items, rc(13, 1, ITEMS_COLS)); // 13.b leather armor
    pub const CHAIN_MAIL: (SpriteSheet, u32) = (SpriteSheet::Items, rc(13, 3, ITEMS_COLS)); // 13.d chain mail
    pub const HELMET: (SpriteSheet, u32) = (SpriteSheet::Items, rc(16, 4, ITEMS_COLS)); // 16.e helm

    // Accessories (rows 17-19: pendants & rings)
    pub const RING: (SpriteSheet, u32) = (SpriteSheet::Items, rc(18, 3, ITEMS_COLS)); // 18.d ruby ring
    pub const AMULET: (SpriteSheet, u32) = (SpriteSheet::Items, rc(17, 2, ITEMS_COLS)); // 17.c crystal pendant

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
    pub const BOTTLE_WATER: (SpriteSheet, u32) = (SpriteSheet::Items, rc(26, 4, ITEMS_COLS)); // 26.e bottle of water

    /// The fire trap item: the pressure plate it's set as, copied into the
    /// items sheet for the inventory (tools/make_tiles.py), and badged with
    /// a flame (`item_defs::badge`).
    pub const FIRE_TRAP: (SpriteSheet, u32) = (SpriteSheet::Items, rc(30, 0, ITEMS_COLS)); // 30.a pressure plate (copied from tiles 17.j)
    pub const FLAME_SWORD: (SpriteSheet, u32) = (SpriteSheet::Items, rc(1, 10, ITEMS_COLS)); // 1.k flame sword (blue-flamed)

    // Custom ability icons (drawn by assets/custom/make_tiles.py, pasted into rows 26-27)
    pub const RAISE_DEAD: (SpriteSheet, u32) = (SpriteSheet::Items, rc(26, 5, ITEMS_COLS)); // 26.f raise dead (custom)
    pub const LIFE_DRAIN: (SpriteSheet, u32) = (SpriteSheet::Items, rc(26, 6, ITEMS_COLS)); // 26.g life drain (custom)
    pub const TAME: (SpriteSheet, u32) = (SpriteSheet::Items, rc(27, 0, ITEMS_COLS)); // 27.a tame (custom)
    pub const REST: (SpriteSheet, u32) = (SpriteSheet::Items, rc(27, 1, ITEMS_COLS)); // 27.b rest (custom)
    pub const SLEEP: (SpriteSheet, u32) = (SpriteSheet::Items, rc(27, 2, ITEMS_COLS)); // 27.c sleep (custom)
    pub const BARKSKIN: (SpriteSheet, u32) = (SpriteSheet::Items, rc(27, 3, ITEMS_COLS)); // 27.d barkskin (custom)
    pub const SPRINT: (SpriteSheet, u32) = (SpriteSheet::Items, rc(27, 4, ITEMS_COLS)); // 27.e sprint (custom)
    pub const DISENGAGE: (SpriteSheet, u32) = (SpriteSheet::Items, rc(27, 5, ITEMS_COLS)); // 27.f disengage (custom)
    pub const TUMBLE: (SpriteSheet, u32) = (SpriteSheet::Items, rc(27, 6, ITEMS_COLS)); // 27.g tumble (custom)
    pub const CRIPPLING_SHOT: (SpriteSheet, u32) = (SpriteSheet::Items, rc(27, 7, ITEMS_COLS)); // 27.h crippling shot (custom)
    pub const STUN: (SpriteSheet, u32) = (SpriteSheet::Items, rc(27, 8, ITEMS_COLS)); // 27.i stun (custom)
    /// Torch item icons: the animated sheet's torch frames, copied into
    /// items.png by tools/make_tiles.py so the inventory (which draws from the
    /// items sheet) can show them.
    pub const TORCH_ICON: (SpriteSheet, u32) = (SpriteSheet::Items, rc(27, 9, ITEMS_COLS)); // 27.j torch, lit (copied from animated-tiles 6)
    pub const TORCH_UNLIT_ICON: (SpriteSheet, u32) = (SpriteSheet::Items, rc(27, 10, ITEMS_COLS)); // 27.k torch, unlit (copied from animated-tiles 5)
    /// The torch a creature holds, drawn over its sprite. Custom art: the
    /// stock torch (`TORCH`) is a floor sprite as long as a person is tall
    /// and covered its carrier's face. The lit one is `HELD_TORCH_FRAMES`
    /// frames in a row; the renderer moves the cell so `HELD_TORCH_GRIP`
    /// lands on the carrier's hand (`off_hand_anchor`).
    pub const HELD_TORCH: (SpriteSheet, u32) = (SpriteSheet::Items, rc(28, 0, ITEMS_COLS)); // 28.a-f held torch, lit (custom, 6 frames)
    pub const HELD_TORCH_UNLIT: (SpriteSheet, u32) = (SpriteSheet::Items, rc(28, 6, ITEMS_COLS)); // 28.g held torch, unlit (custom)
    pub const HELD_TORCH_FRAMES: u32 = 6;
    /// Where the held torch's grip is in its cell, in pixels from the
    /// top-left. A copy of `HELD_TORCH_GRIP` in tools/make_tiles.py, which
    /// draws the torch around it: change both together.
    pub const HELD_TORCH_GRIP: (i32, i32) = (23, 21);
    /// Pixels along each side of a sheet cell.
    pub const CELL_PX: i32 = 32;

    // Projectiles in flight, one cell per heading: `FLIGHT_DIRECTIONS`
    // consecutive cells going anticlockwise from due east, picked by
    // `flight_heading` so a projectile faces where it's going (custom,
    // tools/make_tiles.py).
    pub const FLIGHT_DIRECTIONS: u32 = 16;
    pub const ARROW_FLIGHT: (SpriteSheet, u32) = (SpriteSheet::Items, rc(31, 0, ITEMS_COLS)); // 31.a-32.e arrow in flight (custom)
    pub const GRAVE_BOLT_FLIGHT: (SpriteSheet, u32) = (SpriteSheet::Items, rc(33, 0, ITEMS_COLS)); // 33.a-34.e grave bolt in flight (custom)
    pub const FIREBALL_FLIGHT: (SpriteSheet, u32) = (SpriteSheet::Items, rc(35, 0, ITEMS_COLS)); // 35.a-36.e fireball in flight (custom)
    /// One tile of a blast: flash, fireball, smoke, `EXPLOSION_FRAMES`
    /// frames, in `EXPLOSION_VARIANTS` differently-shaped versions one after
    /// another. Played on every tile an explosion covers (custom).
    pub const EXPLOSION: (SpriteSheet, u32) = (SpriteSheet::Items, rc(37, 0, ITEMS_COLS)); // 37.a-39.b explosion, 3 variants (custom)
    pub const EXPLOSION_FRAMES: u32 = 8;
    pub const EXPLOSION_VARIANTS: u32 = 3;
    /// Call Rain falling on one tile, `RAIN_FRAMES` looping frames (custom).
    pub const RAIN: (SpriteSheet, u32) = (SpriteSheet::Items, rc(40, 0, ITEMS_COLS)); // 40.a-d rain (custom)
    pub const RAIN_FRAMES: u32 = 4;

    /// Which of the `FLIGHT_DIRECTIONS` cells faces along `direction`, a
    /// heading on the grid (x right, y up the screen).
    pub fn flight_heading(direction: (f32, f32)) -> u32 {
        let step = std::f32::consts::TAU / FLIGHT_DIRECTIONS as f32;
        let k = (direction.1.atan2(direction.0) / step).round() as i32;
        k.rem_euclid(FLIGHT_DIRECTIONS as i32) as u32
    }

    // Item badges: a glyph in the top-right corner of an otherwise empty
    // cell, drawn over a potion or scroll to say what it does, since every
    // scroll shares one sprite (custom, tools/make_tiles.py). Whole cells so
    // a badge lines up with the item under it at any size.
    pub const BADGE_HEAL: (SpriteSheet, u32) = (SpriteSheet::Items, rc(28, 7, ITEMS_COLS)); // 28.h heart (custom)
    pub const BADGE_REGEN: (SpriteSheet, u32) = (SpriteSheet::Items, rc(28, 8, ITEMS_COLS)); // 28.i green cross (custom)
    pub const BADGE_STRENGTH: (SpriteSheet, u32) = (SpriteSheet::Items, rc(28, 9, ITEMS_COLS)); // 28.j flexed arm (custom)
    pub const BADGE_CONFUSION: (SpriteSheet, u32) = (SpriteSheet::Items, rc(28, 10, ITEMS_COLS)); // 28.k swirl (custom)
    pub const BADGE_INVISIBILITY: (SpriteSheet, u32) = (SpriteSheet::Items, rc(29, 0, ITEMS_COLS)); // 29.a struck-out eye (custom)
    pub const BADGE_SPEED: (SpriteSheet, u32) = (SpriteSheet::Items, rc(29, 1, ITEMS_COLS)); // 29.b lightning bolt (custom)
    pub const BADGE_PROTECTION: (SpriteSheet, u32) = (SpriteSheet::Items, rc(29, 2, ITEMS_COLS)); // 29.c shield (custom)
    pub const BADGE_BLINK: (SpriteSheet, u32) = (SpriteSheet::Items, rc(29, 3, ITEMS_COLS)); // 29.d sparkle and trail (custom)
    pub const BADGE_FEAR: (SpriteSheet, u32) = (SpriteSheet::Items, rc(29, 4, ITEMS_COLS)); // 29.e skull (custom)
    pub const BADGE_FIREBALL: (SpriteSheet, u32) = (SpriteSheet::Items, rc(29, 5, ITEMS_COLS)); // 29.f flame (custom)
    pub const BADGE_REVEAL: (SpriteSheet, u32) = (SpriteSheet::Items, rc(29, 6, ITEMS_COLS)); // 29.g open eye (custom)
    pub const BADGE_MAPPING: (SpriteSheet, u32) = (SpriteSheet::Items, rc(29, 7, ITEMS_COLS)); // 29.h folded map (custom)
    pub const BADGE_SLOW: (SpriteSheet, u32) = (SpriteSheet::Items, rc(29, 8, ITEMS_COLS)); // 29.i hourglass (custom)

    // Class-kit ability icons: stock 32rogues sprites (no custom art), some
    // tinted in the UI via `ui::hotbar::ability_icon_tint`.
    pub const GUARD: (SpriteSheet, u32) = (SpriteSheet::Items, rc(12, 0, ITEMS_COLS)); // 12.a buckler
    pub const BONE_WARD: (SpriteSheet, u32) = (SpriteSheet::Tiles, rc(11, 3, TILES_COLS)); // 11.d bone 3 (tinted pale blue)
    pub const SACRIFICE: (SpriteSheet, u32) = (SpriteSheet::Monsters, rc(5, 0, MONSTERS_COLS)); // 5.a skeleton (tinted like a raised ally)
    pub const CORPSE_EXPLOSION: (SpriteSheet, u32) = (SpriteSheet::Tiles, rc(22, 0, TILES_COLS)); // 22.a corpse bones 1 (tinted fiery)
    pub const THORNS: (SpriteSheet, u32) = (SpriteSheet::Tiles, rc(20, 1, TILES_COLS)); // 20.b flax (tinted thorny green)
    pub const ENTANGLE: (SpriteSheet, u32) = (SpriteSheet::Tiles, rc(20, 7, TILES_COLS)); // 20.h wheat (tinted vine green)
    pub const CALL_RAIN: (SpriteSheet, u32) = (SpriteSheet::Tiles, rc(23, 3, TILES_COLS)); // 23.d slime large (tinted rain blue: a puddle)
    pub const SHIELD_BASH: (SpriteSheet, u32) = (SpriteSheet::Items, rc(12, 1, ITEMS_COLS)); // 12.b kite shield (tinted steel-orange)
    /// Grave Bolt's hotbar icon: the bolt in flight, heading up and right.
    /// (It was the bone decal 11.c, which carries the floor behind it.)
    pub const GRAVE_BOLT: (SpriteSheet, u32) = (SpriteSheet::Items, rc(33, 2, ITEMS_COLS)); // 33.c grave bolt in flight, NE (custom)

    // ===== TILES SHEET - TRAPS =====

    // Traps (row 17 of tiles.png)
    pub const PRESSURE_PLATE: (SpriteSheet, u32) = (SpriteSheet::Tiles, rc(17, 9, TILES_COLS)); // 17.j pressure plate (up)
    pub const PRESSURE_PLATE_DOWN: (SpriteSheet, u32) = (SpriteSheet::Tiles, rc(17, 10, TILES_COLS)); // 17.k pressure plate (down)
    /// Revealed dungeon floor trap (tinted per trap kind)
    pub const TRAP_DOOR: (SpriteSheet, u32) = (SpriteSheet::Tiles, rc(17, 13, TILES_COLS)); // 17.n trap door
    /// Revealed spike trap
    pub const SPIKES: (SpriteSheet, u32) = (SpriteSheet::Tiles, rc(17, 16, TILES_COLS)); // 17.q spikes (up)

    // Room furniture (repurposed tiles, tinted to distinguish)
    /// Fountain: a stone basin of water with a spout. Custom
    /// (tools/make_tiles.py); it used to be the pit, tinted blue, which read
    /// as a hole in the floor.
    pub const FOUNTAIN: (SpriteSheet, u32) = (SpriteSheet::Items, rc(29, 10, ITEMS_COLS)); // 29.k fountain (custom)
    pub const PIT: (SpriteSheet, u32) = (SpriteSheet::Tiles, rc(17, 12, TILES_COLS)); // 17.m pit
    /// Altar: the pentagram tile, red-tinted
    pub const ALTAR: (SpriteSheet, u32) = (SpriteSheet::Tiles, rc(17, 14, TILES_COLS)); // 17.o pentagram
    /// Shrine: the ankh item sprite, gold-tinted
    pub const SHRINE: (SpriteSheet, u32) = (SpriteSheet::Items, rc(17, 6, ITEMS_COLS)); // items 17.g ankh

    // UI Icons (using items that work as icons)
    /// Panel corner rivet: the buckler, which is a round studded plate and at
    /// icon size reads as a domed metal rivet. Brass-tinted (see
    /// `style::colors::PANEL_RIVET_TINT`) to pull it into the panel palette.
    pub const PANEL_RIVET: (SpriteSheet, u32) = (SpriteSheet::Items, rc(12, 5, ITEMS_COLS)); // 12.f buckler 2
    /// HUD bar icons. The sheets have no heart, star, drumstick or sun, so these are
    /// custom (tools/make_tiles.py, `--paste` writes them into items.png).
    pub const HUD_HEALTH: (SpriteSheet, u32) = (SpriteSheet::Items, rc(26, 7, ITEMS_COLS)); // 26.h heart (custom)
    pub const HUD_XP: (SpriteSheet, u32) = (SpriteSheet::Items, rc(26, 8, ITEMS_COLS)); // 26.i star (custom)
    pub const HUD_HUNGER: (SpriteSheet, u32) = (SpriteSheet::Items, rc(26, 9, ITEMS_COLS)); // 26.j drumstick (custom)
    pub const HUD_LIGHT: (SpriteSheet, u32) = (SpriteSheet::Items, rc(26, 10, ITEMS_COLS)); // 26.k sun (custom)

    // ===== ANIMATED TILES SHEET (light sources, environmental effects) =====

    /// Brazier (lit) - first frame, row 2 in animated-tiles.png
    pub const BRAZIER: (SpriteSheet, u32) = (SpriteSheet::AnimatedTiles, rc(2, 0, ANIMATED_TILES_COLS));
    /// Brazier (unlit) - row 1 in animated-tiles.png (used for toppled braziers)
    pub const BRAZIER_UNLIT: (SpriteSheet, u32) = (SpriteSheet::AnimatedTiles, rc(1, 0, ANIMATED_TILES_COLS));
    /// Fire pit (lit) - first frame, row 4 in animated-tiles.png
    pub const FIRE_PIT: (SpriteSheet, u32) = (SpriteSheet::AnimatedTiles, rc(4, 0, ANIMATED_TILES_COLS));
    /// Fire pit gone out: just the ring of stones (row 3)
    pub const FIRE_PIT_UNLIT: (SpriteSheet, u32) = (SpriteSheet::AnimatedTiles, rc(3, 0, ANIMATED_TILES_COLS));
    /// Hand torch, lit - first frame, row 6 in animated-tiles.png (6 frames)
    pub const TORCH: (SpriteSheet, u32) = (SpriteSheet::AnimatedTiles, rc(6, 0, ANIMATED_TILES_COLS)); // 6. torch (lit)
    /// Hand torch, unlit (row 5)
    pub const TORCH_UNLIT: (SpriteSheet, u32) = (SpriteSheet::AnimatedTiles, rc(5, 0, ANIMATED_TILES_COLS)); // 5. torch (unlit)
    /// Standing lantern, lit - first frame, row 8 in animated-tiles.png (4 frames)
    pub const LANTERN: (SpriteSheet, u32) = (SpriteSheet::AnimatedTiles, rc(8, 0, ANIMATED_TILES_COLS)); // 8. lamp (lit)
    /// Standing lantern, put out (row 7)
    pub const LANTERN_UNLIT: (SpriteSheet, u32) = (SpriteSheet::AnimatedTiles, rc(7, 0, ANIMATED_TILES_COLS)); // 7. lamp (unlit)
    /// Animated water - first frame, row 11 in animated-tiles.png (11 frames)
    pub const WATER_ANIMATED: (SpriteSheet, u32) = (SpriteSheet::AnimatedTiles, rc(11, 0, ANIMATED_TILES_COLS));
    /// Fire effect - first frame, row 9 in animated-tiles.png (for burning
    /// entities, and fuel burning on the floor)
    pub const FIRE_EFFECT: (SpriteSheet, u32) = (SpriteSheet::AnimatedTiles, rc(9, 0, ANIMATED_TILES_COLS));

    /// Where a sprite's off hand is, for drawing a held torch in it.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct OffHand {
        /// The hand, in pixels from the cell's top-left.
        pub x: i32,
        pub y: i32,
        /// The torch leans out to the left instead of the right: for a
        /// sprite whose right-hand side is already holding something.
        pub mirrored: bool,
    }

    /// The off hand of a creature drawn with `sprite`. Read off the sheets
    /// by eye, for the sprites that carry torches today (torch-bearing
    /// enemies and the player classes). Anything else holds it at the
    /// torch's own grip, which suits a typical 32rogues humanoid.
    pub fn off_hand_anchor(sprite: (SpriteSheet, u32)) -> OffHand {
        let at = |x, y| OffHand { x, y, mirrored: false };
        match sprite {
            GOBLIN => at(20, 22),
            // Its right hand holds its staff; the left cups a flame.
            GOBLIN_SHAMAN => OffHand { x: 8, y: 22, mirrored: true },
            FIGHTER => at(25, 20),
            RANGER => at(23, 19),
            ELF => at(22, 20),
            MONK => at(25, 24),
            _ => at(HELD_TORCH_GRIP.0, HELD_TORCH_GRIP.1),
        }
    }
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

    /// Whether this terrain can catch fire (and burn away).
    pub fn is_flammable(&self) -> bool {
        matches!(self, TileType::TallGrass)
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
