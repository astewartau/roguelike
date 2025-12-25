//! Animation-related constants.

/// Visual position lerp speed multiplier
pub const VISUAL_LERP_SPEED: f32 = 15.0;
/// Maximum delta time for animations (prevents snapping after long frames)
pub const MAX_ANIMATION_DT: f32 = 0.025; // 50ms cap (~20 FPS minimum)
/// Attack lunge animation speed
pub const LUNGE_ANIMATION_SPEED: f32 = 12.0;
/// Distance to lunge toward target (in tiles)
pub const LUNGE_DISTANCE: f32 = 0.5;
/// Slash VFX duration in seconds
pub const SLASH_VFX_DURATION: f32 = 0.2;
/// Slash VFX angle (45 degrees)
pub const SLASH_VFX_ANGLE: f32 = std::f32::consts::FRAC_PI_4;
/// Damage number duration in seconds
pub const DAMAGE_NUMBER_DURATION: f32 = 0.8;
/// How high damage numbers rise (in tiles)
pub const DAMAGE_NUMBER_RISE: f32 = 1.0;
