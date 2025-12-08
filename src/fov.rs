use crate::grid::Grid;
use std::collections::HashSet;

/// Field of View calculator using recursive shadowcasting
///
/// This algorithm divides the field of view into 8 octants and scans each one
/// row-by-row, tracking which portions are blocked by obstacles. It's more
/// accurate than raycasting (no missed corners) and faster (O(visible tiles)
/// instead of O(rays × radius)).
pub struct FOV;

impl FOV {
    /// Calculate visible tiles from a given position with a radius.
    /// `entity_blocks_vision` is an optional callback to check if an entity at (x,y) blocks vision.
    pub fn calculate<F>(
        grid: &Grid,
        origin_x: i32,
        origin_y: i32,
        radius: i32,
        entity_blocks_vision: Option<F>,
    ) -> Vec<(i32, i32)>
    where
        F: Fn(i32, i32) -> bool,
    {
        let mut visible = HashSet::new();

        // Origin is always visible
        visible.insert((origin_x, origin_y));

        // Process all 8 octants
        // Each octant is defined by how we transform coordinates:
        // (row, col) -> (dx, dy) relative to origin
        for octant in 0..8 {
            cast_light(
                grid,
                &mut visible,
                origin_x,
                origin_y,
                radius,
                1,           // start at row 1 (row 0 is origin)
                1.0,         // start slope
                0.0,         // end slope
                octant,
                &entity_blocks_vision,
            );
        }

        visible.into_iter().collect()
    }
}

/// Transform (row, col) coordinates based on octant to get (dx, dy)
///
/// Octants are numbered 0-7, starting from the top and going clockwise:
///   \1|2/
///   0\|/3
///   --@--
///   7/|\4
///   /6|5\
#[inline]
fn transform(octant: u8, row: i32, col: i32) -> (i32, i32) {
    match octant {
        0 => (-col, -row),  // NNW
        1 => (-row, -col),  // WNW
        2 => (-row, col),   // ENE
        3 => (col, -row),   // NNE
        4 => (col, row),    // SSE
        5 => (row, col),    // ESE
        6 => (row, -col),   // WSW
        7 => (-col, row),   // SSW
        _ => unreachable!(),
    }
}

/// Recursively cast light in one octant using shadowcasting
///
/// - `row`: current row being scanned (distance from origin)
/// - `start_slope`: slope of the left edge of the visible area (1.0 = 45°)
/// - `end_slope`: slope of the right edge of the visible area (0.0 = straight)
fn cast_light<F>(
    grid: &Grid,
    visible: &mut HashSet<(i32, i32)>,
    origin_x: i32,
    origin_y: i32,
    radius: i32,
    row: i32,
    mut start_slope: f32,
    end_slope: f32,
    octant: u8,
    entity_blocks_vision: &Option<F>,
) where
    F: Fn(i32, i32) -> bool,
{
    if start_slope < end_slope || row > radius {
        return;
    }

    let mut prev_blocked = false;
    let mut saved_start_slope = start_slope;

    // Scan columns in this row from start_slope to end_slope
    let min_col = (row as f32 * end_slope).floor() as i32;
    let max_col = (row as f32 * start_slope).ceil() as i32;

    for col in (min_col..=max_col).rev() {
        let (dx, dy) = transform(octant, row, col);
        let x = origin_x + dx;
        let y = origin_y + dy;

        // Check if within radius (using squared distance for speed)
        let dist_sq = dx * dx + dy * dy;
        let radius_sq = radius * radius;
        if dist_sq > radius_sq {
            continue;
        }

        // Calculate slopes for this cell
        let left_slope = (col as f32 + 0.5) / (row as f32 - 0.5);
        let right_slope = (col as f32 - 0.5) / (row as f32 + 0.5);

        // Skip if entirely outside our visible cone
        if right_slope > start_slope {
            continue;
        }
        if left_slope < end_slope {
            break;
        }

        // This tile is visible
        visible.insert((x, y));

        // Check if this tile blocks vision (tile or entity)
        let tile_blocks = grid
            .get(x, y)
            .map(|t| t.tile_type.blocks_vision())
            .unwrap_or(true);
        let entity_blocks = entity_blocks_vision
            .as_ref()
            .map(|f| f(x, y))
            .unwrap_or(false);
        let is_blocked = tile_blocks || entity_blocks;

        if prev_blocked {
            if is_blocked {
                // Still in shadow, update where shadow starts
                saved_start_slope = right_slope;
            } else {
                // Exiting shadow
                prev_blocked = false;
                start_slope = saved_start_slope;
            }
        } else if is_blocked {
            // Entering shadow - recurse with the visible portion before this blocker
            prev_blocked = true;
            cast_light(
                grid,
                visible,
                origin_x,
                origin_y,
                radius,
                row + 1,
                start_slope,
                left_slope,
                octant,
                entity_blocks_vision,
            );
            saved_start_slope = right_slope;
        }
    }

    // Continue to next row if we didn't end in shadow
    if !prev_blocked {
        cast_light(
            grid,
            visible,
            origin_x,
            origin_y,
            radius,
            row + 1,
            start_slope,
            end_slope,
            octant,
            entity_blocks_vision,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tile::{Tile, TileType};

    fn make_grid(width: usize, height: usize, walls: &[(i32, i32)]) -> Grid {
        let mut grid = Grid {
            width,
            height,
            tiles: vec![Tile::new(TileType::Floor); width * height],
            chest_positions: vec![],
            door_positions: vec![],
            decals: vec![],
            stairs_up_pos: None,
            stairs_down_pos: None,
        };
        for &(x, y) in walls {
            if let Some(tile) = grid.get_mut(x, y) {
                tile.tile_type = TileType::Wall;
            }
        }
        grid
    }

    #[test]
    fn test_origin_always_visible() {
        let grid = make_grid(10, 10, &[]);
        let visible = FOV::calculate(&grid, 5, 5, 3, None::<fn(i32, i32) -> bool>);
        assert!(visible.contains(&(5, 5)));
    }

    #[test]
    fn test_adjacent_tiles_visible() {
        let grid = make_grid(10, 10, &[]);
        let visible = FOV::calculate(&grid, 5, 5, 3, None::<fn(i32, i32) -> bool>);
        // All 4 adjacent tiles should be visible
        assert!(visible.contains(&(5, 6)));
        assert!(visible.contains(&(5, 4)));
        assert!(visible.contains(&(6, 5)));
        assert!(visible.contains(&(4, 5)));
    }

    #[test]
    fn test_wall_blocks_vision() {
        // Wall at (5, 6), should block (5, 7) and beyond
        let grid = make_grid(10, 10, &[(5, 6)]);
        let visible = FOV::calculate(&grid, 5, 5, 5, None::<fn(i32, i32) -> bool>);

        // Wall itself is visible
        assert!(visible.contains(&(5, 6)));
        // Tile behind wall is not visible
        assert!(!visible.contains(&(5, 7)));
    }

    #[test]
    fn test_radius_limit() {
        let grid = make_grid(20, 20, &[]);
        let visible = FOV::calculate(&grid, 10, 10, 3, None::<fn(i32, i32) -> bool>);

        // Tile at distance 3 should be visible
        assert!(visible.contains(&(10, 13)));
        // Tile at distance 5 should not be visible
        assert!(!visible.contains(&(10, 15)));
    }
}
