//! Altar sacrifice window.
//!
//! Opened by interacting with an altar (see `systems::furniture`). Lists the
//! player's inventory; clicking an item sacrifices it — better items give
//! better blessing odds. Esc or Cancel closes without sacrificing.

use super::icons::UiIcons;
use super::style;
use super::UiActions;
use crate::components::{Inventory, ItemInstance};
use crate::systems::furniture::{altar_blessing_chance, altar_item_value};
use hecs::World;

/// Data needed to render the altar window
pub struct AltarWindowData {
    /// One row per item, or per stack of a stackable kind:
    /// (inventory index of the first item, how many, the item)
    pub items: Vec<(usize, u32, ItemInstance)>,
    pub viewport_width: f32,
    pub viewport_height: f32,
}

/// Extract altar window data (the player's sacrificable inventory).
pub fn get_altar_window_data(
    world: &World,
    open_altar: Option<hecs::Entity>,
    player: hecs::Entity,
    viewport_width: f32,
    viewport_height: f32,
) -> Option<AltarWindowData> {
    open_altar?;
    let inventory = world.get::<&Inventory>(player).ok()?;
    let items = crate::systems::stack_items(&inventory.items)
        .into_iter()
        .map(|s| (s.first_index, s.count, inventory.items[s.first_index].clone()))
        .collect();
    Some(AltarWindowData {
        items,
        viewport_width,
        viewport_height,
    })
}

/// Render the altar sacrifice window.
pub fn draw_altar_window(
    ctx: &egui::Context,
    data: &AltarWindowData,
    icons: &UiIcons,
    actions: &mut UiActions,
) {
    style::dungeon_window(
        ctx,
        icons,
        "Altar",
        |window| {
            window
                .default_pos([
                    data.viewport_width / 2.0 - 160.0,
                    data.viewport_height / 2.0 - 120.0,
                ])
                .default_size([320.0, 240.0])
                .collapsible(false)
                .resizable(false)
        },
        |ui| {
            style::panel_header(ui, "Offer a sacrifice");
            ui.label(
                egui::RichText::new("The altar hungers. Finer offerings earn finer blessings.")
                    .italics()
                    .color(style::colors::TEXT_MUTED),
            );
            ui.separator();
            ui.add_space(6.0);

            if data.items.is_empty() {
                ui.label(
                    egui::RichText::new("(you have nothing to offer)")
                        .italics()
                        .color(style::colors::TEXT_MUTED),
                );
            } else {
                egui::ScrollArea::vertical().max_height(220.0).show(ui, |ui| {
                    for (i, count, instance) in data.items.iter() {
                        let item_type = instance.kind;
                        ui.horizontal(|ui| {
                            let uv = icons.get_item_uv(item_type);
                            let image = egui::Image::new(egui::load::SizedTexture::new(
                                icons.items_texture_id,
                                egui::vec2(32.0, 32.0),
                            ))
                            .uv(uv)
                            .tint(UiIcons::item_ui_tint(item_type))
                            .bg_fill(style::colors::PANEL_BG);

                            // A stack is offered one item at a time.
                            let item_name = instance.display_name();
                            let (item_name, click_hint) = if *count > 1 {
                                (format!("{} x{}", item_name, count), "Click to sacrifice one")
                            } else {
                                (item_name, "Click to sacrifice")
                            };
                            let odds =
                                altar_blessing_chance(altar_item_value(instance)) * 100.0;
                            let response = ui.add(egui::ImageButton::new(image).frame(false));
                            if response
                                .on_hover_text(format!(
                                    "{} ({})\nChance of blessing: {:.0}%\n\n{}",
                                    item_name,
                                    instance.rarity.label(),
                                    odds,
                                    click_hint
                                ))
                                .clicked()
                            {
                                actions.altar_sacrifice = Some(*i);
                            }
                            ui.label(
                                egui::RichText::new(item_name)
                                    .color(style::rarity_color(instance.rarity)),
                            );
                        });
                    }
                });
            }

            ui.add_space(8.0);
            ui.separator();
            if ui.button("Cancel").clicked() {
                actions.close_altar = true;
            }
        },
    );
}
