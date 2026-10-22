//! Painting terrain: setting tiles by hand, with the follow-ups a terrain
//! change needs (see `systems::fire`, where burnt grass turns to floor).
//!
//! Terrain comes in two layers. Ground is what's underfoot: floor, grass,
//! water, a path. Walls, tall grass and stairs stand on the ground
//! (`TileType::stands_on_ground`). Painting ground under one of them leaves
//! it standing; `Paint::ClearTop` takes it away and shows its ground again.

use crate::components::{AnimatedSprite, Position, VisualPosition};
use crate::grid::Grid;
use crate::outdoor::{GrassKind, PathKind};
use crate::tile::{tile_ids, Ground, SpriteSheet, Tile, TileType};

/// What the paint tool lays down.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Paint {
    /// Terrain: ground, or something that stands on it, by the type.
    Terrain(TileType),
    /// Grass with one of the outdoor looks.
    Grass(GrassKind),
    /// Grass with the outdoor looks mixed as a field grows them
    /// (`GrassKind::wild_at`).
    WildGrass,
    /// Take away whatever stands on the ground.
    ClearTop,
}

impl From<TileType> for Paint {
    fn from(tile: TileType) -> Self {
        Paint::Terrain(tile)
    }
}

impl From<GrassKind> for Paint {
    fn from(kind: GrassKind) -> Self {
        Paint::Grass(kind)
    }
}

impl Paint {
    pub fn name(self) -> String {
        match self {
            Paint::Terrain(tile) => match tile {
                TileType::Path(kind) => kind.name(),
                TileType::Grass => "Indoor grass",
                TileType::TallGrass => "Tall grass",
                TileType::StairsDown => "Stairs down",
                TileType::StairsUp => "Stairs up",
                other => return format!("{other:?}"),
            }
            .to_string(),
            Paint::Grass(kind) => kind.name().to_string(),
            Paint::WildGrass => "Wild grass (mixed)".to_string(),
            Paint::ClearTop => "Clear what's on top".to_string(),
        }
    }

    /// Its picture in the panel, and the stock one to show if the sheet
    /// that picture is on isn't loaded.
    pub fn icon(self) -> ((SpriteSheet, u32), (SpriteSheet, u32)) {
        match self {
            Paint::Terrain(tile) => (tile.sprite_ref(), tile.fallback_sprite()),
            Paint::Grass(kind) => (kind.icon(), tile_ids::GRASS),
            Paint::WildGrass => (GrassKind::Meadow.icon(), tile_ids::GRASS),
            Paint::ClearTop => (tile_ids::HAMMER, tile_ids::HAMMER),
        }
    }

    /// What it would show at `(x, y)`, for the brush's preview: the look
    /// that tile would get, where that's known before painting.
    pub fn preview_at(self, x: i32, y: i32) -> Option<((SpriteSheet, u32), (SpriteSheet, u32))> {
        match self {
            Paint::Grass(kind) => Some((kind.sprite_at(x, y), tile_ids::GRASS)),
            Paint::WildGrass => Some((GrassKind::wild_at(x, y).sprite_at(x, y), tile_ids::GRASS)),
            Paint::ClearTop => None,
            other => Some(other.icon()),
        }
    }

    /// The ground it lays at `(x, y)` over `current`, if it's ground.
    fn ground_at(self, x: i32, y: i32, current: Ground) -> Option<Ground> {
        let hash = |n: usize| ((x as u32).wrapping_mul(0x9E37_79B1) ^ (y as u32).wrapping_mul(0x85EB_CA77)) as usize % n;
        let look = |tile_type, look| Some(Ground { tile_type, look });
        match self {
            Paint::Terrain(tile) if tile.stands_on_ground() => None,
            // Floor and indoor grass get the variety the dungeon's have.
            Paint::Terrain(TileType::Floor) => look(TileType::Floor, Some(tile_ids::FLOOR_VARIANTS[hash(3)])),
            Paint::Terrain(TileType::Grass) => look(TileType::Grass, Some(tile_ids::GRASS_VARIANTS[hash(3)])),
            // A path keeps the picture it has until its neighbours say
            // otherwise (`outdoor::orient_around`, once painting is done).
            Paint::Terrain(path @ TileType::Path(_)) if current.tile_type == path => Some(current),
            Paint::Terrain(tile) => look(tile, None),
            Paint::Grass(kind) => look(TileType::Grass, Some(kind.sprite_at(x, y))),
            Paint::WildGrass => look(TileType::Grass, Some(GrassKind::wild_at(x, y).sprite_at(x, y))),
            Paint::ClearTop => None,
        }
    }

    /// Lay it on `cell`, at `(x, y)`.
    fn apply(self, cell: &mut Tile, x: i32, y: i32) {
        match self {
            Paint::Terrain(top) if top.stands_on_ground() => cell.put_on_top(top),
            Paint::ClearTop => cell.clear_top(),
            ground => {
                if let Some(ground) = ground.ground_at(x, y, cell.ground()) {
                    cell.set_ground(ground);
                }
            }
        }
    }
}

/// The panel's lists, by where they're used and which layer they paint.
pub const INDOOR_GROUND: [Paint; 4] = [
    Paint::Terrain(TileType::Floor),
    Paint::Terrain(TileType::Stone),
    Paint::Terrain(TileType::Grass),
    Paint::Terrain(TileType::Water),
];
pub const INDOOR_ON_TOP: [Paint; 3] = [
    Paint::Terrain(TileType::Wall),
    Paint::Terrain(TileType::StairsDown),
    Paint::Terrain(TileType::StairsUp),
];
pub const OUTDOOR_GRASS: [Paint; 10] = [
    Paint::WildGrass,
    Paint::Grass(GrassKind::ALL[0]),
    Paint::Grass(GrassKind::ALL[1]),
    Paint::Grass(GrassKind::ALL[2]),
    Paint::Grass(GrassKind::ALL[3]),
    Paint::Grass(GrassKind::ALL[4]),
    Paint::Grass(GrassKind::ALL[5]),
    Paint::Grass(GrassKind::ALL[6]),
    Paint::Grass(GrassKind::ALL[7]),
    Paint::Grass(GrassKind::ALL[8]),
];
pub const OUTDOOR_PATHS: [Paint; 4] = [
    Paint::Terrain(TileType::Path(PathKind::ALL[0])),
    Paint::Terrain(TileType::Path(PathKind::ALL[1])),
    Paint::Terrain(TileType::Path(PathKind::ALL[2])),
    Paint::Terrain(TileType::Path(PathKind::ALL[3])),
];
pub const OUTDOOR_ON_TOP: [Paint; 1] = [Paint::Terrain(TileType::TallGrass)];

/// Paint `what` over the square of side `2 * brush + 1` centred on `at`.
/// Keeps the water list and its animation entities in step and moves the
/// stairs markers, and turns the walls and paths in and around it to show
/// as they should (`walls`, `outdoor`); the caller sets `fov_dirty`.
/// Returns whether anything changed.
pub fn paint(world: &mut crate::world::World, grid: &mut Grid, at: (i32, i32), brush: i32, what: impl Into<Paint>) -> bool {
    let what = what.into();
    let mut changed = false;
    for dy in -brush..=brush {
        for dx in -brush..=brush {
            changed |= paint_tile(world, grid, (at.0 + dx, at.1 + dy), what);
        }
    }
    if changed {
        let (min, max) = ((at.0 - brush, at.1 - brush), (at.0 + brush, at.1 + brush));
        crate::walls::orient_around(&mut grid.tiles, grid.width, grid.height, min, max);
        crate::outdoor::orient_around(&mut grid.tiles, grid.width, grid.height, min, max);
    }
    changed
}

fn paint_tile(world: &mut crate::world::World, grid: &mut Grid, (x, y): (i32, i32), what: Paint) -> bool {
    // A new wall is built in the style of the walls beside it.
    let style = crate::walls::style_from_neighbours(&grid.tiles, grid.width, grid.height, x, y);
    let Some(cell) = grid.get_mut(x, y) else {
        return false;
    };
    let before = (cell.tile_type, cell.sprite_override, cell.under);
    let was = cell.tile_type;
    what.apply(cell, x, y);
    let now = cell.tile_type;
    if (now, cell.sprite_override, cell.under) == before {
        return false;
    }
    if now == TileType::Wall && was != TileType::Wall {
        cell.wall_style = style;
    }
    cell.explored = true;

    // Water that's covered over (or uncovered) stops (or starts) being
    // water, ripples and all.
    if was == TileType::Water && now != TileType::Water {
        grid.water_positions.retain(|&p| p != (x, y));
        let ripples: Vec<hecs::Entity> = world
            .query::<(&Position, &AnimatedSprite)>()
            .iter()
            .filter(|(_, (p, a))| (p.x, p.y) == (x, y) && a.base_tile_id == tile_ids::WATER_ANIMATED.1)
            .map(|(id, _)| id)
            .collect();
        for id in ripples {
            let _ = world.despawn(id);
        }
    }
    if now == TileType::Water && was != TileType::Water {
        grid.water_positions.push((x, y));
        let pos = Position::new(x, y);
        world.spawn((pos, VisualPosition::from_position(&pos), AnimatedSprite::water()));
    }
    match now {
        TileType::StairsDown => grid.stairs_down_pos = Some((x, y)),
        TileType::StairsUp => grid.stairs_up_pos = Some((x, y)),
        _ => {}
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn painting_water_keeps_the_water_list_and_ripples_in_step() {
        let mut world = crate::world::World::new();
        let mut grid = Grid::filled(9, 9, TileType::Floor);
        assert!(paint(&mut world, &mut grid, (4, 4), 1, TileType::Water));
        assert_eq!(grid.water_positions.len(), 9);
        assert_eq!(world.query::<&AnimatedSprite>().iter().count(), 9);

        // Painting floor back over the middle dries those tiles out again.
        paint(&mut world, &mut grid, (4, 4), 0, TileType::Floor);
        assert_eq!(grid.water_positions.len(), 8);
        assert!(!grid.water_positions.contains(&(4, 4)));
        assert_eq!(world.query::<&AnimatedSprite>().iter().count(), 8);
        assert!(!paint(&mut world, &mut grid, (4, 4), 0, TileType::Floor), "nothing left to change");
    }

    /// Painted walls follow the walls' rule: a block of them shows its
    /// face along its open bottom edge and its cap above that, and
    /// clearing the wall under a wall turns that wall to face the gap.
    #[test]
    fn painted_walls_face_the_way_walls_do() {
        let mut world = crate::world::World::new();
        let mut grid = Grid::filled(9, 9, TileType::Floor);
        paint(&mut world, &mut grid, (4, 4), 1, TileType::Wall);
        let sprite = |grid: &Grid, x, y| grid.get(x, y).unwrap().sprite();
        assert_eq!(sprite(&grid, 4, 3), tile_ids::WALL, "bottom row: floor below, face");
        assert_eq!(sprite(&grid, 4, 4), tile_ids::WALL_TOP, "wall below: cap");
        assert_eq!(sprite(&grid, 4, 5), tile_ids::WALL_TOP);

        // Floor painted there goes under the wall, which stays.
        assert!(paint(&mut world, &mut grid, (4, 3), 0, TileType::Floor));
        assert_eq!(grid.get(4, 3).unwrap().tile_type, TileType::Wall);
        paint(&mut world, &mut grid, (4, 3), 0, Paint::ClearTop);
        assert_eq!(grid.get(4, 3).unwrap().tile_type, TileType::Floor);
        assert_eq!(sprite(&grid, 4, 4), tile_ids::WALL, "opened up below: face");
    }

    /// A path painted a tile at a time joins up as it goes: the end of a
    /// track is a cap until the next tile is laid, then a straight run; and
    /// painting over a path tile turns the tiles either side back into ends.
    #[test]
    fn painted_paths_join_their_neighbours_and_part_again() {
        use crate::outdoor::{reduce, E, N, S, W};
        let mut world = crate::world::World::new();
        let mut grid = Grid::filled(9, 9, TileType::Grass);
        let dirt = TileType::Path(PathKind::Dirt);
        let sprite = |grid: &Grid, x, y| grid.get(x, y).unwrap().sprite();
        assert!(paint(&mut world, &mut grid, (3, 4), 0, dirt));
        assert_eq!(sprite(&grid, 3, 4), PathKind::Dirt.sprite(0), "alone: a lone cell");
        paint(&mut world, &mut grid, (4, 4), 0, dirt);
        assert_eq!(sprite(&grid, 3, 4), PathKind::Dirt.sprite(E), "the first tile became an end");
        paint(&mut world, &mut grid, (5, 4), 0, dirt);
        assert_eq!(sprite(&grid, 4, 4), PathKind::Dirt.sprite(E | W), "a straight run");
        // A cobbled road crossing it joins it too.
        paint(&mut world, &mut grid, (4, 5), 0, TileType::Path(PathKind::Cobbles));
        assert_eq!(sprite(&grid, 4, 4), PathKind::Dirt.sprite(reduce(N | E | W)));
        assert_eq!(sprite(&grid, 4, 5), PathKind::Cobbles.sprite(S));

        paint(&mut world, &mut grid, (4, 4), 0, GrassKind::Short);
        assert_eq!(sprite(&grid, 3, 4), PathKind::Dirt.sprite(0), "parted again");
        assert_eq!(sprite(&grid, 5, 4), PathKind::Dirt.sprite(0));
        assert_eq!(sprite(&grid, 4, 5), PathKind::Cobbles.sprite(0));
    }

    /// Grass keeps the look it's painted with, and painting a different
    /// look over it changes the look but not the terrain.
    #[test]
    fn grass_takes_the_look_it_is_painted_with() {
        let mut world = crate::world::World::new();
        let mut grid = Grid::filled(5, 5, TileType::Floor);
        assert!(paint(&mut world, &mut grid, (2, 2), 1, GrassKind::Meadow));
        let tile = |grid: &Grid| grid.get(2, 2).unwrap().clone();
        assert_eq!(tile(&grid).tile_type, TileType::Grass);
        assert_eq!(tile(&grid).sprite(), GrassKind::Meadow.sprite_at(2, 2));
        assert!(!paint(&mut world, &mut grid, (2, 2), 0, GrassKind::Meadow), "already that look");
        assert!(paint(&mut world, &mut grid, (2, 2), 0, GrassKind::Dry));
        assert_eq!(tile(&grid).sprite(), GrassKind::Dry.sprite_at(2, 2));
        // Indoor grass is the same terrain with the dungeon's look, and
        // painting it over an outdoor look takes that look away.
        assert!(paint(&mut world, &mut grid, (2, 2), 0, TileType::Grass));
        assert!(tile_ids::GRASS_VARIANTS.contains(&tile(&grid).sprite()));
    }

    /// Ground goes under what stands on it. Tall grass painted on a lawn
    /// keeps the lawn beneath it; ground painted under the tall grass
    /// changes the lawn, not the grass; and clearing it, or burning it away,
    /// leaves the lawn, not dungeon floor.
    #[test]
    fn ground_goes_under_what_stands_on_it() {
        let mut world = crate::world::World::new();
        let mut grid = Grid::filled(5, 5, TileType::Floor);
        let tile = |grid: &Grid| grid.get(2, 2).unwrap().clone();
        paint(&mut world, &mut grid, (2, 2), 0, GrassKind::Clover);
        let clover = tile(&grid).ground();
        paint(&mut world, &mut grid, (2, 2), 0, TileType::TallGrass);
        assert_eq!(tile(&grid).tile_type, TileType::TallGrass);
        assert_eq!(tile(&grid).ground(), clover, "the lawn is still under the tall grass");

        assert!(paint(&mut world, &mut grid, (2, 2), 0, GrassKind::Dry));
        assert_eq!(tile(&grid).tile_type, TileType::TallGrass, "still standing");
        assert_eq!(tile(&grid).ground().look, Some(GrassKind::Dry.sprite_at(2, 2)));

        // Tall grass painted over tall grass changes nothing.
        assert!(!paint(&mut world, &mut grid, (2, 2), 0, TileType::TallGrass));
        // A wall in its place stands on the same ground.
        paint(&mut world, &mut grid, (2, 2), 0, TileType::Wall);
        assert_eq!(tile(&grid).ground().look, Some(GrassKind::Dry.sprite_at(2, 2)));
        assert!(paint(&mut world, &mut grid, (2, 2), 0, Paint::ClearTop));
        assert_eq!(tile(&grid).tile_type, TileType::Grass);
        assert_eq!(tile(&grid).sprite(), GrassKind::Dry.sprite_at(2, 2));
        assert!(!paint(&mut world, &mut grid, (2, 2), 0, Paint::ClearTop), "nothing on top to clear");
    }

    /// A path runs on under tall grass growing over it: the tiles either
    /// side still join it, and its own picture still follows theirs.
    #[test]
    fn a_path_runs_on_under_tall_grass() {
        use crate::outdoor::{E, W};
        let mut world = crate::world::World::new();
        let mut grid = Grid::filled(7, 5, TileType::Grass);
        for x in 1..=5 {
            paint(&mut world, &mut grid, (x, 2), 0, TileType::Path(PathKind::Gravel));
        }
        paint(&mut world, &mut grid, (3, 2), 0, TileType::TallGrass);
        let ground = |x| grid.get(x, 2).unwrap().ground();
        assert_eq!(ground(3).tile_type, TileType::Path(PathKind::Gravel));
        assert_eq!(ground(3).look, Some(PathKind::Gravel.sprite(E | W)));
        assert_eq!(ground(2).look, Some(PathKind::Gravel.sprite(E | W)), "still joined");
    }

    /// Water covered by a wall isn't water, ripples and all, until the wall
    /// is cleared away again.
    #[test]
    fn water_under_a_wall_comes_back_when_the_wall_goes() {
        let mut world = crate::world::World::new();
        let mut grid = Grid::filled(5, 5, TileType::Floor);
        paint(&mut world, &mut grid, (2, 2), 0, TileType::Water);
        paint(&mut world, &mut grid, (2, 2), 0, TileType::Wall);
        assert!(grid.water_positions.is_empty());
        assert_eq!(world.query::<&AnimatedSprite>().iter().count(), 0);
        paint(&mut world, &mut grid, (2, 2), 0, Paint::ClearTop);
        assert_eq!(grid.water_positions, vec![(2, 2)]);
        assert_eq!(world.query::<&AnimatedSprite>().iter().count(), 1);
    }

    /// Every tile type but solid rock can be painted from one list or
    /// another, and each list holds only its own layer.
    #[test]
    fn the_lists_cover_every_terrain_by_layer() {
        let ground: Vec<Paint> = INDOOR_GROUND.iter().chain(&OUTDOOR_GRASS).chain(&OUTDOOR_PATHS).copied().collect();
        let tops: Vec<Paint> = INDOOR_ON_TOP.iter().chain(&OUTDOOR_ON_TOP).copied().collect();
        for paint in &ground {
            assert!(!matches!(paint, Paint::Terrain(t) if t.stands_on_ground()), "{paint:?} is listed as ground");
        }
        for paint in &tops {
            assert!(matches!(paint, Paint::Terrain(t) if t.stands_on_ground()), "{paint:?} is listed as on top");
        }
        let listed = |t: TileType| ground.iter().chain(&tops).any(|p| *p == Paint::Terrain(t));
        for t in [TileType::Floor, TileType::Wall, TileType::Water, TileType::Grass, TileType::TallGrass,
            TileType::Stone, TileType::StairsDown, TileType::StairsUp] {
            assert!(listed(t), "{t:?} can't be painted");
        }
        for kind in PathKind::ALL {
            assert!(listed(TileType::Path(kind)));
        }
        for kind in GrassKind::ALL {
            assert!(ground.contains(&Paint::Grass(kind)));
        }
    }
}

