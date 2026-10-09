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

use std::time::Instant;

use super::icons::UiIcons;
use super::style;
use super::UiActions;
use crate::constants::*;
use crate::ease;
use crate::components::{
    AbilityType, Actor, ClassAbility, ClassKit, Inventory, ItemType, LearnedAbilities,
    SecondaryAbility,
};
use crate::systems;
use crate::tile::tile_ids;
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

impl Bar {
    /// Row index into [`HotbarAnim`]'s per-slot tables.
    fn index(self) -> usize {
        match self {
            Bar::Main => 0,
            Bar::Shift => 1,
            Bar::Qe => 2,
        }
    }
}

/// Address of a specific hotbar slot.
type SlotAddr = (Bar, usize);

/// Number of hotbars, and the largest slot count any of them has.
const BAR_COUNT: usize = 3;
const MAX_SLOTS: usize = 5;

/// Per-slot presentation state for the hotbars.
///
/// Real-time paced like the rest of the HUD (see the note in
/// `constants::animation`): a ready flash that froze because the game clock
/// stopped would be telling the player nothing at the moment they most need
/// to know the ability is back.
pub struct HotbarAnim {
    /// Cooldown remaining seen last frame, to catch the moment it hits zero.
    last_cooldown: [[f32; MAX_SLOTS]; BAR_COUNT],
    /// When each slot last came off cooldown.
    ready_at: [[Option<Instant>; MAX_SLOTS]; BAR_COUNT],
    /// When the player last pressed a slot they could not use.
    denied_at: [[Option<Instant>; MAX_SLOTS]; BAR_COUNT],
}

impl Default for HotbarAnim {
    fn default() -> Self {
        Self::new()
    }
}

/// Decay a flash started `at` over `duration`: 1.0 the instant it fires,
/// falling to 0.0, eased so it drops away sharply rather than fading linearly.
fn flash_decay(at: Option<Instant>, duration: f32) -> f32 {
    let Some(at) = at else {
        return 0.0;
    };
    let t = at.elapsed().as_secs_f32() / duration;
    if t >= 1.0 {
        0.0
    } else {
        1.0 - ease::out_cubic(t)
    }
}

impl HotbarAnim {
    pub fn new() -> Self {
        Self {
            last_cooldown: [[0.0; MAX_SLOTS]; BAR_COUNT],
            ready_at: [[None; MAX_SLOTS]; BAR_COUNT],
            denied_at: [[None; MAX_SLOTS]; BAR_COUNT],
        }
    }

    /// Record this frame's cooldown for a slot, and return how far through its
    /// "just came off cooldown" flash it is (1.0 the moment it fires, 0.0 once
    /// the flash is spent). Call once per slot per frame.
    fn note_cooldown(&mut self, (bar, i): SlotAddr, cooldown: f32) -> f32 {
        let (bar, i) = (bar.index(), i.min(MAX_SLOTS - 1));
        if cooldown > 0.0 {
            // Back on cooldown: whatever the flash was saying is no longer
            // true, so it stops rather than finishing over a dimmed slot.
            self.ready_at[bar][i] = None;
        } else if self.last_cooldown[bar][i] > 0.0 {
            self.ready_at[bar][i] = Some(Instant::now());
        }
        self.last_cooldown[bar][i] = cooldown;
        flash_decay(self.ready_at[bar][i], HOTBAR_READY_FLASH_DURATION)
    }

    /// Note that the player asked for a slot they cannot currently use.
    fn note_denied(&mut self, (bar, i): SlotAddr) {
        self.denied_at[bar.index()][i.min(MAX_SLOTS - 1)] = Some(Instant::now());
    }

    /// How far through its red "can't use that" flash a slot is.
    fn denied_flash(&self, (bar, i): SlotAddr) -> f32 {
        flash_decay(
            self.denied_at[bar.index()][i.min(MAX_SLOTS - 1)],
            HOTBAR_DENIED_FLASH_DURATION,
        )
    }
}

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
        AbilityType::Tame => (icons.items_texture_id, icons.tame_uv),
        AbilityType::Barkskin => (icons.items_texture_id, icons.barkskin_uv),
        AbilityType::LifeDrain => (icons.items_texture_id, icons.life_drain_uv),
        AbilityType::Fear => (icons.tiles_texture_id, icons.fear_uv),
        AbilityType::Disengage => (icons.items_texture_id, icons.disengage_uv),
        AbilityType::Tumble => (icons.items_texture_id, icons.tumble_uv),
        AbilityType::SnareTrap => (icons.tiles_texture_id, icons.snare_trap_uv),
        AbilityType::CripplingShot => (icons.items_texture_id, icons.crippling_shot_uv),
        AbilityType::Stun => (icons.items_texture_id, icons.stun_uv),
        AbilityType::Rest => (icons.items_texture_id, icons.rest_uv),
        AbilityType::Sleep => (icons.items_texture_id, icons.sleep_uv),
        // Learned spells keep the scroll icon they were studied from.
        AbilityType::LearnedBlink
        | AbilityType::LearnedFireball
        | AbilityType::LearnedFear
        | AbilityType::LearnedSlow
        | AbilityType::LearnedProtection
        | AbilityType::LearnedSpeed
        | AbilityType::LearnedInvisibility => (icons.items_texture_id, icons.scroll_uv),
        AbilityType::RaiseDead => (icons.items_texture_id, icons.raise_dead_uv),
        AbilityType::Guard => (icons.texture_for_sheet(tile_ids::GUARD.0), icons.guard_uv),
        AbilityType::BoneWard => (icons.texture_for_sheet(tile_ids::BONE_WARD.0), icons.bone_ward_uv),
        AbilityType::Sacrifice => (icons.texture_for_sheet(tile_ids::SACRIFICE.0), icons.sacrifice_uv),
        AbilityType::CorpseExplosion => (
            icons.texture_for_sheet(tile_ids::CORPSE_EXPLOSION.0),
            icons.corpse_explosion_uv,
        ),
        AbilityType::Thorns => (icons.texture_for_sheet(tile_ids::THORNS.0), icons.thorns_uv),
        AbilityType::Entangle => (icons.texture_for_sheet(tile_ids::ENTANGLE.0), icons.entangle_uv),
    }
}

/// Tint multiplied into an ability's icon. Stock sprites reused for a kit
/// ability are tinted so they read as that ability rather than as the terrain
/// or monster they were drawn for (CLAUDE.md: tint before custom art).
pub fn ability_icon_tint(ability: AbilityType) -> egui::Color32 {
    match ability {
        AbilityType::BoneWard => egui::Color32::from_rgb(200, 220, 255),
        // Same sickly green as a raised skeleton's SpriteTint.
        AbilityType::Sacrifice => egui::Color32::from_rgb(166, 255, 191),
        AbilityType::CorpseExplosion => egui::Color32::from_rgb(255, 140, 90),
        AbilityType::Thorns => egui::Color32::from_rgb(150, 230, 100),
        AbilityType::Entangle => egui::Color32::from_rgb(110, 220, 90),
        _ => egui::Color32::WHITE,
    }
}

/// Multiply two tints channel by channel (egui tints multiply the texture, so
/// stacking a dim-when-unusable tint on an ability tint is a product).
pub fn mul_tint(a: egui::Color32, b: egui::Color32) -> egui::Color32 {
    let m = |x: u8, y: u8| ((x as u16 * y as u16) / 255) as u8;
    egui::Color32::from_rgba_unmultiplied(
        m(a.r(), b.r()),
        m(a.g(), b.g()),
        m(a.b(), b.b()),
        m(a.a(), b.a()),
    )
}

/// Look up an ability's status for the player: (cooldown_remaining, cooldown_total, usable).
///
/// `usable` is always true for an ability the player actually has: abilities are
/// gated by their cooldown alone, with no spendable resource to fall short of.
/// The flag is kept so callers that grey out a slot keep working.
pub fn ability_status(world: &World, player: Entity, ability: AbilityType) -> (f32, f32, bool) {
    let can_afford = world.get::<&Actor>(player).is_ok();

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
    if let Ok(kit) = world.get::<&ClassKit>(player) {
        if let Some(k) = kit.abilities.iter().find(|k| k.ability == ability) {
            return (k.cooldown_remaining, k.cooldown_total, can_afford);
        }
    }
    if let Ok(la) = world.get::<&LearnedAbilities>(player) {
        if let Some(spell) = la.get(ability) {
            return (spell.cooldown_remaining, spell.cooldown_total, can_afford);
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
    anim: &mut HotbarAnim,
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

    style::dungeon_window(
        ctx,
        icons,
        "Hotbars",
        |window| {
            window
                .fixed_pos([pos_x, pos_y])
                .title_bar(false)
                .resizable(false)
        },
        |ui| {
            ui.horizontal(|ui| {
                draw_bar(
                    ui, world, player, icons, &qer[..], Bar::Qe,
                    &[egui::Key::Q, egui::Key::E, egui::Key::R], false, shift_held,
                    anim, actions, &mut pending_drop, &mut pending_clear,
                );
                ui.add_space(GROUP_GAP);
                draw_bar(
                    ui, world, player, icons, &main[..], Bar::Main, &nums, false, shift_held,
                    anim, actions, &mut pending_drop, &mut pending_clear,
                );
                ui.add_space(GROUP_GAP);
                draw_bar(
                    ui, world, player, icons, &shift[..], Bar::Shift, &nums, true, shift_held,
                    anim, actions, &mut pending_drop, &mut pending_clear,
                );
            });
        },
    );

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
    anim: &mut HotbarAnim,
    actions: &mut UiActions,
    pending_drop: &mut Option<(SlotAddr, HotbarDrag)>,
    pending_clear: &mut Option<SlotAddr>,
) {
    let inventory = world.get::<&Inventory>(player).ok();
    let count_of = |item: ItemType| -> u32 {
        inventory
            .as_ref()
            .map(|inv| inv.items.iter().filter(|t| t.kind == item).count() as u32)
            .unwrap_or(0)
    };
    let first_index_of = |item: ItemType| -> Option<usize> {
        inventory
            .as_ref()
            .and_then(|inv| inv.items.iter().position(|t| t.kind == item))
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
        // `ready_pop` rides the moment an ability comes off cooldown.
        let mut usable = false;
        let mut ready_pop = 0.0;
        match entry {
            Some(HotbarEntry::Item(item)) => {
                let count = count_of(item);
                usable = count > 0;
                paint_icon(ui, rect, icons.items_texture_id, icons.get_item_uv(item), slot_tint(usable), 1.0);
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
                let (cd, total, can_afford) = ability_status(world, player, ab);
                usable = cd <= 0.0 && can_afford;
                ready_pop = anim.note_cooldown(addr, cd);
                let (tex, uv) = ability_icon(icons, ab);
                // The icon swells for an instant as the ability returns.
                let pop = 1.0 + (HOTBAR_READY_POP_SCALE - 1.0) * ready_pop;
                let tint = mul_tint(slot_tint(usable), ability_icon_tint(ab));
                paint_icon(ui, rect, tex, uv, tint, pop);
                if cd > 0.0 {
                    // A wipe rather than a uniform dim: the sweep uncovers the
                    // icon from the bottom as the cooldown runs down, so the
                    // slot shows how far along it is without being read.
                    let remaining = if total > 0.0 {
                        (cd / total).clamp(0.0, 1.0)
                    } else {
                        1.0
                    };
                    let wipe = egui::Rect::from_min_size(
                        rect.left_top(),
                        egui::vec2(rect.width(), rect.height() * remaining),
                    );
                    ui.painter().rect_filled(
                        wipe,
                        0.0,
                        egui::Color32::from_rgba_unmultiplied(
                            style::colors::HOTBAR_COOLDOWN_SWEEP.r(),
                            style::colors::HOTBAR_COOLDOWN_SWEEP.g(),
                            style::colors::HOTBAR_COOLDOWN_SWEEP.b(),
                            HOTBAR_COOLDOWN_SWEEP_ALPHA,
                        ),
                    );
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

        // White flash the instant an ability comes back, and a red one when
        // the player asks for something they cannot have. Both sit over the
        // icon and the cooldown wipe, under the border.
        if ready_pop > 0.0 {
            ui.painter().rect_filled(
                rect,
                0.0,
                tinted(
                    style::colors::HOTBAR_READY_FLASH,
                    (HOTBAR_READY_FLASH_ALPHA as f32 * ready_pop) as u8,
                ),
            );
        }
        let denied = anim.denied_flash(addr);
        if denied > 0.0 {
            ui.painter().rect_filled(
                rect,
                0.0,
                tinted(
                    style::colors::HOTBAR_DENIED_FLASH,
                    (HOTBAR_DENIED_FLASH_ALPHA as f32 * denied) as u8,
                ),
            );
        }

        // Ready cue: a glow just inside the border, so a usable slot reads as
        // usable at a glance rather than only on a border-colour comparison.
        if usable {
            ui.painter().rect_stroke(
                rect.shrink(HOTBAR_READY_GLOW_WIDTH / 2.0 + 1.0),
                0.0,
                egui::Stroke::new(
                    HOTBAR_READY_GLOW_WIDTH,
                    tinted(style::colors::DUNGEON_GOLD, HOTBAR_READY_GLOW_ALPHA),
                ),
            );
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
        let pressed = response.clicked() || key_fired;
        if pressed && !usable && entry.is_some() {
            // Previously silence. The slot now says no.
            anim.note_denied(addr);
        }
        if pressed && usable {
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

/// `color` at `alpha`, for the translucent overlays.
fn tinted(color: egui::Color32, alpha: u8) -> egui::Color32 {
    egui::Color32::from_rgba_unmultiplied(color.r(), color.g(), color.b(), alpha)
}

/// Paint a slot's icon, optionally scaled about the slot centre. The painter
/// is clipped to the slot so a popped icon grows into its own square rather
/// than over its neighbours.
fn paint_icon(
    ui: &egui::Ui,
    rect: egui::Rect,
    tex: egui::TextureId,
    uv: egui::Rect,
    tint: egui::Color32,
    scale: f32,
) {
    let target = if scale == 1.0 {
        rect
    } else {
        egui::Rect::from_center_size(rect.center(), rect.size() * scale)
    };
    let clipped = ui.painter().with_clip_rect(rect);
    clipped.image(tex, target, uv, tint);
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
            let (tex, uv, tint) = match drag.entry {
                HotbarEntry::Item(item) => {
                    (icons.items_texture_id, icons.get_item_uv(item), egui::Color32::WHITE)
                }
                HotbarEntry::Ability(ab) => {
                    let (tex, uv) = ability_icon(icons, ab);
                    (tex, uv, ability_icon_tint(ab))
                }
            };
            let rect = egui::Rect::from_center_size(pos, egui::vec2(40.0, 40.0));
            let painter = ctx.layer_painter(egui::LayerId::new(
                egui::Order::Foreground,
                egui::Id::new("hotbar_drag_ghost"),
            ));
            let ghost = egui::Color32::from_rgba_unmultiplied(255, 255, 255, 200);
            painter.image(tex, rect, uv, mul_tint(ghost, tint));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_flash_starts_full_and_decays() {
        // Nothing to decay.
        assert_eq!(flash_decay(None, 1.0), 0.0);
        // Just fired.
        assert!(flash_decay(Some(Instant::now()), 1.0) > 0.99);
        // Older than its duration, so spent. (A zero-length flash never
        // appears: every caller passes a positive constant.)
        let fired = Instant::now();
        std::thread::sleep(std::time::Duration::from_millis(5));
        assert_eq!(flash_decay(Some(fired), 0.001), 0.0);
    }

    #[test]
    fn a_slot_flashes_the_frame_its_cooldown_runs_out() {
        let mut anim = HotbarAnim::new();
        let slot = (Bar::Main, 0);

        // Still cooling: nothing to say.
        assert_eq!(anim.note_cooldown(slot, 3.0), 0.0);
        assert_eq!(anim.note_cooldown(slot, 1.0), 0.0);

        // The frame it reaches zero is the flash.
        assert!(anim.note_cooldown(slot, 0.0) > 0.9);
        // Sitting at zero does not re-arm it, but the flash does carry on
        // across the frames it is alive for.
        assert!(anim.note_cooldown(slot, 0.0) > 0.0);

        // Used again: the flash is cancelled rather than finishing over a
        // slot that is no longer ready.
        assert_eq!(anim.note_cooldown(slot, 2.0), 0.0);
    }

    #[test]
    fn slots_flash_independently() {
        let mut anim = HotbarAnim::new();
        let (a, b) = ((Bar::Main, 0), (Bar::Qe, 2));
        anim.note_cooldown(a, 1.0);
        anim.note_cooldown(b, 1.0);
        assert!(anim.note_cooldown(a, 0.0) > 0.9);
        // b is still cooling and must not have borrowed a's flash.
        assert_eq!(anim.note_cooldown(b, 0.5), 0.0);
    }

    #[test]
    fn a_denied_press_flashes_only_its_own_slot() {
        let mut anim = HotbarAnim::new();
        let pressed = (Bar::Shift, 3);
        assert_eq!(anim.denied_flash(pressed), 0.0);
        anim.note_denied(pressed);
        assert!(anim.denied_flash(pressed) > 0.9);
        assert_eq!(anim.denied_flash((Bar::Shift, 4)), 0.0);
        assert_eq!(anim.denied_flash((Bar::Main, 3)), 0.0);
    }
}
