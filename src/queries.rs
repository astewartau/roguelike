//! Common entity query helpers.
//!
//! This module provides reusable query functions to reduce code repetition
//! across systems. These are pure read-only queries that don't modify state.

use std::collections::HashSet;

use hecs::{Entity, World};

use crate::components::{
    Actor, Attackable, BlocksMovement, BlocksVision, EffectType, Health, Position,
};
use crate::systems::effects;

/// Get all positions blocked by entities (for pathfinding/collision).
/// Optionally excludes a specific entity (usually the moving entity itself).
pub fn get_blocking_positions(world: &World, exclude: Option<Entity>) -> HashSet<(i32, i32)> {
    world
        .query::<(&Position, &BlocksMovement)>()
        .iter()
        .filter(|(id, _)| exclude.map_or(true, |ex| *id != ex))
        .map(|(_, (pos, _))| (pos.x, pos.y))
        .collect()
}

/// Get all positions that block vision (for FOV calculations).
pub fn get_vision_blocking_positions(world: &World) -> HashSet<(i32, i32)> {
    world
        .query::<(&Position, &BlocksVision)>()
        .iter()
        .map(|(_, (pos, _))| (pos.x, pos.y))
        .collect()
}

/// Find an attackable entity at a specific position.
/// Optionally excludes a specific entity (usually the attacker).
pub fn get_attackable_at(
    world: &World,
    x: i32,
    y: i32,
    exclude: Option<Entity>,
) -> Option<Entity> {
    world
        .query::<(&Position, &Attackable)>()
        .iter()
        .find(|(id, (pos, _))| {
            pos.x == x && pos.y == y && exclude.map_or(true, |ex| *id != ex)
        })
        .map(|(id, _)| id)
}

/// Check if an entity has a specific status effect active.
pub fn has_status_effect(world: &World, entity: Entity, effect: EffectType) -> bool {
    effects::entity_has_effect(world, entity, effect)
}

/// Check if an entity can perform an action (has energy and is not busy).
pub fn can_entity_act(world: &World, entity: Entity) -> bool {
    world
        .get::<&Actor>(entity)
        .map(|a| a.can_act())
        .unwrap_or(false)
}

/// Check if an entity is dead (health <= 0).
pub fn is_entity_dead(world: &World, entity: Entity) -> bool {
    world
        .get::<&Health>(entity)
        .map(|h| h.is_dead())
        .unwrap_or(true)
}

/// Get an entity's logical position as a tuple.
pub fn get_entity_position(world: &World, entity: Entity) -> Option<(i32, i32)> {
    world.get::<&Position>(entity).ok().map(|p| (p.x, p.y))
}

/// Check if a position is blocked by any entity (excluding a specific one).
pub fn is_position_blocked(world: &World, x: i32, y: i32, exclude: Option<Entity>) -> bool {
    world
        .query::<(&Position, &BlocksMovement)>()
        .iter()
        .any(|(id, (pos, _))| {
            pos.x == x && pos.y == y && exclude.map_or(true, |ex| id != ex)
        })
}
