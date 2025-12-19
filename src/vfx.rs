//! Visual effects system for one-shot animations (slashes, particles, etc.)
//!
//! These are separate from entity state - they're spawned, animated, and removed
//! without affecting game logic.

use crate::constants::*;
use crate::events::GameEvent;
use crate::grid::Grid;

/// A one-shot visual effect
pub struct VisualEffect {
    pub x: f32,
    pub y: f32,
    pub effect_type: EffectType,
    pub timer: f32,      // Time remaining
    pub duration: f32,   // Total duration (for progress calculation)
}

impl VisualEffect {
    pub fn new(x: f32, y: f32, effect_type: EffectType) -> Self {
        let duration = effect_type.duration();
        Self {
            x,
            y,
            effect_type,
            timer: duration,
            duration,
        }
    }

    /// Progress from 0.0 (just started) to 1.0 (finished)
    pub fn progress(&self) -> f32 {
        1.0 - (self.timer / self.duration)
    }

    /// Returns true if effect is finished and should be removed
    pub fn is_finished(&self) -> bool {
        self.timer <= 0.0
    }

    /// Update the effect, returns true if still alive
    pub fn update(&mut self, dt: f32) -> bool {
        self.timer -= dt;
        !self.is_finished()
    }
}

#[derive(Clone)]
pub enum EffectType {
    /// Diagonal slash mark (for melee hits)
    Slash { angle: f32 },
    /// Floating damage number
    DamageNumber { amount: i32 },
    /// Fire particle effect (looping)
    Fire { seed: f32 },
    /// Alert indicator "!" when enemy spots player
    Alert,
}

/// Duration for alert indicator
const ALERT_DURATION: f32 = 0.8;

impl EffectType {
    pub fn duration(&self) -> f32 {
        match self {
            EffectType::Slash { .. } => SLASH_VFX_DURATION,
            EffectType::DamageNumber { .. } => DAMAGE_NUMBER_DURATION,
            EffectType::Fire { .. } => f32::INFINITY, // Fire loops forever
            EffectType::Alert => ALERT_DURATION,
        }
    }
}

/// A persistent fire effect (doesn't expire)
pub struct FireEffect {
    pub x: f32,
    pub y: f32,
    pub seed: f32,
    pub time: f32, // Accumulated time for animation
}

impl FireEffect {
    pub fn new(x: f32, y: f32, seed: f32) -> Self {
        Self { x, y, seed, time: 0.0 }
    }

    pub fn update(&mut self, dt: f32) {
        self.time += dt;
    }
}

/// Manager for all active visual effects
pub struct VfxManager {
    pub effects: Vec<VisualEffect>,
    pub fires: Vec<FireEffect>,
}

impl VfxManager {
    pub fn new() -> Self {
        Self { effects: Vec::new(), fires: Vec::new() }
    }

    /// Spawn a new effect
    pub fn spawn(&mut self, x: f32, y: f32, effect_type: EffectType) {
        self.effects.push(VisualEffect::new(x, y, effect_type));
    }

    /// Spawn a slash effect at target position
    pub fn spawn_slash(&mut self, x: f32, y: f32) {
        self.spawn(x, y, EffectType::Slash { angle: SLASH_VFX_ANGLE });
    }

    /// Spawn a floating damage number
    pub fn spawn_damage_number(&mut self, x: f32, y: f32, amount: i32) {
        self.spawn(x, y, EffectType::DamageNumber { amount });
    }

    /// Spawn an alert indicator "!" above an entity
    pub fn spawn_alert(&mut self, x: f32, y: f32) {
        self.spawn(x, y, EffectType::Alert);
    }

    /// Spawn a persistent fire effect
    pub fn spawn_fire(&mut self, x: f32, y: f32) {
        let seed = rand::random::<f32>() * 1000.0;
        self.fires.push(FireEffect::new(x, y, seed));
    }

    /// Update all effects, removing finished ones
    pub fn update(&mut self, dt: f32) {
        self.effects.retain_mut(|effect| effect.update(dt));
        // Update fire animation times
        for fire in &mut self.fires {
            fire.update(dt);
        }
    }

    /// Handle a game event, spawning appropriate VFX.
    /// Only spawns VFX for positions visible to the player (not in fog of war).
    pub fn handle_event(&mut self, event: &GameEvent, grid: &Grid) {
        match event {
            GameEvent::AttackHit { target_pos, damage, .. } => {
                // Only show VFX if the position is visible to the player
                let tile_x = target_pos.0 as i32;
                let tile_y = target_pos.1 as i32;
                if grid.get(tile_x, tile_y).map(|t| t.visible).unwrap_or(false) {
                    self.spawn_slash(target_pos.0, target_pos.1);
                    self.spawn_damage_number(target_pos.0, target_pos.1, *damage);
                }
            }
            GameEvent::ProjectileHit { position, damage, target, .. } => {
                // Only show damage number if we hit an enemy (not a wall) AND position is visible
                if target.is_some() {
                    if grid.get(position.0, position.1).map(|t| t.visible).unwrap_or(false) {
                        self.spawn_damage_number(position.0 as f32, position.1 as f32, *damage);
                    }
                }
            }
            GameEvent::EntityDied { position, .. } => {
                // Could spawn death particles here in the future
                let _ = position;
            }
            _ => {}
        }
    }
}
