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
    DAMAGE_NUMBER_TAKEN_SCALE, EXPLOSION_FLASH_DURATION, EXPLOSION_RING_DELAY,
    EXPLOSION_PUFF_SPILL, EXPLOSION_SHOCKWAVE_DURATION, EXPLOSION_TILE_DURATION, MISS_TEXT_SCALE,
    POTION_SPLASH_RADIUS, RAIN_FRAME_DURATION,
};
use super::icons::UiIcons;
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
    /// Fighter's Guard is up: a bright steel ring.
    pub has_guarding: bool,
    /// Druid's Thorns: green spikes.
    pub has_thorns: bool,
    /// Necromancer's Bone Ward charges left (0 = no ward): one orbiting bone
    /// mote per charge.
    pub bone_ward_charges: u32,
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
        has_guarding: effects::has_effect(&status_effects, EffectType::Guarding),
        has_thorns: effects::has_effect(&status_effects, EffectType::Thorns),
        bone_ward_charges: if effects::has_effect(&status_effects, EffectType::BoneWard) {
            world
                .get::<&crate::components::BoneWard>(player_entity)
                .map(|w| w.charges)
                .unwrap_or(0)
        } else {
            0
        },
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
        // An invisible enemy is not drawn, so neither are its status icons.
        .filter(|(_, (_, _, status_effects, _, _))| {
            !effects::has_effect(status_effects, EffectType::Invisible)
        })
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
/// tile is included, so the marker stays put while standing on the loot, and
/// each tile appears once however many containers share it.
pub fn get_loot_indicator_data(world: &World, grid: &Grid) -> Vec<(i32, i32)> {
    let mut tiles: Vec<(i32, i32)> = world
        .query::<(&crate::components::Position, &crate::components::Container)>()
        .iter()
        .filter(|(id, (_, container))| {
            !container.is_empty()
                && (container.is_open
                    || world.get::<&crate::components::BlocksMovement>(*id).is_err())
        })
        .map(|(_, (pos, _))| (pos.x, pos.y))
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

/// Visible hostile attacks in progress at game time `game_time`, plus fusing
/// oil barrels (drawn like the boss slam: an area filling toward detonation).
///
/// Only attacks whose attacker stands on a visible tile are shown, and only
/// the visible tiles of each; a swing in the dark stays a surprise.
pub fn get_attack_telegraph_data(world: &World, grid: &Grid, game_time: f32) -> Vec<AttackTelegraphData> {
    use crate::systems::telegraph::{all_telegraphs, TelegraphShape};

    let visible = |(x, y): (i32, i32)| grid.get(x, y).map(|t| t.visible).unwrap_or(false);
    all_telegraphs(world, game_time)
        .into_iter()
        .filter(|t| visible(t.attacker_pos))
        .filter_map(|t| {
            let mut tiles = t.shape.tiles();
            // A charge lane ends at the first wall: the dash cannot go further.
            if let TelegraphShape::Lane { .. } = t.shape {
                let open = tiles.iter().take_while(|&&(x, y)| grid.is_walkable(x, y)).count();
                tiles.truncate(open);
            }
            let tiles: Vec<(i32, i32)> = tiles.into_iter().filter(|&p| visible(p)).collect();
            let (label_tile, is_area) = match t.shape {
                TelegraphShape::Tile(p) => (p, false),
                TelegraphShape::Area { center, .. } => (center, true),
                // Label the lane where it starts, next to the orc.
                TelegraphShape::Lane { .. } => (*tiles.first()?, false),
            };
            if tiles.is_empty() {
                return None;
            }
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
        // An invisible enemy is not drawn, so its health bar would give it away.
        .filter(|(id, _)| !effects::entity_has_effect(world, *id, EffectType::Invisible))
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
        let (amount, tier, jitter, miss_label) = match &effect.effect_type {
            VfxType::DamageNumber { amount, tier, jitter } => {
                (*amount, Some(*tier), *jitter, None)
            }
            VfxType::HealNumber { amount } => (*amount, None, 0.0, None),
            VfxType::MissText { jitter, label } => (0, None, *jitter, Some(*label)),
            _ => continue,
        };
        let is_miss = miss_label.is_some();

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
            None if is_miss => miss_label.unwrap_or("miss").to_string(),
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

/// The on-screen rect of the tile whose centre is at world (`cx`, `cy`).
fn tile_rect(camera: &Camera, ppp: f32, cx: f32, cy: f32) -> egui::Rect {
    let tile = camera.zoom / ppp;
    // World y points up: this is the tile's bottom-left corner on screen.
    let (sx, sy) = camera.world_to_screen(cx - 0.5, cy - 0.5);
    egui::Rect::from_min_size(egui::pos2(sx / ppp, sy / ppp - tile), egui::vec2(tile, tile))
}

/// A cheap, stable hash of a tile offset, for per-tile variety that doesn't
/// flicker from frame to frame.
fn tile_hash(dx: i32, dy: i32) -> u32 {
    let h = (dx as u32).wrapping_mul(0x9E37_79B9) ^ (dy as u32).wrapping_mul(0x85EB_CA6B);
    (h ^ (h >> 15)).wrapping_mul(0x2C1B_3C6D)
}

/// Render explosions: a flash and a shockwave ring from the centre, and for
/// a fiery one a blast of fire and smoke (`tile_ids::EXPLOSION`) on every
/// tile it covers, starting later the further out, so it rolls outward.
/// Tiles are flipped and nudged at random so the grid doesn't show.
pub fn draw_explosions(ctx: &egui::Context, effects: &[VisualEffect], camera: &Camera, icons: &UiIcons) {
    let painter = ctx.layer_painter(egui::LayerId::new(
        egui::Order::Foreground,
        egui::Id::new("explosions"),
    ));

    let ppp = ctx.pixels_per_point();
    let tile_size = camera.zoom / ppp;

    for effect in effects {
        let VfxType::Explosion { radius, fiery } = effect.effect_type else {
            continue;
        };
        // Seconds since it went off
        let t = effect.progress() * effect.duration;
        let (cx, cy) = camera.world_to_screen(effect.x, effect.y);
        let centre = egui::pos2(cx / ppp, cy / ppp);
        let reach = (radius as f32 + 0.5) * tile_size;

        if t < EXPLOSION_FLASH_DURATION {
            let k = 1.0 - t / EXPLOSION_FLASH_DURATION;
            let flash = egui::Color32::from_rgba_unmultiplied(255, 244, 214, (140.0 * k) as u8);
            painter.circle_filled(centre, reach * (1.0 - 0.4 * k), flash);
        }

        if t < EXPLOSION_SHOCKWAVE_DURATION {
            let k = t / EXPLOSION_SHOCKWAVE_DURATION;
            let r = tile_size * 0.3 + (reach + tile_size * 0.5) * ease::out_cubic(k);
            let (red, green, blue) = if fiery { (255, 205, 130) } else { (205, 185, 150) };
            let ring = egui::Color32::from_rgba_unmultiplied(red, green, blue, (190.0 * (1.0 - k)) as u8);
            painter.circle_stroke(centre, r, egui::Stroke::new(tile_size * 0.12 * (1.0 - k) + 1.0, ring));
        }

        let frames = crate::tile::tile_ids::EXPLOSION_FRAMES as usize;
        let variants = icons.explosion_uvs.len() / frames;
        if !fiery || variants == 0 {
            continue;
        }
        // Outermost ring first, so the hotter middle draws over its smoke.
        for ring in (0..=radius).rev() {
            let local = t - ring as f32 * EXPLOSION_RING_DELAY;
            if !(0.0..EXPLOSION_TILE_DURATION).contains(&local) {
                continue;
            }
            // Further out, the blast arrives already burning rather than
            // as a flash of its own, which would read as a grid of dots.
            let skip = (ring as usize).min(2).min(frames - 1);
            let ring_frame = skip + (local / EXPLOSION_TILE_DURATION * (frames - skip) as f32) as usize;

            for (dx, dy) in ring_tiles(ring) {
                let h = tile_hash(dx, dy);
                // Each tile its own shape of blast, some a frame ahead of
                // their neighbours: no two side by side quite alike.
                let lead = if ring > 0 { (h >> 24) as usize % 2 } else { 0 };
                let frame = (ring_frame + lead).min(frames - 1);
                // The smoke thins away over the last two frames.
                let left = frames - 1 - frame;
                let opacity = if left < 2 { 0.45 + 0.3 * left as f32 } else { 1.0 };
                let variant = (h >> 4) as usize % variants;
                let nudge = |bits: u32| ((bits & 0xff) as f32 / 255.0 - 0.5) * 0.2 * tile_size;
                // Half again as big as a tile, so neighbouring puffs merge
                // into one cloud instead of sitting in their own squares.
                let rect = tile_rect(camera, ppp, effect.x + dx as f32, effect.y + dy as f32)
                    .expand(tile_size * EXPLOSION_PUFF_SPILL)
                    .translate(egui::vec2(nudge(h >> 8), nudge(h >> 16)));
                let mut uv = icons.explosion_uvs[variant * frames + frame];
                if h & 1 == 1 {
                    uv = egui::Rect::from_min_max(egui::pos2(uv.max.x, uv.min.y), egui::pos2(uv.min.x, uv.max.y));
                }
                painter.image(
                    icons.items_texture_id,
                    rect,
                    uv,
                    egui::Color32::from_white_alpha((255.0 * opacity) as u8),
                );
            }
        }
    }
}

/// The tile offsets exactly `ring` steps (Chebyshev) from the centre.
fn ring_tiles(ring: i32) -> impl Iterator<Item = (i32, i32)> {
    (-ring..=ring)
        .flat_map(move |dy| (-ring..=ring).map(move |dx| (dx, dy)))
        .filter(move |(dx, dy)| dx.abs().max(dy.abs()) == ring)
}

/// Call Rain still falling: its tiles you can see, and how strongly it's
/// raining (it eases off over the last moments).
pub struct RainPatch {
    pub tiles: Vec<(i32, i32)>,
    pub opacity: f32,
}

/// Every rain cloud in the world, as the visible tiles it falls on.
pub fn get_rain_data(world: &World, grid: &Grid) -> Vec<RainPatch> {
    use crate::components::{Position, RainCloud};
    world
        .query::<(&Position, &RainCloud)>()
        .iter()
        .map(|(_, (p, cloud))| RainPatch {
            tiles: (-cloud.radius..=cloud.radius)
                .flat_map(|dy| (-cloud.radius..=cloud.radius).map(move |dx| (p.x + dx, p.y + dy)))
                .filter(|&(x, y)| grid.get(x, y).is_some_and(|t| t.visible && t.tile_type.is_walkable()))
                .collect(),
            opacity: (cloud.remaining / 0.6).clamp(0.0, 1.0),
        })
        .collect()
}

/// Render Call Rain: rain (`tile_ids::RAIN`) falling on each tile of the
/// patch under a cloud's shadow, for as long as the rain lasts. Animated in
/// real time, so it keeps falling while the game waits on you.
pub fn draw_rain(ctx: &egui::Context, camera: &Camera, icons: &UiIcons, patches: &[RainPatch], time: f32) {
    let frames = icons.rain_uvs.len() as u32;
    if frames == 0 || patches.is_empty() {
        return;
    }
    let painter = ctx.layer_painter(egui::LayerId::new(
        egui::Order::Foreground,
        egui::Id::new("rain"),
    ));
    let ppp = ctx.pixels_per_point();

    for patch in patches {
        for &(x, y) in &patch.tiles {
            let rect = tile_rect(camera, ppp, x as f32 + 0.5, y as f32 + 0.5);
            let shadow = egui::Color32::from_rgba_unmultiplied(20, 30, 50, (70.0 * patch.opacity) as u8);
            painter.rect_filled(rect, 0.0, shadow);
            // Neighbouring tiles a little out of step
            let phase = tile_hash(x, y);
            let frame = ((time / RAIN_FRAME_DURATION) as u32).wrapping_add(phase) % frames;
            painter.image(
                icons.items_texture_id,
                rect,
                icons.rain_uvs[frame as usize],
                egui::Color32::from_white_alpha((235.0 * patch.opacity) as u8),
            );
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
        && !data.has_guarding
        && !data.has_thorns
        && data.bone_ward_charges == 0
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

    // Guard: a solid, bright steel ring — it only lasts a fraction of a
    // second, so it does not pulse.
    if data.has_guarding {
        let color = egui::Color32::from_rgba_unmultiplied(210, 220, 240, 190);
        painter.circle_stroke(center, tile_size * 0.52, egui::Stroke::new(3.0_f32, color));
    }

    // Thorns: short green spikes radiating from a ring.
    if data.has_thorns {
        let color = egui::Color32::from_rgba_unmultiplied(110, 190, 70, (150.0 * pulse) as u8);
        let inner = tile_size * 0.44;
        let outer = tile_size * 0.58;
        for i in 0..8 {
            let angle = i as f32 * std::f32::consts::PI / 4.0 + real_time * 0.3;
            let (c, s) = (angle.cos(), angle.sin());
            painter.line_segment(
                [
                    egui::pos2(center.x + c * inner, center.y + s * inner),
                    egui::pos2(center.x + c * outer, center.y + s * outer),
                ],
                egui::Stroke::new(2.0_f32, color),
            );
        }
    }

    // Bone Ward: one bone-white mote orbiting per charge left.
    if data.bone_ward_charges > 0 {
        let color = egui::Color32::from_rgba_unmultiplied(235, 230, 210, 210);
        let radius = tile_size * 0.5;
        let n = data.bone_ward_charges;
        for i in 0..n {
            let angle = i as f32 * std::f32::consts::TAU / n as f32 + real_time * 1.2;
            let p = egui::pos2(center.x + angle.cos() * radius, center.y + angle.sin() * radius);
            painter.circle_filled(p, 3.0_f32, color);
        }
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

// =============================================================================
// CHANNELS (Tame, Raise Dead)
// =============================================================================

/// Which channel is under way, for its colours and its label.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChannelKind {
    Tame,
    RaiseDead,
}

impl ChannelKind {
    fn label(self) -> &'static str {
        match self {
            ChannelKind::Tame => "Taming",
            ChannelKind::RaiseDead => "Raising",
        }
    }

    fn colour(self) -> egui::Color32 {
        match self {
            ChannelKind::Tame => egui::Color32::from_rgb(130, 200, 105),
            ChannelKind::RaiseDead => egui::Color32::from_rgb(120, 230, 170),
        }
    }
}

/// A channel in progress: who is channeling into what, and how far along.
pub struct ChannelData {
    pub kind: ChannelKind,
    /// Tile centres, from the visual positions
    pub caster: (f32, f32),
    pub target: (f32, f32),
    /// 0 to 1
    pub fraction: f32,
}

/// Every Tame and Raise Dead channel under way whose two ends still exist.
pub fn get_channel_data(world: &World) -> Vec<ChannelData> {
    use crate::components::{RaiseDeadInProgress, TamingInProgress};
    let centre = |e: Entity| world.get::<&VisualPosition>(e).ok().map(|v| (v.x + 0.5, v.y + 0.5));
    let mut out = Vec::new();
    let mut add = |kind, caster: Entity, target: Entity, progress: f32, required: f32| {
        if let (Some(caster), Some(target)) = (centre(caster), centre(target)) {
            let fraction = if required > 0.0 { (progress / required).clamp(0.0, 1.0) } else { 1.0 };
            out.push(ChannelData { kind, caster, target, fraction });
        }
    };
    for (id, t) in world.query::<&TamingInProgress>().iter() {
        add(ChannelKind::Tame, id, t.target, t.progress, t.required);
    }
    for (id, r) in world.query::<&RaiseDeadInProgress>().iter() {
        add(ChannelKind::RaiseDead, id, r.target, r.progress, r.required);
    }
    out
}

/// A bar over the channeler's head saying how far along the channel is
/// ("Taming 40%"), so you can see how much longer to keep waiting.
pub fn draw_channel_progress(ctx: &egui::Context, camera: &Camera, channels: &[ChannelData]) {
    if channels.is_empty() {
        return;
    }
    let painter = ctx.layer_painter(egui::LayerId::new(
        egui::Order::Background,
        egui::Id::new("channel_progress"),
    ));
    let ppp = ctx.pixels_per_point();
    let (bar_width, bar_height) = (30.0, 5.0);

    for channel in channels {
        // Just above the caster's head (world y points up)
        let (sx, sy) = camera.world_to_screen(channel.caster.0, channel.caster.1 + 0.62);
        let at = egui::pos2(sx / ppp, sy / ppp);

        let back = egui::Rect::from_center_size(at, egui::vec2(bar_width + 2.0, bar_height + 2.0));
        painter.rect_filled(back, 1.5, egui::Color32::from_rgb(15, 12, 20));
        let inner = back.shrink(1.0);
        let fill = egui::Rect::from_min_size(inner.min, egui::vec2(inner.width() * channel.fraction, inner.height()));
        painter.rect_filled(fill, 1.0, channel.kind.colour());

        let text = format!("{} {}%", channel.kind.label(), (channel.fraction * 100.0).round() as i32);
        let font = egui::FontId::proportional(11.0);
        let label_at = at - egui::vec2(0.0, bar_height + 3.0);
        painter.text(label_at + egui::vec2(1.0, 1.0), egui::Align2::CENTER_BOTTOM, &text, font.clone(), egui::Color32::BLACK);
        painter.text(label_at, egui::Align2::CENTER_BOTTOM, &text, font, egui::Color32::WHITE);
    }
}

/// Raise Dead being channeled: a ring of grave-light turning on the bones,
/// souls rising off them, and a wavering tether back to the necromancer.
pub fn draw_raise_dead_channels(ctx: &egui::Context, camera: &Camera, channels: &[ChannelData], time: f32) {
    let painter = ctx.layer_painter(egui::LayerId::new(
        egui::Order::Foreground,
        egui::Id::new("raise_dead_channels"),
    ));
    let ppp = ctx.pixels_per_point();
    let tile = camera.zoom / ppp;
    let screen = |(x, y): (f32, f32)| {
        let (sx, sy) = camera.world_to_screen(x, y);
        egui::pos2(sx / ppp, sy / ppp)
    };

    for channel in channels.iter().filter(|c| c.kind == ChannelKind::RaiseDead) {
        let bones = screen(channel.target);
        let caster = screen(channel.caster);
        // Stronger as the ritual nears its end
        let strength = 0.55 + 0.45 * channel.fraction;
        let pulse = 0.8 + 0.2 * (time * 5.0).sin();

        // Tether: a wavering line of motes from caster to bones
        let along = bones - caster;
        let len = along.length();
        if len > 4.0 {
            let side = egui::vec2(-along.y, along.x) / len;
            let steps = ((len / 5.0) as usize).max(4);
            for i in 0..=steps {
                let t = i as f32 / steps as f32;
                let wave = (t * 9.0 - time * 7.0).sin() * 2.5 * (1.0 - (2.0 * t - 1.0).abs());
                let p = caster + along * t + side * wave;
                let alpha = (150.0 * strength * pulse) as u8;
                painter.circle_filled(p, 1.6, egui::Color32::from_rgba_unmultiplied(120, 240, 170, alpha));
            }
        }

        // A turning ring of grave-light at the foot of the bones
        let foot = bones + egui::vec2(0.0, tile * 0.28);
        for i in 0..12 {
            let a = time * 1.8 + i as f32 * std::f32::consts::TAU / 12.0;
            let p = foot + egui::vec2(a.cos() * tile * 0.42, a.sin() * tile * 0.16);
            let alpha = (170.0 * strength * (0.6 + 0.4 * (a * 2.0).sin().abs())) as u8;
            painter.circle_filled(p, 1.8, egui::Color32::from_rgba_unmultiplied(150, 255, 190, alpha));
        }

        // Souls drifting up off the bones and fading
        for i in 0..6 {
            let rise = (time * 0.7 + i as f32 / 6.0).fract();
            let drift = ((time * 2.0 + i as f32 * 1.7).sin()) * tile * 0.18;
            let p = foot + egui::vec2(drift + (i as f32 - 2.5) * tile * 0.08, -rise * tile * 0.9);
            let alpha = (200.0 * strength * (1.0 - rise)) as u8;
            painter.circle_filled(p, 2.4 * (1.0 - rise * 0.5), egui::Color32::from_rgba_unmultiplied(190, 255, 215, alpha));
        }

        // The necromancer's hands glow with it
        painter.circle_filled(caster, tile * 0.18 * pulse, egui::Color32::from_rgba_unmultiplied(90, 200, 140, (70.0 * strength) as u8));
    }
}

#[cfg(test)]
mod channel_tests {
    use super::*;
    use crate::components::{Position, RaiseDeadInProgress, TamingInProgress};

    #[test]
    fn a_channel_reports_how_far_along_it_is() {
        let mut world = World::new();
        let at = |x, y| (Position::new(x, y), VisualPosition { x: x as f32, y: y as f32 });
        let wolf = world.spawn(at(5, 5));
        let druid = world.spawn(at(4, 5));
        world.insert_one(druid, TamingInProgress { target: wolf, progress: 1.5, required: 6.0 }).unwrap();
        let bones = world.spawn(at(1, 1));
        let necro = world.spawn(at(2, 1));
        world.insert_one(necro, RaiseDeadInProgress { target: bones, progress: 3.0, required: 4.0 }).unwrap();
        // A channel whose target is gone draws nothing.
        let gone = world.spawn(at(9, 9));
        let orphan = world.spawn(at(8, 9));
        world.insert_one(orphan, TamingInProgress { target: gone, progress: 0.0, required: 1.0 }).unwrap();
        world.despawn(gone).unwrap();

        let mut channels: Vec<_> = get_channel_data(&world).into_iter().map(|c| (c.kind, c.caster, c.fraction)).collect();
        channels.sort_by(|a, b| a.2.total_cmp(&b.2));
        assert_eq!(channels, vec![
            (ChannelKind::Tame, (4.5, 5.5), 0.25),
            (ChannelKind::RaiseDead, (2.5, 1.5), 0.75),
        ]);
    }
}
