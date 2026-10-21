//! How a wall is drawn: the one rule every wall follows, however it came
//! to be there (generated, painted, the sandbox's border).
//!
//! The map is seen from above and a little to the south, as the 32rogues
//! sheet draws it. Each wall style comes as a pair: its *cap* (the top of
//! the wall mass, `a` in the sheet) and its *face* (the cap with the front
//! of the wall below it, `b`). A wall shows its face when the tile below it
//! on screen (lower y) isn't wall, since that front is what you'd see; with
//! wall below, its front is hidden behind that wall and only the cap shows.
//!
//! What a wall is made of is its `WallStyle`, kept on the tile so that
//! re-orienting it (when a neighbour changes) keeps its look.

use crate::tile::{tile_ids, SpriteSheet, Tile, TileType};

/// What a wall is made of: which pair of sprites it's drawn from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum WallStyle {
    /// Stone brick, the dungeon's own.
    #[default]
    Brick,
    /// Rough stone, around overgrown rooms.
    Rough,
    /// Skull walls, around crypts.
    Crypt,
    /// Packed dirt, around caverns.
    Dirt,
    /// Dirt with a vein of ore showing (decoration: there's no mining). It
    /// looks the same whichever way it faces.
    DirtOre,
}

impl WallStyle {
    /// The style of the walls around a room of `theme` (Brick for none).
    pub fn for_theme(theme: Option<crate::dungeon_gen::RoomTheme>) -> Self {
        use crate::dungeon_gen::RoomTheme;
        match theme {
            Some(RoomTheme::Overgrown) => WallStyle::Rough,
            Some(RoomTheme::Crypt) => WallStyle::Crypt,
            Some(RoomTheme::Cavern) => WallStyle::Dirt,
            _ => WallStyle::Brick,
        }
    }

    /// Its (cap, face) sprites.
    fn sprites(self) -> ((SpriteSheet, u32), (SpriteSheet, u32)) {
        match self {
            WallStyle::Brick => (tile_ids::WALL_TOP, tile_ids::WALL),
            WallStyle::Rough => (tile_ids::WALL_ROUGH_TOP, tile_ids::WALL_ROUGH),
            WallStyle::Crypt => (tile_ids::WALL_CRYPT_TOP, tile_ids::WALL_CRYPT),
            WallStyle::Dirt => (tile_ids::WALL_DIRT_TOP, tile_ids::WALL_DIRT),
            WallStyle::DirtOre => (tile_ids::CAVE_ORE_WALL, tile_ids::CAVE_ORE_WALL),
        }
    }

    /// The style a newly built wall takes from the walls already around it:
    /// so a wall painted onto a crypt's edge is a skull wall. (Ore is the
    /// odd vein, not a style to spread.)
    fn plain(self) -> Self {
        if self == WallStyle::DirtOre {
            WallStyle::Dirt
        } else {
            self
        }
    }
}

/// Whether the tile at `(x, y)` is wall (off the map is not).
fn is_wall(tiles: &[Tile], width: usize, height: usize, x: i32, y: i32) -> bool {
    x >= 0
        && y >= 0
        && (x as usize) < width
        && (y as usize) < height
        && tiles[y as usize * width + x as usize].tile_type == TileType::Wall
}

/// The sprite the wall at `(x, y)` shows: its face if there's no wall
/// below it, its cap if there is.
fn sprite_for(tiles: &[Tile], width: usize, height: usize, x: i32, y: i32) -> (SpriteSheet, u32) {
    let (cap, face) = tiles[y as usize * width + x as usize].wall_style.sprites();
    if is_wall(tiles, width, height, x, y - 1) {
        cap
    } else {
        face
    }
}

/// Give the wall at `(x, y)` (if it is one) the sprite the rule says.
fn orient(tiles: &mut [Tile], width: usize, height: usize, x: i32, y: i32) {
    if !is_wall(tiles, width, height, x, y) {
        return;
    }
    let sprite = sprite_for(tiles, width, height, x, y);
    tiles[y as usize * width + x as usize].sprite_override = Some(sprite);
}

/// Orient every wall on the map.
pub fn orient_all(tiles: &mut [Tile], width: usize, height: usize) {
    for y in 0..height as i32 {
        for x in 0..width as i32 {
            orient(tiles, width, height, x, y);
        }
    }
}

/// Orient the walls in the rectangle `min..=max` and those a change there
/// could have turned: the row above it (whose face depends on what's below
/// it). For after the terrain in that rectangle changes.
pub fn orient_around(tiles: &mut [Tile], width: usize, height: usize, min: (i32, i32), max: (i32, i32)) {
    for y in min.1..=max.1 + 1 {
        for x in min.0..=max.0 {
            orient(tiles, width, height, x, y);
        }
    }
}

/// The style for a wall newly built at `(x, y)`: that of the walls beside
/// it (the most common, ties going to the first in N, S, E, W, then
/// diagonal order), or Brick with none about.
pub fn style_from_neighbours(tiles: &[Tile], width: usize, height: usize, x: i32, y: i32) -> WallStyle {
    const AROUND: [(i32, i32); 8] = [(0, 1), (0, -1), (1, 0), (-1, 0), (1, 1), (-1, 1), (1, -1), (-1, -1)];
    let mut counts: Vec<(WallStyle, usize)> = Vec::new();
    for (dx, dy) in AROUND {
        if !is_wall(tiles, width, height, x + dx, y + dy) {
            continue;
        }
        let style = tiles[(y + dy) as usize * width + (x + dx) as usize].wall_style.plain();
        match counts.iter_mut().find(|(s, _)| *s == style) {
            Some((_, n)) => *n += 1,
            None => counts.push((style, 1)),
        }
    }
    // `max_by_key` keeps the last of equals; the first should win a tie.
    counts.iter().rev().max_by_key(|(_, n)| *n).map(|(s, _)| *s).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A map from rows of text, top row first (as on screen): `#` wall,
    /// `.` floor, `c` crypt wall.
    fn map(rows: &[&str]) -> (Vec<Tile>, usize, usize) {
        let (width, height) = (rows[0].len(), rows.len());
        let mut tiles = vec![Tile::new(TileType::Floor); width * height];
        for (row, line) in rows.iter().enumerate() {
            let y = height - 1 - row;
            for (x, c) in line.chars().enumerate() {
                let tile = &mut tiles[y * width + x];
                if c != '.' {
                    tile.tile_type = TileType::Wall;
                    tile.wall_style = if c == 'c' { WallStyle::Crypt } else { WallStyle::Brick };
                }
            }
        }
        (tiles, width, height)
    }

    fn at(tiles: &[Tile], width: usize, height: usize, x: usize, row: usize) -> (SpriteSheet, u32) {
        tiles[(height - 1 - row) * width + x].sprite()
    }

    /// A room: the wall along its top shows its face into the room, the
    /// side walls (wall below them) their caps, and the bottom wall (open
    /// below, off the map) its face.
    #[test]
    fn a_wall_faces_whatever_is_open_below_it() {
        let (mut tiles, w, h) = map(&["#####", "#...#", "#...#", "#####"]);
        orient_all(&mut tiles, w, h);
        assert_eq!(at(&tiles, w, h, 2, 0), tile_ids::WALL, "top wall: room below, face");
        assert_eq!(at(&tiles, w, h, 0, 0), tile_ids::WALL_TOP, "top corner: wall below, cap");
        assert_eq!(at(&tiles, w, h, 0, 1), tile_ids::WALL_TOP, "side wall: cap");
        assert_eq!(at(&tiles, w, h, 4, 2), tile_ids::WALL_TOP, "side wall: cap");
        assert_eq!(at(&tiles, w, h, 2, 3), tile_ids::WALL, "bottom wall: nothing below, face");
        assert_eq!(at(&tiles, w, h, 0, 3), tile_ids::WALL, "bottom corner: face");
    }

    /// Knocking out a wall turns the one above it to face the new opening;
    /// a painted wall takes the style of the walls around it.
    #[test]
    fn changes_reorient_what_they_affect() {
        let (mut tiles, w, h) = map(&["ccc", "ccc", "..."]);
        orient_all(&mut tiles, w, h);
        assert_eq!(at(&tiles, w, h, 1, 0), tile_ids::WALL_CRYPT_TOP);

        // Knock out the middle of the lower row (row 1 is y = 1).
        tiles[w + 1] = Tile::new(TileType::Floor);
        orient_around(&mut tiles, w, h, (1, 1), (1, 1));
        assert_eq!(at(&tiles, w, h, 1, 0), tile_ids::WALL_CRYPT, "open below now: face");

        // And build it back, in the style around it.
        let style = style_from_neighbours(&tiles, w, h, 1, 1);
        assert_eq!(style, WallStyle::Crypt);
        tiles[w + 1].tile_type = TileType::Wall;
        tiles[w + 1].wall_style = style;
        orient_around(&mut tiles, w, h, (1, 1), (1, 1));
        assert_eq!(at(&tiles, w, h, 1, 0), tile_ids::WALL_CRYPT_TOP, "wall below again: cap");
        assert_eq!(at(&tiles, w, h, 1, 1), tile_ids::WALL_CRYPT, "the new wall faces the floor");
    }
}
