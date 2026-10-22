//! Inventory and character window UI component.
//!
//! Left: a paper doll — the character with an equipment socket for each slot
//! around them — then attributes, load and gold. Right: tabs for the pack (a
//! grid of sockets) and the spellbook (a list of abilities to drag onto the
//! hotbar). Items and equipment have tooltips and right-click menus.

use super::icons::UiIcons;
use super::style::{self, colors};
use super::{ability_icon, ability_status, CharacterTab, GameUiState, HotbarDrag, HotbarEntry, UiActions};
use crate::components::{
    AbilityType, ClassKit, Equipment, EquippedWeapon, Inventory, ItemInstance, ItemType, Rarity,
    SecondaryAbility, Stats,
};
use crate::constants::*;
use crate::multi_tileset::MultiTileset;
use crate::systems;
use crate::systems::item_defs::ArmorSlot;
use crate::world::World;
use egui::{Color32, Pos2, Rect, RichText};

/// Data needed to render the inventory window
pub struct InventoryWindowData {
    pub viewport_width: f32,
    pub viewport_height: f32,
}

// =============================================================================
// SOCKETS
// =============================================================================
// An item socket is a recess in the panel with an item icon in it. Shared with
// the start screen's class details, so an item looks the same wherever it is
// shown.

/// Paint `item` in a socket at `rect`: the recess (warmer while `hovered`),
/// the icon at 1:1 in its middle, and its badge if it has one.
pub(super) fn paint_item_socket(
    painter: &egui::Painter,
    icons: &UiIcons,
    rect: Rect,
    item: ItemType,
    hovered: bool,
) {
    paint_socket(painter, rect, hovered);
    let icon = socket_icon_rect(painter, rect);
    let tint = UiIcons::item_ui_tint(item);
    painter.image(icons.items_texture_id, icon, icons.get_item_uv(item), tint);
    icons.paint_item_badge(painter, icon, item, Color32::WHITE);
}

/// An empty socket's recess, warmer while `hovered`.
fn paint_socket(painter: &egui::Painter, rect: Rect, hovered: bool) {
    let fill = if hovered { colors::SOCKET_HOVER_FILL } else { colors::SOCKET_BG };
    style::paint_recess(painter, rect, fill);
}

/// Where the icon goes in a socket: [`CHARACTER_SOCKET_ICON_SIZE`] square,
/// centred, on whole pixels so the sprite stays crisp.
fn socket_icon_rect(painter: &egui::Painter, socket: Rect) -> Rect {
    let size = egui::vec2(CHARACTER_SOCKET_ICON_SIZE, CHARACTER_SOCKET_ICON_SIZE);
    painter.round_rect_to_pixels(Rect::from_center_size(socket.center(), size))
}

/// A stack's count in the bottom-right corner of its socket, outlined so it
/// reads over any icon.
pub(super) fn paint_stack_count(painter: &egui::Painter, rect: Rect, count: u32) {
    let text = count.to_string();
    let pos = rect.right_bottom() + egui::vec2(-3.0, -2.0);
    let font = egui::FontId::proportional(12.0);
    for offset in [egui::vec2(-1.0, 0.0), egui::vec2(1.0, 0.0), egui::vec2(0.0, -1.0), egui::vec2(0.0, 1.0)] {
        painter.text(pos + offset, egui::Align2::RIGHT_BOTTOM, &text, font.clone(), Color32::BLACK);
    }
    painter.text(pos, egui::Align2::RIGHT_BOTTOM, &text, font, colors::TEXT_PRIMARY);
}

/// Frame a socket in its item's rarity colour, so Magic, Rare and Legendary
/// gear stands out in the grid. Common items keep the plain recess.
fn paint_rarity_frame(painter: &egui::Painter, rect: Rect, rarity: Rarity) {
    if rarity == Rarity::Common {
        return;
    }
    let color = style::rarity_color(rarity);
    let rect = painter.round_rect_to_pixels(rect);
    painter.rect_stroke(rect, 0.0, egui::Stroke::new(style::BORDER_WIDTH, color));
    painter.rect_stroke(rect.shrink(1.0), 0.0, egui::Stroke::new(style::BORDER_WIDTH, color.gamma_multiply(0.4)));
}

/// The brass edge a socket gets while the pointer is on it.
fn paint_hover_frame(painter: &egui::Painter, rect: Rect) {
    let rect = painter.round_rect_to_pixels(rect);
    painter.rect_stroke(rect, 0.0, egui::Stroke::new(style::BORDER_WIDTH, colors::TEXT_ACCENT));
}

// =============================================================================
// TOOLTIPS
// =============================================================================

/// Rarity + affix block for a gear tooltip.
///
/// Unidentified items show their rarity color and an "Unidentified" tag but
/// hide their affix lines behind "???" (Legendary names are hidden through
/// `ItemInstance::display_name`). Identified curse lines render red.
fn affix_block_ui(ui: &mut egui::Ui, inst: &ItemInstance) {
    if inst.rarity == Rarity::Common && inst.affixes.is_empty() {
        return;
    }
    ui.add_space(4.0);
    ui.label(RichText::new(inst.rarity.label()).color(style::rarity_color(inst.rarity)));
    if !inst.identified {
        ui.label(RichText::new("Unidentified").italics().color(colors::TEXT_MUTED));
        for _ in &inst.affixes {
            ui.label(RichText::new("???").color(colors::TEXT_MUTED));
        }
    } else {
        for affix in &inst.affixes {
            let color = if affix.is_curse() { colors::CURSE } else { colors::TEXT_PRIMARY };
            ui.label(RichText::new(affix.describe()).color(color));
        }
    }
}

/// Muted footer hint lines for a tooltip ("Click to unequip", ...).
fn tooltip_hint_ui(ui: &mut egui::Ui, hint: &str) {
    ui.add_space(4.0);
    ui.label(RichText::new(hint).small().color(colors::TEXT_MUTED));
}

/// A tooltip's title line: the item's name, in its rarity colour.
fn tooltip_title_ui(ui: &mut egui::Ui, title: &str, rarity: Rarity) {
    ui.label(RichText::new(title).strong().color(style::rarity_color(rarity)));
}

// =============================================================================
// WINDOW
// =============================================================================

/// Render the inventory/character window
#[allow(clippy::too_many_arguments)]
pub fn draw_inventory_window(
    ctx: &egui::Context,
    world: &World,
    player_entity: hecs::Entity,
    data: &InventoryWindowData,
    icons: &UiIcons,
    tileset: &MultiTileset,
    ui_state: &mut GameUiState,
    actions: &mut UiActions,
) {
    let grid_width = grid_width();
    let width = CHARACTER_LEFT_WIDTH + 2.0 * PANEL_INNER_MARGIN_GAP + grid_width;
    style::dungeon_window(
        ctx,
        icons,
        "Character",
        |window| {
            window
                .default_pos([
                    data.viewport_width / 2.0 - width / 2.0,
                    data.viewport_height / 2.0 - 230.0,
                ])
                .collapsible(false)
                .resizable(false)
        },
        |ui| {
            let Ok(stats) = world.get::<&Stats>(player_entity) else {
                return;
            };
            let columns = ui.horizontal_top(|ui| {
                let left = ui.vertical(|ui| {
                    ui.set_width(CHARACTER_LEFT_WIDTH);
                    draw_paper_doll(ui, world, player_entity, icons, tileset, ui_state, actions);
                    ui.add_space(6.0);
                    draw_attributes(ui, world, player_entity, &stats, icons);
                });
                // Room for the divider, drawn once both columns' height is
                // known.
                ui.add_space(PANEL_INNER_MARGIN_GAP * 2.0);
                let right = ui.vertical(|ui| {
                    ui.set_width(grid_width);
                    draw_tabs(ui, world, player_entity, ui_state);
                    ui.add_space(8.0);
                    match ui_state.character_tab {
                        CharacterTab::Inventory => {
                            draw_inventory_grid(ui, world, player_entity, icons, ui_state, actions)
                        }
                        CharacterTab::Spellbook => draw_spellbook(ui, world, player_entity, icons),
                    }
                });
                (left.response.rect, right.response.rect)
            });
            // Midway between the two columns, the full height of the taller.
            let (left, right) = columns.inner;
            let painter = ui.painter();
            let x = painter.round_to_pixel_center((left.right() + right.left()) / 2.0);
            let stroke = egui::Stroke::new(style::BORDER_WIDTH, colors::PANEL_RULE);
            painter.vline(x, columns.response.rect.y_range(), stroke);
        },
    );

    // Draw context menu popup (outside the main window)
    draw_item_context_menu(ctx, world, player_entity, ui_state, actions);

    // Draw equipped item context menu popup
    draw_equipped_context_menu(ctx, world, player_entity, ui_state, actions);
}

/// Space either side of the divider between the window's two columns.
const PANEL_INNER_MARGIN_GAP: f32 = 10.0;

/// Width of the inventory grid, which sets the width of the right column.
fn grid_width() -> f32 {
    let columns = CHARACTER_GRID_COLUMNS as f32;
    columns * CHARACTER_SOCKET_SIZE + (columns - 1.0) * CHARACTER_GRID_GAP
}

// =============================================================================
// PAPER DOLL
// =============================================================================

/// What an equipment socket holds, for drawing and interaction.
enum Worn<'a> {
    Weapon(&'a EquippedWeapon, Option<&'a ItemInstance>),
    Piece(ArmorSlot, &'a ItemInstance),
    Empty,
}

/// The character with an equipment socket for each slot around them:
///
/// ```text
///   .      Head     Amulet
///   Weapon (you)    Off-hand
///   Ring   Body     .
/// ```
fn draw_paper_doll(
    ui: &mut egui::Ui,
    world: &World,
    player_entity: hecs::Entity,
    icons: &UiIcons,
    tileset: &MultiTileset,
    ui_state: &mut GameUiState,
    actions: &mut UiActions,
) {
    let (area, _) = ui.allocate_exact_size(
        egui::vec2(CHARACTER_LEFT_WIDTH, CHARACTER_DOLL_HEIGHT),
        egui::Sense::hover(),
    );
    let painter = ui.painter();
    // A darker alcove for the character to stand in, lit from within.
    style::paint_recess(painter, area, colors::SOCKET_BG);
    style::paint_glow(
        painter,
        area.center(),
        CHARACTER_DOLL_HEIGHT * 0.45,
        colors::TORCH_GLOW,
        MENU_CLASS_GLOW_STRENGTH,
    );
    if let Ok(sprite) = world.get::<&crate::components::Sprite>(player_entity) {
        let size = egui::vec2(CHARACTER_DOLL_SPRITE_SIZE, CHARACTER_DOLL_SPRITE_SIZE);
        let tileset_uv = tileset.get_egui_uv(sprite.sheet, sprite.tile_id);
        painter.image(
            icons.texture_for_sheet(sprite.sheet),
            painter.round_rect_to_pixels(Rect::from_center_size(area.center(), size)),
            tileset_uv,
            Color32::WHITE,
        );
    }

    let Ok(equipment) = world.get::<&Equipment>(player_entity) else {
        return;
    };

    // Socket centres on a 3x3 grid across the alcove, the middle taken by
    // the character.
    let inset = CHARACTER_SOCKET_SIZE / 2.0 + 10.0;
    let xs = [area.left() + inset, area.center().x, area.right() - inset];
    let ys = [area.top() + inset, area.center().y, area.bottom() - inset];
    let weapon = match &equipment.weapon {
        Some(w) => Worn::Weapon(w, equipment.weapon_source.as_ref()),
        None => Worn::Empty,
    };
    fn piece(slot: ArmorSlot, item: &Option<ItemInstance>) -> Worn<'_> {
        match item {
            Some(instance) => Worn::Piece(slot, instance),
            None => Worn::Empty,
        }
    }
    let sockets = [
        ("Weapon", icons.sword_uv, Pos2::new(xs[0], ys[1]), weapon),
        ("Head", icons.helmet_uv, Pos2::new(xs[1], ys[0]), piece(ArmorSlot::Head, &equipment.head)),
        ("Amulet", icons.amulet_uv, Pos2::new(xs[2], ys[0]), piece(ArmorSlot::Amulet, &equipment.amulet)),
        ("Off-hand", icons.torch_uv, Pos2::new(xs[2], ys[1]), piece(ArmorSlot::OffHand, &equipment.off_hand)),
        ("Ring", icons.ring_uv, Pos2::new(xs[0], ys[2]), piece(ArmorSlot::Ring, &equipment.ring)),
        ("Body", icons.leather_armor_uv, Pos2::new(xs[1], ys[2]), piece(ArmorSlot::Body, &equipment.body)),
    ];

    for (name, silhouette, centre, worn) in sockets {
        let rect = Rect::from_center_size(
            centre,
            egui::vec2(CHARACTER_SOCKET_SIZE, CHARACTER_SOCKET_SIZE),
        );
        let response = ui.interact(rect, ui.id().with(("equipment_socket", name)), egui::Sense::click());
        equipment_socket(ui, icons, rect, response, name, silhouette, worn, ui_state, actions);
    }
}

/// Draw one equipment socket and handle its clicks: left-click unequips, and
/// the weapon has a right-click menu. An empty socket shows a faint
/// silhouette of what goes there.
#[allow(clippy::too_many_arguments)]
fn equipment_socket(
    ui: &egui::Ui,
    icons: &UiIcons,
    rect: Rect,
    response: egui::Response,
    name: &str,
    silhouette: Rect,
    worn: Worn,
    ui_state: &mut GameUiState,
    actions: &mut UiActions,
) {
    let painter = ui.painter();
    let hovered = response.hovered();
    let response = response.on_hover_cursor(match worn {
        Worn::Empty => egui::CursorIcon::Default,
        _ => egui::CursorIcon::PointingHand,
    });

    match worn {
        Worn::Empty => {
            paint_socket(painter, rect, hovered);
            painter.image(
                icons.items_texture_id,
                socket_icon_rect(painter, rect),
                silhouette,
                colors::SOCKET_SILHOUETTE.gamma_multiply(CHARACTER_SILHOUETTE_ALPHA),
            );
            response.on_hover_ui(|ui| {
                ui.label(RichText::new(name).strong());
                ui.label(RichText::new("Empty").italics().color(colors::TEXT_MUTED));
            });
        }
        Worn::Weapon(weapon, source) => {
            let item = match weapon {
                EquippedWeapon::Melee(w) => w.item.unwrap_or(ItemType::Sword),
                EquippedWeapon::Ranged(_) => ItemType::Bow,
            };
            paint_item_socket(painter, icons, rect, item, hovered);
            if let Some(inst) = source {
                paint_rarity_frame(painter, rect, inst.rarity);
            }
            if hovered {
                paint_hover_frame(painter, rect);
            }
            // Unidentified sources hide their (Legendary) name
            let title = source.map(|inst| inst.display_name()).unwrap_or_else(|| match weapon {
                EquippedWeapon::Melee(w) => w.name.clone(),
                EquippedWeapon::Ranged(b) => b.name.clone(),
            });
            let rarity = source.map(|inst| inst.rarity).unwrap_or(Rarity::Common);
            let response = response.on_hover_ui(|ui| {
                tooltip_title_ui(ui, &title, rarity);
                match weapon {
                    EquippedWeapon::Melee(w) => ui.label(format!(
                        "Damage: {} + {} = {}",
                        w.base_damage,
                        w.damage_bonus,
                        systems::weapon_damage(w),
                    )),
                    EquippedWeapon::Ranged(b) => ui.label(format!(
                        "Damage: {}\nSpeed: {:.0} tiles/sec",
                        b.base_damage, b.arrow_speed,
                    )),
                };
                if let Some(inst) = source {
                    affix_block_ui(ui, inst);
                }
                tooltip_hint_ui(ui, "Click to unequip\nRight-click for options");
            });
            if response.clicked() {
                actions.unequip_weapon = true;
            }
            if response.secondary_clicked() {
                ui_state.equipped_context_menu = Some(response.rect.right_top());
            }
        }
        Worn::Piece(slot, instance) => {
            paint_item_socket(painter, icons, rect, instance.kind, hovered);
            paint_rarity_frame(painter, rect, instance.rarity);
            if hovered {
                paint_hover_frame(painter, rect);
            }
            let response = response.on_hover_ui(|ui| {
                use crate::systems::item_defs::{armor_base_defense, is_accessory_kind};
                tooltip_title_ui(ui, &instance.display_name(), instance.rarity);
                // Only armor has a defense line: accessories are pure affix
                // carriers and torches are torches. Unidentified armor only
                // shows its known base defense.
                if instance.kind == ItemType::Torch {
                    ui.label("Lights your way, and you.\nCtrl+direction: set grass or oil alight.");
                } else if instance.kind == ItemType::UnlitTorch {
                    ui.label("Doused.\nCtrl+direction at a fire to relight it.");
                } else if !is_accessory_kind(instance.kind) {
                    let bonus = if instance.identified { instance.defense_bonus() } else { 0 };
                    ui.label(format!("Defense: {}", armor_base_defense(instance.kind) + bonus));
                }
                affix_block_ui(ui, instance);
                tooltip_hint_ui(ui, "Click to unequip");
            });
            if response.clicked() {
                actions.unequip_armor = Some(slot);
            }
        }
    }
}

// =============================================================================
// ATTRIBUTES, LOAD AND GOLD
// =============================================================================

fn draw_attributes(
    ui: &mut egui::Ui,
    world: &World,
    player_entity: hecs::Entity,
    stats: &Stats,
    icons: &UiIcons,
) {
    // Defense and damage, the two numbers the doll's gear adds up to.
    if let Ok(equipment) = world.get::<&Equipment>(player_entity) {
        let damage = match &equipment.weapon {
            Some(EquippedWeapon::Melee(w)) => systems::weapon_damage(w).to_string(),
            Some(EquippedWeapon::Ranged(b)) => b.base_damage.to_string(),
            None => "—".to_string(),
        };
        ui.horizontal(|ui| {
            readout(ui, "DEFENSE", &equipment.total_defense().to_string());
            ui.add_space(16.0);
            readout(ui, "DAMAGE", &damage);
        });
    }

    ui.add_space(4.0);
    style::panel_rule(ui);
    ui.add_space(2.0);

    // Effective stats include stat affixes on equipped gear; show the gear
    // contribution beside the value, green for a bonus and red for a curse.
    let effective = crate::queries::effective_stats(world, player_entity);
    egui::Grid::new("character_attributes")
        .num_columns(3)
        .spacing(egui::vec2(10.0, 3.0))
        .show(ui, |ui| {
            for (name, base, eff) in [
                ("Strength", stats.strength, effective.strength),
                ("Intelligence", stats.intelligence, effective.intelligence),
                ("Agility", stats.agility, effective.agility),
            ] {
                ui.label(RichText::new(name).color(colors::TEXT_MUTED));
                ui.label(RichText::new(eff.to_string()).strong().color(colors::TEXT_PRIMARY));
                let diff = eff - base;
                if diff != 0 {
                    let color = if diff > 0 { colors::STAT_BONUS } else { colors::CURSE };
                    ui.label(RichText::new(format!("{:+}", diff)).small().color(color))
                        .on_hover_text(format!("{} base, {:+} from gear", base, diff));
                } else {
                    ui.label("");
                }
                ui.end_row();
            }
        });

    ui.add_space(4.0);
    style::panel_rule(ui);
    ui.add_space(4.0);

    let Ok(inventory) = world.get::<&Inventory>(player_entity) else {
        return;
    };
    let capacity = effective.strength as f32 * CARRY_CAPACITY_PER_STRENGTH;
    weight_bar(ui, inventory.current_weight_kg, capacity);
    ui.add_space(6.0);
    ui.horizontal(|ui| {
        let size = egui::vec2(16.0, 16.0);
        let (rect, _) = ui.allocate_exact_size(size, egui::Sense::hover());
        ui.painter().image(icons.items_texture_id, rect, icons.coins_uv, Color32::WHITE);
        ui.label(RichText::new(inventory.gold.to_string()).strong().color(colors::DUNGEON_GOLD));
        ui.label(RichText::new("gold").color(colors::TEXT_MUTED));
    });
}

/// A small caption over a number: "DEFENSE / 4".
fn readout(ui: &mut egui::Ui, caption: &str, value: &str) {
    ui.label(RichText::new(caption).size(11.0).color(colors::TEXT_MUTED));
    ui.label(RichText::new(value).strong().color(colors::TEXT_PRIMARY));
}

/// Carried weight against capacity: a caption with the figures, then a bar in
/// the same recess the HUD bars use. Turns red past capacity.
fn weight_bar(ui: &mut egui::Ui, weight: f32, capacity: f32) {
    let over = weight > capacity;
    ui.horizontal(|ui| {
        ui.label(RichText::new("LOAD").size(11.0).color(colors::TEXT_MUTED));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let color = if over { colors::HUNGER_CRITICAL } else { colors::TEXT_PRIMARY };
            ui.label(RichText::new(format!("{:.1} / {:.1} kg", weight, capacity)).size(12.0).color(color));
        });
    });
    let (rect, _) = ui.allocate_exact_size(
        egui::vec2(ui.available_width(), CHARACTER_WEIGHT_BAR_HEIGHT),
        egui::Sense::hover(),
    );
    let painter = ui.painter();
    style::paint_recess(painter, rect, colors::WEIGHT_BAR_BG);
    let fraction = if capacity > 0.0 { (weight / capacity).clamp(0.0, 1.0) } else { 1.0 };
    if fraction > 0.0 {
        let inner = rect.shrink(2.0);
        let fill = Rect::from_min_size(inner.min, egui::vec2(inner.width() * fraction, inner.height()));
        let color = if over { colors::HP_BAR } else { colors::WEIGHT_BAR };
        painter.rect_filled(painter.round_rect_to_pixels(fill), 0.0, color);
        let top = Rect::from_min_size(fill.min, egui::vec2(fill.width(), 1.0));
        painter.rect_filled(painter.round_rect_to_pixels(top), 0.0, style::brighten(color, 1.3));
    }
}

// =============================================================================
// TABS
// =============================================================================

/// The Inventory / Spellbook tabs, with a count on each. The active tab is lit
/// and open to the rule beneath it, so it reads as the front of the two.
fn draw_tabs(ui: &mut egui::Ui, world: &World, player_entity: hecs::Entity, ui_state: &mut GameUiState) {
    let item_count = world
        .get::<&Inventory>(player_entity)
        .map(|inv| systems::stack_items(&inv.items).len())
        .unwrap_or(0);
    let spell_count = player_abilities(world, player_entity).len();
    let tabs = [
        (CharacterTab::Inventory, format!("Inventory  {}", item_count)),
        (CharacterTab::Spellbook, format!("Spellbook  {}", spell_count)),
    ];

    let (row, _) = ui.allocate_exact_size(
        egui::vec2(ui.available_width(), CHARACTER_TAB_HEIGHT),
        egui::Sense::hover(),
    );
    let tab_width = row.width() / tabs.len() as f32;
    let mut active_span = None;
    for (i, (tab, label)) in tabs.iter().enumerate() {
        let rect = Rect::from_min_size(
            row.min + egui::vec2(i as f32 * tab_width, 0.0),
            egui::vec2(tab_width, row.height()),
        );
        let rect = ui.painter().round_rect_to_pixels(rect.shrink2(egui::vec2(1.0, 0.0)));
        let response = ui
            .interact(rect, ui.id().with(("character_tab", i)), egui::Sense::click())
            .on_hover_cursor(egui::CursorIcon::PointingHand);
        if response.clicked() {
            ui_state.character_tab = *tab;
        }
        let active = ui_state.character_tab == *tab;
        let painter = ui.painter();
        if active {
            active_span = Some(rect.x_range());
            painter.rect_filled(rect, 0.0, colors::BUTTON_HOVER);
            // Brass along the top edge, the lit lip of the front tab.
            let lip = Rect::from_min_size(rect.min, egui::vec2(rect.width(), 2.0));
            painter.rect_filled(lip, 0.0, colors::TEXT_ACCENT);
        } else {
            let fill = if response.hovered() { colors::BUTTON_BG } else { colors::SOCKET_BG };
            painter.rect_filled(rect, 0.0, fill);
            painter.rect_stroke(rect, 0.0, egui::Stroke::new(style::BORDER_WIDTH, colors::PANEL_RULE));
        }
        let text_color = if active {
            colors::DUNGEON_GOLD
        } else if response.hovered() {
            colors::TEXT_PRIMARY
        } else {
            colors::TEXT_MUTED
        };
        painter.text(
            rect.center(),
            egui::Align2::CENTER_CENTER,
            label.to_uppercase(),
            egui::FontId::proportional(13.0),
            text_color,
        );
    }

    // The rule under the tabs, broken beneath the active one.
    let painter = ui.painter();
    let y = painter.round_to_pixel_center(row.bottom());
    let stroke = egui::Stroke::new(style::BORDER_WIDTH, colors::TEXT_ACCENT.gamma_multiply(0.5));
    match active_span {
        Some(span) => {
            painter.hline(row.left()..=span.min, y, stroke);
            painter.hline(span.max..=row.right(), y, stroke);
        }
        None => {
            painter.hline(row.x_range(), y, stroke);
        }
    }
}

// =============================================================================
// INVENTORY GRID
// =============================================================================

/// Represents an inventory slot for display (may be a stack or single item)
struct InventorySlot {
    item_type: ItemType,
    count: u32,
    first_index: usize, // Index of first occurrence in inventory
}

/// The pack as a grid of sockets, at least [`CHARACTER_GRID_MIN_ROWS`] deep so
/// there is always visible room. Left-click uses, right-click opens the item's
/// menu, dragging carries it to the hotbar.
fn draw_inventory_grid(
    ui: &mut egui::Ui,
    world: &World,
    player_entity: hecs::Entity,
    icons: &UiIcons,
    ui_state: &mut GameUiState,
    actions: &mut UiActions,
) {
    // Which ammo the bow currently loads (for the tooltip marker)
    let active_ammo = world
        .get::<&crate::components::ActiveAmmo>(player_entity)
        .map(|a| a.kind)
        .unwrap_or(ItemType::Arrow);
    let Ok(inventory) = world.get::<&Inventory>(player_entity) else {
        return;
    };
    let slots = build_inventory_slots(&inventory.items);
    let rows = slots.len().div_ceil(CHARACTER_GRID_COLUMNS).max(CHARACTER_GRID_MIN_ROWS);
    let step = CHARACTER_SOCKET_SIZE + CHARACTER_GRID_GAP;
    let visible_height = CHARACTER_GRID_MIN_ROWS as f32 * step;

    egui::ScrollArea::vertical()
        .max_height(visible_height)
        .drag_to_scroll(false)
        .show(ui, |ui| {
            let (grid, _) = ui.allocate_exact_size(
                egui::vec2(grid_width(), rows as f32 * step - CHARACTER_GRID_GAP),
                egui::Sense::hover(),
            );
            for cell in 0..rows * CHARACTER_GRID_COLUMNS {
                let (row, column) = (cell / CHARACTER_GRID_COLUMNS, cell % CHARACTER_GRID_COLUMNS);
                let rect = Rect::from_min_size(
                    grid.min + egui::vec2(column as f32 * step, row as f32 * step),
                    egui::vec2(CHARACTER_SOCKET_SIZE, CHARACTER_SOCKET_SIZE),
                );
                match slots.get(cell) {
                    Some(slot) => inventory_cell(
                        ui, icons, rect, slot, &inventory.items, active_ammo, ui_state, actions,
                    ),
                    None => paint_socket(ui.painter(), rect, false),
                }
            }
        });

    ui.add_space(6.0);
    let hint = if slots.is_empty() {
        "Your pack is empty."
    } else {
        "Click to use  •  Right-click for options  •  Drag to hotbar"
    };
    ui.label(RichText::new(hint).size(11.0).color(colors::TEXT_MUTED));
}

/// One filled cell of the pack grid: its socket, and what clicking, dragging
/// and hovering it does.
#[allow(clippy::too_many_arguments)]
fn inventory_cell(
    ui: &mut egui::Ui,
    icons: &UiIcons,
    rect: Rect,
    slot: &InventorySlot,
    items: &[ItemInstance],
    active_ammo: ItemType,
    ui_state: &mut GameUiState,
    actions: &mut UiActions,
) {
    // click_and_drag so the slot can both be used and dragged to the hotbar
    let response = ui
        .interact(rect, ui.id().with(("inventory_cell", slot.first_index)), egui::Sense::click_and_drag())
        .on_hover_cursor(egui::CursorIcon::PointingHand);
    let painter = ui.painter();
    paint_item_socket(painter, icons, rect, slot.item_type, response.hovered());
    if slot.count > 1 {
        paint_stack_count(painter, rect, slot.count);
    }

    // Rarity + affix block for gear (looked up from the backing instance);
    // identified Legendary items show their name, unidentified gear hides
    // affixes behind "???".
    let backing = items.get(slot.first_index);
    if let Some(inst) = backing {
        paint_rarity_frame(painter, rect, inst.rarity);
    }
    if response.hovered() {
        paint_hover_frame(painter, rect);
    }
    let display_name = backing
        .map(|inst| inst.display_name())
        .unwrap_or_else(|| systems::item_name(slot.item_type).to_string());
    let rarity = backing.map(|inst| inst.rarity).unwrap_or(Rarity::Common);
    let is_throwable = systems::items::item_is_throwable(slot.item_type);

    // Ammo stacks show whether they're the loaded ammo type
    let ammo_text = slot.item_type.is_ammo().then(|| {
        if slot.item_type == active_ammo {
            "Active ammo"
        } else {
            "Right-click to use as ammo"
        }
    });

    let response = response.on_hover_ui(|ui| {
        let title = if slot.count > 1 {
            format!("{} (x{})", display_name, slot.count)
        } else {
            display_name.clone()
        };
        tooltip_title_ui(ui, &title, rarity);
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
        response.dnd_set_drag_payload(HotbarDrag::external(HotbarEntry::Item(slot.item_type)));
    }

    // Left-click: use/drink the item (only for single items or non-stackables)
    if response.clicked() && slot.count == 1 {
        actions.item_to_use = Some(slot.first_index);
    }

    // Right-click: open context menu (for all items)
    if response.secondary_clicked() {
        ui_state.item_context_menu = Some((slot.first_index, response.rect.right_top()));
    }
}

/// Build inventory display slots, grouping stackable items together
fn build_inventory_slots(items: &[ItemInstance]) -> Vec<InventorySlot> {
    let mut slots: Vec<InventorySlot> = crate::systems::stack_items(items)
        .into_iter()
        .map(|s| InventorySlot {
            item_type: items[s.first_index].kind,
            count: s.count,
            first_index: s.first_index,
        })
        .collect();

    // Stackable items (ammo) sit at the end, after the gear and consumables.
    slots.sort_by_key(|s| (s.item_type.is_stackable(), s.first_index));

    slots
}

// =============================================================================
// SPELLBOOK
// =============================================================================

/// The player's abilities: class, then secondary, then kit, then learned
/// spells (studied scrolls, the Necromancer's Raise Dead), then Rest, which
/// every class has.
fn player_abilities(world: &World, player_entity: hecs::Entity) -> Vec<AbilityType> {
    let mut abilities: Vec<AbilityType> = Vec::new();
    if let Ok(a) = world.get::<&crate::components::ClassAbility>(player_entity) {
        abilities.push(a.ability_type);
    }
    if let Ok(a) = world.get::<&SecondaryAbility>(player_entity) {
        abilities.push(a.ability_type);
    }
    if let Ok(kit) = world.get::<&ClassKit>(player_entity) {
        abilities.extend(kit.abilities.iter().map(|k| k.ability));
    }
    if let Ok(la) = world.get::<&crate::components::LearnedAbilities>(player_entity) {
        abilities.extend(la.spells.iter().map(|spell| spell.ability));
    }
    abilities.push(AbilityType::Rest);
    abilities
}

/// The spellbook: a row per ability with its icon, name and what it does,
/// each a drag source for the hotbar. Abilities on cooldown are dimmed and
/// show the time left.
fn draw_spellbook(ui: &mut egui::Ui, world: &World, player_entity: hecs::Entity, icons: &UiIcons) {
    let step = CHARACTER_SOCKET_SIZE + CHARACTER_GRID_GAP;
    let row_height = CHARACTER_SPELL_ICON_SIZE + 6.0;
    egui::ScrollArea::vertical()
        .max_height(CHARACTER_GRID_MIN_ROWS as f32 * step)
        .drag_to_scroll(false)
        .show(ui, |ui| {
            ui.spacing_mut().item_spacing.y = 2.0;
            for ability in player_abilities(world, player_entity) {
                let (rect, response) = ui.allocate_exact_size(
                    egui::vec2(ui.available_width(), row_height),
                    egui::Sense::click_and_drag(),
                );
                let response = response.on_hover_cursor(egui::CursorIcon::Grab);
                let painter = ui.painter();
                if response.hovered() {
                    painter.rect_filled(rect, 0.0, colors::SOCKET_HOVER_FILL);
                }

                let socket = Rect::from_min_size(rect.min, egui::vec2(row_height, row_height));
                paint_socket(painter, socket, false);
                let (cd, _total, can_afford) = ability_status(world, player_entity, ability);
                let ready = cd <= 0.0 && can_afford;
                let tint = if ready { Color32::WHITE } else { Color32::from_white_alpha(110) };
                let tint = super::hotbar::mul_tint(tint, super::hotbar::ability_icon_tint(ability));
                let (texture, uv) = ability_icon(icons, ability);
                let icon = egui::vec2(CHARACTER_SPELL_ICON_SIZE, CHARACTER_SPELL_ICON_SIZE);
                painter.image(
                    texture,
                    painter.round_rect_to_pixels(Rect::from_center_size(socket.center(), icon)),
                    uv,
                    tint,
                );

                let text_left = socket.right() + 8.0;
                let name_color = if response.hovered() { colors::DUNGEON_GOLD } else { colors::TEXT_PRIMARY };
                painter.text(
                    Pos2::new(text_left, rect.top() + 3.0),
                    egui::Align2::LEFT_TOP,
                    ability.name(),
                    egui::FontId::proportional(14.0),
                    name_color,
                );
                if cd > 0.0 {
                    painter.text(
                        Pos2::new(rect.right() - 4.0, rect.top() + 3.0),
                        egui::Align2::RIGHT_TOP,
                        format!("{:.0}s", cd.ceil()),
                        egui::FontId::proportional(12.0),
                        colors::TEXT_MUTED,
                    );
                }
                // The first line of the description, cut to fit; the
                // tooltip has the rest.
                let summary = one_line(ui, ability.description(), rect.right() - text_left);
                ui.painter().galley(
                    Pos2::new(text_left, rect.bottom() - 3.0 - summary.size().y),
                    summary,
                    colors::TEXT_MUTED,
                );

                // Drag source: carry this ability to a hotbar slot.
                if response.drag_started() {
                    response.dnd_set_drag_payload(HotbarDrag::external(HotbarEntry::Ability(ability)));
                }
                response.on_hover_ui(|ui| {
                    ui.label(RichText::new(ability.name()).strong());
                    ui.label(ability.description());
                    tooltip_hint_ui(ui, "Drag to a hotbar slot");
                });
            }
        });
    ui.add_space(6.0);
    ui.label(RichText::new("Drag a spell onto a hotbar slot").size(11.0).color(colors::TEXT_MUTED));
}

/// Lay `text` out on a single line no wider than `width`, ending in an
/// ellipsis if it had to be cut.
fn one_line(ui: &egui::Ui, text: &str, width: f32) -> std::sync::Arc<egui::Galley> {
    let mut job = egui::text::LayoutJob::single_section(
        text.to_string(),
        egui::TextFormat {
            font_id: egui::FontId::proportional(11.0),
            color: colors::TEXT_MUTED,
            ..Default::default()
        },
    );
    job.wrap = egui::text::TextWrapping::truncate_at_width(width);
    ui.fonts(|f| f.layout_job(job))
}

// =============================================================================
// CONTEXT MENUS
// =============================================================================

/// A full-width entry in a right-click menu.
fn context_item(ui: &mut egui::Ui, label: impl Into<egui::WidgetText>) -> bool {
    ui.add(egui::Button::new(label).min_size(egui::vec2(ui.available_width(), 22.0)))
        .clicked()
}

/// Close a right-click menu on Escape, or on a left click anywhere outside
/// `menu_rect` (the area the menu was actually drawn in, so a tall menu's
/// lower entries are not mistaken for outside).
fn should_close_context_menu(ctx: &egui::Context, menu_rect: Rect) -> bool {
    ctx.input(|i| {
        i.key_pressed(egui::Key::Escape)
            || (i.pointer.primary_clicked()
                && i.pointer.interact_pos().is_some_and(|pos| !menu_rect.contains(pos)))
    })
}

fn draw_item_context_menu(
    ctx: &egui::Context,
    world: &World,
    player_entity: hecs::Entity,
    ui_state: &mut GameUiState,
    actions: &mut UiActions,
) {
    let Some((item_idx, pos)) = ui_state.item_context_menu else {
        return;
    };
    // Get the item type and count to show appropriate options
    let (item_type, stack_count) = world
        .get::<&Inventory>(player_entity)
        .ok()
        .and_then(|inv| {
            let item = inv.items.get(item_idx)?.kind;
            let count = if item.is_stackable() {
                inv.items.iter().filter(|i| i.kind == item).count() as u32
            } else {
                1
            };
            Some((item, count))
        })
        .unzip();
    let (Some(item_type), Some(stack_count)) = (item_type, stack_count) else {
        // Item no longer exists, close menu
        ui_state.item_context_menu = None;
        return;
    };

    let is_throwable = systems::items::item_is_throwable(item_type);
    let is_stackable = item_type.is_stackable();

    let menu = egui::Area::new(egui::Id::new("item_context_menu"))
        .fixed_pos(pos)
        .order(egui::Order::Foreground)
        .show(ctx, |ui| {
            style::dungeon_window_frame().show(ui, |ui| {
                ui.set_min_width(CONTEXT_MENU_MIN_WIDTH);

                // The item's name heads the menu, with its count for stacks
                let name = systems::item_name(item_type);
                let title = if stack_count > 1 {
                    format!("{} (x{})", name, stack_count)
                } else {
                    name.to_string()
                };
                ui.label(RichText::new(title).strong().color(colors::TEXT_PRIMARY));
                style::panel_rule(ui);

                // Ammo stacks: select which ammo the bow loads
                if item_type.is_ammo() && context_item(ui, "Use as ammo") {
                    actions.set_active_ammo = Some(item_type);
                    ui_state.item_context_menu = None;
                }

                // Learnable scrolls: Study (consumes the scroll and
                // permanently learns its spell if INT allows; trying with low
                // INT logs the requirement instead).
                if let Some(required_int) = crate::systems::item_defs::min_learn_int(item_type) {
                    let current_int = crate::queries::effective_stats(world, player_entity).intelligence;
                    let label = if current_int >= required_int {
                        RichText::new("Study")
                    } else {
                        RichText::new(format!("Study (needs {} INT)", required_int)).color(colors::TEXT_MUTED)
                    };
                    if context_item(ui, label) {
                        actions.item_to_study = Some(item_idx);
                        ui_state.item_context_menu = None;
                    }
                }

                // Show options based on item type (not for stackable ammo)
                if !is_stackable {
                    if is_throwable {
                        if context_item(ui, "Drink") {
                            actions.item_to_use = Some(item_idx);
                            ui_state.item_context_menu = None;
                        }
                        if context_item(ui, "Throw") {
                            actions.item_to_throw = Some(item_idx);
                            ui_state.item_context_menu = None;
                        }
                    } else {
                        // Non-throwable items: Use/Equip
                        let is_equippable = matches!(
                            crate::systems::item_defs::get_def(item_type).use_effect,
                            crate::systems::item_defs::UseEffect::Equip
                        );
                        if context_item(ui, if is_equippable { "Equip" } else { "Use" }) {
                            actions.item_to_use = Some(item_idx);
                            ui_state.item_context_menu = None;
                        }
                    }
                }

                // Drop option - shows "Drop" for single items, "Drop One" for stacks
                if context_item(ui, if stack_count > 1 { "Drop One" } else { "Drop" }) {
                    actions.item_to_drop = Some(item_idx);
                    ui_state.item_context_menu = None;
                }

                style::panel_rule(ui);
                if context_item(ui, RichText::new("Cancel").color(colors::TEXT_MUTED)) {
                    ui_state.item_context_menu = None;
                }
            });
        });

    if should_close_context_menu(ctx, menu.response.rect) {
        ui_state.item_context_menu = None;
    }
}

fn draw_equipped_context_menu(
    ctx: &egui::Context,
    world: &World,
    player_entity: hecs::Entity,
    ui_state: &mut GameUiState,
    actions: &mut UiActions,
) {
    let Some(pos) = ui_state.equipped_context_menu else {
        return;
    };
    // Check if player still has a weapon equipped
    let has_weapon = world
        .get::<&Equipment>(player_entity)
        .map(|eq| eq.weapon.is_some())
        .unwrap_or(false);
    if !has_weapon {
        // Weapon no longer equipped, close menu
        ui_state.equipped_context_menu = None;
        return;
    }

    let menu = egui::Area::new(egui::Id::new("equipped_context_menu"))
        .fixed_pos(pos)
        .order(egui::Order::Foreground)
        .show(ctx, |ui| {
            style::dungeon_window_frame().show(ui, |ui| {
                ui.set_min_width(CONTEXT_MENU_MIN_WIDTH);
                if context_item(ui, "Unequip") {
                    actions.unequip_weapon = true;
                    ui_state.equipped_context_menu = None;
                }
                if context_item(ui, "Drop") {
                    actions.drop_equipped_weapon = true;
                    ui_state.equipped_context_menu = None;
                }
                style::panel_rule(ui);
                if context_item(ui, RichText::new("Cancel").color(colors::TEXT_MUTED)) {
                    ui_state.equipped_context_menu = None;
                }
            });
        });

    if should_close_context_menu(ctx, menu.response.rect) {
        ui_state.equipped_context_menu = None;
    }
}
