//! Ability bar UI component.
//!
//! Displays the player's class ability button with cooldown indicator.

use super::icons::UiIcons;
use super::style;
use crate::components::AbilityType;

/// Data needed to render the ability bar
pub struct AbilityBarData {
    pub ability_type: AbilityType,
    pub cooldown_remaining: f32,
    pub cooldown_total: f32,
    pub can_use: bool, // has energy and off cooldown
    pub viewport_height: f32,
}

/// Render the ability bar at the bottom of the screen.
/// Returns true if the ability button was clicked.
pub fn draw_ability_bar(ctx: &egui::Context, data: &AbilityBarData, icons: &UiIcons) -> bool {
    let mut clicked = false;

    egui::Window::new("Ability")
        .fixed_pos([10.0, data.viewport_height - 90.0])
        .fixed_size([80.0, 80.0])
        .title_bar(false)
        .frame(style::dungeon_window_frame())
        .show(ctx, |ui| {
            ui.vertical_centered(|ui| {
                // Get the appropriate icon UV based on ability type
                let (uv, tooltip) = match data.ability_type {
                    AbilityType::Cleave => (
                        icons.cleave_uv,
                        "Cleave\nAttack all adjacent enemies\n\n[Q]",
                    ),
                    AbilityType::Sprint => (
                        icons.sprint_uv,
                        "Sprint\nDouble movement speed for 10s\n\n[Q]",
                    ),
                    AbilityType::Tame => (
                        icons.heart_uv,
                        "Tame Animal\nChannel to tame a nearby animal\n\n[Q]",
                    ),
                    AbilityType::Barkskin => (
                        icons.barkskin_uv,
                        "Barkskin\n50% damage reduction for 15s\n\n[E]",
                    ),
                };

                // Create the button size
                let button_size = egui::vec2(48.0, 48.0);

                // Allocate space for the button
                let (rect, response) = ui.allocate_exact_size(button_size, egui::Sense::click());

                // Draw background
                let bg_color = if !data.can_use {
                    egui::Color32::from_rgb(30, 25, 25) // Darker when unavailable
                } else if response.hovered() {
                    style::colors::BUTTON_HOVER
                } else {
                    style::colors::BUTTON_BG
                };
                ui.painter().rect_filled(rect, 0.0, bg_color);

                // Draw border
                let border_color = if data.can_use && data.cooldown_remaining <= 0.0 {
                    style::colors::DUNGEON_GOLD // Gold border when ready
                } else {
                    style::colors::BUTTON_BORDER
                };
                ui.painter()
                    .rect_stroke(rect, 0.0, egui::Stroke::new(2.0, border_color));

                // Draw the icon
                let image = egui::Image::new(egui::load::SizedTexture::new(
                    icons.items_texture_id,
                    button_size,
                ))
                .uv(uv);
                image.paint_at(ui, rect);

                // Draw cooldown overlay if on cooldown
                if data.cooldown_remaining > 0.0 {
                    // Dark overlay
                    let overlay_color = egui::Color32::from_rgba_unmultiplied(0, 0, 0, 180);
                    ui.painter().rect_filled(rect, 0.0, overlay_color);

                    // Cooldown text
                    let cd_text = format!("{:.0}s", data.cooldown_remaining);
                    ui.painter().text(
                        rect.center(),
                        egui::Align2::CENTER_CENTER,
                        cd_text,
                        egui::FontId::proportional(14.0),
                        egui::Color32::WHITE,
                    );
                }

                // Handle click
                if response.clicked() && data.can_use {
                    clicked = true;
                }

                // Tooltip
                response.on_hover_text(tooltip);

                // Hotkey hint
                ui.label(
                    egui::RichText::new("[Q]")
                        .color(style::colors::TEXT_MUTED)
                        .small(),
                );
            });
        });

    clicked
}

/// Render the secondary ability bar (positioned to the right of the primary).
/// Returns true if the ability button was clicked.
pub fn draw_secondary_ability_bar(ctx: &egui::Context, data: &AbilityBarData, icons: &UiIcons) -> bool {
    let mut clicked = false;

    egui::Window::new("Secondary Ability")
        .fixed_pos([100.0, data.viewport_height - 90.0]) // Offset to the right of primary
        .fixed_size([80.0, 80.0])
        .title_bar(false)
        .frame(style::dungeon_window_frame())
        .show(ctx, |ui| {
            ui.vertical_centered(|ui| {
                // Get the appropriate icon UV based on ability type
                let (uv, tooltip) = match data.ability_type {
                    AbilityType::Barkskin => (
                        icons.barkskin_uv,
                        "Barkskin\n50% damage reduction for 15s\n\n[E]",
                    ),
                    // Secondary abilities only - others shouldn't appear here
                    _ => (icons.heart_uv, "Unknown ability"),
                };

                // Create the button size
                let button_size = egui::vec2(48.0, 48.0);

                // Allocate space for the button
                let (rect, response) = ui.allocate_exact_size(button_size, egui::Sense::click());

                // Draw background
                let bg_color = if !data.can_use {
                    egui::Color32::from_rgb(30, 25, 25) // Darker when unavailable
                } else if response.hovered() {
                    style::colors::BUTTON_HOVER
                } else {
                    style::colors::BUTTON_BG
                };
                ui.painter().rect_filled(rect, 0.0, bg_color);

                // Draw border - use brown/green for nature theme
                let border_color = if data.can_use && data.cooldown_remaining <= 0.0 {
                    egui::Color32::from_rgb(100, 140, 80) // Green border when ready
                } else {
                    style::colors::BUTTON_BORDER
                };
                ui.painter()
                    .rect_stroke(rect, 0.0, egui::Stroke::new(2.0, border_color));

                // Draw the icon
                let image = egui::Image::new(egui::load::SizedTexture::new(
                    icons.items_texture_id,
                    button_size,
                ))
                .uv(uv);
                image.paint_at(ui, rect);

                // Draw cooldown overlay if on cooldown
                if data.cooldown_remaining > 0.0 {
                    // Dark overlay
                    let overlay_color = egui::Color32::from_rgba_unmultiplied(0, 0, 0, 180);
                    ui.painter().rect_filled(rect, 0.0, overlay_color);

                    // Cooldown text
                    let cd_text = format!("{:.0}s", data.cooldown_remaining);
                    ui.painter().text(
                        rect.center(),
                        egui::Align2::CENTER_CENTER,
                        cd_text,
                        egui::FontId::proportional(14.0),
                        egui::Color32::WHITE,
                    );
                }

                // Handle click
                if response.clicked() && data.can_use {
                    clicked = true;
                }

                // Tooltip
                response.on_hover_text(tooltip);

                // Hotkey hint
                ui.label(
                    egui::RichText::new("[E]")
                        .color(style::colors::TEXT_MUTED)
                        .small(),
                );
            });
        });

    clicked
}
