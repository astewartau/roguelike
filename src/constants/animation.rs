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
/// Base font size for damage/heal numbers, in egui points. The number is
/// scaled from this by `ease::pop` as it rises.
pub const DAMAGE_NUMBER_FONT_SIZE: f32 = 20.0;
/// Step, in egui points, that the popped damage-number size is rounded to.
/// egui rasterizes and caches glyphs per distinct font size, so a size that
/// varied continuously would add a new set to the font atlas every frame.
/// Rounding bounds that to a handful of sizes. Smaller is smoother and costs
/// more atlas space; larger is coarser and cheaper.
pub const DAMAGE_NUMBER_POP_SIZE_STEP: f32 = 0.5;

// =============================================================================
// HIT FLASH
// =============================================================================

/// How long a sprite stays flashed after taking damage, in seconds. Longer
/// makes each hit read as heavier, but hits smear together under rapid fire;
/// shorter is snappier and easier to miss entirely.
pub const HIT_FLASH_DURATION: f32 = 0.08;
/// Tint a flashing sprite is lifted toward. The tint multiplies the texture,
/// so values above 1.0 overbrighten and blow the sprite out toward white.
/// Lower it for a subtler flash, raise it for a harsher one.
pub const HIT_FLASH_TINT: (f32, f32, f32) = (2.6, 2.6, 2.6);
/// Tint the *player* is lifted toward instead, so taking a hit reads
/// differently from landing one. Red-dominant: raise the red channel to make
/// damage more alarming, raise green/blue to desaturate it back toward white.
pub const HIT_FLASH_PLAYER_TINT: (f32, f32, f32) = (2.6, 0.4, 0.4);

// =============================================================================
// CAMERA SHAKE
// =============================================================================

/// Fraction of the shake amplitude left after one second of real time (the
/// decay is exponential: `amplitude *= DECAY^dt`). Smaller is a sharper knock
/// that dies quickly; larger rings on.
pub const CAMERA_SHAKE_DECAY: f32 = 0.002;
/// Shake oscillations per second. Higher is a buzzy rattle, lower a slow
/// lurch.
pub const CAMERA_SHAKE_FREQUENCY: f32 = 22.0;
/// Multiplier on the vertical frequency, so x and y never stay in phase.
/// At exactly 1.0 the offset would trace a straight diagonal line instead of
/// a rattle; keep it irrational-ish.
pub const CAMERA_SHAKE_FREQUENCY_Y_RATIO: f32 = 1.37;
/// Ceiling on shake amplitude, in tiles. Stacked events cannot exceed this,
/// so the view can never be thrown off the player. Keep it below 1.0: the
/// renderer's `get_visible_bounds` pads by a single tile, and a shake larger
/// than that padding would let unculled edge tiles pop in and out.
pub const CAMERA_SHAKE_MAX_AMPLITUDE: f32 = 0.45;
/// Amplitude in tiles below which a shake is treated as finished and snapped
/// to zero, so the camera does not jitter imperceptibly forever.
pub const CAMERA_SHAKE_CUTOFF: f32 = 0.002;
/// Distance in tiles over which a shake with a world origin (an explosion, a
/// boss ability) falls off to nothing. Larger makes distant booms carry
/// further.
pub const CAMERA_SHAKE_FALLOFF_DISTANCE: f32 = 14.0;
/// Shake amplitude in tiles for a hit that costs the player their entire max
/// HP; a real hit is scaled by the fraction of max HP it actually took. Up
/// for a more punishing feel.
pub const CAMERA_SHAKE_PLAYER_HIT: f32 = 0.55;
/// Floor on a player-hit shake, so even a 1-damage scratch registers as a
/// nudge rather than nothing at all.
pub const CAMERA_SHAKE_PLAYER_HIT_MIN: f32 = 0.04;
/// Shake amplitude in tiles for an explosion (fireball, oil barrel) centred
/// on the camera, before distance falloff.
pub const CAMERA_SHAKE_EXPLOSION: f32 = 0.35;
/// Shake amplitude in tiles for a boss ability, before distance falloff.
pub const CAMERA_SHAKE_BOSS_ABILITY: f32 = 0.30;
/// Shake amplitude in tiles for the player's own heavy swing. Deliberately
/// small and directional — a kick along the swing, not a rattle — so the
/// player's own attacks never fight their aim.
pub const CAMERA_SHAKE_PLAYER_SWING: f32 = 0.12;

// =============================================================================
// HUD ANIMATION
// =============================================================================
// These are paced by *real* time, not game time, for the same reason camera
// shake is (see the note in `engine::tick` beside `camera.update`): the HUD is
// presentation, nothing in the simulation can be reached from it, and the game
// clock stops to wait for input — which is precisely when the player is
// reading the HUD. Pacing these by game time would freeze them mid-animation.

/// How long the HP chip ("ghost") bar holds station after a hit before it
/// starts draining, in seconds. The pause is what makes the lost chunk
/// readable; too long and the bar feels laggy.
pub const HP_CHIP_HOLD: f32 = 0.12;
/// How long the HP chip bar takes to catch up to the real HP once it starts
/// draining, in seconds. Longer reads as a heavier wound and lets rapid hits
/// overlap; shorter approaches no chip bar at all.
pub const HP_CHIP_DRAIN_DURATION: f32 = 0.4;
/// HP fraction below which the HP bar pulses. Up makes the warning start
/// earlier (and nag more); down makes it a last-gasp signal only.
pub const HP_LOW_PULSE_THRESHOLD: f32 = 0.25;
/// Full bright-dim-bright cycles per second of the low-HP pulse. Up is a
/// panicky flutter, down a slow heartbeat.
pub const HP_LOW_PULSE_RATE: f32 = 1.3;
/// How far the low-HP pulse lifts the fill brightness at its peak, as a
/// fraction above normal. Up is more alarming, 0.0 disables the pulse.
pub const HP_LOW_PULSE_DEPTH: f32 = 0.55;

/// How long a hunger or fatigue bar takes to slide into place once it starts
/// mattering, in seconds. The bar appearing at all *is* the warning, so the
/// slide has to read as motion — too fast and it looks like a layout glitch.
pub const SURVIVAL_BAR_SLIDE_DURATION: f32 = 0.35;
