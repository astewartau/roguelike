//! Status effect application systems.
//!
//! This module handles applying status effects to entities.
//! Functions here operate on StatusEffects components directly (pure ECS pattern).

use std::collections::HashSet;

use hecs::{Entity, World};

use crate::components::{ActiveEffect, ChaseAI, EffectType, Position, StatusEffects};
use crate::fov::Fov;
use crate::grid::Grid;

// =============================================================================
// PURE STATUS EFFECT FUNCTIONS (operate on component data)
// =============================================================================

/// Check if a StatusEffects component has a specific effect active
pub fn has_effect(effects: &StatusEffects, effect_type: EffectType) -> bool {
    effects.effects.iter().any(|e| e.effect_type == effect_type)
}

/// Add or refresh an effect with the given duration
pub fn add_effect(effects: &mut StatusEffects, effect_type: EffectType, duration: f32) {
    if let Some(existing) = effects.effects.iter_mut().find(|e| e.effect_type == effect_type) {
        existing.remaining_duration = duration;
        // A refresh restarts the HUD sweep rather than leaving it part-spent.
        existing.total_duration = duration;
    } else {
        effects.effects.push(ActiveEffect {
            effect_type,
            remaining_duration: duration,
            total_duration: duration,
            last_damage_tick: 0.0, // First damage tick happens immediately
        });
    }
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

/// Add or refresh an effect on an entity. Returns whether it took.
///
/// This is the chokepoint every status goes through, so the rules about which
/// effects exclude each other live here rather than at each source:
///
/// - Fear never sticks to `FearImmune` entities (bosses).
/// - Nothing Wet can catch fire.
/// - Becoming Wet puts out Burning and washes off Oily; becoming Oily dries
///   you off. Whichever coating came last wins.
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
    let Ok(mut effects) = world.get::<&mut StatusEffects>(entity) else {
        return false;
    };
    match effect_type {
        EffectType::Burning if has_effect(&effects, EffectType::Wet) => return false,
        EffectType::Wet => {
            remove_effect(&mut effects, EffectType::Burning);
            remove_effect(&mut effects, EffectType::Oily);
        }
        EffectType::Oily => remove_effect(&mut effects, EffectType::Wet),
        _ => {}
    }
    add_effect(&mut effects, effect_type, duration);
    true
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

    #[test]
    fn test_wet_blocks_burning_and_coatings_replace_each_other() {
        let mut world = World::new();
        let e = world.spawn((StatusEffects::new(),));

        // Burning, then soaked: the water puts the fire out.
        assert!(add_effect_to_entity(&mut world, e, EffectType::Burning, 5.0));
        assert!(add_effect_to_entity(&mut world, e, EffectType::Wet, 5.0));
        assert!(!entity_has_effect(&world, e, EffectType::Burning), "water douses");

        // While wet, fire doesn't take.
        assert!(!add_effect_to_entity(&mut world, e, EffectType::Burning, 5.0));
        assert!(!entity_has_effect(&world, e, EffectType::Burning), "wet things don't burn");

        // Oil over water: the newer coating wins, and then fire takes again.
        add_effect_to_entity(&mut world, e, EffectType::Oily, 5.0);
        assert!(!entity_has_effect(&world, e, EffectType::Wet));
        assert!(add_effect_to_entity(&mut world, e, EffectType::Burning, 5.0));

        // And water over oil washes it off.
        add_effect_to_entity(&mut world, e, EffectType::Wet, 5.0);
        assert!(!entity_has_effect(&world, e, EffectType::Oily));
    }
}
