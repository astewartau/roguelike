//! Status effect application systems.
//!
//! This module handles applying status effects to entities.
//! Functions here operate on StatusEffects components directly (pure ECS pattern).

use std::collections::HashSet;

use hecs::{Entity, World};

use crate::components::{ActiveEffect, ChaseAI, EffectType, Position, StatusEffects};
use crate::events::{EventQueue, GameEvent};
use crate::fov::Fov;
use crate::grid::Grid;

// =============================================================================
// PURE STATUS EFFECT FUNCTIONS (operate on component data)
// =============================================================================

/// Check if a StatusEffects component has a specific effect active
pub fn has_effect(effects: &StatusEffects, effect_type: EffectType) -> bool {
    effects.effects.iter().any(|e| e.effect_type == effect_type)
}

/// Add or refresh an effect with the given duration. Returns whether the
/// effect is active afterwards (false when an interaction refused it).
///
/// **Refresh keeps the longer timer.** Re-applying an effect that is already
/// running sets its remainder to `max(remaining, duration)`, so a short
/// re-application (a 0.5s stagger on a 5s stun, a 3s lesser-spider bite on a
/// 6s poison) never cuts an effect short. `total_duration` follows whichever
/// timer won, so the HUD sweep restarts only when the effect was extended.
/// Code that genuinely wants to end an effect early must say so with
/// [`remove_effect`] (Guard does, when its action completes). No caller
/// relies on a re-application shortening an effect.
///
/// Surface-status interactions live here so every source obeys them:
/// - **Wet** blocks Burning (nothing is added) and, when gained, puts out
///   Burning and washes off Oiled. Wet also blocks gaining Oiled.
/// - **Burning** on an Oiled creature caps the Oiled timer at the burn's
///   remaining time: the oil burns off with the fire (and boosts burn damage
///   until then — see `time_system::tick_burn_damage`).
pub fn add_effect(effects: &mut StatusEffects, effect_type: EffectType, duration: f32) -> bool {
    match effect_type {
        EffectType::Burning | EffectType::Oiled if has_effect(effects, EffectType::Wet) => {
            return false;
        }
        EffectType::Wet => {
            remove_effect(effects, EffectType::Burning);
            remove_effect(effects, EffectType::Oiled);
        }
        _ => {}
    }

    if let Some(existing) = effects.effects.iter_mut().find(|e| e.effect_type == effect_type) {
        if duration > existing.remaining_duration {
            existing.remaining_duration = duration;
            // An extension restarts the HUD sweep; a shorter re-application
            // leaves both the timer and the sweep alone.
            existing.total_duration = duration;
        }
    } else {
        effects.effects.push(ActiveEffect {
            effect_type,
            remaining_duration: duration,
            total_duration: duration,
            last_damage_tick: 0.0, // First damage tick happens immediately
        });
    }

    if effect_type == EffectType::Burning {
        let burn_left = effects
            .effects
            .iter()
            .find(|e| e.effect_type == EffectType::Burning)
            .map(|e| e.remaining_duration)
            .unwrap_or(duration);
        if let Some(oil) = effects.effects.iter_mut().find(|e| e.effect_type == EffectType::Oiled) {
            oil.remaining_duration = oil.remaining_duration.min(burn_left);
        }
    }
    true
}

/// Remove an effect from a StatusEffects component
pub fn remove_effect(effects: &mut StatusEffects, effect_type: EffectType) {
    effects.effects.retain(|e| e.effect_type != effect_type);
}

// =============================================================================
// ENTITY-LEVEL HELPERS (operate on World)
// =============================================================================

/// Check if an entity has a specific status effect active
pub fn entity_has_effect(world: &World, entity: Entity, effect_type: EffectType) -> bool {
    world
        .get::<&StatusEffects>(entity)
        .map(|e| has_effect(&e, effect_type))
        .unwrap_or(false)
}

/// Add or refresh an effect on an entity (see [`add_effect`] for refresh and
/// interaction rules). Returns whether the effect is active afterwards: false
/// for entities without `StatusEffects`, for Fear on `FearImmune` entities
/// (bosses), and for Burning/Oiled refused by Wet. Callers that announce the
/// effect (`CaughtFire`) should only do so on `true`.
pub fn add_effect_to_entity(
    world: &mut World,
    entity: Entity,
    effect_type: EffectType,
    duration: f32,
) -> bool {
    if effect_type == EffectType::Feared
        && world.get::<&crate::components::FearImmune>(entity).is_ok()
    {
        return false;
    }
    if let Ok(mut effects) = world.get::<&mut StatusEffects>(entity) {
        add_effect(&mut effects, effect_type, duration)
    } else {
        false
    }
}

/// Add or refresh an effect and, if the entity did not already have it, emit
/// [`GameEvent::StatusEffectGained`] (message log lines, "You are soaked.").
/// A refresh is silent, so standing in water does not spam the log. Returns
/// whether the effect is active afterwards.
pub fn add_effect_announced(
    world: &mut World,
    events: &mut EventQueue,
    entity: Entity,
    effect_type: EffectType,
    duration: f32,
) -> bool {
    let had = entity_has_effect(world, entity, effect_type);
    let active = add_effect_to_entity(world, entity, effect_type, duration);
    if active && !had {
        events.push(GameEvent::StatusEffectGained { entity, effect: effect_type });
    }
    active
}

/// Remove an effect from an entity
pub fn remove_effect_from_entity(world: &mut World, entity: Entity, effect_type: EffectType) {
    if let Ok(mut effects) = world.get::<&mut StatusEffects>(entity) {
        remove_effect(&mut effects, effect_type);
    }
}

// =============================================================================
// BATCH EFFECT APPLICATION
// =============================================================================

/// Apply an effect to all enemies visible from a position.
/// Uses FOV calculation to determine which enemies can be seen.
pub fn apply_effect_to_visible_enemies(
    world: &mut World,
    grid: &Grid,
    caster_pos: (i32, i32),
    fov_radius: i32,
    effect: EffectType,
    duration: f32,
) {
    // Calculate visible tiles from caster's perspective
    let visible_tiles: HashSet<(i32, i32)> = Fov::calculate(
        grid,
        caster_pos.0,
        caster_pos.1,
        fov_radius,
        None::<fn(i32, i32) -> bool>,
    ).into_iter().collect();

    // Find all enemies in visible tiles and apply effect
    let enemies_to_affect: Vec<Entity> = world
        .query::<(&Position, &ChaseAI)>()
        .iter()
        .filter(|(_, (pos, _))| visible_tiles.contains(&(pos.x, pos.y)))
        .map(|(entity, _)| entity)
        .collect();

    for entity in enemies_to_affect {
        add_effect_to_entity(world, entity, effect, duration);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::components::FearImmune;
    use crate::constants::{OILED_DURATION, WET_DURATION};

    fn remaining(s: &StatusEffects, t: EffectType) -> Option<(f32, f32)> {
        s.effects
            .iter()
            .find(|e| e.effect_type == t)
            .map(|e| (e.remaining_duration, e.total_duration))
    }

    /// Re-applying keeps the longer timer: a short refresh never cuts an
    /// effect, a long one extends it, and the HUD total follows the winner.
    #[test]
    fn refresh_keeps_the_longer_remaining_duration() {
        let mut s = StatusEffects::new();
        add_effect(&mut s, EffectType::Stunned, 5.0);
        s.effects[0].remaining_duration = 4.0; // 1s has passed

        add_effect(&mut s, EffectType::Stunned, 0.5);
        assert_eq!(
            remaining(&s, EffectType::Stunned),
            Some((4.0, 5.0)),
            "a shorter re-application leaves timer and sweep alone"
        );

        add_effect(&mut s, EffectType::Stunned, 7.0);
        assert_eq!(
            remaining(&s, EffectType::Stunned),
            Some((7.0, 7.0)),
            "a longer one extends and restarts the sweep"
        );
        assert_eq!(s.effects.len(), 1, "refresh never duplicates");
    }

    /// Wet and Oiled exclude each other, Wet wins, and Wet puts out fire.
    #[test]
    fn wet_douses_burning_washes_oil_and_refuses_both() {
        let mut s = StatusEffects::new();
        assert!(add_effect(&mut s, EffectType::Oiled, OILED_DURATION));
        assert!(add_effect(&mut s, EffectType::Burning, 3.0));

        assert!(add_effect(&mut s, EffectType::Wet, WET_DURATION));
        assert!(has_effect(&s, EffectType::Wet));
        assert!(!has_effect(&s, EffectType::Burning), "gaining Wet puts out Burning");
        assert!(!has_effect(&s, EffectType::Oiled), "gaining Wet washes off Oiled");

        assert!(!add_effect(&mut s, EffectType::Burning, 10.0), "Wet refuses Burning");
        assert!(!add_effect(&mut s, EffectType::Oiled, 10.0), "Wet refuses Oiled");
        assert!(!has_effect(&s, EffectType::Burning) && !has_effect(&s, EffectType::Oiled));
    }

    /// Catching fire while Oiled caps the oil at the burn: it burns off with
    /// the fire rather than outliving it.
    #[test]
    fn burning_caps_oiled_at_the_burn_duration() {
        let mut s = StatusEffects::new();
        add_effect(&mut s, EffectType::Oiled, OILED_DURATION);
        add_effect(&mut s, EffectType::Burning, 4.0);
        assert_eq!(remaining(&s, EffectType::Oiled).map(|r| r.0), Some(4.0));
    }

    /// Only a fresh gain is announced; refreshes (standing in water) are not.
    #[test]
    fn announced_effects_fire_once_per_gain() {
        let mut world = World::new();
        let e = world.spawn((StatusEffects::new(),));
        let mut events = EventQueue::new();
        add_effect_announced(&mut world, &mut events, e, EffectType::Wet, WET_DURATION);
        add_effect_announced(&mut world, &mut events, e, EffectType::Wet, WET_DURATION);
        let gains = events
            .drain()
            .filter(|ev| matches!(ev, GameEvent::StatusEffectGained { effect: EffectType::Wet, .. }))
            .count();
        assert_eq!(gains, 1);
    }

    #[test]
    fn test_fear_immune_blocks_feared_but_not_other_effects() {
        let mut world = World::new();
        let boss = world.spawn((StatusEffects::new(), FearImmune));

        add_effect_to_entity(&mut world, boss, EffectType::Feared, 5.0);
        assert!(
            !entity_has_effect(&world, boss, EffectType::Feared),
            "FearImmune entities can never be Feared"
        );

        add_effect_to_entity(&mut world, boss, EffectType::Slowed, 5.0);
        assert!(
            entity_has_effect(&world, boss, EffectType::Slowed),
            "other effects still apply to FearImmune entities"
        );
    }
}
