//! Item system functions.

use crate::components::{EffectType, Health, Inventory, ItemType, StatusEffects};
use crate::constants::*;
use hecs::{Entity, World};

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
    match item {
        // Weapons
        ItemType::Sword => "Sword",
        ItemType::Bow => "Bow",
        // Potions
        ItemType::HealthPotion => "Health Potion",
        ItemType::RegenerationPotion => "Regeneration Potion",
        ItemType::StrengthPotion => "Strength Potion",
        ItemType::ConfusionPotion => "Confusion Potion",
        // Scrolls
        ItemType::ScrollOfInvisibility => "Scroll of Invisibility",
        ItemType::ScrollOfSpeed => "Scroll of Speed",
        ItemType::ScrollOfProtection => "Scroll of Protection",
        ItemType::ScrollOfBlink => "Scroll of Blink",
        ItemType::ScrollOfFear => "Scroll of Fear",
        ItemType::ScrollOfFireball => "Scroll of Fireball",
        ItemType::ScrollOfReveal => "Scroll of Reveal",
        ItemType::ScrollOfMapping => "Scroll of Mapping",
        ItemType::ScrollOfSlow => "Scroll of Slow",
    }
}

/// Returns true if the item requires a target selection before use
pub fn item_requires_target(item: ItemType) -> bool {
    matches!(
        item,
        ItemType::ScrollOfBlink | ItemType::ScrollOfFireball
    )
}

/// Returns true if the item is a throwable potion
pub fn item_is_throwable(item: ItemType) -> bool {
    matches!(
        item,
        ItemType::HealthPotion
            | ItemType::RegenerationPotion
            | ItemType::StrengthPotion
            | ItemType::ConfusionPotion
    )
}

/// Returns true if the item is a weapon that can be equipped
pub fn item_is_weapon(item: ItemType) -> bool {
    matches!(item, ItemType::Sword | ItemType::Bow)
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

    // Check if item is a weapon (should be equipped, not "used")
    if item_is_weapon(item_type) {
        return ItemUseResult::IsWeapon { item_type, item_index };
    }

    // Check if item requires targeting (includes throwable potions and some scrolls)
    if item_requires_target(item_type) {
        return ItemUseResult::RequiresTarget { item_type, item_index };
    }

    // Apply item effect based on type
    let result = match item_type {
        // Weapons handled above
        ItemType::Sword | ItemType::Bow => {
            return ItemUseResult::IsWeapon { item_type, item_index };
        }
        // Potions - drink them (apply effect to self)
        ItemType::HealthPotion => {
            if let Ok(mut health) = world.get::<&mut Health>(entity) {
                let heal = item_heal_amount(item_type);
                health.current = (health.current + heal).min(health.max);
            }
            ItemUseResult::Used
        }
        ItemType::RegenerationPotion => {
            if let Ok(mut effects) = world.get::<&mut StatusEffects>(entity) {
                effects.add_effect(EffectType::Regenerating, REGENERATION_DURATION);
            }
            ItemUseResult::Used
        }
        ItemType::StrengthPotion => {
            if let Ok(mut effects) = world.get::<&mut StatusEffects>(entity) {
                effects.add_effect(EffectType::Strengthened, STRENGTH_DURATION);
            }
            ItemUseResult::Used
        }
        ItemType::ConfusionPotion => {
            // Drinking a confusion potion confuses yourself (not very useful!)
            if let Ok(mut effects) = world.get::<&mut StatusEffects>(entity) {
                effects.add_effect(EffectType::Confused, CONFUSION_DURATION);
            }
            ItemUseResult::Used
        }
        ItemType::ScrollOfInvisibility => {
            if let Ok(mut effects) = world.get::<&mut StatusEffects>(entity) {
                effects.add_effect(EffectType::Invisible, INVISIBILITY_DURATION);
            }
            ItemUseResult::Used
        }
        ItemType::ScrollOfSpeed => {
            if let Ok(mut effects) = world.get::<&mut StatusEffects>(entity) {
                effects.add_effect(EffectType::SpeedBoost, SPEED_BOOST_DURATION);
            }
            ItemUseResult::Used
        }
        ItemType::ScrollOfProtection => {
            if let Ok(mut effects) = world.get::<&mut StatusEffects>(entity) {
                effects.add_effect(EffectType::Protected, PROTECTION_DURATION);
            }
            ItemUseResult::Used
        }
        ItemType::ScrollOfBlink | ItemType::ScrollOfFireball => {
            // Shouldn't reach here due to requires_target check above
            return ItemUseResult::RequiresTarget { item_type, item_index };
        }
        ItemType::ScrollOfFear => {
            // Special handling: apply fear to visible enemies
            // The actual application is done by the caller
            return ItemUseResult::ApplyFearToVisible;
        }
        ItemType::ScrollOfReveal => {
            // Special handling: reveal all enemies
            // The actual revelation is done by the caller
            return ItemUseResult::RevealEnemies;
        }
        ItemType::ScrollOfMapping => {
            // Special handling: reveal entire map
            // The actual revelation is done by the caller
            return ItemUseResult::RevealMap;
        }
        ItemType::ScrollOfSlow => {
            // Special handling: apply slow to visible enemies
            // The actual application is done by the caller
            return ItemUseResult::ApplySlowToVisible;
        }
    };

    // Remove item from inventory (only for items that were fully consumed here)
    if result == ItemUseResult::Used {
        remove_item_from_inventory(world, entity, item_index);
    }

    result
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
    match item {
        // Weapons
        ItemType::Sword => SWORD_WEIGHT,
        ItemType::Bow => BOW_WEIGHT,
        // Potions
        ItemType::HealthPotion
        | ItemType::RegenerationPotion
        | ItemType::StrengthPotion
        | ItemType::ConfusionPotion => HEALTH_POTION_WEIGHT,
        // Scrolls
        ItemType::ScrollOfInvisibility
        | ItemType::ScrollOfSpeed
        | ItemType::ScrollOfProtection
        | ItemType::ScrollOfBlink
        | ItemType::ScrollOfFear
        | ItemType::ScrollOfFireball
        | ItemType::ScrollOfReveal
        | ItemType::ScrollOfMapping
        | ItemType::ScrollOfSlow => SCROLL_WEIGHT,
    }
}

/// Get the heal amount for healing items (0 for non-healing items)
pub fn item_heal_amount(item: ItemType) -> i32 {
    match item {
        ItemType::HealthPotion => HEALTH_POTION_HEAL,
        _ => 0,
    }
}

/// Get the tile ID for an item's icon
pub fn item_tile_id(item: ItemType) -> u32 {
    use crate::tile::tile_ids;
    match item {
        ItemType::Sword => tile_ids::SWORD,
        ItemType::Bow => tile_ids::BOW,
        ItemType::HealthPotion => tile_ids::RED_POTION,
        ItemType::RegenerationPotion => tile_ids::GREEN_POTION,
        ItemType::StrengthPotion => tile_ids::AMBER_POTION,
        ItemType::ConfusionPotion => tile_ids::BLUE_POTION,
        ItemType::ScrollOfInvisibility
        | ItemType::ScrollOfSpeed
        | ItemType::ScrollOfProtection
        | ItemType::ScrollOfBlink
        | ItemType::ScrollOfFear
        | ItemType::ScrollOfFireball
        | ItemType::ScrollOfReveal
        | ItemType::ScrollOfMapping
        | ItemType::ScrollOfSlow => tile_ids::SCROLL,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
