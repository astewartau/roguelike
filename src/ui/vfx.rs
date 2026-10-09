//! Visual effects UI rendering.
//!
//! Handles rendering of damage numbers, alert indicators, explosions,
//! health bars, status indicators, and buff auras.

use super::style;
use crate::camera::Camera;
use crate::components::{AlarmInProgress, Asleep, ChaseAI, EffectType, Health, ItemType, StatusEffects, VisualPosition};
use crate::constants::{
    DAMAGE_NUMBER_BIG_SCALE, DAMAGE_NUMBER_CRIT_SCALE, DAMAGE_NUMBER_FONT_SIZE,
    DAMAGE_NUMBER_OUTLINE_OFFSET, DAMAGE_NUMBER_POP_SIZE_STEP, DAMAGE_NUMBER_RISE,
    DAMAGE_NUMBER_TAKEN_SCALE, MISS_TEXT_SCALE, POTION_SPLASH_RADIUS,
};
use crate::ease;
use crate::grid::Grid;
use crate::systems::effects;
use crate::vfx::{DamageTier, VfxType, VisualEffect};
use hecs::{Entity, World};

/// Data for an enemy with active status effects
pub struct EnemyStatusData {
    pub x: f32,
    pub y: f32,
    pub is_feared: bool,
    pub is_slowed: bool,
    pub is_confused: bool,
    pub is_stunned: bool,
    pub is_rooted: bool,
    pub is_asleep: bool,
    pub is_shouting: bool,
    pub is_stirring: bool,
}

/// Data for an enemy's health bar
pub struct EnemyHealthData {
    pub x: f32,
    pub y: f32,
    pub current_health: i32,
    pub max_health: i32,
}

/// Data for player buff aura visualization
pub struct PlayerBuffAuraData {
    pub player_x: f32,
    pub player_y: f32,
    pub has_regen: bool,
    pub has_protected: bool,
    pub has_barkskin: bool,
    pub is_sneaking: bool,
    /// Hunger meter at zero: dim red pulse (the body consuming itself).
    pub is_starving: bool,
    /// Fatigue meter maxed: heavy grey haze.
    pub is_exhausted: bool,
}

/// Extract player buff aura data from the world
pub fn get_buff_aura_data(world: &World, player_entity: Entity) -> Option<PlayerBuffAuraData> {
    let player_vis_pos = world.get::<&VisualPosition>(player_entity).ok()?;
    let status_effects = world.get::<&StatusEffects>(player_entity).ok()?;

    Some(PlayerBuffAuraData {
        player_x: player_vis_pos.x,
        player_y: player_vis_pos.y,
        has_regen: effects::has_effect(&status_effects, EffectType::Regenerating),
        has_protected: effects::has_effect(&status_effects, EffectType::Protected),
        has_barkskin: effects::has_effect(&status_effects, EffectType::Barkskin),
        is_sneaking: world.get::<&crate::components::Sneaking>(player_entity).is_ok(),
        is_starving: world
            .get::<&crate::components::Hunger>(player_entity)
            .map(|h| h.is_starving())
            .unwrap_or(false),
        is_exhausted: world
            .get::<&crate::components::Fatigue>(player_entity)
            .map(|f| f.is_exhausted())
            .unwrap_or(false),
    })
}

/// Extract enemy status effect data from the world
pub fn get_enemy_status_data(world: &World, grid: &Grid) -> Vec<EnemyStatusData> {
    world
        .query::<(&VisualPosition, &ChaseAI, &StatusEffects, Option<&Asleep>, Option<&AlarmInProgress>)>()
        .iter()
        .filter(|(_, (pos, _, _, _, _))| {
            // Only show for visible tiles
            grid.get(pos.x as i32, pos.y as i32)
                .map(|t| t.visible)
                .unwrap_or(false)
        })
        .map(|(_, (pos, ai, status_effects, asleep, alarm))| {
            let stirring = ai.state == crate::components::AIState::Unaware && ai.alertness > 0.0;
            EnemyStatusData {
                x: pos.x,
                y: pos.y,
                is_feared: effects::has_effect(status_effects, EffectType::Feared),
                is_slowed: effects::has_effect(status_effects, EffectType::Slowed),
                is_confused: effects::has_effect(status_effects, EffectType::Confused),
                is_stunned: effects::has_effect(status_effects, EffectType::Stunned),
                is_rooted: effects::has_effect(status_effects, EffectType::Rooted),
                // Show "stirring" instead of "asleep" once the meter starts rising.
                is_asleep: asleep.is_some() && !stirring,
                is_shouting: alarm.is_some(),
                is_stirring: stirring,
            }
        })
        .collect()
}

/// Visible tiles holding something the player could loot: a corpse or item
/// pile with anything in it, or an opened chest left half-emptied. Closed
/// chests do not count — what is in them is not known yet. The player's own
/// tile is skipped (the loot window covers that), and each tile appears once
/// however many containers share it.
pub fn get_loot_indicator_data(world: &World, grid: &Grid, player_entity: Entity) -> Vec<(i32, i32)> {
    let player_tile = world
        .get::<&crate::components::Position>(player_entity)
        .map(|p| (p.x, p.y))
        .ok();
    let mut tiles: Vec<(i32, i32)> = world
        .query::<(&crate::components::Position, &crate::components::Container)>()
        .iter()
        .filter(|(id, (_, container))| {
            !container.is_empty()
                && (container.is_open
                    || world.get::<&crate::components::BlocksMovement>(*id).is_err())
        })
        .map(|(_, (pos, _))| (pos.x, pos.y))
        .filter(|&tile| Some(tile) != player_tile)
        .filter(|&(x, y)| grid.get(x, y).map(|t| t.visible).unwrap_or(false))
        .collect();
    tiles.sort_unstable();
    tiles.dedup();
    tiles
}

/// Draw a small gold diamond over each lootable tile.
pub fn draw_loot_indicators(ctx: &egui::Context, camera: &Camera, tiles: &[(i32, i32)]) {
    use crate::constants::{
        LOOT_MARKER_ALPHA, LOOT_MARKER_BOB_AMPLITUDE, LOOT_MARKER_BOB_SPEED,
        LOOT_MARKER_HALF_SIZE, LOOT_MARKER_HEIGHT,
    };

    if tiles.is_empty() {
        return;
    }

    // Same layer as the enemy health bars: over the world, under the panels.
    let painter = ctx.layer_painter(egui::LayerId::new(
        egui::Order::Background,
        egui::Id::new("loot_indicators"),
    ));
    let ppp = ctx.pixels_per_point();
    let time = ctx.input(|i| i.time) as f32;

    let fill = style::colors::TEXT_ACCENT.gamma_multiply(LOOT_MARKER_ALPHA as f32 / 255.0);
    let outline = egui::Stroke::new(
        1.0,
        egui::Color32::from_black_alpha(LOOT_MARKER_ALPHA),
    );

    for &(x, y) in tiles {
        let (sx, sy) = camera.world_to_screen(x as f32 + 0.5, y as f32 + LOOT_MARKER_HEIGHT);
        // Offset each tile's phase by its position so a room full of corpses
        // does not bob in lockstep.
        let phase = (x * 7 + y * 13) as f32 * 0.37;
        let bob = (time * LOOT_MARKER_BOB_SPEED + phase).sin() * LOOT_MARKER_BOB_AMPLITUDE;
        let c = egui::pos2(sx / ppp, sy / ppp + bob);
        let h = LOOT_MARKER_HALF_SIZE;
        let diamond = vec![
            egui::pos2(c.x, c.y - h),
            egui::pos2(c.x + h, c.y),
            egui::pos2(c.x, c.y + h),
            egui::pos2(c.x - h, c.y),
        ];
        painter.add(egui::Shape::convex_polygon(diamond, fill, outline));
    }
    // Keep the bob moving even when nothing else is asking for a repaint.
    ctx.request_repaint();
}

/// One visible hostile attack in progress, ready to draw.
pub struct AttackTelegraphData {
    /// Visible tiles the attack will hit.
    pub tiles: Vec<(i32, i32)>,
    /// Tile the "time until it lands" label goes on.
    pub label_tile: (i32, i32),
    /// 0.0 just started, 1.0 about to land (game time, so frozen while the
    /// player decides).
    pub progress: f32,
    /// Game seconds until it lands.
    pub remaining: f32,
    /// An area attack (ground slam) rather than a single melee swing.
    pub is_area: bool,
}

/// Visible hostile attacks in progress at game time `game_time`.
///
/// Only attacks whose attacker stands on a visible tile are shown, and only
/// the visible tiles of each; a swing in the dark stays a surprise.
pub fn get_attack_telegraph_data(world: &World, grid: &Grid, game_time: f32) -> Vec<AttackTelegraphData> {
    use crate::systems::telegraph::{hostile_telegraphs, TelegraphShape};

    let visible = |(x, y): (i32, i32)| grid.get(x, y).map(|t| t.visible).unwrap_or(false);
    hostile_telegraphs(world, game_time)
        .into_iter()
        .filter(|t| visible(t.attacker_pos))
        .filter_map(|t| {
            let tiles: Vec<(i32, i32)> = t.shape.tiles().into_iter().filter(|&p| visible(p)).collect();
            if tiles.is_empty() {
                return None;
            }
            let (label_tile, is_area) = match t.shape {
                TelegraphShape::Tile(p) => (p, false),
                TelegraphShape::Area { center, .. } => (center, true),
            };
            Some(AttackTelegraphData {
                tiles,
                label_tile,
                progress: t.progress,
                remaining: t.remaining,
                is_area,
            })
        })
        .collect()
}

/// Draw a red marker on every tile a visible hostile is about to hit.
///
/// The outline is there from the first moment; inside it a fill grows from
/// `ATTACK_TELEGRAPH_MIN_FILL` of the tile to all of it and darkens as the
/// attack nears landing. With `show_timer` (the player is choosing an action)
/// each threatened tile also gets a "0.3s" label: the soonest attack landing
/// there, in game seconds.
pub fn draw_attack_telegraphs(
    ctx: &egui::Context,
    camera: &Camera,
    telegraphs: &[AttackTelegraphData],
    show_timer: bool,
) {
    use crate::constants::{
        ATTACK_TELEGRAPH_ALPHA_MAX, ATTACK_TELEGRAPH_ALPHA_MIN, ATTACK_TELEGRAPH_COLOR,
        ATTACK_TELEGRAPH_LABEL_FONT_SIZE, ATTACK_TELEGRAPH_LABEL_HEIGHT,
        ATTACK_TELEGRAPH_MIN_FILL, ATTACK_TELEGRAPH_OUTLINE_ALPHA,
        ATTACK_TELEGRAPH_OUTLINE_WIDTH, SLAM_TELEGRAPH_ALPHA_SCALE,
    };

    if telegraphs.is_empty() {
        return;
    }

    // Same layer as the health bars and loot markers: over the world, under
    // every panel.
    let painter = ctx.layer_painter(egui::LayerId::new(
        egui::Order::Background,
        egui::Id::new("attack_telegraphs"),
    ));
    let ppp = ctx.pixels_per_point();
    let tile_size = camera.zoom / ppp;
    let (r, g, b) = ATTACK_TELEGRAPH_COLOR;

    // Tile rect in egui points: world (x, y) is the tile's bottom-left corner.
    let tile_rect = |x: i32, y: i32| {
        let (sx, sy) = camera.world_to_screen(x as f32, y as f32);
        egui::Rect::from_min_size(
            egui::pos2(sx / ppp, sy / ppp - tile_size),
            egui::vec2(tile_size, tile_size),
        )
    };

    // Soonest landing per tile, for the labels.
    let mut soonest: Vec<((i32, i32), f32)> = Vec::new();

    for t in telegraphs {
        let p = t.progress.clamp(0.0, 1.0);
        let scale = if t.is_area { SLAM_TELEGRAPH_ALPHA_SCALE } else { 1.0 };
        let alpha_min = ATTACK_TELEGRAPH_ALPHA_MIN as f32;
        let alpha_max = ATTACK_TELEGRAPH_ALPHA_MAX as f32;
        let fill_alpha = ((alpha_min + (alpha_max - alpha_min) * p) * scale) as u8;
        let outline_alpha = (ATTACK_TELEGRAPH_OUTLINE_ALPHA as f32 * scale) as u8;
        let fill = egui::Color32::from_rgba_unmultiplied(r, g, b, fill_alpha);
        let outline = egui::Stroke::new(
            ATTACK_TELEGRAPH_OUTLINE_WIDTH,
            egui::Color32::from_rgba_unmultiplied(r, g, b, outline_alpha),
        );
        let fill_frac = ATTACK_TELEGRAPH_MIN_FILL + (1.0 - ATTACK_TELEGRAPH_MIN_FILL) * p;

        for &(x, y) in &t.tiles {
            let rect = tile_rect(x, y);
            painter.rect_stroke(rect.shrink(ATTACK_TELEGRAPH_OUTLINE_WIDTH / 2.0), 0.0, outline);
            painter.rect_filled(
                egui::Rect::from_center_size(rect.center(), rect.size() * fill_frac),
                0.0,
                fill,
            );
        }

        match soonest.iter_mut().find(|(tile, _)| *tile == t.label_tile) {
            Some((_, rem)) => *rem = rem.min(t.remaining),
            None => soonest.push((t.label_tile, t.remaining)),
        }
    }

    if !show_timer {
        return;
    }
    let font = egui::FontId::monospace(ATTACK_TELEGRAPH_LABEL_FONT_SIZE);
    let color = egui::Color32::from_rgb(r, g, b);
    for ((x, y), remaining) in soonest {
        let (sx, sy) =
            camera.world_to_screen(x as f32 + 0.5, y as f32 + ATTACK_TELEGRAPH_LABEL_HEIGHT);
        outlined_number(
            &painter,
            egui::pos2(sx / ppp, sy / ppp),
            format!("{remaining:.1}s"),
            font.clone(),
            color,
            style::colors::NUMBER_OUTLINE,
        );
    }
}

/// Extract health data for visible damaged enemies
pub fn get_enemy_health_data(world: &World, grid: &Grid, player_entity: Entity) -> Vec<EnemyHealthData> {
    world
        .query::<(&VisualPosition, &Health, &ChaseAI)>()
        .iter()
        .filter(|(id, _)| *id != player_entity) // Exclude player
        .filter(|(_, (pos, health, _))| {
            // Only show for visible tiles and damaged enemies
            let is_visible = grid
                .get(pos.x as i32, pos.y as i32)
                .map(|t| t.visible)
                .unwrap_or(false);
            let is_damaged = health.current < health.max;
            is_visible && is_damaged
        })
        .map(|(_, (pos, health, _))| EnemyHealthData {
            x: pos.x,
            y: pos.y,
            current_health: health.current,
            max_health: health.max,
        })
        .collect()
}

/// Colour and font-size multiplier for a damage tier.
fn damage_style(tier: DamageTier) -> (egui::Color32, f32) {
    match tier {
        DamageTier::Dealt => (style::colors::DAMAGE_DEALT, 1.0),
        DamageTier::Big => (style::colors::DAMAGE_BIG, DAMAGE_NUMBER_BIG_SCALE),
        DamageTier::Crit => (style::colors::DAMAGE_CRIT, DAMAGE_NUMBER_CRIT_SCALE),
        DamageTier::Taken => (style::colors::DAMAGE_TAKEN, DAMAGE_NUMBER_TAKEN_SCALE),
    }
}

/// Draw `text` centred on `pos` with a black outline behind it.
///
/// The outline is the number drawn four times at the diagonals; without it
/// these vanish over a lit stone floor. Two galleys are laid out rather than
/// five, because only the colour differs between the copies.
fn outlined_number(
    painter: &egui::Painter,
    pos: egui::Pos2,
    text: String,
    font: egui::FontId,
    color: egui::Color32,
    outline: egui::Color32,
) {
    let shadow = painter.layout_no_wrap(text.clone(), font.clone(), outline);
    let face = painter.layout_no_wrap(text, font, color);
    let top_left = pos - face.size() / 2.0;
    let d = DAMAGE_NUMBER_OUTLINE_OFFSET;
    for offset in [
        egui::vec2(-d, -d),
        egui::vec2(d, -d),
        egui::vec2(-d, d),
        egui::vec2(d, d),
    ] {
        painter.galley(top_left + offset, shadow.clone(), outline);
    }
    painter.galley(top_left, face, color);
}

/// Render floating damage and heal numbers
pub fn draw_damage_numbers(ctx: &egui::Context, effects: &[VisualEffect], camera: &Camera) {
    let painter = ctx.layer_painter(egui::LayerId::new(
        egui::Order::Foreground,
        egui::Id::new("damage_numbers"),
    ));

    // Get egui's pixels per point for HiDPI scaling
    let ppp = ctx.pixels_per_point();

    for effect in effects {
        // Handle both damage and heal numbers. Heals are their own thing and
        // do not tier: there is no such thing as a critical heal here.
        // A miss is not a number at all, but it floats the same way.
        let (amount, tier, jitter, is_miss) = match &effect.effect_type {
            VfxType::DamageNumber { amount, tier, jitter } => {
                (*amount, Some(*tier), *jitter, false)
            }
            VfxType::HealNumber { amount } => (*amount, None, 0.0, false),
            VfxType::MissText { jitter } => (0, None, *jitter, true),
            _ => continue,
        };

        let progress = effect.progress();

        // Convert world position to screen position
        // The effect position is already centered on the tile.
        // Eased rather than linear: the number leaps away from the hit and
        // settles, instead of drifting at a constant speed.
        let rise_offset = ease::out_cubic(progress) * DAMAGE_NUMBER_RISE;
        // The jitter is per instance, so three hits on one tile fan out
        // instead of overprinting each other.
        let world_x = effect.x + jitter;
        let world_y = effect.y + rise_offset; // Rise up (positive Y is up in world space)

        // Transform from world to screen coordinates (in physical pixels)
        let screen_pos = camera.world_to_screen(world_x, world_y);

        // Convert to egui points (egui uses logical points, not physical pixels)
        let egui_x = screen_pos.0 / ppp;
        let egui_y = screen_pos.1 / ppp;

        // Fade out as progress increases
        let fade = 1.0 - progress;

        let (base_color, tier_scale) = match tier {
            Some(tier) => damage_style(tier),
            None if is_miss => (style::colors::MISS_TEXT, MISS_TEXT_SCALE),
            None => (style::colors::HEAL_NUMBER, 1.0),
        };
        let color = base_color.gamma_multiply(fade);
        let outline = style::colors::NUMBER_OUTLINE.gamma_multiply(fade);

        // Crits announce themselves; heals keep their + prefix.
        let text = match tier {
            Some(DamageTier::Crit) => format!("{}!", amount),
            Some(_) => format!("{}", amount),
            None if is_miss => "miss".to_string(),
            None => format!("+{}", amount),
        };

        // Punch out oversized and settle back, so a hit lands rather than
        // merely appearing. Rounded to a step because egui caches rasterized
        // glyphs per distinct font size — the tier multipliers multiply the
        // number of distinct sizes, so the rounding matters more now, not
        // less.
        let popped = DAMAGE_NUMBER_FONT_SIZE * tier_scale * ease::pop(progress);
        let font_size =
            (popped / DAMAGE_NUMBER_POP_SIZE_STEP).round() * DAMAGE_NUMBER_POP_SIZE_STEP;
        let font_id = egui::FontId::monospace(font_size);

        outlined_number(
            &painter,
            egui::pos2(egui_x, egui_y),
            text,
            font_id,
            color,
            outline,
        );
    }
}

/// Render alert indicators ("!") when enemies spot the player
pub fn draw_alert_indicators(ctx: &egui::Context, effects: &[VisualEffect], camera: &Camera) {
    let painter = ctx.layer_painter(egui::LayerId::new(
        egui::Order::Foreground,
        egui::Id::new("alert_indicators"),
    ));

    // Get egui's pixels per point for HiDPI scaling
    let ppp = ctx.pixels_per_point();

    for effect in effects {
        let VfxType::Alert = &effect.effect_type else {
            continue;
        };

        let progress = effect.progress();

        // Pop up animation: start small, grow to full size, then shrink slightly
        let scale = if progress < 0.2 {
            // Quick pop-in (0.0 to 0.2 progress -> 0.0 to 1.2 scale)
            progress * 6.0
        } else if progress < 0.4 {
            // Settle to normal size (0.2 to 0.4 progress -> 1.2 to 1.0 scale)
            1.2 - (progress - 0.2) * 1.0
        } else {
            // Hold at normal size, then fade
            1.0
        };

        // Rise slightly above the entity
        let rise_offset = 0.8;
        let world_x = effect.x;
        let world_y = effect.y + rise_offset;

        // Transform from world to screen coordinates
        let screen_pos = camera.world_to_screen(world_x, world_y);

        // Convert to egui points
        let egui_x = screen_pos.0 / ppp;
        let egui_y = screen_pos.1 / ppp;

        // Fade out near the end
        let alpha = if progress > 0.7 {
            ((1.0 - progress) / 0.3 * 255.0) as u8
        } else {
            255
        };

        // Yellow/orange color for alert
        let color = egui::Color32::from_rgba_unmultiplied(255, 200, 50, alpha);

        // Draw the "!"
        let font_size = 28.0 * scale;
        let font_id = egui::FontId::monospace(font_size);

        painter.text(
            egui::pos2(egui_x, egui_y),
            egui::Align2::CENTER_CENTER,
            "!",
            font_id,
            color,
        );
    }
}

/// Render the persistent resting indicator ("Zzz" speech bubble) above the
/// player while they are resting. Drawn from a single persistent bubble (not the
/// timed VFX list) so it stays put and gently bobs for the whole rest.
pub fn draw_resting_indicators(
    ctx: &egui::Context,
    bubble: Option<&crate::vfx::RestingBubble>,
    camera: &Camera,
) {
    let Some(bubble) = bubble else {
        return;
    };

    let painter = ctx.layer_painter(egui::LayerId::new(
        egui::Order::Foreground,
        egui::Id::new("resting_indicators"),
    ));
    let ppp = ctx.pixels_per_point();

    // Quick fade/scale-in over the first ~0.2s, then hold.
    let appear = (bubble.time / 0.2).min(1.0);
    let alpha = (appear * 255.0) as u8;
    if alpha == 0 {
        return;
    }
    let scale = 0.6 + 0.4 * appear;

    // Gentle vertical bob.
    let bob = (bubble.time * 3.0).sin() * 0.06;
    let rise_offset = 0.95 + bob;
    let screen_pos = camera.world_to_screen(bubble.x, bubble.y + rise_offset);
    let center = egui::pos2(screen_pos.0 / ppp, screen_pos.1 / ppp);

    let font_id = egui::FontId::proportional(20.0 * scale);
    let text_color = egui::Color32::from_rgba_unmultiplied(40, 50, 80, alpha);
    let bubble_fill = egui::Color32::from_rgba_unmultiplied(245, 248, 255, alpha);
    let bubble_stroke = egui::Stroke::new(
        1.5,
        egui::Color32::from_rgba_unmultiplied(120, 140, 180, alpha),
    );

    // Size the rounded bubble around the text.
    let galley = painter.layout_no_wrap("Zzz".to_string(), font_id.clone(), text_color);
    let pad = egui::vec2(8.0 * scale, 5.0 * scale);
    let rect = egui::Rect::from_center_size(center, galley.size() + pad * 2.0);
    let rounding = rect.height() / 2.0;

    // Little tail pointing down toward the player (drawn first, then covered by
    // the bubble body so only the protruding tip shows).
    let tail = vec![
        egui::pos2(center.x - 5.0 * scale, rect.bottom() - 1.0),
        egui::pos2(center.x + 5.0 * scale, rect.bottom() - 1.0),
        egui::pos2(center.x - 2.0 * scale, rect.bottom() + 7.0 * scale),
    ];
    painter.add(egui::Shape::convex_polygon(tail, bubble_fill, bubble_stroke));

    painter.rect_filled(rect, rounding, bubble_fill);
    painter.rect_stroke(rect, rounding, bubble_stroke);
    painter.text(center, egui::Align2::CENTER_CENTER, "Zzz", font_id, text_color);
}

/// Render health bars above damaged enemies
pub fn draw_enemy_health_bars(ctx: &egui::Context, camera: &Camera, enemies: &[EnemyHealthData]) {
    if enemies.is_empty() {
        return;
    }

    // Background order: above the (OpenGL) game world but below the egui UI
    // windows (hotbars, status, inventory), which sit at Order::Middle.
    let painter = ctx.layer_painter(egui::LayerId::new(
        egui::Order::Background,
        egui::Id::new("enemy_health_bars"),
    ));

    let ppp = ctx.pixels_per_point();

    // Health bar dimensions
    let bar_width = 24.0;
    let bar_height = 4.0;
    let bar_y_offset = 0.9; // Position above the sprite (higher Y = higher on screen)

    for enemy in enemies {
        // Convert world position to screen position
        let world_x = enemy.x + 0.5; // Center on tile
        let world_y = enemy.y + bar_y_offset;

        let screen_pos = camera.world_to_screen(world_x, world_y);
        let egui_x = screen_pos.0 / ppp;
        let egui_y = screen_pos.1 / ppp;

        // Calculate health percentage
        let health_pct = (enemy.current_health as f32 / enemy.max_health as f32).clamp(0.0, 1.0);

        // Background (dark)
        let bg_rect = egui::Rect::from_center_size(
            egui::pos2(egui_x, egui_y),
            egui::vec2(bar_width, bar_height),
        );
        painter.rect_filled(bg_rect, 1.0, egui::Color32::from_rgb(20, 15, 15));

        // Health fill (red to yellow to green based on health)
        let fill_color = if health_pct > 0.5 {
            // Green to yellow
            let t = (health_pct - 0.5) * 2.0;
            egui::Color32::from_rgb((255.0 * (1.0 - t)) as u8, 200, 50)
        } else {
            // Yellow to red
            let t = health_pct * 2.0;
            egui::Color32::from_rgb(220, (180.0 * t) as u8, 50)
        };

        let fill_width = bar_width * health_pct;
        if fill_width > 0.0 {
            let fill_rect = egui::Rect::from_min_size(
                egui::pos2(egui_x - bar_width / 2.0, egui_y - bar_height / 2.0),
                egui::vec2(fill_width, bar_height),
            );
            painter.rect_filled(fill_rect, 1.0, fill_color);
        }

        // Border
        painter.rect_stroke(
            bg_rect,
            1.0,
            egui::Stroke::new(1.0, egui::Color32::from_rgb(40, 35, 35)),
        );
    }
}

/// Render persistent status effect indicators above enemies
pub fn draw_enemy_status_indicators(
    ctx: &egui::Context,
    camera: &Camera,
    enemies: &[EnemyStatusData],
) {
    let painter = ctx.layer_painter(egui::LayerId::new(
        egui::Order::Foreground,
        egui::Id::new("enemy_status_indicators"),
    ));

    let ppp = ctx.pixels_per_point();

    // Animate on real time so the pulse keeps going while game-time is paused
    // (matches the player buff auras, which also use the real-time clock).
    let time = ctx.input(|i| i.time) as f32;

    for enemy in enemies {
        if !enemy.is_feared
            && !enemy.is_slowed
            && !enemy.is_confused
            && !enemy.is_stunned
            && !enemy.is_rooted
            && !enemy.is_asleep
            && !enemy.is_shouting
            && !enemy.is_stirring
        {
            continue;
        }

        // Collect symbols to display
        let mut symbols: Vec<(&str, egui::Color32)> = Vec::new();
        if enemy.is_asleep {
            symbols.push(("z", egui::Color32::from_rgb(150, 180, 220))); // Pale blue "Zzz" for sleep
        }
        if enemy.is_stirring {
            symbols.push(("?", egui::Color32::from_rgb(255, 220, 120))); // Yellow "?" — noticing you
        }
        if enemy.is_shouting {
            symbols.push(("❗", egui::Color32::from_rgb(255, 160, 40))); // Orange alarm for shouting
        }
        if enemy.is_stunned {
            symbols.push(("*", egui::Color32::from_rgb(255, 230, 120))); // Gold "seeing stars" for stun
        }
        if enemy.is_feared {
            symbols.push(("!", egui::Color32::from_rgb(255, 80, 80))); // Red for fear
        }
        if enemy.is_rooted {
            symbols.push(("⚓", egui::Color32::from_rgb(139, 90, 43))); // Brown anchor for root/snare
        }
        if enemy.is_slowed {
            symbols.push(("❄", egui::Color32::from_rgb(100, 150, 255))); // Blue for slow
        }
        if enemy.is_confused {
            symbols.push(("?", egui::Color32::from_rgb(200, 100, 200))); // Purple for confusion
        }

        // Subtle pulsing animation
        let pulse = 0.85 + 0.15 * (time * 4.0).sin();

        // Position above the entity
        let rise_offset = 0.75;
        let world_x = enemy.x;
        let world_y = enemy.y + rise_offset;

        let screen_pos = camera.world_to_screen(world_x, world_y);
        let egui_x = screen_pos.0 / ppp;
        let egui_y = screen_pos.1 / ppp;

        // Draw each symbol, offset horizontally if multiple
        let total_width = symbols.len() as f32 * 16.0;
        let start_x = egui_x - total_width / 2.0 + 8.0;

        for (i, (symbol, color)) in symbols.iter().enumerate() {
            let x = start_x + i as f32 * 16.0;
            let font_size = 20.0 * pulse;
            let font_id = egui::FontId::monospace(font_size);

            painter.text(
                egui::pos2(x, egui_y),
                egui::Align2::CENTER_CENTER,
                *symbol,
                font_id,
                *color,
            );
        }
    }
}

/// Render explosion effects (fireballs)
pub fn draw_explosions(ctx: &egui::Context, effects: &[VisualEffect], camera: &Camera) {
    let painter = ctx.layer_painter(egui::LayerId::new(
        egui::Order::Foreground,
        egui::Id::new("explosions"),
    ));

    let ppp = ctx.pixels_per_point();
    let tile_size = camera.zoom / ppp;

    for effect in effects {
        let VfxType::Explosion { radius } = &effect.effect_type else {
            continue;
        };

        let progress = effect.progress();

        // Explosion expands outward then fades
        let expand = if progress < 0.3 {
            progress / 0.3
        } else {
            1.0
        };

        let alpha = if progress > 0.5 {
            ((1.0 - progress) / 0.5 * 200.0) as u8
        } else {
            200
        };

        // Draw expanding circles for the explosion
        for r in 0..=*radius {
            let r_progress = r as f32 / (*radius as f32).max(1.0);
            let current_expand = expand * (1.0 - r_progress * 0.3);

            // Calculate color: orange/red gradient
            let red = 255;
            let green = (150.0 * (1.0 - r_progress)) as u8;
            let blue = (50.0 * (1.0 - r_progress)) as u8;
            let ring_alpha = (alpha as f32 * (1.0 - r_progress * 0.5)) as u8;

            let color = egui::Color32::from_rgba_unmultiplied(red, green, blue, ring_alpha);

            // Draw tiles in this ring
            for dx in -r..=r {
                for dy in -r..=r {
                    let dist = dx.abs().max(dy.abs());
                    if dist != r {
                        continue;
                    }

                    let world_x = effect.x + dx as f32;
                    let world_y = effect.y + dy as f32;

                    let screen_pos = camera.world_to_screen(world_x - 0.5, world_y - 0.5);
                    let egui_x = screen_pos.0 / ppp;
                    let egui_y = screen_pos.1 / ppp;

                    let size = tile_size * current_expand;
                    let offset = (tile_size - size) / 2.0;

                    let rect = egui::Rect::from_min_size(
                        egui::pos2(egui_x + offset, egui_y - tile_size + offset),
                        egui::vec2(size, size),
                    );
                    painter.rect_filled(rect, size / 4.0, color);
                }
            }
        }
    }
}

/// Render potion splash effects
pub fn draw_potion_splashes(ctx: &egui::Context, effects: &[VisualEffect], camera: &Camera) {
    let painter = ctx.layer_painter(egui::LayerId::new(
        egui::Order::Foreground,
        egui::Id::new("potion_splashes"),
    ));

    let ppp = ctx.pixels_per_point();
    let tile_size = camera.zoom / ppp;

    for effect in effects {
        let VfxType::PotionSplash { potion_type } = &effect.effect_type else {
            continue;
        };

        let progress = effect.progress();

        // Determine color based on potion type
        let (base_r, base_g, base_b) = match potion_type {
            ItemType::HealthPotion => (220, 50, 50),       // Red
            ItemType::RegenerationPotion => (50, 200, 80), // Green
            ItemType::StrengthPotion => (220, 160, 50),    // Amber/Orange
            ItemType::ConfusionPotion => (80, 120, 220),   // Blue
            ItemType::WaterFlaskFull => (90, 160, 230),    // Water blue
            _ => (200, 200, 200),                          // Fallback gray
        };

        // Splash expands outward then fades
        let expand = if progress < 0.2 {
            progress / 0.2
        } else {
            1.0
        };

        let alpha = if progress > 0.4 {
            ((1.0 - progress) / 0.6 * 180.0) as u8
        } else {
            180
        };

        // Draw splash in the splash radius (1 tile)
        let radius = POTION_SPLASH_RADIUS;
        for r in 0..=radius {
            let r_progress = r as f32 / (radius as f32).max(1.0);
            let current_expand = expand * (1.0 - r_progress * 0.2);

            // Fade color slightly outward
            let red = (base_r as f32 * (1.0 - r_progress * 0.2)) as u8;
            let green = (base_g as f32 * (1.0 - r_progress * 0.2)) as u8;
            let blue = (base_b as f32 * (1.0 - r_progress * 0.2)) as u8;
            let ring_alpha = (alpha as f32 * (1.0 - r_progress * 0.4)) as u8;

            let color = egui::Color32::from_rgba_unmultiplied(red, green, blue, ring_alpha);

            // Draw tiles in this ring
            for dx in -r..=r {
                for dy in -r..=r {
                    let dist = dx.abs().max(dy.abs());
                    if dist != r {
                        continue;
                    }

                    let world_x = effect.x + dx as f32;
                    let world_y = effect.y + dy as f32;

                    let screen_pos = camera.world_to_screen(world_x - 0.5, world_y - 0.5);
                    let egui_x = screen_pos.0 / ppp;
                    let egui_y = screen_pos.1 / ppp;

                    let size = tile_size * current_expand;
                    let offset = (tile_size - size) / 2.0;

                    let rect = egui::Rect::from_min_size(
                        egui::pos2(egui_x + offset, egui_y - tile_size + offset),
                        egui::vec2(size, size),
                    );
                    painter.rect_filled(rect, size / 3.0, color);
                }
            }
        }
    }
}

/// Render glowing aura around player for active buffs (Regenerating, Protected, Barkskin)
pub fn draw_player_buff_auras(
    ctx: &egui::Context,
    camera: &Camera,
    data: Option<&PlayerBuffAuraData>,
) {
    let Some(data) = data else {
        return;
    };
    if !data.has_regen
        && !data.has_protected
        && !data.has_barkskin
        && !data.is_sneaking
        && !data.is_starving
        && !data.is_exhausted
    {
        return;
    }

    let painter = ctx.layer_painter(egui::LayerId::new(
        egui::Order::Background,
        egui::Id::new("buff_auras"),
    ));

    let ppp = ctx.pixels_per_point();
    let tile_size = camera.zoom / ppp;

    // Convert player position to screen
    let screen_pos = camera.world_to_screen(data.player_x, data.player_y);
    let egui_x = screen_pos.0 / ppp;
    let egui_y = screen_pos.1 / ppp;

    // Center of the player tile
    let center = egui::pos2(egui_x + tile_size / 2.0, egui_y - tile_size / 2.0);

    // Sneaking indicator: an eye emoji tucked into the top-right corner of the
    // player sprite (foreground so it reads over the sprite/terrain).
    if data.is_sneaking {
        let fg = ctx.layer_painter(egui::LayerId::new(
            egui::Order::Foreground,
            egui::Id::new("sneak_indicator"),
        ));
        // Top-left corner of the player tile (like the other status indicators).
        let corner = egui::pos2(center.x - tile_size * 0.32, center.y - tile_size * 0.32);
        let font = egui::FontId::proportional((tile_size * 0.5).clamp(14.0, 24.0));
        // Transparent deep blue/purple.
        fg.text(
            corner,
            egui::Align2::CENTER_CENTER,
            "👁",
            font,
            egui::Color32::from_rgba_unmultiplied(110, 80, 190, 170),
        );
    }

    // Use real time for smooth animation (not game time)
    let real_time = ctx.input(|i| i.time) as f32;

    // Pulsing effect
    let pulse = 0.7 + 0.3 * (real_time * 3.0).sin();

    // Starving: a slow, dim red pulse — the body consuming itself.
    if data.is_starving {
        let slow_pulse = 0.6 + 0.4 * (real_time * 1.5).sin();
        let color =
            egui::Color32::from_rgba_unmultiplied(200, 45, 30, (55.0 * slow_pulse) as u8);
        painter.circle_filled(center, tile_size * 0.55, color);
    }

    // Exhausted: a heavy grey haze around the player.
    if data.is_exhausted {
        let color = egui::Color32::from_rgba_unmultiplied(120, 115, 140, 50);
        painter.circle_filled(center, tile_size * 0.6, color);
    }

    // Draw regeneration aura (green glow)
    if data.has_regen {
        let base_alpha = (80.0 * pulse) as u8;
        let color = egui::Color32::from_rgba_unmultiplied(50, 255, 100, base_alpha);
        let radius = tile_size * 0.6 * (0.9 + 0.1 * pulse);
        painter.circle_filled(center, radius, color);

        // Inner brighter ring
        let inner_color = egui::Color32::from_rgba_unmultiplied(100, 255, 150, (40.0 * pulse) as u8);
        painter.circle_filled(center, radius * 0.7, inner_color);
    }

    // Draw protection aura (blue glow) - drawn on top if both active
    if data.has_protected {
        let base_alpha = (70.0 * pulse) as u8;
        let color = egui::Color32::from_rgba_unmultiplied(100, 150, 255, base_alpha);
        let radius = tile_size * 0.55 * (0.9 + 0.1 * pulse);
        painter.circle_stroke(center, radius, egui::Stroke::new(3.0 * pulse, color));

        // Shield icon effect - draw small diamond shapes around
        let shield_alpha = (100.0 * pulse) as u8;
        let shield_color = egui::Color32::from_rgba_unmultiplied(150, 180, 255, shield_alpha);
        let shield_radius = tile_size * 0.45;
        for i in 0..4 {
            let angle = (i as f32 * std::f32::consts::PI / 2.0) + real_time * 0.5;
            let px = center.x + angle.cos() * shield_radius;
            let py = center.y + angle.sin() * shield_radius;
            painter.circle_filled(egui::pos2(px, py), 2.0 * pulse, shield_color);
        }
    }

    // Draw barkskin aura (brown/green nature-themed glow)
    if data.has_barkskin {
        // Bark brown outer ring with pulsing
        let bark_alpha = (80.0 * pulse) as u8;
        let bark_color = egui::Color32::from_rgba_unmultiplied(139, 90, 43, bark_alpha);
        let bark_radius = tile_size * 0.58 * (0.92 + 0.08 * pulse);
        painter.circle_stroke(center, bark_radius, egui::Stroke::new(4.0 * pulse, bark_color));

        // Green leaf-like particles orbiting
        let leaf_alpha = (120.0 * pulse) as u8;
        let leaf_color = egui::Color32::from_rgba_unmultiplied(60, 140, 60, leaf_alpha);
        let leaf_radius = tile_size * 0.48;
        for i in 0..6 {
            let angle = (i as f32 * std::f32::consts::PI / 3.0) + real_time * 0.8;
            let px = center.x + angle.cos() * leaf_radius;
            let py = center.y + angle.sin() * leaf_radius;
            painter.circle_filled(egui::pos2(px, py), 2.5 * pulse, leaf_color);
        }

        // Inner brown bark texture ring
        let inner_bark_alpha = (50.0 * pulse) as u8;
        let inner_bark_color = egui::Color32::from_rgba_unmultiplied(101, 67, 33, inner_bark_alpha);
        painter.circle_stroke(center, bark_radius * 0.7, egui::Stroke::new(2.0, inner_bark_color));
    }
}

/// Data for life drain beam rendering
pub struct LifeDrainBeamData {
    pub caster_x: f32,
    pub caster_y: f32,
    pub target_x: f32,
    pub target_y: f32,
    pub time: f32,
}

/// Extract life drain beam data from the world
pub fn get_life_drain_beam_data(
    world: &World,
    beams: &[crate::vfx::LifeDrainBeam],
) -> Vec<LifeDrainBeamData> {
    beams
        .iter()
        .filter_map(|beam| {
            let caster_pos = world.get::<&VisualPosition>(beam.caster).ok()?;
            let target_pos = world.get::<&VisualPosition>(beam.target).ok()?;
            Some(LifeDrainBeamData {
                caster_x: caster_pos.x + 0.5,
                caster_y: caster_pos.y + 0.5,
                target_x: target_pos.x + 0.5,
                target_y: target_pos.y + 0.5,
                time: beam.time,
            })
        })
        .collect()
}

/// Render life drain beams (siphoning energy connection)
pub fn draw_life_drain_beams(
    ctx: &egui::Context,
    camera: &Camera,
    beams: &[LifeDrainBeamData],
) {
    if beams.is_empty() {
        return;
    }

    let painter = ctx.layer_painter(egui::LayerId::new(
        egui::Order::Foreground,
        egui::Id::new("life_drain_beams"),
    ));

    let ppp = ctx.pixels_per_point();

    for beam in beams {
        // Convert world positions to screen
        let caster_screen = camera.world_to_screen(beam.caster_x, beam.caster_y);
        let target_screen = camera.world_to_screen(beam.target_x, beam.target_y);

        let caster_pos = egui::pos2(caster_screen.0 / ppp, caster_screen.1 / ppp);
        let target_pos = egui::pos2(target_screen.0 / ppp, target_screen.1 / ppp);

        // Pulsing animation
        let pulse = 0.7 + 0.3 * (beam.time * 6.0).sin();
        let wave = (beam.time * 4.0).sin();

        // Draw main beam (dark purple/red gradient)
        let beam_alpha = (180.0 * pulse) as u8;
        let beam_color = egui::Color32::from_rgba_unmultiplied(150, 50, 100, beam_alpha);
        painter.line_segment(
            [caster_pos, target_pos],
            egui::Stroke::new(3.0 * pulse, beam_color),
        );

        // Draw inner bright core
        let core_alpha = (220.0 * pulse) as u8;
        let core_color = egui::Color32::from_rgba_unmultiplied(220, 80, 120, core_alpha);
        painter.line_segment(
            [caster_pos, target_pos],
            egui::Stroke::new(1.5 * pulse, core_color),
        );

        // Draw floating particles along the beam (moving from target to caster)
        let dx = caster_pos.x - target_pos.x;
        let dy = caster_pos.y - target_pos.y;
        let len = (dx * dx + dy * dy).sqrt();

        if len > 5.0 {
            let num_particles = ((len / 15.0) as usize).clamp(3, 8);
            for i in 0..num_particles {
                // Particle moves from target to caster over time
                let base_t = i as f32 / num_particles as f32;
                let t = (base_t + beam.time * 0.5).fract(); // Loop along beam

                // Add some wave motion perpendicular to beam
                let perpx = -dy / len;
                let perpy = dx / len;
                let wave_offset = wave * 3.0 * (1.0 - (t - 0.5).abs() * 2.0);

                let px = target_pos.x + dx * t + perpx * wave_offset;
                let py = target_pos.y + dy * t + perpy * wave_offset;

                let particle_alpha = (150.0 * pulse * (1.0 - (t - 0.5).abs() * 1.5).max(0.0)) as u8;
                let particle_color = egui::Color32::from_rgba_unmultiplied(255, 100, 150, particle_alpha);
                painter.circle_filled(egui::pos2(px, py), 2.5 * pulse, particle_color);
            }
        }

        // Draw glow at target (being drained)
        let drain_glow_alpha = (100.0 * pulse) as u8;
        let drain_glow_color = egui::Color32::from_rgba_unmultiplied(200, 50, 80, drain_glow_alpha);
        painter.circle_filled(target_pos, 8.0 * pulse, drain_glow_color);

        // Draw glow at caster (receiving life)
        let heal_glow_alpha = (80.0 * pulse) as u8;
        let heal_glow_color = egui::Color32::from_rgba_unmultiplied(100, 200, 100, heal_glow_alpha);
        painter.circle_filled(caster_pos, 6.0 * pulse, heal_glow_color);
    }
}

/// Data for taming channel beam rendering
pub struct TamingBeamData {
    pub tamer_x: f32,
    pub tamer_y: f32,
    pub target_x: f32,
    pub target_y: f32,
    pub time: f32,
}

/// Extract taming beam data from the world. Only emits a beam while the tamer is
/// still channeling (has a `TamingInProgress` component) and both ends exist.
pub fn get_taming_beam_data(
    world: &World,
    beams: &[crate::vfx::TamingBeam],
) -> Vec<TamingBeamData> {
    beams
        .iter()
        .filter_map(|beam| {
            world
                .get::<&crate::components::TamingInProgress>(beam.tamer)
                .ok()?;
            let tamer_pos = world.get::<&VisualPosition>(beam.tamer).ok()?;
            let target_pos = world.get::<&VisualPosition>(beam.target).ok()?;
            Some(TamingBeamData {
                tamer_x: tamer_pos.x + 0.5,
                tamer_y: tamer_pos.y + 0.5,
                target_x: target_pos.x + 0.5,
                target_y: target_pos.y + 0.5,
                time: beam.time,
            })
        })
        .collect()
}

/// Render taming channels (calming nature connection from tamer to target).
pub fn draw_taming_beams(ctx: &egui::Context, camera: &Camera, beams: &[TamingBeamData]) {
    if beams.is_empty() {
        return;
    }

    let painter = ctx.layer_painter(egui::LayerId::new(
        egui::Order::Foreground,
        egui::Id::new("taming_beams"),
    ));

    let ppp = ctx.pixels_per_point();

    for beam in beams {
        let tamer_screen = camera.world_to_screen(beam.tamer_x, beam.tamer_y);
        let target_screen = camera.world_to_screen(beam.target_x, beam.target_y);

        let tamer_pos = egui::pos2(tamer_screen.0 / ppp, tamer_screen.1 / ppp);
        let target_pos = egui::pos2(target_screen.0 / ppp, target_screen.1 / ppp);

        // Gentle, calming pulse (slower than life drain's hungry pulse)
        let pulse = 0.75 + 0.25 * (beam.time * 4.0).sin();
        let wave = (beam.time * 3.0).sin();

        // Main beam (soft green)
        let beam_alpha = (150.0 * pulse) as u8;
        let beam_color = egui::Color32::from_rgba_unmultiplied(90, 180, 100, beam_alpha);
        painter.line_segment(
            [tamer_pos, target_pos],
            egui::Stroke::new(3.0 * pulse, beam_color),
        );

        // Inner bright core (pale gold-green)
        let core_alpha = (210.0 * pulse) as u8;
        let core_color = egui::Color32::from_rgba_unmultiplied(190, 230, 150, core_alpha);
        painter.line_segment(
            [tamer_pos, target_pos],
            egui::Stroke::new(1.5 * pulse, core_color),
        );

        // Floating motes drifting from the tamer toward the target (offering calm)
        let dx = target_pos.x - tamer_pos.x;
        let dy = target_pos.y - tamer_pos.y;
        let len = (dx * dx + dy * dy).sqrt();

        if len > 5.0 {
            let num_particles = ((len / 15.0) as usize).clamp(3, 8);
            for i in 0..num_particles {
                let base_t = i as f32 / num_particles as f32;
                let t = (base_t + beam.time * 0.4).fract(); // drift along beam

                let perpx = -dy / len;
                let perpy = dx / len;
                let wave_offset = wave * 3.0 * (1.0 - (t - 0.5).abs() * 2.0);

                let px = tamer_pos.x + dx * t + perpx * wave_offset;
                let py = tamer_pos.y + dy * t + perpy * wave_offset;

                let particle_alpha =
                    (150.0 * pulse * (1.0 - (t - 0.5).abs() * 1.5).max(0.0)) as u8;
                let particle_color =
                    egui::Color32::from_rgba_unmultiplied(210, 235, 140, particle_alpha);
                painter.circle_filled(egui::pos2(px, py), 2.5 * pulse, particle_color);
            }
        }

        // Soft glow at the target (being soothed)
        let target_glow_alpha = (110.0 * pulse) as u8;
        let target_glow_color = egui::Color32::from_rgba_unmultiplied(120, 220, 120, target_glow_alpha);
        painter.circle_filled(target_pos, 8.0 * pulse, target_glow_color);

        // Warm glow at the tamer (channeling)
        let tamer_glow_alpha = (80.0 * pulse) as u8;
        let tamer_glow_color = egui::Color32::from_rgba_unmultiplied(220, 200, 110, tamer_glow_alpha);
        painter.circle_filled(tamer_pos, 6.0 * pulse, tamer_glow_color);
    }
}
