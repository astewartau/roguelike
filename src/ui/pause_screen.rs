//! Pause menu UI component.
//!
//! Shown when the player presses Escape with no other menu open. Drawn as a
//! translucent overlay over the frozen dungeon. Keyboard- and mouse-navigable.

use super::style;
use egui::{Color32, RichText};
use egui_glow::EguiGlow;
use winit::window::Window;

/// What the player chose on the pause menu.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PauseChoice {
    /// No choice made this frame
    None,
    /// Resume play
    Resume,
    /// Restart the run with the same class
    Retry,
    /// Return to the class selection screen
    MainMenu,
    /// Quit the application
    Exit,
}

const OPTIONS: [&str; 4] = ["Resume", "Retry", "Main Menu", "Exit"];

fn choice_for(idx: usize) -> PauseChoice {
    match idx {
        0 => PauseChoice::Resume,
        1 => PauseChoice::Retry,
        2 => PauseChoice::MainMenu,
        _ => PauseChoice::Exit,
    }
}

/// Run the pause menu. `selected` is the keyboard-highlighted option index
/// (persisted by the caller). Arrows/WASD move the highlight, Enter/Space
/// confirms, mouse hover/click also work.
pub fn run_pause_screen(
    egui_glow: &mut EguiGlow,
    window: &Window,
    selected: &mut usize,
) -> PauseChoice {
    let mut choice = PauseChoice::None;

    egui_glow.run(window, |ctx| {
        // Keyboard navigation.
        let pointer_moved = ctx.input(|i| {
            use egui::Key;
            let n = OPTIONS.len();
            if i.key_pressed(Key::ArrowDown) || i.key_pressed(Key::S) {
                *selected = (*selected + 1) % n;
            }
            if i.key_pressed(Key::ArrowUp) || i.key_pressed(Key::W) {
                *selected = (*selected + n - 1) % n;
            }
            if i.key_pressed(Key::Enter) || i.key_pressed(Key::Space) {
                choice = choice_for(*selected);
            }
            i.pointer.delta() != egui::Vec2::ZERO
        });

        egui::CentralPanel::default()
            .frame(egui::Frame::none().fill(Color32::from_rgba_unmultiplied(8, 8, 14, 200)))
            .show(ctx, |ui| {
                ui.vertical_centered(|ui| {
                    ui.add_space(110.0);
                    ui.heading(
                        RichText::new("Paused")
                            .size(48.0)
                            .color(style::colors::DUNGEON_GOLD),
                    );
                    ui.add_space(36.0);

                    for (idx, label) in OPTIONS.iter().enumerate() {
                        let is_selected = idx == *selected;
                        let fill = if is_selected {
                            style::colors::SELECTED
                        } else {
                            style::colors::BUTTON_BG
                        };
                        let button = egui::Button::new(
                            RichText::new(*label).size(22.0).color(Color32::WHITE),
                        )
                        .min_size(egui::vec2(240.0, 46.0))
                        .fill(fill);
                        let response = ui.add(button);
                        // Mouse hover moves the highlight only when the pointer is moving,
                        // so a resting mouse doesn't fight keyboard navigation.
                        if response.hovered() && pointer_moved {
                            *selected = idx;
                        }
                        if response.clicked() {
                            choice = choice_for(idx);
                        }
                        ui.add_space(12.0);
                    }

                    ui.add_space(8.0);
                    ui.label(
                        RichText::new("↑/↓ select   •   Enter confirm   •   Esc resume")
                            .size(12.0)
                            .color(style::colors::TEXT_MUTED),
                    );
                });
            });
    });

    choice
}
