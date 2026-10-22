//! The sandbox's starting map: a stretch of countryside to try things in.
//!
//! Outdoors under the sky: wild grass, a wood round the edges and groves
//! here and there, a stone cottage (lantern, chest, oil barrel), a cobbled
//! road across the middle, a dirt track up to the cottage door with
//! flagstones before it, a gravel path down to a pond, a campfire in a
//! clearing, and tall grass to hide in. Everything systemic is in reach:
//! trees and grass burn, the pond and rain douse, the barrel explodes, the
//! wood hides you and the night is dark.
//!
//! Laid out the same every time (`SANDBOX_LAYOUT_SEED`). The ground is put
//! down with the Paint tab's own painting (`devtools::paint`), so it's just
//! what painting it by hand would give.
//!
//! Coordinates are tiles, up the screen is +y.

use rand::rngs::StdRng;
use rand::{Rng, SeedableRng};

use crate::components::LanternStyle;
use crate::constants::SANDBOX_LAYOUT_SEED;
use crate::devtools::paint::{paint, Paint};
use crate::grid::Grid;
use crate::outdoor::PathKind;
use crate::tile::TileType;
use crate::world::World;

/// The cottage's outer walls, inclusive: (x0, y0, x1, y1).
const COTTAGE: (i32, i32, i32, i32) = (25, 17, 32, 23);
/// Its door, in the middle of its front (bottom) wall.
const COTTAGE_DOOR: (i32, i32) = (28, 17);
/// The road's rows, running the whole width.
const ROAD_ROWS: (i32, i32) = (9, 10);
/// Flagstones before the cottage door, inclusive: (x0, y0, x1, y1).
const DOORSTEP: (i32, i32, i32, i32) = (27, 15, 29, 16);
/// The dirt track from the road up to the doorstep: its column, rows.
const TRACK: (i32, i32, i32) = (28, 11, 14);
/// The gravel path from the road towards the pond: its column, rows.
const POND_PATH: (i32, i32, i32) = (12, 11, 18);
/// The pond: its centre and radii.
const POND: (f32, f32, f32, f32) = (11.0, 21.0, 4.0, 2.6);
/// The campfire, in a clearing south of the road.
const CAMPFIRE: (i32, i32) = (19, 5);
/// How far round the campfire is kept clear of trees.
const CLEARING_RADIUS: f32 = 3.5;
/// Patches of tall grass: centre and radius.
const TALL_GRASS: [(f32, f32, f32); 3] = [(5.0, 4.0, 2.5), (35.0, 4.0, 2.0), (35.0, 27.0, 1.8)];
/// How deep the wood round the edges is, in tiles.
const WOOD_DEPTH: i32 = 2;
/// Chance a tile in the edge wood has a tree, and one out in the open does
/// where the groves are.
const WOOD_DENSITY: f64 = 0.6;
const GROVE_DENSITY: f64 = 0.4;
/// Of the trees, the share that are young (they don't block sight).
const YOUNG_TREES: f64 = 0.2;

/// Lay the sandbox's map out on `grid` (already the sandbox's size), and
/// put its things in `world`.
pub fn lay_out(world: &mut World, grid: &mut Grid) {
    let (w, h) = (grid.width as i32, grid.height as i32);
    let lay = |world: &mut World, grid: &mut Grid, at: (i32, i32), what: Paint| {
        paint(world, grid, at, 0, what);
    };

    // Grass everywhere, then the pond.
    for y in 0..h {
        for x in 0..w {
            lay(world, grid, (x, y), Paint::WildGrass);
        }
    }
    for y in 0..h {
        for x in 0..w {
            if in_pond(x, y) {
                lay(world, grid, (x, y), Paint::Terrain(TileType::Water));
            }
        }
    }

    // The ways about: the road, the track to the cottage and its doorstep,
    // the path to the pond.
    let path = |kind| Paint::Terrain(TileType::Path(kind));
    for y in ROAD_ROWS.0..=ROAD_ROWS.1 {
        for x in 0..w {
            lay(world, grid, (x, y), path(PathKind::Cobbles));
        }
    }
    for y in TRACK.1..=TRACK.2 {
        lay(world, grid, (TRACK.0, y), path(PathKind::Dirt));
    }
    for y in DOORSTEP.1..=DOORSTEP.3 {
        for x in DOORSTEP.0..=DOORSTEP.2 {
            lay(world, grid, (x, y), path(PathKind::Flagstones));
        }
    }
    for y in POND_PATH.1..=POND_PATH.2 {
        lay(world, grid, (POND_PATH.0, y), path(PathKind::Gravel));
    }

    // The cottage: stone walls round a floor, a door in the front.
    let (x0, y0, x1, y1) = COTTAGE;
    for y in y0..=y1 {
        for x in x0..=x1 {
            let edge = x == x0 || x == x1 || y == y0 || y == y1;
            let what = if edge { TileType::Wall } else { TileType::Floor };
            if edge {
                lay(world, grid, (x, y), Paint::Terrain(TileType::Floor));
            }
            lay(world, grid, (x, y), Paint::Terrain(what));
        }
    }
    lay(world, grid, COTTAGE_DOOR, Paint::ClearTop);
    let mut rng = StdRng::seed_from_u64(SANDBOX_LAYOUT_SEED);
    spawn_door(world, COTTAGE_DOOR);
    crate::spawning::spawn_lantern(world, x0 + 1, y1 - 1, LanternStyle::Warm);
    spawn_chest(world, (x1 - 1, y1 - 1), &mut rng);
    crate::spawning::spawn_oil_barrel(world, x1 - 1, y0 + 1);

    // A campfire in a clearing, and tall grass to hide in.
    crate::spawning::spawn_campfire(world, CAMPFIRE.0, CAMPFIRE.1);
    for y in 0..h {
        for x in 0..w {
            if TALL_GRASS.iter().any(|&(cx, cy, r)| dist(x, y, cx, cy) <= r) && !reserved(x, y) {
                lay(world, grid, (x, y), Paint::Terrain(TileType::TallGrass));
            }
        }
    }

    // The wood round the edges and groves out in the open.
    for y in 0..h {
        for x in 0..w {
            let edge = x.min(y).min(w - 1 - x).min(h - 1 - y) < WOOD_DEPTH;
            let grove = in_grove(x, y);
            let chance = if edge { WOOD_DENSITY } else if grove { GROVE_DENSITY } else { 0.0 };
            if reserved(x, y) || !grid.is_walkable(x, y) || grid.get(x, y).is_some_and(|t| t.tile_type == TileType::TallGrass) {
                continue;
            }
            if rng.gen_bool(chance) {
                crate::spawning::spawn_tree(world, x, y, rng.gen_bool(YOUNG_TREES));
            }
        }
    }

    grid.outdoors = true;
    for tile in &mut grid.tiles {
        tile.explored = true;
    }
}

/// A closed wooden door, as the Spawn panel's.
fn spawn_door(world: &mut World, (x, y): (i32, i32)) {
    use crate::components::{BlocksMovement, BlocksVision, Door, Position, Sprite, VisualPosition};
    let door = Door::new();
    let pos = Position::new(x, y);
    world.spawn((pos, VisualPosition::from_position(&pos), Sprite::from_ref(door.closed_sprite), door, BlocksVision, BlocksMovement));
}

/// A closed chest of a first floor's loot.
fn spawn_chest(world: &mut World, (x, y): (i32, i32), rng: &mut StdRng) {
    use crate::components::{BlocksMovement, Name, Position, Sprite, VisualPosition};
    let container = crate::engine::initialization::generate_chest_contents(world, 0, rng);
    let pos = Position::new(x, y);
    world.spawn((
        pos,
        VisualPosition::from_position(&pos),
        Sprite::from_ref(crate::tile::tile_ids::CHEST_CLOSED),
        container,
        Name::new("Chest"),
        BlocksMovement,
    ));
}

fn dist(x: i32, y: i32, cx: f32, cy: f32) -> f32 {
    ((x as f32 - cx).powi(2) + (y as f32 - cy).powi(2)).sqrt()
}

fn in_pond(x: i32, y: i32) -> bool {
    let (cx, cy, rx, ry) = POND;
    ((x as f32 - cx) / rx).powi(2) + ((y as f32 - cy) / ry).powi(2) <= 1.0
}

/// Groves out in the open: two loose clumps of wood.
fn in_grove(x: i32, y: i32) -> bool {
    dist(x, y, 20.0, 24.0) <= 4.0 || dist(x, y, 33.0, 13.0) <= 2.5 || dist(x, y, 6.0, 14.0) <= 3.0
}

/// Kept clear of trees: the ways about and a tile either side, the cottage
/// and a margin round it, the pond's banks, the campfire's clearing.
fn reserved(x: i32, y: i32) -> bool {
    let near = |a: i32, lo: i32, hi: i32, margin: i32| a >= lo - margin && a <= hi + margin;
    let road = near(y, ROAD_ROWS.0, ROAD_ROWS.1, 1);
    let track = near(x, TRACK.0, TRACK.0, 1) && near(y, TRACK.1, TRACK.2, 0);
    let doorstep = near(x, DOORSTEP.0, DOORSTEP.2, 1) && near(y, DOORSTEP.1, DOORSTEP.3, 1);
    let pond_path = near(x, POND_PATH.0, POND_PATH.0, 1) && near(y, POND_PATH.1, POND_PATH.2, 0);
    let (x0, y0, x1, y1) = COTTAGE;
    let cottage = near(x, x0, x1, 2) && near(y, y0, y1, 2);
    let (cx, cy, rx, ry) = POND;
    let banks = ((x as f32 - cx) / (rx + 1.5)).powi(2) + ((y as f32 - cy) / (ry + 1.5)).powi(2) <= 1.0;
    let clearing = dist(x, y, CAMPFIRE.0 as f32, CAMPFIRE.1 as f32) <= CLEARING_RADIUS;
    road || track || doorstep || pond_path || cottage || banks || clearing
}

#[cfg(test)]
mod tests {
    use crate::components::{BlocksMovement, BlocksVision, Door, Fuel, Name, Position};
    use crate::engine::GameState;
    use crate::outdoor::PathKind;
    use crate::tile::TileType;

    /// The sandbox starts as countryside: outdoors, grass and paths with
    /// their looks, a pond, a cottage with a door, and trees that burn and
    /// hide you, none of them in the way of the road.
    #[test]
    fn the_sandbox_starts_as_countryside() {
        let state = GameState::new_sandbox(crate::components::PlayerClass::Fighter);
        let (world, grid) = (&state.world, &state.grid);
        assert!(grid.outdoors);
        assert!(grid.tiles.iter().all(|t| t.explored));
        assert!(!grid.water_positions.is_empty(), "a pond");
        let kinds = |k: TileType| grid.tiles.iter().filter(|t| t.tile_type == k).count();
        assert!(kinds(TileType::Grass) > grid.tiles.len() / 2, "mostly grass");
        assert!(kinds(TileType::TallGrass) > 0 && kinds(TileType::Wall) > 0);
        for kind in PathKind::ALL {
            assert!(kinds(TileType::Path(kind)) > 0, "a {kind:?} path");
        }
        assert!(
            grid.tiles.iter().filter(|t| matches!(t.tile_type, TileType::Path(_) | TileType::Grass)).all(|t| t.sprite_override.is_some()),
            "every grass tile and path has its outdoor look"
        );
        assert_eq!(world.query::<(&Position, &Door)>().iter().count(), 1, "the cottage door");

        let trees: Vec<(i32, i32)> = world
            .query::<(&Position, &Name, &Fuel)>()
            .iter()
            .filter(|(_, (_, n, _))| n.0.contains("ree"))
            .map(|(_, (p, _, _))| (p.x, p.y))
            .collect();
        assert!(trees.len() > 40, "a wood: {} trees", trees.len());
        assert!(world.query::<&BlocksVision>().with::<&Fuel>().iter().count() > 0, "grown trees hide you");
        for &(x, y) in &trees {
            assert!(!matches!(grid.get(x, y).unwrap().tile_type, TileType::Path(_)), "no tree on a path at {x},{y}");
        }
        // The road is clear end to end.
        let blocked: std::collections::HashSet<(i32, i32)> =
            world.query::<(&Position, &BlocksMovement)>().iter().map(|(_, (p, _))| (p.x, p.y)).collect();
        for x in 0..grid.width as i32 {
            assert!(!blocked.contains(&(x, super::ROAD_ROWS.0)), "the road is blocked at {x}");
        }
    }
}
