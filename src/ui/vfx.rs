//! Visual effects UI rendering.
//!
//! Handles rendering of damage numbers, alert indicators, explosions,
//! health bars, status indicators, and buff auras.

use crate::camera::Camera;
use crate::components::{ChaseAI, EffectType, Health, ItemType, StatusEffects, VisualPosition};
use crate::constants::{DAMAGE_NUMBER_RISE, POTION_SPLASH_RADIUS};
use crate::grid::Grid;
use crate::systems::effects;
use crate::vfx::{VfxType, VisualEffect};
use hecs::{Entity, World};

/// Data for an enemy with active status effects
pub struct EnemyStatusData {
    pub x: f32,
    pub y: f32,
    pub is_feared: bool,
    pub is_slowed: bool,
    pub is_confused: bool,
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
    })
}

/// Extract enemy status effect data from the world
pub fn get_enemy_status_data(world: &World, grid: &Grid) -> Vec<EnemyStatusData> {
    world
        .query::<(&VisualPosition, &ChaseAI, &StatusEffects)>()
        .iter()
        .filter(|(_, (pos, _, _))| {
            // Only show for visible tiles
            grid.get(pos.x as i32, pos.y as i32)
                .map(|t| t.visible)
                .unwrap_or(false)
        })
        .map(|(_, (pos, _, status_effects))| EnemyStatusData {
            x: pos.x,
            y: pos.y,
            is_feared: effects::has_effect(status_effects, EffectType::Feared),
            is_slowed: effects::has_effect(status_effects, EffectType::Slowed),
            is_confused: effects::has_effect(status_effects, EffectType::Confused),
        })
        .collect()
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

/// Render floating damage numbers
pub fn draw_damage_numbers(ctx: &egui::Context, effects: &[VisualEffect], camera: &Camera) {
    let painter = ctx.layer_painter(egui::LayerId::new(
        egui::Order::Foreground,
        egui::Id::new("damage_numbers"),
    ));

    // Get egui's pixels per point for HiDPI scaling
    let ppp = ctx.pixels_per_point();

    for effect in effects {
        let VfxType::DamageNumber { amount } = &effect.effect_type else {
            continue;
        };

        let progress = effect.progress();

        // Convert world position to screen position
        // The effect position is already centered on the tile
        let rise_offset = progress * DAMAGE_NUMBER_RISE;
        let world_x = effect.x;
        let world_y = effect.y + rise_offset; // Rise up (positive Y is up in world space)

        // Transform from world to screen coordinates (in physical pixels)
        let screen_pos = camera.world_to_screen(world_x, world_y);

        // Convert to egui points (egui uses logical points, not physical pixels)
        let egui_x = screen_pos.0 / ppp;
        let egui_y = screen_pos.1 / ppp;

        // Fade out as progress increases
        let alpha = ((1.0 - progress) * 255.0) as u8;

        // Red color for damage
        let color = egui::Color32::from_rgba_unmultiplied(255, 80, 80, alpha);

        // Draw the damage number
        let text = format!("{}", amount);
        let font_id = egui::FontId::monospace(20.0);

        painter.text(
            egui::pos2(egui_x, egui_y),
            egui::Align2::CENTER_CENTER,
            text,
            font_id,
            color,
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

/// Render health bars above damaged enemies
pub fn draw_enemy_health_bars(ctx: &egui::Context, camera: &Camera, enemies: &[EnemyHealthData]) {
    if enemies.is_empty() {
        return;
    }

    let painter = ctx.layer_painter(egui::LayerId::new(
        egui::Order::Foreground,
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
    time: f32,
) {
    let painter = ctx.layer_painter(egui::LayerId::new(
        egui::Order::Foreground,
        egui::Id::new("enemy_status_indicators"),
    ));

    let ppp = ctx.pixels_per_point();

    for enemy in enemies {
        if !enemy.is_feared && !enemy.is_slowed && !enemy.is_confused {
            continue;
        }

        // Collect symbols to display
        let mut symbols: Vec<(&str, egui::Color32)> = Vec::new();
        if enemy.is_feared {
            symbols.push(("!", egui::Color32::from_rgb(255, 80, 80))); // Red for fear
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

/// Render glowing aura around player for active buffs (Regenerating, Protected)
pub fn draw_player_buff_auras(
    ctx: &egui::Context,
    camera: &Camera,
    data: Option<&PlayerBuffAuraData>,
) {
    let Some(data) = data else {
        return;
    };
    if !data.has_regen && !data.has_protected {
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

    // Use real time for smooth animation (not game time)
    let real_time = ctx.input(|i| i.time) as f32;

    // Pulsing effect
    let pulse = 0.7 + 0.3 * (real_time * 3.0).sin();

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
}
