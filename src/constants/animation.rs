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

/// How long before a status effect expires that its pip starts flashing, in
/// seconds. Up gives more warning; down makes the flash a final jolt.
pub const EFFECT_PIP_FLASH_LEAD: f32 = 3.0;
/// Flash cycles per second for an about-to-expire status pip. Up is more
/// urgent and harder to ignore.
pub const EFFECT_PIP_FLASH_RATE: f32 = 3.0;
/// How far the flash lifts an expiring pip's icon and border brightness at its
/// peak, as a fraction above normal.
pub const EFFECT_PIP_FLASH_DEPTH: f32 = 0.9;

/// How long a hotbar slot's white "ready" flash lasts after its ability comes
/// off cooldown, in seconds. This is the cue that saves the player watching
/// the number; longer is more obvious, shorter easier to miss.
pub const HOTBAR_READY_FLASH_DURATION: f32 = 0.3;
/// Peak opacity of that white flash, 0-255.
pub const HOTBAR_READY_FLASH_ALPHA: u8 = 150;
/// Peak scale of the icon pop that rides along with the ready flash. The icon
/// is clipped to its slot, so much above ~1.2 just crops the sprite.
pub const HOTBAR_READY_POP_SCALE: f32 = 1.1;
/// How long a hotbar slot flashes red after the player presses its key for
/// something they cannot use, in seconds. This replaces silence, so it has to
/// be long enough to notice and short enough not to linger over a spam-press.
pub const HOTBAR_DENIED_FLASH_DURATION: f32 = 0.22;
/// Peak opacity of that red flash, 0-255.
pub const HOTBAR_DENIED_FLASH_ALPHA: u8 = 150;
/// Opacity of the glow drawn just inside a ready, affordable slot's border,
/// 0-255. This is the "stronger than a border swap" ready cue; up to make
/// usable slots shout.
pub const HOTBAR_READY_GLOW_ALPHA: u8 = 65;
/// Thickness of that ready glow, in points.
pub const HOTBAR_READY_GLOW_WIDTH: f32 = 3.0;
/// Opacity of the cooldown sweep over the un-recovered part of a slot, 0-255.
/// The sweep's *shape* now carries the information, so it does not need to be
/// as dark as the flat dim it replaces.
pub const HOTBAR_COOLDOWN_SWEEP_ALPHA: u8 = 170;

/// How long a new message-log line takes to fade and slide into place, in
/// seconds. Long enough to read as motion, short enough that a burst of
/// combat lines does not visibly queue up.
pub const LOG_LINE_ARRIVE_DURATION: f32 = 0.15;
/// How far, in points, a new log line rises from as it arrives. Up for a more
/// pronounced entrance; the line is clipped to its own row, so anything much
/// past a line height just delays its appearance.
pub const LOG_LINE_SLIDE_DISTANCE: f32 = 8.0;
/// How far above its resting colour a log line is over-brightened on arrival,
/// as a fraction. Up makes new lines flare; 0.0 removes the flare.
pub const LOG_LINE_ARRIVE_LIFT: f32 = 0.7;
/// Alpha lost per line of age in the log, so the newest line reads as newest.
/// Up fades history away faster.
pub const LOG_LINE_AGE_FADE: f32 = 0.11;
/// Floor on a faded log line's alpha, so the oldest visible line stays
/// readable instead of vanishing.
pub const LOG_LINE_MIN_ALPHA: f32 = 0.35;
/// How long the repeat-count badge flares and hops for when its counter
/// ticks, in seconds.
pub const LOG_COUNT_POP_DURATION: f32 = 0.25;
/// How far, in points, the repeat-count badge hops when it ticks.
pub const LOG_COUNT_POP_RISE: f32 = 3.0;
/// How far the count badge is over-brightened at the peak of a tick, as a
/// fraction above its line's colour.
pub const LOG_COUNT_POP_LIFT: f32 = 1.1;

/// Horizontal jitter applied to a damage number, in tiles either side of the
/// tile centre. Up spreads stacked hits further apart (and further off their
/// tile); 0.0 stacks them back into an illegible pile.
pub const DAMAGE_NUMBER_JITTER: f32 = 0.3;
/// Offset of the black outline drawn behind a floating number, in egui points.
/// The number is drawn four times at plus and minus this on each axis. Up
/// thickens the outline and starts to choke the glyphs.
pub const DAMAGE_NUMBER_OUTLINE_OFFSET: f32 = 1.0;
/// Damage at or above which a hit is drawn larger and brighter. Down tiers
/// more hits as big and flattens the distinction; up reserves the treatment
/// for genuinely heavy blows.
pub const DAMAGE_NUMBER_BIG_THRESHOLD: i32 = 10;
/// Font-size multiplier for a big hit. Up makes heavy hits dominate; 1.0
/// removes size tiering.
pub const DAMAGE_NUMBER_BIG_SCALE: f32 = 1.3;
/// Font-size multiplier for a critical hit, which also gets its own colour and
/// a trailing `!`.
pub const DAMAGE_NUMBER_CRIT_SCALE: f32 = 1.55;
/// Font-size multiplier for damage the *player* takes, so a hit landing on you
/// never reads the same as a hit you landed.
pub const DAMAGE_NUMBER_TAKEN_SCALE: f32 = 1.25;

/// Font-size multiplier for the floating "miss" text shown when a melee
/// attack lands on an empty tile because its target stepped out of reach.
/// Below 1.0 so a miss reads as quieter than any hit.
pub const MISS_TEXT_SCALE: f32 = 0.85;
/// How far the attacker leans toward its target while winding up a melee
/// attack, in tiles, at the moment the attack lands (it grows linearly with
/// the wind-up's progress). A cue that the swing is coming, not the swing
/// itself, so keep it well under `LUNGE_DISTANCE`; 0.0 disables the lean.
pub const ATTACK_TELEGRAPH_LEAN: f32 = 0.15;

// =============================================================================
// HIT-STOP
// =============================================================================

/// Real seconds visual animation freezes for when the player lands a critical
/// hit. A hit-stop is presentation only: the simulation never sees it. Longer
/// makes crits feel weightier but starts to read as a stutter.
pub const HITSTOP_CRIT: f32 = 0.05;
/// Real seconds of hit-stop when the player (or one of their companions)
/// kills something. Slightly longer than a crit so the finishing blow lands.
pub const HITSTOP_KILL: f32 = 0.08;
/// Real seconds of hit-stop when a boss's ground slam hits the player: the
/// heaviest blow in the game gets the longest freeze.
pub const HITSTOP_HEAVY: f32 = 0.1;
/// Fraction of real time that still reaches visual animation during a
/// hit-stop. 0.0 is a dead freeze; a little above zero is a slow-motion
/// crawl instead.
pub const HITSTOP_TIME_SCALE: f32 = 0.0;
