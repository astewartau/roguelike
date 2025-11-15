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

    fn intersects(&self, other: &Rect) -> bool {
        self.x <= other.x + other.width
            && self.x + self.width >= other.x
            && self.y <= other.y + other.height
            && self.y + self.height >= other.y
    }
}

pub struct DungeonGenerator {
    width: usize,
    height: usize,
    tiles: Vec<Tile>,
    rooms: Vec<Rect>,
}

impl DungeonGenerator {
    pub fn new(width: usize, height: usize) -> Self {
        Self {
            width,
            height,
            tiles: vec![Tile::new(TileType::Wall); width * height],
            rooms: Vec::new(),
        }
    }

    pub fn generate(width: usize, height: usize) -> (Vec<Tile>, Vec<(i32, i32)>) {
        let mut gen = Self::new(width, height);

        // Generate rooms
        gen.generate_rooms();

        // Connect all rooms with corridors
        gen.connect_rooms();

        // Collect chest spawn positions (center of each room except first)
        let chest_positions: Vec<(i32, i32)> = gen.rooms.iter()
            .skip(1) // Skip first room (player spawns there)
            .map(|room| room.center())
            .collect();

        (gen.tiles, chest_positions)
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

    fn generate_rooms(&mut self) {
        let mut rng = rand::thread_rng();

        // Try to place 30-40 rooms
        let max_attempts = 500;
        let target_rooms = rng.gen_range(30..41);

        for _ in 0..max_attempts {
            if self.rooms.len() >= target_rooms {
                break;
            }

            // Random room size
            let width = rng.gen_range(4..12);
            let height = rng.gen_range(4..12);
            let x = rng.gen_range(1..self.width as i32 - width - 1);
            let y = rng.gen_range(1..self.height as i32 - height - 1);

            let new_room = Rect::new(x, y, width, height);

            // Check if room overlaps with existing rooms (with 1 tile padding)
            let mut overlaps = false;
            for room in &self.rooms {
                let padded = Rect::new(
                    room.x - 1,
                    room.y - 1,
                    room.width + 2,
                    room.height + 2,
                );
                if new_room.intersects(&padded) {
                    overlaps = true;
                    break;
                }
            }

            if !overlaps {
                self.carve_room(&new_room);
                self.rooms.push(new_room);
            }
        }
    }

    fn carve_room(&mut self, room: &Rect) {
        for y in room.y..room.y + room.height {
            for x in room.x..room.x + room.width {
                self.set_tile(x, y, TileType::Floor);
            }
        }
    }

    fn connect_rooms(&mut self) {
        let mut rng = rand::thread_rng();

        // Connect each room to the next one
        for i in 0..self.rooms.len() - 1 {
            let (x1, y1) = self.rooms[i].center();
            let (x2, y2) = self.rooms[i + 1].center();

            // Randomly choose to go horizontal then vertical, or vice versa
            if rng.gen_bool(0.5) {
                self.create_h_corridor(x1, x2, y1);
                self.create_v_corridor(y1, y2, x2);
            } else {
                self.create_v_corridor(y1, y2, x1);
                self.create_h_corridor(x1, x2, y2);
            }
        }

        // Add some extra connections for more interesting layout
        let num_extra = (self.rooms.len() / 4).max(3);
        for _ in 0..num_extra {
            let i = rng.gen_range(0..self.rooms.len());
            let j = rng.gen_range(0..self.rooms.len());

            if i != j {
                let (x1, y1) = self.rooms[i].center();
                let (x2, y2) = self.rooms[j].center();

                if rng.gen_bool(0.5) {
                    self.create_h_corridor(x1, x2, y1);
                    self.create_v_corridor(y1, y2, x2);
                } else {
                    self.create_v_corridor(y1, y2, x1);
                    self.create_h_corridor(x1, x2, y2);
                }
            }
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
}
