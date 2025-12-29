//! Loot window UI component.
//!
//! Displays contents of opened chests and corpses.

use super::icons::UiIcons;
use super::style;
use super::UiActions;
use crate::components::{Container, ItemType};
use crate::systems;
use hecs::World;

/// Data needed to render the loot window
pub struct LootWindowData {
    pub items: Vec<ItemType>,
    pub gold: u32,
    pub viewport_width: f32,
    pub viewport_height: f32,
}

/// Extract loot window data from the world
pub fn get_loot_window_data(
    world: &World,
    open_chest: Option<hecs::Entity>,
    viewport_width: f32,
    viewport_height: f32,
) -> Option<LootWindowData> {
    let chest_id = open_chest?;
    let container = world.get::<&Container>(chest_id).ok()?;

    Some(LootWindowData {
        items: container.items.clone(),
        gold: container.gold,
        viewport_width,
        viewport_height,
    })
}

/// Render the loot window (chest/bones contents)
pub fn draw_loot_window(
    ctx: &egui::Context,
    data: &LootWindowData,
    icons: &UiIcons,
    actions: &mut UiActions,
) {
    egui::Window::new("Loot")
        .default_pos([
            data.viewport_width / 2.0 - 150.0,
            data.viewport_height / 2.0 - 100.0,
        ])
        .default_size([300.0, 200.0])
        .collapsible(false)
        .resizable(false)
        .frame(style::dungeon_window_frame())
        .show(ctx, |ui| {
            ui.heading("Contents");
            ui.separator();
            ui.add_space(10.0);

            let has_contents = !data.items.is_empty() || data.gold > 0;

            if !has_contents {
                ui.label(
                    egui::RichText::new("(empty)")
                        .italics()
                        .color(style::colors::TEXT_MUTED),
                );
            } else {
                // Show gold if present
                if data.gold > 0 {
                    ui.horizontal(|ui| {
                        let coin_img = egui::Image::new(egui::load::SizedTexture::new(
                            icons.items_texture_id,
                            egui::vec2(32.0, 32.0),
                        ))
                        .uv(icons.coins_uv)
                        .bg_fill(style::colors::PANEL_BG);

                        if ui
                            .add(egui::ImageButton::new(coin_img))
                            .on_hover_text(format!("{} Gold\n\nClick to take", data.gold))
                            .clicked()
                        {
                            actions.chest_take_gold = true;
                        }
                        ui.label(format!("{} gold", data.gold));
                    });
                    ui.add_space(5.0);
                }

                // Show items
                ui.horizontal_wrapped(|ui| {
                    for (i, item_type) in data.items.iter().enumerate() {
                        let uv = icons.get_item_uv(*item_type);

                        let image = egui::Image::new(egui::load::SizedTexture::new(
                            icons.items_texture_id,
                            egui::vec2(48.0, 48.0),
                        ))
                        .uv(uv)
                        .bg_fill(style::colors::PANEL_BG);

                        let response = ui.add(egui::ImageButton::new(image));

                        if response
                            .on_hover_text(format!(
                                "{}\n\nClick to take",
                                systems::item_name(*item_type)
                            ))
                            .clicked()
                        {
                            actions.chest_item_to_take = Some(i);
                        }
                    }
                });
            }

            ui.add_space(10.0);
            ui.separator();
            ui.horizontal(|ui| {
                if has_contents {
                    if ui.button("Take All").clicked() {
                        actions.chest_take_all = true;
                    }
                }
                if ui.button("Close").clicked() {
                    actions.close_chest = true;
                }
            });
        });
}
