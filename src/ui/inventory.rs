//! Inventory and character window UI component.
//!
//! Displays player stats, equipment, and inventory with context menus.

use super::icons::UiIcons;
use super::style;
use super::{ability_icon, ability_status, CharacterTab, GameUiState, HotbarDrag, HotbarEntry, UiActions};
use crate::components::{
    AbilityType, ClassAbility, Equipment, Inventory, RangerAbilities, SecondaryAbility, Stats,
};
use crate::systems;
use hecs::World;

/// Data needed to render the inventory window
pub struct InventoryWindowData {
    pub viewport_width: f32,
    pub viewport_height: f32,
}

/// Color used for identified curse lines in tooltips.
const CURSE_TEXT: egui::Color32 = egui::Color32::from_rgb(205, 90, 80);

/// Rarity + affix block for a gear tooltip.
///
/// Unidentified items show their rarity color and an "Unidentified" tag but
/// hide their affix lines behind "???" (Legendary names are hidden through
/// `ItemInstance::display_name`). Identified curse lines render red.
fn affix_block_ui(ui: &mut egui::Ui, inst: &crate::components::ItemInstance) {
    use crate::components::Rarity;
    if inst.rarity == Rarity::Common && inst.affixes.is_empty() {
        return;
    }
    ui.add_space(4.0);
    ui.label(egui::RichText::new(inst.rarity.label()).color(style::rarity_color(inst.rarity)));
    if !inst.identified {
        ui.label(
            egui::RichText::new("Unidentified")
                .italics()
                .color(style::colors::TEXT_MUTED),
        );
        for _ in &inst.affixes {
            ui.label(egui::RichText::new("???").color(style::colors::TEXT_MUTED));
        }
    } else {
        for affix in &inst.affixes {
            let color = if affix.is_curse() {
                CURSE_TEXT
            } else {
                style::colors::TEXT_PRIMARY
            };
            ui.label(egui::RichText::new(affix.describe()).color(color));
        }
    }
}

/// Muted footer hint lines for a tooltip ("Click to unequip", ...).
fn tooltip_hint_ui(ui: &mut egui::Ui, hint: &str) {
    ui.add_space(4.0);
    ui.label(egui::RichText::new(hint).small().color(style::colors::TEXT_MUTED));
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
    style::dungeon_window(
        ctx,
        icons,
        "Character",
        |window| {
            window
                .default_pos([
                    data.viewport_width / 2.0 - 300.0,
                    data.viewport_height / 2.0 - 250.0,
                ])
                .default_size([600.0, 500.0])
                .collapsible(false)
                .resizable(true)
        },
        |ui| {
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

                    // Right column: Inventory / Spellbook tabs
                    let col = &mut columns[1];
                    col.horizontal(|ui| {
                        ui.selectable_value(
                            &mut ui_state.character_tab,
                            CharacterTab::Inventory,
                            "INVENTORY",
                        );
                        ui.selectable_value(
                            &mut ui_state.character_tab,
                            CharacterTab::Spellbook,
                            "SPELLBOOK",
                        );
                    });
                    col.separator();
                    col.add_space(6.0);
                    match ui_state.character_tab {
                        CharacterTab::Inventory => draw_inventory_column(
                            col,
                            world,
                            player_entity,
                            icons,
                            ui_state,
                            actions,
                        ),
                        CharacterTab::Spellbook => {
                            draw_spellbook_column(col, world, player_entity, icons)
                        }
                    }
                });
            }
        },
    );

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
        style::panel_header(ui, "CHARACTER STATS");
        ui.add_space(10.0);
        // Effective stats include stat affixes on equipped gear; show the
        // gear contribution alongside the base value.
        let effective = crate::queries::effective_stats(world, player_entity);
        let stat_line = |label: &str, base: i32, eff: i32| {
            if eff != base {
                format!("{}: {} ({:+})", label, eff, eff - base)
            } else {
                format!("{}: {}", label, base)
            }
        };
        ui.label(stat_line("Strength", stats.strength, effective.strength));
        ui.add_space(5.0);
        ui.label(stat_line("Intelligence", stats.intelligence, effective.intelligence));
        ui.add_space(5.0);
        ui.label(stat_line("Agility", stats.agility, effective.agility));
        ui.add_space(10.0);
        ui.separator();

        let carry_capacity = effective.strength as f32 * 2.0;
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
        style::panel_header(ui, "EQUIPMENT");
        ui.add_space(10.0);

        if let Ok(equipment) = world.get::<&Equipment>(player_entity) {
            let weapon_source = equipment.weapon_source.as_ref();

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

                        // Unidentified sources hide their (Legendary) name
                        let title = weapon_source
                            .map(|inst| inst.display_name())
                            .unwrap_or_else(|| weapon.name.clone());
                        let response = response.on_hover_ui(|ui| {
                            ui.label(egui::RichText::new(title).strong());
                            ui.label(format!(
                                "Damage: {} + {} = {}",
                                weapon.base_damage,
                                weapon.damage_bonus,
                                systems::weapon_damage(weapon),
                            ));
                            if let Some(inst) = weapon_source {
                                affix_block_ui(ui, inst);
                            }
                            tooltip_hint_ui(ui, "Click to unequip\nRight-click for options");
                        });

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

                        let title = weapon_source
                            .map(|inst| inst.display_name())
                            .unwrap_or_else(|| bow.name.clone());
                        let response = response.on_hover_ui(|ui| {
                            ui.label(egui::RichText::new(title).strong());
                            ui.label(format!(
                                "Damage: {}\nSpeed: {:.0} tiles/sec",
                                bow.base_damage, bow.arrow_speed,
                            ));
                            if let Some(inst) = weapon_source {
                                affix_block_ui(ui, inst);
                            }
                            tooltip_hint_ui(ui, "Click to unequip\nRight-click for options");
                        });

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

            // Armor + accessory slots (body / head / ring / amulet)
            use crate::systems::item_defs::{armor_base_defense, is_accessory_kind, ArmorSlot};
            for (label, slot, piece) in [
                ("Body:", ArmorSlot::Body, &equipment.body),
                ("Head:", ArmorSlot::Head, &equipment.head),
                ("Ring:", ArmorSlot::Ring, &equipment.ring),
                ("Amulet:", ArmorSlot::Amulet, &equipment.amulet),
            ] {
                ui.horizontal(|ui| {
                    ui.label(label);
                    match piece {
                        Some(instance) => {
                            let size = egui::vec2(48.0, 48.0);
                            let (rect, response) = ui.allocate_exact_size(size, egui::Sense::click());
                            ui.painter().rect_filled(rect, 0.0, egui::Color32::BLACK);
                            let image = egui::Image::new(egui::load::SizedTexture::new(
                                icons.items_texture_id,
                                size,
                            ))
                            .uv(icons.get_item_uv(instance.kind));
                            image.paint_at(ui, rect);

                            let response = response.on_hover_ui(|ui| {
                                ui.label(egui::RichText::new(instance.display_name()).strong());
                                // Accessories are pure affix carriers: no defense line.
                                // Unidentified armor only shows its known base defense.
                                if !is_accessory_kind(instance.kind) {
                                    let defense = armor_base_defense(instance.kind)
                                        + if instance.identified {
                                            instance.defense_bonus()
                                        } else {
                                            0
                                        };
                                    ui.label(format!("Defense: {}", defense));
                                }
                                affix_block_ui(ui, instance);
                                tooltip_hint_ui(ui, "Click to unequip");
                            });
                            if response.clicked() {
                                actions.unequip_armor = Some(slot);
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

            // Total defense readout
            ui.add_space(4.0);
            ui.label(format!("Defense: {}", equipment.total_defense()));
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
        style::panel_header(ui, "INVENTORY");
        ui.add_space(10.0);

        // Which ammo the bow currently loads (for the tooltip marker)
        let active_ammo = world
            .get::<&crate::components::ActiveAmmo>(player_entity)
            .map(|a| a.kind)
            .unwrap_or(crate::components::ItemType::Arrow);

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
                        // (click_and_drag so the slot can both be used and dragged to the hotbar)
                        let size = egui::vec2(48.0, 48.0);
                        let (rect, response) =
                            ui.allocate_exact_size(size, egui::Sense::click_and_drag());

                        // Paint black background first
                        ui.painter().rect_filled(rect, 0.0, egui::Color32::BLACK);

                        // Then paint the image on top (fire arrows tinted orange)
                        let image = egui::Image::new(egui::load::SizedTexture::new(
                            icons.items_texture_id,
                            size,
                        ))
                        .uv(uv)
                        .tint(UiIcons::item_ui_tint(slot.item_type));
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

                        // Rarity + affix block for gear (looked up from the backing
                        // instance); identified Legendary items show their name,
                        // unidentified gear hides affixes behind "???".
                        let backing = inventory.items.get(slot.first_index);
                        let item_display_name = backing
                            .map(|inst| inst.display_name())
                            .unwrap_or_else(|| systems::item_name(slot.item_type).to_string());

                        // Rarity-colored border so Magic/Rare/Legendary gear stands out
                        if let Some(inst) = backing {
                            if inst.rarity != crate::components::Rarity::Common {
                                ui.painter().rect_stroke(
                                    rect,
                                    0.0,
                                    egui::Stroke::new(2.0, style::rarity_color(inst.rarity)),
                                );
                            }
                        }

                        // Ammo stacks show whether they're the loaded ammo type
                        let ammo_text = if slot.item_type.is_ammo() {
                            if slot.item_type == active_ammo {
                                Some("Active ammo")
                            } else {
                                Some("Right-click to use as ammo")
                            }
                        } else {
                            None
                        };

                        let response = response.on_hover_ui(|ui| {
                            let title = if slot.count > 1 {
                                format!("{} (x{})", item_display_name, slot.count)
                            } else {
                                item_display_name.clone()
                            };
                            ui.label(egui::RichText::new(title).strong());
                            if let Some(inst) = backing {
                                affix_block_ui(ui, inst);
                            }
                            if let Some(ammo) = ammo_text {
                                ui.add_space(4.0);
                                ui.label(ammo);
                            }
                            let hint = if slot.count > 1 {
                                "Right-click for options"
                            } else if is_throwable {
                                "Left-click to drink\nRight-click for options"
                            } else {
                                "Left-click to use\nRight-click for options"
                            };
                            tooltip_hint_ui(ui, hint);
                        });

                        // Drag source: carry this item type to the hotbar (ammo excluded)
                        if response.drag_started() && !slot.item_type.is_ammo() {
                            response.dnd_set_drag_payload(HotbarDrag::external(HotbarEntry::Item(
                                slot.item_type,
                            )));
                        }

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

/// Render the spellbook: the player's abilities as drag sources for the hotbar.
fn draw_spellbook_column(
    ui: &mut egui::Ui,
    world: &World,
    player_entity: hecs::Entity,
    icons: &UiIcons,
) {
    ui.vertical(|ui| {
        style::panel_header(ui, "SPELLBOOK");
        ui.add_space(10.0);

        // Collect the player's abilities (class, then secondary, then ranger).
        let mut abilities: Vec<AbilityType> = Vec::new();
        if let Ok(a) = world.get::<&ClassAbility>(player_entity) {
            abilities.push(a.ability_type);
        }
        if let Ok(a) = world.get::<&SecondaryAbility>(player_entity) {
            abilities.push(a.ability_type);
        }
        if let Ok(ra) = world.get::<&RangerAbilities>(player_entity) {
            for (at, _, _) in ra.abilities.iter() {
                abilities.push(*at);
            }
        }
        // Learned spells (studied scrolls + the Necromancer's Raise Dead).
        if let Ok(la) = world.get::<&crate::components::LearnedAbilities>(player_entity) {
            for spell in la.spells.iter() {
                abilities.push(spell.ability);
            }
        }
        // Rest is a universal ability available to every class.
        abilities.push(AbilityType::Rest);

        if abilities.is_empty() {
            ui.label(
                egui::RichText::new("(no abilities)")
                    .italics()
                    .color(style::colors::TEXT_MUTED),
            );
            return;
        }

        ui.label(
            egui::RichText::new("Drag a spell onto a hotbar slot")
                .small()
                .color(style::colors::TEXT_MUTED),
        );
        ui.add_space(8.0);

        ui.horizontal_wrapped(|ui| {
            for ability in abilities {
                let size = egui::vec2(48.0, 48.0);
                let (rect, response) =
                    ui.allocate_exact_size(size, egui::Sense::click_and_drag());

                ui.painter().rect_filled(rect, 0.0, style::colors::BUTTON_BG);

                let (cd, _total, can_afford) = ability_status(world, player_entity, ability);
                let ready = cd <= 0.0 && can_afford;
                let tint = if ready {
                    egui::Color32::WHITE
                } else {
                    egui::Color32::from_rgba_unmultiplied(255, 255, 255, 110)
                };
                let (tex, uv) = ability_icon(icons, ability);
                egui::Image::new(egui::load::SizedTexture::new(tex, size))
                    .uv(uv)
                    .tint(tint)
                    .paint_at(ui, rect);

                ui.painter().rect_stroke(
                    rect,
                    0.0,
                    egui::Stroke::new(2.0, style::colors::BUTTON_BORDER),
                );

                // Drag source: carry this ability to a hotbar slot.
                if response.drag_started() {
                    response.dnd_set_drag_payload(HotbarDrag::external(HotbarEntry::Ability(
                        ability,
                    )));
                }

                response.on_hover_text(format!(
                    "{}\n{}\n\nDrag to a hotbar slot",
                    ability.name(),
                    ability.description()
                ));

                ui.add_space(6.0);
            }
        });
    });
}

/// Build inventory display slots, grouping stackable items together
fn build_inventory_slots(items: &[crate::components::ItemInstance]) -> Vec<InventorySlot> {
    use std::collections::HashMap;

    let mut slots = Vec::new();
    let mut stackable_counts: HashMap<crate::components::ItemType, (u32, usize)> = HashMap::new();

    for (i, instance) in items.iter().enumerate() {
        let item_type = instance.kind;
        if item_type.is_stackable() {
            // Track count and first index for stackable items
            stackable_counts
                .entry(item_type)
                .and_modify(|(count, _)| *count += 1)
                .or_insert((1, i));
        } else {
            // Non-stackable items get their own slot
            slots.push(InventorySlot {
                item_type,
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
                if let Some(instance) = inv.items.get(item_idx) {
                    let item = instance.kind;
                    let count = if item.is_stackable() {
                        inv.items.iter().filter(|i| i.kind == item).count() as u32
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

                        // Ammo stacks: select which ammo the bow loads
                        if item_type.is_ammo()
                            && ui.button("Use as ammo").clicked() {
                                actions.set_active_ammo = Some(item_type);
                                ui_state.item_context_menu = None;
                            }

                        // Learnable scrolls: Study (consumes the scroll and
                        // permanently learns its spell if INT allows; trying
                        // with low INT logs the requirement instead).
                        if let Some(required_int) =
                            crate::systems::item_defs::min_learn_int(item_type)
                        {
                            let current_int =
                                crate::queries::effective_stats(world, player_entity).intelligence;
                            let label = if current_int >= required_int {
                                "Study".to_string()
                            } else {
                                format!("Study (needs {} INT)", required_int)
                            };
                            if ui.button(label).clicked() {
                                actions.item_to_study = Some(item_idx);
                                ui_state.item_context_menu = None;
                            }
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
                                let is_equippable = matches!(
                                    crate::systems::item_defs::get_def(item_type).use_effect,
                                    crate::systems::item_defs::UseEffect::Equip
                                );
                                let button_text = if is_equippable { "Equip" } else { "Use" };
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
