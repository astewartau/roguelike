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
/// Duration for shooting action (draw + aim + release)
pub const ACTION_SHOOT_DURATION: f32 = 1.2;
/// Duration for recovery after shooting (allows arrow to fly)
pub const ACTION_RECOVER_DURATION: f32 = 0.25;
/// Base duration for pushing a piece of furniture (a barrel) one tile. A
/// little quicker than a step, slower than opening a door. Up makes shoving
/// barrels into a fight a bigger commitment.
pub const ACTION_PUSH_DURATION: f32 = 0.8;

// =============================================================================
// TIME OF DAY (see systems::sky)
// =============================================================================

/// Game seconds in one day and night. At 1440 a game second is a minute of
/// the day: a step takes a minute, a full day about 1400 steps (or a minute
/// of sleeping, at the fast-forward speed). Lower makes days pass quicker.
pub const DAY_LENGTH: f32 = 1440.0;
/// The hour (0-24) a new run or sandbox starts at.
pub const DAY_START_HOUR: f32 = 8.0;
/// When the sun is half risen and half set, in hours (0-24), unless the
/// developer tools change them. Dawn and dusk spread `SKY_TWILIGHT_HOURS`
/// either side.
pub const SUNRISE_HOUR: f32 = 6.0;
pub const SUNSET_HOUR: f32 = 19.0;
