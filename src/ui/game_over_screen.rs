//! Game over / retry screen UI component.
//!
//! Shown when the player dies. Displays the run summary and offers to retry
//! (restart with the same class) or return to the class selection screen.

use super::icons::UiIcons;
use super::status_bar::format_game_clock;
use super::style::{self, colors, ButtonWeight};
use crate::constants::*;
use egui::RichText;
use egui_glow::EguiGlow;
use winit::window::Window;

/// What the player chose on the game over screen.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GameOverChoice {
    /// No choice made yet
    None,
    /// Restart the run with the same class
    Retry,
    /// Return to the class selection screen
    MainMenu,
}

/// This run's summary, shown on the game over screen.
#[derive(Debug, Clone)]
pub struct GameOverStats {
    /// Elapsed game time in seconds.
    pub time_survived: f32,
    /// Zero-based floor the player died on.
    pub floor: u32,
    /// The run's seed (enter it on the start screen to retry the same dungeon).
    pub seed: u64,
    /// Hostile enemies slain.
    pub kills: u32,
    /// Best-effort cause of death ("Goblin", "burning", ...).
    pub cause_of_death: String,
}

impl Default for GameOverStats {
    fn default() -> Self {
        Self {
            time_survived: 0.0,
            floor: 0,
            seed: 0,
            kills: 0,
            cause_of_death: "unknown".to_string(),
        }
    }
}

/// Run the game over screen UI.
///
/// Drawn as a dimmed, red-tinged overlay so the frozen dungeon — and whatever
/// killed you — stays visible behind it. Mouse only: a player mashing keys as
/// they die should not retry or leave by accident.
pub fn run_game_over_screen(
    egui_glow: &mut EguiGlow,
    window: &Window,
    icons: &UiIcons,
    stats: &GameOverStats,
) -> GameOverChoice {
    let mut choice = GameOverChoice::None;

    egui_glow.run(window, |ctx| {
        style::menu_overlay(ctx, icons, colors::DEATH_DIM, |ui| {
            ui.add_space(14.0);
            style::menu_title(ui, "You Died", colors::DEATH_TITLE);
            ui.add_space(4.0);
            style::ornament_rule(ui, MENU_RULE_WIDTH * 0.75);
            ui.add_space(12.0);

            // Cause of death, front and center.
            ui.label(
                RichText::new(format!("Slain by {}", stats.cause_of_death))
                    .size(18.0)
                    .color(colors::TEXT_PRIMARY),
            );
            ui.add_space(14.0);

            // Run summary, three figures side by side.
            let figures = [
                ("SURVIVED", format_game_clock(stats.time_survived)),
                ("FLOOR", (stats.floor + 1).to_string()),
                ("KILLS", stats.kills.to_string()),
            ];
            let column = MENU_PANEL_WIDTH / figures.len() as f32;
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 0.0;
                for (caption, value) in figures {
                    ui.allocate_ui_with_layout(
                        egui::vec2(column, 0.0),
                        egui::Layout::top_down(egui::Align::Center),
                        |ui| {
                            ui.label(RichText::new(caption).size(11.0).color(colors::TEXT_MUTED));
                            ui.label(RichText::new(value).size(20.0).strong().color(colors::TEXT_PRIMARY));
                        },
                    );
                }
            });
            ui.add_space(10.0);
            style::panel_rule(ui);
            ui.add_space(6.0);

            // The seed, so this exact dungeon can be run again.
            ui.label(
                RichText::new(format!("Seed {}", stats.seed))
                    .size(14.0)
                    .monospace()
                    .color(colors::DUNGEON_GOLD),
            );
            ui.label(
                RichText::new("enter it on the start screen to face this dungeon again")
                    .size(11.0)
                    .color(colors::TEXT_MUTED),
            );
            ui.add_space(18.0);

            if style::menu_button(ui, "Retry", ButtonWeight::Primary, false).clicked() {
                choice = GameOverChoice::Retry;
            }
            ui.add_space(MENU_BUTTON_SPACING);
            if style::menu_button(ui, "Main Menu", ButtonWeight::Normal, false).clicked() {
                choice = GameOverChoice::MainMenu;
            }
            ui.add_space(10.0);
        });
    });

    choice
}
