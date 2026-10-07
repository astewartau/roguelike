//! Game over / retry screen UI component.
//!
//! Shown when the player dies. Displays the run summary and offers to retry
//! (restart with the same class) or return to the class selection screen.

use super::status_bar::format_game_clock;
use super::style;
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
/// Drawn as a translucent overlay so the frozen dungeon remains visible
/// behind it.
pub fn run_game_over_screen(
    egui_glow: &mut EguiGlow,
    window: &Window,
    stats: &GameOverStats,
) -> GameOverChoice {
    let mut choice = GameOverChoice::None;

    egui_glow.run(window, |ctx| {
        egui::CentralPanel::default()
            .frame(egui::Frame::none().fill(egui::Color32::from_rgba_unmultiplied(8, 6, 6, 220)))
            .show(ctx, |ui| {
                ui.vertical_centered(|ui| {
                    ui.add_space(120.0);

                    // Title
                    ui.heading(
                        egui::RichText::new("You Died")
                            .size(56.0)
                            .color(style::colors::HP_BAR),
                    );

                    ui.add_space(30.0);

                    // Cause of death, front and center.
                    ui.label(
                        egui::RichText::new(format!("Slain by {}", stats.cause_of_death))
                            .size(22.0)
                            .color(style::colors::TEXT_PRIMARY),
                    );

                    ui.add_space(16.0);

                    // Run summary
                    ui.label(
                        egui::RichText::new(format!(
                            "Survived: {}",
                            format_game_clock(stats.time_survived)
                        ))
                        .size(20.0)
                        .monospace()
                        .color(style::colors::TEXT_PRIMARY),
                    );
                    ui.label(
                        egui::RichText::new(format!("Reached floor {}", stats.floor + 1))
                            .size(20.0)
                            .color(style::colors::TEXT_MUTED),
                    );
                    ui.label(
                        egui::RichText::new(format!("Kills: {}", stats.kills))
                            .size(20.0)
                            .color(style::colors::TEXT_MUTED),
                    );

                    ui.add_space(12.0);

                    // The seed, so this exact dungeon can be run again.
                    ui.label(
                        egui::RichText::new(format!("Seed: {}", stats.seed))
                            .size(16.0)
                            .monospace()
                            .color(style::colors::DUNGEON_GOLD),
                    );
                    ui.label(
                        egui::RichText::new(
                            "(enter this seed on the start screen to retry the same dungeon)",
                        )
                        .size(12.0)
                        .color(style::colors::TEXT_MUTED),
                    );

                    ui.add_space(28.0);

                    // Retry button
                    let retry = egui::Button::new(
                        egui::RichText::new("Retry")
                            .size(24.0)
                            .color(egui::Color32::WHITE),
                    )
                    .min_size(egui::vec2(220.0, 50.0))
                    .fill(style::colors::DUNGEON_GREEN);
                    if ui.add(retry).clicked() {
                        choice = GameOverChoice::Retry;
                    }

                    ui.add_space(16.0);

                    // Main menu button
                    let menu = egui::Button::new(
                        egui::RichText::new("Main Menu")
                            .size(20.0)
                            .color(style::colors::TEXT_PRIMARY),
                    )
                    .min_size(egui::vec2(220.0, 44.0))
                    .fill(style::colors::BUTTON_BG);
                    if ui.add(menu).clicked() {
                        choice = GameOverChoice::MainMenu;
                    }
                });
            });
    });

    choice
}
