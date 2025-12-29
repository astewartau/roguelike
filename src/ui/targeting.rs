//! Targeting overlay UI component.
//!
//! Displays targeting indicators for abilities like Blink and Fireball.

use crate::camera::Camera;
use crate::components::{ItemType, Position};
use crate::input::TargetingMode;
use hecs::World;

/// Data needed for the targeting overlay
pub struct TargetingOverlayData {
    pub player_x: i32,
    pub player_y: i32,
    pub cursor_x: i32,
    pub cursor_y: i32,
    pub max_range: i32,
    pub radius: i32,
    pub is_blink: bool,
    pub item_type: Option<ItemType>,
}

/// Extract targeting overlay data from targeting mode and world state
pub fn get_targeting_overlay_data(
    world: &World,
    player_entity: hecs::Entity,
    targeting_mode: Option<&TargetingMode>,
    cursor_screen_pos: (f32, f32),
    camera: &Camera,
) -> Option<TargetingOverlayData> {
    let targeting = targeting_mode?;

    let player_pos = world.get::<&Position>(player_entity).ok()?;

    // Convert screen cursor to world coordinates
    let world_pos = camera.screen_to_world(cursor_screen_pos.0, cursor_screen_pos.1);
    let cursor_x = world_pos.x.floor() as i32;
    let cursor_y = world_pos.y.floor() as i32;

    Some(TargetingOverlayData {
        player_x: player_pos.x,
        player_y: player_pos.y,
        cursor_x,
        cursor_y,
        max_range: targeting.max_range,
        radius: targeting.radius,
        is_blink: matches!(targeting.item_type, ItemType::ScrollOfBlink),
        item_type: Some(targeting.item_type),
    })
}

/// Draw the targeting overlay when in targeting mode
pub fn draw_targeting_overlay(ctx: &egui::Context, camera: &Camera, data: &TargetingOverlayData) {
    // Convert from screen pixels to egui points
    let ppp = ctx.pixels_per_point();

    // Tile size: camera.zoom pixels = 1 world unit, convert to egui points
    let tile_size = camera.zoom / ppp;

    // Get the painter for the foreground layer
    let painter = ctx.layer_painter(egui::LayerId::new(
        egui::Order::Foreground,
        egui::Id::new("targeting_overlay"),
    ));

    // Helper to convert world position to egui rect for a tile
    let tile_rect = |world_x: i32, world_y: i32| {
        let screen_pos = camera.world_to_screen(world_x as f32, world_y as f32);
        let egui_x = screen_pos.0 / ppp;
        let egui_y = screen_pos.1 / ppp;
        egui::Rect::from_min_size(
            egui::pos2(egui_x, egui_y - tile_size),
            egui::vec2(tile_size, tile_size),
        )
    };

    // Draw tiles in range with a subtle highlight
    let range_color = egui::Color32::from_rgba_unmultiplied(100, 150, 255, 40);

    for dx in -data.max_range..=data.max_range {
        for dy in -data.max_range..=data.max_range {
            // Use Chebyshev distance for range check
            let dist = dx.abs().max(dy.abs());
            if dist > data.max_range {
                continue;
            }

            let tile_x = data.player_x + dx;
            let tile_y = data.player_y + dy;

            let rect = tile_rect(tile_x, tile_y);
            painter.rect_filled(rect, 0.0, range_color);
        }
    }

    // Calculate distance from player to cursor
    let cursor_dist =
        (data.cursor_x - data.player_x).abs().max((data.cursor_y - data.player_y).abs());
    let in_range = cursor_dist <= data.max_range;

    // Draw cursor tile highlight
    let cursor_color = if in_range {
        if data.is_blink {
            egui::Color32::from_rgba_unmultiplied(100, 255, 100, 120) // Green for blink
        } else {
            egui::Color32::from_rgba_unmultiplied(255, 150, 50, 120) // Orange for fireball
        }
    } else {
        egui::Color32::from_rgba_unmultiplied(255, 50, 50, 120) // Red for out of range
    };

    let cursor_rect = tile_rect(data.cursor_x, data.cursor_y);
    painter.rect_filled(cursor_rect, 0.0, cursor_color);
    painter.rect_stroke(
        cursor_rect,
        0.0,
        egui::Stroke::new(2.0, egui::Color32::WHITE),
    );

    // For Fireball, draw the AoE radius around cursor
    if !data.is_blink && data.radius > 0 && in_range {
        let aoe_color = egui::Color32::from_rgba_unmultiplied(255, 100, 50, 60);

        for dx in -data.radius..=data.radius {
            for dy in -data.radius..=data.radius {
                // Skip the center tile (already highlighted)
                if dx == 0 && dy == 0 {
                    continue;
                }

                // Chebyshev distance for AoE
                let dist = dx.abs().max(dy.abs());
                if dist > data.radius {
                    continue;
                }

                let aoe_x = data.cursor_x + dx;
                let aoe_y = data.cursor_y + dy;

                let aoe_rect = tile_rect(aoe_x, aoe_y);
                painter.rect_filled(aoe_rect, 0.0, aoe_color);
            }
        }
    }

    // Draw info text near the cursor
    let info_text = if in_range {
        match data.item_type {
            Some(ItemType::ScrollOfBlink) => "Click to teleport",
            Some(ItemType::ScrollOfFireball) => "Click to cast fireball",
            Some(ItemType::HealthPotion) => "Click to throw healing potion",
            Some(ItemType::RegenerationPotion) => "Click to throw regen potion",
            Some(ItemType::StrengthPotion) => "Click to throw strength potion",
            Some(ItemType::ConfusionPotion) => "Click to throw confusion potion",
            _ => {
                if data.is_blink {
                    "Click to teleport"
                } else {
                    "Click to use"
                }
            }
        }
    } else {
        "Out of range"
    };

    let text_color = if in_range {
        egui::Color32::WHITE
    } else {
        egui::Color32::from_rgb(255, 100, 100)
    };

    // Position text above the cursor tile
    let text_pos = egui::pos2(cursor_rect.center().x, cursor_rect.min.y - 5.0);

    painter.text(
        text_pos,
        egui::Align2::CENTER_BOTTOM,
        info_text,
        egui::FontId::monospace(14.0),
        text_color,
    );
}
