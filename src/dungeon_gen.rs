use crate::constants::*;
use crate::grid::Decal;
use crate::tile::{tile_ids, Tile, TileType};
use rand::Rng;

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
    /// Overgrown room with tall grass patches
    Overgrown,
    /// Flooded room with water pools
    Flooded,
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
    pub door_positions: Vec<(i32, i32)>,
    pub decals: Vec<Decal>,
    pub stairs_up_pos: Option<(i32, i32)>,
    pub stairs_down_pos: Option<(i32, i32)>,
    /// The starting room where the player spawns (for NPC placement and enemy exclusion)
    pub starting_room: Option<Rect>,
}

pub struct DungeonGenerator {
    width: usize,
    height: usize,
    tiles: Vec<Tile>,
}

impl DungeonGenerator {
    pub fn new(width: usize, height: usize) -> Self {
        Self {
            width,
            height,
            tiles: vec![Tile::new(TileType::Wall); width * height],
        }
    }

    /// Generate a dungeon floor. floor_num 0 is the starting floor (no stairs up).
    pub fn generate(width: usize, height: usize, floor_num: u32) -> DungeonResult {
        let mut gen = Self::new(width, height);
        let mut rng = rand::thread_rng();

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
        let themed_rooms: Vec<ThemedRoom> = room_rects
            .iter()
            .enumerate()
            .map(|(i, rect)| {
                // First room is always Normal (player spawn)
                let theme = if i == 0 {
                    RoomTheme::Normal
                } else if rng.gen::<f32>() < THEMED_ROOM_CHANCE {
                    // Randomly pick between Overgrown and Flooded
                    if rng.gen_bool(0.5) {
                        RoomTheme::Overgrown
                    } else {
                        RoomTheme::Flooded
                    }
                } else {
                    RoomTheme::Normal
                };
                ThemedRoom { rect: *rect, theme }
            })
            .collect();

        // Carve all rooms into the tile map (with terrain based on theme)
        for room in &themed_rooms {
            gen.carve_themed_room(room, &mut rng);
        }

        // Get plain room rects for functions that don't need themes
        let rooms: Vec<Rect> = themed_rooms.iter().map(|r| r.rect).collect();

        // Connect sibling rooms by traversing the BSP tree
        gen.connect_bsp(&root, &mut rng);

        // Find door positions (but keep floor tiles - doors are entities)
        let door_positions = gen.find_door_positions(&rooms);

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
        let chest_positions: Vec<(i32, i32)> = rooms.iter()
            .enumerate()
            .filter(|(i, _)| *i != 0 && *i != rooms.len() - 1)
            .map(|(_, room)| room.center())
            .collect();

        // Starting room is the first room (where player spawns)
        let starting_room = rooms.first().copied();

        // Set wall orientations based on neighbors
        gen.set_wall_orientations();

        DungeonResult {
            tiles: gen.tiles,
            chest_positions,
            door_positions,
            decals,
            stairs_up_pos,
            stairs_down_pos,
            starting_room,
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
            self.tiles[idx] = Tile::new(tile_type);
        }
    }

    /// Set wall sprite overrides based on orientation.
    /// Walls adjacent to floor tiles on north/south get the "top" sprite (horizontal edge).
    /// Walls adjacent to floor tiles on east/west get the "side" sprite (vertical edge).
    fn set_wall_orientations(&mut self) {
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

                // Check neighbors for walkable tiles
                let north_walkable = if y > 0 {
                    self.tiles[(y - 1) as usize * self.width + x as usize].tile_type.is_walkable()
                } else {
                    false
                };
                let south_walkable = if y < height - 1 {
                    self.tiles[(y + 1) as usize * self.width + x as usize].tile_type.is_walkable()
                } else {
                    false
                };
                let east_walkable = if x < width - 1 {
                    self.tiles[y as usize * self.width + (x + 1) as usize].tile_type.is_walkable()
                } else {
                    false
                };
                let west_walkable = if x > 0 {
                    self.tiles[y as usize * self.width + (x - 1) as usize].tile_type.is_walkable()
                } else {
                    false
                };

                // Determine sprite based on adjacent walkable tiles
                // Vertical walls (floor to east/west) use "top" sprite
                // Horizontal walls (floor to north/south) use "side" sprite (default)
                // Top corners (floor to south + east/west, but not north) use "top" sprite
                // Bottom corners (floor to north + east/west) use "side" sprite (default)
                let has_horizontal_neighbor = east_walkable || west_walkable;

                // Use WALL_TOP for:
                // 1. Pure vertical walls (floor only to east/west)
                // 2. Top corners (floor to south and east/west, but not north)
                let is_vertical_wall = has_horizontal_neighbor && !north_walkable && !south_walkable;
                let is_top_corner = south_walkable && has_horizontal_neighbor && !north_walkable;

                if is_vertical_wall || is_top_corner {
                    overrides.push((idx, tile_ids::WALL_TOP));
                }
                // Default WALL sprite (side) is used for horizontal edges and bottom corners
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
        // First, carve the room as floor
        self.carve_room(&room.rect);

        // Then apply theme-specific terrain
        match room.theme {
            RoomTheme::Normal => {}
            RoomTheme::Overgrown => self.add_grass_patches(room, rng),
            RoomTheme::Flooded => self.add_water_pools(room, rng),
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

            // Fill rough circle with water
            for dy in -radius..=radius {
                for dx in -radius..=radius {
                    // Rough circle shape with some randomness
                    if dx * dx + dy * dy <= radius * radius + rng.gen_range(0..=1) {
                        let x = cx + dx;
                        let y = cy + dy;
                        if room.rect.contains(x, y) {
                            self.set_tile(x, y, TileType::Water);
                        }
                    }
                }
            }
        }
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
        if let (Some(ref left), Some(ref right)) = (&node.left, &node.right) {
            if let (Some(left_room), Some(right_room)) = (left.get_room(), right.get_room()) {
                self.connect_rooms(&left_room, &right_room, rng);
            }
        }
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
    fn find_door_positions(&self, rooms: &[Rect]) -> Vec<(i32, i32)> {
        let mut door_positions = Vec::new();

        for room in rooms {
            // Check just outside each edge of the room for corridor entrances

            // Top edge (y = room.y - 1)
            let y = room.y - 1;
            for x in room.x..room.x + room.width {
                if self.is_door_candidate(x, y) {
                    door_positions.push((x, y));
                }
            }

            // Bottom edge (y = room.y + room.height)
            let y = room.y + room.height;
            for x in room.x..room.x + room.width {
                if self.is_door_candidate(x, y) {
                    door_positions.push((x, y));
                }
            }

            // Left edge (x = room.x - 1)
            let x = room.x - 1;
            for y in room.y..room.y + room.height {
                if self.is_door_candidate(x, y) {
                    door_positions.push((x, y));
                }
            }

            // Right edge (x = room.x + room.width)
            let x = room.x + room.width;
            for y in room.y..room.y + room.height {
                if self.is_door_candidate(x, y) {
                    door_positions.push((x, y));
                }
            }
        }

        door_positions
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
            (tile_ids::SKULL, 1),
            (tile_ids::MUSHROOM, 2),
            (tile_ids::FLOWERS, 2),
            (tile_ids::PLANT, 3),
        ];

        // Overgrown room decals (more plants and mushrooms)
        let overgrown_decals: Vec<DecalType> = vec![
            (tile_ids::PLANT, 5),
            (tile_ids::MUSHROOM, 4),
            (tile_ids::FLOWERS, 4),
            (tile_ids::BONES_1, 1),
            (tile_ids::ROCKS, 2),
        ];

        // Flooded room decals (sparse, mostly rocks)
        let flooded_decals: Vec<DecalType> = vec![
            (tile_ids::ROCKS, 4),
            (tile_ids::BONES_1, 2),
            (tile_ids::SKULL, 1),
        ];

        for room in rooms {
            let decal_types = match room.theme {
                RoomTheme::Normal => &normal_decals,
                RoomTheme::Overgrown => &overgrown_decals,
                RoomTheme::Flooded => &flooded_decals,
            };
            let total_weight: u32 = decal_types.iter().map(|(_, w)| w).sum();

            // Flooded rooms get fewer decals
            let density_divisor = match room.theme {
                RoomTheme::Flooded => 20,
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
        }

        decals
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
        for (x, y) in result.door_positions {
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
