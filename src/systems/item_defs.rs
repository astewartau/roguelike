//! Item definitions - all item properties in one place.
//!
//! This module provides a data-driven approach to item properties.
//! Instead of scattering match statements across the codebase,
//! all item attributes are defined in a single static table.

#![allow(dead_code)] // Fields reserved for future item system expansion

use crate::components::{EffectType, ItemType};
use crate::constants::*;
use crate::tile::{tile_ids, SpriteSheet};

/// Categories of items for behavior grouping
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ItemCategory {
    Weapon,
    Armor,
    /// Rings and amulets — pure affix carriers with no base stats
    Accessory,
    Potion,
    Scroll,
    Food,
    Trap,
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
    pub sprite: (SpriteSheet, u32),
    pub use_effect: UseEffect,
    pub targeting: Option<TargetingParams>,
    pub is_throwable: bool,
    /// Base price in gold (for vendor system)
    pub base_price: u32,
}

/// Get the definition for an item type
pub fn get_def(item: ItemType) -> &'static ItemDef {
    ITEM_DEFS
        .iter()
        .find(|def| def.item_type == item)
        .expect("All ItemType variants must have a definition")
}

/// Get the base price for an item type
pub fn get_price(item: ItemType) -> u32 {
    get_def(item).base_price
}

/// Get the sell price for an item (50% of base price)
pub fn get_sell_price(item: ItemType) -> u32 {
    get_def(item).base_price / 2
}

/// Base flat defense granted by an armor item kind (before affixes).
/// Returns 0 for non-armor kinds.
pub fn armor_base_defense(item: ItemType) -> i32 {
    match item {
        ItemType::LeatherArmor => LEATHER_ARMOR_DEFENSE,
        ItemType::ChainMail => CHAIN_MAIL_DEFENSE,
        ItemType::Helmet => HELMET_DEFENSE,
        _ => 0,
    }
}

/// Which equipment slot an armor/accessory item kind occupies.
/// Returns `None` for non-wearable kinds.
pub fn armor_slot(item: ItemType) -> Option<ArmorSlot> {
    match item {
        ItemType::LeatherArmor | ItemType::ChainMail => Some(ArmorSlot::Body),
        ItemType::Helmet => Some(ArmorSlot::Head),
        ItemType::Ring => Some(ArmorSlot::Ring),
        ItemType::Amulet => Some(ArmorSlot::Amulet),
        _ => None,
    }
}

/// Equipment slot a worn piece (armor or accessory) occupies.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArmorSlot {
    Body,
    Head,
    Ring,
    Amulet,
}

/// Minimum effective Intelligence required to Study this scroll into a
/// permanent ability. `None` for anything that isn't a learnable scroll
/// (Reveal/Mapping stay consumable utility).
pub fn min_learn_int(item: crate::components::ItemType) -> Option<i32> {
    use crate::components::ItemType;
    match item {
        ItemType::ScrollOfBlink => Some(LEARN_INT_BLINK),
        ItemType::ScrollOfFireball => Some(LEARN_INT_FIREBALL),
        ItemType::ScrollOfFear => Some(LEARN_INT_FEAR),
        ItemType::ScrollOfSlow => Some(LEARN_INT_SLOW),
        ItemType::ScrollOfProtection => Some(LEARN_INT_PROTECTION),
        ItemType::ScrollOfSpeed => Some(LEARN_INT_SPEED),
        ItemType::ScrollOfInvisibility => Some(LEARN_INT_INVISIBILITY),
        _ => None,
    }
}

/// The learned ability a scroll teaches when studied. Paired with
/// [`min_learn_int`]; both return `Some` for exactly the learnable scrolls.
pub fn scroll_learned_ability(
    item: crate::components::ItemType,
) -> Option<crate::components::AbilityType> {
    use crate::components::{AbilityType, ItemType};
    match item {
        ItemType::ScrollOfBlink => Some(AbilityType::LearnedBlink),
        ItemType::ScrollOfFireball => Some(AbilityType::LearnedFireball),
        ItemType::ScrollOfFear => Some(AbilityType::LearnedFear),
        ItemType::ScrollOfSlow => Some(AbilityType::LearnedSlow),
        ItemType::ScrollOfProtection => Some(AbilityType::LearnedProtection),
        ItemType::ScrollOfSpeed => Some(AbilityType::LearnedSpeed),
        ItemType::ScrollOfInvisibility => Some(AbilityType::LearnedInvisibility),
        _ => None,
    }
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
        sprite: tile_ids::SWORD,
        use_effect: UseEffect::Equip,
        targeting: None,
        is_throwable: false,
        base_price: 80,
    },
    ItemDef {
        item_type: ItemType::Bow,
        name: "Bow",
        category: ItemCategory::Weapon,
        weight: BOW_WEIGHT,
        sprite: tile_ids::BOW,
        use_effect: UseEffect::Equip,
        targeting: None,
        is_throwable: false,
        base_price: 100,
    },
    ItemDef {
        item_type: ItemType::Dagger,
        name: "Dagger",
        category: ItemCategory::Weapon,
        weight: DAGGER_WEIGHT,
        sprite: tile_ids::DAGGER,
        use_effect: UseEffect::Equip,
        targeting: None,
        is_throwable: false,
        base_price: 40,
    },
    ItemDef {
        item_type: ItemType::Staff,
        name: "Staff",
        category: ItemCategory::Weapon,
        weight: STAFF_WEIGHT,
        sprite: tile_ids::STAFF,
        use_effect: UseEffect::Equip,
        targeting: None,
        is_throwable: false,
        base_price: 70,
    },
    // =========================================================================
    // ARMOR
    // =========================================================================
    ItemDef {
        item_type: ItemType::LeatherArmor,
        name: "Leather Armor",
        category: ItemCategory::Armor,
        weight: LEATHER_ARMOR_WEIGHT,
        sprite: tile_ids::LEATHER_ARMOR,
        use_effect: UseEffect::Equip,
        targeting: None,
        is_throwable: false,
        base_price: 60,
    },
    ItemDef {
        item_type: ItemType::ChainMail,
        name: "Chain Mail",
        category: ItemCategory::Armor,
        weight: CHAIN_MAIL_WEIGHT,
        sprite: tile_ids::CHAIN_MAIL,
        use_effect: UseEffect::Equip,
        targeting: None,
        is_throwable: false,
        base_price: 140,
    },
    ItemDef {
        item_type: ItemType::Helmet,
        name: "Helmet",
        category: ItemCategory::Armor,
        weight: HELMET_WEIGHT,
        sprite: tile_ids::HELMET,
        use_effect: UseEffect::Equip,
        targeting: None,
        is_throwable: false,
        base_price: 50,
    },
    // =========================================================================
    // ACCESSORIES (pure affix carriers — no base stats)
    // =========================================================================
    ItemDef {
        item_type: ItemType::Ring,
        name: "Ring",
        category: ItemCategory::Accessory,
        weight: RING_WEIGHT,
        sprite: tile_ids::RING,
        use_effect: UseEffect::Equip,
        targeting: None,
        is_throwable: false,
        base_price: RING_PRICE,
    },
    ItemDef {
        item_type: ItemType::Amulet,
        name: "Amulet",
        category: ItemCategory::Accessory,
        weight: AMULET_WEIGHT,
        sprite: tile_ids::AMULET,
        use_effect: UseEffect::Equip,
        targeting: None,
        is_throwable: false,
        base_price: AMULET_PRICE,
    },
    // =========================================================================
    // POTIONS
    // =========================================================================
    ItemDef {
        item_type: ItemType::HealthPotion,
        name: "Health Potion",
        category: ItemCategory::Potion,
        weight: HEALTH_POTION_WEIGHT,
        sprite: tile_ids::RED_POTION,
        use_effect: UseEffect::Heal(HEALTH_POTION_HEAL),
        targeting: Some(TargetingParams {
            max_range: POTION_THROW_RANGE,
            radius: POTION_SPLASH_RADIUS,
        }),
        is_throwable: true,
        base_price: 25,
    },
    ItemDef {
        item_type: ItemType::RegenerationPotion,
        name: "Regeneration Potion",
        category: ItemCategory::Potion,
        weight: HEALTH_POTION_WEIGHT,
        sprite: tile_ids::GREEN_POTION,
        use_effect: UseEffect::ApplyEffect(EffectType::Regenerating, REGENERATION_DURATION),
        targeting: Some(TargetingParams {
            max_range: POTION_THROW_RANGE,
            radius: POTION_SPLASH_RADIUS,
        }),
        is_throwable: true,
        base_price: 40,
    },
    ItemDef {
        item_type: ItemType::StrengthPotion,
        name: "Strength Potion",
        category: ItemCategory::Potion,
        weight: HEALTH_POTION_WEIGHT,
        sprite: tile_ids::AMBER_POTION,
        use_effect: UseEffect::ApplyEffect(EffectType::Strengthened, STRENGTH_DURATION),
        targeting: Some(TargetingParams {
            max_range: POTION_THROW_RANGE,
            radius: POTION_SPLASH_RADIUS,
        }),
        is_throwable: true,
        base_price: 50,
    },
    ItemDef {
        item_type: ItemType::ConfusionPotion,
        name: "Confusion Potion",
        category: ItemCategory::Potion,
        weight: HEALTH_POTION_WEIGHT,
        sprite: tile_ids::BLUE_POTION,
        use_effect: UseEffect::ApplyEffect(EffectType::Confused, CONFUSION_DURATION),
        targeting: Some(TargetingParams {
            max_range: POTION_THROW_RANGE,
            radius: POTION_SPLASH_RADIUS,
        }),
        is_throwable: true,
        base_price: 35,
    },
    // =========================================================================
    // SCROLLS
    // =========================================================================
    ItemDef {
        item_type: ItemType::ScrollOfInvisibility,
        name: "Scroll of Invisibility",
        category: ItemCategory::Scroll,
        weight: SCROLL_WEIGHT,
        sprite: tile_ids::SCROLL,
        use_effect: UseEffect::ApplyEffect(EffectType::Invisible, INVISIBILITY_DURATION),
        targeting: None,
        is_throwable: false,
        base_price: 60,
    },
    ItemDef {
        item_type: ItemType::ScrollOfSpeed,
        name: "Scroll of Speed",
        category: ItemCategory::Scroll,
        weight: SCROLL_WEIGHT,
        sprite: tile_ids::SCROLL,
        use_effect: UseEffect::ApplyEffect(EffectType::SpeedBoost, SPEED_BOOST_DURATION),
        targeting: None,
        is_throwable: false,
        base_price: 30,
    },
    ItemDef {
        item_type: ItemType::ScrollOfProtection,
        name: "Scroll of Protection",
        category: ItemCategory::Scroll,
        weight: SCROLL_WEIGHT,
        sprite: tile_ids::SCROLL,
        use_effect: UseEffect::ApplyEffect(EffectType::Protected, PROTECTION_DURATION),
        targeting: None,
        is_throwable: false,
        base_price: 35,
    },
    ItemDef {
        item_type: ItemType::ScrollOfBlink,
        name: "Scroll of Blink",
        category: ItemCategory::Scroll,
        weight: SCROLL_WEIGHT,
        sprite: tile_ids::SCROLL,
        use_effect: UseEffect::RequiresTarget,
        targeting: Some(TargetingParams {
            max_range: BLINK_RANGE,
            radius: 0,
        }),
        is_throwable: false,
        base_price: 75,
    },
    ItemDef {
        item_type: ItemType::ScrollOfFear,
        name: "Scroll of Fear",
        category: ItemCategory::Scroll,
        weight: SCROLL_WEIGHT,
        sprite: tile_ids::SCROLL,
        use_effect: UseEffect::ApplyEffectToVisible(EffectType::Feared, FEAR_DURATION),
        targeting: None,
        is_throwable: false,
        base_price: 80,
    },
    ItemDef {
        item_type: ItemType::ScrollOfFireball,
        name: "Scroll of Fireball",
        category: ItemCategory::Scroll,
        weight: SCROLL_WEIGHT,
        sprite: tile_ids::SCROLL,
        use_effect: UseEffect::RequiresTarget,
        targeting: Some(TargetingParams {
            max_range: FIREBALL_RANGE,
            radius: FIREBALL_RADIUS,
        }),
        is_throwable: false,
        base_price: 100,
    },
    ItemDef {
        item_type: ItemType::ScrollOfReveal,
        name: "Scroll of Reveal",
        category: ItemCategory::Scroll,
        weight: SCROLL_WEIGHT,
        sprite: tile_ids::SCROLL,
        use_effect: UseEffect::RevealEnemies,
        targeting: None,
        is_throwable: false,
        base_price: 45,
    },
    ItemDef {
        item_type: ItemType::ScrollOfMapping,
        name: "Scroll of Mapping",
        category: ItemCategory::Scroll,
        weight: SCROLL_WEIGHT,
        sprite: tile_ids::SCROLL,
        use_effect: UseEffect::RevealMap,
        targeting: None,
        is_throwable: false,
        base_price: 50,
    },
    ItemDef {
        item_type: ItemType::ScrollOfSlow,
        name: "Scroll of Slow",
        category: ItemCategory::Scroll,
        weight: SCROLL_WEIGHT,
        sprite: tile_ids::SCROLL,
        use_effect: UseEffect::ApplyEffectToVisible(EffectType::Slowed, SLOW_DURATION),
        targeting: None,
        is_throwable: false,
        base_price: 40,
    },
    // =========================================================================
    // FOOD
    // =========================================================================
    ItemDef {
        item_type: ItemType::Cheese,
        name: "Cheese",
        category: ItemCategory::Food,
        weight: FOOD_WEIGHT,
        sprite: tile_ids::CHEESE,
        use_effect: UseEffect::Heal(CHEESE_HEAL),
        targeting: None,
        is_throwable: false,
        base_price: 10,
    },
    ItemDef {
        item_type: ItemType::Bread,
        name: "Bread",
        category: ItemCategory::Food,
        weight: FOOD_WEIGHT,
        sprite: tile_ids::BREAD,
        use_effect: UseEffect::Heal(BREAD_HEAL),
        targeting: None,
        is_throwable: false,
        base_price: 10,
    },
    ItemDef {
        item_type: ItemType::Apple,
        name: "Apple",
        category: ItemCategory::Food,
        weight: FOOD_WEIGHT,
        sprite: tile_ids::APPLE,
        use_effect: UseEffect::Heal(APPLE_HEAL),
        targeting: None,
        is_throwable: false,
        base_price: 10,
    },
    // =========================================================================
    // WATER FLASKS (fill/drink/throw handled specially in systems::items and
    // the throw/splash path; the use_effect fields here are placeholders)
    // =========================================================================
    ItemDef {
        item_type: ItemType::WaterFlaskEmpty,
        name: "Water Flask (Empty)",
        category: ItemCategory::Potion,
        weight: WATER_FLASK_WEIGHT,
        sprite: tile_ids::BOTTLE_WATER,
        use_effect: UseEffect::Heal(0), // placeholder: use_item fills it from water
        targeting: None,
        is_throwable: false,
        base_price: WATER_FLASK_EMPTY_PRICE,
    },
    ItemDef {
        item_type: ItemType::WaterFlaskFull,
        name: "Water Flask",
        category: ItemCategory::Potion,
        weight: WATER_FLASK_WEIGHT,
        sprite: tile_ids::BOTTLE_WATER,
        use_effect: UseEffect::Heal(0), // placeholder: use_item douses the drinker
        targeting: Some(TargetingParams {
            max_range: POTION_THROW_RANGE,
            radius: POTION_SPLASH_RADIUS,
        }),
        is_throwable: true,
        base_price: WATER_FLASK_FULL_PRICE,
    },
    // =========================================================================
    // TRAPS
    // =========================================================================
    ItemDef {
        item_type: ItemType::FireTrap,
        name: "Fire Trap",
        category: ItemCategory::Trap,
        weight: FIRE_TRAP_WEIGHT,
        sprite: tile_ids::FIRE_TRAP,
        use_effect: UseEffect::RequiresTarget,
        targeting: Some(TargetingParams {
            max_range: FIRE_TRAP_RANGE,
            radius: 0,
        }),
        is_throwable: false,
        base_price: 50,
    },
    // =========================================================================
    // AMMUNITION
    // =========================================================================
    ItemDef {
        item_type: ItemType::Arrow,
        name: "Arrow",
        category: ItemCategory::Trap, // Reuse category for now
        weight: ARROW_WEIGHT,
        sprite: tile_ids::ARROW,
        use_effect: UseEffect::Equip, // Can't be "used" directly
        targeting: None,
        is_throwable: false,
        base_price: 2, // Cheap per arrow
    },
    ItemDef {
        item_type: ItemType::FireArrow,
        name: "Fire Arrow",
        category: ItemCategory::Trap, // Same grouping as Arrow
        weight: FIRE_ARROW_WEIGHT,
        sprite: tile_ids::ARROW, // Arrow sprite, red/orange-tinted in flight
        use_effect: UseEffect::Equip, // Can't be "used" directly
        targeting: None,
        is_throwable: false,
        base_price: FIRE_ARROW_PRICE,
    },
];

// =============================================================================
// GEAR ROLL TABLES (rarity, affixes, legendary names)
// =============================================================================

use crate::components::{Affix, ItemInstance, Rarity};
use rand::seq::SliceRandom;
use rand::Rng;

/// Weapons, armor, and accessories that can drop as rolled gear.
pub const GEAR_POOL: [ItemType; 9] = [
    ItemType::Sword,
    ItemType::Dagger,
    ItemType::Staff,
    ItemType::Bow,
    ItemType::LeatherArmor,
    ItemType::ChainMail,
    ItemType::Helmet,
    ItemType::Ring,
    ItemType::Amulet,
];

/// Is this item kind a weapon (rolls weapon affixes)?
fn is_weapon_kind(kind: ItemType) -> bool {
    matches!(
        kind,
        ItemType::Sword | ItemType::Bow | ItemType::Dagger | ItemType::Staff
    )
}

/// Is this item kind an accessory (ring/amulet — pure affix carrier)?
pub fn is_accessory_kind(kind: ItemType) -> bool {
    matches!(kind, ItemType::Ring | ItemType::Amulet)
}

/// Roll a rarity tier, weighted toward better tiers on deeper floors.
pub fn roll_rarity(floor: u32, rng: &mut impl Rng) -> Rarity {
    // Weights (out of their sum). Deeper floors push toward Magic/Rare/Legendary.
    let common_w = 70u32.saturating_sub(floor * 3).max(20);
    let magic_w = 25 + floor * 2;
    let rare_w = 5 + floor * 3;
    let legendary_w = 1 + floor;
    let total = common_w + magic_w + rare_w + legendary_w;

    let roll = rng.gen_range(0..total);
    if roll < common_w {
        Rarity::Common
    } else if roll < common_w + magic_w {
        Rarity::Magic
    } else if roll < common_w + magic_w + rare_w {
        Rarity::Rare
    } else {
        Rarity::Legendary
    }
}

/// Number of affix components an item of this rarity carries.
pub fn affix_count_for_rarity(rarity: Rarity, rng: &mut impl Rng) -> usize {
    match rarity {
        Rarity::Common => 0,
        Rarity::Magic => 1,
        Rarity::Rare => 2,
        Rarity::Legendary => rng.gen_range(3..=4),
    }
}

/// Kinds of affix that can roll, used to avoid duplicates within one item.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AffixKind {
    Damage,
    Defense,
    Strength,
    Agility,
    Intelligence,
    MaxHealth,
    OnHitIgnite,
    OnHitSlow,
    OnHitFear,
    OnHitLifesteal,
    OnHitKnockback,
    KillHeal,
    LowHealthDamage,
    CursedFragile,
    CursedHeavy,
    CursedLoud,
}

/// Positive affix kinds available to weapons.
const WEAPON_AFFIX_KINDS: [AffixKind; 10] = [
    AffixKind::Damage,
    AffixKind::Strength,
    AffixKind::Agility,
    AffixKind::Intelligence,
    AffixKind::OnHitIgnite,
    AffixKind::OnHitSlow,
    AffixKind::OnHitFear,
    AffixKind::OnHitLifesteal,
    AffixKind::OnHitKnockback,
    AffixKind::KillHeal,
];

/// Extra weapon affix kinds that are conditionals rather than on-hits.
const WEAPON_CONDITIONAL_KINDS: [AffixKind; 1] = [AffixKind::LowHealthDamage];

/// Positive affix kinds available to armor.
const ARMOR_AFFIX_KINDS: [AffixKind; 5] = [
    AffixKind::Defense,
    AffixKind::Strength,
    AffixKind::Agility,
    AffixKind::Intelligence,
    AffixKind::MaxHealth,
];

/// Affix kinds available to accessories (rings/amulets). Restricted to kinds
/// that apply through the shared stat/defense/damage aggregation helpers —
/// on-hit triggers stay weapon-only (they resolve off `weapon_source`).
const ACCESSORY_AFFIX_KINDS: [AffixKind; 6] = [
    AffixKind::Damage,
    AffixKind::Defense,
    AffixKind::Strength,
    AffixKind::Agility,
    AffixKind::Intelligence,
    AffixKind::MaxHealth,
];

/// Curse affix kinds (can roll alongside good affixes on Magic+ items).
const CURSE_AFFIX_KINDS: [AffixKind; 3] = [
    AffixKind::CursedFragile,
    AffixKind::CursedHeavy,
    AffixKind::CursedLoud,
];

/// Roll concrete values for an affix kind.
fn roll_affix_value(kind: AffixKind, rng: &mut impl Rng) -> Affix {
    match kind {
        AffixKind::Damage => Affix::Damage(rng.gen_range(AFFIX_DAMAGE_MIN..=AFFIX_DAMAGE_MAX)),
        AffixKind::Defense => Affix::Defense(rng.gen_range(AFFIX_DEFENSE_MIN..=AFFIX_DEFENSE_MAX)),
        AffixKind::Strength => Affix::Strength(rng.gen_range(AFFIX_STAT_MIN..=AFFIX_STAT_MAX)),
        AffixKind::Agility => Affix::Agility(rng.gen_range(AFFIX_STAT_MIN..=AFFIX_STAT_MAX)),
        AffixKind::Intelligence => {
            Affix::Intelligence(rng.gen_range(AFFIX_STAT_MIN..=AFFIX_STAT_MAX))
        }
        AffixKind::MaxHealth => {
            Affix::MaxHealth(rng.gen_range(AFFIX_MAX_HEALTH_MIN..=AFFIX_MAX_HEALTH_MAX))
        }
        AffixKind::OnHitIgnite => {
            Affix::OnHitIgnite(rng.gen_range(AFFIX_IGNITE_CHANCE_MIN..=AFFIX_IGNITE_CHANCE_MAX))
        }
        AffixKind::OnHitSlow => {
            Affix::OnHitSlow(rng.gen_range(AFFIX_SLOW_CHANCE_MIN..=AFFIX_SLOW_CHANCE_MAX))
        }
        AffixKind::OnHitFear => {
            Affix::OnHitFear(rng.gen_range(AFFIX_FEAR_CHANCE_MIN..=AFFIX_FEAR_CHANCE_MAX))
        }
        AffixKind::OnHitLifesteal => {
            Affix::OnHitLifesteal(rng.gen_range(AFFIX_LIFESTEAL_MIN..=AFFIX_LIFESTEAL_MAX))
        }
        AffixKind::OnHitKnockback => Affix::OnHitKnockback,
        AffixKind::KillHeal => {
            Affix::KillHeal(rng.gen_range(AFFIX_KILL_HEAL_MIN..=AFFIX_KILL_HEAL_MAX))
        }
        AffixKind::LowHealthDamage => Affix::LowHealthDamage(
            rng.gen_range(AFFIX_LOW_HEALTH_DAMAGE_MIN..=AFFIX_LOW_HEALTH_DAMAGE_MAX),
        ),
        AffixKind::CursedFragile => Affix::CursedFragile(
            rng.gen_range(AFFIX_CURSED_FRAGILE_MIN..=AFFIX_CURSED_FRAGILE_MAX),
        ),
        AffixKind::CursedHeavy => {
            Affix::CursedHeavy(rng.gen_range(AFFIX_CURSED_HEAVY_MIN..=AFFIX_CURSED_HEAVY_MAX))
        }
        AffixKind::CursedLoud => Affix::CursedLoud,
    }
}

/// Roll a gear instance (weapon, armor, or accessory) with floor-scaled
/// rarity and affixes.
///
/// Affix count is fixed by rarity (Common 0 / Magic 1 / Rare 2 / Legendary 3-4).
/// Weapons, armor, and accessories draw from separate positive-affix pools
/// without duplicate kinds; on Magic+ items one component has a small chance
/// to be a curse. Legendary items get a generated name built from their
/// components. Magic+ rolls spawn unidentified.
pub fn roll_gear(kind: ItemType, floor: u32, rng: &mut impl Rng) -> ItemInstance {
    let rarity = roll_rarity(floor, rng);
    roll_gear_with_rarity(kind, rarity, rng)
}

/// Roll a gear instance at a fixed rarity (used by roll_gear and tests).
///
/// Accessories are never Common (a plain ring is pointless): a Common roll is
/// promoted to Magic, and their affix count is capped at
/// `ACCESSORY_AFFIX_MAX` (1-3 components).
pub fn roll_gear_with_rarity(kind: ItemType, rarity: Rarity, rng: &mut impl Rng) -> ItemInstance {
    let rarity = if is_accessory_kind(kind) && rarity == Rarity::Common {
        Rarity::Magic
    } else {
        rarity
    };

    let mut affix_count = affix_count_for_rarity(rarity, rng);
    if is_accessory_kind(kind) {
        affix_count = affix_count.min(ACCESSORY_AFFIX_MAX);
    }

    // Build the positive pool for this item category.
    let mut pool: Vec<AffixKind> = if is_weapon_kind(kind) {
        WEAPON_AFFIX_KINDS
            .iter()
            .chain(WEAPON_CONDITIONAL_KINDS.iter())
            .copied()
            .collect()
    } else if is_accessory_kind(kind) {
        ACCESSORY_AFFIX_KINDS.to_vec()
    } else {
        ARMOR_AFFIX_KINDS.to_vec()
    };

    let mut kinds: Vec<AffixKind> = Vec::with_capacity(affix_count);

    // On multi-component items, one component has a small chance to be a curse
    // (gamble items: a downside alongside good affixes, never pure trash).
    if affix_count > 1 && rng.gen::<f32>() < CURSE_AFFIX_CHANCE {
        if let Some(&curse) = CURSE_AFFIX_KINDS.choose(rng) {
            kinds.push(curse);
        }
    }

    // Fill the rest from the positive pool without duplicate kinds.
    while kinds.len() < affix_count && !pool.is_empty() {
        let idx = rng.gen_range(0..pool.len());
        kinds.push(pool.swap_remove(idx));
    }

    let affixes: Vec<Affix> = kinds.iter().map(|&k| roll_affix_value(k, rng)).collect();

    let name = if rarity == Rarity::Legendary {
        Some(generate_legendary_name(kind, &affixes))
    } else {
        None
    };

    // Magic+ gear drops unidentified: the carrier learns its components by
    // carrying it (systems::identify). Common gear has nothing to hide.
    let identified = rarity == Rarity::Common;

    ItemInstance {
        kind,
        rarity,
        affixes,
        name,
        identified,
        identify_progress: 0.0,
    }
}

/// First name part for a legendary name, keyed by affix kind.
fn name_prefix(affix: &Affix) -> &'static str {
    match affix {
        Affix::Damage(_) => "Gore",
        Affix::Defense(_) => "Ward",
        Affix::Strength(_) => "Ox",
        Affix::Agility(_) => "Swift",
        Affix::Intelligence(_) => "Rune",
        Affix::MaxHealth(_) => "Heart",
        Affix::OnHitIgnite(_) => "Ember",
        Affix::OnHitSlow(_) => "Frost",
        Affix::OnHitFear(_) => "Dread",
        Affix::OnHitLifesteal(_) => "Leech",
        Affix::OnHitKnockback => "Ram",
        Affix::KillHeal(_) => "Reaper",
        Affix::LowHealthDamage(_) => "Rage",
        Affix::CursedFragile(_) => "Brittle",
        Affix::CursedHeavy(_) => "Lead",
        Affix::CursedLoud => "Bell",
    }
}

/// Second name part (syllable) for a legendary name, keyed by affix kind.
fn name_suffix(affix: &Affix) -> &'static str {
    match affix {
        Affix::Damage(_) => "fang",
        Affix::Defense(_) => "guard",
        Affix::Strength(_) => "fist",
        Affix::Agility(_) => "wind",
        Affix::Intelligence(_) => "gleam",
        Affix::MaxHealth(_) => "blood",
        Affix::OnHitIgnite(_) => "brand",
        Affix::OnHitSlow(_) => "chill",
        Affix::OnHitFear(_) => "wail",
        Affix::OnHitLifesteal(_) => "thirst",
        Affix::OnHitKnockback => "shock",
        Affix::KillHeal(_) => "harvest",
        Affix::LowHealthDamage(_) => "fury",
        Affix::CursedFragile(_) => "bane",
        Affix::CursedHeavy(_) => "weight",
        Affix::CursedLoud => "toll",
    }
}

/// Epithet for a legendary name ("... of the <epithet>"), keyed by affix kind.
fn name_epithet(affix: &Affix) -> &'static str {
    match affix {
        Affix::Damage(_) => "Wolf",
        Affix::Defense(_) => "Turtle",
        Affix::Strength(_) => "Giant",
        Affix::Agility(_) => "Fox",
        Affix::Intelligence(_) => "Owl",
        Affix::MaxHealth(_) => "Bear",
        Affix::OnHitIgnite(_) => "Flame",
        Affix::OnHitSlow(_) => "Glacier",
        Affix::OnHitFear(_) => "Banshee",
        Affix::OnHitLifesteal(_) => "Vampire",
        Affix::OnHitKnockback => "Tempest",
        Affix::KillHeal(_) => "Ghoul",
        Affix::LowHealthDamage(_) => "Berserker",
        Affix::CursedFragile(_) => "Broken",
        Affix::CursedHeavy(_) => "Burden",
        Affix::CursedLoud => "Herald",
    }
}

/// Generate a legendary name from an item's affix components, e.g.
/// "Emberfang, Sword of the Wolf". Deterministic given kind + affixes.
pub fn generate_legendary_name(kind: ItemType, affixes: &[Affix]) -> String {
    let base = get_def(kind).name;
    match affixes {
        [] => format!("Nameless {}", base),
        [only] => format!(
            "{}{}, {} of the {}",
            name_prefix(only),
            name_suffix(only),
            base,
            name_epithet(only)
        ),
        [first, second, rest @ ..] => {
            // Epithet comes from the last component so 3+ affix items read
            // differently from 2-affix ones.
            let epithet_src = rest.last().unwrap_or(second);
            format!(
                "{}{}, {} of the {}",
                name_prefix(first),
                name_suffix(second),
                base,
                name_epithet(epithet_src)
            )
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_all_item_types_have_definitions() {
        // Ensure we have a definition for every ItemType variant
        let all_items = [
            ItemType::Sword,
            ItemType::Bow,
            ItemType::Dagger,
            ItemType::Staff,
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
            ItemType::Cheese,
            ItemType::Bread,
            ItemType::Apple,
            ItemType::FireTrap,
            ItemType::Arrow,
            ItemType::FireArrow,
            ItemType::Ring,
            ItemType::Amulet,
            ItemType::WaterFlaskEmpty,
            ItemType::WaterFlaskFull,
        ];

        for item in all_items {
            let def = get_def(item);
            assert_eq!(def.item_type, item);
        }
    }

    #[test]
    fn test_affix_count_per_rarity() {
        use rand::rngs::StdRng;
        use rand::SeedableRng;

        let mut rng = StdRng::seed_from_u64(42);
        for _ in 0..200 {
            for (rarity, min, max) in [
                (Rarity::Common, 0, 0),
                (Rarity::Magic, 1, 1),
                (Rarity::Rare, 2, 2),
                (Rarity::Legendary, 3, 4),
            ] {
                let inst = roll_gear_with_rarity(ItemType::Sword, rarity, &mut rng);
                assert!(
                    inst.affixes.len() >= min && inst.affixes.len() <= max,
                    "{:?} rolled {} affixes, expected {}..={}",
                    rarity,
                    inst.affixes.len(),
                    min,
                    max
                );
                let armor = roll_gear_with_rarity(ItemType::ChainMail, rarity, &mut rng);
                assert!(armor.affixes.len() >= min && armor.affixes.len() <= max);
            }
        }
    }

    #[test]
    fn test_no_duplicate_affix_kinds() {
        use rand::rngs::StdRng;
        use rand::SeedableRng;

        let mut rng = StdRng::seed_from_u64(7);
        for _ in 0..200 {
            let inst = roll_gear_with_rarity(ItemType::Bow, Rarity::Legendary, &mut rng);
            let discriminants: Vec<std::mem::Discriminant<Affix>> =
                inst.affixes.iter().map(std::mem::discriminant).collect();
            let unique: std::collections::HashSet<_> = discriminants.iter().collect();
            assert_eq!(
                unique.len(),
                discriminants.len(),
                "duplicate affix kinds on one item: {:?}",
                inst.affixes
            );
        }
    }

    #[test]
    fn test_legendary_items_are_named() {
        use rand::rngs::StdRng;
        use rand::SeedableRng;

        let mut rng = StdRng::seed_from_u64(99);
        for _ in 0..50 {
            let inst = roll_gear_with_rarity(ItemType::Sword, Rarity::Legendary, &mut rng);
            let name = inst.name.as_deref().expect("legendary items must be named");
            assert!(name.contains("Sword"), "name should contain base kind: {}", name);
            assert!(name.contains("of the "), "name should have an epithet: {}", name);

            // Non-legendary rarities never get a name.
            let magic = roll_gear_with_rarity(ItemType::Sword, Rarity::Magic, &mut rng);
            assert!(magic.name.is_none());
        }
    }

    #[test]
    fn test_legendary_name_from_components() {
        let affixes = [
            Affix::OnHitIgnite(0.2),
            Affix::Damage(2),
            Affix::Damage(2),
        ];
        let name = generate_legendary_name(ItemType::Sword, &affixes);
        assert_eq!(name, "Emberfang, Sword of the Wolf");
    }

    #[test]
    fn test_armor_never_rolls_on_hit_affixes() {
        use rand::rngs::StdRng;
        use rand::SeedableRng;

        let mut rng = StdRng::seed_from_u64(1234);
        for _ in 0..200 {
            let inst = roll_gear_with_rarity(ItemType::Helmet, Rarity::Legendary, &mut rng);
            for affix in &inst.affixes {
                assert!(
                    !matches!(
                        affix,
                        Affix::Damage(_)
                            | Affix::OnHitIgnite(_)
                            | Affix::OnHitSlow(_)
                            | Affix::OnHitFear(_)
                            | Affix::OnHitLifesteal(_)
                            | Affix::OnHitKnockback
                            | Affix::KillHeal(_)
                            | Affix::LowHealthDamage(_)
                    ) || affix.is_curse(),
                    "armor rolled a weapon affix: {:?}",
                    affix
                );
            }
        }
    }

    #[test]
    fn test_magic_plus_gear_rolls_unidentified() {
        use rand::rngs::StdRng;
        use rand::SeedableRng;

        let mut rng = StdRng::seed_from_u64(21);
        for _ in 0..50 {
            // Common gear has nothing to hide.
            let common = roll_gear_with_rarity(ItemType::Sword, Rarity::Common, &mut rng);
            assert!(common.identified);
            // Magic+ spawns unidentified with zero progress.
            for rarity in [Rarity::Magic, Rarity::Rare, Rarity::Legendary] {
                let inst = roll_gear_with_rarity(ItemType::Sword, rarity, &mut rng);
                assert!(!inst.identified, "{:?} gear should drop unidentified", rarity);
                assert_eq!(inst.identify_progress, 0.0);
            }
        }
        // Plain instances (consumables, vendor stock) are always identified.
        assert!(crate::components::ItemInstance::plain(ItemType::Bread).identified);
    }

    #[test]
    fn test_accessories_never_common_and_roll_1_to_3_affixes() {
        use rand::rngs::StdRng;
        use rand::SeedableRng;

        let mut rng = StdRng::seed_from_u64(77);
        for _ in 0..200 {
            for kind in [ItemType::Ring, ItemType::Amulet] {
                // A Common roll is promoted to Magic — a plain ring is pointless.
                let inst = roll_gear_with_rarity(kind, Rarity::Common, &mut rng);
                assert_eq!(inst.rarity, Rarity::Magic);
                assert_eq!(inst.affixes.len(), 1);

                // Legendary accessories cap at 3 components.
                let leg = roll_gear_with_rarity(kind, Rarity::Legendary, &mut rng);
                assert!(
                    (1..=ACCESSORY_AFFIX_MAX).contains(&leg.affixes.len()),
                    "accessory rolled {} affixes",
                    leg.affixes.len()
                );

                // Accessories only carry affixes that apply through the shared
                // stat/defense/damage helpers (on-hit stays weapon-only), plus
                // possibly one curse.
                for affix in leg.affixes.iter().chain(inst.affixes.iter()) {
                    assert!(
                        matches!(
                            affix,
                            Affix::Damage(_)
                                | Affix::Defense(_)
                                | Affix::Strength(_)
                                | Affix::Agility(_)
                                | Affix::Intelligence(_)
                                | Affix::MaxHealth(_)
                        ) || affix.is_curse(),
                        "accessory rolled unsupported affix: {:?}",
                        affix
                    );
                }
            }
        }
    }

    #[test]
    fn test_scroll_learnability() {
        use crate::components::AbilityType;

        // Learnable scrolls expose both a threshold and a learned ability.
        let expected = [
            (ItemType::ScrollOfBlink, LEARN_INT_BLINK, AbilityType::LearnedBlink),
            (ItemType::ScrollOfFireball, LEARN_INT_FIREBALL, AbilityType::LearnedFireball),
            (ItemType::ScrollOfFear, LEARN_INT_FEAR, AbilityType::LearnedFear),
            (ItemType::ScrollOfSlow, LEARN_INT_SLOW, AbilityType::LearnedSlow),
            (ItemType::ScrollOfProtection, LEARN_INT_PROTECTION, AbilityType::LearnedProtection),
            (ItemType::ScrollOfSpeed, LEARN_INT_SPEED, AbilityType::LearnedSpeed),
            (ItemType::ScrollOfInvisibility, LEARN_INT_INVISIBILITY, AbilityType::LearnedInvisibility),
        ];
        for (scroll, int, ability) in expected {
            assert_eq!(min_learn_int(scroll), Some(int), "{:?}", scroll);
            assert_eq!(scroll_learned_ability(scroll), Some(ability), "{:?}", scroll);
            // Every learnable spell must carry a learned cooldown.
            assert!(ability.learned_cooldown().is_some(), "{:?}", ability);
        }

        // Utility scrolls (and non-scrolls) are NOT learnable.
        for item in [
            ItemType::ScrollOfReveal,
            ItemType::ScrollOfMapping,
            ItemType::HealthPotion,
            ItemType::Sword,
        ] {
            assert_eq!(min_learn_int(item), None, "{:?}", item);
            assert_eq!(scroll_learned_ability(item), None, "{:?}", item);
        }
    }

    #[test]
    fn test_weapons_have_equip_effect() {
        assert!(matches!(get_def(ItemType::Sword).use_effect, UseEffect::Equip));
        assert!(matches!(get_def(ItemType::Bow).use_effect, UseEffect::Equip));
        assert!(matches!(get_def(ItemType::Dagger).use_effect, UseEffect::Equip));
        assert!(matches!(get_def(ItemType::Staff).use_effect, UseEffect::Equip));
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
