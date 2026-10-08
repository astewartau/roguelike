//! Item-related constants (weights, damage, etc.).

/// Health potion heal amount
pub const HEALTH_POTION_HEAL: i32 = 20;
/// Health potion weight in kg
pub const HEALTH_POTION_WEIGHT: f32 = 0.5;

/// Scroll weight in kg
pub const SCROLL_WEIGHT: f32 = 0.1;

/// Sword weight in kg
pub const SWORD_WEIGHT: f32 = 2.0;
/// Bow weight in kg
pub const BOW_WEIGHT: f32 = 1.5;
/// Dagger weight in kg
pub const DAGGER_WEIGHT: f32 = 0.5;
/// Staff weight in kg
pub const STAFF_WEIGHT: f32 = 1.0;

/// Leather armor weight in kg
pub const LEATHER_ARMOR_WEIGHT: f32 = 4.0;
/// Chain mail weight in kg
pub const CHAIN_MAIL_WEIGHT: f32 = 8.0;
/// Helmet weight in kg
pub const HELMET_WEIGHT: f32 = 2.0;

/// Leather armor base defense (body slot)
pub const LEATHER_ARMOR_DEFENSE: i32 = 2;
/// Chain mail base defense (body slot)
pub const CHAIN_MAIL_DEFENSE: i32 = 4;
/// Helmet base defense (head slot)
pub const HELMET_DEFENSE: i32 = 1;

/// Sword base damage
pub const SWORD_BASE_DAMAGE: i32 = 10;
/// Sword damage bonus
pub const SWORD_DAMAGE_BONUS: i32 = 4;

/// Dagger base damage (lower than sword but faster attacks)
pub const DAGGER_BASE_DAMAGE: i32 = 6;
/// Dagger damage bonus
pub const DAGGER_DAMAGE_BONUS: i32 = 2;

/// Staff base damage (druid weapon)
pub const STAFF_BASE_DAMAGE: i32 = 6;
/// Staff damage bonus
pub const STAFF_DAMAGE_BONUS: i32 = 2;

/// Bow base damage
pub const BOW_BASE_DAMAGE: i32 = 8;
/// Arrow speed in tiles per second
pub const ARROW_SPEED: f32 = 15.0;

/// Chance an arrow that hit its target is recoverable from the ground.
/// Arrows that missed are always recoverable.
pub const ARROW_RECOVERY_CHANCE_ON_HIT: f32 = 0.5;

/// Speed of thrown potions (tiles per second)
pub const POTION_THROW_SPEED: f32 = 12.0;
/// Range for throwing potions
pub const POTION_THROW_RANGE: i32 = 6;
/// Splash radius for all thrown potions
pub const POTION_SPLASH_RADIUS: i32 = 1;

// Food
//
// Food's primary job is feeding the hunger meter (see systems/survival.rs);
// the heal amounts are a small side benefit (halved from their pre-hunger
// values).
/// Food weight in kg
pub const FOOD_WEIGHT: f32 = 0.2;
/// Cheese heal amount (less than health potion)
pub const CHEESE_HEAL: i32 = 4;
/// Bread heal amount
pub const BREAD_HEAL: i32 = 5;
/// Apple heal amount
pub const APPLE_HEAL: i32 = 2;
/// Hunger restored by eating cheese
pub const CHEESE_HUNGER_RESTORE: f32 = 25.0;
/// Hunger restored by eating bread
pub const BREAD_HUNGER_RESTORE: f32 = 35.0;
/// Hunger restored by eating an apple
pub const APPLE_HUNGER_RESTORE: f32 = 15.0;

// Fire Trap
/// Fire trap weight in kg
pub const FIRE_TRAP_WEIGHT: f32 = 0.3;
/// Fire trap placement range (adjacent tiles only)
pub const FIRE_TRAP_RANGE: i32 = 1;
/// Fire trap burst damage when triggered
pub const FIRE_TRAP_BURST_DAMAGE: i32 = 15;

// Arrows (ammunition)
/// Arrow weight in kg (per arrow)
pub const ARROW_WEIGHT: f32 = 0.05;
/// Starting arrow count for Ranger
pub const STARTING_ARROW_COUNT: u32 = 20;
// Stacking limits are not enforced yet. `FIRE_ARROW_STACK_MAX` below documents
// itself against ARROW_STACK_MAX and FIRE_ARROW_BUNDLE_COUNT is live, so these
// two stay as the plain-arrow half of the same pair.
/// Maximum arrows in a single stack
#[allow(dead_code)]
pub const ARROW_STACK_MAX: u32 = 50;
/// Arrows in a bundle pickup
#[allow(dead_code)]
pub const ARROW_BUNDLE_COUNT: u32 = 10;

// Fire arrows (ammunition)
/// Fire arrow weight in kg (per arrow)
pub const FIRE_ARROW_WEIGHT: f32 = 0.05;
/// Base price of a single fire arrow in gold
pub const FIRE_ARROW_PRICE: u32 = 4;
/// Fire arrows stocked by vendors (a bundle)
pub const FIRE_ARROW_BUNDLE_COUNT: u32 = 5;
/// Maximum fire arrows in a single stack (nominal, like ARROW_STACK_MAX)
#[allow(dead_code)]
pub const FIRE_ARROW_STACK_MAX: u32 = 50;
/// Chance a chest also contains a small bundle of fire arrows
pub const CHEST_FIRE_ARROW_CHANCE: f32 = 0.15;
/// Fire arrows found in a chest (min)
pub const CHEST_FIRE_ARROW_MIN: u32 = 3;
/// Fire arrows found in a chest (max)
pub const CHEST_FIRE_ARROW_MAX: u32 = 5;
/// Sprite tint for fire arrow projectiles (multiplied with the arrow texture)
pub const FIRE_ARROW_TINT: (f32, f32, f32) = (1.0, 0.55, 0.3);

// Water flasks (fire ecosystem utility — see systems/fire.rs)
/// Water flask weight in kg (same empty or full so the in-place kind swap
/// doesn't disturb inventory weight bookkeeping)
pub const WATER_FLASK_WEIGHT: f32 = 0.5;
/// Full water flask base price in gold
pub const WATER_FLASK_FULL_PRICE: u32 = 15;
/// Empty water flask base price in gold
pub const WATER_FLASK_EMPTY_PRICE: u32 = 5;
/// Sprite tint for thrown water flask projectiles (light blue)
pub const WATER_FLASK_TINT: (f32, f32, f32) = (0.55, 0.75, 1.0);

// Accessories (rings / amulets — pure affix carriers)
/// Ring weight in kg
pub const RING_WEIGHT: f32 = 0.05;
/// Amulet weight in kg
pub const AMULET_WEIGHT: f32 = 0.1;
/// Ring base price in gold
pub const RING_PRICE: u32 = 90;
/// Amulet base price in gold
pub const AMULET_PRICE: u32 = 120;
/// Maximum affix components on an accessory (rings/amulets roll 1-3)
pub const ACCESSORY_AFFIX_MAX: usize = 3;

// =============================================================================
// IDENTIFICATION (see systems/identify.rs)
// =============================================================================

/// Game-time seconds to identify a carried item at Intelligence 10
pub const IDENTIFY_BASE_SECONDS: f32 = 30.0;
/// Discrete game-time step for identification progress (accumulator pattern)
pub const IDENTIFY_TICK_INTERVAL: f32 = 0.25;
/// Clamp bounds for the INT scaling factor `1 + (int - 10) * 0.1`, so absurd
/// stats can't make identification instant or effectively never finish.
pub const IDENTIFY_INT_FACTOR_MIN: f32 = 0.25;
pub const IDENTIFY_INT_FACTOR_MAX: f32 = 3.0;

// =============================================================================
// AFFIX ROLL TABLES (see systems/item_defs.rs)
// =============================================================================

/// Chance that one rolled component on a Magic+ item is a curse
pub const CURSE_AFFIX_CHANCE: f32 = 0.15;
/// Damage affix roll range (weapons)
pub const AFFIX_DAMAGE_MIN: i32 = 1;
pub const AFFIX_DAMAGE_MAX: i32 = 3;
/// Defense affix roll range (armor)
pub const AFFIX_DEFENSE_MIN: i32 = 1;
pub const AFFIX_DEFENSE_MAX: i32 = 2;
/// Stat affix (Strength/Agility/Intelligence) roll range
pub const AFFIX_STAT_MIN: i32 = 1;
pub const AFFIX_STAT_MAX: i32 = 2;
/// Max-health affix roll range
pub const AFFIX_MAX_HEALTH_MIN: i32 = 3;
pub const AFFIX_MAX_HEALTH_MAX: i32 = 8;
/// On-hit ignite chance roll range
pub const AFFIX_IGNITE_CHANCE_MIN: f32 = 0.10;
pub const AFFIX_IGNITE_CHANCE_MAX: f32 = 0.25;
/// On-hit slow chance roll range
pub const AFFIX_SLOW_CHANCE_MIN: f32 = 0.15;
pub const AFFIX_SLOW_CHANCE_MAX: f32 = 0.30;
/// On-hit fear chance roll range
pub const AFFIX_FEAR_CHANCE_MIN: f32 = 0.10;
pub const AFFIX_FEAR_CHANCE_MAX: f32 = 0.20;
/// Lifesteal fraction roll range
pub const AFFIX_LIFESTEAL_MIN: f32 = 0.10;
pub const AFFIX_LIFESTEAL_MAX: f32 = 0.25;
/// Kill-heal amount roll range
pub const AFFIX_KILL_HEAL_MIN: i32 = 2;
pub const AFFIX_KILL_HEAL_MAX: i32 = 5;
/// Low-health bonus damage fraction roll range
pub const AFFIX_LOW_HEALTH_DAMAGE_MIN: f32 = 0.15;
pub const AFFIX_LOW_HEALTH_DAMAGE_MAX: f32 = 0.30;
/// Cursed-fragile defense penalty roll range
pub const AFFIX_CURSED_FRAGILE_MIN: i32 = 1;
pub const AFFIX_CURSED_FRAGILE_MAX: i32 = 2;
/// Cursed-heavy slow fraction roll range
pub const AFFIX_CURSED_HEAVY_MIN: f32 = 0.10;
pub const AFFIX_CURSED_HEAVY_MAX: f32 = 0.25;
