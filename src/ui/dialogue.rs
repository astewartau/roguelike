//! Dialogue window UI component.
//!
//! Displays NPC dialogue with response options.

use super::icons::UiIcons;
use super::style;
use super::UiActions;
use crate::components::{Dialogue, Sprite};
use crate::multi_tileset::MultiTileset;
use crate::systems;
use crate::tile::SpriteSheet;
use hecs::World;

/// Data needed to render the dialogue window
pub struct DialogueWindowData {
    pub npc_name: String,
    pub text: String,
    pub options: Vec<String>,
    /// Speaker's sprite (portrait shown in the window), if the NPC has one
    pub sprite: Option<(SpriteSheet, u32)>,
    pub viewport_width: f32,
    pub viewport_height: f32,
}

/// Extract dialogue window data from the world
pub fn get_dialogue_window_data(
    world: &World,
    talking_to: Option<hecs::Entity>,
    viewport_width: f32,
    viewport_height: f32,
) -> Option<DialogueWindowData> {
    let npc_id = talking_to?;
    let dialogue = world.get::<&Dialogue>(npc_id).ok()?;
    let node = systems::dialogue::current_node(&dialogue)?;

    let sprite = world.get::<&Sprite>(npc_id).ok().map(|s| (s.sheet, s.tile_id));

    Some(DialogueWindowData {
        npc_name: dialogue.name.clone(),
        text: node.text.clone(),
        options: node.options.iter().map(|o| o.label.clone()).collect(),
        sprite,
        viewport_width,
        viewport_height,
    })
}

/// Render the dialogue window for NPC conversations.
///
/// `selected` is the keyboard-highlighted option index (persisted in
/// `GameUiState`). Options can be driven by mouse (hover highlights, click
/// confirms) or keyboard (Up/Down or W/S to move, Enter/Space to confirm,
/// number keys to pick directly, Esc to close).
pub fn draw_dialogue_window(
    ctx: &egui::Context,
    data: &DialogueWindowData,
    icons: &UiIcons,
    tileset: &MultiTileset,
    selected: &mut usize,
    actions: &mut UiActions,
) {
    let option_count = data.options.len();
    if option_count == 0 {
        return;
    }

    // Keep the highlight in range (the node may have changed since last frame).
    if *selected >= option_count {
        *selected = option_count - 1;
    }

    // --- Keyboard navigation -------------------------------------------------
    // (Number keys are intentionally not used here — they drive the hotbars.)
    let mut confirm = false;
    let pointer_moved = ctx.input(|i| {
        use egui::Key;
        if i.key_pressed(Key::ArrowDown) || i.key_pressed(Key::S) {
            *selected = (*selected + 1) % option_count;
        }
        if i.key_pressed(Key::ArrowUp) || i.key_pressed(Key::W) {
            *selected = (*selected + option_count - 1) % option_count;
        }
        if i.key_pressed(Key::Enter) || i.key_pressed(Key::Space) {
            confirm = true;
        }
        if i.key_pressed(Key::Escape) {
            actions.close_dialogue = true;
        }
        i.pointer.delta() != egui::Vec2::ZERO
    });

    egui::Window::new(&data.npc_name)
        .default_pos([
            data.viewport_width / 2.0 - 200.0,
            data.viewport_height / 2.0 - 100.0,
        ])
        .default_size([400.0, 200.0])
        .collapsible(false)
        .resizable(false)
        .frame(style::dungeon_window_frame())
        .show(ctx, |ui| {
            // Pin a stable width so option buttons are uniform and the window
            // doesn't jump between nodes; height auto-fits the content.
            ui.set_min_width(380.0);
            let full_width = ui.available_width();
            ui.add_space(5.0);

            // Speaker portrait (left) alongside the dialogue text (right). The
            // text is given an explicit width so it wraps instead of stretching
            // the window — a horizontal layout would otherwise leave it unbounded.
            const PORTRAIT_SIZE: f32 = 64.0;
            const PORTRAIT_GAP: f32 = 8.0;
            ui.horizontal_top(|ui| {
                let mut text_width = full_width;
                if let Some((sheet, tile_id)) = data.sprite {
                    let size = egui::vec2(PORTRAIT_SIZE, PORTRAIT_SIZE);
                    let (rect, _) = ui.allocate_exact_size(size, egui::Sense::hover());
                    ui.painter().rect_filled(rect, 4.0, egui::Color32::BLACK);
                    let image = egui::Image::new(egui::load::SizedTexture::new(
                        icons.texture_for_sheet(sheet),
                        size,
                    ))
                    .uv(tileset.get_egui_uv(sheet, tile_id));
                    image.paint_at(ui, rect);
                    ui.add_space(PORTRAIT_GAP);
                    text_width -= PORTRAIT_SIZE + PORTRAIT_GAP;
                }
                ui.allocate_ui_with_layout(
                    egui::vec2(text_width, 0.0),
                    egui::Layout::top_down(egui::Align::LEFT),
                    |ui| {
                        ui.label(egui::RichText::new(&data.text).size(14.0));
                    },
                );
            });

            ui.add_space(15.0);
            ui.separator();
            ui.add_space(10.0);

            // Response options as uniform-width buttons. The highlighted option
            // (keyboard or hover) gets the "selected" fill.
            let width = ui.available_width();
            for (i, option_text) in data.options.iter().enumerate() {
                let is_selected = i == *selected;
                let fill = if is_selected {
                    style::colors::SELECTED
                } else {
                    style::colors::BUTTON_BG
                };
                let label = format!("{}. {}", i + 1, option_text);
                let button = egui::Button::new(egui::RichText::new(label).size(14.0))
                    .fill(fill)
                    .min_size(egui::vec2(width, 30.0));
                let response = ui.add(button);

                // Mouse hover moves the highlight, but only when the pointer is
                // actually moving — otherwise a stationary mouse would fight
                // keyboard navigation.
                if response.hovered() && pointer_moved {
                    *selected = i;
                }
                if response.clicked() {
                    *selected = i;
                    confirm = true;
                }
                ui.add_space(4.0);
            }

            ui.add_space(4.0);
            ui.label(
                egui::RichText::new("↑/↓ select   •   Enter confirm   •   Esc close")
                    .size(11.0)
                    .color(style::colors::TEXT_MUTED),
            );
        });

    if confirm {
        actions.dialogue_option_selected = Some(*selected);
        // Reset for the next node (or the next conversation).
        *selected = 0;
    }
}
