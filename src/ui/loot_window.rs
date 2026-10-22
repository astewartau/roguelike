//! Loot window UI component.
//!
//! Displays the contents of everything lootable on one tile: an opened chest,
//! or every corpse and dropped item underfoot, each in its own section so it
//! is clear which corpse held what. Stackable items (arrows) show as one row
//! with a count.

use super::icons::UiIcons;
use super::style;
use super::UiActions;
use crate::components::ItemInstance;
use crate::systems::{held, loot_source_label, loot_sources, source_gold, source_items, stack_items, LootSource};
use crate::world::World;
use hecs::Entity;

/// One row in a loot section: a single item, or a stack of one kind.
pub struct LootRow {
    /// The row's first item (what a click takes, with the rest of its stack)
    pub item: Entity,
    pub count: u32,
    pub instance: ItemInstance,
}

/// One source's worth of the loot window.
pub struct LootSection {
    pub source: LootSource,
    /// "Chest", "Goblin's corpse", "On the ground"
    pub label: String,
    pub rows: Vec<LootRow>,
    pub gold: u32,
}

impl LootSection {
    fn is_empty(&self) -> bool {
        self.rows.is_empty() && self.gold == 0
    }
}

/// Data needed to render the loot window
pub struct LootWindowData {
    pub sections: Vec<LootSection>,
    pub viewport_width: f32,
    pub viewport_height: f32,
}

/// Extract loot window data from the world
pub fn get_loot_window_data(
    world: &World,
    open_chest: Option<hecs::Entity>,
    loot_tile: (i32, i32),
    viewport_width: f32,
    viewport_height: f32,
) -> Option<LootWindowData> {
    open_chest?;
    let sections: Vec<LootSection> = loot_sources(world, open_chest, loot_tile)
        .into_iter()
        .map(|source| {
            let items = source_items(world, source);
            let values = held::instances(world, &items);
            let rows = stack_items(&values)
                .into_iter()
                .map(|stack| LootRow {
                    item: items[stack.first_index],
                    count: stack.count,
                    instance: values[stack.first_index].clone(),
                })
                .collect();
            LootSection { source, label: loot_source_label(world, source), rows, gold: source_gold(world, source) }
        })
        .collect();
    if sections.is_empty() {
        return None;
    }

    Some(LootWindowData {
        sections,
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
    style::dungeon_window(
        ctx,
        icons,
        "Loot",
        |window| {
            window
                .default_pos([
                    data.viewport_width / 2.0 - 150.0,
                    data.viewport_height / 2.0 - 100.0,
                ])
                .default_size([300.0, 200.0])
                .collapsible(false)
                .resizable(false)
        },
        |ui| {
            let has_contents = data.sections.iter().any(|s| !s.is_empty());

            egui::ScrollArea::vertical()
                .id_salt("loot_sections")
                .max_height(data.viewport_height * 0.6)
                .show(ui, |ui| {
                    for (n, section) in data.sections.iter().enumerate() {
                        if n > 0 {
                            ui.add_space(8.0);
                        }
                        draw_loot_section(ui, section, icons, actions);
                    }
                });

            ui.add_space(10.0);
            ui.separator();
            ui.horizontal(|ui| {
                if has_contents
                    && ui.button("Take All").clicked() {
                        actions.chest_take_all = true;
                    }
                if ui.button("Close").clicked() {
                    actions.close_chest = true;
                }
            });
        },
    );
}

/// One container's header, gold and item rows.
fn draw_loot_section(
    ui: &mut egui::Ui,
    section: &LootSection,
    icons: &UiIcons,
    actions: &mut UiActions,
) {
    style::panel_header(ui, &section.label);
    ui.add_space(6.0);

    if section.is_empty() {
        ui.label(
            egui::RichText::new("(empty)")
                .italics()
                .color(style::colors::TEXT_MUTED),
        );
        return;
    }

    if section.gold > 0 {
        ui.horizontal(|ui| {
            let coin_img = egui::Image::new(egui::load::SizedTexture::new(
                icons.items_texture_id,
                egui::vec2(32.0, 32.0),
            ))
            .uv(icons.coins_uv)
            .bg_fill(style::colors::PANEL_BG);

            if ui
                .add(egui::ImageButton::new(coin_img))
                .on_hover_text(format!("{} Gold\n\nClick to take", section.gold))
                .clicked()
            {
                actions.chest_take_gold = Some(section.source);
            }
            ui.label(format!("{} gold", section.gold));
        });
        ui.add_space(5.0);
    }

    for row in &section.rows {
        let instance = &row.instance;
        let item_type = instance.kind;
        ui.horizontal(|ui| {
            let uv = icons.get_item_uv(item_type);

            let image = egui::Image::new(egui::load::SizedTexture::new(
                icons.items_texture_id,
                egui::vec2(40.0, 40.0),
            ))
            .uv(uv)
            .tint(UiIcons::item_ui_tint(item_type))
            .bg_fill(style::colors::PANEL_BG);

            let item_name = instance.display_name();
            let label = if row.count > 1 {
                format!("{} x{}", item_name, row.count)
            } else {
                item_name.clone()
            };
            let response = ui.add(egui::ImageButton::new(image).frame(false));
            icons.paint_item_badge(ui.painter(), response.rect, item_type, egui::Color32::WHITE);

            // Affix lines for the hover tooltip. Unidentified gear
            // shows its rarity but hides components behind "???".
            let affix_text: String = if instance.identified {
                instance
                    .affixes
                    .iter()
                    .map(|a| format!("\n{}", a.describe()))
                    .collect()
            } else {
                instance.affixes.iter().map(|_| "\n???".to_string()).collect()
            };
            if response
                .on_hover_text(format!(
                    "{} ({}){}\n\nClick to take",
                    label,
                    instance.rarity.label(),
                    affix_text
                ))
                .clicked()
            {
                actions.chest_item_to_take = Some((section.source, row.item));
            }
            ui.label(egui::RichText::new(label).color(style::rarity_color(instance.rarity)));
        });
    }
}
