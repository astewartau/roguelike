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

/// Run the game over screen UI.
///
/// `time_survived` is the elapsed game time in seconds and `floor` is the
/// (zero-based) floor the player died on. Drawn as a translucent overlay so the
/// frozen dungeon remains visible behind it.
pub fn run_game_over_screen(
    egui_glow: &mut EguiGlow,
    window: &Window,
    time_survived: f32,
    floor: u32,
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

                    // Run summary
                    ui.label(
                        egui::RichText::new(format!("Survived: {}", format_game_clock(time_survived)))
                            .size(20.0)
                            .monospace()
                            .color(style::colors::TEXT_PRIMARY),
                    );
                    ui.label(
                        egui::RichText::new(format!("Reached floor {}", floor + 1))
                            .size(20.0)
                            .color(style::colors::TEXT_MUTED),
                    );

                    ui.add_space(40.0);

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
