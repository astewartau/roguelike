//! Status effect application systems.
//!
//! This module handles applying status effects to entities.
//! Functions here operate on StatusEffects components directly (pure ECS pattern).

use std::collections::HashSet;

use hecs::{Entity, World};

use crate::components::{ActiveEffect, ChaseAI, EffectType, Position, StatusEffects};
use crate::fov::FOV;
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
    } else {
        effects.effects.push(ActiveEffect {
            effect_type,
            remaining_duration: duration,
        });
    }
}

/// Get remaining duration of an effect (None if not active)
pub fn get_effect_duration(effects: &StatusEffects, effect_type: EffectType) -> Option<f32> {
    effects
        .effects
        .iter()
        .find(|e| e.effect_type == effect_type)
        .map(|e| e.remaining_duration)
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

/// Add or refresh an effect on an entity
pub fn add_effect_to_entity(
    world: &mut World,
    entity: Entity,
    effect_type: EffectType,
    duration: f32,
) {
    if let Ok(mut effects) = world.get::<&mut StatusEffects>(entity) {
        add_effect(&mut effects, effect_type, duration);
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
    let visible_tiles: HashSet<(i32, i32)> = FOV::calculate(
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
