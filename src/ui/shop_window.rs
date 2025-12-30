//! Shop window UI component.
//!
//! Displays vendor inventory for buying and player inventory for selling.

use super::icons::UiIcons;
use super::style;
use super::UiActions;
use crate::components::{Dialogue, Inventory, ItemType, Vendor};
use crate::systems::item_defs::{get_price, get_sell_price};
use crate::systems::item_name;
use hecs::{Entity, World};

/// Data needed to render the shop window
pub struct ShopWindowData {
    pub vendor_name: String,
    /// Vendor items: (item type, price, stock count)
    pub vendor_items: Vec<(ItemType, u32, u32)>,
    pub vendor_gold: u32,
    /// Player items that can be sold: (item type, sell value)
    pub player_items: Vec<(ItemType, u32)>,
    pub player_gold: u32,
    pub viewport_width: f32,
    pub viewport_height: f32,
}

/// Extract shop window data from the world
pub fn get_shop_window_data(
    world: &World,
    shopping_at: Option<Entity>,
    player_entity: Entity,
    viewport_width: f32,
    viewport_height: f32,
) -> Option<ShopWindowData> {
    let vendor_id = shopping_at?;
    let vendor = world.get::<&Vendor>(vendor_id).ok()?;
    let dialogue = world.get::<&Dialogue>(vendor_id).ok()?;
    let player_inv = world.get::<&Inventory>(player_entity).ok()?;

    // Build vendor items with prices
    let vendor_items: Vec<(ItemType, u32, u32)> = vendor
        .inventory
        .iter()
        .filter(|(_, stock)| *stock > 0)
        .map(|(item, stock)| (*item, get_price(*item), *stock))
        .collect();

    // Build player sellable items with sell prices
    let player_items: Vec<(ItemType, u32)> = player_inv
        .items
        .iter()
        .map(|item| (*item, get_sell_price(*item)))
        .collect();

    Some(ShopWindowData {
        vendor_name: dialogue.name.clone(),
        vendor_items,
        vendor_gold: vendor.gold,
        player_items,
        player_gold: player_inv.gold,
        viewport_width,
        viewport_height,
    })
}

/// Render the shop window
pub fn draw_shop_window(
    ctx: &egui::Context,
    data: &ShopWindowData,
    icons: &UiIcons,
    actions: &mut UiActions,
) {
    egui::Window::new(format!("{}'s Shop", data.vendor_name))
        .default_pos([
            data.viewport_width / 2.0 - 250.0,
            data.viewport_height / 2.0 - 175.0,
        ])
        .default_size([500.0, 350.0])
        .collapsible(false)
        .resizable(false)
        .frame(style::dungeon_window_frame())
        .show(ctx, |ui| {
            // Two columns: Buy (left) and Sell (right)
            ui.columns(2, |columns| {
                // === BUY COLUMN ===
                columns[0].heading("Buy");
                columns[0].separator();

                if data.vendor_items.is_empty() {
                    columns[0].label(
                        egui::RichText::new("(nothing for sale)")
                            .italics()
                            .color(style::colors::TEXT_MUTED),
                    );
                } else {
                    egui::ScrollArea::vertical()
                        .id_salt("shop_buy")
                        .max_height(200.0)
                        .show(&mut columns[0], |ui| {
                            for (i, (item_type, price, stock)) in data.vendor_items.iter().enumerate() {
                                ui.horizontal(|ui| {
                                    // Item icon
                                    let uv = icons.get_item_uv(*item_type);
                                    let image = egui::Image::new(egui::load::SizedTexture::new(
                                        icons.items_texture_id,
                                        egui::vec2(32.0, 32.0),
                                    ))
                                    .uv(uv)
                                    .bg_fill(style::colors::PANEL_BG);
                                    ui.add(image);

                                    // Item name and stock
                                    ui.vertical(|ui| {
                                        ui.label(item_name(*item_type));
                                        ui.label(
                                            egui::RichText::new(format!("x{}", stock))
                                                .small()
                                                .color(style::colors::TEXT_MUTED),
                                        );
                                    });

                                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                        let can_afford = data.player_gold >= *price;
                                        let btn = egui::Button::new(format!("{} g", price));

                                        if ui.add_enabled(can_afford, btn)
                                            .on_hover_text(if can_afford {
                                                format!("Buy {} for {} gold", item_name(*item_type), price)
                                            } else {
                                                format!("Not enough gold (need {})", price)
                                            })
                                            .clicked()
                                        {
                                            actions.buy_item = Some(i);
                                        }
                                    });
                                });
                                ui.add_space(2.0);
                            }
                        });
                }

                // === SELL COLUMN ===
                columns[1].heading("Sell");
                columns[1].separator();

                if data.player_items.is_empty() {
                    columns[1].label(
                        egui::RichText::new("(nothing to sell)")
                            .italics()
                            .color(style::colors::TEXT_MUTED),
                    );
                } else {
                    egui::ScrollArea::vertical()
                        .id_salt("shop_sell")
                        .max_height(200.0)
                        .show(&mut columns[1], |ui| {
                            for (i, (item_type, sell_value)) in data.player_items.iter().enumerate() {
                                ui.horizontal(|ui| {
                                    // Item icon
                                    let uv = icons.get_item_uv(*item_type);
                                    let image = egui::Image::new(egui::load::SizedTexture::new(
                                        icons.items_texture_id,
                                        egui::vec2(32.0, 32.0),
                                    ))
                                    .uv(uv)
                                    .bg_fill(style::colors::PANEL_BG);
                                    ui.add(image);

                                    // Item name
                                    ui.label(item_name(*item_type));

                                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                        let vendor_can_buy = data.vendor_gold >= *sell_value;
                                        let btn = egui::Button::new(format!("+{} g", sell_value));

                                        if ui.add_enabled(vendor_can_buy, btn)
                                            .on_hover_text(if vendor_can_buy {
                                                format!("Sell {} for {} gold", item_name(*item_type), sell_value)
                                            } else {
                                                "Vendor doesn't have enough gold".to_string()
                                            })
                                            .clicked()
                                        {
                                            actions.sell_item = Some(i);
                                        }
                                    });
                                });
                                ui.add_space(2.0);
                            }
                        });
                }
            });

            ui.add_space(10.0);
            ui.separator();

            // Gold display and close button
            ui.horizontal(|ui| {
                // Player gold
                let coin_img = egui::Image::new(egui::load::SizedTexture::new(
                    icons.items_texture_id,
                    egui::vec2(20.0, 20.0),
                ))
                .uv(icons.coins_uv);
                ui.add(coin_img);
                ui.label(format!("Your gold: {}", data.player_gold));

                ui.add_space(20.0);

                // Vendor gold
                ui.label(
                    egui::RichText::new(format!("Vendor: {} g", data.vendor_gold))
                        .color(style::colors::TEXT_MUTED),
                );

                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.button("Close").clicked() {
                        actions.close_shop = true;
                    }
                });
            });
        });
}
