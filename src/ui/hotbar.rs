//! Quick-use hotbars.
//!
//! Three drag-assignable bars along the bottom-center of the screen. Each slot
//! can hold either an inventory item or an ability (a [`HotbarEntry`]):
//! - Main:  5 slots, keys `1`-`5`
//! - Shift: 5 slots, keys `Shift+1`-`Shift+5`
//! - Q/E/R: 3 slots, keys `Q` / `E` / `R`
//!
//! Items are dragged in from the inventory, abilities from the Spellbook tab.
//! Dragging from one hotbar slot onto another swaps them. Slots are bound by
//! value, not by index, so they survive inventory churn. Activation reuses the
//! existing item-use and ability-activation paths via `UiActions`.

use super::icons::UiIcons;
use super::style;
use super::UiActions;
use crate::components::{
    AbilityType, Actor, ClassAbility, Inventory, ItemType, RangerAbilities, SecondaryAbility,
};
use crate::systems;
use hecs::{Entity, World};

/// An entry placed in a hotbar slot: an inventory item or an ability.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum HotbarEntry {
    Item(ItemType),
    Ability(AbilityType),
}

/// Which hotbar a slot belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Bar {
    Main,
    Shift,
    Qe,
}

/// Address of a specific hotbar slot.
type SlotAddr = (Bar, usize);

/// The egui drag-and-drop payload for hotbar entries. Carries the source slot
/// (when dragged from a hotbar) so a drop onto another slot can swap them.
#[derive(Debug, Clone, Copy)]
pub struct HotbarDrag {
    pub entry: HotbarEntry,
    source: Option<SlotAddr>,
}

impl HotbarDrag {
    /// A drag originating outside the hotbars (inventory item or spellbook ability).
    pub fn external(entry: HotbarEntry) -> Self {
        Self { entry, source: None }
    }
}

const SLOT_SIZE: f32 = 48.0;
const SLOT_SPACING: f32 = 6.0;
const GROUP_GAP: f32 = 18.0;
const BOTTOM_MARGIN: f32 = 20.0;

/// Map an ability to its icon (texture id + uv).
pub fn ability_icon(icons: &UiIcons, ability: AbilityType) -> (egui::TextureId, egui::Rect) {
    match ability {
        AbilityType::Cleave => (icons.items_texture_id, icons.cleave_uv),
        AbilityType::Sprint => (icons.items_texture_id, icons.sprint_uv),
        AbilityType::Tame => (icons.items_texture_id, icons.heart_uv),
        AbilityType::Barkskin => (icons.items_texture_id, icons.barkskin_uv),
        AbilityType::LifeDrain => (icons.items_texture_id, icons.life_drain_uv),
        AbilityType::Fear => (icons.tiles_texture_id, icons.fear_uv),
        AbilityType::Disengage => (icons.items_texture_id, icons.disengage_uv),
        AbilityType::Tumble => (icons.items_texture_id, icons.tumble_uv),
        AbilityType::SnareTrap => (icons.tiles_texture_id, icons.snare_trap_uv),
        AbilityType::CripplingShot => (icons.items_texture_id, icons.crippling_shot_uv),
        AbilityType::Stun => (icons.items_texture_id, icons.diamond_uv),
        AbilityType::Rest => (icons.items_texture_id, icons.heart_uv),
    }
}

/// Look up an ability's status for the player: (cooldown_remaining, cooldown_total, can_afford).
pub fn ability_status(world: &World, player: Entity, ability: AbilityType) -> (f32, f32, bool) {
    let can_afford = world
        .get::<&Actor>(player)
        .map(|a| a.max_energy >= ability.energy_cost())
        .unwrap_or(false);

    if let Ok(a) = world.get::<&ClassAbility>(player) {
        if a.ability_type == ability {
            return (a.cooldown_remaining, a.cooldown_total, can_afford);
        }
    }
    if let Ok(a) = world.get::<&SecondaryAbility>(player) {
        if a.ability_type == ability {
            return (a.cooldown_remaining, a.cooldown_total, can_afford);
        }
    }
    if let Ok(ra) = world.get::<&RangerAbilities>(player) {
        if let Some((_, cd, total)) = ra.abilities.iter().find(|(at, _, _)| *at == ability) {
            return (*cd, *total, can_afford);
        }
    }
    (0.0, 0.0, can_afford)
}

/// Draw the three hotbars in one bottom-center window (a single row so they all
/// share the same height).
#[allow(clippy::too_many_arguments)]
pub fn draw_hotbars(
    ctx: &egui::Context,
    world: &World,
    player: Entity,
    icons: &UiIcons,
    main: &mut [Option<HotbarEntry>; 5],
    shift: &mut [Option<HotbarEntry>; 5],
    qer: &mut [Option<HotbarEntry>; 3],
    actions: &mut UiActions,
) {
    let shift_held = ctx.input(|i| i.modifiers.shift);

    let group_w = |n: usize| n as f32 * SLOT_SIZE + (n as f32 - 1.0) * SLOT_SPACING;
    let total = group_w(5) + GROUP_GAP + group_w(5) + GROUP_GAP + group_w(3);
    let screen = ctx.screen_rect();
    let pos_x = (screen.width() - total) / 2.0;
    let pos_y = screen.height() - SLOT_SIZE - BOTTOM_MARGIN - 8.0;

    let nums = [
        egui::Key::Num1,
        egui::Key::Num2,
        egui::Key::Num3,
        egui::Key::Num4,
        egui::Key::Num5,
    ];

    // Drop / clear are recorded during rendering and applied afterwards, so we
    // never need two mutable slot borrows at once (needed for swapping).
    let mut pending_drop: Option<(SlotAddr, HotbarDrag)> = None;
    let mut pending_clear: Option<SlotAddr> = None;

    egui::Window::new("Hotbars")
        .fixed_pos([pos_x, pos_y])
        .title_bar(false)
        .resizable(false)
        .frame(style::dungeon_window_frame())
        .show(ctx, |ui| {
            ui.horizontal(|ui| {
                draw_bar(
                    ui, world, player, icons, &qer[..], Bar::Qe,
                    &[egui::Key::Q, egui::Key::E, egui::Key::R], false, shift_held,
                    actions, &mut pending_drop, &mut pending_clear,
                );
                ui.add_space(GROUP_GAP);
                draw_bar(
                    ui, world, player, icons, &main[..], Bar::Main, &nums, false, shift_held,
                    actions, &mut pending_drop, &mut pending_clear,
                );
                ui.add_space(GROUP_GAP);
                draw_bar(
                    ui, world, player, icons, &shift[..], Bar::Shift, &nums, true, shift_held,
                    actions, &mut pending_drop, &mut pending_clear,
                );
            });
        });

    // Apply deferred mutations.
    if let Some((tgt, drag)) = pending_drop {
        match drag.source {
            Some(src) if src != tgt => {
                let src_entry = slot_ref(main, shift, qer, src);
                let tgt_entry = slot_ref(main, shift, qer, tgt);
                *slot_mut(main, shift, qer, tgt) = src_entry;
                *slot_mut(main, shift, qer, src) = tgt_entry;
            }
            Some(_) => {} // dropped onto itself
            None => *slot_mut(main, shift, qer, tgt) = Some(drag.entry),
        }
    }
    if let Some(addr) = pending_clear {
        *slot_mut(main, shift, qer, addr) = None;
    }
}

fn slot_ref(
    main: &[Option<HotbarEntry>; 5],
    shift: &[Option<HotbarEntry>; 5],
    qer: &[Option<HotbarEntry>; 3],
    (bar, i): SlotAddr,
) -> Option<HotbarEntry> {
    match bar {
        Bar::Main => main[i],
        Bar::Shift => shift[i],
        Bar::Qe => qer[i],
    }
}

fn slot_mut<'a>(
    main: &'a mut [Option<HotbarEntry>; 5],
    shift: &'a mut [Option<HotbarEntry>; 5],
    qer: &'a mut [Option<HotbarEntry>; 3],
    (bar, i): SlotAddr,
) -> &'a mut Option<HotbarEntry> {
    match bar {
        Bar::Main => &mut main[i],
        Bar::Shift => &mut shift[i],
        Bar::Qe => &mut qer[i],
    }
}

/// Label shown in a slot's corner (and tooltip) for a given bar/index.
fn slot_label(bar: Bar, i: usize) -> String {
    match bar {
        Bar::Main => format!("{}", i + 1),
        Bar::Shift => format!("S{}", i + 1),
        Bar::Qe => ["Q", "E", "R"].get(i).copied().unwrap_or("?").to_string(),
    }
}

/// Draw one bar's slots directly into `ui` (no nested layout, so all bars line up).
#[allow(clippy::too_many_arguments)]
fn draw_bar(
    ui: &mut egui::Ui,
    world: &World,
    player: Entity,
    icons: &UiIcons,
    slots: &[Option<HotbarEntry>],
    bar: Bar,
    keys: &[egui::Key],
    require_shift: bool,
    shift_held: bool,
    actions: &mut UiActions,
    pending_drop: &mut Option<(SlotAddr, HotbarDrag)>,
    pending_clear: &mut Option<SlotAddr>,
) {
    let inventory = world.get::<&Inventory>(player).ok();
    let count_of = |item: ItemType| -> u32 {
        inventory
            .as_ref()
            .map(|inv| inv.items.iter().filter(|&&t| t == item).count() as u32)
            .unwrap_or(0)
    };
    let first_index_of = |item: ItemType| -> Option<usize> {
        inventory
            .as_ref()
            .and_then(|inv| inv.items.iter().position(|&t| t == item))
    };

    for i in 0..slots.len() {
        let entry = slots[i];
        let addr = (bar, i);
        let label = slot_label(bar, i);

        let (rect, response) =
            ui.allocate_exact_size(egui::vec2(SLOT_SIZE, SLOT_SIZE), egui::Sense::click_and_drag());

        // Drag source: pick this entry up (carrying its source for swapping).
        if response.drag_started() {
            if let Some(e) = entry {
                response.dnd_set_drag_payload(HotbarDrag { entry: e, source: Some(addr) });
            }
        }
        // Accept a dropped entry.
        if let Some(drag) = response.dnd_release_payload::<HotbarDrag>() {
            *pending_drop = Some((addr, *drag));
        }
        let hovering_drop = response.dnd_hover_payload::<HotbarDrag>().is_some();

        // Slot background.
        let bg = if response.hovered() {
            style::colors::BUTTON_HOVER
        } else {
            style::colors::BUTTON_BG
        };
        ui.painter().rect_filled(rect, 0.0, bg);

        // Draw the entry and determine whether it can be activated right now.
        let mut usable = false;
        match entry {
            Some(HotbarEntry::Item(item)) => {
                let count = count_of(item);
                usable = count > 0;
                paint_icon(ui, rect, icons.items_texture_id, icons.get_item_uv(item), slot_tint(usable));
                if count > 1 {
                    draw_count_badge(ui, rect, count);
                }
                response.clone().on_hover_text(format!(
                    "{}\n\n[{}] to use • drag to move • right-click to clear",
                    systems::item_name(item),
                    label
                ));
            }
            Some(HotbarEntry::Ability(ab)) => {
                let (cd, _total, can_afford) = ability_status(world, player, ab);
                usable = cd <= 0.0 && can_afford;
                let (tex, uv) = ability_icon(icons, ab);
                paint_icon(ui, rect, tex, uv, slot_tint(usable));
                if cd > 0.0 {
                    ui.painter()
                        .rect_filled(rect, 0.0, egui::Color32::from_rgba_unmultiplied(0, 0, 0, 180));
                    ui.painter().text(
                        rect.center(),
                        egui::Align2::CENTER_CENTER,
                        format!("{:.0}s", cd),
                        egui::FontId::proportional(14.0),
                        egui::Color32::WHITE,
                    );
                }
                response.clone().on_hover_text(format!(
                    "{}\n{}\n\n[{}] to use • drag to move • right-click to clear",
                    ab.name(),
                    ab.description(),
                    label
                ));
            }
            None => {}
        }

        // Border: gold while a drag hovers or when the slot is ready to use.
        let border = if hovering_drop || usable {
            style::colors::DUNGEON_GOLD
        } else {
            style::colors::BUTTON_BORDER
        };
        ui.painter()
            .rect_stroke(rect, 0.0, egui::Stroke::new(2.0, border));

        // Key label in the corner.
        ui.painter().text(
            rect.left_top() + egui::vec2(3.0, 2.0),
            egui::Align2::LEFT_TOP,
            &label,
            egui::FontId::proportional(11.0),
            style::colors::TEXT_MUTED,
        );

        // Activation by click or by this slot's key (respecting the Shift modifier).
        let key_fired = i < keys.len()
            && shift_held == require_shift
            && ui.input(|inp| inp.key_pressed(keys[i]));
        if (response.clicked() || key_fired) && usable {
            match entry {
                Some(HotbarEntry::Item(item)) => {
                    if let Some(idx) = first_index_of(item) {
                        actions.item_to_use = Some(idx);
                    }
                }
                Some(HotbarEntry::Ability(ab)) => actions.ability_to_use = Some(ab),
                None => {}
            }
        }

        // Right-click clears the binding.
        if response.secondary_clicked() {
            *pending_clear = Some(addr);
        }

        if i < slots.len() - 1 {
            ui.add_space(SLOT_SPACING);
        }
    }
}

fn slot_tint(usable: bool) -> egui::Color32 {
    if usable {
        egui::Color32::WHITE
    } else {
        egui::Color32::from_rgba_unmultiplied(255, 255, 255, 80)
    }
}

fn paint_icon(ui: &egui::Ui, rect: egui::Rect, tex: egui::TextureId, uv: egui::Rect, tint: egui::Color32) {
    egui::Image::new(egui::load::SizedTexture::new(tex, egui::vec2(SLOT_SIZE, SLOT_SIZE)))
        .uv(uv)
        .tint(tint)
        .paint_at(ui, rect);
}

fn draw_count_badge(ui: &egui::Ui, rect: egui::Rect, count: u32) {
    let text = format!("{}", count);
    let p = rect.right_bottom() + egui::vec2(-4.0, -4.0);
    ui.painter().text(
        p + egui::vec2(1.0, 1.0),
        egui::Align2::RIGHT_BOTTOM,
        &text,
        egui::FontId::proportional(14.0),
        egui::Color32::BLACK,
    );
    ui.painter().text(
        p,
        egui::Align2::RIGHT_BOTTOM,
        &text,
        egui::FontId::proportional(14.0),
        egui::Color32::WHITE,
    );
}

/// Paint the dragged entry's icon under the cursor while a drag is in progress.
/// Call once per frame (after the rest of the UI) so it draws on top.
pub fn draw_drag_ghost(ctx: &egui::Context, icons: &UiIcons) {
    if let Some(drag) = egui::DragAndDrop::payload::<HotbarDrag>(ctx) {
        if let Some(pos) = ctx.pointer_interact_pos() {
            let (tex, uv) = match drag.entry {
                HotbarEntry::Item(item) => (icons.items_texture_id, icons.get_item_uv(item)),
                HotbarEntry::Ability(ab) => ability_icon(icons, ab),
            };
            let rect = egui::Rect::from_center_size(pos, egui::vec2(40.0, 40.0));
            let painter = ctx.layer_painter(egui::LayerId::new(
                egui::Order::Foreground,
                egui::Id::new("hotbar_drag_ghost"),
            ));
            painter.image(tex, rect, uv, egui::Color32::from_rgba_unmultiplied(255, 255, 255, 200));
        }
    }
}
