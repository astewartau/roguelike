//! Dialogue window UI component.
//!
//! Displays NPC dialogue with response options.

use super::style;
use super::UiActions;
use crate::components::Dialogue;
use crate::systems;
use hecs::World;

/// Data needed to render the dialogue window
pub struct DialogueWindowData {
    pub npc_name: String,
    pub text: String,
    pub options: Vec<String>,
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

    Some(DialogueWindowData {
        npc_name: dialogue.name.clone(),
        text: node.text.clone(),
        options: node.options.iter().map(|o| o.label.clone()).collect(),
        viewport_width,
        viewport_height,
    })
}

/// Render the dialogue window for NPC conversations
pub fn draw_dialogue_window(
    ctx: &egui::Context,
    data: &DialogueWindowData,
    actions: &mut UiActions,
) {
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
            // NPC's dialogue text
            ui.add_space(5.0);
            ui.label(egui::RichText::new(&data.text).size(14.0));
            ui.add_space(15.0);
            ui.separator();
            ui.add_space(10.0);

            // Response options as buttons
            for (i, option_text) in data.options.iter().enumerate() {
                if ui.button(option_text).clicked() {
                    actions.dialogue_option_selected = Some(i);
                }
                ui.add_space(3.0);
            }
        });
}
