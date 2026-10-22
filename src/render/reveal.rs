//! Tiles fade into view and back out, rather than popping, and the edge of
//! what you can see blends softly into the fog of war.
//!
//! Visibility itself (`Tile::visible`) changes at once, for play: this is
//! only how it's drawn. Each tile has a reveal amount, 0 to 1, that eases
//! toward 1 while it's in view and toward 0 when it isn't, in real time like
//! the fire fades (`systems::fire_fade`). The renderer hands the shader a map
//! of it (`seen_map`), softened at the edges and sampled smoothly between
//! tiles, and the shader blends each pixel between a remembered tile's light
//! and its own by it: tiles brighten straight out of the fog and settle
//! straight back into it, however often they flip in and out of view, and
//! the edge of your sight is a gradient rather than a line.

use crate::constants::{LIGHT_FOG_EDGE_SPREAD, REVEAL_FADE_IN_SECONDS, REVEAL_FADE_OUT_SECONDS};
use crate::grid::Grid;

#[derive(Debug, Default)]
pub struct RevealFade {
    width: usize,
    height: usize,
    amount: Vec<f32>,
}

impl RevealFade {
    /// Ease every tile toward in view or not, over `dt` real seconds. A map
    /// of a new size (another floor) starts from nothing, so it fades in.
    pub fn update(&mut self, grid: &Grid, dt: f32) {
        if (self.width, self.height) != (grid.width, grid.height) {
            *self = RevealFade { width: grid.width, height: grid.height, amount: vec![0.0; grid.tiles.len()] };
        }
        let step = |seconds: f32| if seconds > 0.0 { dt / seconds } else { 1.0 };
        let (up, down) = (step(REVEAL_FADE_IN_SECONDS), step(REVEAL_FADE_OUT_SECONDS));
        for (amount, tile) in self.amount.iter_mut().zip(&grid.tiles) {
            *amount = if tile.visible { (*amount + up).min(1.0) } else { (*amount - down).max(0.0) };
        }
    }

    /// How seen each tile of a `width` x `height` map looks, 0 (remembered)
    /// to 1 (in full view), row-major from (0, 0): its reveal amount, eased,
    /// with a share of the most seen tile beside it spilling over
    /// (`LIGHT_FOG_EDGE_SPREAD`), so a remembered tile at the edge of your
    /// sight is partly lit and what you see is never dimmed. A map it hasn't
    /// caught up with yet (the first frame of a floor) is all in view.
    pub fn seen_map(&self, width: usize, height: usize) -> Vec<f32> {
        if (self.width, self.height) != (width, height) {
            return vec![1.0; width * height];
        }
        let eased: Vec<f32> = self.amount.iter().map(|&a| ease(a)).collect();
        let at = |x: i32, y: i32| {
            if x < 0 || y < 0 || x as usize >= width || y as usize >= height {
                0.0
            } else {
                eased[y as usize * width + x as usize]
            }
        };
        let mut out = eased.clone();
        for y in 0..height as i32 {
            for x in 0..width as i32 {
                let mut beside: f32 = 0.0;
                for (dx, dy) in [(1, 0), (-1, 0), (0, 1), (0, -1)] {
                    beside = beside.max(at(x + dx, y + dy));
                }
                for (dx, dy) in [(1, 1), (-1, 1), (1, -1), (-1, -1)] {
                    beside = beside.max(at(x + dx, y + dy) * std::f32::consts::FRAC_1_SQRT_2);
                }
                let i = y as usize * width + x as usize;
                out[i] = out[i].max(beside * LIGHT_FOG_EDGE_SPREAD);
            }
        }
        out
    }
}

fn ease(t: f32) -> f32 {
    t * t * (3.0 - 2.0 * t)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tile::TileType;

    fn seen(fade: &RevealFade, grid: &Grid, x: usize, y: usize) -> f32 {
        fade.seen_map(grid.width, grid.height)[y * grid.width + x]
    }

    #[test]
    fn a_tile_fades_in_as_it_comes_into_view_and_out_as_it_leaves() {
        let mut grid = Grid::filled(3, 3, TileType::Floor);
        let mut fade = RevealFade::default();
        fade.update(&grid, 0.0);
        grid.get_mut(1, 1).unwrap().visible = true;
        fade.update(&grid, REVEAL_FADE_IN_SECONDS / 2.0);
        let half_in = seen(&fade, &grid, 1, 1);
        assert!(half_in > 0.0 && half_in < 1.0, "half way in: {half_in}");
        fade.update(&grid, REVEAL_FADE_IN_SECONDS);
        assert_eq!(seen(&fade, &grid, 1, 1), 1.0, "fully in");

        grid.get_mut(1, 1).unwrap().visible = false;
        fade.update(&grid, REVEAL_FADE_OUT_SECONDS / 2.0);
        let half_out = seen(&fade, &grid, 1, 1);
        assert!(half_out > 0.0 && half_out < 1.0);
        fade.update(&grid, REVEAL_FADE_OUT_SECONDS);
        assert_eq!(seen(&fade, &grid, 1, 1), 0.0, "remembered");
    }

    /// Flipping in and out of view mid-fade carries on from where it was,
    /// never jumping: it only ever moves by a frame's worth.
    #[test]
    fn flipping_in_and_out_of_view_never_jumps() {
        let mut grid = Grid::filled(3, 3, TileType::Floor);
        let mut fade = RevealFade::default();
        fade.update(&grid, 0.0);
        let dt = 1.0 / 60.0;
        let mut last = seen(&fade, &grid, 1, 1);
        for frame in 0..120 {
            grid.get_mut(1, 1).unwrap().visible = (frame / 7) % 2 == 0;
            fade.update(&grid, dt);
            let now = seen(&fade, &grid, 1, 1);
            // A frame moves the amount by dt over the fade time; the easing
            // is at most 1.5 times as steep.
            let most = 1.5 * dt / REVEAL_FADE_IN_SECONDS.min(REVEAL_FADE_OUT_SECONDS) + 1e-4;
            assert!((now - last).abs() <= most, "frame {frame}: {last} -> {now}");
            last = now;
        }
    }

    /// The edge of sight is soft on the remembered side only: a remembered
    /// tile beside one in view is partly lit, one further off isn't, and a
    /// lone tile in view keeps its full light.
    #[test]
    fn the_edge_of_sight_spills_into_the_fog_without_dimming_what_you_see() {
        let mut grid = Grid::filled(5, 1, TileType::Floor);
        let mut fade = RevealFade::default();
        fade.update(&grid, 0.0);
        grid.get_mut(0, 0).unwrap().visible = true;
        fade.update(&grid, REVEAL_FADE_IN_SECONDS * 2.0);
        let map = fade.seen_map(5, 1);
        assert_eq!(map[0], 1.0, "what you see keeps its full light");
        assert!((map[1] - LIGHT_FOG_EDGE_SPREAD).abs() < 1e-6, "beside it, lifted: {}", map[1]);
        assert_eq!(map[2], 0.0, "further off, remembered");
    }
}
