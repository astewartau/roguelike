//! Post-processing constants: tuning for the fullscreen bloom and vignette pass.
//!
//! These drive `src/render/post.rs`. Nothing here is baked into the shader
//! sources — every value is uploaded as a uniform, so tuning is a recompile of
//! this file rather than a rewrite of GLSL.

/// Brightness at which a pixel starts feeding the bloom, measured on its
/// brightest colour channel (max of R/G/B, not luma, so saturated orange fire
/// is not under-read the way a luma weighting would read it).
///
/// What sits above this in practice: fire, because the fire shader blends
/// additively and overlapping flames stack past 1.0; lit brazier and campfire
/// sprites, because the tile shader lets a pixel inside a light pool reach a
/// brightness of 1.5 and only clamps there; and light-coloured sprites
/// standing in that light. Mid-grey stone floor lands around 0.5 even fully
/// lit, so it stays out of the bloom at this setting.
///
/// Lower it toward 0.5 to haze the whole lit area, which reads as torchlight
/// in the air but softens the tiles; raise it past 1.0 to restrict the glow to
/// genuinely overbright pixels - essentially fire and nothing else.
pub const BLOOM_THRESHOLD: f32 = 0.8;

/// Width of the soft ramp below [`BLOOM_THRESHOLD`], as a fraction of the
/// threshold, so a pixel fades into the bloom instead of popping in the instant
/// it crosses the line.
///
/// Raise it for a gentler onset — worth doing if flickering flames make the
/// glow pulse — at the cost of a slightly hazier image overall. Drop it to 0.0
/// for a hard cut-off.
pub const BLOOM_SOFT_KNEE: f32 = 0.5;

/// Divisor applied to the screen size to get the blur buffer size, so `4` means
/// the bloom is blurred at quarter resolution on each axis.
///
/// This is the main cost/width lever: a bigger divisor is cheaper and widens
/// the glow for free (each blur tap covers more screen), but past about 8 the
/// low-resolution buffer starts to shimmer as the camera pans. Must be at least
/// 1; `1` blurs at full resolution, which is sharp, slow and barely reads as
/// bloom.
pub const BLOOM_DOWNSAMPLE: u32 = 4;

/// Spacing between the blur's taps, in texels of the (downsampled) blur buffer.
///
/// The blur is a fixed 9-tap Gaussian, so this is what sets its radius: the
/// glow reaches roughly `4 * BLOOM_BLUR_RADIUS * BLOOM_DOWNSAMPLE` pixels from
/// a bright source. Raise it for a wider, softer halo; past ~2.5 the taps are
/// far enough apart that the Gaussian starts to show as distinct rings rather
/// than a smooth falloff.
pub const BLOOM_BLUR_RADIUS: f32 = 1.5;

/// How much of the blurred bright-pass is added back on top of the scene.
///
/// `0.0` disables bloom without removing the pass. Raise it for a hotter,
/// hazier dungeon; past ~2.0 fire stops reading as fire and washes the tiles
/// around it out to white.
pub const BLOOM_INTENSITY: f32 = 1.0;

/// How dark the screen corners get, from `0.0` (vignette off) to `1.0` (corners
/// crushed to black).
///
/// The point is to stop the unlit area reading as a black rectangle the dungeon
/// sits in, so this wants to stay subtle — much past 0.5 it reads as a dirty
/// lens instead of as falloff, and starts hiding entities at the screen edge.
pub const VIGNETTE_STRENGTH: f32 = 0.35;

/// Where the darkening begins, as a fraction of the distance from the centre of
/// the screen to a corner: `0.0` starts it at the centre, `1.0` confines it to
/// the very corners.
///
/// Lower it to pull the falloff inward and tighten the apparent lit area; raise
/// it to keep more of the screen at full brightness.
pub const VIGNETTE_RADIUS: f32 = 0.55;
