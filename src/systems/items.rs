//! Item system functions.

use crate::components::{EffectType, Health, Inventory, ItemType, StatusEffects};
use crate::constants::*;
use hecs::{Entity, World};

/// Get the display name of an item
pub fn item_name(item: ItemType) -> &'static str {
    match item {
        ItemType::HealthPotion => "Health Potion",
        ItemType::ScrollOfInvisibility => "Scroll of Invisibility",
        ItemType::ScrollOfSpeed => "Scroll of Speed",
    }
}

/// Use an item from an entity's inventory
pub fn use_item(world: &mut World, entity: Entity, item_index: usize) {
    // Get the item type before removing it
    let item_type = {
        let Ok(inv) = world.get::<&Inventory>(entity) else {
            return;
        };
        if item_index >= inv.items.len() {
            return;
        }
        inv.items[item_index]
    };

    // Apply item effect based on type
    match item_type {
        ItemType::HealthPotion => {
            if let Ok(mut health) = world.get::<&mut Health>(entity) {
                health.current = (health.current + HEALTH_POTION_HEAL).min(health.max);
            }
        }
        ItemType::ScrollOfInvisibility => {
            if let Ok(mut effects) = world.get::<&mut StatusEffects>(entity) {
                effects.add_effect(EffectType::Invisible, INVISIBILITY_DURATION);
            }
        }
        ItemType::ScrollOfSpeed => {
            if let Ok(mut effects) = world.get::<&mut StatusEffects>(entity) {
                effects.add_effect(EffectType::SpeedBoost, SPEED_BOOST_DURATION);
            }
        }
    }

    // Remove item from inventory
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
        ItemType::HealthPotion => HEALTH_POTION_WEIGHT,
        ItemType::ScrollOfInvisibility => SCROLL_WEIGHT,
        ItemType::ScrollOfSpeed => SCROLL_WEIGHT,
    }
}

/// Get the heal amount for healing items (0 for non-healing items)
pub fn item_heal_amount(item: ItemType) -> i32 {
    match item {
        ItemType::HealthPotion => HEALTH_POTION_HEAL,
        ItemType::ScrollOfInvisibility => 0,
        ItemType::ScrollOfSpeed => 0,
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
    }

    #[test]
    fn test_item_weight() {
        assert_eq!(item_weight(ItemType::HealthPotion), HEALTH_POTION_WEIGHT);
        assert_eq!(item_weight(ItemType::ScrollOfInvisibility), SCROLL_WEIGHT);
        assert_eq!(item_weight(ItemType::ScrollOfSpeed), SCROLL_WEIGHT);
    }

    #[test]
    fn test_item_heal_amount() {
        assert_eq!(item_heal_amount(ItemType::HealthPotion), HEALTH_POTION_HEAL);
        assert_eq!(item_heal_amount(ItemType::ScrollOfInvisibility), 0);
        assert_eq!(item_heal_amount(ItemType::ScrollOfSpeed), 0);
    }
}
