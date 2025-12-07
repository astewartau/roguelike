use crate::constants::*;
use crate::tile::{Tile, TileType};
use rand::Rng;

#[derive(Clone, Copy)]
struct Rect {
    x: i32,
    y: i32,
    width: i32,
    height: i32,
}

impl Rect {
    fn new(x: i32, y: i32, width: i32, height: i32) -> Self {
        Self { x, y, width, height }
    }

    fn center(&self) -> (i32, i32) {
        (self.x + self.width / 2, self.y + self.height / 2)
    }
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

    /// Returns (tiles, chest_positions, door_positions)
    pub fn generate(width: usize, height: usize) -> (Vec<Tile>, Vec<(i32, i32)>, Vec<(i32, i32)>) {
        let mut gen = Self::new(width, height);
        let mut rng = rand::thread_rng();

        // Create the root BSP node covering the entire map
        let root_region = Rect::new(0, 0, width as i32, height as i32);
        let mut root = BspNode::new(root_region);

        // Recursively split the space
        root.split(&mut rng);

        // Create rooms in each leaf
        root.create_rooms(&mut rng);

        // Carve all rooms into the tile map
        let mut rooms = Vec::new();
        root.collect_rooms(&mut rooms);
        for room in &rooms {
            gen.carve_room(room);
        }

        // Connect sibling rooms by traversing the BSP tree
        gen.connect_bsp(&root, &mut rng);

        // Find door positions (but keep floor tiles - doors are entities)
        let door_positions = gen.find_door_positions(&rooms);

        // Collect chest spawn positions (center of each room except first)
        let chest_positions: Vec<(i32, i32)> = rooms.iter()
            .skip(1)
            .map(|room| room.center())
            .collect();

        (gen.tiles, chest_positions, door_positions)
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

    fn carve_room(&mut self, room: &Rect) {
        for y in room.y..room.y + room.height {
            for x in room.x..room.x + room.width {
                self.set_tile(x, y, TileType::Floor);
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
        let (tiles, _, _) = DungeonGenerator::generate(50, 50);
        assert_eq!(tiles.len(), 50 * 50);
    }

    #[test]
    fn test_dungeon_has_floor_tiles() {
        let (tiles, _, _) = DungeonGenerator::generate(50, 50);
        let floor_count = tiles.iter().filter(|t| t.tile_type == TileType::Floor).count();
        // Should have at least some floor tiles
        assert!(floor_count > 0);
    }

    #[test]
    fn test_dungeon_has_wall_tiles() {
        let (tiles, _, _) = DungeonGenerator::generate(50, 50);
        let wall_count = tiles.iter().filter(|t| t.tile_type == TileType::Wall).count();
        // Should have some walls
        assert!(wall_count > 0);
    }

    #[test]
    fn test_dungeon_generates_chest_positions() {
        let (_, chest_positions, _) = DungeonGenerator::generate(50, 50);
        // Should have at least some chests (one per room except first)
        assert!(!chest_positions.is_empty());
    }

    #[test]
    fn test_dungeon_generates_door_positions() {
        let (_, _, door_positions) = DungeonGenerator::generate(50, 50);
        // Should have some doors
        assert!(!door_positions.is_empty());
    }

    #[test]
    fn test_chest_positions_are_on_floor() {
        let (tiles, chest_positions, _) = DungeonGenerator::generate(50, 50);
        for (x, y) in chest_positions {
            let idx = y as usize * 50 + x as usize;
            assert_eq!(tiles[idx].tile_type, TileType::Floor);
        }
    }

    #[test]
    fn test_door_positions_are_on_floor() {
        let (tiles, _, door_positions) = DungeonGenerator::generate(50, 50);
        for (x, y) in door_positions {
            let idx = y as usize * 50 + x as usize;
            assert_eq!(tiles[idx].tile_type, TileType::Floor);
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
