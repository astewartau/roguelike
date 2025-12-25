//! Time system constants.

/// Base duration for walking one tile (seconds)
pub const ACTION_WALK_DURATION: f32 = 1.0;
/// Base duration for attacking (seconds)
pub const ACTION_ATTACK_DURATION: f32 = 0.8;
/// Base duration for opening a door (seconds)
pub const ACTION_DOOR_DURATION: f32 = 0.5;
/// Base duration for opening/interacting with a chest (seconds)
pub const ACTION_CHEST_DURATION: f32 = 0.5;
/// Base duration for waiting/passing (seconds)
pub const ACTION_WAIT_DURATION: f32 = 0.5;
/// Multiplier for diagonal movement duration (sqrt(2))
pub const DIAGONAL_MOVEMENT_MULTIPLIER: f32 = 1.414;
/// Duration for shooting action (draw + aim + recovery)
pub const ACTION_SHOOT_DURATION: f32 = 1.2;
