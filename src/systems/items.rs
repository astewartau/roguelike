//! Item system functions.

use crate::components::{Health, Inventory, ItemType};
use hecs::{Entity, World};

use super::item_defs::{get_def, ItemCategory, UseEffect};

// Re-export TargetingParams from item_defs for external use
pub use super::item_defs::TargetingParams;

/// Result of attempting to use an item
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ItemUseResult {
    /// Item was used successfully and consumed
    Used { item_type: ItemType },
    /// Item use failed (invalid index, missing component, etc.)
    Failed,
    /// Item requires a target selection before use (scrolls, throwable potions)
    RequiresTarget { item_type: ItemType, item_index: usize },
    /// Item is a weapon that should be equipped
    IsWeapon { item_type: ItemType, item_index: usize },
    /// Item is armor that should be equipped to a body/head slot
    IsArmor { item_type: ItemType, item_index: usize },
    /// Scroll of Reveal: show all enemies on floor
    RevealEnemies,
    /// Scroll of Mapping: reveal entire floor layout
    RevealMap,
    /// Scroll of Fear: apply fear to all visible enemies
    ApplyFearToVisible,
    /// Scroll of Slow: apply slow to all visible enemies
    ApplySlowToVisible,
    /// Empty water flask: try to fill it from an adjacent water tile
    /// (needs grid access, so the caller resolves it via `fill_water_flask`)
    FillWaterFlask { item_index: usize },
}

/// Get the display name of an item
pub fn item_name(item: ItemType) -> &'static str {
    get_def(item).name
}

/// Returns true if the item requires a target selection before use
#[cfg(test)]
pub fn item_requires_target(item: ItemType) -> bool {
    matches!(get_def(item).use_effect, UseEffect::RequiresTarget)
}

/// Get targeting parameters for an item (for items that require targeting or throwing)
pub fn item_targeting_params(item: ItemType) -> TargetingParams {
    get_def(item).targeting.unwrap_or_default()
}

/// Returns true if the item is a throwable potion
pub fn item_is_throwable(item: ItemType) -> bool {
    get_def(item).is_throwable
}

/// Use an item from an entity's inventory
/// Returns the result of the item use attempt
pub fn use_item(world: &mut World, entity: Entity, item_index: usize) -> ItemUseResult {
    // Get the item type before removing it
    let item_type = {
        let Ok(inv) = world.get::<&Inventory>(entity) else {
            return ItemUseResult::Failed;
        };
        if item_index >= inv.items.len() {
            return ItemUseResult::Failed;
        }
        inv.items[item_index].kind
    };

    // Water flasks are stateful (empty <-> full) and are handled specially
    // rather than through the def table:
    //  - drinking a full flask douses any Burning on the drinker and leaves
    //    the empty flask behind (the kind swaps in place; same weight);
    //  - an empty flask needs grid access to find water, so the caller
    //    resolves the returned FillWaterFlask via `fill_water_flask`.
    match item_type {
        ItemType::WaterFlaskFull => {
            super::effects::remove_effect_from_entity(
                world,
                entity,
                crate::components::EffectType::Burning,
            );
            if let Ok(mut inv) = world.get::<&mut Inventory>(entity) {
                if let Some(item) = inv.items.get_mut(item_index) {
                    item.kind = ItemType::WaterFlaskEmpty;
                }
            }
            return ItemUseResult::Used { item_type };
        }
        ItemType::WaterFlaskEmpty => {
            return ItemUseResult::FillWaterFlask { item_index };
        }
        _ => {}
    }

    let def = get_def(item_type);

    // Handle based on use effect from definition
    let result = match def.use_effect {
        UseEffect::Equip => {
            // Armor and accessories both route through the body/head/ring/
            // amulet slot path; only true weapons go to the weapon slot.
            if matches!(def.category, ItemCategory::Armor | ItemCategory::Accessory) {
                return ItemUseResult::IsArmor { item_type, item_index };
            }
            return ItemUseResult::IsWeapon { item_type, item_index };
        }
        UseEffect::RequiresTarget => {
            return ItemUseResult::RequiresTarget { item_type, item_index };
        }
        UseEffect::Heal(amount) => {
            apply_heal(world, entity, amount);
            // Food's primary job is feeding the hunger meter; the heal is a
            // small side benefit. No-op for non-food healers (potions).
            if let Some(hunger) = super::survival::food_hunger_restore(item_type) {
                super::survival::restore_hunger(world, entity, hunger);
            }
            ItemUseResult::Used { item_type }
        }
        UseEffect::ApplyEffect(effect_type, duration) => {
            // Scroll magnitudes scale with the reader's effective INT
            // (Invisibility/Speed/Protection durations). Potions are alchemy,
            // not magic — they stay fixed.
            let duration = if def.category == ItemCategory::Scroll {
                duration * crate::queries::int_power(world, entity)
            } else {
                duration
            };
            apply_status_effect(world, entity, effect_type, duration);
            ItemUseResult::Used { item_type }
        }
        UseEffect::RevealEnemies => {
            return ItemUseResult::RevealEnemies;
        }
        UseEffect::RevealMap => {
            return ItemUseResult::RevealMap;
        }
        UseEffect::ApplyEffectToVisible(effect_type, _duration) => {
            // Map to the specific result variants the caller expects
            match effect_type {
                crate::components::EffectType::Feared => return ItemUseResult::ApplyFearToVisible,
                crate::components::EffectType::Slowed => return ItemUseResult::ApplySlowToVisible,
                _ => return ItemUseResult::Failed,
            }
        }
    };

    // Remove item from inventory (only for items that were fully consumed here)
    if matches!(result, ItemUseResult::Used { .. }) {
        remove_item_from_inventory(world, entity, item_index);
    }

    result
}

/// Try to fill an empty water flask: succeeds if the entity is standing on or
/// next to a water tile, swapping the inventory item to a full flask in place.
/// Returns whether the flask was filled.
pub fn fill_water_flask(
    world: &mut World,
    grid: &crate::grid::Grid,
    entity: Entity,
    item_index: usize,
) -> bool {
    let Some((px, py)) = crate::queries::get_entity_position(world, entity) else {
        return false;
    };

    let near_water = grid
        .water_positions
        .iter()
        .any(|&(wx, wy)| (wx - px).abs() <= 1 && (wy - py).abs() <= 1);
    if !near_water {
        return false;
    }

    if let Ok(mut inv) = world.get::<&mut Inventory>(entity) {
        if let Some(item) = inv.items.get_mut(item_index) {
            if item.kind == ItemType::WaterFlaskEmpty {
                item.kind = ItemType::WaterFlaskFull;
                return true;
            }
        }
    }
    false
}

// Helper functions for applying item effects

fn apply_heal(world: &mut World, entity: Entity, amount: i32) {
    if let Ok(mut health) = world.get::<&mut Health>(entity) {
        health.current = (health.current + amount).min(health.max);
    }
}

fn apply_status_effect(
    world: &mut World,
    entity: Entity,
    effect_type: crate::components::EffectType,
    duration: f32,
) {
    super::effects::add_effect_to_entity(world, entity, effect_type, duration);
}

/// Remove an item from an entity's inventory by index
pub fn remove_item_from_inventory(world: &mut World, entity: Entity, item_index: usize) {
    if let Ok(mut inv) = world.get::<&mut Inventory>(entity) {
        if item_index < inv.items.len() {
            let item = inv.items.remove(item_index);
            inv.current_weight_kg -= item_weight(item.kind);
        }
    }
}

/// Get the weight of an item in kg
pub fn item_weight(item: ItemType) -> f32 {
    get_def(item).weight
}

/// Get the heal amount for healing items (0 for non-healing items)
#[cfg(test)]
pub fn item_heal_amount(item: ItemType) -> i32 {
    match get_def(item).use_effect {
        UseEffect::Heal(amount) => amount,
        _ => 0,
    }
}

/// Get the sprite reference for an item's icon
pub fn item_sprite(item: ItemType) -> (crate::tile::SpriteSheet, u32) {
    get_def(item).sprite
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::constants::{HEALTH_POTION_HEAL, HEALTH_POTION_WEIGHT, SCROLL_WEIGHT};

    #[test]
    fn test_item_name() {
        assert_eq!(item_name(ItemType::HealthPotion), "Health Potion");
        assert_eq!(item_name(ItemType::ScrollOfInvisibility), "Scroll of Invisibility");
        assert_eq!(item_name(ItemType::ScrollOfSpeed), "Scroll of Speed");
        assert_eq!(item_name(ItemType::RegenerationPotion), "Regeneration Potion");
        assert_eq!(item_name(ItemType::StrengthPotion), "Strength Potion");
        assert_eq!(item_name(ItemType::ConfusionPotion), "Confusion Potion");
    }

    #[test]
    fn test_item_weight() {
        assert_eq!(item_weight(ItemType::HealthPotion), HEALTH_POTION_WEIGHT);
        assert_eq!(item_weight(ItemType::ScrollOfInvisibility), SCROLL_WEIGHT);
        assert_eq!(item_weight(ItemType::ScrollOfSpeed), SCROLL_WEIGHT);
        assert_eq!(item_weight(ItemType::RegenerationPotion), HEALTH_POTION_WEIGHT);
    }

    #[test]
    fn test_item_heal_amount() {
        assert_eq!(item_heal_amount(ItemType::HealthPotion), HEALTH_POTION_HEAL);
        assert_eq!(item_heal_amount(ItemType::ScrollOfInvisibility), 0);
        assert_eq!(item_heal_amount(ItemType::ScrollOfSpeed), 0);
    }

    #[test]
    fn test_item_requires_target() {
        // Targeted scrolls
        assert!(item_requires_target(ItemType::ScrollOfBlink));
        assert!(item_requires_target(ItemType::ScrollOfFireball));
        // Fire trap requires targeting
        assert!(item_requires_target(ItemType::FireTrap));
        // Potions are drinkable by default (throwable via context menu)
        assert!(!item_requires_target(ItemType::HealthPotion));
        assert!(!item_requires_target(ItemType::ConfusionPotion));
        // Non-targeted scrolls
        assert!(!item_requires_target(ItemType::ScrollOfSpeed));
        // Weapons don't require targeting
        assert!(!item_requires_target(ItemType::Sword));
        assert!(!item_requires_target(ItemType::Bow));
    }

    #[test]
    fn test_water_flask_fill_and_empty_transitions() {
        use crate::components::{ItemInstance, Position, StatusEffects};
        use crate::grid::Grid;
        use crate::tile::{Tile, TileType};

        let mut world = hecs::World::new();
        let mut inv = Inventory::new();
        inv.items.push(ItemInstance::plain(ItemType::WaterFlaskEmpty));
        let entity = world.spawn((Position::new(2, 2), inv, StatusEffects::new()));

        let mut grid = Grid {
            width: 5,
            height: 5,
            tiles: vec![Tile::new(TileType::Floor); 25],
            chest_positions: vec![],
            door_positions: vec![],
            brazier_positions: vec![],
            decals: vec![],
            stairs_up_pos: None,
            stairs_down_pos: None,
            starting_room: None,
            illumination: vec![0.0; 25],
            themed_rooms: vec![],
            water_positions: vec![],
            coffin_positions: vec![],
            barrel_positions: vec![],
            shop_position: None,
            shop_decor_positions: vec![],
            trap_positions: vec![],
            furniture_positions: vec![],
            secret_room: None,
            secret_door_pos: None,
        };

        // Using the empty flask defers to the fill flow.
        assert_eq!(
            use_item(&mut world, entity, 0),
            ItemUseResult::FillWaterFlask { item_index: 0 }
        );

        // No water anywhere: fill fails, flask stays empty.
        assert!(!super::fill_water_flask(&mut world, &grid, entity, 0));
        {
            let inv = world.get::<&Inventory>(entity).expect("inventory");
            assert_eq!(inv.items[0].kind, ItemType::WaterFlaskEmpty);
        }

        // Water adjacent: fill succeeds, flask becomes full.
        grid.water_positions.push((3, 2));
        assert!(super::fill_water_flask(&mut world, &grid, entity, 0));
        {
            let inv = world.get::<&Inventory>(entity).expect("inventory");
            assert_eq!(inv.items[0].kind, ItemType::WaterFlaskFull);
        }

        // Drinking the full flask douses Burning on self and leaves the empty
        // flask behind (not consumed).
        crate::systems::effects::add_effect_to_entity(
            &mut world,
            entity,
            crate::components::EffectType::Burning,
            10.0,
        );
        assert_eq!(
            use_item(&mut world, entity, 0),
            ItemUseResult::Used { item_type: ItemType::WaterFlaskFull }
        );
        let inv = world.get::<&Inventory>(entity).expect("inventory");
        assert_eq!(inv.items.len(), 1, "flask must not be consumed");
        assert_eq!(inv.items[0].kind, ItemType::WaterFlaskEmpty);
        drop(inv);
        let burning = crate::systems::effects::entity_has_effect(
            &world,
            entity,
            crate::components::EffectType::Burning,
        );
        assert!(!burning, "drinking the flask douses the drinker");
    }

    #[test]
    fn test_item_is_throwable() {
        // All potions are throwable
        assert!(item_is_throwable(ItemType::ConfusionPotion));
        assert!(item_is_throwable(ItemType::HealthPotion));
        assert!(item_is_throwable(ItemType::RegenerationPotion));
        assert!(item_is_throwable(ItemType::StrengthPotion));
        // Scrolls are not throwable
        assert!(!item_is_throwable(ItemType::ScrollOfFireball));
        // Weapons are not throwable
        assert!(!item_is_throwable(ItemType::Sword));
    }
}
