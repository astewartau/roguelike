//! Item system functions.

use crate::components::ItemType;
use crate::constants::*;

/// Get the display name of an item
pub fn item_name(item: ItemType) -> &'static str {
    match item {
        ItemType::HealthPotion => "Health Potion",
    }
}

/// Get the weight of an item in kg
pub fn item_weight(item: ItemType) -> f32 {
    match item {
        ItemType::HealthPotion => HEALTH_POTION_WEIGHT,
    }
}

/// Get the heal amount for healing items (0 for non-healing items)
pub fn item_heal_amount(item: ItemType) -> i32 {
    match item {
        ItemType::HealthPotion => HEALTH_POTION_HEAL,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_item_name() {
        assert_eq!(item_name(ItemType::HealthPotion), "Health Potion");
    }

    #[test]
    fn test_item_weight() {
        assert_eq!(item_weight(ItemType::HealthPotion), HEALTH_POTION_WEIGHT);
    }

    #[test]
    fn test_item_heal_amount() {
        assert_eq!(item_heal_amount(ItemType::HealthPotion), HEALTH_POTION_HEAL);
    }
}
