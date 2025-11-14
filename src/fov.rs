use crate::grid::Grid;

/// Field of View calculator using simple raycasting
pub struct FOV;

impl FOV {
    /// Calculate visible tiles from a given position with a radius
    pub fn calculate(grid: &Grid, origin_x: i32, origin_y: i32, radius: i32) -> Vec<(i32, i32)> {
        let mut visible = Vec::new();

        // Origin is always visible
        visible.push((origin_x, origin_y));

        // Cast rays in a circle around the origin
        let num_rays = 360;
        for i in 0..num_rays {
            let angle = (i as f32) * (std::f32::consts::PI * 2.0) / (num_rays as f32);
            let dx = angle.cos();
            let dy = angle.sin();

            // Cast ray outward
            for step in 1..=radius {
                let x = origin_x + (dx * step as f32).round() as i32;
                let y = origin_y + (dy * step as f32).round() as i32;

                // Check if tile is in the grid
                if let Some(tile) = grid.get(x, y) {
                    // Add to visible list if not already there
                    if !visible.contains(&(x, y)) {
                        visible.push((x, y));
                    }

                    // Stop ray if it hits a wall
                    if tile.tile_type.blocks_vision() {
                        break;
                    }
                } else {
                    // Out of bounds - stop ray
                    break;
                }
            }
        }

        visible
    }
}
