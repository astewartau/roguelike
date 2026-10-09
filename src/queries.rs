//! Common entity query helpers.
//!
//! This module provides reusable query functions to reduce code repetition
//! across systems. These are pure read-only queries that don't modify state.

use std::collections::HashSet;

use hecs::{Entity, World};

use crate::components::{
    Actor, Attackable, BlocksMovement, EffectType, Equipment, Position, Stats,
};
use crate::spatial_cache::SpatialCache;
use crate::systems::effects;

/// Get all positions blocked by entities (for pathfinding/collision).
/// Optionally excludes a specific entity (usually the moving entity itself).
pub fn get_blocking_positions(world: &World, exclude: Option<Entity>) -> HashSet<(i32, i32)> {
    world
        .query::<(&Position, &BlocksMovement)>()
        .iter()
        .filter(|(id, _)| exclude != Some(*id))
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
            pos.x == x && pos.y == y && (exclude != Some(*id))
        })
        .map(|(id, _)| id)
}

/// Check if an entity has a specific status effect active.
pub fn has_status_effect(world: &World, entity: Entity, effect: EffectType) -> bool {
    effects::entity_has_effect(world, entity, effect)
}

/// Whether `entity` is slippery — slick with oil (Oiled) — and so hard to
/// get a grip on. Grabs and holds (the zombie grab) should fail or slip on a
/// slippery target.
#[allow(dead_code)] // Consumer (zombie grab) lands in the next combat phase
pub fn is_slippery(world: &World, entity: Entity) -> bool {
    has_status_effect(world, entity, EffectType::Oiled)
}

/// Check if an entity can perform an action (has energy and is not busy).
pub fn can_entity_act(world: &World, entity: Entity) -> bool {
    world
        .get::<&Actor>(entity)
        .map(|a| a.can_act())
        .unwrap_or(false)
}


/// Get an entity's logical position as a tuple.
pub fn get_entity_position(world: &World, entity: Entity) -> Option<(i32, i32)> {
    world.get::<&Position>(entity).ok().map(|p| (p.x, p.y))
}

/// An entity's effective stats: base `Stats` plus stat affixes on every
/// equipped item (weapon, body, head, ring, amulet). Read stats through this
/// helper wherever gameplay math depends on them so equipped gear actually
/// applies.
///
/// Returns baseline 10/10/10 for entities without a `Stats` component.
pub fn effective_stats(world: &World, entity: Entity) -> Stats {
    let mut stats = world
        .get::<&Stats>(entity)
        .map(|s| *s)
        .unwrap_or(Stats::new(10, 10, 10));

    if let Ok(equipment) = world.get::<&Equipment>(entity) {
        for inst in equipment.equipped_instances() {
            stats.strength += inst.strength_bonus();
            stats.agility += inst.agility_bonus();
            stats.intelligence += inst.intelligence_bonus();
        }
    }

    stats
}

/// Magic power multiplier for an entity, from its *effective* Intelligence
/// (base stats plus INT affixes on equipped gear). See
/// [`crate::constants::int_power_mult`] for the curve. Use this at every
/// magic-effect application site so gear INT counts.
pub fn int_power(world: &World, entity: Entity) -> f32 {
    crate::constants::int_power_mult(effective_stats(world, entity).intelligence)
}

/// Check if a position is blocked by any entity (excluding a specific one).
///
/// Answered from the `SpatialCache` rather than by scanning the world, so this
/// and AI pathfinding read the same source of truth. The engine tick holds a
/// `debug_assert` that the cache still agrees with a rebuild, so drift between
/// the two surfaces as a test/debug failure instead of as phantom blockers.
pub fn is_position_blocked(
    spatial_cache: &SpatialCache,
    x: i32,
    y: i32,
    exclude: Option<Entity>,
) -> bool {
    spatial_cache.is_blocked_excluding((x, y), exclude)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::components::{Affix, ItemInstance, ItemType, Rarity};

    /// Test helper: an identified gear instance with the given affixes.
    fn instance(kind: ItemType, rarity: Rarity, affixes: Vec<Affix>) -> ItemInstance {
        ItemInstance {
            kind,
            rarity,
            affixes,
            name: None,
            identified: true,
            identify_progress: 0.0,
        }
    }

    #[test]
    fn test_effective_stats_includes_equipped_affixes() {
        let mut world = World::new();

        let mut equipment = Equipment::empty();
        equipment.weapon_source = Some(instance(
            ItemType::Sword,
            Rarity::Rare,
            vec![Affix::Strength(2), Affix::Damage(1)],
        ));
        equipment.body = Some(instance(
            ItemType::LeatherArmor,
            Rarity::Magic,
            vec![Affix::Agility(1)],
        ));
        equipment.head = Some(instance(
            ItemType::Helmet,
            Rarity::Magic,
            vec![Affix::Intelligence(2)],
        ));

        let entity = world.spawn((Stats::new(16, 10, 12), equipment));

        let effective = effective_stats(&world, entity);
        assert_eq!(effective.strength, 18); // 16 + 2
        assert_eq!(effective.agility, 13); // 12 + 1
        assert_eq!(effective.intelligence, 12); // 10 + 2
    }

    #[test]
    fn test_effective_stats_includes_ring_and_amulet_affixes() {
        let mut world = World::new();

        let mut equipment = Equipment::empty();
        equipment.ring = Some(instance(
            ItemType::Ring,
            Rarity::Magic,
            vec![Affix::Strength(2), Affix::Intelligence(1)],
        ));
        equipment.amulet = Some(instance(
            ItemType::Amulet,
            Rarity::Rare,
            vec![Affix::Agility(2), Affix::Damage(2)],
        ));

        let entity = world.spawn((Stats::new(10, 10, 10), equipment));

        let effective = effective_stats(&world, entity);
        assert_eq!(effective.strength, 12); // 10 + 2 (ring)
        assert_eq!(effective.intelligence, 11); // 10 + 1 (ring)
        assert_eq!(effective.agility, 12); // 10 + 2 (amulet)

        // Damage affixes on accessories feed the damage helper, not stats.
        let eq = world.get::<&Equipment>(entity).expect("equipment");
        assert_eq!(eq.affix_damage_bonus(), 2);
    }

    #[test]
    fn test_effective_stats_without_equipment() {
        let mut world = World::new();
        let entity = world.spawn((Stats::new(14, 11, 9),));
        let effective = effective_stats(&world, entity);
        assert_eq!(effective.strength, 14);
        assert_eq!(effective.intelligence, 11);
        assert_eq!(effective.agility, 9);
    }
}
