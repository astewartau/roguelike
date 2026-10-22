//! Outdoor ground: the looks grass can take, and paths that join up.
//!
//! Both are drawn from `custom/outdoor.png`, which
//! `tools/make_outdoor_tiles.py` draws. That sheet is optional. Without it a
//! tile here falls back to the stock sprite its terrain has
//! (`TileType::fallback_sprite`), so the map still reads, just plainer.
//!
//! A grass look is purely a picture. The tile stays `TileType::Grass`, and the
//! look is kept in its `sprite_override`, as a dungeon floor keeps its
//! variety.
//!
//! A path is ground of its own, `TileType::Path`. Each path tile shows one of
//! 47 pictures, picked from which of its 8 neighbours are path too, so its
//! edges, outer corners, inner corners and junctions all line up. The choice
//! is kept as the ground's look (`Tile::ground`), so a path runs on under
//! tall grass or a wall standing on it, and is redone by `orient_around`
//! whenever the terrain around it is painted, as `walls` does for walls.

use crate::constants::{
    PATH_SPARE_MIDDLE_CHANCE, WILD_GRASS_CLOVER_SHARE, WILD_GRASS_DRY_BELOW, WILD_GRASS_LUSH_ABOVE,
    WILD_GRASS_MEADOW_SHARE, WILD_GRASS_MOSS_SHARE, WILD_GRASS_PATCH_SIZE, WILD_GRASS_PEBBLES_SHARE,
    WILD_GRASS_WORN_SHARE,
};
use crate::tile::{tile_ids, Ground, SpriteSheet, Tile, TileType};

/// The outdoor sheet's width in tiles.
pub const OUTDOOR_COLS: u32 = 16;

/// The looks grass can be painted with.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum GrassKind {
    Short,
    Lush,
    Meadow,
    Clover,
    Moss,
    Dry,
    Leaves,
    Pebbles,
    Worn,
}

impl GrassKind {
    pub const ALL: [GrassKind; 9] = [
        GrassKind::Short,
        GrassKind::Lush,
        GrassKind::Meadow,
        GrassKind::Clover,
        GrassKind::Moss,
        GrassKind::Dry,
        GrassKind::Leaves,
        GrassKind::Pebbles,
        GrassKind::Worn,
    ];

    /// Where its pictures start on the sheet, and how many there are. They
    /// run left to right along the sheet's first two rows, in the order of
    /// `GRASS` in the script.
    fn slots(self) -> (u32, u32) {
        match self {
            GrassKind::Short => (0, 4),
            GrassKind::Lush => (4, 4),
            GrassKind::Meadow => (8, 4),
            GrassKind::Clover => (12, 2),
            GrassKind::Moss => (14, 2),
            GrassKind::Dry => (16, 3),
            GrassKind::Leaves => (19, 2),
            GrassKind::Pebbles => (21, 2),
            GrassKind::Worn => (23, 1),
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            GrassKind::Short => "Short grass",
            GrassKind::Lush => "Lush grass",
            GrassKind::Meadow => "Meadow flowers",
            GrassKind::Clover => "Clover",
            GrassKind::Moss => "Moss",
            GrassKind::Dry => "Dry grass",
            GrassKind::Leaves => "Fallen leaves",
            GrassKind::Pebbles => "Pebbly grass",
            GrassKind::Worn => "Worn patch",
        }
    }

    /// Its first picture, for a menu.
    pub fn icon(self) -> (SpriteSheet, u32) {
        (SpriteSheet::Outdoor, self.slots().0)
    }

    /// The look wild grass has at `(x, y)`, as a field grows: mostly short
    /// grass, in drifts of lush and of dry, with flowers, clover, pebbles,
    /// moss and the odd worn patch dotted through. Fallen leaves are left
    /// out; they belong under trees. Made from the position, like
    /// `sprite_at`, so painting wild grass twice changes nothing.
    pub fn wild_at(x: i32, y: i32) -> GrassKind {
        let drift = smooth_noise(x, y, WILD_GRASS_PATCH_SIZE);
        if drift > WILD_GRASS_LUSH_ABOVE {
            return GrassKind::Lush;
        }
        if drift < WILD_GRASS_DRY_BELOW {
            return GrassKind::Dry;
        }
        let roll = (cell_hash(x ^ 0x5BD1, y ^ 0x0E99) % 10_000) as f32 / 10_000.0;
        let dotted = [
            (GrassKind::Meadow, WILD_GRASS_MEADOW_SHARE),
            (GrassKind::Clover, WILD_GRASS_CLOVER_SHARE),
            (GrassKind::Pebbles, WILD_GRASS_PEBBLES_SHARE),
            (GrassKind::Moss, WILD_GRASS_MOSS_SHARE),
            (GrassKind::Worn, WILD_GRASS_WORN_SHARE),
        ];
        let mut upto = 0.0;
        for (kind, share) in dotted {
            upto += share;
            if roll < upto {
                return kind;
            }
        }
        GrassKind::Short
    }

    /// The picture grass of this kind shows at `(x, y)`. All of a kind's
    /// pictures tile with each other and with every other kind's, so which
    /// one is a free choice. It's made from the position, so painting the
    /// same kind again changes nothing.
    pub fn sprite_at(self, x: i32, y: i32) -> (SpriteSheet, u32) {
        let (first, count) = self.slots();
        (SpriteSheet::Outdoor, first + cell_hash(x, y) % count)
    }
}

/// What a path is made of.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PathKind {
    Dirt,
    Cobbles,
    Flagstones,
    Gravel,
}

impl PathKind {
    pub const ALL: [PathKind; 4] = [PathKind::Dirt, PathKind::Cobbles, PathKind::Flagstones, PathKind::Gravel];

    pub fn name(self) -> &'static str {
        match self {
            PathKind::Dirt => "Dirt track",
            PathKind::Cobbles => "Cobbles",
            PathKind::Flagstones => "Flagstones",
            PathKind::Gravel => "Gravel path",
        }
    }

    /// The sheet's first row of this path's three, counting from 0.
    fn first_row(self) -> u32 {
        let block = match self {
            PathKind::Dirt => 0,
            PathKind::Cobbles => 1,
            PathKind::Flagstones => 2,
            PathKind::Gravel => 3,
        };
        GRASS_ROWS + block * PATH_ROWS
    }

    fn at(self, (col, row): (u32, u32)) -> (SpriteSheet, u32) {
        (SpriteSheet::Outdoor, (self.first_row() + row) * OUTDOOR_COLS + col)
    }

    /// The picture for a path tile whose neighbours are `mask` (`mask_at`).
    pub fn sprite(self, mask: u8) -> (SpriteSheet, u32) {
        self.at(slot_of(reduce(mask)))
    }

    /// Its open middle, for a menu.
    pub fn icon(self) -> (SpriteSheet, u32) {
        self.sprite(FULL)
    }

    /// What it looks like without the outdoor sheet: the stock floor
    /// nearest to it.
    pub fn stock_sprite(self) -> (SpriteSheet, u32) {
        match self {
            PathKind::Dirt => tile_ids::DIRT_ON_GRASS,
            PathKind::Cobbles => tile_ids::FLOOR,
            PathKind::Flagstones => tile_ids::FLOOR_2,
            PathKind::Gravel => tile_ids::DIRT_ON_GRASS_2,
        }
    }
}

/// Rows of grass at the top of the sheet, then three rows for each path.
const GRASS_ROWS: u32 = 2;
const PATH_ROWS: u32 = 3;

// Neighbour bits. Up the screen is +y (see `walls`), so N is (x, y + 1).
pub const N: u8 = 1;
pub const NE: u8 = 2;
pub const E: u8 = 4;
pub const SE: u8 = 8;
pub const S: u8 = 16;
pub const SW: u8 = 32;
pub const W: u8 = 64;
pub const NW: u8 = 128;
const FULL: u8 = 255;

const NEIGHBOURS: [(u8, i32, i32); 8] = [
    (N, 0, 1),
    (NE, 1, 1),
    (E, 1, 0),
    (SE, 1, -1),
    (S, 0, -1),
    (SW, -1, -1),
    (W, -1, 0),
    (NW, -1, 1),
];

/// Each diagonal with the two orthogonals either side of it.
const CORNERS: [(u8, u8, u8); 4] = [(NE, N, E), (SE, S, E), (SW, S, W), (NW, N, W)];

/// Drop each diagonal neighbour unless both orthogonals beside it are set:
/// a corner only changes the picture when it closes an inner corner. This
/// is what brings 256 masks down to the 47 the sheet draws.
pub fn reduce(mut mask: u8) -> u8 {
    for (d, a, b) in CORNERS {
        if mask & a == 0 || mask & b == 0 {
            mask &= !d;
        }
    }
    mask
}

/// The mask each path picture is drawn for, by column and row within its
/// path's three rows. Mirrors `block_slots` in tools/make_outdoor_tiles.py:
/// change one and you must change the other. The middle (255) appears twice;
/// the second, `SPARE_MIDDLE`, is the same open path with a little more
/// going on, for variety.
const LAYOUT: [[u8; OUTDOOR_COLS as usize]; PATH_ROWS as usize] = [
    [28, 124, 112, 16, 4, 68, 64, 5, 20, 21, 23, 29, 65, 69, 71, 80],
    [31, 255, 241, 17, 247, 223, 0, 81, 84, 85, 87, 92, 93, 95, 113, 116],
    [7, 199, 193, 1, 253, 127, 255, 117, 119, 125, 197, 209, 213, 215, 221, 245],
];
const SPARE_MIDDLE: (u32, u32) = (6, 2);

/// Where the picture for a reduced mask sits, as (column, row).
fn slot_of(mask: u8) -> (u32, u32) {
    for (row, masks) in LAYOUT.iter().enumerate() {
        if let Some(col) = masks.iter().position(|&m| m == mask) {
            return (col as u32, row as u32);
        }
    }
    // Every reduced mask is in the layout (see the tests), so this is only
    // reached by a mask that wasn't reduced: show the middle.
    (1, 1)
}

/// A small, steady hash of a cell, for picking among equal pictures.
fn cell_hash(x: i32, y: i32) -> u32 {
    let mut h = (x as u32).wrapping_mul(0x9E37_79B1) ^ (y as u32).wrapping_mul(0x85EB_CA77);
    h ^= h >> 15;
    h = h.wrapping_mul(0x2C1B_3C6D);
    h ^= h >> 12;
    h
}

/// A value in [0, 1] that drifts smoothly from cell to cell, over about
/// `size` cells: random values on a lattice that far apart, blended between.
fn smooth_noise(x: i32, y: i32, size: i32) -> f32 {
    let corner = |cx: i32, cy: i32| (cell_hash(cx.wrapping_mul(7919), cy.wrapping_mul(104_729)) % 10_000) as f32 / 10_000.0;
    let (gx, gy) = (x.div_euclid(size), y.div_euclid(size));
    let ease = |t: f32| t * t * (3.0 - 2.0 * t);
    let tx = ease(x.rem_euclid(size) as f32 / size as f32);
    let ty = ease(y.rem_euclid(size) as f32 / size as f32);
    let top = corner(gx, gy) + (corner(gx + 1, gy) - corner(gx, gy)) * tx;
    let bottom = corner(gx, gy + 1) + (corner(gx + 1, gy + 1) - corner(gx, gy + 1)) * tx;
    top + (bottom - top) * ty
}

fn is_path(tiles: &[Tile], width: usize, height: usize, x: i32, y: i32) -> bool {
    if x < 0 || y < 0 || x as usize >= width || y as usize >= height {
        // Off the map counts as more path, so a path run up to the edge
        // carries on out of view instead of stopping short of it.
        return true;
    }
    matches!(tiles[y as usize * width + x as usize].ground().tile_type, TileType::Path(_))
}

/// Which of the 8 neighbours of `(x, y)` have path for ground, of any kind:
/// a dirt track running into a cobbled road joins it rather than stopping
/// at a strip of grass, and a path carries on under tall grass.
pub fn mask_at(tiles: &[Tile], width: usize, height: usize, x: i32, y: i32) -> u8 {
    NEIGHBOURS
        .iter()
        .filter(|(_, dx, dy)| is_path(tiles, width, height, x + dx, y + dy))
        .fold(0, |m, (bit, _, _)| m | bit)
}

/// The picture the path at `(x, y)` shows.
fn sprite_for(tiles: &[Tile], width: usize, height: usize, x: i32, y: i32, kind: PathKind) -> (SpriteSheet, u32) {
    let mask = reduce(mask_at(tiles, width, height, x, y));
    if mask == FULL && (cell_hash(x, y) % 1000) < (PATH_SPARE_MIDDLE_CHANCE * 1000.0) as u32 {
        return kind.at(SPARE_MIDDLE);
    }
    kind.sprite(mask)
}

/// Give the path at `(x, y)` (if its ground is one) the picture its
/// neighbours call for.
fn orient(tiles: &mut [Tile], width: usize, height: usize, x: i32, y: i32) {
    if x < 0 || y < 0 || x as usize >= width || y as usize >= height {
        return;
    }
    let i = y as usize * width + x as usize;
    let ground = tiles[i].ground();
    let TileType::Path(kind) = ground.tile_type else {
        return;
    };
    let sprite = sprite_for(tiles, width, height, x, y, kind);
    tiles[i].set_ground(Ground { look: Some(sprite), ..ground });
}

/// Orient every path on the map.
#[allow(dead_code)] // for generators that lay paths down in bulk
pub fn orient_all(tiles: &mut [Tile], width: usize, height: usize) {
    for y in 0..height as i32 {
        for x in 0..width as i32 {
            orient(tiles, width, height, x, y);
        }
    }
}

/// Orient the paths in the rectangle `min..=max` and in the ring of tiles
/// round it, whose pictures depend on what's inside. For after the terrain
/// in that rectangle changes.
pub fn orient_around(tiles: &mut [Tile], width: usize, height: usize, min: (i32, i32), max: (i32, i32)) {
    for y in min.1 - 1..=max.1 + 1 {
        for x in min.0 - 1..=max.0 + 1 {
            orient(tiles, width, height, x, y);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::grid::Grid;

    /// The picture each path is drawn with, by name, mirroring the script.
    fn slot(kind: PathKind, sprite: (SpriteSheet, u32)) -> (u32, u32) {
        assert_eq!(sprite.0, SpriteSheet::Outdoor);
        let id = sprite.1 - kind.first_row() * OUTDOOR_COLS;
        (id % OUTDOOR_COLS, id / OUTDOOR_COLS)
    }

    #[test]
    fn every_way_a_path_can_join_has_its_own_picture() {
        let reduced: std::collections::BTreeSet<u8> = (0..=255u8).map(reduce).collect();
        assert_eq!(reduced.len(), 47);
        let mut seen = std::collections::BTreeSet::new();
        for mask in &reduced {
            let at = slot_of(*mask);
            assert_eq!(LAYOUT[at.1 as usize][at.0 as usize], *mask);
            assert!(seen.insert(at), "two masks share slot {at:?}");
        }
        // 47 masks and the spare middle fill the three rows exactly.
        assert_eq!(seen.len() + 1, (OUTDOOR_COLS * PATH_ROWS) as usize);
        assert!(!seen.contains(&SPARE_MIDDLE));
        assert_eq!(LAYOUT[SPARE_MIDDLE.1 as usize][SPARE_MIDDLE.0 as usize], FULL);
    }

    #[test]
    fn a_corner_counts_only_when_it_closes_an_inner_corner() {
        assert_eq!(reduce(NE), 0, "a lone diagonal changes nothing");
        assert_eq!(reduce(N | NE), N);
        assert_eq!(reduce(N | E | NE), N | E | NE);
        assert_eq!(reduce(FULL), FULL);
    }

    /// Paths of every kind run into each other, and on off the map's edge.
    #[test]
    fn paths_join_any_path_and_the_edge_of_the_map() {
        let mut grid = Grid::filled(5, 5, TileType::Grass);
        let set = |grid: &mut Grid, x, y, kind| grid.get_mut(x, y).unwrap().tile_type = TileType::Path(kind);
        set(&mut grid, 1, 2, PathKind::Dirt);
        set(&mut grid, 2, 2, PathKind::Cobbles);
        set(&mut grid, 2, 3, PathKind::Gravel);
        set(&mut grid, 4, 4, PathKind::Dirt);
        let mask = |grid: &Grid, x, y| mask_at(&grid.tiles, grid.width, grid.height, x, y);
        assert_eq!(mask(&grid, 2, 2), W | N);
        // The gravel at (2, 3) is on its diagonal, which only counts once
        // reduced if it closes a corner: here it doesn't.
        assert_eq!(mask(&grid, 1, 2), E | NE);
        assert_eq!(reduce(mask(&grid, 1, 2)), E);
        // The top-right corner: off the map to the north and east.
        assert_eq!(reduce(mask(&grid, 4, 4)), N | E | NE);
    }

    /// The path's pictures, by where they sit on the sheet: a block
    /// of path shows its corners and edges round an open middle, and a
    /// single missing tile in a block shows as four inner corners.
    #[test]
    fn a_painted_block_shows_its_edges_and_corners() {
        let mut grid = Grid::filled(7, 7, TileType::Grass);
        for y in 1..=5 {
            for x in 1..=5 {
                grid.get_mut(x, y).unwrap().tile_type = TileType::Path(PathKind::Cobbles);
            }
        }
        grid.get_mut(3, 3).unwrap().tile_type = TileType::Grass;
        orient_all(&mut grid.tiles, grid.width, grid.height);
        let at = |x, y| slot(PathKind::Cobbles, grid.get(x, y).unwrap().sprite());
        // Up the screen is +y, so the top-left corner of the block is (1, 5).
        assert_eq!(at(1, 5), (0, 0), "top-left corner");
        assert_eq!(at(5, 5), (2, 0), "top-right corner");
        assert_eq!(at(1, 1), (0, 2), "bottom-left corner");
        assert_eq!(at(5, 1), (2, 2), "bottom-right corner");
        assert_eq!(at(3, 5), (1, 0), "top edge");
        assert_eq!(at(1, 3), (0, 1), "left edge");
        // Round the hole: each neighbour has the hole on its inner corner.
        assert_eq!(at(2, 4), (4, 1), "hole to the bottom-right");
        assert_eq!(at(4, 4), (5, 1), "hole to the bottom-left");
        assert_eq!(at(2, 2), (4, 2), "hole to the top-right");
        assert_eq!(at(4, 2), (5, 2), "hole to the top-left");
        // Orthogonal neighbours of the hole are edges, not middles.
        assert_eq!(LAYOUT[at(3, 4).1 as usize][at(3, 4).0 as usize], reduce(FULL & !S & !SE & !SW));
    }

    /// Wild grass is mostly short, grows lush and dry in drifts rather than
    /// speckled, has every look but fallen leaves, and is the same each
    /// time it's painted.
    #[test]
    fn wild_grass_grows_like_a_field() {
        let mut counts = std::collections::HashMap::new();
        let mut lush_beside_lush = 0;
        let mut lush = 0;
        for y in 0..96 {
            for x in 0..96 {
                let kind = GrassKind::wild_at(x, y);
                assert_eq!(kind, GrassKind::wild_at(x, y));
                *counts.entry(kind).or_insert(0) += 1;
                if kind == GrassKind::Lush {
                    lush += 1;
                    if GrassKind::wild_at(x + 1, y) == GrassKind::Lush {
                        lush_beside_lush += 1;
                    }
                }
            }
        }
        let count = |k| counts.get(&k).copied().unwrap_or(0);
        for kind in GrassKind::ALL {
            if kind == GrassKind::Leaves {
                assert_eq!(count(kind), 0, "leaves belong under trees");
            } else {
                assert!(count(kind) > 0, "no {kind:?} in a 96x96 field");
            }
        }
        let most = GrassKind::ALL.iter().max_by_key(|k| count(**k)).copied();
        assert_eq!(most, Some(GrassKind::Short));
        assert!(lush_beside_lush * 2 > lush, "lush grass comes in drifts: {lush_beside_lush} of {lush}");
    }

    #[test]
    fn grass_looks_pick_one_of_their_own_pictures() {
        for kind in GrassKind::ALL {
            let (first, count) = kind.slots();
            for (x, y) in [(0, 0), (3, 7), (-2, 5), (40, 40)] {
                let (sheet, id) = kind.sprite_at(x, y);
                assert_eq!(sheet, SpriteSheet::Outdoor);
                assert!((first..first + count).contains(&id), "{kind:?} at {x},{y}");
                assert_eq!(kind.sprite_at(x, y), (sheet, id), "the same cell, the same picture");
            }
        }
        // The grass looks fill the slots before the paths, with no gaps.
        let mut slots: Vec<(u32, u32)> = GrassKind::ALL.iter().map(|k| k.slots()).collect();
        slots.sort();
        let mut next = 0;
        for (first, count) in slots {
            assert_eq!(first, next);
            next += count;
        }
        assert!(next <= GRASS_ROWS * OUTDOOR_COLS);
    }
}
