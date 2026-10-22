//! Pause menu UI component.
//!
//! Shown when the player presses Escape with no other menu open. Drawn as a
//! dimmed, vignetted overlay over the frozen dungeon with the menu on a stone
//! panel in the middle. Keyboard- and mouse-navigable.

use super::icons::UiIcons;
use super::style::{self, colors, ButtonWeight};
use crate::constants::*;
use egui::RichText;
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
    icons: &UiIcons,
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

        style::menu_overlay(ctx, icons, colors::PAUSE_DIM, |ui| {
            ui.add_space(14.0);
            style::menu_title(ui, "Paused", colors::DUNGEON_GOLD);
            ui.add_space(4.0);
            style::ornament_rule(ui, MENU_RULE_WIDTH * 0.75);
            ui.add_space(20.0);

            for (idx, label) in OPTIONS.iter().enumerate() {
                let weight = if idx == 0 { ButtonWeight::Primary } else { ButtonWeight::Normal };
                let response = style::menu_button(ui, label, weight, idx == *selected);
                // Mouse hover moves the highlight only when the pointer is
                // moving, so a resting mouse doesn't fight keyboard navigation.
                if response.hovered() && pointer_moved {
                    *selected = idx;
                }
                if response.clicked() {
                    choice = choice_for(idx);
                }
                ui.add_space(MENU_BUTTON_SPACING);
            }

            ui.add_space(10.0);
            ui.label(
                RichText::new("↑/↓ select   •   Enter confirm   •   Esc resume")
                    .size(12.0)
                    .color(colors::TEXT_MUTED),
            );
            ui.add_space(6.0);
        });
    });

    choice
}
