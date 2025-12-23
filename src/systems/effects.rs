//! Status effect application systems.
//!
//! This module handles applying status effects to groups of entities.

use std::collections::HashSet;

use hecs::{Entity, World};

use crate::components::{ChaseAI, EffectType, Position, StatusEffects};
use crate::fov::FOV;
use crate::grid::Grid;

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
        if let Ok(mut effects) = world.get::<&mut StatusEffects>(entity) {
            effects.add_effect(effect, duration);
        }
    }
}
