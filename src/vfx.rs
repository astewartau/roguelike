//! Visual effects system for one-shot animations (slashes, particles, etc.)
//!
//! These are separate from entity state - they're spawned, animated, and removed
//! without affecting game logic.

use crate::events::GameEvent;

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

#[derive(Clone, Copy)]
pub enum EffectType {
    /// Diagonal slash mark (for melee hits)
    Slash { angle: f32 },
}

impl EffectType {
    pub fn duration(&self) -> f32 {
        match self {
            EffectType::Slash { .. } => 0.2,
        }
    }
}

/// Manager for all active visual effects
pub struct VfxManager {
    pub effects: Vec<VisualEffect>,
}

impl VfxManager {
    pub fn new() -> Self {
        Self { effects: Vec::new() }
    }

    /// Spawn a new effect
    pub fn spawn(&mut self, x: f32, y: f32, effect_type: EffectType) {
        self.effects.push(VisualEffect::new(x, y, effect_type));
    }

    /// Spawn a slash effect at target position
    pub fn spawn_slash(&mut self, x: f32, y: f32) {
        // Slight randomization to angle for variety
        let angle = std::f32::consts::FRAC_PI_4; // 45 degrees
        self.spawn(x, y, EffectType::Slash { angle });
    }

    /// Update all effects, removing finished ones
    pub fn update(&mut self, dt: f32) {
        self.effects.retain_mut(|effect| effect.update(dt));
    }

    /// Handle a game event, spawning appropriate VFX
    pub fn handle_event(&mut self, event: &GameEvent) {
        match event {
            GameEvent::AttackHit { target_pos, .. } => {
                self.spawn_slash(target_pos.0, target_pos.1);
            }
            GameEvent::EntityDied { position, .. } => {
                // Could spawn death particles here in the future
                let _ = position;
            }
            _ => {}
        }
    }
}
