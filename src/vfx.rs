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
    pub effect_type: VfxType,
    pub timer: f32,      // Time remaining
    pub duration: f32,   // Total duration (for progress calculation)
}

impl VisualEffect {
    pub fn new(x: f32, y: f32, effect_type: VfxType) -> Self {
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
pub enum VfxType {
    /// Diagonal slash mark (for melee hits)
    Slash { angle: f32 },
    /// Floating damage number
    DamageNumber { amount: i32 },
    /// Floating heal number (green, positive)
    HealNumber { amount: i32 },
    /// Fire particle effect (looping)
    #[allow(dead_code)] // Reserved for torch/fire terrain
    Fire { seed: f32 },
    /// Alert indicator "!" when enemy spots player
    Alert,
    /// Explosion effect (fireball impact)
    Explosion { radius: i32 },
    /// Potion splash effect
    PotionSplash { potion_type: crate::components::ItemType },
}

/// Duration for alert indicator
const ALERT_DURATION: f32 = 0.8;
/// Duration for explosion effect
const EXPLOSION_DURATION: f32 = 0.5;
/// Duration for potion splash effect
const POTION_SPLASH_DURATION: f32 = 0.4;

impl VfxType {
    pub fn duration(&self) -> f32 {
        match self {
            VfxType::Slash { .. } => SLASH_VFX_DURATION,
            VfxType::DamageNumber { .. } => DAMAGE_NUMBER_DURATION,
            VfxType::HealNumber { .. } => DAMAGE_NUMBER_DURATION, // Same duration as damage
            VfxType::Fire { .. } => f32::INFINITY, // Fire loops forever
            VfxType::Alert => ALERT_DURATION,
            VfxType::Explosion { .. } => EXPLOSION_DURATION,
            VfxType::PotionSplash { .. } => POTION_SPLASH_DURATION,
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

/// A persistent life drain beam effect (caster to target connection)
pub struct LifeDrainBeam {
    pub caster: hecs::Entity,
    pub target: hecs::Entity,
    pub time: f32, // Accumulated time for animation
}

impl LifeDrainBeam {
    pub fn new(caster: hecs::Entity, target: hecs::Entity) -> Self {
        Self { caster, target, time: 0.0 }
    }

    pub fn update(&mut self, dt: f32) {
        self.time += dt;
    }
}

/// Manager for all active visual effects
pub struct VfxManager {
    pub effects: Vec<VisualEffect>,
    pub fires: Vec<FireEffect>,
    pub life_drain_beams: Vec<LifeDrainBeam>,
}

impl VfxManager {
    pub fn new() -> Self {
        Self {
            effects: Vec::new(),
            fires: Vec::new(),
            life_drain_beams: Vec::new(),
        }
    }

    /// Spawn a new effect
    pub fn spawn(&mut self, x: f32, y: f32, effect_type: VfxType) {
        self.effects.push(VisualEffect::new(x, y, effect_type));
    }

    /// Spawn a slash effect at target position
    pub fn spawn_slash(&mut self, x: f32, y: f32) {
        self.spawn(x, y, VfxType::Slash { angle: SLASH_VFX_ANGLE });
    }

    /// Spawn a floating damage number
    pub fn spawn_damage_number(&mut self, x: f32, y: f32, amount: i32) {
        self.spawn(x, y, VfxType::DamageNumber { amount });
    }

    /// Spawn an alert indicator "!" above an entity
    pub fn spawn_alert(&mut self, x: f32, y: f32) {
        self.spawn(x, y, VfxType::Alert);
    }

    /// Spawn an explosion effect (fireball)
    pub fn spawn_explosion(&mut self, x: f32, y: f32, radius: i32) {
        self.spawn(x, y, VfxType::Explosion { radius });
    }

    /// Spawn a potion splash effect
    pub fn spawn_potion_splash(&mut self, x: f32, y: f32, potion_type: crate::components::ItemType) {
        self.spawn(x, y, VfxType::PotionSplash { potion_type });
    }

    /// Spawn a persistent fire effect
    pub fn spawn_fire(&mut self, x: f32, y: f32) {
        let seed = rand::random::<f32>() * 1000.0;
        self.fires.push(FireEffect::new(x, y, seed));
    }

    /// Start a life drain beam effect between caster and target
    pub fn start_life_drain_beam(&mut self, caster: hecs::Entity, target: hecs::Entity) {
        // Remove any existing beam from this caster first
        self.life_drain_beams.retain(|b| b.caster != caster);
        self.life_drain_beams.push(LifeDrainBeam::new(caster, target));
    }

    /// Stop a life drain beam for the given caster
    pub fn stop_life_drain_beam(&mut self, caster: hecs::Entity) {
        self.life_drain_beams.retain(|b| b.caster != caster);
    }

    /// Update all effects, removing finished ones
    pub fn update(&mut self, dt: f32) {
        self.effects.retain_mut(|effect| effect.update(dt));
        // Update fire animation times
        for fire in &mut self.fires {
            fire.update(dt);
        }
        // Update life drain beam animation times
        for beam in &mut self.life_drain_beams {
            beam.update(dt);
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
            GameEvent::FireballExplosion { x, y, radius } => {
                // Spawn explosion at center
                if grid.get(*x, *y).map(|t| t.visible).unwrap_or(false) {
                    self.spawn_explosion(*x as f32 + 0.5, *y as f32 + 0.5, *radius);
                }
            }
            GameEvent::PotionSplash { x, y, potion_type } => {
                // Spawn potion splash at impact location
                if grid.get(*x, *y).map(|t| t.visible).unwrap_or(false) {
                    self.spawn_potion_splash(*x as f32 + 0.5, *y as f32 + 0.5, *potion_type);
                }
            }
            GameEvent::CleavePerformed { center } => {
                // Spawn slashes on all tiles within radius 2 (5x5 area)
                for dx in -2..=2 {
                    for dy in -2..=2 {
                        if dx == 0 && dy == 0 {
                            continue; // Skip center
                        }
                        let tx = center.0 + dx;
                        let ty = center.1 + dy;
                        if grid.get(tx, ty).map(|t| t.visible).unwrap_or(false) {
                            self.spawn_slash(tx as f32 + 0.5, ty as f32 + 0.5);
                        }
                    }
                }
            }
            GameEvent::BurnDamage { position, damage, .. } => {
                // Show damage number for burn damage
                let tile_x = position.0 as i32;
                let tile_y = position.1 as i32;
                if grid.get(tile_x, tile_y).map(|t| t.visible).unwrap_or(false) {
                    self.spawn_damage_number(position.0, position.1, *damage);
                }
            }
            GameEvent::LifeDrainStarted { caster, target } => {
                // Start the life drain beam visual
                self.start_life_drain_beam(*caster, *target);
            }
            GameEvent::LifeDrainEnded { caster, .. } | GameEvent::LifeDrainInterrupted { caster, .. } => {
                // Stop the life drain beam visual
                self.stop_life_drain_beam(*caster);
            }
            GameEvent::LifeDrainTick { target_pos, caster_pos, damage, healed, .. } => {
                // Show damage number on target
                let tile_x = target_pos.0 as i32;
                let tile_y = target_pos.1 as i32;
                if grid.get(tile_x, tile_y).map(|t| t.visible).unwrap_or(false) {
                    self.spawn_damage_number(target_pos.0, target_pos.1, *damage);
                }
                // Show heal number on caster (green/positive)
                let caster_tile_x = caster_pos.0 as i32;
                let caster_tile_y = caster_pos.1 as i32;
                if grid.get(caster_tile_x, caster_tile_y).map(|t| t.visible).unwrap_or(false) {
                    // Spawn heal number (using negative to indicate healing)
                    self.spawn(caster_pos.0, caster_pos.1, VfxType::HealNumber { amount: *healed });
                }
            }
            _ => {}
        }
    }
}
