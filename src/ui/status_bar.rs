//! Status bar UI component.
//!
//! Displays player health, XP, gold, and active status effects.

use std::time::Instant;

use super::icons::UiIcons;
use super::style::{self, colors};
use crate::components::{
    EffectType as StatusEffectType, Fatigue, FatigueState, Health, Hunger, HungerState,
    Inventory, Position, StatusEffects,
};
use crate::constants::*;
use crate::ease;
use crate::grid::Grid;
use crate::systems;
use crate::tile::TileType;
use hecs::World;

/// Format elapsed game time (seconds) as HH:MM:SS, starting from 00:00:00.
pub fn format_game_clock(seconds: f32) -> String {
    let total = seconds.max(0.0) as u64;
    let hours = total / 3600;
    let minutes = (total % 3600) / 60;
    let secs = total % 60;
    format!("{:02}:{:02}:{:02}", hours, minutes, secs)
}

/// Data needed to render the status bar
pub struct StatusBarData {
    pub health_current: i32,
    pub health_max: i32,
    pub xp_progress: f32,
    pub xp_level: u32,
    pub gold: u32,
    /// Total flat defense from equipped armor
    pub defense: i32,
    /// Whether the player is currently concealed (standing in tall grass)
    pub is_concealed: bool,
    /// Whether the player is currently sneaking (crouch toggle)
    pub is_sneaking: bool,
    /// Hunger meter value (0 = starving, HUNGER_MAX = fully fed)
    pub hunger: f32,
    /// Coarse hunger state for the warning label
    pub hunger_state: HungerState,
    /// Fatigue meter value (0 = rested, FATIGUE_MAX = exhausted)
    pub fatigue: f32,
    /// Coarse fatigue state for the warning label
    pub fatigue_state: FatigueState,
    /// Active status effects with remaining duration
    pub active_effects: Vec<(StatusEffectType, f32)>,
}

/// Extract status bar data from the world
pub fn get_status_bar_data(world: &World, player_entity: hecs::Entity, grid: &Grid) -> StatusBarData {
    let (health_current, health_max) = world
        .get::<&Health>(player_entity)
        .map(|h| (h.current, h.max))
        .unwrap_or((0, 0));

    let gold = world
        .get::<&Inventory>(player_entity)
        .map(|inv| inv.gold)
        .unwrap_or(0);

    let defense = world
        .get::<&crate::components::Equipment>(player_entity)
        .map(|e| e.total_defense())
        .unwrap_or(0);

    // Concealed is a derived/positional state: true while standing in tall grass.
    let is_concealed = world
        .get::<&Position>(player_entity)
        .ok()
        .and_then(|p| grid.get(p.x, p.y).map(|t| t.tile_type == TileType::TallGrass))
        .unwrap_or(false);

    // Sneaking is a derived state from the crouch toggle (Sneaking marker).
    let is_sneaking = world.get::<&crate::components::Sneaking>(player_entity).is_ok();

    // Survival meters (player-only; default to "fine" if missing).
    let (hunger, hunger_state) = world
        .get::<&Hunger>(player_entity)
        .map(|h| (h.value, h.state()))
        .unwrap_or((HUNGER_MAX, HungerState::Fed));
    let (fatigue, fatigue_state) = world
        .get::<&Fatigue>(player_entity)
        .map(|f| (f.value, f.state()))
        .unwrap_or((0.0, FatigueState::Rested));

    let (xp_progress, xp_level) = world
        .get::<&crate::components::Experience>(player_entity)
        .map(|exp| (systems::xp_progress(&exp), exp.level))
        .unwrap_or((0.0, 1));

    // Collect active status effects
    let active_effects = world
        .get::<&StatusEffects>(player_entity)
        .map(|effects| {
            effects
                .effects
                .iter()
                .map(|e| (e.effect_type, e.remaining_duration))
                .collect()
        })
        .unwrap_or_default();

    StatusBarData {
        health_current,
        health_max,
        xp_progress,
        xp_level,
        gold,
        defense,
        is_concealed,
        is_sneaking,
        hunger,
        hunger_state,
        fatigue,
        fatigue_state,
        active_effects,
    }
}

/// Presentation-only animation state the status bar carries between frames.
///
/// Paced by real time rather than the game clock, for the same reason camera
/// shake is (see the HUD note in `constants::animation`): the game clock stops
/// to wait for input, which is exactly when the player is reading the HUD, and
/// a chip bar frozen half-drained is worse than no chip bar at all.
pub struct StatusBarAnim {
    /// HP the chip ("ghost") bar is currently showing.
    chip_hp: f32,
    /// HP the current chip drain started from, so stacked hits chain smoothly
    /// instead of each one restarting from the new, lower HP.
    chip_from: f32,
    /// Player HP as of the previous frame, to notice a drop.
    last_hp: i32,
    /// When the current chip drain was triggered.
    damaged_at: Instant,
    /// Fixed origin for looping pulses, so their phase stays continuous.
    born: Instant,
}

impl Default for StatusBarAnim {
    fn default() -> Self {
        Self::new()
    }
}

impl StatusBarAnim {
    pub fn new() -> Self {
        let now = Instant::now();
        Self {
            chip_hp: 0.0,
            chip_from: 0.0,
            last_hp: 0,
            // Far enough back that the first frame finds no drain in progress.
            damaged_at: now,
            born: now,
        }
    }

    /// Advance the chip bar for this frame and return the HP it should show.
    ///
    /// A drop arms a fresh drain from wherever the ghost currently sits; a gain
    /// snaps it, because a ghost lagging *above* the real bar would read as
    /// damage the player never took.
    fn advance_chip(&mut self, current: i32) -> f32 {
        let hp = current as f32;
        if current > self.last_hp {
            self.chip_hp = hp;
            self.chip_from = hp;
        } else if current < self.last_hp {
            self.chip_from = self.chip_hp.max(hp);
            self.damaged_at = Instant::now();
        }
        self.last_hp = current;

        let elapsed = self.damaged_at.elapsed().as_secs_f32();
        let t = ((elapsed - HP_CHIP_HOLD) / HP_CHIP_DRAIN_DURATION).clamp(0.0, 1.0);
        self.chip_hp = self.chip_from + (hp - self.chip_from) * ease::out_cubic(t);
        self.chip_hp
    }

    /// A 0..1 pulse at `rate` full cycles per second, for a brightness wobble.
    fn pulse(&self, rate: f32) -> f32 {
        ease::ping_pong(self.born.elapsed().as_secs_f32() * rate)
    }
}

/// What sits in a bar row's icon gutter.
enum BarIcon {
    /// A sprite from one of the sheets.
    Sprite(egui::TextureId, egui::Rect),
    /// A text glyph, for state with no sprite to its name (CLAUDE.md's
    /// last-resort option for transient state).
    Glyph(&'static str, egui::Color32),
}

/// Everything needed to hand-paint one resource bar.
struct BarSpec {
    /// Fill fraction, clamped to 0.0..=1.0 when painted.
    fill: f32,
    fill_color: egui::Color32,
    /// The dark inset the fill sits in.
    recess_color: egui::Color32,
    /// Ghost fill drawn behind the real one, as a fraction. HP only.
    chip: Option<f32>,
    /// Number of equal segments to score the bar into, so it can be counted.
    /// 0 or 1 means no notches.
    segments: i32,
    /// Brightness multiplier on the fill; 1.0 for none.
    glow: f32,
    label: String,
}

impl BarSpec {
    /// A plain bar: no chip, no notches, no glow.
    fn new(fill: f32, fill_color: egui::Color32, recess_color: egui::Color32, label: String) -> Self {
        Self {
            fill,
            fill_color,
            recess_color,
            chip: None,
            segments: 0,
            glow: 1.0,
            label,
        }
    }
}

/// Hand-paint one resource bar into `rect`.
///
/// Replaces `egui::ProgressBar`, which gave a flat rounded fill with no recess,
/// no leading edge and no way to put a second (chip) fill behind the first.
fn paint_bar(painter: &egui::Painter, rect: egui::Rect, spec: &BarSpec) {
    // The recess the fill sits in, tinted toward the fill so an empty bar still
    // says which resource it belongs to.
    painter.rect_filled(rect, 0.0, spec.recess_color);

    // 1px inset bevel: dark along the top and left, light along the bottom and
    // right. This is what makes the recess read as sunk into the panel rather
    // than painted onto it.
    let b = HUD_BAR_BEVEL;
    let strip = |min: egui::Pos2, size: egui::Vec2, color| {
        painter.rect_filled(egui::Rect::from_min_size(min, size), 0.0, color);
    };
    strip(rect.left_top(), egui::vec2(rect.width(), b), colors::BAR_BEVEL_DARK);
    strip(rect.left_top(), egui::vec2(b, rect.height()), colors::BAR_BEVEL_DARK);
    strip(
        rect.left_bottom() - egui::vec2(0.0, b),
        egui::vec2(rect.width(), b),
        colors::BAR_BEVEL_LIGHT,
    );
    strip(
        rect.right_top() - egui::vec2(b, 0.0),
        egui::vec2(b, rect.height()),
        colors::BAR_BEVEL_LIGHT,
    );

    // Fills live inside the bevel so they never paint over the frame.
    let inner = rect.shrink(b);

    // Chip ghost first, so the real fill draws over it and only the lost
    // chunk between the two is left showing.
    if let Some(chip) = spec.chip {
        let chip_w = inner.width() * chip.clamp(0.0, 1.0);
        if chip_w > 0.0 {
            strip(inner.left_top(), egui::vec2(chip_w, inner.height()), colors::HP_CHIP);
        }
    }

    let fill_w = inner.width() * spec.fill.clamp(0.0, 1.0);
    if fill_w > 0.0 {
        let fill = style::brighten(spec.fill_color, spec.glow);
        strip(inner.left_top(), egui::vec2(fill_w, inner.height()), fill);

        // Brighter leading edge at the fill boundary, kept inside the fill so a
        // full bar still shows one.
        let edge_w = HUD_BAR_LEADING_EDGE_WIDTH.min(fill_w);
        strip(
            egui::pos2(inner.left() + fill_w - edge_w, inner.top()),
            egui::vec2(edge_w, inner.height()),
            style::brighten(fill, HUD_BAR_LEADING_EDGE_LIFT),
        );
    }

    // Segment notches, scored over the fill as well as the recess so the
    // filled half stays countable too.
    if spec.segments > 1 {
        let notch = egui::Color32::from_rgba_unmultiplied(
            colors::BAR_NOTCH.r(),
            colors::BAR_NOTCH.g(),
            colors::BAR_NOTCH.b(),
            HUD_BAR_NOTCH_ALPHA,
        );
        for i in 1..spec.segments {
            let x = inner.left() + inner.width() * i as f32 / spec.segments as f32;
            strip(egui::pos2(x, inner.top()), egui::vec2(1.0, inner.height()), notch);
        }
    }

    // Hairline frame between the recess and the panel.
    painter.rect_stroke(
        rect,
        0.0,
        egui::Stroke::new(style::BORDER_WIDTH, colors::BAR_FRAME),
    );

    // Label, with a 1px drop shadow so it reads over both the fill and the
    // empty recess.
    let font = egui::FontId::proportional(HUD_BAR_FONT_SIZE);
    painter.text(
        rect.center() + egui::vec2(1.0, 1.0),
        egui::Align2::CENTER_CENTER,
        &spec.label,
        font.clone(),
        colors::BAR_BEVEL_DARK,
    );
    painter.text(
        rect.center(),
        egui::Align2::CENTER_CENTER,
        &spec.label,
        font,
        colors::TEXT_PRIMARY,
    );
}

/// Lay out and paint one icon-plus-bar row.
fn bar_row(ui: &mut egui::Ui, icon: BarIcon, spec: &BarSpec) {
    let (rect, _) = ui.allocate_exact_size(
        egui::vec2(HUD_STATUS_WIDTH, HUD_BAR_HEIGHT),
        egui::Sense::hover(),
    );
    let icon_rect = egui::Rect::from_center_size(
        egui::pos2(rect.left() + HUD_BAR_ICON_SIZE / 2.0, rect.center().y),
        egui::Vec2::splat(HUD_BAR_ICON_SIZE),
    );
    match icon {
        BarIcon::Sprite(tex, uv) => {
            ui.painter().image(tex, icon_rect, uv, egui::Color32::WHITE);
        }
        BarIcon::Glyph(text, color) => {
            ui.painter().text(
                icon_rect.center(),
                egui::Align2::CENTER_CENTER,
                text,
                egui::FontId::proportional(HUD_BAR_FONT_SIZE),
                color,
            );
        }
    }
    let bar_rect = egui::Rect::from_min_max(
        egui::pos2(icon_rect.right() + HUD_BAR_ICON_GAP, rect.top()),
        rect.right_bottom(),
    );
    paint_bar(ui.painter(), bar_rect, spec);
}

/// Short name for a status effect, for the text fallback.
fn effect_label(effect: StatusEffectType) -> &'static str {
    match effect {
        StatusEffectType::Invisible => "Invisible",
        StatusEffectType::SpeedBoost => "Speed",
        StatusEffectType::Regenerating => "Regen",
        StatusEffectType::Strengthened => "Strength",
        StatusEffectType::Protected => "Protected",
        StatusEffectType::Barkskin => "Barkskin",
        StatusEffectType::Confused => "Confused",
        StatusEffectType::Feared => "Feared",
        StatusEffectType::Slowed => "Slowed",
        StatusEffectType::Burning => "Burning",
        StatusEffectType::Rooted => "Rooted",
        StatusEffectType::Invulnerable => "Invuln",
        StatusEffectType::Stunned => "Stunned",
    }
}

/// Render the status bar (health, XP, gold, game clock, status effects)
pub fn draw_status_bar(
    ctx: &egui::Context,
    data: &StatusBarData,
    anim: &mut StatusBarAnim,
    icons: &UiIcons,
    game_time: f32,
) {
    let health_percent = if data.health_max > 0 {
        data.health_current as f32 / data.health_max as f32
    } else {
        0.0
    };
    let chip_hp = anim.advance_chip(data.health_current);
    let chip_percent = if data.health_max > 0 {
        chip_hp / data.health_max as f32
    } else {
        0.0
    };
    // Under the threshold the fill breathes, so a dangerous HP bar is loud even
    // in peripheral vision.
    let hp_glow = if health_percent < HP_LOW_PULSE_THRESHOLD {
        1.0 + HP_LOW_PULSE_DEPTH * anim.pulse(HP_LOW_PULSE_RATE)
    } else {
        1.0
    };

    // The window sizes itself to its content: the rows below come and go, and
    // summing their heights by hand only ever drifts out of step with them.
    egui::Window::new("Status")
        .fixed_pos([10.0, 10.0])
        .resizable(false)
        .title_bar(false)
        .frame(style::dungeon_window_frame())
        .show(ctx, |ui| {
            ui.set_min_width(HUD_STATUS_WIDTH);

            // HP, with the chip ghost, a notch per HUD_BAR_HP_PER_NOTCH of max
            // HP, and the low-HP pulse.
            let mut hp = BarSpec::new(
                health_percent,
                colors::HP_BAR,
                colors::HP_BAR_BG,
                format!("{}/{}", data.health_current, data.health_max),
            );
            hp.chip = Some(chip_percent);
            hp.segments = (data.health_max / HUD_BAR_HP_PER_NOTCH).clamp(0, HUD_BAR_MAX_NOTCHES);
            hp.glow = hp_glow;
            bar_row(
                ui,
                BarIcon::Sprite(icons.items_texture_id, icons.heart_uv),
                &hp,
            );

            bar_row(
                ui,
                BarIcon::Sprite(icons.items_texture_id, icons.diamond_uv),
                &BarSpec::new(
                    data.xp_progress,
                    colors::XP_BAR,
                    colors::XP_BAR_BG,
                    format!("Lv {} - {:.0}%", data.xp_level, data.xp_progress * 100.0),
                ),
            );

            // Hunger drains over time; food refills it.
            bar_row(
                ui,
                BarIcon::Sprite(icons.items_texture_id, icons.cheese_uv),
                &BarSpec::new(
                    (data.hunger / HUNGER_MAX).clamp(0.0, 1.0),
                    colors::HUNGER_BAR,
                    colors::HUNGER_BAR_BG,
                    format!("{:.0}/{:.0}", data.hunger, HUNGER_MAX),
                ),
            );

            // Fatigue fills up as the player gets more tired.
            bar_row(
                ui,
                BarIcon::Glyph("Zz", colors::FATIGUE_ICON),
                &BarSpec::new(
                    (data.fatigue / FATIGUE_MAX).clamp(0.0, 1.0),
                    colors::FATIGUE_BAR,
                    colors::FATIGUE_BAR_BG,
                    format!("{:.0}/{:.0}", data.fatigue, FATIGUE_MAX),
                ),
            );

            // Gold with coins icon
            ui.horizontal(|ui| {
                let coin_img = egui::Image::new(egui::load::SizedTexture::new(
                    icons.items_texture_id,
                    egui::vec2(HUD_BAR_ICON_SIZE, HUD_BAR_ICON_SIZE),
                ))
                .uv(icons.coins_uv);
                ui.add(coin_img);
                ui.label(format!("{}", data.gold));
            });

            // Defense (only shown when the player has armor)
            if data.defense > 0 {
                ui.label(format!("🛡 Defense: {}", data.defense));
            }

            // Concealed (derived: standing in tall grass)
            if data.is_concealed {
                ui.label(egui::RichText::new("🌿 Concealed").color(colors::CONCEALED));
            }

            // Sneaking (derived: crouch toggle)
            if data.is_sneaking {
                ui.label(egui::RichText::new("👁 Sneaking").color(colors::SNEAKING));
            }

            // Hunger warning label (derived from the hunger meter)
            match data.hunger_state {
                HungerState::Hungry => {
                    ui.label(
                        egui::RichText::new("Hungry — no natural healing")
                            .color(colors::HUNGER_WARNING),
                    );
                }
                HungerState::Starving => {
                    ui.label(
                        egui::RichText::new("Starving!")
                            .strong()
                            .color(colors::HUNGER_CRITICAL),
                    );
                }
                HungerState::Fed => {}
            }

            // Fatigue warning label (derived from the fatigue meter)
            match data.fatigue_state {
                FatigueState::Tired => {
                    ui.label(
                        egui::RichText::new("Tired — sloppy and easy to spot")
                            .color(colors::FATIGUE_WARNING),
                    );
                }
                FatigueState::Exhausted => {
                    ui.label(
                        egui::RichText::new("Exhausted!")
                            .strong()
                            .color(colors::FATIGUE_CRITICAL),
                    );
                }
                FatigueState::Rested => {}
            }

            // Elapsed game time (HH:MM:SS)
            ui.horizontal(|ui| {
                ui.label(
                    egui::RichText::new("Time")
                        .color(colors::TEXT_MUTED)
                        .small(),
                );
                ui.label(
                    egui::RichText::new(format_game_clock(game_time))
                        .monospace()
                        .color(colors::TEXT_PRIMARY),
                );
            });

            // Active status effects
            if !data.active_effects.is_empty() {
                ui.separator();
                ui.horizontal_wrapped(|ui| {
                    for (effect_type, duration) in &data.active_effects {
                        ui.label(
                            egui::RichText::new(format!(
                                "{} ({:.0}s)",
                                effect_label(*effect_type),
                                duration
                            ))
                            .color(style::effect_color(*effect_type))
                            .small(),
                        );
                    }
                });
            }
        });
}
