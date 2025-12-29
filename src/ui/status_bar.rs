//! Status bar UI component.
//!
//! Displays player health, XP, gold, and active status effects.

use super::icons::UiIcons;
use super::style;
use crate::components::{EffectType as StatusEffectType, Health, Inventory, StatusEffects};
use crate::systems;
use hecs::World;

/// Data needed to render the status bar
pub struct StatusBarData {
    pub health_current: i32,
    pub health_max: i32,
    pub xp_progress: f32,
    pub xp_level: u32,
    pub gold: u32,
    /// Active status effects with remaining duration
    pub active_effects: Vec<(StatusEffectType, f32)>,
}

/// Extract status bar data from the world
pub fn get_status_bar_data(world: &World, player_entity: hecs::Entity) -> StatusBarData {
    let (health_current, health_max) = world
        .get::<&Health>(player_entity)
        .map(|h| (h.current, h.max))
        .unwrap_or((0, 0));

    let gold = world
        .get::<&Inventory>(player_entity)
        .map(|inv| inv.gold)
        .unwrap_or(0);

    let (xp_progress, xp_level) = world
        .get::<&crate::components::Experience>(player_entity)
        .map(|exp| (systems::xp_progress(&exp), exp.level))
        .unwrap_or((0.0, 1));

    // Collect active status effects
    let active_effects = world
        .get::<&StatusEffects>(player_entity)
        .map(|effects| {
            effects
                .effects
                .iter()
                .map(|e| (e.effect_type, e.remaining_duration))
                .collect()
        })
        .unwrap_or_default();

    StatusBarData {
        health_current,
        health_max,
        xp_progress,
        xp_level,
        gold,
        active_effects,
    }
}

/// Render the status bar (health, XP, gold, status effects)
pub fn draw_status_bar(ctx: &egui::Context, data: &StatusBarData, icons: &UiIcons) {
    // Calculate window height based on number of status effects
    let base_height = 90.0;
    let effects_height = if data.active_effects.is_empty() {
        0.0
    } else {
        25.0
    };
    let window_height = base_height + effects_height;

    egui::Window::new("Status")
        .fixed_pos([10.0, 10.0])
        .fixed_size([220.0, window_height])
        .title_bar(false)
        .frame(style::dungeon_window_frame())
        .show(ctx, |ui| {
            let health_percent = if data.health_max > 0 {
                data.health_current as f32 / data.health_max as f32
            } else {
                0.0
            };

            // HP bar with heart icon
            ui.horizontal(|ui| {
                let heart_img = egui::Image::new(egui::load::SizedTexture::new(
                    icons.items_texture_id,
                    egui::vec2(16.0, 16.0),
                ))
                .uv(icons.heart_uv);
                ui.add(heart_img);
                ui.add_sized(
                    [180.0, 18.0],
                    egui::ProgressBar::new(health_percent)
                        .fill(style::colors::HP_BAR)
                        .text(format!("{}/{}", data.health_current, data.health_max)),
                );
            });

            // XP bar with diamond icon
            ui.horizontal(|ui| {
                let diamond_img = egui::Image::new(egui::load::SizedTexture::new(
                    icons.items_texture_id,
                    egui::vec2(16.0, 16.0),
                ))
                .uv(icons.diamond_uv);
                ui.add(diamond_img);
                ui.add_sized(
                    [180.0, 18.0],
                    egui::ProgressBar::new(data.xp_progress)
                        .fill(style::colors::XP_BAR)
                        .text(format!(
                            "Lv {} - {:.0}%",
                            data.xp_level,
                            data.xp_progress * 100.0
                        )),
                );
            });

            // Gold with coins icon
            ui.horizontal(|ui| {
                let coin_img = egui::Image::new(egui::load::SizedTexture::new(
                    icons.items_texture_id,
                    egui::vec2(16.0, 16.0),
                ))
                .uv(icons.coins_uv);
                ui.add(coin_img);
                ui.label(format!("{}", data.gold));
            });

            // Active status effects
            if !data.active_effects.is_empty() {
                ui.separator();
                ui.horizontal(|ui| {
                    for (effect_type, duration) in &data.active_effects {
                        let (label, color) = match effect_type {
                            StatusEffectType::Invisible => {
                                ("Invisible", egui::Color32::from_rgb(180, 180, 255))
                            }
                            StatusEffectType::SpeedBoost => {
                                ("Speed", egui::Color32::from_rgb(255, 220, 100))
                            }
                            StatusEffectType::Regenerating => {
                                ("Regen", egui::Color32::from_rgb(100, 255, 100))
                            }
                            StatusEffectType::Strengthened => {
                                ("Strength", egui::Color32::from_rgb(255, 150, 50))
                            }
                            StatusEffectType::Protected => {
                                ("Protected", egui::Color32::from_rgb(150, 150, 255))
                            }
                            StatusEffectType::Barkskin => {
                                ("Barkskin", egui::Color32::from_rgb(139, 90, 43)) // Brown/bark color
                            }
                            StatusEffectType::Confused => {
                                ("Confused", egui::Color32::from_rgb(200, 100, 200))
                            }
                            StatusEffectType::Feared => {
                                ("Feared", egui::Color32::from_rgb(255, 100, 100))
                            }
                            StatusEffectType::Slowed => {
                                ("Slowed", egui::Color32::from_rgb(100, 150, 200))
                            }
                        };
                        ui.label(
                            egui::RichText::new(format!("{} ({:.0}s)", label, duration))
                                .color(color)
                                .small(),
                        );
                    }
                });
            }
        });
}
