//! Developer menu UI component.
//!
//! Provides tools for spawning entities and adding items during development.

use super::icons::UiIcons;
use super::style;
use crate::components::ItemType;
use crate::multi_tileset::MultiTileset;
use crate::systems::items::{item_name, item_sprite};
use crate::tile::tile_ids;
use crate::tile::SpriteSheet;

/// Placement tools - click on map to spawn
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum DevTool {
    SpawnChest,
    SpawnEnemy,
    SpawnFire,
    SpawnStairsDown,
    SpawnStairsUp,
}

impl DevTool {
    pub fn name(&self) -> &'static str {
        match self {
            DevTool::SpawnChest => "Chest",
            DevTool::SpawnEnemy => "Enemy",
            DevTool::SpawnFire => "Fire",
            DevTool::SpawnStairsDown => "Stairs Down",
            DevTool::SpawnStairsUp => "Stairs Up",
        }
    }

    pub fn sprite(&self) -> (SpriteSheet, u32) {
        match self {
            DevTool::SpawnChest => tile_ids::CHEST_CLOSED,
            DevTool::SpawnEnemy => tile_ids::SKELETON,
            DevTool::SpawnFire => tile_ids::RED_POTION,
            DevTool::SpawnStairsDown => tile_ids::STAIRS_DOWN,
            DevTool::SpawnStairsUp => tile_ids::STAIRS_UP,
        }
    }

    pub const ALL: [DevTool; 5] = [
        DevTool::SpawnChest,
        DevTool::SpawnEnemy,
        DevTool::SpawnFire,
        DevTool::SpawnStairsDown,
        DevTool::SpawnStairsUp,
    ];
}

/// All item types for the dev menu
const ALL_ITEMS: [ItemType; 13] = [
    // Potions
    ItemType::HealthPotion,
    ItemType::RegenerationPotion,
    ItemType::StrengthPotion,
    ItemType::ConfusionPotion,
    // Scrolls
    ItemType::ScrollOfInvisibility,
    ItemType::ScrollOfSpeed,
    ItemType::ScrollOfProtection,
    ItemType::ScrollOfBlink,
    ItemType::ScrollOfFear,
    ItemType::ScrollOfFireball,
    ItemType::ScrollOfReveal,
    ItemType::ScrollOfMapping,
    ItemType::ScrollOfSlow,
];

/// State for the developer menu
pub struct DevMenu {
    pub visible: bool,
    pub selected_tool: Option<DevTool>,
    /// Item to add to player inventory (set when an item is clicked)
    pub item_to_give: Option<ItemType>,
}

impl Default for DevMenu {
    fn default() -> Self {
        Self {
            visible: false,
            selected_tool: None,
            item_to_give: None,
        }
    }
}

impl DevMenu {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn toggle(&mut self) {
        self.visible = !self.visible;
        if !self.visible {
            self.selected_tool = None;
        }
    }

    pub fn has_active_tool(&self) -> bool {
        self.visible && self.selected_tool.is_some()
    }

    /// Take the pending item to give (clears it after reading)
    pub fn take_item_to_give(&mut self) -> Option<ItemType> {
        self.item_to_give.take()
    }
}

/// Helper to draw a tile button
#[allow(dead_code)] // Reserved for dev tool palette expansion
fn draw_tile_button(
    ui: &mut egui::Ui,
    tileset_texture_id: egui::TextureId,
    uv_rect: egui::Rect,
    is_selected: bool,
    tooltip: &str,
) -> egui::Response {
    let button_size = egui::vec2(36.0, 36.0);
    let (rect, response) = ui.allocate_exact_size(button_size, egui::Sense::click());

    let bg_color = if is_selected {
        style::colors::SELECTED
    } else if response.hovered() {
        style::colors::HOVERED
    } else {
        style::colors::BUTTON_BG
    };
    ui.painter().rect_filled(rect, 0.0, bg_color);

    let image_rect = rect.shrink(3.0);
    ui.painter().image(
        tileset_texture_id,
        image_rect,
        uv_rect,
        egui::Color32::WHITE,
    );

    response.on_hover_text(tooltip)
}

/// Draw a list item with icon and label (for dev menu)
fn draw_list_item(
    ui: &mut egui::Ui,
    tileset_texture_id: egui::TextureId,
    uv_rect: egui::Rect,
    is_selected: bool,
    label: &str,
) -> egui::Response {
    let icon_size = 24.0;
    let total_height = 28.0;
    let available_width = ui.available_width();

    let (rect, response) = ui.allocate_exact_size(
        egui::vec2(available_width, total_height),
        egui::Sense::click(),
    );

    let bg_color = if is_selected {
        style::colors::SELECTED
    } else if response.hovered() {
        style::colors::HOVERED
    } else {
        egui::Color32::TRANSPARENT
    };

    ui.painter().rect_filled(rect, 0.0, bg_color);

    // Draw icon
    let icon_rect = egui::Rect::from_min_size(
        rect.min + egui::vec2(4.0, (total_height - icon_size) / 2.0),
        egui::vec2(icon_size, icon_size),
    );
    ui.painter().image(
        tileset_texture_id,
        icon_rect,
        uv_rect,
        egui::Color32::WHITE,
    );

    // Draw label
    let text_pos = egui::pos2(rect.min.x + icon_size + 12.0, rect.center().y);
    ui.painter().text(
        text_pos,
        egui::Align2::LEFT_CENTER,
        label,
        egui::FontId::monospace(14.0),
        style::colors::TEXT_PRIMARY,
    );

    response
}

/// Draw the developer menu
pub fn draw_dev_menu(
    ctx: &egui::Context,
    dev_menu: &mut DevMenu,
    icons: &UiIcons,
    tileset: &MultiTileset,
) {
    if !dev_menu.visible {
        return;
    }

    egui::Window::new("Developer Tools")
        .fixed_pos([10.0, 120.0])
        .min_width(200.0)
        .max_height(500.0)
        .title_bar(true)
        .collapsible(true)
        .scroll([false, true])
        .frame(style::dungeon_window_frame())
        .show(ctx, |ui| {
            // === PLACEMENT TOOLS ===
            ui.heading("Placement (click map)");
            for tool in DevTool::ALL {
                let is_selected = dev_menu.selected_tool == Some(tool);
                let sprite = tool.sprite();
                let texture_id = icons.texture_for_sheet(sprite.0);
                let uv_rect = tileset.get_egui_uv(sprite.0, sprite.1);

                let response = draw_list_item(ui, texture_id, uv_rect, is_selected, tool.name());

                if response.clicked() {
                    if is_selected {
                        dev_menu.selected_tool = None;
                    } else {
                        dev_menu.selected_tool = Some(tool);
                    }
                }
            }

            if let Some(tool) = dev_menu.selected_tool {
                ui.add_space(4.0);
                ui.label(format!("→ Click map to place {}", tool.name()));
            }

            ui.add_space(8.0);
            ui.separator();

            // === ITEMS (click to add to inventory) ===
            ui.heading("Items (click to add)");

            // Potions
            ui.label("Potions:");
            for item in ALL_ITEMS.iter().take(4) {
                let sprite = item_sprite(*item);
                let texture_id = icons.texture_for_sheet(sprite.0);
                let uv_rect = tileset.get_egui_uv(sprite.0, sprite.1);
                let response = draw_list_item(ui, texture_id, uv_rect, false, item_name(*item));

                if response.clicked() {
                    dev_menu.item_to_give = Some(*item);
                }
            }

            ui.add_space(4.0);

            // Scrolls
            ui.label("Scrolls:");
            for item in ALL_ITEMS.iter().skip(4) {
                let sprite = item_sprite(*item);
                let texture_id = icons.texture_for_sheet(sprite.0);
                let uv_rect = tileset.get_egui_uv(sprite.0, sprite.1);
                let response = draw_list_item(ui, texture_id, uv_rect, false, item_name(*item));

                if response.clicked() {
                    dev_menu.item_to_give = Some(*item);
                }
            }
        });
}
