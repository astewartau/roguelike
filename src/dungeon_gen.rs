use crate::constants::*;
use crate::grid::Decal;
use crate::tile::{tile_ids, Tile, TileType};
use rand::rngs::StdRng;
use rand::{Rng, SeedableRng};

/// Indices of the largest 4-connected run of open cells in a `wall` grid of
/// `w` x `h`. Used by the cave automaton to discard the pockets it leaves
/// behind. Returns None when every cell is wall.
fn largest_open_region(wall: &[bool], w: usize, h: usize) -> Option<std::collections::HashSet<usize>> {
    use std::collections::{HashSet, VecDeque};

    let mut seen = vec![false; w * h];
    let mut best: Option<HashSet<usize>> = None;

    for start in 0..w * h {
        if wall[start] || seen[start] {
            continue;
        }
        let mut region: HashSet<usize> = HashSet::new();
        let mut queue = VecDeque::new();
        queue.push_back(start);
        seen[start] = true;
        while let Some(i) = queue.pop_front() {
            region.insert(i);
            let (x, y) = ((i % w) as i32, (i / w) as i32);
            for (dx, dy) in [(-1, 0), (1, 0), (0, -1), (0, 1)] {
                let (nx, ny) = (x + dx, y + dy);
                if nx < 0 || ny < 0 || nx >= w as i32 || ny >= h as i32 {
                    continue;
                }
                let ni = ny as usize * w + nx as usize;
                if !wall[ni] && !seen[ni] {
                    seen[ni] = true;
                    queue.push_back(ni);
                }
            }
        }
        if best.as_ref().map(|b| region.len() > b.len()).unwrap_or(true) {
            best = Some(region);
        }
    }
    best
}

/// A rectangle representing a room or region
#[derive(Clone, Copy, Debug)]
pub struct Rect {
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
}

impl Rect {
    pub fn new(x: i32, y: i32, width: i32, height: i32) -> Self {
        Self { x, y, width, height }
    }

    pub fn center(&self) -> (i32, i32) {
        (self.x + self.width / 2, self.y + self.height / 2)
    }

    /// Check if a point is inside this rectangle
    pub fn contains(&self, x: i32, y: i32) -> bool {
        x >= self.x && x < self.x + self.width && y >= self.y && y < self.y + self.height
    }
}

/// Theme for a room that determines terrain and decal generation
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum RoomTheme {
    /// Standard dungeon room with stone floors
    Normal,
    /// Overgrown room with tall grass patches and rough stone walls
    Overgrown,
    /// Flooded room with water pools
    Flooded,
    /// Crypt room with coffins, bones, and skull walls
    Crypt,
    /// Storage room with barrels of food
    Storage,
    /// Shop room with vendor - red stone floors
    Shop,
    /// Natural cave: carved by cellular automata rather than as a rectangle,
    /// with dirt walls, rubble floor, blocking stalagmites and glowing fungus.
    Cavern,
}

/// A room with its theme
#[derive(Clone, Copy, Debug)]
pub struct ThemedRoom {
    pub rect: Rect,
    pub theme: RoomTheme,
}

/// A node in the BSP tree. Either a leaf (contains a room) or an internal node (has two children).
struct BspNode {
    /// The region this node covers
    region: Rect,
    /// The room carved in this region (only for leaves)
    room: Option<Rect>,
    /// Left/top child after split
    left: Option<Box<BspNode>>,
    /// Right/bottom child after split
    right: Option<Box<BspNode>>,
}

impl BspNode {
    fn new(region: Rect) -> Self {
        Self {
            region,
            room: None,
            left: None,
            right: None,
        }
    }

    fn is_leaf(&self) -> bool {
        self.left.is_none() && self.right.is_none()
    }

    /// Recursively split this node until leaves are small enough for rooms.
    fn split(&mut self, rng: &mut impl Rng) {
        // Don't split if already too small
        if self.region.width < DUNGEON_MIN_LEAF_SIZE * 2 && self.region.height < DUNGEON_MIN_LEAF_SIZE * 2 {
            return;
        }

        // Decide split direction based on aspect ratio (prefer splitting the longer axis)
        let split_horizontal = if self.region.width > self.region.height * 2 {
            false // Too wide, split vertically
        } else if self.region.height > self.region.width * 2 {
            true // Too tall, split horizontally
        } else {
            rng.gen_bool(0.5)
        };

        if split_horizontal {
            // Split horizontally (top/bottom children)
            if self.region.height < DUNGEON_MIN_LEAF_SIZE * 2 {
                return; // Can't split this direction
            }

            // Choose split point, keeping both children at least DUNGEON_MIN_LEAF_SIZE
            let split_y = rng.gen_range(DUNGEON_MIN_LEAF_SIZE..self.region.height - DUNGEON_MIN_LEAF_SIZE + 1);

            let top = Rect::new(
                self.region.x,
                self.region.y,
                self.region.width,
                split_y,
            );
            let bottom = Rect::new(
                self.region.x,
                self.region.y + split_y,
                self.region.width,
                self.region.height - split_y,
            );

            self.left = Some(Box::new(BspNode::new(top)));
            self.right = Some(Box::new(BspNode::new(bottom)));
        } else {
            // Split vertically (left/right children)
            if self.region.width < DUNGEON_MIN_LEAF_SIZE * 2 {
                return; // Can't split this direction
            }

            let split_x = rng.gen_range(DUNGEON_MIN_LEAF_SIZE..self.region.width - DUNGEON_MIN_LEAF_SIZE + 1);

            let left = Rect::new(
                self.region.x,
                self.region.y,
                split_x,
                self.region.height,
            );
            let right = Rect::new(
                self.region.x + split_x,
                self.region.y,
                self.region.width - split_x,
                self.region.height,
            );

            self.left = Some(Box::new(BspNode::new(left)));
            self.right = Some(Box::new(BspNode::new(right)));
        }

        // Recursively split children
        if let Some(ref mut left) = self.left {
            left.split(rng);
        }
        if let Some(ref mut right) = self.right {
            right.split(rng);
        }
    }

    /// Create a room in each leaf node.
    fn create_rooms(&mut self, rng: &mut impl Rng) {
        if self.is_leaf() {
            // Create a room within this region, with some margin
            let max_width = self.region.width - DUNGEON_ROOM_MARGIN * 2;
            let max_height = self.region.height - DUNGEON_ROOM_MARGIN * 2;

            if max_width < DUNGEON_MIN_ROOM_SIZE || max_height < DUNGEON_MIN_ROOM_SIZE {
                return; // Region too small for a room
            }

            let room_width = rng.gen_range(DUNGEON_MIN_ROOM_SIZE..=max_width);
            let room_height = rng.gen_range(DUNGEON_MIN_ROOM_SIZE..=max_height);

            // Random position within the region (with margin)
            let room_x = self.region.x + DUNGEON_ROOM_MARGIN +
                rng.gen_range(0..=(max_width - room_width));
            let room_y = self.region.y + DUNGEON_ROOM_MARGIN +
                rng.gen_range(0..=(max_height - room_height));

            self.room = Some(Rect::new(room_x, room_y, room_width, room_height));
        } else {
            if let Some(ref mut left) = self.left {
                left.create_rooms(rng);
            }
            if let Some(ref mut right) = self.right {
                right.create_rooms(rng);
            }
        }
    }

    /// Get a room from this subtree (used for corridor connection).
    /// Returns a room from the left-most leaf if possible, otherwise right.
    fn get_room(&self) -> Option<Rect> {
        if let Some(room) = self.room {
            return Some(room);
        }

        // Try left subtree first, then right
        if let Some(ref left) = self.left {
            if let Some(room) = left.get_room() {
                return Some(room);
            }
        }
        if let Some(ref right) = self.right {
            if let Some(room) = right.get_room() {
                return Some(room);
            }
        }

        None
    }

    /// Collect all rooms in this subtree.
    fn collect_rooms(&self, rooms: &mut Vec<Rect>) {
        if let Some(room) = self.room {
            rooms.push(room);
        }
        if let Some(ref left) = self.left {
            left.collect_rooms(rooms);
        }
        if let Some(ref right) = self.right {
            right.collect_rooms(rooms);
        }
    }
}

/// Result of dungeon generation
pub struct DungeonResult {
    pub tiles: Vec<Tile>,
    pub chest_positions: Vec<(i32, i32)>,
    /// Door positions with their theme (for selecting appropriate sprite)
    pub door_positions: Vec<((i32, i32), RoomTheme)>,
    pub brazier_positions: Vec<(i32, i32)>,
    pub decals: Vec<Decal>,
    pub stairs_up_pos: Option<(i32, i32)>,
    pub stairs_down_pos: Option<(i32, i32)>,
    /// The starting room where the player spawns (for NPC placement and enemy exclusion)
    pub starting_room: Option<Rect>,
    /// All themed rooms for wall theming
    pub themed_rooms: Vec<ThemedRoom>,
    /// Water positions for animated water tiles
    pub water_positions: Vec<(i32, i32)>,
    /// Coffin positions in Crypt rooms
    pub coffin_positions: Vec<(i32, i32)>,
    /// Barrel positions in Storage rooms
    pub barrel_positions: Vec<(i32, i32)>,
    /// Shop vendor spawn position
    pub shop_position: Option<(i32, i32)>,
    /// Shop decoration positions (jars, sacks, etc.)
    pub shop_decor_positions: Vec<(i32, i32)>,
    /// Hidden floor trap positions (kinds are rolled at spawn time)
    pub trap_positions: Vec<(i32, i32)>,
    /// Room furniture positions (fountain/altar/shrine, rolled at spawn time)
    pub furniture_positions: Vec<(i32, i32)>,
    /// The sealed hidden room, if this floor has one
    pub secret_room: Option<Rect>,
    /// The sealed doorway of the hidden room (spawns a SecretDoor entity)
    pub secret_door_pos: Option<(i32, i32)>,
    /// Blocking stalagmite clusters in Cavern rooms
    pub stalagmite_positions: Vec<(i32, i32)>,
    /// Glowing mushroom patches in Cavern rooms (light sources, flammable)
    pub mushroom_positions: Vec<(i32, i32)>,
    /// Crystal clusters in Cavern rooms (light sources)
    pub crystal_positions: Vec<(i32, i32)>,
}

/// The cave dressing picked for a floor's Cavern rooms.
pub struct CavernFeatures {
    pub stalagmites: Vec<(i32, i32)>,
    pub mushrooms: Vec<(i32, i32)>,
    pub crystals: Vec<(i32, i32)>,
}

pub struct DungeonGenerator {
    width: usize,
    height: usize,
    tiles: Vec<Tile>,
    /// Water positions collected during generation
    water_positions: Vec<(i32, i32)>,
    /// Child rng for cosmetic floor-tile sprite variants (see set_tile);
    /// seeded from the layout rng so seeded floors are fully reproducible.
    floor_variant_rng: StdRng,
}

impl DungeonGenerator {
    pub fn new(width: usize, height: usize) -> Self {
        Self {
            width,
            height,
            tiles: vec![Tile::new(TileType::Wall); width * height],
            water_positions: Vec::new(),
            // Reseeded from the layout rng in generate_with_rng.
            floor_variant_rng: StdRng::seed_from_u64(0),
        }
    }

    /// Generate a dungeon floor with an unseeded (entropy) rng.
    /// Prefer `generate_with_rng` for seeded runs. (Kept as the convenience
    /// entry point for tests; the game always passes a seeded rng.)
    #[allow(dead_code)]
    pub fn generate(width: usize, height: usize, floor_num: u32) -> DungeonResult {
        Self::generate_with_rng(width, height, floor_num, &mut rand::thread_rng())
    }

    /// Generate a dungeon floor from the given rng: the same rng state always
    /// produces the same layout. floor_num 0 is the starting floor (no stairs up).
    pub fn generate_with_rng(
        width: usize,
        height: usize,
        floor_num: u32,
        rng: &mut impl Rng,
    ) -> DungeonResult {
        let mut gen = Self::new(width, height);
        // Cosmetic floor-tile variants come from a child rng so set_tile
        // (called without an rng argument) stays deterministic too.
        gen.floor_variant_rng = StdRng::seed_from_u64(rng.gen());
        let mut rng = rng;

        // Create the root BSP node covering the entire map
        let root_region = Rect::new(0, 0, width as i32, height as i32);
        let mut root = BspNode::new(root_region);

        // Recursively split the space
        root.split(&mut rng);

        // Create rooms in each leaf
        root.create_rooms(&mut rng);

        // Collect room rectangles
        let mut room_rects = Vec::new();
        root.collect_rooms(&mut room_rects);

        // Assign themes to rooms
        // First room is always Normal (player spawn), last room is always Normal (stairs down)
        // Guarantee at least one of each special type, plus exactly one Shop
        let mut themed_rooms: Vec<ThemedRoom> = room_rects
            .iter()
            .map(|rect| ThemedRoom { rect: *rect, theme: RoomTheme::Normal })
            .collect();

        // Required themes (excluding Normal and Shop which are handled specially)
        // Cavern first: on a floor with few spare rooms it is the one worth
        // keeping, and `biggest_slot` below pairs it with the roomiest rect.
        let required_themes = [
            RoomTheme::Cavern,
            RoomTheme::Overgrown,
            RoomTheme::Flooded,
            RoomTheme::Crypt,
            RoomTheme::Storage,
        ];

        // Eligible room indices: not first (player spawn) or last (stairs down)
        let last_idx = themed_rooms.len().saturating_sub(1);
        let mut available_indices: Vec<usize> = (1..last_idx).collect();

        // Shuffle available indices for random assignment
        for i in (1..available_indices.len()).rev() {
            let j = rng.gen_range(0..=i);
            available_indices.swap(i, j);
        }

        // A cavern wants elbow room: the automaton needs space to leave an
        // outline worth looking at. Float the largest room big enough for it
        // to the front of the queue so Cavern claims that one.
        let biggest_slot = available_indices
            .iter()
            .enumerate()
            .filter(|&(_, &i)| {
                let r = themed_rooms[i].rect;
                r.width >= CAVERN_MIN_DIM && r.height >= CAVERN_MIN_DIM
            })
            .max_by_key(|&(_, &i)| {
                let r = themed_rooms[i].rect;
                r.width * r.height
            })
            .map(|(slot, _)| slot);
        let cavern_slot = required_themes
            .iter()
            .position(|t| *t == RoomTheme::Cavern)
            .unwrap_or(0);
        if let Some(biggest) = biggest_slot {
            if cavern_slot < available_indices.len() {
                available_indices.swap(cavern_slot, biggest);
            }
        }

        // Assign one of each required theme
        for (i, &theme) in required_themes.iter().enumerate() {
            if i < available_indices.len() {
                themed_rooms[available_indices[i]].theme = theme;
            }
        }

        // Assign exactly one Shop room from remaining available slots
        if available_indices.len() > required_themes.len() {
            themed_rooms[available_indices[required_themes.len()]].theme = RoomTheme::Shop;
        }

        // Fill remaining rooms with random themes (weighted)
        let assigned_count = required_themes.len() + 1; // +1 for shop
        for &idx in available_indices.iter().skip(assigned_count) {
            let roll: f32 = rng.gen();
            themed_rooms[idx].theme = if roll < 0.35 {
                RoomTheme::Normal
            } else if roll < 0.50 {
                RoomTheme::Overgrown
            } else if roll < 0.63 {
                RoomTheme::Flooded
            } else if roll < 0.76 {
                RoomTheme::Crypt
            } else if roll < 0.88 {
                RoomTheme::Storage
            } else {
                // A second cave only if the room has space for one; below
                // that the automaton has nothing to work with.
                let rect = themed_rooms[idx].rect;
                if rect.width >= CAVERN_MIN_DIM && rect.height >= CAVERN_MIN_DIM {
                    RoomTheme::Cavern
                } else {
                    RoomTheme::Normal
                }
            };
        }

        // Carve all rooms into the tile map (with terrain based on theme)
        for room in &themed_rooms {
            gen.carve_themed_room(room, &mut rng);
        }

        // Get plain room rects for functions that don't need themes
        let rooms: Vec<Rect> = themed_rooms.iter().map(|r| r.rect).collect();

        // Connect sibling rooms by traversing the BSP tree
        gen.connect_bsp(&root, &mut rng);

        // Corridors are aimed at room centres and carve straight through
        // whatever is in the way, so a cavern comes out of connect_bsp with a
        // ruler-straight channel through it and, sometimes, pockets of cave
        // that channel never reached. Rough the channels up and join every
        // pocket to the main body before anything reads the tiles.
        gen.connect_cavern_interiors(&themed_rooms, &mut rng);

        // Find door positions (but keep floor tiles - doors are entities)
        let door_positions = gen.find_door_positions(&themed_rooms);

        // Generate decorative decals in rooms
        let decals = gen.generate_themed_decals(&themed_rooms, &mut rng);

        // Place stairs
        // First room is the starting room (player spawns here)
        // On floor 0, no stairs up. On other floors, stairs up in first room.
        // Stairs down always in the last room (furthest from start).
        let stairs_up_pos = if floor_num > 0 && !rooms.is_empty() {
            let (x, y) = rooms[0].center();
            gen.set_tile(x, y, TileType::StairsUp);
            Some((x, y))
        } else {
            None
        };

        // Stairs down in last room (or a random room that's not the first)
        let stairs_down_pos = if rooms.len() >= 2 {
            let room_idx = rooms.len() - 1;
            let (x, y) = rooms[room_idx].center();
            gen.set_tile(x, y, TileType::StairsDown);
            Some((x, y))
        } else if rooms.len() == 1 {
            // Only one room - place stairs in a corner
            let room = &rooms[0];
            let x = room.x + 1;
            let y = room.y + 1;
            gen.set_tile(x, y, TileType::StairsDown);
            Some((x, y))
        } else {
            None
        };

        // Collect chest spawn positions (center of each room except first and last)
        // First room has player spawn (and maybe stairs up on deeper floors)
        // Last room has stairs down
        // Shop rooms: place chest in corner (not center, where vendor stands)
        // A cavern's centre is usually solid rock, so the nominal spot is
        // snapped to the nearest walkable tile in the room.
        let chest_positions: Vec<(i32, i32)> = themed_rooms.iter()
            .enumerate()
            .filter(|(i, _)| *i != 0 && *i != themed_rooms.len() - 1)
            .filter_map(|(_, room)| {
                let target = if room.theme == RoomTheme::Shop {
                    // Place chest in top-left corner area (offset from wall)
                    (room.rect.x + 1, room.rect.y + 1)
                } else {
                    room.rect.center()
                };
                gen.nearest_walkable_in(&room.rect, target)
            })
            .collect();

        // Generate brazier positions in room corners (skip starting room)
        let brazier_positions = gen.generate_brazier_positions(&rooms, &mut rng);

        // Generate coffin positions in Crypt rooms
        let coffin_positions = gen.generate_coffin_positions(&themed_rooms, &mut rng);

        // Generate barrel positions in Storage rooms.
        //
        // Fed the tiles already claimed by chests, braziers and coffins so a
        // Storage room's barrels do not double-book a tile with the room's
        // chest. Barrel placement already retries, so avoiding a taken tile
        // costs a retry rather than a barrel. `TileOccupancy` would refuse the
        // second placement at spawn time regardless; this keeps the two
        // position lists from disagreeing in the first place.
        let mut occupied: Vec<(i32, i32)> = Vec::new();
        occupied.extend(chest_positions.iter().copied());
        occupied.extend(brazier_positions.iter().copied());
        occupied.extend(coffin_positions.iter().copied());
        let barrel_positions =
            gen.generate_barrel_positions(&themed_rooms, &occupied, &mut rng);

        // Generate shop positions
        let shop_position = gen.generate_shop_position(&themed_rooms);
        let shop_decor_positions = gen.generate_shop_decor_positions(&themed_rooms, &mut rng);

        // Occasionally seal one small side-room behind a secret door.
        // Must happen after door positions are known; the sealed doorway is
        // removed from the normal door list.
        let mut door_positions = door_positions;
        let (secret_room, secret_door_pos) = gen.select_secret_room(
            &themed_rooms,
            &mut door_positions,
            stairs_up_pos,
            stairs_down_pos,
            &mut rng,
        );

        // Room furniture (fountains/altars/shrines): roughly one per 3 rooms.
        // Avoid tiles already claimed by chests, braziers, coffins, etc.
        // `occupied` already holds the chest, brazier and coffin tiles from the
        // barrel pass above.
        occupied.extend(barrel_positions.iter().copied());
        occupied.extend(shop_decor_positions.iter().copied());
        occupied.extend(shop_position.iter().copied());
        occupied.extend(gen.water_positions.iter().copied());
        let furniture_positions =
            gen.generate_furniture_positions(&themed_rooms, &occupied, &mut rng);

        // Hidden floor traps (1-3, +1 per 2 floors deeper) on open floor away
        // from stairs, doors, the player start, and claimed tiles.
        occupied.extend(furniture_positions.iter().copied());
        occupied.extend(door_positions.iter().map(|(pos, _)| *pos));
        occupied.extend(secret_door_pos.iter().copied());
        let trap_positions = gen.generate_trap_positions(
            &themed_rooms,
            &occupied,
            stairs_up_pos,
            stairs_down_pos,
            floor_num,
            &mut rng,
        );

        // Cave features: blocking stalagmites plus the glowing fungus and
        // crystals that light a cavern. Placed after the traps so nothing
        // shares a tile with them.
        occupied.extend(trap_positions.iter().copied());
        let cavern_features = gen.generate_cavern_features(
            &themed_rooms,
            &occupied,
            stairs_up_pos,
            stairs_down_pos,
            &mut rng,
        );

        // Starting room is the first room (where player spawns)
        let starting_room = rooms.first().copied();

        // Convert void areas (walls not adjacent to walkable tiles) to empty
        gen.convert_void_to_empty();

        // Set wall orientations based on neighbors and room themes
        gen.set_wall_orientations(&themed_rooms, &mut rng);

        // Extract water positions before moving tiles
        let water_positions = gen.water_positions;

        DungeonResult {
            tiles: gen.tiles,
            chest_positions,
            door_positions,
            brazier_positions,
            decals,
            stairs_up_pos,
            stairs_down_pos,
            starting_room,
            themed_rooms,
            water_positions,
            coffin_positions,
            barrel_positions,
            shop_position,
            shop_decor_positions,
            trap_positions,
            furniture_positions,
            secret_room,
            secret_door_pos,
            stalagmite_positions: cavern_features.stalagmites,
            mushroom_positions: cavern_features.mushrooms,
            crystal_positions: cavern_features.crystals,
        }
    }

    fn get_index(&self, x: i32, y: i32) -> Option<usize> {
        if x < 0 || y < 0 || x >= self.width as i32 || y >= self.height as i32 {
            return None;
        }
        Some(y as usize * self.width + x as usize)
    }

    fn set_tile(&mut self, x: i32, y: i32, tile_type: TileType) {
        if let Some(idx) = self.get_index(x, y) {
            let mut tile = Tile::new(tile_type);
            // Randomly vary floor tiles for visual interest
            if tile_type == TileType::Floor {
                let variant = self
                    .floor_variant_rng
                    .gen_range(0..tile_ids::FLOOR_VARIANTS.len());
                tile.sprite_override = Some(tile_ids::FLOOR_VARIANTS[variant]);
            }
            self.tiles[idx] = tile;
        }
    }

    /// Convert walls that aren't adjacent to any walkable tile into empty space.
    /// This turns the "void" areas outside the dungeon into blank tiles.
    fn convert_void_to_empty(&mut self) {
        let width = self.width as i32;
        let height = self.height as i32;

        // Collect indices to change (can't mutate while iterating)
        let mut to_empty = Vec::new();

        for y in 0..height {
            for x in 0..width {
                let idx = y as usize * self.width + x as usize;
                if self.tiles[idx].tile_type != TileType::Wall {
                    continue;
                }

                // Check if this wall is adjacent to any walkable tile
                let mut adjacent_to_walkable = false;
                for (dx, dy) in [(-1, 0), (1, 0), (0, -1), (0, 1), (-1, -1), (1, -1), (-1, 1), (1, 1)] {
                    let nx = x + dx;
                    let ny = y + dy;
                    if let Some(nidx) = self.get_index(nx, ny) {
                        if self.tiles[nidx].tile_type.is_walkable() {
                            adjacent_to_walkable = true;
                            break;
                        }
                    }
                }

                if !adjacent_to_walkable {
                    to_empty.push(idx);
                }
            }
        }

        // Convert void walls to empty
        for idx in to_empty {
            self.tiles[idx] = Tile::new(TileType::Empty);
        }
    }

    /// Set wall sprite overrides based on orientation and room theme.
    /// Walls adjacent to floor tiles on north/south get the "top" sprite (horizontal edge).
    /// Walls adjacent to floor tiles on east/west get the "side" sprite (vertical edge).
    /// Walls are themed based on the room they're adjacent to (Overgrown -> rough stone, Crypt -> skull walls).
    /// Works per tile on neighbour walkability, so the irregular outline of a
    /// Cavern needs no special handling beyond its sprite choice.
    fn set_wall_orientations(&mut self, themed_rooms: &[ThemedRoom], rng: &mut impl Rng) {
        let width = self.width as i32;
        let height = self.height as i32;

        // First pass: collect wall orientation data
        let mut overrides: Vec<(usize, (crate::tile::SpriteSheet, u32))> = Vec::new();

        for y in 0..height {
            for x in 0..width {
                let idx = y as usize * self.width + x as usize;
                if self.tiles[idx].tile_type != TileType::Wall {
                    continue;
                }

                // Check neighbors for walkable tiles and their positions
                let neighbors = [
                    (0, -1, y > 0),              // north
                    (0, 1, y < height - 1),      // south
                    (1, 0, x < width - 1),       // east
                    (-1, 0, x > 0),              // west
                ];

                let mut north_walkable = false;
                let mut south_walkable = false;
                let mut east_walkable = false;
                let mut west_walkable = false;
                let mut adjacent_theme: Option<RoomTheme> = None;

                for (dx, dy, in_bounds) in neighbors {
                    if !in_bounds {
                        continue;
                    }
                    let nx = x + dx;
                    let ny = y + dy;
                    let nidx = ny as usize * self.width + nx as usize;
                    let is_walkable = self.tiles[nidx].tile_type.is_walkable();

                    if is_walkable {
                        // Track which direction is walkable
                        match (dx, dy) {
                            (0, -1) => north_walkable = true,
                            (0, 1) => south_walkable = true,
                            (1, 0) => east_walkable = true,
                            (-1, 0) => west_walkable = true,
                            _ => {}
                        }

                        // Find theme of adjacent room (if any)
                        if adjacent_theme.is_none() {
                            for room in themed_rooms {
                                if room.rect.contains(nx, ny) {
                                    adjacent_theme = Some(room.theme);
                                    break;
                                }
                            }
                        }
                    }
                }

                // Determine sprite based on adjacent walkable tiles
                // Vertical walls (floor to east/west) use "top" sprite
                // Horizontal walls (floor to north/south) use "side" sprite (default)
                let has_horizontal_neighbor = east_walkable || west_walkable;

                // Check all diagonal neighbors for corner detection
                let ne_walkable = if x < width - 1 && y > 0 {
                    self.tiles[(y - 1) as usize * self.width + (x + 1) as usize].tile_type.is_walkable()
                } else {
                    false
                };
                let nw_walkable = if x > 0 && y > 0 {
                    self.tiles[(y - 1) as usize * self.width + (x - 1) as usize].tile_type.is_walkable()
                } else {
                    false
                };
                let _se_walkable = if x < width - 1 && y < height - 1 {
                    self.tiles[(y + 1) as usize * self.width + (x + 1) as usize].tile_type.is_walkable()
                } else {
                    false
                };
                let _sw_walkable = if x > 0 && y < height - 1 {
                    self.tiles[(y + 1) as usize * self.width + (x - 1) as usize].tile_type.is_walkable()
                } else {
                    false
                };

                // Use WALL_TOP variant for:
                // 1. Pure vertical walls (floor only to east/west)
                // 2. Top corners with direct horizontal floor neighbor
                // 3. Top corner vertices: no cardinal floor neighbors, but floor diagonally to NE or NW
                //    (top of room = higher Y, floor is at lower Y = north direction)
                let is_vertical_wall = has_horizontal_neighbor && !north_walkable && !south_walkable;
                let is_top_corner_direct = south_walkable && has_horizontal_neighbor && !north_walkable;
                let is_top_corner_vertex = !north_walkable && !south_walkable && !east_walkable && !west_walkable
                    && (ne_walkable || nw_walkable);
                let use_top_sprite = is_vertical_wall || is_top_corner_direct || is_top_corner_vertex;

                // Select themed wall sprites
                let sprite = match adjacent_theme {
                    Some(RoomTheme::Overgrown) => {
                        if use_top_sprite {
                            tile_ids::WALL_ROUGH_TOP
                        } else {
                            tile_ids::WALL_ROUGH
                        }
                    }
                    Some(RoomTheme::Crypt) => {
                        if use_top_sprite {
                            tile_ids::WALL_CRYPT_TOP
                        } else {
                            tile_ids::WALL_CRYPT
                        }
                    }
                    Some(RoomTheme::Cavern) => {
                        // Warm brown dirt, with the odd ore vein showing
                        // through. The ore is decoration -- there is no
                        // mining action.
                        if rng.gen_bool(CAVERN_ORE_WALL_CHANCE) {
                            tile_ids::CAVE_ORE_WALL
                        } else if use_top_sprite {
                            tile_ids::WALL_DIRT_TOP
                        } else {
                            tile_ids::WALL_DIRT
                        }
                    }
                    _ => {
                        // Normal, Flooded, Storage, or no adjacent room - use default walls
                        if use_top_sprite {
                            tile_ids::WALL_TOP
                        } else {
                            continue; // No override needed for default wall (side)
                        }
                    }
                };

                overrides.push((idx, sprite));
            }
        }

        // Apply overrides
        for (idx, sprite) in overrides {
            self.tiles[idx].sprite_override = Some(sprite);
        }
    }

    fn carve_room(&mut self, room: &Rect) {
        for y in room.y..room.y + room.height {
            for x in room.x..room.x + room.width {
                self.set_tile(x, y, TileType::Floor);
            }
        }
    }

    /// Carve a room with terrain based on its theme
    fn carve_themed_room(&mut self, room: &ThemedRoom, rng: &mut impl Rng) {
        // Caverns are the one theme that isn't a rectangle: the automaton
        // decides which cells inside the rect are rock, so they skip the
        // blanket carve entirely.
        if room.theme == RoomTheme::Cavern {
            self.carve_cavern(&room.rect, rng);
            return;
        }

        // First, carve the room as floor
        self.carve_room(&room.rect);

        // Then apply theme-specific terrain
        match room.theme {
            RoomTheme::Normal => self.add_cover_scatter(room, rng),
            RoomTheme::Overgrown => self.add_grass_patches(room, rng),
            RoomTheme::Flooded => self.add_water_pools(room, rng),
            RoomTheme::Crypt => self.add_cover_scatter(room, rng), // standard floor + a little cover
            RoomTheme::Storage => self.add_cover_scatter(room, rng), // barrels added separately
            RoomTheme::Shop => self.add_shop_floor(room, rng),
            RoomTheme::Cavern => unreachable!("cavern rooms are carved by carve_cavern"),
        }
    }

    /// Scatter a little tall-grass cover through an otherwise plain room so there
    /// is always something to break line of sight or hide in. Kept sparse (a few
    /// small tufts) so the room still reads as a stone room, not overgrown.
    fn add_cover_scatter(&mut self, room: &ThemedRoom, rng: &mut impl Rng) {
        let area = room.rect.width * room.rect.height;
        let clumps = (area / 30).max(1); // scales gently with room size
        for _ in 0..clumps {
            let cx = rng.gen_range(room.rect.x..room.rect.x + room.rect.width);
            let cy = rng.gen_range(room.rect.y..room.rect.y + room.rect.height);
            for _ in 0..rng.gen_range(2..=4) {
                let x = (cx + rng.gen_range(-1..=1)).clamp(room.rect.x, room.rect.x + room.rect.width - 1);
                let y = (cy + rng.gen_range(-1..=1)).clamp(room.rect.y, room.rect.y + room.rect.height - 1);
                if let Some(idx) = self.get_index(x, y) {
                    if self.tiles[idx].tile_type == TileType::Floor {
                        self.set_tile(x, y, TileType::TallGrass);
                    }
                }
            }
        }
    }

    /// Add red stone floor to shop room
    fn add_shop_floor(&mut self, room: &ThemedRoom, rng: &mut impl Rng) {
        for y in room.rect.y..room.rect.y + room.rect.height {
            for x in room.rect.x..room.rect.x + room.rect.width {
                if let Some(idx) = self.get_index(x, y) {
                    if self.tiles[idx].tile_type == TileType::Floor {
                        let variant = rng.gen_range(0..tile_ids::FLOOR_SHOP_VARIANTS.len());
                        self.tiles[idx].sprite_override = Some(tile_ids::FLOOR_SHOP_VARIANTS[variant]);
                    }
                }
            }
        }
    }

    /// Add grass patches to an overgrown room
    fn add_grass_patches(&mut self, room: &ThemedRoom, rng: &mut impl Rng) {
        let area = room.rect.width * room.rect.height;
        let grass_count = (area as f32 * 0.35) as i32; // ~35% coverage

        for _ in 0..grass_count {
            let x = rng.gen_range(room.rect.x..room.rect.x + room.rect.width);
            let y = rng.gen_range(room.rect.y..room.rect.y + room.rect.height);

            // Use TallGrass for most, regular Grass for variety
            let grass_type = if rng.gen_bool(0.7) {
                TileType::TallGrass
            } else {
                TileType::Grass
            };
            self.set_tile(x, y, grass_type);
        }

        // Change remaining floor tiles to use grass variants instead of stone
        for y in room.rect.y..room.rect.y + room.rect.height {
            for x in room.rect.x..room.rect.x + room.rect.width {
                if let Some(idx) = self.get_index(x, y) {
                    if self.tiles[idx].tile_type == TileType::Floor {
                        let variant = rng.gen_range(0..tile_ids::GRASS_VARIANTS.len());
                        self.tiles[idx].sprite_override = Some(tile_ids::GRASS_VARIANTS[variant]);
                    }
                }
            }
        }
    }

    /// Add water pools to a flooded room
    fn add_water_pools(&mut self, room: &ThemedRoom, rng: &mut impl Rng) {
        // Create 1-3 pools per room
        let pool_count = rng.gen_range(1..=3);

        for _ in 0..pool_count {
            // Ensure we have enough room for a pool
            if room.rect.width < 4 || room.rect.height < 4 {
                continue;
            }

            // Random center point (with margin from edges)
            let cx = rng.gen_range(room.rect.x + 1..room.rect.x + room.rect.width - 1);
            let cy = rng.gen_range(room.rect.y + 1..room.rect.y + room.rect.height - 1);
            let radius = rng.gen_range(1..=2);

            // Fill rough circle with water (keep floor tile, track position for animated water entity)
            for dy in -radius..=radius {
                for dx in -radius..=radius {
                    // Rough circle shape with some randomness
                    if dx * dx + dy * dy <= radius * radius + rng.gen_range(0..=1) {
                        let x = cx + dx;
                        let y = cy + dy;
                        if room.rect.contains(x, y) {
                            // Keep floor tile but track water position
                            self.water_positions.push((x, y));
                        }
                    }
                }
            }
        }
    }

    // =========================================================================
    // CAVERNS
    // =========================================================================

    /// Floor with the cave rubble sprite. The override doubles as the marker
    /// that a tile was opened by the cave automaton rather than punched
    /// through later by a corridor (see `roughen_cavern_intrusions`).
    fn set_cave_floor(&mut self, x: i32, y: i32) {
        self.set_tile(x, y, TileType::Floor);
        if let Some(idx) = self.get_index(x, y) {
            self.tiles[idx].sprite_override = Some(tile_ids::CAVE_RUBBLE_FLOOR);
        }
    }

    fn is_cave_floor(&self, x: i32, y: i32) -> bool {
        self.get_index(x, y)
            .map(|idx| {
                self.tiles[idx].tile_type == TileType::Floor
                    && self.tiles[idx].sprite_override == Some(tile_ids::CAVE_RUBBLE_FLOOR)
            })
            .unwrap_or(false)
    }

    /// Carve a cave into an arbitrary region with cellular automata:
    ///
    /// 1. seed every cell as wall with probability `CAVERN_FILL_CHANCE`;
    /// 2. smooth `CAVERN_SMOOTH_PASSES` times, a cell becoming wall when at
    ///    least `CAVERN_WALL_NEIGHBOURS` of its 8 neighbours are wall;
    /// 3. keep only the largest open region and fill the rest.
    ///
    /// Cells outside the region count as wall while smoothing, which pulls the
    /// cave in from the edges and gives it an organic outline inside the rect.
    /// Step 4 -- joining the cave to the corridors that reach it -- can only
    /// run once those corridors exist; see `connect_cavern_interiors`.
    ///
    /// Takes a region rather than a `ThemedRoom` so the same carve can later
    /// drive whole-floor cave generation.
    fn carve_cavern(&mut self, region: &Rect, rng: &mut impl Rng) {
        if region.width < CAVERN_MIN_DIM || region.height < CAVERN_MIN_DIM {
            // Below this the automaton leaves little more than rubble, so the
            // room is carved plain and themed by its floor alone.
            for y in region.y..region.y + region.height {
                for x in region.x..region.x + region.width {
                    self.set_cave_floor(x, y);
                }
            }
            return;
        }

        let w = region.width as usize;
        let h = region.height as usize;
        let area = w * h;

        // A single roll can land anywhere from "sealed solid" to "plain
        // rectangle", so roll until the cave is worth having. Rolls come from
        // the same seeded rng, so the retries stay reproducible.
        let mut carved: Option<Vec<bool>> = None;
        for _ in 0..CAVERN_CARVE_ATTEMPTS {
            // 1. Seed.
            let mut wall: Vec<bool> =
                (0..area).map(|_| rng.gen_bool(CAVERN_FILL_CHANCE)).collect();

            // 2. Smooth.
            for _ in 0..CAVERN_SMOOTH_PASSES {
                let mut next = vec![false; area];
                for y in 0..h {
                    for x in 0..w {
                        let mut walls = 0;
                        for dy in -1..=1i32 {
                            for dx in -1..=1i32 {
                                // Neighbours are clamped to the region rather
                                // than treated as solid rock outside it. A
                                // forced wall border swamps a room-sized grid
                                // -- it erodes the cave to a rounded blob or
                                // seals it outright -- so the automaton runs
                                // as if the region were a window onto a larger
                                // cave system, which is what leaves the
                                // irregular outline and interior rock.
                                let nx = (x as i32 + dx).clamp(0, w as i32 - 1) as usize;
                                let ny = (y as i32 + dy).clamp(0, h as i32 - 1) as usize;
                                if wall[ny * w + nx] {
                                    walls += 1;
                                }
                            }
                        }
                        next[y * w + x] = walls >= CAVERN_WALL_NEIGHBOURS;
                    }
                }
                wall = next;
            }

            // 3. Keep the largest open region; smaller pockets become rock.
            let Some(main) = largest_open_region(&wall, w, h) else {
                continue; // the roll sealed the room solid
            };
            let open = main.len() as f32 / area as f32;
            if !(CAVERN_MIN_OPEN_FRACTION..=CAVERN_MAX_OPEN_FRACTION).contains(&open) {
                continue; // too cramped to use, or so open it reads as a room
            }
            for (i, cell) in wall.iter_mut().enumerate() {
                *cell = !main.contains(&i);
            }
            carved = Some(wall);
            break;
        }

        // Every roll came out unusable (well under 1% of rooms): carve plain
        // rather than hand back a sealed or near-sealed room.
        let wall = carved.unwrap_or_else(|| vec![false; area]);

        // Write the result into the map.
        for y in 0..h {
            for x in 0..w {
                let (mx, my) = (region.x + x as i32, region.y + y as i32);
                if wall[y * w + x] {
                    self.set_tile(mx, my, TileType::Wall);
                } else {
                    self.set_cave_floor(mx, my);
                }
            }
        }
    }

    /// Step 4 of cave carving, run once the room-to-room corridors exist.
    ///
    /// `find_door_positions` looks for corridor entrances in the band just
    /// outside a room's rect and assumes the tile inside the rect is floor.
    /// For a cavern it often isn't, which would orphan the room; and the
    /// corridor pass itself aims at the room's centre, cutting a straight
    /// channel through the rock. This roughs those channels up, opens every
    /// perimeter entrance, and joins every pocket of cave to the main body.
    fn connect_cavern_interiors(&mut self, themed_rooms: &[ThemedRoom], rng: &mut impl Rng) {
        for room in themed_rooms.iter().filter(|r| r.theme == RoomTheme::Cavern) {
            self.roughen_cavern_intrusions(&room.rect, rng);
            self.open_cavern_entrances(&room.rect);
            self.join_cavern_regions(&room.rect, rng);
        }
    }

    /// Turn the ruler-straight corridor channels that `connect_bsp` punched
    /// through a cavern into something cave-shaped: relay them as cave floor
    /// and nibble outwards at random.
    fn roughen_cavern_intrusions(&mut self, rect: &Rect, rng: &mut impl Rng) {
        let intrusions: Vec<(i32, i32)> = (rect.y..rect.y + rect.height)
            .flat_map(|y| (rect.x..rect.x + rect.width).map(move |x| (x, y)))
            .filter(|&(x, y)| {
                self.get_tile(x, y) == Some(TileType::Floor) && !self.is_cave_floor(x, y)
            })
            .collect();

        for (x, y) in intrusions {
            self.set_cave_floor(x, y);
            if rng.gen_bool(CAVERN_INTRUSION_ROUGHEN_CHANCE) {
                let (dx, dy) = [(-1, 0), (1, 0), (0, -1), (0, 1)][rng.gen_range(0..4)];
                if rect.contains(x + dx, y + dy) {
                    self.set_cave_floor(x + dx, y + dy);
                }
            }
        }
    }

    /// Make sure every corridor entrance on the perimeter has open cave on the
    /// other side of it, so `find_door_positions` and the player both get in.
    fn open_cavern_entrances(&mut self, rect: &Rect) {
        let walkable = |g: &Self, x: i32, y: i32| {
            g.get_tile(x, y).map(|t| t.is_walkable()).unwrap_or(false)
        };
        let mut entries: Vec<(i32, i32)> = Vec::new();
        for x in rect.x..rect.x + rect.width {
            if walkable(self, x, rect.y - 1) {
                entries.push((x, rect.y));
            }
            if walkable(self, x, rect.y + rect.height) {
                entries.push((x, rect.y + rect.height - 1));
            }
        }
        for y in rect.y..rect.y + rect.height {
            if walkable(self, rect.x - 1, y) {
                entries.push((rect.x, y));
            }
            if walkable(self, rect.x + rect.width, y) {
                entries.push((rect.x + rect.width - 1, y));
            }
        }
        for (x, y) in entries {
            if !walkable(self, x, y) {
                self.set_cave_floor(x, y);
            }
        }
    }

    /// Join every walkable pocket inside the rect to the largest one with a
    /// wandering channel, so no part of a cavern -- entrance stubs included --
    /// is sealed off from the rest.
    fn join_cavern_regions(&mut self, rect: &Rect, rng: &mut impl Rng) {
        let mut regions = self.walkable_regions_in(rect);
        if regions.len() < 2 {
            return;
        }
        // Largest first; ties broken on the scan-order seed tile so the
        // choice is reproducible for a given seed.
        regions.sort_by_key(|r| (std::cmp::Reverse(r.len()), r[0]));
        let main = regions.remove(0);

        for region in regions {
            // Closest pair between this pocket and the main body; ties go to
            // the first in scan order, so the choice stays reproducible.
            let nearest = region
                .iter()
                .flat_map(|&a| main.iter().map(move |&b| (a, b)))
                .min_by_key(|&(a, b)| (a.0 - b.0).abs() + (a.1 - b.1).abs());
            if let Some((from, to)) = nearest {
                self.carve_cave_channel(from, to, rect, rng);
            }
        }
    }

    /// 4-connected regions of walkable tiles inside `rect`, in scan order.
    fn walkable_regions_in(&self, rect: &Rect) -> Vec<Vec<(i32, i32)>> {
        use std::collections::{HashSet, VecDeque};

        let mut seen: HashSet<(i32, i32)> = HashSet::new();
        let mut regions: Vec<Vec<(i32, i32)>> = Vec::new();
        let walkable = |x: i32, y: i32| {
            rect.contains(x, y) && self.get_tile(x, y).map(|t| t.is_walkable()).unwrap_or(false)
        };

        for y in rect.y..rect.y + rect.height {
            for x in rect.x..rect.x + rect.width {
                if !walkable(x, y) || seen.contains(&(x, y)) {
                    continue;
                }
                let mut region = Vec::new();
                let mut queue = VecDeque::new();
                queue.push_back((x, y));
                seen.insert((x, y));
                while let Some((cx, cy)) = queue.pop_front() {
                    region.push((cx, cy));
                    for (dx, dy) in [(-1, 0), (1, 0), (0, -1), (0, 1)] {
                        let next = (cx + dx, cy + dy);
                        if walkable(next.0, next.1) && seen.insert(next) {
                            queue.push_back(next);
                        }
                    }
                }
                regions.push(region);
            }
        }
        regions
    }

    /// Carve a wandering, sometimes two-tiles-wide channel from `from` to
    /// `to`, staying inside `rect`. A straight one-tile channel reads as
    /// obviously artificial next to organic cave walls, so each step has a
    /// chance of drifting sideways instead of advancing.
    fn carve_cave_channel(
        &mut self,
        from: (i32, i32),
        to: (i32, i32),
        rect: &Rect,
        rng: &mut impl Rng,
    ) {
        let (mut x, mut y) = from;
        // Generous bound: the walk advances most steps, and the jitter can
        // only wander within the rect.
        let max_steps = (rect.width + rect.height) * 4;

        for _ in 0..max_steps {
            self.set_cave_floor(x, y);
            if rng.gen_bool(CAVERN_PATH_WIDEN_CHANCE) {
                let (dx, dy) = [(-1, 0), (1, 0), (0, -1), (0, 1)][rng.gen_range(0..4)];
                if rect.contains(x + dx, y + dy) {
                    self.set_cave_floor(x + dx, y + dy);
                }
            }
            if (x, y) == to {
                return;
            }

            let (dx, dy) = ((to.0 - x).signum(), (to.1 - y).signum());
            if dx != 0 && dy != 0 {
                // Diagonal run: pick an axis. Both make progress.
                if rng.gen_bool(0.5) {
                    x += dx;
                } else {
                    y += dy;
                }
            } else {
                // Straight run: occasionally step off-axis instead.
                let (step_x, step_y) = if rng.gen_bool(CAVERN_PATH_JITTER_CHANCE) {
                    let drift = if rng.gen_bool(0.5) { 1 } else { -1 };
                    if dx != 0 { (0, drift) } else { (drift, 0) }
                } else {
                    (dx, dy)
                };
                if rect.contains(x + step_x, y + step_y) {
                    x += step_x;
                    y += step_y;
                } else {
                    x += dx;
                    y += dy;
                }
            }
        }
        self.set_cave_floor(to.0, to.1);
    }

    /// The walkable tile in `rect` closest to `target`, or None if the room
    /// holds no walkable tile at all. A cavern isn't a rectangle, so its
    /// centre -- the nominal chest spot -- is often solid rock.
    fn nearest_walkable_in(&self, rect: &Rect, target: (i32, i32)) -> Option<(i32, i32)> {
        let mut best: Option<((i32, i32), i32)> = None;
        for y in rect.y..rect.y + rect.height {
            for x in rect.x..rect.x + rect.width {
                if !self.get_tile(x, y).map(|t| t.is_walkable()).unwrap_or(false) {
                    continue;
                }
                let d = (x - target.0).abs() + (y - target.1).abs();
                if best.map(|(_, bd)| d < bd).unwrap_or(true) {
                    best = Some(((x, y), d));
                }
            }
        }
        best.map(|(pos, _)| pos)
    }

    /// Pick the cave dressing for every Cavern room: blocking stalagmites
    /// (cover you can shoot over but not walk through) plus the glowing
    /// mushrooms and crystal clusters that light the place.
    ///
    /// Stalagmites block movement and nothing can ever clear one, so each is
    /// only accepted if it leaves every tile that was reachable before any of
    /// them went in still reachable. Checking only that the two staircases
    /// stay connected is far too weak: a cave is full of one-tile chokepoints
    /// off the critical path, and plugging one strands whole rooms (and their
    /// chests) behind it.
    fn generate_cavern_features(
        &self,
        themed_rooms: &[ThemedRoom],
        occupied: &[(i32, i32)],
        stairs_up: Option<(i32, i32)>,
        stairs_down: Option<(i32, i32)>,
        rng: &mut impl Rng,
    ) -> CavernFeatures {
        let mut features = CavernFeatures {
            stalagmites: Vec::new(),
            mushrooms: Vec::new(),
            crystals: Vec::new(),
        };

        // Floor 0 has no up staircase; the player starts in the first room.
        let start = stairs_up.or_else(|| themed_rooms.first().map(|r| r.rect.center()));
        // Everything the player could walk to before any stalagmite went in.
        let open_before = start.map(|s| self.reachable_from(s, &[]));

        for room in themed_rooms.iter().filter(|r| r.theme == RoomTheme::Cavern) {
            let mut pool: Vec<(i32, i32)> = (room.rect.y..room.rect.y + room.rect.height)
                .flat_map(|y| (room.rect.x..room.rect.x + room.rect.width).map(move |x| (x, y)))
                .filter(|&(x, y)| self.is_cave_floor(x, y))
                .filter(|p| !occupied.contains(p))
                .filter(|&p| Some(p) != stairs_up && Some(p) != stairs_down)
                .collect();

            // Scale with the open area so a cramped cave doesn't end up
            // wall-to-wall stalagmites.
            let stalagmite_count = (pool.len() / CAVERN_TILES_PER_STALAGMITE)
                .clamp(CAVERN_STALAGMITES_MIN, CAVERN_STALAGMITES_MAX);
            for _ in 0..stalagmite_count {
                if pool.is_empty() {
                    break;
                }
                let idx = rng.gen_range(0..pool.len());
                let candidate = pool[idx];
                let mut blocked = features.stalagmites.clone();
                blocked.push(candidate);
                let safe = match (start, &open_before) {
                    (Some(s), Some(before)) => {
                        let after = self.reachable_from(s, &blocked);
                        before
                            .iter()
                            .all(|tile| after.contains(tile) || blocked.contains(tile))
                    }
                    _ => true,
                };
                pool.swap_remove(idx);
                if safe {
                    features.stalagmites.push(candidate);
                }
            }

            // Lights are walkable decals, so they just need a free tile.
            let mushroom_count = rng.gen_range(CAVERN_MUSHROOMS_MIN..=CAVERN_MUSHROOMS_MAX);
            for _ in 0..mushroom_count {
                if pool.is_empty() {
                    break;
                }
                let idx = rng.gen_range(0..pool.len());
                features.mushrooms.push(pool.swap_remove(idx));
            }
            let crystal_count = rng.gen_range(CAVERN_CRYSTALS_MIN..=CAVERN_CRYSTALS_MAX);
            for _ in 0..crystal_count {
                if pool.is_empty() {
                    break;
                }
                let idx = rng.gen_range(0..pool.len());
                features.crystals.push(pool.swap_remove(idx));
            }
        }

        features
    }

    /// Connect rooms by traversing the BSP tree and linking sibling subtrees.
    fn connect_bsp(&mut self, node: &BspNode, rng: &mut impl Rng) {
        if node.is_leaf() {
            return;
        }

        // Recursively connect children first
        if let Some(ref left) = node.left {
            self.connect_bsp(left, rng);
        }
        if let Some(ref right) = node.right {
            self.connect_bsp(right, rng);
        }

        // Connect a room from the left subtree to a room from the right subtree
        // But only if they're not already connected (avoids duplicate hallways)
        if let (Some(ref left), Some(ref right)) = (&node.left, &node.right) {
            if let (Some(left_room), Some(right_room)) = (left.get_room(), right.get_room()) {
                if !self.rooms_are_connected(&left_room, &right_room) {
                    self.connect_rooms(&left_room, &right_room, rng);
                }
            }
        }
    }

    /// Check if two rooms are already connected via walkable tiles (flood fill).
    fn rooms_are_connected(&self, room1: &Rect, room2: &Rect) -> bool {
        use std::collections::{HashSet, VecDeque};

        let start = room1.center();

        // BFS from room1's center to see if we can reach room2's center
        let mut visited = HashSet::new();
        let mut queue = VecDeque::new();
        queue.push_back(start);
        visited.insert(start);

        while let Some((x, y)) = queue.pop_front() {
            // Check if we reached room2
            if room2.contains(x, y) {
                return true;
            }

            // Explore neighbors
            for (dx, dy) in [(-1, 0), (1, 0), (0, -1), (0, 1)] {
                let nx = x + dx;
                let ny = y + dy;

                if visited.contains(&(nx, ny)) {
                    continue;
                }

                if let Some(tile_type) = self.get_tile(nx, ny) {
                    if tile_type.is_walkable() {
                        visited.insert((nx, ny));
                        queue.push_back((nx, ny));
                    }
                }
            }
        }

        false
    }

    /// Connect two rooms with an L-shaped corridor.
    fn connect_rooms(&mut self, room1: &Rect, room2: &Rect, rng: &mut impl Rng) {
        let (x1, y1) = room1.center();
        let (x2, y2) = room2.center();

        // Randomly choose to go horizontal-then-vertical or vertical-then-horizontal
        if rng.gen_bool(0.5) {
            self.create_h_corridor(x1, x2, y1);
            self.create_v_corridor(y1, y2, x2);
        } else {
            self.create_v_corridor(y1, y2, x1);
            self.create_h_corridor(x1, x2, y2);
        }
    }

    fn create_h_corridor(&mut self, x1: i32, x2: i32, y: i32) {
        let start = x1.min(x2);
        let end = x1.max(x2);

        for x in start..=end {
            self.set_tile(x, y, TileType::Floor);
        }
    }

    fn create_v_corridor(&mut self, y1: i32, y2: i32, x: i32) {
        let start = y1.min(y2);
        let end = y1.max(y2);

        for y in start..=end {
            self.set_tile(x, y, TileType::Floor);
        }
    }

    fn get_tile(&self, x: i32, y: i32) -> Option<TileType> {
        self.get_index(x, y).map(|idx| self.tiles[idx].tile_type)
    }

    /// Find positions where doors should be placed.
    /// A door candidate is a floor tile adjacent to walls on two opposite sides
    /// (indicating a doorway/chokepoint).
    fn find_door_positions(&self, themed_rooms: &[ThemedRoom]) -> Vec<((i32, i32), RoomTheme)> {
        let mut door_candidates: Vec<((i32, i32), RoomTheme)> = Vec::new();

        for themed_room in themed_rooms {
            let room = &themed_room.rect;
            let theme = themed_room.theme;

            // Check just outside each edge of the room for corridor entrances

            // Top edge (y = room.y - 1)
            let y = room.y - 1;
            for x in room.x..room.x + room.width {
                if self.is_door_candidate(x, y) {
                    door_candidates.push(((x, y), theme));
                }
            }

            // Bottom edge (y = room.y + room.height)
            let y = room.y + room.height;
            for x in room.x..room.x + room.width {
                if self.is_door_candidate(x, y) {
                    door_candidates.push(((x, y), theme));
                }
            }

            // Left edge (x = room.x - 1)
            let x = room.x - 1;
            for y in room.y..room.y + room.height {
                if self.is_door_candidate(x, y) {
                    door_candidates.push(((x, y), theme));
                }
            }

            // Right edge (x = room.x + room.width)
            let x = room.x + room.width;
            for y in room.y..room.y + room.height {
                if self.is_door_candidate(x, y) {
                    door_candidates.push(((x, y), theme));
                }
            }
        }

        // Filter out adjacent doors (keep only one from each cluster)
        self.filter_adjacent_doors_themed(door_candidates)
    }

    /// Filter out doors that are adjacent to other doors.
    /// When multiple doors are next to each other, keep only one.
    #[allow(dead_code)]
    fn filter_adjacent_doors(&self, candidates: Vec<(i32, i32)>) -> Vec<(i32, i32)> {
        use std::collections::HashSet;

        let candidate_set: HashSet<(i32, i32)> = candidates.iter().copied().collect();
        let mut removed: HashSet<(i32, i32)> = HashSet::new();
        let mut result = Vec::new();

        for &(x, y) in &candidates {
            if removed.contains(&(x, y)) {
                continue;
            }

            // Check adjacent tiles and mark any door candidates as removed
            for (dx, dy) in [(-1, 0), (1, 0), (0, -1), (0, 1)] {
                let neighbor = (x + dx, y + dy);
                if candidate_set.contains(&neighbor) && !removed.contains(&neighbor) && neighbor != (x, y) {
                    // Mark the neighbor as removed (we keep the current one)
                    removed.insert(neighbor);
                }
            }

            result.push((x, y));
        }

        result
    }

    /// Filter out doors that are adjacent to other doors (themed version).
    fn filter_adjacent_doors_themed(&self, candidates: Vec<((i32, i32), RoomTheme)>) -> Vec<((i32, i32), RoomTheme)> {
        use std::collections::HashSet;

        let candidate_positions: HashSet<(i32, i32)> = candidates.iter().map(|(pos, _)| *pos).collect();
        let mut removed: HashSet<(i32, i32)> = HashSet::new();
        let mut result = Vec::new();

        for &((x, y), theme) in &candidates {
            if removed.contains(&(x, y)) {
                continue;
            }

            // Check adjacent tiles and mark any door candidates as removed
            for (dx, dy) in [(-1, 0), (1, 0), (0, -1), (0, 1)] {
                let neighbor = (x + dx, y + dy);
                if candidate_positions.contains(&neighbor) && !removed.contains(&neighbor) && neighbor != (x, y) {
                    removed.insert(neighbor);
                }
            }

            result.push(((x, y), theme));
        }

        result
    }

    /// Generate decorative decals in rooms with theme-appropriate types
    fn generate_themed_decals(&self, rooms: &[ThemedRoom], rng: &mut impl Rng) -> Vec<Decal> {
        use crate::tile::SpriteSheet;
        let mut decals = Vec::new();

        // Decal type: ((SpriteSheet, tile_id), weight)
        type DecalType = ((SpriteSheet, u32), u32);

        // Normal room decals (bones, rocks, etc.)
        let normal_decals: Vec<DecalType> = vec![
            (tile_ids::BONES_1, 3),
            (tile_ids::BONES_2, 2),
            (tile_ids::BONES_3, 2),
            (tile_ids::BONES_4, 1),
            (tile_ids::ROCKS, 4),
            (tile_ids::ROCKS_2, 3),
            (tile_ids::SKULL, 1),
            (tile_ids::MUSHROOM, 2),
            (tile_ids::FLOWERS, 2),
            (tile_ids::PLANT, 3),
        ];

        // Overgrown room decals (more plants and mushrooms)
        let overgrown_decals: Vec<DecalType> = vec![
            (tile_ids::PLANT, 5),
            (tile_ids::PLANT_FLAX, 2),
            (tile_ids::PLANT_PAPYRUS, 2),
            (tile_ids::PLANT_RICE, 2),
            (tile_ids::PLANT_CORN, 2),
            (tile_ids::MUSHROOM, 4),
            (tile_ids::MUSHROOM_LARGE, 2),
            (tile_ids::FLOWERS, 4),
            (tile_ids::BONES_1, 1),
            (tile_ids::ROCKS, 2),
        ];

        // Flooded room decals (sparse, rocks and slime)
        let flooded_decals: Vec<DecalType> = vec![
            (tile_ids::ROCKS, 4),
            (tile_ids::BONES_1, 2),
            (tile_ids::SKULL, 1),
            (tile_ids::SLIME_SMALL, 2),
            (tile_ids::SLIME_LARGE, 1),
        ];

        // Crypt room decals (bones, skulls, blood)
        let crypt_decals: Vec<DecalType> = vec![
            (tile_ids::BONES_1, 3),
            (tile_ids::BONES_2, 3),
            (tile_ids::BONES_3, 2),
            (tile_ids::BONES_4, 2),
            (tile_ids::SKULL, 3),
            (tile_ids::BLOOD_1, 2),
            (tile_ids::BLOOD_2, 2),
        ];

        // Storage room decals (minimal - contents are the barrels)
        let storage_decals: Vec<DecalType> = vec![
            (tile_ids::ROCKS, 2),
            (tile_ids::BONES_1, 1),
        ];

        // Shop room decals (minimal - vendor and containers are the focus)
        let shop_decals: Vec<DecalType> = vec![
            (tile_ids::ROCKS, 1),
        ];

        // Cavern decals (rockfall and fungus; the glowing kind are entities)
        let cavern_decals: Vec<DecalType> = vec![
            (tile_ids::ROCKS, 5),
            (tile_ids::ROCKS_2, 4),
            (tile_ids::MUSHROOM, 3),
            (tile_ids::MUSHROOM_LARGE, 2),
            (tile_ids::BONES_1, 1),
            (tile_ids::BONES_3, 1),
        ];

        for room in rooms {
            let decal_types = match room.theme {
                RoomTheme::Normal => &normal_decals,
                RoomTheme::Overgrown => &overgrown_decals,
                RoomTheme::Flooded => &flooded_decals,
                RoomTheme::Crypt => &crypt_decals,
                RoomTheme::Storage => &storage_decals,
                RoomTheme::Shop => &shop_decals,
                RoomTheme::Cavern => &cavern_decals,
            };
            let total_weight: u32 = decal_types.iter().map(|(_, w)| w).sum();

            // Flooded, Storage, and Shop rooms get fewer decals
            let density_divisor = match room.theme {
                RoomTheme::Flooded => 20,
                RoomTheme::Storage => 25,
                RoomTheme::Shop => 30, // Minimal decals for shop
                _ => 12,
            };

            let area = room.rect.width * room.rect.height;
            let num_decals = rng.gen_range(area / (density_divisor + 5)..=area / density_divisor).max(1);

            for _ in 0..num_decals {
                // Random position within the room (avoid edges)
                let x = if room.rect.width > 2 {
                    rng.gen_range(room.rect.x + 1..room.rect.x + room.rect.width - 1)
                } else {
                    room.rect.x + room.rect.width / 2
                };
                let y = if room.rect.height > 2 {
                    rng.gen_range(room.rect.y + 1..room.rect.y + room.rect.height - 1)
                } else {
                    room.rect.y + room.rect.height / 2
                };

                // Skip anything that isn't open ground. (A no-op for the
                // rectangular themes, where every tile in the rect is floor;
                // a cavern's rect is mostly rock.)
                if !self.get_tile(x, y).map(|t| t.is_walkable()).unwrap_or(false) {
                    continue;
                }
                // Skip if this tile is water
                if let Some(idx) = self.get_index(x, y) {
                    if self.tiles[idx].tile_type == TileType::Water {
                        continue;
                    }
                }

                // Pick a random decal type using weights
                let roll = rng.gen_range(0..total_weight);
                let mut cumulative = 0;
                let mut sprite_ref = tile_ids::ROCKS;
                for (sprite, weight) in decal_types {
                    cumulative += weight;
                    if roll < cumulative {
                        sprite_ref = *sprite;
                        break;
                    }
                }

                decals.push(Decal {
                    x,
                    y,
                    sheet: sprite_ref.0,
                    tile_id: sprite_ref.1,
                });
            }

            // Stalactites hang from a cave's ceiling. The sprite is drawn at
            // the top of its tile with nothing below it, so it goes down as a
            // decal over a wall (decals render above the tile) rather than as
            // a wall sprite of its own -- and only over walls whose southern
            // face is the one you actually see from inside the cave.
            if room.theme == RoomTheme::Cavern {
                let rect = room.rect;
                for y in rect.y - 1..rect.y + rect.height + 1 {
                    for x in rect.x - 1..rect.x + rect.width + 1 {
                        if self.get_tile(x, y) != Some(TileType::Wall) {
                            continue;
                        }
                        let below_is_cave = self
                            .get_tile(x, y + 1)
                            .map(|t| t.is_walkable())
                            .unwrap_or(false)
                            && rect.contains(x, y + 1);
                        if below_is_cave && rng.gen_bool(CAVERN_STALACTITE_CHANCE) {
                            decals.push(Decal {
                                x,
                                y,
                                sheet: tile_ids::CAVE_STALACTITES.0,
                                tile_id: tile_ids::CAVE_STALACTITES.1,
                            });
                        }
                    }
                }
            }
        }

        decals
    }

    /// Generate brazier positions in rooms.
    /// Places braziers in corners of larger rooms (not the starting room).
    fn generate_brazier_positions(&self, rooms: &[Rect], rng: &mut impl Rng) -> Vec<(i32, i32)> {
        let mut positions = Vec::new();

        for (i, room) in rooms.iter().enumerate() {
            // Skip the starting room (index 0) - it has the campfire
            if i == 0 {
                continue;
            }

            // Only place braziers in rooms large enough (at least 5x5)
            if room.width < 5 || room.height < 5 {
                continue;
            }

            // ~40% chance to have braziers in a room
            if !rng.gen_bool(0.4) {
                continue;
            }

            // Try to place 1-2 braziers in corners
            let corners = [
                (room.x + 1, room.y + 1),                           // top-left
                (room.x + room.width - 2, room.y + 1),              // top-right
                (room.x + 1, room.y + room.height - 2),             // bottom-left
                (room.x + room.width - 2, room.y + room.height - 2), // bottom-right
            ];

            // Pick 1-2 random corners
            let num_braziers = rng.gen_range(1..=2);
            let mut used_corners: Vec<(i32, i32)> = Vec::new();

            for _ in 0..num_braziers {
                // Find an unused corner that's a valid floor tile
                let available: Vec<_> = corners.iter()
                    .filter(|c| !used_corners.contains(c))
                    .filter(|&&(x, y)| {
                        self.get_tile(x, y) == Some(TileType::Floor)
                    })
                    .copied()
                    .collect();

                if !available.is_empty() {
                    let corner = available[rng.gen_range(0..available.len())];
                    positions.push(corner);
                    used_corners.push(corner);
                }
            }
        }

        positions
    }

    /// Generate coffin positions in Crypt rooms
    fn generate_coffin_positions(&self, themed_rooms: &[ThemedRoom], rng: &mut impl Rng) -> Vec<(i32, i32)> {
        let mut positions = Vec::new();

        for room in themed_rooms {
            if room.theme != RoomTheme::Crypt {
                continue;
            }

            // Only place coffins in rooms large enough (at least 5x5)
            if room.rect.width < 5 || room.rect.height < 5 {
                continue;
            }

            // Place 2-4 coffins per crypt room
            let num_coffins = rng.gen_range(2..=4);

            // Try corners and edges for placement
            let potential_spots = [
                (room.rect.x + 1, room.rect.y + 1),
                (room.rect.x + room.rect.width - 2, room.rect.y + 1),
                (room.rect.x + 1, room.rect.y + room.rect.height - 2),
                (room.rect.x + room.rect.width - 2, room.rect.y + room.rect.height - 2),
                // Mid-edges
                (room.rect.x + room.rect.width / 2, room.rect.y + 1),
                (room.rect.x + room.rect.width / 2, room.rect.y + room.rect.height - 2),
            ];

            let mut used: Vec<(i32, i32)> = Vec::new();
            for _ in 0..num_coffins {
                let available: Vec<_> = potential_spots.iter()
                    .filter(|c| !used.contains(c))
                    .filter(|&&(x, y)| self.get_tile(x, y) == Some(TileType::Floor))
                    .copied()
                    .collect();

                if !available.is_empty() {
                    let spot = available[rng.gen_range(0..available.len())];
                    positions.push(spot);
                    used.push(spot);
                }
            }
        }

        positions
    }

    /// Generate barrel positions in Storage rooms
    fn generate_barrel_positions(
        &self,
        themed_rooms: &[ThemedRoom],
        occupied: &[(i32, i32)],
        rng: &mut impl Rng,
    ) -> Vec<(i32, i32)> {
        let mut positions = Vec::new();

        for room in themed_rooms {
            if room.theme != RoomTheme::Storage {
                continue;
            }

            // Only place barrels in rooms large enough
            if room.rect.width < 4 || room.rect.height < 4 {
                continue;
            }

            // Place 3-6 barrels per storage room
            let num_barrels = rng.gen_range(3..=6);
            let mut used: Vec<(i32, i32)> = Vec::new();

            for _ in 0..num_barrels {
                // Try random positions within the room
                for _ in 0..10 {
                    let x = rng.gen_range(room.rect.x + 1..room.rect.x + room.rect.width - 1);
                    let y = rng.gen_range(room.rect.y + 1..room.rect.y + room.rect.height - 1);

                    if !used.contains(&(x, y))
                        && !occupied.contains(&(x, y))
                        && self.get_tile(x, y) == Some(TileType::Floor)
                    {
                        positions.push((x, y));
                        used.push((x, y));
                        break;
                    }
                }
            }
        }

        positions
    }

    /// Generate shop vendor spawn position (center of shop room)
    fn generate_shop_position(&self, themed_rooms: &[ThemedRoom]) -> Option<(i32, i32)> {
        for room in themed_rooms {
            if room.theme == RoomTheme::Shop {
                return Some(room.rect.center());
            }
        }
        None
    }

    /// Generate shop decoration positions (jars, sacks around edges)
    fn generate_shop_decor_positions(&self, themed_rooms: &[ThemedRoom], rng: &mut impl Rng) -> Vec<(i32, i32)> {
        let mut positions = Vec::new();

        for room in themed_rooms {
            if room.theme != RoomTheme::Shop {
                continue;
            }

            // Only place decorations in rooms large enough
            if room.rect.width < 4 || room.rect.height < 4 {
                continue;
            }

            // Place 3-5 decorations around edges (avoiding center where vendor is)
            // Also avoid top-left corner where chest is placed
            let num_decor = rng.gen_range(3..=5);
            let (center_x, center_y) = room.rect.center();
            let chest_pos = (room.rect.x + 1, room.rect.y + 1);
            let mut used: Vec<(i32, i32)> = vec![chest_pos]; // Reserve chest spot

            // Potential spots: corners (except top-left for chest) and edge midpoints
            let potential_spots = [
                (room.rect.x + room.rect.width - 2, room.rect.y + 1),
                (room.rect.x + 1, room.rect.y + room.rect.height - 2),
                (room.rect.x + room.rect.width - 2, room.rect.y + room.rect.height - 2),
                (room.rect.x + 1, center_y),
                (room.rect.x + room.rect.width - 2, center_y),
            ];

            for _ in 0..num_decor {
                let available: Vec<_> = potential_spots.iter()
                    .filter(|c| !used.contains(c))
                    .filter(|&&(x, y)| self.get_tile(x, y) == Some(TileType::Floor))
                    .filter(|&&(x, y)| x != center_x || y != center_y) // Avoid center
                    .copied()
                    .collect();

                if !available.is_empty() {
                    let spot = available[rng.gen_range(0..available.len())];
                    positions.push(spot);
                    used.push(spot);
                }
            }
        }

        positions
    }

    /// Pick positions for room furniture (fountain/altar/shrine): roughly one
    /// per `FURNITURE_ROOMS_PER_PIECE` rooms, never in Shop rooms or the
    /// starting room, on open floor away from the room center (chest spot).
    fn generate_furniture_positions(
        &self,
        themed_rooms: &[ThemedRoom],
        occupied: &[(i32, i32)],
        rng: &mut impl Rng,
    ) -> Vec<(i32, i32)> {
        let mut positions = Vec::new();

        // Eligible rooms: not the starting room (index 0), not Shop, and big
        // enough to hold furniture without choking the walkway.
        let eligible: Vec<&ThemedRoom> = themed_rooms
            .iter()
            .enumerate()
            .filter(|(i, room)| {
                *i != 0
                    && room.theme != RoomTheme::Shop
                    && room.rect.width >= 4
                    && room.rect.height >= 4
            })
            .map(|(_, room)| room)
            .collect();

        if eligible.is_empty() {
            return positions;
        }

        let count = (themed_rooms.len() / FURNITURE_ROOMS_PER_PIECE).max(1);

        // Shuffle eligible rooms, take the first `count`.
        let mut pool: Vec<&ThemedRoom> = eligible;
        for i in (1..pool.len()).rev() {
            let j = rng.gen_range(0..=i);
            pool.swap(i, j);
        }

        for room in pool.into_iter().take(count) {
            let center = room.rect.center();
            // Try a handful of interior tiles.
            for _ in 0..12 {
                let x = rng.gen_range(room.rect.x + 1..room.rect.x + room.rect.width - 1);
                let y = rng.gen_range(room.rect.y + 1..room.rect.y + room.rect.height - 1);
                if (x, y) == center {
                    continue; // chest / stairs spot
                }
                if occupied.contains(&(x, y)) || positions.contains(&(x, y)) {
                    continue;
                }
                if self.get_tile(x, y) == Some(TileType::Floor) {
                    positions.push((x, y));
                    break;
                }
            }
        }

        positions
    }

    /// Pick positions for hidden floor traps: 1-3 per floor plus one per two
    /// floors of depth, on room/corridor floor tiles away from stairs and the
    /// starting room, skipping doors and already-claimed tiles.
    fn generate_trap_positions(
        &self,
        themed_rooms: &[ThemedRoom],
        occupied: &[(i32, i32)],
        stairs_up: Option<(i32, i32)>,
        stairs_down: Option<(i32, i32)>,
        floor_num: u32,
        rng: &mut impl Rng,
    ) -> Vec<(i32, i32)> {
        let count =
            rng.gen_range(TRAPS_PER_FLOOR_MIN..=TRAPS_PER_FLOOR_MAX) + floor_num / TRAP_EXTRA_PER_FLOORS;

        let starting_room = themed_rooms.first().map(|r| r.rect);
        let shop_rooms: Vec<Rect> = themed_rooms
            .iter()
            .filter(|r| r.theme == RoomTheme::Shop)
            .map(|r| r.rect)
            .collect();

        let near_stairs = |x: i32, y: i32| {
            [stairs_up, stairs_down].iter().flatten().any(|&(sx, sy)| {
                (x - sx).abs().max((y - sy).abs()) < TRAP_MIN_DIST_FROM_STAIRS
            })
        };

        let mut candidates: Vec<(i32, i32)> = Vec::new();
        for y in 0..self.height as i32 {
            for x in 0..self.width as i32 {
                if self.get_tile(x, y) != Some(TileType::Floor) {
                    continue;
                }
                if near_stairs(x, y)
                    || occupied.contains(&(x, y))
                    || starting_room.map(|r| r.contains(x, y)).unwrap_or(false)
                    || shop_rooms.iter().any(|r| r.contains(x, y))
                {
                    continue;
                }
                candidates.push((x, y));
            }
        }

        let mut positions = Vec::new();
        for _ in 0..count {
            if candidates.is_empty() {
                break;
            }
            let idx = rng.gen_range(0..candidates.len());
            positions.push(candidates.swap_remove(idx));
        }
        positions
    }

    /// Walkable openings on a room's one-tile perimeter ring: ring tiles that
    /// are walkable and cardinally adjacent to a walkable tile inside the room.
    fn room_openings(&self, room: &Rect) -> Vec<(i32, i32)> {
        let mut openings = Vec::new();
        let mut check = |x: i32, y: i32| {
            if self.get_tile(x, y).map(|t| t.is_walkable()).unwrap_or(false) {
                // Cardinal neighbor inside the room must be walkable too.
                for (dx, dy) in [(-1, 0), (1, 0), (0, -1), (0, 1)] {
                    let (nx, ny) = (x + dx, y + dy);
                    if room.contains(nx, ny)
                        && self.get_tile(nx, ny).map(|t| t.is_walkable()).unwrap_or(false)
                    {
                        openings.push((x, y));
                        return;
                    }
                }
            }
        };
        for x in room.x..room.x + room.width {
            check(x, room.y - 1);
            check(x, room.y + room.height);
        }
        for y in room.y..room.y + room.height {
            check(room.x - 1, y);
            check(room.x + room.width, y);
        }
        openings
    }

    /// BFS reachability over walkable tiles with one tile treated as blocked.
    fn is_reachable_without(&self, start: (i32, i32), goal: (i32, i32), blocked: (i32, i32)) -> bool {
        start == goal || self.reachable_from(start, &[blocked]).contains(&goal)
    }

    /// Every walkable tile reachable from `start`, treating `blocked` as
    /// solid. 4-connected, matching the game's pathfinding.
    fn reachable_from(
        &self,
        start: (i32, i32),
        blocked: &[(i32, i32)],
    ) -> std::collections::HashSet<(i32, i32)> {
        use std::collections::{HashSet, VecDeque};

        let mut visited: HashSet<(i32, i32)> = HashSet::new();
        let mut queue = VecDeque::new();
        queue.push_back(start);
        visited.insert(start);
        while let Some((x, y)) = queue.pop_front() {
            for (dx, dy) in [(-1, 0), (1, 0), (0, -1), (0, 1)] {
                let next = (x + dx, y + dy);
                if blocked.contains(&next) || visited.contains(&next) {
                    continue;
                }
                if self.get_tile(next.0, next.1).map(|t| t.is_walkable()).unwrap_or(false) {
                    visited.insert(next);
                    queue.push_back(next);
                }
            }
        }
        visited
    }

    /// Occasionally (35% of floors) seal one small side-room behind a secret
    /// door. Only rooms with exactly one walkable opening — which must also be
    /// a door candidate — are eligible, and sealing must keep both staircases
    /// reachable from each other. The sealed doorway is removed from the
    /// normal door list. Returns (room, doorway).
    fn select_secret_room(
        &self,
        themed_rooms: &[ThemedRoom],
        door_positions: &mut Vec<((i32, i32), RoomTheme)>,
        stairs_up: Option<(i32, i32)>,
        stairs_down: Option<(i32, i32)>,
        rng: &mut impl Rng,
    ) -> (Option<Rect>, Option<(i32, i32)>) {
        if themed_rooms.len() < 3 || !rng.gen_bool(SECRET_ROOM_CHANCE) {
            return (None, None);
        }

        let last_idx = themed_rooms.len() - 1;
        let door_set: Vec<(i32, i32)> = door_positions.iter().map(|(pos, _)| *pos).collect();

        let mut candidates: Vec<(Rect, (i32, i32))> = Vec::new();
        for (i, room) in themed_rooms.iter().enumerate() {
            // Never the starting room, the stairs-down room, or the shop.
            if i == 0 || i == last_idx || room.theme == RoomTheme::Shop {
                continue;
            }
            if room.rect.width * room.rect.height > SECRET_ROOM_MAX_AREA {
                continue;
            }
            let openings = self.room_openings(&room.rect);
            let [doorway] = openings.as_slice() else {
                continue; // needs exactly one way in
            };
            if !door_set.contains(doorway) {
                continue; // the opening must be a proper doorway chokepoint
            }
            // Sealing must not cut the path between staircases (belt and
            // braces — a one-opening room can't be on the critical path, but
            // verify anyway).
            let reachable = match (stairs_up, stairs_down) {
                (Some(up), Some(down)) => self.is_reachable_without(up, down, *doorway),
                _ => true,
            };
            if reachable {
                candidates.push((room.rect, *doorway));
            }
        }

        if candidates.is_empty() {
            return (None, None);
        }
        let (room, doorway) = candidates[rng.gen_range(0..candidates.len())];
        door_positions.retain(|(pos, _)| *pos != doorway);
        (Some(room), Some(doorway))
    }

    /// Check if a tile is a good door candidate:
    /// - Must be a floor tile
    /// - Must have walls on two opposite sides (horizontal or vertical)
    fn is_door_candidate(&self, x: i32, y: i32) -> bool {
        let Some(tile_type) = self.get_tile(x, y) else {
            return false;
        };

        if tile_type != TileType::Floor {
            return false;
        }

        let north = self.get_tile(x, y - 1).unwrap_or(TileType::Wall);
        let south = self.get_tile(x, y + 1).unwrap_or(TileType::Wall);
        let east = self.get_tile(x + 1, y).unwrap_or(TileType::Wall);
        let west = self.get_tile(x - 1, y).unwrap_or(TileType::Wall);

        let is_wall = |t: TileType| t == TileType::Wall;

        // Horizontal doorway: walls north and south, open east and west
        let h_doorway = is_wall(north) && is_wall(south) && !is_wall(east) && !is_wall(west);
        // Vertical doorway: walls east and west, open north and south
        let v_doorway = is_wall(east) && is_wall(west) && !is_wall(north) && !is_wall(south);

        h_doorway || v_doorway
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::{HashSet, VecDeque};

    #[test]
    fn test_rect_center() {
        let rect = Rect::new(0, 0, 10, 10);
        assert_eq!(rect.center(), (5, 5));

        let rect2 = Rect::new(5, 5, 4, 6);
        assert_eq!(rect2.center(), (7, 8));
    }

    #[test]
    fn test_dungeon_generates_tiles() {
        let result = DungeonGenerator::generate(50, 50, 0);
        assert_eq!(result.tiles.len(), 50 * 50);
    }

    #[test]
    fn test_dungeon_has_floor_tiles() {
        let result = DungeonGenerator::generate(50, 50, 0);
        let floor_count = result.tiles.iter().filter(|t| t.tile_type == TileType::Floor).count();
        // Should have at least some floor tiles
        assert!(floor_count > 0);
    }

    #[test]
    fn test_dungeon_has_wall_tiles() {
        let result = DungeonGenerator::generate(50, 50, 0);
        let wall_count = result.tiles.iter().filter(|t| t.tile_type == TileType::Wall).count();
        // Should have some walls
        assert!(wall_count > 0);
    }

    #[test]
    fn test_dungeon_generates_chest_positions() {
        let result = DungeonGenerator::generate(50, 50, 0);
        // Chests are placed in rooms except first (player spawn) and last (stairs down)
        // With a 50x50 dungeon we should have at least 3 rooms, so at least 1 chest
        // But this can vary based on BSP randomness, so just check it doesn't crash
        // and positions are valid if any exist
        for (x, y) in &result.chest_positions {
            assert!(*x >= 0 && *x < 50);
            assert!(*y >= 0 && *y < 50);
        }
    }

    #[test]
    fn test_dungeon_generates_door_positions() {
        let result = DungeonGenerator::generate(50, 50, 0);
        // Should have some doors
        assert!(!result.door_positions.is_empty());
    }

    #[test]
    fn test_chest_positions_are_on_floor() {
        let result = DungeonGenerator::generate(50, 50, 0);
        for (x, y) in result.chest_positions {
            let idx = y as usize * 50 + x as usize;
            assert_eq!(result.tiles[idx].tile_type, TileType::Floor);
        }
    }

    #[test]
    fn test_door_positions_are_on_floor() {
        let result = DungeonGenerator::generate(50, 50, 0);
        for ((x, y), _theme) in result.door_positions {
            let idx = y as usize * 50 + x as usize;
            assert_eq!(result.tiles[idx].tile_type, TileType::Floor);
        }
    }

    #[test]
    fn test_floor_0_has_stairs_down_no_stairs_up() {
        let result = DungeonGenerator::generate(50, 50, 0);
        assert!(result.stairs_down_pos.is_some());
        assert!(result.stairs_up_pos.is_none());
    }

    #[test]
    fn test_floor_1_has_both_stairs() {
        let result = DungeonGenerator::generate(50, 50, 1);
        assert!(result.stairs_down_pos.is_some());
        assert!(result.stairs_up_pos.is_some());
    }

    #[test]
    fn test_stairs_are_on_stair_tiles() {
        let result = DungeonGenerator::generate(50, 50, 1);
        if let Some((x, y)) = result.stairs_up_pos {
            let idx = y as usize * 50 + x as usize;
            assert_eq!(result.tiles[idx].tile_type, TileType::StairsUp);
        }
        if let Some((x, y)) = result.stairs_down_pos {
            let idx = y as usize * 50 + x as usize;
            assert_eq!(result.tiles[idx].tile_type, TileType::StairsDown);
        }
    }

    #[test]
    fn test_traps_avoid_stairs_and_player_start() {
        // Deeper floor so there are guaranteed traps and both staircases.
        for _ in 0..10 {
            let result = DungeonGenerator::generate(50, 50, 4);
            assert!(!result.trap_positions.is_empty(), "deep floors always roll traps");
            let start = result.starting_room.expect("has a starting room");
            for &(x, y) in &result.trap_positions {
                // On plain floor.
                let idx = y as usize * 50 + x as usize;
                assert_eq!(result.tiles[idx].tile_type, TileType::Floor);
                // Never inside the starting room.
                assert!(!start.contains(x, y), "trap inside starting room at ({x},{y})");
                // Never near either staircase.
                for stairs in [result.stairs_up_pos, result.stairs_down_pos].iter().flatten() {
                    let dist = (x - stairs.0).abs().max((y - stairs.1).abs());
                    assert!(
                        dist >= TRAP_MIN_DIST_FROM_STAIRS,
                        "trap at ({x},{y}) too close to stairs at {stairs:?}"
                    );
                }
            }
        }
    }

    #[test]
    fn test_secret_room_sealed_only_with_single_doorway() {
        // Run generation many times; whenever a secret room appears, its
        // doorway must be the room's only opening, it must not be in the
        // normal door list, and both staircases must stay mutually reachable.
        let mut found = 0;
        for _ in 0..60 {
            let result = DungeonGenerator::generate(50, 50, 1);
            let (Some(room), Some(doorway)) = (result.secret_room, result.secret_door_pos) else {
                continue;
            };
            found += 1;

            // Rebuild a generator view over the produced tiles to reuse helpers.
            let gen = DungeonGenerator {
                width: 50,
                height: 50,
                tiles: result.tiles.clone(),
                water_positions: vec![],
                floor_variant_rng: StdRng::seed_from_u64(0),
            };
            let openings = gen.room_openings(&room);
            assert_eq!(openings, vec![doorway], "secret room must have exactly one opening");

            // The sealed doorway spawns no normal door.
            assert!(
                !result.door_positions.iter().any(|(pos, _)| *pos == doorway),
                "sealed doorway must be removed from the door list"
            );

            // Sealing keeps the stairs connected.
            let (up, down) = (
                result.stairs_up_pos.expect("floor 1 has stairs up"),
                result.stairs_down_pos.expect("has stairs down"),
            );
            assert!(
                gen.is_reachable_without(up, down, doorway),
                "stairs must remain mutually reachable with the doorway sealed"
            );
        }
        assert!(found > 0, "60 floors at 35% should produce at least one secret room");
    }

    #[test]
    fn test_furniture_positions_on_floor_outside_shop() {
        for _ in 0..10 {
            let result = DungeonGenerator::generate(50, 50, 0);
            let shop_rooms: Vec<Rect> = result
                .themed_rooms
                .iter()
                .filter(|r| r.theme == RoomTheme::Shop)
                .map(|r| r.rect)
                .collect();
            for &(x, y) in &result.furniture_positions {
                let idx = y as usize * 50 + x as usize;
                assert_eq!(result.tiles[idx].tile_type, TileType::Floor);
                assert!(
                    !shop_rooms.iter().any(|r| r.contains(x, y)),
                    "furniture must never spawn in shop rooms"
                );
                assert!(
                    !result.chest_positions.contains(&(x, y)),
                    "furniture must not share a tile with a chest"
                );
            }
        }
    }

    // ---- Caverns --------------------------------------------------------

    /// Generate a floor from a fixed seed, so cavern assertions are stable.
    fn seeded_floor(seed: u64, floor: u32) -> DungeonResult {
        use rand::rngs::StdRng;
        use rand::SeedableRng;
        let mut rng = StdRng::seed_from_u64(seed);
        DungeonGenerator::generate_with_rng(40, 40, floor, &mut rng)
    }

    fn caverns(result: &DungeonResult) -> Vec<Rect> {
        result
            .themed_rooms
            .iter()
            .filter(|r| r.theme == RoomTheme::Cavern)
            .map(|r| r.rect)
            .collect()
    }

    fn walkable_at(result: &DungeonResult, x: i32, y: i32) -> bool {
        if x < 0 || y < 0 || x >= 40 || y >= 40 {
            return false;
        }
        result.tiles[y as usize * 40 + x as usize].tile_type.is_walkable()
    }

    /// Tiles reachable from `start` over walkable terrain, optionally treating
    /// some tiles as solid.
    fn flood(result: &DungeonResult, start: (i32, i32), blocked: &[(i32, i32)]) -> HashSet<(i32, i32)> {
        let mut seen = HashSet::new();
        let mut queue = VecDeque::new();
        queue.push_back(start);
        seen.insert(start);
        while let Some((x, y)) = queue.pop_front() {
            for (dx, dy) in [(-1, 0), (1, 0), (0, -1), (0, 1)] {
                let next = (x + dx, y + dy);
                if seen.contains(&next) || blocked.contains(&next) {
                    continue;
                }
                if walkable_at(result, next.0, next.1) {
                    seen.insert(next);
                    queue.push_back(next);
                }
            }
        }
        seen
    }

    fn cavern_tiles(result: &DungeonResult, rect: &Rect) -> Vec<(i32, i32)> {
        (rect.y..rect.y + rect.height)
            .flat_map(|y| (rect.x..rect.x + rect.width).map(move |x| (x, y)))
            .filter(|&(x, y)| walkable_at(result, x, y))
            .collect()
    }

    #[test]
    fn test_cavern_is_carved_not_rectangular() {
        // A cavern is the one theme whose rect isn't carved wholesale, so it
        // must come out with rock inside it -- and still with room to move.
        let mut irregular = 0;
        let mut checked = 0;
        for seed in 0..100u64 {
            let result = seeded_floor(seed, 1);
            for rect in caverns(&result) {
                checked += 1;
                let area = (rect.width * rect.height) as usize;
                let open = cavern_tiles(&result, &rect).len();
                assert!(open * 4 >= area, "cavern {rect:?} is barely open: {open}/{area}");
                if open < area {
                    irregular += 1;
                }
            }
        }
        assert!(checked >= 100, "every floor should have a cavern, got {checked}");
        assert!(
            irregular * 10 >= checked * 9,
            "caverns should almost always have interior rock, got {irregular}/{checked}"
        );
    }

    #[test]
    fn test_every_cavern_entrance_leads_into_the_cave() {
        // The gotcha this guards: `find_door_positions` scans the band just
        // outside a room's rect for corridor entrances and assumes the tile
        // inside is floor. Cellular-automata carving routinely leaves that
        // tile solid, so a corridor dead-ends in rock -- and opening it can
        // leave a stub detached from the cave body. Both happen on roughly
        // one floor in twelve, so this needs a good spread of seeds.
        let mut entrances = 0;
        for seed in 0..200u64 {
            let result = seeded_floor(seed, 1);
            for rect in caverns(&result) {
                let tiles = cavern_tiles(&result, &rect);
                let main = *tiles.first().expect("cavern is solid rock");
                let body = flood(&result, main, &[]);

                let mut ring: Vec<(i32, i32)> = Vec::new();
                for x in rect.x..rect.x + rect.width {
                    ring.push((x, rect.y - 1));
                    ring.push((x, rect.y + rect.height));
                }
                for y in rect.y..rect.y + rect.height {
                    ring.push((rect.x - 1, y));
                    ring.push((rect.x + rect.width, y));
                }

                for (rx, ry) in ring {
                    if !walkable_at(&result, rx, ry) {
                        continue;
                    }
                    entrances += 1;
                    let inside = [(-1, 0), (1, 0), (0, -1), (0, 1)]
                        .iter()
                        .map(|(dx, dy)| (rx + dx, ry + dy))
                        .filter(|&(x, y)| rect.contains(x, y))
                        .collect::<Vec<_>>();
                    assert!(
                        inside.iter().any(|p| walkable_at(&result, p.0, p.1) && body.contains(p)),
                        "seed {seed}: corridor entrance at ({rx}, {ry}) on cavern {rect:?} \
                         does not open into the cave"
                    );
                }
            }
        }
        assert!(entrances >= 200, "expected plenty of cavern entrances, got {entrances}");
    }

    #[test]
    fn test_cavern_is_fully_reachable_from_the_stairs() {
        for seed in 0..200u64 {
            let result = seeded_floor(seed, 1);
            let start = result.stairs_down_pos.expect("floor has stairs down");
            let reachable = flood(&result, start, &[]);
            for rect in caverns(&result) {
                let tiles = cavern_tiles(&result, &rect);
                assert!(!tiles.is_empty(), "seed {seed}: cavern {rect:?} is solid rock");
                let orphans: Vec<_> =
                    tiles.iter().filter(|p| !reachable.contains(p)).collect();
                assert!(
                    orphans.is_empty(),
                    "seed {seed}: cavern {rect:?} has tiles unreachable from the stairs: {orphans:?}"
                );
            }
        }
    }

    #[test]
    fn test_cavern_stalagmites_strand_nothing() {
        // Stalagmites block movement and nothing can clear one, so they must
        // not cut anything off -- not just the path between the staircases.
        // A cave is full of one-tile chokepoints off the critical path:
        // guarding only stairs-to-stairs stranded walkable tiles on a third
        // of floors and a chest on a sixth of them.
        for seed in 0..200u64 {
            let result = seeded_floor(seed, 1);
            let down = result.stairs_down_pos.expect("floor has stairs down");
            let stalagmites: HashSet<(i32, i32)> =
                result.stalagmite_positions.iter().copied().collect();

            let before = flood(&result, down, &[]);
            let after = flood(&result, down, &result.stalagmite_positions);

            let stranded: Vec<_> = before
                .iter()
                .filter(|t| !after.contains(t) && !stalagmites.contains(t))
                .collect();
            assert!(
                stranded.is_empty(),
                "seed {seed}: stalagmites stranded {} tiles, e.g. {:?}",
                stranded.len(),
                stranded.first()
            );

            let up = result.stairs_up_pos.expect("floor 1 has stairs up");
            assert!(after.contains(&up), "seed {seed}: stairs cut off");
            for chest in &result.chest_positions {
                assert!(after.contains(chest), "seed {seed}: chest {chest:?} walled in");
            }
        }
    }

    #[test]
    fn test_nothing_spawns_on_a_non_walkable_cavern_tile() {
        // chest_positions used the room centre with no walkability check at
        // all, which is solid rock in most caverns; the other placement
        // functions are checked here too.
        for seed in 0..200u64 {
            let result = seeded_floor(seed, 1);
            let placements: Vec<(&str, &Vec<(i32, i32)>)> = vec![
                ("chest", &result.chest_positions),
                ("stalagmite", &result.stalagmite_positions),
                ("mushroom", &result.mushroom_positions),
                ("crystal", &result.crystal_positions),
                ("brazier", &result.brazier_positions),
                ("coffin", &result.coffin_positions),
                ("barrel", &result.barrel_positions),
                ("shop decor", &result.shop_decor_positions),
                ("trap", &result.trap_positions),
                ("furniture", &result.furniture_positions),
            ];
            for (what, positions) in placements {
                for &(x, y) in positions {
                    assert!(
                        walkable_at(&result, x, y),
                        "seed {seed}: {what} at ({x}, {y}) is inside a wall"
                    );
                }
            }
            for decal in &result.decals {
                // Stalactites are the deliberate exception: they hang off the
                // cave ceiling, so they go over wall tiles.
                if (decal.sheet, decal.tile_id) == tile_ids::CAVE_STALACTITES {
                    continue;
                }
                assert!(
                    walkable_at(&result, decal.x, decal.y),
                    "seed {seed}: decal at ({}, {}) is inside a wall",
                    decal.x,
                    decal.y
                );
            }
        }
    }

    #[test]
    fn test_chest_spot_snaps_out_of_solid_rock() {
        // `chest_positions` used `room.rect.center()` with no walkability
        // check at all. On the current corridor scheme a cavern's centre
        // happens to be open -- corridors are aimed at room centres, and
        // `roughen_cavern_intrusions` relays that channel as cave floor -- so
        // this exercises the guard on the geometry it exists for rather than
        // waiting for a seed to produce it.
        let mut gen = DungeonGenerator::new(20, 20);
        let rect = Rect::new(4, 4, 9, 9);
        // A ring of cave with solid rock through the middle.
        for y in rect.y..rect.y + rect.height {
            for x in rect.x..rect.x + rect.width {
                let on_edge = x == rect.x
                    || y == rect.y
                    || x == rect.x + rect.width - 1
                    || y == rect.y + rect.height - 1;
                if on_edge {
                    gen.set_cave_floor(x, y);
                }
            }
        }

        let centre = rect.center();
        assert!(
            !gen.get_tile(centre.0, centre.1).map(|t| t.is_walkable()).unwrap_or(false),
            "the test fixture should have a solid centre"
        );

        let spot = gen.nearest_walkable_in(&rect, centre).expect("ring has open tiles");
        assert!(
            gen.get_tile(spot.0, spot.1).map(|t| t.is_walkable()).unwrap_or(false),
            "snapped chest spot must be walkable"
        );
        assert!(rect.contains(spot.0, spot.1), "snapped spot must stay in the room");

        // And a room with nothing open at all yields no chest rather than one
        // buried in the wall.
        let solid = DungeonGenerator::new(20, 20);
        assert_eq!(solid.nearest_walkable_in(&rect, centre), None);
    }

    #[test]
    fn test_cavern_features_are_on_cave_floor_and_distinct() {
        let result = seeded_floor(12345, 2);
        let rects = caverns(&result);
        assert!(!rects.is_empty(), "expected a cavern");

        let features: Vec<(i32, i32)> = result
            .stalagmite_positions
            .iter()
            .chain(result.mushroom_positions.iter())
            .chain(result.crystal_positions.iter())
            .copied()
            .collect();
        assert!(!features.is_empty(), "a cavern should be dressed");

        let unique: HashSet<(i32, i32)> = features.iter().copied().collect();
        assert_eq!(unique.len(), features.len(), "cave features share a tile");

        for &(x, y) in &features {
            assert!(
                rects.iter().any(|r| r.contains(x, y)),
                "cave feature at ({x}, {y}) is outside every cavern"
            );
            assert_eq!(
                result.tiles[y as usize * 40 + x as usize].sprite_override,
                Some(tile_ids::CAVE_RUBBLE_FLOOR),
                "cave feature at ({x}, {y}) is not on cave floor"
            );
        }
    }

    #[test]
    fn test_same_seed_same_cavern() {
        use rand::rngs::StdRng;
        use rand::SeedableRng;

        let build = || {
            let mut rng = StdRng::seed_from_u64(90210);
            let result = DungeonGenerator::generate_with_rng(40, 40, 2, &mut rng);
            let shape: Vec<(i32, i32)> = caverns(&result)
                .iter()
                .flat_map(|rect| cavern_tiles(&result, rect))
                .collect();
            (
                caverns(&result),
                shape,
                result.stalagmite_positions.clone(),
                result.mushroom_positions.clone(),
                result.crystal_positions.clone(),
            )
        };

        let (rects, shape, stalagmites, mushrooms, crystals) = build();
        assert!(!rects.is_empty(), "expected a cavern");
        assert!(!shape.is_empty(), "cavern should have open tiles");
        let (rects2, shape2, stalagmites2, mushrooms2, crystals2) = build();

        assert_eq!(
            rects.iter().map(|r| (r.x, r.y, r.width, r.height)).collect::<Vec<_>>(),
            rects2.iter().map(|r| (r.x, r.y, r.width, r.height)).collect::<Vec<_>>(),
        );
        assert_eq!(shape, shape2, "same seed must carve the same cave");
        assert_eq!(stalagmites, stalagmites2);
        assert_eq!(mushrooms, mushrooms2);
        assert_eq!(crystals, crystals2);
    }

    #[test]
    fn test_cavern_walls_use_the_dirt_sprites() {
        let result = seeded_floor(2024, 1);
        let rects = caverns(&result);
        assert!(!rects.is_empty(), "expected a cavern");

        let cave_wall_sprites = [
            tile_ids::WALL_DIRT,
            tile_ids::WALL_DIRT_TOP,
            tile_ids::CAVE_ORE_WALL,
        ];
        let mut seen = 0;
        for rect in &rects {
            for y in rect.y..rect.y + rect.height {
                for x in rect.x..rect.x + rect.width {
                    let tile = &result.tiles[y as usize * 40 + x as usize];
                    if tile.tile_type != TileType::Wall {
                        continue;
                    }
                    // Only walls facing into the cave get a themed sprite;
                    // rock with no open neighbour keeps the default.
                    let touches_cave = [(-1, 0), (1, 0), (0, -1), (0, 1)]
                        .iter()
                        .any(|(dx, dy)| walkable_at(&result, x + dx, y + dy));
                    if !touches_cave {
                        continue;
                    }
                    let sprite = tile.sprite_override.expect("cave wall needs a sprite");
                    assert!(
                        cave_wall_sprites.contains(&sprite),
                        "wall at ({x}, {y}) inside a cavern uses {sprite:?}, not a dirt wall"
                    );
                    seen += 1;
                }
            }
        }
        assert!(seen > 0, "a cavern should have walls facing into it");
    }

    #[test]
    fn test_bsp_node_is_leaf() {
        let node = BspNode::new(Rect::new(0, 0, 10, 10));
        assert!(node.is_leaf());
    }

    #[test]
    fn test_bsp_split_creates_children() {
        let mut node = BspNode::new(Rect::new(0, 0, 100, 100));
        let mut rng = rand::thread_rng();
        node.split(&mut rng);
        // After splitting, should have children (unless region was too small)
        assert!(!node.is_leaf());
    }

    #[test]
    fn test_bsp_small_node_doesnt_split() {
        let mut node = BspNode::new(Rect::new(0, 0, 5, 5)); // Too small to split
        let mut rng = rand::thread_rng();
        node.split(&mut rng);
        // Should remain a leaf
        assert!(node.is_leaf());
    }
}
