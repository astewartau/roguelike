//! Enemy stats and spawning constants.

/// Maximum distance from player for AI to be active (Manhattan distance)
/// Enemies further than this skip their turns entirely for performance
pub const AI_ACTIVE_RADIUS: i32 = 25;

// SKELETON
/// Number of skeletons to spawn
pub const SKELETON_SPAWN_COUNT: usize = 25;
/// Skeleton health
pub const SKELETON_HEALTH: i32 = 50;
/// Skeleton maximum energy pool
pub const SKELETON_MAX_ENERGY: i32 = 3;
/// Skeleton action speed multiplier (1.5 = 50% faster than player)
pub const SKELETON_SPEED: f32 = 1.5;
/// Skeleton sight radius for chase AI
pub const SKELETON_SIGHT_RADIUS: i32 = 8;
/// Skeleton strength
pub const SKELETON_STRENGTH: i32 = 10;
/// Skeleton intelligence
pub const SKELETON_INTELLIGENCE: i32 = 1;
/// Skeleton agility
pub const SKELETON_AGILITY: i32 = 3;
/// Skeleton attack damage
pub const SKELETON_DAMAGE: i32 = 6;

// RAT
/// Number of rats to spawn
pub const RAT_SPAWN_COUNT: usize = 40;
/// Rat health (weak)
pub const RAT_HEALTH: i32 = 30;
/// Rat maximum energy pool
pub const RAT_MAX_ENERGY: i32 = 4;
/// Rat action speed multiplier (fast and nimble)
pub const RAT_SPEED: f32 = 2.0;
/// Rat sight radius (poor eyesight)
pub const RAT_SIGHT_RADIUS: i32 = 5;
/// Rat strength (weak)
pub const RAT_STRENGTH: i32 = 3;
/// Rat intelligence
pub const RAT_INTELLIGENCE: i32 = 1;
/// Rat agility (quick)
pub const RAT_AGILITY: i32 = 8;
/// Rat attack damage (weak bite)
pub const RAT_DAMAGE: i32 = 5;

// SKELETON ARCHER
/// Number of skeleton archers to spawn
pub const SKELETON_ARCHER_SPAWN_COUNT: usize = 8;
/// Skeleton archer health (slightly weaker than melee skeleton)
pub const SKELETON_ARCHER_HEALTH: i32 = 40;
/// Skeleton archer maximum energy pool
pub const SKELETON_ARCHER_MAX_ENERGY: i32 = 3;
/// Skeleton archer action speed
pub const SKELETON_ARCHER_SPEED: f32 = 1.3;
/// Skeleton archer sight radius (good vision for ranged)
pub const SKELETON_ARCHER_SIGHT_RADIUS: i32 = 10;
/// Skeleton archer strength
pub const SKELETON_ARCHER_STRENGTH: i32 = 6;
/// Skeleton archer intelligence
pub const SKELETON_ARCHER_INTELLIGENCE: i32 = 3;
/// Skeleton archer agility
pub const SKELETON_ARCHER_AGILITY: i32 = 5;
/// Skeleton archer melee damage (weak, prefers ranged)
pub const SKELETON_ARCHER_MELEE_DAMAGE: i32 = 3;
/// Skeleton archer bow damage
pub const SKELETON_ARCHER_BOW_DAMAGE: i32 = 8;
/// Minimum range for skeleton archer to use bow (won't shoot if closer)
pub const SKELETON_ARCHER_MIN_RANGE: i32 = 2;
/// Maximum range for skeleton archer bow
pub const SKELETON_ARCHER_MAX_RANGE: i32 = 8;

/// Gold dropped by enemies (min)
pub const ENEMY_GOLD_DROP_MIN: u32 = 1;
/// Gold dropped by enemies (max)
pub const ENEMY_GOLD_DROP_MAX: u32 = 10;
