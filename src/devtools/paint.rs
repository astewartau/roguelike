//! Painting terrain: setting tiles by hand, with the follow-ups a terrain
//! change needs (see `systems::fire`, where burnt grass turns to floor).

use crate::components::{AnimatedSprite, Position, VisualPosition};
use crate::grid::Grid;
use crate::tile::{tile_ids, TileType};

/// The tiles the panel can paint.
pub const PAINTABLE: [TileType; 8] = [
    TileType::Floor,
    TileType::Wall,
    TileType::Water,
    TileType::Grass,
    TileType::TallGrass,
    TileType::Stone,
    TileType::StairsDown,
    TileType::StairsUp,
];

/// Paint `tile` over the square of side `2 * brush + 1` centred on `at`.
/// Keeps the water list and its animation entities in step and moves the
/// stairs markers; the caller sets `fov_dirty`. Returns whether anything
/// changed.
pub fn paint(world: &mut crate::world::World, grid: &mut Grid, at: (i32, i32), brush: i32, tile: TileType) -> bool {
    let mut changed = false;
    for dy in -brush..=brush {
        for dx in -brush..=brush {
            changed |= paint_tile(world, grid, (at.0 + dx, at.1 + dy), tile);
        }
    }
    changed
}

fn paint_tile(world: &mut crate::world::World, grid: &mut Grid, (x, y): (i32, i32), tile: TileType) -> bool {
    let Some(cell) = grid.get_mut(x, y) else {
        return false;
    };
    if cell.tile_type == tile {
        return false;
    }
    let was = cell.tile_type;
    cell.tile_type = tile;
    // Walls carry an oriented sprite for their neighbours; whatever is
    // painted now draws as itself.
    cell.sprite_override = None;
    cell.explored = true;

    if was == TileType::Water {
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
    if tile == TileType::Water {
        grid.water_positions.push((x, y));
        let pos = Position::new(x, y);
        world.spawn((pos, VisualPosition::from_position(&pos), AnimatedSprite::water()));
    }
    match tile {
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
}
