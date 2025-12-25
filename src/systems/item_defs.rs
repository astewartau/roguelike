//! Item definitions - all item properties in one place.
//!
//! This module provides a data-driven approach to item properties.
//! Instead of scattering match statements across the codebase,
//! all item attributes are defined in a single static table.

#![allow(dead_code)] // Fields reserved for future item system expansion

use crate::components::{EffectType, ItemType};
use crate::constants::*;
use crate::tile::tile_ids;

/// Categories of items for behavior grouping
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ItemCategory {
    Weapon,
    Potion,
    Scroll,
}

/// How an item is used when consumed
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum UseEffect {
    /// Cannot be "used" - must be equipped (weapons)
    Equip,
    /// Heals the user for the specified amount
    Heal(i32),
    /// Applies a status effect to the user
    ApplyEffect(EffectType, f32),
    /// Requires target selection before use (blink, fireball)
    RequiresTarget,
    /// Reveal all enemies on the floor
    RevealEnemies,
    /// Reveal entire floor layout
    RevealMap,
    /// Apply effect to all visible enemies
    ApplyEffectToVisible(EffectType, f32),
}

/// Targeting parameters for items that require targeting
#[derive(Debug, Clone, Copy)]
pub struct TargetingParams {
    pub max_range: i32,
    pub radius: i32,
}

impl Default for TargetingParams {
    fn default() -> Self {
        Self { max_range: 8, radius: 0 }
    }
}

/// Complete definition of an item's properties
pub struct ItemDef {
    pub item_type: ItemType,
    pub name: &'static str,
    pub category: ItemCategory,
    pub weight: f32,
    pub tile_id: u32,
    pub use_effect: UseEffect,
    pub targeting: Option<TargetingParams>,
    pub is_throwable: bool,
}

/// Get the definition for an item type
pub fn get_def(item: ItemType) -> &'static ItemDef {
    ITEM_DEFS
        .iter()
        .find(|def| def.item_type == item)
        .expect("All ItemType variants must have a definition")
}

/// Static table of all item definitions
pub static ITEM_DEFS: &[ItemDef] = &[
    // =========================================================================
    // WEAPONS
    // =========================================================================
    ItemDef {
        item_type: ItemType::Sword,
        name: "Sword",
        category: ItemCategory::Weapon,
        weight: SWORD_WEIGHT,
        tile_id: tile_ids::SWORD,
        use_effect: UseEffect::Equip,
        targeting: None,
        is_throwable: false,
    },
    ItemDef {
        item_type: ItemType::Bow,
        name: "Bow",
        category: ItemCategory::Weapon,
        weight: BOW_WEIGHT,
        tile_id: tile_ids::BOW,
        use_effect: UseEffect::Equip,
        targeting: None,
        is_throwable: false,
    },
    // =========================================================================
    // POTIONS
    // =========================================================================
    ItemDef {
        item_type: ItemType::HealthPotion,
        name: "Health Potion",
        category: ItemCategory::Potion,
        weight: HEALTH_POTION_WEIGHT,
        tile_id: tile_ids::RED_POTION,
        use_effect: UseEffect::Heal(HEALTH_POTION_HEAL),
        targeting: Some(TargetingParams {
            max_range: POTION_THROW_RANGE,
            radius: POTION_SPLASH_RADIUS,
        }),
        is_throwable: true,
    },
    ItemDef {
        item_type: ItemType::RegenerationPotion,
        name: "Regeneration Potion",
        category: ItemCategory::Potion,
        weight: HEALTH_POTION_WEIGHT,
        tile_id: tile_ids::GREEN_POTION,
        use_effect: UseEffect::ApplyEffect(EffectType::Regenerating, REGENERATION_DURATION),
        targeting: Some(TargetingParams {
            max_range: POTION_THROW_RANGE,
            radius: POTION_SPLASH_RADIUS,
        }),
        is_throwable: true,
    },
    ItemDef {
        item_type: ItemType::StrengthPotion,
        name: "Strength Potion",
        category: ItemCategory::Potion,
        weight: HEALTH_POTION_WEIGHT,
        tile_id: tile_ids::AMBER_POTION,
        use_effect: UseEffect::ApplyEffect(EffectType::Strengthened, STRENGTH_DURATION),
        targeting: Some(TargetingParams {
            max_range: POTION_THROW_RANGE,
            radius: POTION_SPLASH_RADIUS,
        }),
        is_throwable: true,
    },
    ItemDef {
        item_type: ItemType::ConfusionPotion,
        name: "Confusion Potion",
        category: ItemCategory::Potion,
        weight: HEALTH_POTION_WEIGHT,
        tile_id: tile_ids::BLUE_POTION,
        use_effect: UseEffect::ApplyEffect(EffectType::Confused, CONFUSION_DURATION),
        targeting: Some(TargetingParams {
            max_range: POTION_THROW_RANGE,
            radius: POTION_SPLASH_RADIUS,
        }),
        is_throwable: true,
    },
    // =========================================================================
    // SCROLLS
    // =========================================================================
    ItemDef {
        item_type: ItemType::ScrollOfInvisibility,
        name: "Scroll of Invisibility",
        category: ItemCategory::Scroll,
        weight: SCROLL_WEIGHT,
        tile_id: tile_ids::SCROLL,
        use_effect: UseEffect::ApplyEffect(EffectType::Invisible, INVISIBILITY_DURATION),
        targeting: None,
        is_throwable: false,
    },
    ItemDef {
        item_type: ItemType::ScrollOfSpeed,
        name: "Scroll of Speed",
        category: ItemCategory::Scroll,
        weight: SCROLL_WEIGHT,
        tile_id: tile_ids::SCROLL,
        use_effect: UseEffect::ApplyEffect(EffectType::SpeedBoost, SPEED_BOOST_DURATION),
        targeting: None,
        is_throwable: false,
    },
    ItemDef {
        item_type: ItemType::ScrollOfProtection,
        name: "Scroll of Protection",
        category: ItemCategory::Scroll,
        weight: SCROLL_WEIGHT,
        tile_id: tile_ids::SCROLL,
        use_effect: UseEffect::ApplyEffect(EffectType::Protected, PROTECTION_DURATION),
        targeting: None,
        is_throwable: false,
    },
    ItemDef {
        item_type: ItemType::ScrollOfBlink,
        name: "Scroll of Blink",
        category: ItemCategory::Scroll,
        weight: SCROLL_WEIGHT,
        tile_id: tile_ids::SCROLL,
        use_effect: UseEffect::RequiresTarget,
        targeting: Some(TargetingParams {
            max_range: BLINK_RANGE,
            radius: 0,
        }),
        is_throwable: false,
    },
    ItemDef {
        item_type: ItemType::ScrollOfFear,
        name: "Scroll of Fear",
        category: ItemCategory::Scroll,
        weight: SCROLL_WEIGHT,
        tile_id: tile_ids::SCROLL,
        use_effect: UseEffect::ApplyEffectToVisible(EffectType::Feared, FEAR_DURATION),
        targeting: None,
        is_throwable: false,
    },
    ItemDef {
        item_type: ItemType::ScrollOfFireball,
        name: "Scroll of Fireball",
        category: ItemCategory::Scroll,
        weight: SCROLL_WEIGHT,
        tile_id: tile_ids::SCROLL,
        use_effect: UseEffect::RequiresTarget,
        targeting: Some(TargetingParams {
            max_range: FIREBALL_RANGE,
            radius: FIREBALL_RADIUS,
        }),
        is_throwable: false,
    },
    ItemDef {
        item_type: ItemType::ScrollOfReveal,
        name: "Scroll of Reveal",
        category: ItemCategory::Scroll,
        weight: SCROLL_WEIGHT,
        tile_id: tile_ids::SCROLL,
        use_effect: UseEffect::RevealEnemies,
        targeting: None,
        is_throwable: false,
    },
    ItemDef {
        item_type: ItemType::ScrollOfMapping,
        name: "Scroll of Mapping",
        category: ItemCategory::Scroll,
        weight: SCROLL_WEIGHT,
        tile_id: tile_ids::SCROLL,
        use_effect: UseEffect::RevealMap,
        targeting: None,
        is_throwable: false,
    },
    ItemDef {
        item_type: ItemType::ScrollOfSlow,
        name: "Scroll of Slow",
        category: ItemCategory::Scroll,
        weight: SCROLL_WEIGHT,
        tile_id: tile_ids::SCROLL,
        use_effect: UseEffect::ApplyEffectToVisible(EffectType::Slowed, SLOW_DURATION),
        targeting: None,
        is_throwable: false,
    },
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_all_item_types_have_definitions() {
        // Ensure we have a definition for every ItemType variant
        let all_items = [
            ItemType::Sword,
            ItemType::Bow,
            ItemType::HealthPotion,
            ItemType::RegenerationPotion,
            ItemType::StrengthPotion,
            ItemType::ConfusionPotion,
            ItemType::ScrollOfInvisibility,
            ItemType::ScrollOfSpeed,
            ItemType::ScrollOfProtection,
            ItemType::ScrollOfBlink,
            ItemType::ScrollOfFear,
            ItemType::ScrollOfFireball,
            ItemType::ScrollOfReveal,
            ItemType::ScrollOfMapping,
            ItemType::ScrollOfSlow,
        ];

        for item in all_items {
            let def = get_def(item);
            assert_eq!(def.item_type, item);
        }
    }

    #[test]
    fn test_weapons_have_equip_effect() {
        assert!(matches!(get_def(ItemType::Sword).use_effect, UseEffect::Equip));
        assert!(matches!(get_def(ItemType::Bow).use_effect, UseEffect::Equip));
    }

    #[test]
    fn test_potions_are_throwable() {
        assert!(get_def(ItemType::HealthPotion).is_throwable);
        assert!(get_def(ItemType::ConfusionPotion).is_throwable);
        assert!(get_def(ItemType::RegenerationPotion).is_throwable);
        assert!(get_def(ItemType::StrengthPotion).is_throwable);
    }

    #[test]
    fn test_scrolls_not_throwable() {
        assert!(!get_def(ItemType::ScrollOfBlink).is_throwable);
        assert!(!get_def(ItemType::ScrollOfFireball).is_throwable);
    }
}
