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

/// Sword base damage
pub const SWORD_BASE_DAMAGE: i32 = 10;
/// Sword damage bonus
pub const SWORD_DAMAGE_BONUS: i32 = 4;

/// Dagger base damage (lower than sword but faster attacks)
pub const DAGGER_BASE_DAMAGE: i32 = 6;
/// Dagger damage bonus
pub const DAGGER_DAMAGE_BONUS: i32 = 2;

/// Bow base damage
pub const BOW_BASE_DAMAGE: i32 = 8;
/// Arrow speed in tiles per second
pub const ARROW_SPEED: f32 = 15.0;

/// Speed of thrown potions (tiles per second)
pub const POTION_THROW_SPEED: f32 = 12.0;
/// Range for throwing potions
pub const POTION_THROW_RANGE: i32 = 6;
/// Splash radius for all thrown potions
pub const POTION_SPLASH_RADIUS: i32 = 1;

// Food
/// Food weight in kg
pub const FOOD_WEIGHT: f32 = 0.2;
/// Cheese heal amount (less than health potion)
pub const CHEESE_HEAL: i32 = 8;
/// Bread heal amount
pub const BREAD_HEAL: i32 = 10;
/// Apple heal amount
pub const APPLE_HEAL: i32 = 5;
