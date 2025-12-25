//! Core gameplay constants (player stats, XP, FOV).

/// Player's default starting health
pub const PLAYER_STARTING_HEALTH: i32 = 50;
/// Player's maximum energy pool
pub const PLAYER_MAX_ENERGY: i32 = 5;
/// Player's action speed multiplier (1.0 = baseline)
pub const PLAYER_SPEED: f32 = 1.0;
/// Player's starting strength
pub const PLAYER_STRENGTH: i32 = 14;
/// Player's starting intelligence
pub const PLAYER_INTELLIGENCE: i32 = 10;
/// Player's starting agility
pub const PLAYER_AGILITY: i32 = 14;

/// Default FOV radius for player
pub const FOV_RADIUS: i32 = 10;

/// Base XP formula multiplier (XP needed = level * this)
pub const XP_PER_LEVEL_MULTIPLIER: u32 = 100;

/// Unarmed attack damage
pub const UNARMED_DAMAGE: i32 = 2;

/// Player HP regenerated per regen event
pub const PLAYER_HP_REGEN_AMOUNT: i32 = 1;
/// Seconds between each player HP regen event
pub const PLAYER_HP_REGEN_INTERVAL: f32 = 10.0;
