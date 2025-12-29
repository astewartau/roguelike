//! Start screen UI component.
//!
//! Displays class selection and game start interface.

use super::icons::UiIcons;
use super::style;
use crate::components::PlayerClass;
use crate::multi_tileset::MultiTileset;
use egui_glow::EguiGlow;
use winit::window::Window;

/// Run the start screen UI for class selection.
/// Returns Some(PlayerClass) if the player clicked Start, None otherwise.
pub fn run_start_screen(
    egui_glow: &mut EguiGlow,
    window: &Window,
    tileset: &MultiTileset,
    icons: &UiIcons,
    selected_class: &mut Option<PlayerClass>,
) -> Option<PlayerClass> {
    let mut start_clicked = None;

    egui_glow.run(window, |ctx| {
        // Center the window
        egui::CentralPanel::default()
            .frame(egui::Frame::none().fill(egui::Color32::from_rgb(20, 20, 30)))
            .show(ctx, |ui| {
                ui.vertical_centered(|ui| {
                    ui.add_space(100.0);

                    // Title
                    ui.heading(
                        egui::RichText::new("Roguelike")
                            .size(48.0)
                            .color(style::colors::DUNGEON_GOLD),
                    );

                    ui.add_space(40.0);

                    ui.label(
                        egui::RichText::new("Choose Your Class")
                            .size(24.0)
                            .color(egui::Color32::WHITE),
                    );

                    ui.add_space(30.0);

                    // Class selection buttons
                    ui.horizontal(|ui| {
                        // Calculate total width: 120px per class + 20px spacing between
                        let class_count = PlayerClass::ALL.len() as f32;
                        let total_width = class_count * 120.0 + (class_count - 1.0) * 20.0;
                        ui.add_space((ui.available_width() - total_width) / 2.0);

                        for class in PlayerClass::ALL {
                            let is_selected = *selected_class == Some(class);
                            let sprite = class.sprite();
                            let texture_id = icons.texture_for_sheet(sprite.0);
                            let uv_rect = tileset.get_egui_uv(sprite.0, sprite.1);

                            let (response, painter) = ui.allocate_painter(
                                egui::vec2(120.0, 150.0),
                                egui::Sense::click(),
                            );

                            // Background
                            let bg_color = if is_selected {
                                style::colors::DUNGEON_GOLD.gamma_multiply(0.3)
                            } else if response.hovered() {
                                egui::Color32::from_rgb(50, 50, 60)
                            } else {
                                egui::Color32::from_rgb(35, 35, 45)
                            };
                            painter.rect_filled(response.rect, 8.0, bg_color);

                            // Border
                            let border_color = if is_selected {
                                style::colors::DUNGEON_GOLD
                            } else {
                                egui::Color32::from_rgb(80, 80, 90)
                            };
                            painter.rect_stroke(
                                response.rect,
                                8.0,
                                egui::Stroke::new(2.0, border_color),
                            );

                            // Sprite (centered, larger)
                            let sprite_size = 64.0;
                            let sprite_rect = egui::Rect::from_center_size(
                                response.rect.center() - egui::vec2(0.0, 20.0),
                                egui::vec2(sprite_size, sprite_size),
                            );
                            painter.image(texture_id, sprite_rect, uv_rect, egui::Color32::WHITE);

                            // Class name
                            let text_pos = response.rect.center() + egui::vec2(0.0, 40.0);
                            painter.text(
                                text_pos,
                                egui::Align2::CENTER_CENTER,
                                class.name(),
                                egui::FontId::proportional(18.0),
                                egui::Color32::WHITE,
                            );

                            if response.clicked() {
                                *selected_class = Some(class);
                            }

                            ui.add_space(20.0);
                        }
                    });

                    ui.add_space(40.0);

                    // Start button
                    let start_enabled = selected_class.is_some();
                    let button = egui::Button::new(
                        egui::RichText::new("Start Game").size(24.0).color(
                            if start_enabled {
                                egui::Color32::WHITE
                            } else {
                                egui::Color32::GRAY
                            },
                        ),
                    )
                    .min_size(egui::vec2(200.0, 50.0))
                    .fill(if start_enabled {
                        style::colors::DUNGEON_GREEN
                    } else {
                        egui::Color32::from_rgb(50, 50, 50)
                    });

                    if ui.add_enabled(start_enabled, button).clicked() {
                        start_clicked = *selected_class;
                    }

                    ui.add_space(20.0);

                    // Class description
                    if let Some(class) = selected_class {
                        let (str, int, agi) = class.stats();
                        let weapon = match class {
                            PlayerClass::Fighter => "Sword",
                            PlayerClass::Ranger => "Bow",
                            PlayerClass::Druid => "Staff",
                        };
                        let inventory = match class {
                            PlayerClass::Fighter => "(empty)",
                            PlayerClass::Ranger => "Dagger",
                            PlayerClass::Druid => "(empty)",
                        };

                        ui.label(
                            egui::RichText::new(format!(
                                "STR: {}  INT: {}  AGI: {}",
                                str, int, agi
                            ))
                            .size(16.0)
                            .color(egui::Color32::LIGHT_GRAY),
                        );
                        ui.label(
                            egui::RichText::new(format!(
                                "Equipped: {}  |  Inventory: {}",
                                weapon, inventory
                            ))
                            .size(14.0)
                            .color(egui::Color32::GRAY),
                        );
                    }
                });
            });
    });

    start_clicked
}
