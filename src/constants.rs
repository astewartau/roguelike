//! Game constants organized by category.
//!
//! Centralizing magic numbers makes tuning easier and documents intent.

// =============================================================================
// CAMERA
// =============================================================================

/// Default zoom level (pixels per grid cell)
pub const CAMERA_DEFAULT_ZOOM: f32 = 32.0;
/// Minimum zoom level
pub const CAMERA_MIN_ZOOM: f32 = 4.0;
/// Maximum zoom level
pub const CAMERA_MAX_ZOOM: f32 = 128.0;
/// Zoom speed multiplier per scroll unit
pub const CAMERA_ZOOM_FACTOR: f32 = 1.1;
/// Smoothing factor for camera tracking (lower = smoother)
pub const CAMERA_TRACKING_SMOOTHING: f32 = 0.85;
/// Velocity damping factor (lower = more friction)
pub const CAMERA_VELOCITY_DAMPING: f32 = 0.90;
/// Velocity threshold below which camera stops
pub const CAMERA_VELOCITY_THRESHOLD: f32 = 0.001;
/// Zoom difference threshold for snapping
pub const CAMERA_ZOOM_SNAP_THRESHOLD: f32 = 0.01;
/// Momentum multiplier when releasing pan
pub const CAMERA_MOMENTUM_SCALE: f32 = 2.0;

// =============================================================================
// ANIMATION
// =============================================================================

/// Visual position lerp speed multiplier
pub const VISUAL_LERP_SPEED: f32 = 25.0;
/// Attack lunge animation speed
pub const LUNGE_ANIMATION_SPEED: f32 = 12.0;
/// Distance to lunge toward target (in tiles)
pub const LUNGE_DISTANCE: f32 = 0.5;
/// Hit flash duration in seconds
pub const HIT_FLASH_DURATION: f32 = 0.15;
/// Slash VFX duration in seconds
pub const SLASH_VFX_DURATION: f32 = 0.2;
/// Slash VFX angle (45 degrees)
pub const SLASH_VFX_ANGLE: f32 = std::f32::consts::FRAC_PI_4;

// =============================================================================
// DUNGEON GENERATION
// =============================================================================

/// Minimum size of a BSP leaf node
pub const DUNGEON_MIN_LEAF_SIZE: i32 = 10;
/// Minimum room size within a leaf
pub const DUNGEON_MIN_ROOM_SIZE: i32 = 4;
/// Margin around rooms within their leaf
pub const DUNGEON_ROOM_MARGIN: i32 = 1;
/// Default dungeon width
pub const DUNGEON_DEFAULT_WIDTH: usize = 100;
/// Default dungeon height
pub const DUNGEON_DEFAULT_HEIGHT: usize = 100;

// =============================================================================
// GAMEPLAY
// =============================================================================

/// Player's default starting health
pub const PLAYER_STARTING_HEALTH: i32 = 15;
/// Player's action speed (higher = slower, costs more energy)
pub const PLAYER_SPEED: i32 = 3;
/// Player's starting strength
pub const PLAYER_STRENGTH: i32 = 10;
/// Player's starting intelligence
pub const PLAYER_INTELLIGENCE: i32 = 8;
/// Player's starting agility
pub const PLAYER_AGILITY: i32 = 12;

/// Default FOV radius for player
pub const FOV_RADIUS: i32 = 10;

/// Base XP formula multiplier (XP needed = level * this)
pub const XP_PER_LEVEL_MULTIPLIER: u32 = 100;

/// Unarmed attack damage
pub const UNARMED_DAMAGE: i32 = 1;

// =============================================================================
// ENEMIES
// =============================================================================

/// Number of skeletons to spawn
pub const SKELETON_SPAWN_COUNT: usize = 10;
/// Skeleton health
pub const SKELETON_HEALTH: i32 = 25;
/// Skeleton action speed (lower = faster)
pub const SKELETON_SPEED: i32 = 2;
/// Skeleton sight radius for chase AI
pub const SKELETON_SIGHT_RADIUS: i32 = 8;
/// Skeleton strength
pub const SKELETON_STRENGTH: i32 = 4;
/// Skeleton intelligence
pub const SKELETON_INTELLIGENCE: i32 = 1;
/// Skeleton agility
pub const SKELETON_AGILITY: i32 = 3;
/// Skeleton attack damage
pub const SKELETON_DAMAGE: i32 = 5;

/// Gold dropped by enemies (min)
pub const ENEMY_GOLD_DROP_MIN: u32 = 1;
/// Gold dropped by enemies (max)
pub const ENEMY_GOLD_DROP_MAX: u32 = 10;

// =============================================================================
// ITEMS
// =============================================================================

/// Health potion heal amount
pub const HEALTH_POTION_HEAL: i32 = 50;
/// Health potion weight in kg
pub const HEALTH_POTION_WEIGHT: f32 = 0.5;

/// Sword base damage
pub const SWORD_BASE_DAMAGE: i32 = 8;
/// Sword damage bonus
pub const SWORD_DAMAGE_BONUS: i32 = 2;

// =============================================================================
// UI / WINDOW
// =============================================================================

/// Default window width
pub const WINDOW_DEFAULT_WIDTH: u32 = 1280;
/// Default window height
pub const WINDOW_DEFAULT_HEIGHT: u32 = 720;

/// Click drag threshold (pixels) to distinguish click from drag
pub const CLICK_DRAG_THRESHOLD: f32 = 5.0;
