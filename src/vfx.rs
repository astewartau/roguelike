//! Visual effects system for one-shot animations (slashes, particles, etc.)
//!
//! These are separate from entity state - they're spawned, animated, and removed
//! without affecting game logic.

use crate::camera::ShakeRequest;
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

/// How a floating damage number should read.
///
/// Damage taken and damage dealt are the same event to the simulation and
/// completely different news to the player, so they do not get to look alike.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum DamageTier {
    /// An ordinary hit the player landed.
    Dealt,
    /// A heavy hit the player landed.
    Big,
    /// A critical hit the player landed.
    Crit,
    /// Damage the player took.
    Taken,
}

/// Pick a tier from who took the hit and how hard it was.
fn damage_tier(
    victim: Option<hecs::Entity>,
    player: hecs::Entity,
    damage: i32,
    crit: bool,
) -> DamageTier {
    if victim == Some(player) {
        DamageTier::Taken
    } else if crit {
        DamageTier::Crit
    } else if damage >= DAMAGE_NUMBER_BIG_THRESHOLD {
        DamageTier::Big
    } else {
        DamageTier::Dealt
    }
}

#[derive(Clone)]
pub enum VfxType {
    /// Diagonal slash mark (for melee hits)
    Slash { angle: f32 },
    /// Floating damage number
    DamageNumber {
        amount: i32,
        tier: DamageTier,
        /// Sideways offset in tiles, rolled per instance so several hits on
        /// one tile do not stack into an unreadable pile.
        jitter: f32,
    },
    /// Floating heal number (green, positive)
    HealNumber { amount: i32 },
    /// Floating word over a swing that did not land as a hit: "miss" for a
    /// target that stepped out of reach, "BLOCK" for a guarded blow, "WARD"
    /// for a hit a bone ward swallowed. Drawn in the damage-number style;
    /// `jitter` as for `DamageNumber`.
    MissText { jitter: f32, label: &'static str },
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
            VfxType::MissText { .. } => DAMAGE_NUMBER_DURATION,
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

/// A persistent taming channel effect (tamer to target connection)
pub struct TamingBeam {
    pub tamer: hecs::Entity,
    pub target: hecs::Entity,
    pub time: f32, // Accumulated time for animation
}

impl TamingBeam {
    pub fn new(tamer: hecs::Entity, target: hecs::Entity) -> Self {
        Self { tamer, target, time: 0.0 }
    }

    pub fn update(&mut self, dt: f32) {
        self.time += dt;
    }
}

/// A persistent "Zzz" bubble shown above the player while resting.
pub struct RestingBubble {
    pub x: f32,
    pub y: f32,
    pub time: f32, // Accumulated time for the bob animation
}

/// Manager for all active visual effects
pub struct VfxManager {
    pub effects: Vec<VisualEffect>,
    pub fires: Vec<FireEffect>,
    pub life_drain_beams: Vec<LifeDrainBeam>,
    pub taming_beams: Vec<TamingBeam>,
    /// Persistent resting indicator, present only while the player is resting.
    pub resting_bubble: Option<RestingBubble>,
    /// Camera shakes queued by events since the last frame, waiting to be
    /// handed to the camera. They live here because event handling runs deep
    /// in the simulation, where the camera is not reachable, while the engine
    /// tick has both this and the camera in hand.
    shake_requests: Vec<ShakeRequest>,
    /// Hit-stop timer, fed by events alongside the rest of the vfx and read
    /// by the engine tick to slow the animation clock (`systems::hitstop`).
    pub hitstop: crate::systems::hitstop::HitStop,
}

impl VfxManager {
    pub fn new() -> Self {
        Self {
            effects: Vec::new(),
            fires: Vec::new(),
            life_drain_beams: Vec::new(),
            taming_beams: Vec::new(),
            resting_bubble: None,
            shake_requests: Vec::new(),
            hitstop: crate::systems::hitstop::HitStop::new(),
        }
    }

    /// Queue a camera shake for the engine to pass on this frame.
    pub fn request_shake(&mut self, request: ShakeRequest) {
        self.shake_requests.push(request);
    }

    /// Take everything queued since the last call. The engine tick drains this
    /// every frame, so requests never pile up; a frame with no events hands
    /// back an empty vec and the camera's own decay carries on untouched.
    pub fn take_shake_requests(&mut self) -> Vec<ShakeRequest> {
        std::mem::take(&mut self.shake_requests)
    }

    /// Spawn a new effect
    pub fn spawn(&mut self, x: f32, y: f32, effect_type: VfxType) {
        self.effects.push(VisualEffect::new(x, y, effect_type));
    }

    /// Spawn a slash effect at target position
    pub fn spawn_slash(&mut self, x: f32, y: f32) {
        self.spawn(x, y, VfxType::Slash { angle: SLASH_VFX_ANGLE });
    }

    /// Spawn a floating damage number.
    ///
    /// The jitter is rolled from the thread RNG, not the seeded game RNG:
    /// where a number happens to sit is presentation, and drawing from the
    /// game RNG here would make the simulation depend on how many hits were
    /// on screen. `spawn_fire` does the same for the same reason.
    pub fn spawn_damage_number(&mut self, x: f32, y: f32, amount: i32, tier: DamageTier) {
        let jitter = (rand::random::<f32>() * 2.0 - 1.0) * DAMAGE_NUMBER_JITTER;
        self.spawn(x, y, VfxType::DamageNumber { amount, tier, jitter });
    }

    /// Spawn a floating "miss" (a melee swing that found nobody in reach).
    /// Jitter comes from the thread RNG for the same reason as damage numbers.
    pub fn spawn_miss_text(&mut self, x: f32, y: f32) {
        self.spawn_float_text(x, y, "miss");
    }

    /// Spawn a floating word in the miss-text style ("BLOCK", "WARD").
    pub fn spawn_float_text(&mut self, x: f32, y: f32, label: &'static str) {
        let jitter = (rand::random::<f32>() * 2.0 - 1.0) * DAMAGE_NUMBER_JITTER;
        self.spawn(x, y, VfxType::MissText { jitter, label });
    }

    /// Spawn an alert indicator "!" above an entity
    pub fn spawn_alert(&mut self, x: f32, y: f32) {
        self.spawn(x, y, VfxType::Alert);
    }

    /// Show (or move) the persistent resting "Zzz" bubble above a position.
    pub fn set_resting_bubble(&mut self, x: f32, y: f32) {
        match &mut self.resting_bubble {
            Some(b) => {
                b.x = x;
                b.y = y;
            }
            None => self.resting_bubble = Some(RestingBubble { x, y, time: 0.0 }),
        }
    }

    /// Hide the resting bubble (rest ended).
    pub fn clear_resting_bubble(&mut self) {
        self.resting_bubble = None;
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

    /// Start a taming channel effect between tamer and target
    pub fn start_taming_beam(&mut self, tamer: hecs::Entity, target: hecs::Entity) {
        // Remove any existing beam from this tamer first
        self.taming_beams.retain(|b| b.tamer != tamer);
        self.taming_beams.push(TamingBeam::new(tamer, target));
    }

    /// Stop a taming channel effect for the given tamer
    pub fn stop_taming_beam(&mut self, tamer: hecs::Entity) {
        self.taming_beams.retain(|b| b.tamer != tamer);
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
        // Update taming beam animation times
        for beam in &mut self.taming_beams {
            beam.update(dt);
        }
        // Advance the resting bubble's bob animation
        if let Some(bubble) = &mut self.resting_bubble {
            bubble.time += dt;
        }
    }

    /// Handle a game event, spawning appropriate VFX.
    /// Only spawns VFX for positions visible to the player (not in fog of war).
    pub fn handle_event(&mut self, event: &GameEvent, grid: &Grid, player: hecs::Entity) {
        match event {
            GameEvent::AttackHit { target, target_pos, damage, crit, flanked, .. } => {
                // Only show VFX if the position is visible to the player
                let tile_x = target_pos.0 as i32;
                let tile_y = target_pos.1 as i32;
                if grid.get(tile_x, tile_y).map(|t| t.visible).unwrap_or(false) {
                    self.spawn_slash(target_pos.0, target_pos.1);
                    self.spawn_damage_number(
                        target_pos.0,
                        target_pos.1,
                        *damage,
                        damage_tier(Some(*target), player, *damage, *crit),
                    );
                    // A flanked hit says so beside its number.
                    if *flanked {
                        self.spawn_float_text(target_pos.0, target_pos.1, "FLANK");
                    }
                }
            }
            GameEvent::ChargeMissed { position, outcome: crate::events::ChargeOutcome::Wall, .. } => {
                self.spawn_float_text(position.0 as f32 + 0.5, position.1 as f32 + 0.5, "SLAM");
            }
            GameEvent::AttackMissed {
                target_pos: Some(target_pos),
                reason: crate::events::MissReason::OutOfReach,
                ..
            } if grid
                .get(target_pos.0 as i32, target_pos.1 as i32)
                .map(|t| t.visible)
                .unwrap_or(false) =>
            {
                self.spawn_miss_text(target_pos.0, target_pos.1);
            }
            GameEvent::AttackBlocked { defender_pos, .. }
                if grid
                    .get(defender_pos.0 as i32, defender_pos.1 as i32)
                    .map(|t| t.visible)
                    .unwrap_or(false) =>
            {
                self.spawn_float_text(defender_pos.0, defender_pos.1, "BLOCK");
            }
            GameEvent::BoneWardAbsorbed { position, .. }
                if grid
                    .get(position.0 as i32, position.1 as i32)
                    .map(|t| t.visible)
                    .unwrap_or(false) =>
            {
                self.spawn_float_text(position.0, position.1, "WARD");
            }
            // Vines burst out of every tile of the patch: the green potion
            // splash, reused rather than a new effect.
            GameEvent::EntangleCast { tiles, .. } => {
                for &(x, y) in tiles {
                    if grid.get(x, y).map(|t| t.visible).unwrap_or(false) {
                        self.spawn_potion_splash(
                            x as f32 + 0.5,
                            y as f32 + 0.5,
                            crate::components::ItemType::RegenerationPotion,
                        );
                    }
                }
            }
            GameEvent::ProjectileHit { position, damage, target, .. }
                // Only show damage number if we hit an enemy (not a wall) AND position is visible
                if target.is_some()
                    && grid.get(position.0, position.1).map(|t| t.visible).unwrap_or(false) => {
                        self.spawn_damage_number(
                            position.0 as f32,
                            position.1 as f32,
                            *damage,
                            damage_tier(*target, player, *damage, false),
                        );
                    }
            GameEvent::EntityDied { position, .. } => {
                // Could spawn death particles here in the future
                let _ = position;
            }
            // Spawn explosion at center
            GameEvent::FireballExplosion { x, y, radius }
                if grid.get(*x, *y).map(|t| t.visible).unwrap_or(false) =>
            {
                self.spawn_explosion(*x as f32 + 0.5, *y as f32 + 0.5, *radius);
            }
            // Spawn potion splash at impact location
            GameEvent::PotionSplash { x, y, potion_type }
                if grid.get(*x, *y).map(|t| t.visible).unwrap_or(false) =>
            {
                self.spawn_potion_splash(*x as f32 + 0.5, *y as f32 + 0.5, *potion_type);
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
            GameEvent::DotDamage { entity, position, damage, .. }
                if grid
                    .get(position.0 as i32, position.1 as i32)
                    .map(|t| t.visible)
                    .unwrap_or(false) =>
            {
                // Poison and bleed ticks float a number like burn damage does.
                self.spawn_damage_number(
                    position.0,
                    position.1,
                    *damage,
                    damage_tier(Some(*entity), player, *damage, false),
                );
            }
            // Rain falls on every tile of the patch: the water-flask splash,
            // reused rather than a new effect.
            GameEvent::RainCalled { tiles, .. } => {
                for &(x, y) in tiles {
                    if grid.get(x, y).map(|t| t.visible).unwrap_or(false) {
                        self.spawn_potion_splash(
                            x as f32 + 0.5,
                            y as f32 + 0.5,
                            crate::components::ItemType::WaterFlaskFull,
                        );
                    }
                }
            }
            GameEvent::BurnDamage { entity, position, damage } => {
                // Show damage number for burn damage
                let tile_x = position.0 as i32;
                let tile_y = position.1 as i32;
                if grid.get(tile_x, tile_y).map(|t| t.visible).unwrap_or(false) {
                    self.spawn_damage_number(
                        position.0,
                        position.1,
                        *damage,
                        damage_tier(Some(*entity), player, *damage, false),
                    );
                }
            }
            GameEvent::StarvationDamage { position, damage, .. } => {
                // Starvation damage floats a number like burn damage does
                // (it's always the player, so always on a visible tile).
                self.spawn_damage_number(position.0, position.1, *damage, DamageTier::Taken);
            }
            GameEvent::TamingStarted { tamer, target } => {
                // Start the taming channel visual
                self.start_taming_beam(*tamer, *target);
            }
            GameEvent::TamingCompleted { tamer, .. } | GameEvent::TamingFailed { tamer, .. } => {
                // Stop the taming channel visual
                self.stop_taming_beam(*tamer);
            }
            GameEvent::LifeDrainStarted { caster, target } => {
                // Start the life drain beam visual
                self.start_life_drain_beam(*caster, *target);
            }
            GameEvent::LifeDrainEnded { caster, .. } | GameEvent::LifeDrainInterrupted { caster, .. } => {
                // Stop the life drain beam visual
                self.stop_life_drain_beam(*caster);
            }
            // Green heal number over the mended ally (event is only
            // emitted when the tile is visible, but double-check).
            GameEvent::EnemyHealed { amount, position, .. }
                if grid.get(position.0, position.1).map(|t| t.visible).unwrap_or(false) =>
            {
                self.spawn(
                    position.0 as f32 + 0.5,
                    position.1 as f32 + 0.5,
                    VfxType::HealNumber { amount: *amount },
                );
            }
            GameEvent::BossAbilityUsed { ability, position, .. }
                // Gnash's slam gets an explosion ring; other boss casts read
                // through their spawned effects (spiders, skeletons).
                if *ability == crate::components::BossAbility::GroundSlam
                    && grid.get(position.0, position.1).map(|t| t.visible).unwrap_or(false)
                => {
                    self.spawn_explosion(
                        position.0 as f32 + 0.5,
                        position.1 as f32 + 0.5,
                        crate::constants::BOSS_SLAM_RADIUS,
                    );
                }
            GameEvent::LifeDrainTick { target, target_pos, caster_pos, damage, healed, .. } => {
                // Show damage number on target
                let tile_x = target_pos.0 as i32;
                let tile_y = target_pos.1 as i32;
                if grid.get(tile_x, tile_y).map(|t| t.visible).unwrap_or(false) {
                    self.spawn_damage_number(
                        target_pos.0,
                        target_pos.1,
                        *damage,
                        damage_tier(Some(*target), player, *damage, false),
                    );
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
