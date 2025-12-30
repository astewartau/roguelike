//! Item system functions.

use crate::components::{Health, Inventory, ItemType};
use hecs::{Entity, World};

use super::item_defs::{get_def, UseEffect};

// Re-export TargetingParams from item_defs for external use
pub use super::item_defs::TargetingParams;

/// Result of attempting to use an item
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ItemUseResult {
    /// Item was used successfully and consumed
    Used,
    /// Item use failed (invalid index, missing component, etc.)
    Failed,
    /// Item requires a target selection before use (scrolls, throwable potions)
    RequiresTarget { item_type: ItemType, item_index: usize },
    /// Item is a weapon that should be equipped
    IsWeapon { item_type: ItemType, item_index: usize },
    /// Scroll of Reveal: show all enemies on floor
    RevealEnemies,
    /// Scroll of Mapping: reveal entire floor layout
    RevealMap,
    /// Scroll of Fear: apply fear to all visible enemies
    ApplyFearToVisible,
    /// Scroll of Slow: apply slow to all visible enemies
    ApplySlowToVisible,
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
        inv.items[item_index]
    };

    let def = get_def(item_type);

    // Handle based on use effect from definition
    let result = match def.use_effect {
        UseEffect::Equip => {
            return ItemUseResult::IsWeapon { item_type, item_index };
        }
        UseEffect::RequiresTarget => {
            return ItemUseResult::RequiresTarget { item_type, item_index };
        }
        UseEffect::Heal(amount) => {
            apply_heal(world, entity, amount);
            ItemUseResult::Used
        }
        UseEffect::ApplyEffect(effect_type, duration) => {
            apply_status_effect(world, entity, effect_type, duration);
            ItemUseResult::Used
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
    if result == ItemUseResult::Used {
        remove_item_from_inventory(world, entity, item_index);
    }

    result
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
            inv.current_weight_kg -= item_weight(item);
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
