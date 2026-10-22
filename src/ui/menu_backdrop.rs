//! The start screen's backdrop: a dark stone room lit by two braziers.
//!
//! Painted with egui from the game's own sheets rather than through the world
//! renderer, because the start screen has no world. It borrows the dungeon's
//! pieces instead — the stone floor and brick wall tiles, the brazier and its
//! animation, the in-game light flicker — so the menu looks like the first
//! room of a run rather than a screen in front of it.

use super::icons::UiIcons;
use super::style::{self, colors};
use crate::components::AnimatedSprite;
use crate::constants::*;
use crate::multi_tileset::MultiTileset;
use crate::tile::tile_ids;
use egui::{Color32, Painter, Pos2, Rect};

/// Where the backdrop's braziers stand: either side of the screen's centre,
/// on the first row of floor at the foot of the wall. The title sits on the
/// wall between them.
pub fn brazier_positions(screen: Rect) -> [Pos2; 2] {
    let y = screen.top() + MENU_BACKDROP_TILE_SIZE * (MENU_BACKDROP_WALL_ROWS as f32 + 0.5);
    [
        Pos2::new(screen.center().x - MENU_BRAZIER_OFFSET_X, y),
        Pos2::new(screen.center().x + MENU_BRAZIER_OFFSET_X, y),
    ]
}

/// Where a brazier's flame is: the upper part of its sprite, which is where
/// the glow and the embers come from.
fn flame(brazier: Pos2) -> Pos2 {
    brazier - egui::vec2(0.0, MENU_BACKDROP_TILE_SIZE * 0.2)
}

/// Paint the backdrop over `screen` at `time` seconds (any monotonic clock;
/// it drives the flames, the flicker and the embers).
pub fn paint_menu_backdrop(
    painter: &Painter,
    screen: Rect,
    tileset: &MultiTileset,
    icons: &UiIcons,
    time: f32,
) {
    painter.rect_filled(screen, 0.0, colors::MENU_BACKDROP);
    paint_tiles(painter, screen, tileset, icons);

    let braziers = brazier_positions(screen);
    for (i, &brazier) in braziers.iter().enumerate() {
        // Each brazier on its own phase, so the two do not pulse in step.
        let flicker = crate::render::flicker_offset(time, i as f32 * 2.3, 1.0);
        let swell = 1.0 + flicker * MENU_BRAZIER_FLICKER;
        style::paint_glow(
            painter,
            flame(brazier),
            MENU_BRAZIER_GLOW_RADIUS * swell,
            colors::TORCH_GLOW,
            MENU_BRAZIER_GLOW_STRENGTH * swell,
        );
    }

    // The same animation the braziers in the dungeon play.
    let animation = AnimatedSprite::brazier();
    let texture = icons.texture_for_sheet(animation.sheet);
    for (i, &brazier) in braziers.iter().enumerate() {
        let frames_in = (time / animation.frame_duration) as u32 + i as u32 * 3;
        let tile = animation.base_tile_id + frames_in % animation.frame_count;
        let size = egui::vec2(MENU_BACKDROP_TILE_SIZE, MENU_BACKDROP_TILE_SIZE);
        painter.image(
            texture,
            painter.round_rect_to_pixels(Rect::from_center_size(brazier, size)),
            tileset.get_egui_uv(animation.sheet, tile),
            Color32::WHITE,
        );
    }
    for (i, &brazier) in braziers.iter().enumerate() {
        paint_embers(painter, flame(brazier), time, i);
    }

    style::paint_vignette(painter, screen, colors::VIGNETTE, MENU_VIGNETTE_ALPHA, MENU_VIGNETTE_REACH);
}

/// The room: a band of brick wall across the top, stone floor below, all
/// darkened and warmed by [`colors::MENU_BACKDROP_TINT`] so the firelight
/// added on top is what shapes it.
fn paint_tiles(painter: &Painter, screen: Rect, tileset: &MultiTileset, icons: &UiIcons) {
    let tile = MENU_BACKDROP_TILE_SIZE;
    let tiles_texture = icons.texture_for_sheet(tile_ids::FLOOR.0);
    let columns = (screen.width() / tile).ceil() as i32 + 2;
    let rows = (screen.height() / tile).ceil() as i32 + 1;
    // Columns are laid out from the centre, so the room is symmetrical about
    // the title whatever the window's width.
    let left = screen.center().x - tile / 2.0 - (columns / 2) as f32 * tile;
    for row in 0..rows {
        for column in 0..columns {
            let (sheet, id) = match row as usize {
                r if r + 1 < MENU_BACKDROP_WALL_ROWS => tile_ids::WALL_TOP,
                r if r + 1 == MENU_BACKDROP_WALL_ROWS => tile_ids::WALL,
                _ => {
                    let variants = tile_ids::FLOOR_VARIANTS;
                    variants[floor_variant(column, row) % variants.len()]
                }
            };
            let min = Pos2::new(left + column as f32 * tile, screen.top() + row as f32 * tile);
            painter.image(
                tiles_texture,
                painter.round_rect_to_pixels(Rect::from_min_size(min, egui::vec2(tile, tile))),
                tileset.get_egui_uv(sheet, id),
                colors::MENU_BACKDROP_TINT,
            );
        }
    }
}

/// Which floor variant a backdrop cell gets: scattered, but the same every
/// frame and every launch, so the floor does not shimmer.
fn floor_variant(column: i32, row: i32) -> usize {
    let mut h = (column as u32).wrapping_mul(0x9E37_79B1) ^ (row as u32).wrapping_mul(0x85EB_CA77);
    h ^= h >> 15;
    h = h.wrapping_mul(0x2C1B_3C6D);
    h ^= h >> 12;
    h as usize
}

/// Embers drifting up from a flame. Each one is on a fixed, staggered cycle
/// derived from its index, so they need no state: rise, sway, fade, and
/// reappear at the flame.
fn paint_embers(painter: &Painter, flame: Pos2, time: f32, brazier_index: usize) {
    for i in 0..MENU_EMBER_COUNT {
        // Spread the embers evenly through the cycle, and offset each
        // brazier's set so the two do not mirror each other.
        let stagger = (i as f32 + brazier_index as f32 * 0.5) / MENU_EMBER_COUNT as f32;
        let age = (time / MENU_EMBER_LIFETIME + stagger).fract();
        let seed = (i * 7 + brazier_index * 13) as f32;
        let sway = (time * 1.7 + seed).sin() * MENU_EMBER_SWAY * age;
        let pos = flame + egui::vec2(sway, -age * MENU_EMBER_RISE_SPEED * MENU_EMBER_LIFETIME);
        // Bright as it leaves the flame, out by the top of its climb.
        let alpha = (1.0 - age).powf(1.5);
        let size = egui::vec2(MENU_EMBER_SIZE, MENU_EMBER_SIZE);
        painter.rect_filled(
            painter.round_rect_to_pixels(Rect::from_center_size(pos, size)),
            0.0,
            colors::EMBER.gamma_multiply(alpha),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The floor pattern must be stable from frame to frame, or the backdrop
    /// shimmers; and it must actually vary, or it is a flat grid.
    #[test]
    fn floor_variants_are_stable_and_varied() {
        assert_eq!(floor_variant(3, 4), floor_variant(3, 4));
        let picks: std::collections::HashSet<usize> =
            (0..8).flat_map(|c| (0..8).map(move |r| floor_variant(c, r) % 3)).collect();
        assert_eq!(picks.len(), 3, "every floor variant should appear");
    }

    /// The braziers flank the centre symmetrically, on the floor just below
    /// the wall.
    #[test]
    fn braziers_flank_the_centre_at_the_foot_of_the_wall() {
        let screen = Rect::from_min_size(Pos2::ZERO, egui::vec2(1280.0, 720.0));
        let [left, right] = brazier_positions(screen);
        assert_eq!(left.y, right.y);
        assert!((screen.center().x - left.x - (right.x - screen.center().x)).abs() < 0.01);
        let wall_bottom = MENU_BACKDROP_TILE_SIZE * MENU_BACKDROP_WALL_ROWS as f32;
        assert!(left.y > wall_bottom && left.y < wall_bottom + MENU_BACKDROP_TILE_SIZE);
    }
}
