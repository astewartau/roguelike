//! Inventory and character window UI component.
//!
//! Displays player stats, equipment, and inventory with context menus.

use super::icons::UiIcons;
use super::style;
use super::GameUiState;
use super::UiActions;
use crate::components::{Equipment, Inventory, Stats};
use crate::systems;
use hecs::World;

/// Data needed to render the inventory window
pub struct InventoryWindowData {
    pub viewport_width: f32,
    pub viewport_height: f32,
}

/// Render the inventory/character window
pub fn draw_inventory_window(
    ctx: &egui::Context,
    world: &World,
    player_entity: hecs::Entity,
    data: &InventoryWindowData,
    icons: &UiIcons,
    ui_state: &mut GameUiState,
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
        .frame(style::dungeon_window_frame())
        .show(ctx, |ui| {
            if let Ok(stats) = world.get::<&Stats>(player_entity) {
                ui.columns(2, |columns| {
                    // Left column: Stats + Equipment
                    draw_stats_column(
                        &mut columns[0],
                        world,
                        player_entity,
                        &stats,
                        icons,
                        ui_state,
                        actions,
                    );

                    // Right column: Inventory
                    draw_inventory_column(
                        &mut columns[1],
                        world,
                        player_entity,
                        icons,
                        ui_state,
                        actions,
                    );
                });
            }
        });

    // Draw context menu popup (outside the main window)
    draw_item_context_menu(ctx, world, player_entity, ui_state, actions);

    // Draw equipped item context menu popup
    draw_equipped_context_menu(ctx, world, player_entity, ui_state, actions);
}

fn draw_stats_column(
    ui: &mut egui::Ui,
    world: &World,
    player_entity: hecs::Entity,
    stats: &Stats,
    icons: &UiIcons,
    ui_state: &mut GameUiState,
    actions: &mut UiActions,
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
                let size = egui::vec2(24.0, 24.0);
                let (rect, _response) = ui.allocate_exact_size(size, egui::Sense::hover());

                ui.painter().rect_filled(rect, 0.0, egui::Color32::BLACK);

                let coin_img = egui::Image::new(egui::load::SizedTexture::new(
                    icons.items_texture_id,
                    size,
                ))
                .uv(icons.coins_uv);
                coin_img.paint_at(ui, rect);

                ui.label(format!("{} gold", inventory.gold));
            });
        }

        ui.add_space(20.0);
        ui.heading("EQUIPMENT");
        ui.separator();
        ui.add_space(10.0);

        if let Ok(equipment) = world.get::<&Equipment>(player_entity) {
            // Single weapon slot
            ui.horizontal(|ui| {
                ui.label("Weapon:");
                match &equipment.weapon {
                    Some(crate::components::EquippedWeapon::Melee(weapon)) => {
                        let size = egui::vec2(48.0, 48.0);
                        let (rect, response) = ui.allocate_exact_size(size, egui::Sense::click());

                        ui.painter().rect_filled(rect, 0.0, egui::Color32::BLACK);

                        // Use correct icon based on weapon type
                        let weapon_uv = match weapon.name.as_str() {
                            "Dagger" => icons.dagger_uv,
                            "Staff" => icons.staff_uv,
                            _ => icons.sword_uv,
                        };
                        let image = egui::Image::new(egui::load::SizedTexture::new(
                            icons.items_texture_id,
                            size,
                        ))
                        .uv(weapon_uv);
                        image.paint_at(ui, rect);

                        let response = response.on_hover_text(format!(
                            "{}\n\nDamage: {} + {} = {}\n\nClick to unequip\nRight-click for options",
                            weapon.name,
                            weapon.base_damage,
                            weapon.damage_bonus,
                            systems::weapon_damage(weapon)
                        ));

                        // Left-click unequips
                        if response.clicked() {
                            actions.unequip_weapon = true;
                        }

                        // Right-click opens context menu
                        if response.secondary_clicked() {
                            ui_state.equipped_context_menu = Some(response.rect.right_top());
                        }
                    }
                    Some(crate::components::EquippedWeapon::Ranged(bow)) => {
                        let size = egui::vec2(48.0, 48.0);
                        let (rect, response) = ui.allocate_exact_size(size, egui::Sense::click());

                        ui.painter().rect_filled(rect, 0.0, egui::Color32::BLACK);

                        let image = egui::Image::new(egui::load::SizedTexture::new(
                            icons.items_texture_id,
                            size,
                        ))
                        .uv(icons.bow_uv);
                        image.paint_at(ui, rect);

                        let response = response.on_hover_text(format!(
                            "{}\n\nDamage: {}\nSpeed: {:.0} tiles/sec\n\nClick to unequip\nRight-click for options",
                            bow.name, bow.base_damage, bow.arrow_speed
                        ));

                        // Left-click unequips
                        if response.clicked() {
                            actions.unequip_weapon = true;
                        }

                        // Right-click opens context menu
                        if response.secondary_clicked() {
                            ui_state.equipped_context_menu = Some(response.rect.right_top());
                        }
                    }
                    None => {
                        ui.label(
                            egui::RichText::new("(none)")
                                .italics()
                                .color(style::colors::TEXT_MUTED),
                        );
                    }
                }
            });
        }
    });
}

/// Represents an inventory slot for display (may be a stack or single item)
struct InventorySlot {
    item_type: crate::components::ItemType,
    count: u32,
    first_index: usize, // Index of first occurrence in inventory
}

fn draw_inventory_column(
    ui: &mut egui::Ui,
    world: &World,
    player_entity: hecs::Entity,
    icons: &UiIcons,
    ui_state: &mut GameUiState,
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
                        .color(style::colors::TEXT_MUTED),
                );
            } else {
                // Build display slots: group stackable items, keep others separate
                let slots = build_inventory_slots(&inventory.items);

                ui.horizontal_wrapped(|ui| {
                    for slot in &slots {
                        let uv = icons.get_item_uv(slot.item_type);
                        let is_throwable = systems::items::item_is_throwable(slot.item_type);

                        // Allocate space and paint black background manually
                        let size = egui::vec2(48.0, 48.0);
                        let (rect, response) = ui.allocate_exact_size(size, egui::Sense::click());

                        // Paint black background first
                        ui.painter().rect_filled(rect, 0.0, egui::Color32::BLACK);

                        // Then paint the image on top
                        let image = egui::Image::new(egui::load::SizedTexture::new(
                            icons.items_texture_id,
                            size,
                        ))
                        .uv(uv);
                        image.paint_at(ui, rect);

                        // Draw stack count if more than 1
                        if slot.count > 1 {
                            let count_text = format!("{}", slot.count);
                            // Draw shadow/outline for visibility
                            let text_pos = rect.right_bottom() + egui::vec2(-4.0, -4.0);
                            ui.painter().text(
                                text_pos + egui::vec2(1.0, 1.0),
                                egui::Align2::RIGHT_BOTTOM,
                                &count_text,
                                egui::FontId::proportional(14.0),
                                egui::Color32::BLACK,
                            );
                            ui.painter().text(
                                text_pos,
                                egui::Align2::RIGHT_BOTTOM,
                                &count_text,
                                egui::FontId::proportional(14.0),
                                egui::Color32::WHITE,
                            );
                        }

                        // Build hover text based on item type
                        let hover_text = if slot.count > 1 {
                            format!(
                                "{} (x{})\n\nRight-click for options",
                                systems::item_name(slot.item_type),
                                slot.count
                            )
                        } else if is_throwable {
                            format!(
                                "{}\n\nLeft-click to drink\nRight-click for options",
                                systems::item_name(slot.item_type)
                            )
                        } else {
                            format!(
                                "{}\n\nLeft-click to use\nRight-click for options",
                                systems::item_name(slot.item_type)
                            )
                        };

                        let response = response.on_hover_text(hover_text);

                        // Left-click: use/drink the item (only for single items or non-stackables)
                        if response.clicked() && slot.count == 1 {
                            actions.item_to_use = Some(slot.first_index);
                        }

                        // Right-click: open context menu (for all items)
                        if response.secondary_clicked() {
                            // Get the screen position for the popup
                            let pos = response.rect.right_top();
                            ui_state.item_context_menu = Some((slot.first_index, pos));
                        }
                    }
                });
            }
        }
    });
}

/// Build inventory display slots, grouping stackable items together
fn build_inventory_slots(items: &[crate::components::ItemType]) -> Vec<InventorySlot> {
    use std::collections::HashMap;

    let mut slots = Vec::new();
    let mut stackable_counts: HashMap<crate::components::ItemType, (u32, usize)> = HashMap::new();

    for (i, item_type) in items.iter().enumerate() {
        if item_type.is_stackable() {
            // Track count and first index for stackable items
            stackable_counts
                .entry(*item_type)
                .and_modify(|(count, _)| *count += 1)
                .or_insert((1, i));
        } else {
            // Non-stackable items get their own slot
            slots.push(InventorySlot {
                item_type: *item_type,
                count: 1,
                first_index: i,
            });
        }
    }

    // Add stackable items as single slots with counts
    for (item_type, (count, first_index)) in stackable_counts {
        slots.push(InventorySlot {
            item_type,
            count,
            first_index,
        });
    }

    // Sort slots so stackable items appear at the end (or you could sort differently)
    slots.sort_by_key(|s| (s.item_type.is_stackable(), s.first_index));

    slots
}

fn draw_item_context_menu(
    ctx: &egui::Context,
    world: &World,
    player_entity: hecs::Entity,
    ui_state: &mut GameUiState,
    actions: &mut UiActions,
) {
    if let Some((item_idx, pos)) = ui_state.item_context_menu {
        // Get the item type and count to show appropriate options
        let (item_type, stack_count) = world
            .get::<&Inventory>(player_entity)
            .ok()
            .map(|inv| {
                if let Some(&item) = inv.items.get(item_idx) {
                    let count = if item.is_stackable() {
                        inv.items.iter().filter(|&&i| i == item).count() as u32
                    } else {
                        1
                    };
                    (Some(item), count)
                } else {
                    (None, 0)
                }
            })
            .unwrap_or((None, 0));

        if let Some(item_type) = item_type {
            let is_throwable = systems::items::item_is_throwable(item_type);
            let is_stackable = item_type.is_stackable();

            egui::Area::new(egui::Id::new("item_context_menu"))
                .fixed_pos(pos)
                .order(egui::Order::Foreground)
                .show(ctx, |ui| {
                    style::dungeon_window_frame().show(ui, |ui| {
                        ui.set_min_width(120.0);

                        // Show item name with count for stacks
                        if stack_count > 1 {
                            ui.label(
                                egui::RichText::new(format!(
                                    "{} (x{})",
                                    systems::item_name(item_type),
                                    stack_count
                                ))
                                .color(style::colors::TEXT_PRIMARY),
                            );
                            ui.separator();
                        }

                        // Show options based on item type (not for stackable ammo)
                        if !is_stackable {
                            if is_throwable {
                                if ui.button("Drink").clicked() {
                                    actions.item_to_use = Some(item_idx);
                                    ui_state.item_context_menu = None;
                                }
                                if ui.button("Throw").clicked() {
                                    actions.item_to_throw = Some(item_idx);
                                    ui_state.item_context_menu = None;
                                }
                            } else {
                                // Non-throwable items: Use/Equip
                                let is_weapon = matches!(
                                    item_type,
                                    crate::components::ItemType::Sword
                                        | crate::components::ItemType::Bow
                                );
                                let button_text = if is_weapon { "Equip" } else { "Use" };
                                if ui.button(button_text).clicked() {
                                    actions.item_to_use = Some(item_idx);
                                    ui_state.item_context_menu = None;
                                }
                            }
                        }

                        // Drop option - shows "Drop" for single items, "Drop One" for stacks
                        let drop_text = if stack_count > 1 { "Drop One" } else { "Drop" };
                        if ui.button(drop_text).clicked() {
                            actions.item_to_drop = Some(item_idx);
                            ui_state.item_context_menu = None;
                        }

                        ui.separator();
                        if ui.button("Cancel").clicked() {
                            ui_state.item_context_menu = None;
                        }
                    });
                });

            // Close context menu on ESC key
            if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
                ui_state.item_context_menu = None;
            }

            // Close context menu if left-clicked elsewhere (not on the popup)
            if ctx.input(|i| i.pointer.primary_clicked()) {
                // Check if click was outside the popup
                let popup_rect = egui::Rect::from_min_size(pos, egui::vec2(120.0, 100.0));
                if let Some(pointer_pos) = ctx.input(|i| i.pointer.interact_pos()) {
                    if !popup_rect.contains(pointer_pos) {
                        ui_state.item_context_menu = None;
                    }
                }
            }
        } else {
            // Item no longer exists, close menu
            ui_state.item_context_menu = None;
        }
    }
}

fn draw_equipped_context_menu(
    ctx: &egui::Context,
    world: &World,
    player_entity: hecs::Entity,
    ui_state: &mut GameUiState,
    actions: &mut UiActions,
) {
    if let Some(pos) = ui_state.equipped_context_menu {
        // Check if player still has a weapon equipped
        let has_weapon = world
            .get::<&Equipment>(player_entity)
            .map(|eq| eq.weapon.is_some())
            .unwrap_or(false);

        if has_weapon {
            egui::Area::new(egui::Id::new("equipped_context_menu"))
                .fixed_pos(pos)
                .order(egui::Order::Foreground)
                .show(ctx, |ui| {
                    style::dungeon_window_frame().show(ui, |ui| {
                        ui.set_min_width(120.0);

                        if ui.button("Unequip").clicked() {
                            actions.unequip_weapon = true;
                            ui_state.equipped_context_menu = None;
                        }
                        if ui.button("Drop").clicked() {
                            actions.drop_equipped_weapon = true;
                            ui_state.equipped_context_menu = None;
                        }

                        ui.separator();
                        if ui.button("Cancel").clicked() {
                            ui_state.equipped_context_menu = None;
                        }
                    });
                });

            // Close context menu on ESC key
            if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
                ui_state.equipped_context_menu = None;
            }

            // Close context menu if left-clicked elsewhere
            if ctx.input(|i| i.pointer.primary_clicked()) {
                let popup_rect = egui::Rect::from_min_size(pos, egui::vec2(120.0, 100.0));
                if let Some(pointer_pos) = ctx.input(|i| i.pointer.interact_pos()) {
                    if !popup_rect.contains(pointer_pos) {
                        ui_state.equipped_context_menu = None;
                    }
                }
            }
        } else {
            // Weapon no longer equipped, close menu
            ui_state.equipped_context_menu = None;
        }
    }
}
