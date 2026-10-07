//! Status bar UI component.
//!
//! Displays player health, XP, gold, and active status effects.

use super::icons::UiIcons;
use super::style;
use crate::components::{
    EffectType as StatusEffectType, Fatigue, FatigueState, Health, Hunger, HungerState,
    Inventory, Position, StatusEffects,
};
use crate::constants::{FATIGUE_MAX, HUNGER_MAX};
use crate::grid::Grid;
use crate::systems;
use crate::tile::TileType;
use hecs::World;

/// Format elapsed game time (seconds) as HH:MM:SS, starting from 00:00:00.
pub fn format_game_clock(seconds: f32) -> String {
    let total = seconds.max(0.0) as u64;
    let hours = total / 3600;
    let minutes = (total % 3600) / 60;
    let secs = total % 60;
    format!("{:02}:{:02}:{:02}", hours, minutes, secs)
}

/// Data needed to render the status bar
pub struct StatusBarData {
    pub health_current: i32,
    pub health_max: i32,
    pub xp_progress: f32,
    pub xp_level: u32,
    pub gold: u32,
    /// Total flat defense from equipped armor
    pub defense: i32,
    /// Whether the player is currently concealed (standing in tall grass)
    pub is_concealed: bool,
    /// Whether the player is currently sneaking (crouch toggle)
    pub is_sneaking: bool,
    /// Hunger meter value (0 = starving, HUNGER_MAX = fully fed)
    pub hunger: f32,
    /// Coarse hunger state for the warning label
    pub hunger_state: HungerState,
    /// Fatigue meter value (0 = rested, FATIGUE_MAX = exhausted)
    pub fatigue: f32,
    /// Coarse fatigue state for the warning label
    pub fatigue_state: FatigueState,
    /// Active status effects with remaining duration
    pub active_effects: Vec<(StatusEffectType, f32)>,
}

/// Extract status bar data from the world
pub fn get_status_bar_data(world: &World, player_entity: hecs::Entity, grid: &Grid) -> StatusBarData {
    let (health_current, health_max) = world
        .get::<&Health>(player_entity)
        .map(|h| (h.current, h.max))
        .unwrap_or((0, 0));

    let gold = world
        .get::<&Inventory>(player_entity)
        .map(|inv| inv.gold)
        .unwrap_or(0);

    let defense = world
        .get::<&crate::components::Equipment>(player_entity)
        .map(|e| e.total_defense())
        .unwrap_or(0);

    // Concealed is a derived/positional state: true while standing in tall grass.
    let is_concealed = world
        .get::<&Position>(player_entity)
        .ok()
        .and_then(|p| grid.get(p.x, p.y).map(|t| t.tile_type == TileType::TallGrass))
        .unwrap_or(false);

    // Sneaking is a derived state from the crouch toggle (Sneaking marker).
    let is_sneaking = world.get::<&crate::components::Sneaking>(player_entity).is_ok();

    // Survival meters (player-only; default to "fine" if missing).
    let (hunger, hunger_state) = world
        .get::<&Hunger>(player_entity)
        .map(|h| (h.value, h.state()))
        .unwrap_or((HUNGER_MAX, HungerState::Fed));
    let (fatigue, fatigue_state) = world
        .get::<&Fatigue>(player_entity)
        .map(|f| (f.value, f.state()))
        .unwrap_or((0.0, FatigueState::Rested));

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
        defense,
        is_concealed,
        is_sneaking,
        hunger,
        hunger_state,
        fatigue,
        fatigue_state,
        active_effects,
    }
}

/// Render the status bar (health, XP, gold, game clock, status effects)
pub fn draw_status_bar(ctx: &egui::Context, data: &StatusBarData, icons: &UiIcons, game_time: f32) {
    // Calculate window height based on number of status effects
    // (base includes the HP, XP, hunger, fatigue, gold and game-clock rows)
    let base_height = 156.0;
    let effects_height = if data.active_effects.is_empty() {
        0.0
    } else {
        25.0
    };
    let defense_height = if data.defense > 0 { 22.0 } else { 0.0 };
    let concealed_height = if data.is_concealed { 22.0 } else { 0.0 };
    let sneaking_height = if data.is_sneaking { 22.0 } else { 0.0 };
    let hunger_label_height = if data.hunger_state != HungerState::Fed { 22.0 } else { 0.0 };
    let fatigue_label_height = if data.fatigue_state != FatigueState::Rested { 22.0 } else { 0.0 };
    let window_height = base_height
        + effects_height
        + defense_height
        + concealed_height
        + sneaking_height
        + hunger_label_height
        + fatigue_label_height;

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

            // Hunger bar with cheese icon (drains over time; food refills it)
            ui.horizontal(|ui| {
                let food_img = egui::Image::new(egui::load::SizedTexture::new(
                    icons.items_texture_id,
                    egui::vec2(16.0, 16.0),
                ))
                .uv(icons.cheese_uv);
                ui.add(food_img);
                let hunger_frac = (data.hunger / HUNGER_MAX).clamp(0.0, 1.0);
                ui.add_sized(
                    [180.0, 18.0],
                    egui::ProgressBar::new(hunger_frac)
                        .fill(egui::Color32::from_rgb(190, 130, 55))
                        .text(format!("{:.0}/{:.0}", data.hunger, HUNGER_MAX)),
                );
            });

            // Fatigue bar ("Zz") — fills up as the player gets more tired.
            ui.horizontal(|ui| {
                ui.add_sized(
                    [16.0, 16.0],
                    egui::Label::new(
                        egui::RichText::new("Zz")
                            .strong()
                            .color(egui::Color32::from_rgb(150, 140, 210)),
                    ),
                );
                let fatigue_frac = (data.fatigue / FATIGUE_MAX).clamp(0.0, 1.0);
                ui.add_sized(
                    [180.0, 18.0],
                    egui::ProgressBar::new(fatigue_frac)
                        .fill(egui::Color32::from_rgb(110, 100, 185))
                        .text(format!("{:.0}/{:.0}", data.fatigue, FATIGUE_MAX)),
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

            // Defense (only shown when the player has armor)
            if data.defense > 0 {
                ui.horizontal(|ui| {
                    ui.label(format!("🛡 Defense: {}", data.defense));
                });
            }

            // Concealed (derived: standing in tall grass)
            if data.is_concealed {
                ui.horizontal(|ui| {
                    ui.label(
                        egui::RichText::new("🌿 Concealed")
                            .color(egui::Color32::from_rgb(120, 200, 120)),
                    );
                });
            }

            // Sneaking (derived: crouch toggle)
            if data.is_sneaking {
                ui.horizontal(|ui| {
                    ui.label(
                        egui::RichText::new("👁 Sneaking")
                            .color(egui::Color32::from_rgb(150, 120, 210)),
                    );
                });
            }

            // Hunger warning label (derived from the hunger meter)
            match data.hunger_state {
                HungerState::Hungry => {
                    ui.label(
                        egui::RichText::new("Hungry — no natural healing")
                            .color(egui::Color32::from_rgb(220, 160, 70)),
                    );
                }
                HungerState::Starving => {
                    ui.label(
                        egui::RichText::new("Starving!")
                            .strong()
                            .color(egui::Color32::from_rgb(230, 80, 70)),
                    );
                }
                HungerState::Fed => {}
            }

            // Fatigue warning label (derived from the fatigue meter)
            match data.fatigue_state {
                FatigueState::Tired => {
                    ui.label(
                        egui::RichText::new("Tired — sloppy and easy to spot")
                            .color(egui::Color32::from_rgb(200, 190, 110)),
                    );
                }
                FatigueState::Exhausted => {
                    ui.label(
                        egui::RichText::new("Exhausted!")
                            .strong()
                            .color(egui::Color32::from_rgb(170, 120, 220)),
                    );
                }
                FatigueState::Rested => {}
            }

            // Elapsed game time (HH:MM:SS)
            ui.horizontal(|ui| {
                ui.label(
                    egui::RichText::new("Time")
                        .color(style::colors::TEXT_MUTED)
                        .small(),
                );
                ui.label(
                    egui::RichText::new(format_game_clock(game_time))
                        .monospace()
                        .color(style::colors::TEXT_PRIMARY),
                );
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
                            StatusEffectType::Burning => {
                                ("Burning", egui::Color32::from_rgb(255, 100, 50)) // Orange-red fire color
                            }
                            StatusEffectType::Rooted => {
                                ("Rooted", egui::Color32::from_rgb(139, 90, 43)) // Brown/root color
                            }
                            StatusEffectType::Invulnerable => {
                                ("Invuln", egui::Color32::from_rgb(255, 215, 0)) // Gold color
                            }
                            StatusEffectType::Stunned => {
                                ("Stunned", egui::Color32::from_rgb(255, 230, 120)) // Pale gold
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
