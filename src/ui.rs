//! UI rendering using egui.
//!
//! Handles all game UI: status bars, inventory, loot windows, etc.

use crate::components::{Container, Equipment, Health, Inventory, ItemType, Stats};
use crate::systems;
use crate::tile::tile_ids;
use crate::tileset::Tileset;
use hecs::World;

/// Data needed to render the status bar
pub struct StatusBarData {
    pub health_percent: f32,
    pub xp_progress: f32,
    pub xp_level: u32,
    pub gold: u32,
}

/// Data needed to render the loot window
pub struct LootWindowData {
    pub items: Vec<ItemType>,
    pub gold: u32,
    pub viewport_width: f32,
    pub viewport_height: f32,
}

/// Data needed to render the inventory window
pub struct InventoryWindowData {
    pub viewport_width: f32,
    pub viewport_height: f32,
}

/// Actions the UI wants to perform (returned to game logic)
#[derive(Default)]
pub struct UiActions {
    pub item_to_use: Option<usize>,
    pub chest_item_to_take: Option<usize>,
    pub chest_take_all: bool,
    pub chest_take_gold: bool,
    pub close_chest: bool,
}

/// Render the status bar (health, XP, gold)
pub fn draw_status_bar(
    ctx: &egui::Context,
    data: &StatusBarData,
    tileset_texture_id: egui::TextureId,
    coins_uv: egui::Rect,
) {
    egui::Window::new("Status")
        .fixed_pos([10.0, 10.0])
        .fixed_size([200.0, 90.0])
        .title_bar(false)
        .show(ctx, |ui| {
            ui.add(
                egui::ProgressBar::new(data.health_percent)
                    .text(format!("HP: {:.0}%", data.health_percent * 100.0)),
            );
            ui.add(
                egui::ProgressBar::new(data.xp_progress)
                    .fill(egui::Color32::from_rgb(100, 149, 237))
                    .text(format!(
                        "Lv {} - XP: {:.0}%",
                        data.xp_level,
                        data.xp_progress * 100.0
                    )),
            );
            ui.horizontal(|ui| {
                let coin_img = egui::Image::new(egui::load::SizedTexture::new(
                    tileset_texture_id,
                    egui::vec2(16.0, 16.0),
                ))
                .uv(coins_uv);
                ui.add(coin_img);
                ui.label(format!("{}", data.gold));
            });
        });
}

/// Render the loot window (chest/bones contents)
pub fn draw_loot_window(
    ctx: &egui::Context,
    data: &LootWindowData,
    tileset_texture_id: egui::TextureId,
    coins_uv: egui::Rect,
    potion_uv: egui::Rect,
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
        .show(ctx, |ui| {
            ui.heading("Contents");
            ui.separator();
            ui.add_space(10.0);

            let has_contents = !data.items.is_empty() || data.gold > 0;

            if !has_contents {
                ui.label(
                    egui::RichText::new("(empty)")
                        .italics()
                        .color(egui::Color32::GRAY),
                );
            } else {
                // Show gold if present
                if data.gold > 0 {
                    ui.horizontal(|ui| {
                        let coin_img = egui::Image::new(egui::load::SizedTexture::new(
                            tileset_texture_id,
                            egui::vec2(32.0, 32.0),
                        ))
                        .uv(coins_uv);

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
                        let uv = match item_type {
                            ItemType::HealthPotion => potion_uv,
                        };

                        let image = egui::Image::new(egui::load::SizedTexture::new(
                            tileset_texture_id,
                            egui::vec2(48.0, 48.0),
                        ))
                        .uv(uv);

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

/// Render the inventory/character window
pub fn draw_inventory_window(
    ctx: &egui::Context,
    world: &World,
    player_entity: hecs::Entity,
    data: &InventoryWindowData,
    tileset_texture_id: egui::TextureId,
    sword_uv: egui::Rect,
    coins_uv: egui::Rect,
    potion_uv: egui::Rect,
    actions: &mut UiActions,
) {
    egui::Window::new("Character")
        .default_pos([
            data.viewport_width / 2.0 - 300.0,
            data.viewport_height / 2.0 - 250.0,
        ])
        .default_size([600.0, 500.0])
        .collapsible(false)
        .resizable(true)
        .show(ctx, |ui| {
            if let Ok(stats) = world.get::<&Stats>(player_entity) {
                ui.columns(2, |columns| {
                    // Left column: Stats + Equipment
                    draw_stats_column(&mut columns[0], world, player_entity, &stats, tileset_texture_id, sword_uv, coins_uv);

                    // Right column: Inventory
                    draw_inventory_column(&mut columns[1], world, player_entity, tileset_texture_id, potion_uv, actions);
                });
            }
        });
}

fn draw_stats_column(
    ui: &mut egui::Ui,
    world: &World,
    player_entity: hecs::Entity,
    stats: &Stats,
    tileset_texture_id: egui::TextureId,
    sword_uv: egui::Rect,
    coins_uv: egui::Rect,
) {
    ui.vertical(|ui| {
        ui.heading("CHARACTER STATS");
        ui.separator();
        ui.add_space(10.0);
        ui.label(format!("Strength: {}", stats.strength));
        ui.add_space(5.0);
        ui.label(format!("Intelligence: {}", stats.intelligence));
        ui.add_space(5.0);
        ui.label(format!("Agility: {}", stats.agility));
        ui.add_space(10.0);
        ui.separator();

        let carry_capacity = stats.strength as f32 * 2.0;
        if let Ok(inventory) = world.get::<&Inventory>(player_entity) {
            ui.label(format!(
                "Weight: {:.1} / {:.1} kg",
                inventory.current_weight_kg, carry_capacity
            ));

            ui.add_space(10.0);
            ui.horizontal(|ui| {
                let coin_img = egui::Image::new(egui::load::SizedTexture::new(
                    tileset_texture_id,
                    egui::vec2(24.0, 24.0),
                ))
                .uv(coins_uv);
                ui.add(coin_img);
                ui.label(format!("{} gold", inventory.gold));
            });
        }

        ui.add_space(20.0);
        ui.heading("EQUIPMENT");
        ui.separator();
        ui.add_space(10.0);

        // Weapon slot
        ui.horizontal(|ui| {
            ui.label("Weapon:");
            if let Ok(equipment) = world.get::<&Equipment>(player_entity) {
                if let Some(weapon) = &equipment.weapon {
                    let image = egui::Image::new(egui::load::SizedTexture::new(
                        tileset_texture_id,
                        egui::vec2(48.0, 48.0),
                    ))
                    .uv(sword_uv);

                    ui.add(egui::ImageButton::new(image)).on_hover_text(format!(
                        "{}\n\nDamage: {} + {} = {}",
                        weapon.name,
                        weapon.base_damage,
                        weapon.damage_bonus,
                        systems::weapon_damage(weapon)
                    ));
                } else {
                    ui.label(
                        egui::RichText::new("(none)")
                            .italics()
                            .color(egui::Color32::GRAY),
                    );
                }
            }
        });
    });
}

fn draw_inventory_column(
    ui: &mut egui::Ui,
    world: &World,
    player_entity: hecs::Entity,
    tileset_texture_id: egui::TextureId,
    potion_uv: egui::Rect,
    actions: &mut UiActions,
) {
    ui.vertical(|ui| {
        ui.heading("INVENTORY");
        ui.separator();
        ui.add_space(10.0);

        if let Ok(inventory) = world.get::<&Inventory>(player_entity) {
            if inventory.items.is_empty() {
                ui.label(
                    egui::RichText::new("(empty)")
                        .italics()
                        .color(egui::Color32::GRAY),
                );
            } else {
                ui.horizontal_wrapped(|ui| {
                    for (i, item_type) in inventory.items.iter().enumerate() {
                        let uv = match item_type {
                            ItemType::HealthPotion => potion_uv,
                        };

                        let image = egui::Image::new(egui::load::SizedTexture::new(
                            tileset_texture_id,
                            egui::vec2(48.0, 48.0),
                        ))
                        .uv(uv);

                        let response = ui.add(egui::ImageButton::new(image));

                        if response
                            .on_hover_text(format!(
                                "{}\n\nClick to use",
                                systems::item_name(*item_type)
                            ))
                            .clicked()
                        {
                            actions.item_to_use = Some(i);
                        }
                    }
                });
            }
        }
    });
}

/// Helper struct containing pre-computed UV coordinates for UI icons
pub struct UiIcons {
    pub tileset_texture_id: egui::TextureId,
    pub sword_uv: egui::Rect,
    pub potion_uv: egui::Rect,
    pub coins_uv: egui::Rect,
}

impl UiIcons {
    pub fn new(tileset: &Tileset, tileset_egui_id: egui::TextureId) -> Self {
        Self {
            tileset_texture_id: tileset_egui_id,
            sword_uv: tileset.get_egui_uv(tile_ids::SWORD),
            potion_uv: tileset.get_egui_uv(tile_ids::RED_POTION),
            coins_uv: tileset.get_egui_uv(tile_ids::COINS),
        }
    }
}

/// Extract status bar data from the world
pub fn get_status_bar_data(world: &World, player_entity: hecs::Entity) -> StatusBarData {
    let health_percent = world
        .get::<&Health>(player_entity)
        .map(|h| (h.current as f32 / h.max as f32).clamp(0.0, 1.0))
        .unwrap_or(1.0);

    let gold = world
        .get::<&Inventory>(player_entity)
        .map(|inv| inv.gold)
        .unwrap_or(0);

    let (xp_progress, xp_level) = world
        .get::<&crate::components::Experience>(player_entity)
        .map(|exp| (systems::xp_progress(&exp), exp.level))
        .unwrap_or((0.0, 1));

    StatusBarData {
        health_percent,
        xp_progress,
        xp_level,
        gold,
    }
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
