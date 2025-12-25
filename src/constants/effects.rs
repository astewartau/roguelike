//! Status effect durations and parameters.

/// Duration of invisibility effect in game-time seconds
pub const INVISIBILITY_DURATION: f32 = 60.0;

/// Duration of speed boost effect in game-time seconds
pub const SPEED_BOOST_DURATION: f32 = 45.0;
/// Speed multiplier when speed boost is active (2.0 = twice as fast)
pub const SPEED_BOOST_MULTIPLIER: f32 = 2.0;

/// Duration of regeneration effect in game-time seconds
pub const REGENERATION_DURATION: f32 = 60.0;
/// HP regenerated per tick when Regenerating effect is active
pub const REGENERATION_BOOST_AMOUNT: i32 = 3;
/// Seconds between regen ticks when Regenerating effect is active
pub const REGENERATION_BOOST_INTERVAL: f32 = 3.0;

/// Duration of strength effect in game-time seconds
pub const STRENGTH_DURATION: f32 = 45.0;
/// Damage multiplier when Strengthened effect is active
pub const STRENGTH_DAMAGE_MULTIPLIER: f32 = 1.5;

/// Duration of protection effect in game-time seconds
pub const PROTECTION_DURATION: f32 = 60.0;
/// Damage reduction multiplier when Protected effect is active (0.5 = 50% reduction)
pub const PROTECTION_DAMAGE_REDUCTION: f32 = 0.5;

/// Duration of confusion effect in game-time seconds
pub const CONFUSION_DURATION: f32 = 30.0;

/// Duration of fear effect in game-time seconds
pub const FEAR_DURATION: f32 = 45.0;

/// Duration of slow effect in game-time seconds
pub const SLOW_DURATION: f32 = 45.0;
/// Speed multiplier when Slowed effect is active (0.5 = half speed)
pub const SLOW_MULTIPLIER: f32 = 0.5;

/// Maximum teleport range for Blink scroll
pub const BLINK_RANGE: i32 = 8;
/// Maximum casting range for Fireball scroll
pub const FIREBALL_RANGE: i32 = 10;
/// Explosion radius for Fireball scroll
pub const FIREBALL_RADIUS: i32 = 2;
/// Damage dealt by Fireball scroll
pub const FIREBALL_DAMAGE: i32 = 25;
/// Duration of Scroll of Reveal effect (game-time seconds)
pub const REVEAL_DURATION: f32 = 10.0;
/// Radius around each enemy revealed by Scroll of Reveal
pub const REVEAL_RADIUS: i32 = 3;
