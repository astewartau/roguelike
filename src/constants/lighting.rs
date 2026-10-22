//! Lighting constants: light colours, flicker shaping, falloff and occlusion.
//!
//! These drive the per-frame light map built in `systems::light` (which the
//! tile shader in src/renderer.rs samples as a texture) and the gameplay light
//! the AI reads. Colours live here rather than in the shader source so a
//! retune is a recompile of this file, not a rewrite of GLSL.

// =============================================================================
// FALLOFF AND OCCLUSION (see systems/light.rs)
// =============================================================================

/// The player's own light: what you see by in the dark, not a lamp (enemies
/// don't count it, see systems/light.rs). Brightness at the edge of your
/// sight, and everywhere you can see. Lower makes unlit areas darker, which
/// is what gives lights their contrast.
pub const LIGHT_PLAYER_AMBIENT: f32 = 0.12;
/// Extra brightness of the pool around the player, falling off as `t^2` over
/// the sight radius (full at your feet, nothing at the edge).
pub const LIGHT_PLAYER_FOCUSED: f32 = 0.2;
/// The editor's lighting (while the developer tools are shown, player or no
/// player): brightness everywhere in view, even and untinted, in place of
/// the player's own light, with no pool of light anywhere. Higher shows the
/// scene more plainly; lower lets the world's own lights stand out more.
pub const LIGHT_EDITOR_AMBIENT: f32 = 0.6;
/// How remembered tiles (explored, out of sight) are lit: this share of the
/// ambient light, the sky's and your own soft light everywhere in view, but
/// never a lamp's or a fire's (you don't see those from behind a wall). So
/// remembered ground is as dark as a moonless dungeon underground and nearly
/// as bright as what you see at noon outdoors. Lower sets the edge of your
/// sight off more; 1 makes remembered ground as bright as open ground.
pub const LIGHT_FOG_SHARE: f32 = 0.8;
/// The per-sprite fog value remembered things are drawn with. Informational
/// only now: the tile shader decides what's seen from the map of how seen
/// each tile is (`render::RevealFade::seen_map`) and lights remembered places
/// by `LIGHT_FOG_SHARE`.
pub const LIGHT_FOG_MARK: f32 = 0.1;

/// Weight of a light's focused falloff (`t^2`, `t = 1 - d/r`): the bright pool
/// close to the source.
pub const LIGHT_FOCUSED_WEIGHT: f32 = 0.7;
/// Weight of a light's spread falloff (`t`): the gentle wash that carries it
/// out to its radius. Raising it (and lowering FOCUSED) flattens lights into
/// wide even pools; lowering it makes tight bright spots.
pub const LIGHT_SPREAD_WEIGHT: f32 = 0.3;
/// Fraction of light a tall-grass tile lets through. Lower makes grass a
/// deeper shade to hide in behind a fire; 0 would make it a wall.
pub const LIGHT_TALL_GRASS_TRANSMITTANCE: f32 = 0.5;
/// Fraction of light a solid object (barrel, chest, coffin, stalagmite...)
/// lets through, so it casts a soft shadow rather than a hard one.
pub const LIGHT_OBJECT_TRANSMITTANCE: f32 = 0.6;

// =============================================================================
// LIGHT COLOURS
// =============================================================================
//
// # The max-channel invariant
//
// Every colour here has at least one channel at exactly 1.0, and that is not
// cosmetic. The bloom bright pass in src/render/post.rs measures a pixel on
// `max(r, g, b)` rather than luma (see [`crate::constants::BLOOM_THRESHOLD`]),
// so a light whose brightest channel is 1.0 drives the same bloom mask a white
// light of the same intensity used to: the glow picks up the light's hue
// without the threshold needing a retune. Give a new colour a max channel
// below 1.0 and it will quietly stop blooming.
//
// Saturation is the lever for mood, and it costs brightness: the dimmer
// channels pull the lit area's luma down, so a deeply saturated light reads as
// a darker pool than a pale one. These sit at a moderate saturation on
// purpose - warm enough to read as firelight, not so warm the floor goes
// brown.

/// Firelight: campfires, braziers, burning grass, burning oil, burning webs.
///
/// A true flame colour, near the ~2100K of a real wood fire, rather than the
/// pale amber of a warm light bulb. The saturation matters more than it looks:
/// this tileset's stone is a desaturated blue-*green* grey whose green channel
/// sits well above its red, so a pale warm light pulls blue down, leaves green
/// on top, and the room goes khaki instead of warm. Red has to clearly
/// dominate for the pool to read as fire.
///
/// Push green and blue down together for a deeper, redder, dimmer ember;
/// raise them toward 1.0 and the flame washes out to white, passing back
/// through khaki on the way.
pub const LIGHT_COLOR_FIRE: (f32, f32, f32) = (1.0, 0.60, 0.30);

/// The player's own light - a neutral cream, not white.
///
/// This one multiplies the ambient floor as well as the falloff, so it tints
/// everything the player can see. It stays very close to white deliberately:
/// any real saturation here colours the whole dungeon rather than reading as a
/// light source. Pull green/blue down for a warmer lantern, push to
/// `(1.0, 1.0, 1.0)` for the old untinted look.
pub const LIGHT_COLOR_PLAYER: (f32, f32, f32) = (1.0, 0.96, 0.88);

/// Glowing cave fungus: a sickly, slightly toxic green.
///
/// The counterweight to all the orange - a cavern lit by fungus should feel
/// like a different place from a room lit by a brazier. Raise red toward 1.0
/// to make it lime and friendly; drop it for a colder, more poisonous green.
pub const LIGHT_COLOR_FUNGUS: (f32, f32, f32) = (0.55, 1.0, 0.65);

/// Cave crystal clusters: a cool magical blue.
///
/// Reads as "cold light" against the warm fire palette. Raise red and green
/// together toward 1.0 for a paler, icier glow; drop them for a deeper,
/// darker blue that lights very little.
pub const LIGHT_COLOR_CRYSTAL: (f32, f32, f32) = (0.62, 0.80, 1.0);

/// Fallback for a light whose colour was never set, or was set to something
/// unusable (a non-finite channel, or all channels at zero - which would
/// otherwise render the light as a pool of pure black).
///
/// Plain white, so an uncoloured light behaves exactly as every light did
/// before colours existed. See `LightSource::color_or_default`.
pub const LIGHT_COLOR_DEFAULT: (f32, f32, f32) = (1.0, 1.0, 1.0);

// =============================================================================
// FLICKER
// =============================================================================

/// Primary flicker frequency, in radians per second.
///
/// Roughly one cycle per second - the slow body of the flicker. Raise it for a
/// nervous, guttering flame; lower it for a lazy, breathing glow.
pub const LIGHT_FLICKER_FREQ_PRIMARY: f32 = 6.1;

/// Secondary flicker frequency, in radians per second.
///
/// Deliberately `LIGHT_FLICKER_FREQ_PRIMARY` times the golden ratio: the two
/// frequencies are incommensurable, so their sum has no period and the flicker
/// never visibly loops. Keep the ratio irrational when retuning - a simple
/// ratio like 2:1 or 3:2 gives a short, very audible-looking cycle.
pub const LIGHT_FLICKER_FREQ_SECONDARY: f32 = 9.868;

/// How much of the flicker comes from the primary sine, with the remainder
/// from the secondary.
///
/// The two weights sum to 1.0, which keeps the combined signal inside
/// `[-1, 1]` so the amplitudes below mean what they say. Move it toward 1.0
/// for a simpler, more regular pulse; toward 0.5 for a busier one.
pub const LIGHT_FLICKER_PRIMARY_WEIGHT: f32 = 0.6;

/// Multiplier applied to a light's phase seed for the secondary sine, so the
/// two sines are not merely offset by the same amount per light.
///
/// Any value that is not 1.0 works; the point is only that two lights with
/// different seeds differ in *both* sines rather than sharing a waveform.
pub const LIGHT_FLICKER_PHASE_SPREAD: f32 = 1.7;

/// Peak fractional change in a light's intensity, so `0.15` means intensity
/// swings +/-15% around its nominal value.
///
/// What this looks like on screen is roughly half the nominal figure: the
/// light is only part of a pixel's brightness, the rest being the texture and
/// the neutral ambient floor. Measured over 18 frames of static lit scenery,
/// this setting moves it about 9%, against 3.5% at the 0.06 it started on.
///
/// Raise it for a wilder, more guttering flame. Past about 0.25 the room's
/// readability starts to change frame to frame and it reads as a fault rather
/// than a fire. If the flicker makes the bloom halo pulse distractingly,
/// widen [`crate::constants::BLOOM_SOFT_KNEE`] rather than shrinking this.
pub const LIGHT_FLICKER_INTENSITY_AMPLITUDE: f32 = 0.15;

/// Peak fractional change in a light's radius, so `0.10` means the pool of
/// light breathes in and out by +/-10%.
///
/// Kept below [`LIGHT_FLICKER_INTENSITY_AMPLITUDE`] on purpose: radius
/// modulation moves the *edge* of the lit area, which is more noticeable than
/// a brightness wobble, and past about 0.2 it starts to look like the walls
/// themselves are swaying.
pub const LIGHT_FLICKER_RADIUS_AMPLITUDE: f32 = 0.10;

/// Flicker scale for firelight: the full amplitude. Fire is the thing the
/// flicker exists for.
pub const LIGHT_FLICKER_SCALE_FIRE: f32 = 1.0;

/// Flicker scale for glowing fungus - a slow, faint breath rather than a
/// guttering flame. Raise it toward 1.0 to make the fungus pulse like fire;
/// drop it to 0.0 for a dead-steady glow.
pub const LIGHT_FLICKER_SCALE_FUNGUS: f32 = 0.35;

/// Flicker scale for crystal clusters. Almost nothing - a crystal is not
/// burning, and the slight unsteadiness only stops it reading as a flat
/// cut-out.
pub const LIGHT_FLICKER_SCALE_CRYSTAL: f32 = 0.15;

/// Flicker scale for the player's own light.
///
/// Subtle, and it modulates radius only (the player light has no intensity
/// term in the shader). Enough to keep the edge of the visible area from
/// looking like a fixed circle drawn on the floor.
pub const LIGHT_FLICKER_SCALE_PLAYER: f32 = 0.5;

/// Fixed phase seed for the player's light.
///
/// The player has one light, so it needs no randomisation - but it does need a
/// phase that is not zero, or it would start every run at the exact peak of
/// both sines. Any constant off the peaks does the job.
pub const LIGHT_PLAYER_FLICKER_PHASE: f32 = 1.3;

/// How much of a light's colour reaches its flat ambient term, from `0.0`
/// (ambient stays neutral white) to `1.0` (ambient fully tinted).
///
/// Each light contributes twice: a focused quadratic falloff, which always
/// carries the light's full colour, and a wide flat term that lifts the whole
/// area around it. This dial applies only to the second.
///
/// It exists because tinting both is what makes a warm dungeon look dirty
/// rather than lit. The wide term reaches far more pixels than the focused
/// one, so at `1.0` every stone surface in the room takes the light's hue -
/// and a warm wash over blue-grey stone is khaki, not firelight. At `0.0` the
/// stone keeps its own colour and only the pools of light are tinted, which
/// reads as a torch in a grey dungeon.
///
/// `0.25` keeps a trace of warmth in the air around a brazier while leaving
/// the walls reading as stone. Past about `0.4` the khaki comes back.
pub const LIGHT_AMBIENT_TINT: f32 = 0.25;

// =============================================================================
// CARRIED AND ROOM LIGHTS (torches, lanterns)
// =============================================================================

/// Radius (tiles) of a carried or dropped torch. Bigger means torch-bearers
/// are seen from further off, and light you up from further off too.
pub const TORCH_LIGHT_RADIUS: f32 = 4.5;
/// Intensity of a torch.
pub const TORCH_LIGHT_INTENSITY: f32 = 0.85;
/// How long (game seconds) a dropped torch burns on the floor.
pub const DROPPED_TORCH_BURN_DURATION: f32 = 20.0;

/// Warm lantern (shops, storerooms): a steady, wide pool.
pub const LANTERN_LIGHT_RADIUS: f32 = 5.0;
pub const LANTERN_LIGHT_INTENSITY: f32 = 0.8;
/// Lantern flame is behind glass, so it barely wavers.
pub const LIGHT_FLICKER_SCALE_LANTERN: f32 = 0.3;
/// Lamplight: a softer, yellower warmth than open fire.
pub const LIGHT_COLOR_LANTERN: (f32, f32, f32) = (1.0, 0.82, 0.5);

/// Crypt corpse-candle: small, dim and cold, enough to show the coffins and
/// little else.
pub const GHOST_LANTERN_LIGHT_RADIUS: f32 = 4.0;
pub const GHOST_LANTERN_LIGHT_INTENSITY: f32 = 0.55;
/// Ghost-light wavers more than a lamp, less than a fire.
pub const LIGHT_FLICKER_SCALE_GHOST: f32 = 0.6;
/// Pale, cold blue-green.
pub const LIGHT_COLOR_GHOST: (f32, f32, f32) = (0.55, 0.95, 0.85);
/// Sprite tint that turns the stock lantern into a green corpse-candle.
pub const GHOST_LANTERN_TINT: (f32, f32, f32) = (0.6, 1.0, 0.8);

/// Lanterns per themed room. They go in the room's corners, so 4 is the most
/// a room can take.
pub const LANTERNS_PER_SHOP: usize = 4;
pub const LANTERNS_PER_CRYPT: usize = 2;
pub const LANTERNS_PER_STORAGE: usize = 1;

// =============================================================================
// LIGHT AS GAMEPLAY (see systems/light.rs)
// =============================================================================

/// Radius (tiles) of the glow a burning creature or barrel sheds. Bigger makes
/// anyone on fire a beacon from further off, and lights up its neighbours.
pub const BURNING_LIGHT_RADIUS: f32 = 4.0;
/// Intensity of that glow (same scale as `LightSource::intensity`).
pub const BURNING_LIGHT_INTENSITY: f32 = 0.85;

/// Light exposure at which you count as fully lit: the HUD light bar is full
/// and unaware enemies notice you at `LIT_ALERTNESS_MULT`. Below it both scale
/// down linearly to darkness. On open floor that puts a campfire's pool at
/// full within 3 tiles, ~80% at 4, ~50% at 5, gone by 8; a brazier's at full
/// within 2, ~75% at 3, ~40% at 4; one tile off glowing fungus ~75%. Lower
/// makes lights count for more, further out.
pub const LIGHT_LIT_LEVEL: f32 = 0.4;

// =============================================================================
// THE SKY (outdoor maps, see systems::sky)
// =============================================================================

/// Hours either side of sunrise and sunset over which light fades between
/// night and day, and dawn and dusk glow warm. Longer makes softer, slower
/// twilights.
pub const SKY_TWILIGHT_HOURS: f32 = 1.0;
/// Sky light at noon, added over the whole map like the player's ambient.
/// The same scale as the light map: 0.8 sits at the bloom threshold, so a
/// bright sprite glows only faintly at midday. Higher washes the art out.
pub const SKY_NOON_LEVEL: f32 = 0.8;
/// Sky light with the sun just up: the bottom of the daytime curve, which
/// climbs to `SKY_NOON_LEVEL` at noon.
pub const SKY_HORIZON_LEVEL: f32 = 0.4;
/// Sky light on a moonless night: starlight, enough to make out shapes.
pub const SKY_STARLIGHT_LEVEL: f32 = 0.05;
/// What a full moon adds to the night (scaled by the moon's brightness, 0-1).
/// Moonlight past about `LIGHT_LIT_LEVEL / 2` makes a lone figure in the
/// open fairly easy to spot.
pub const SKY_MOONLIGHT_LEVEL: f32 = 0.16;
/// How bright the moon is in a new run, 0 (new moon) to 1 (full).
pub const SKY_MOON_DEFAULT: f32 = 0.6;
/// How strongly dawn and dusk tint the light toward `SKY_COLOR_TWILIGHT`,
/// 0 (not at all) to 1 (fully, at the horizon).
pub const SKY_TWILIGHT_STRENGTH: f32 = 0.8;
/// Daylight: a warm white. One channel at 1.0, like every light colour.
pub const SKY_COLOR_DAY: (f32, f32, f32) = (1.0, 0.97, 0.9);
/// Sunrise and sunset: amber-rose.
pub const SKY_COLOR_TWILIGHT: (f32, f32, f32) = (1.0, 0.62, 0.42);
/// Moonlight and starlight: a cold blue.
pub const SKY_COLOR_NIGHT: (f32, f32, f32) = (0.55, 0.68, 1.0);
/// How much further than your own light (`FOV_RADIUS`) a full moon lets you
/// see outdoors, in tiles. In full daylight you see as far as the land lets
/// you.
pub const SKY_MOON_SIGHT_BONUS: i32 = 4;
/// How far the light of what you can see spills into remembered tiles right
/// beside it, 0 to 1: the edge of your sight blends into the fog of war over
/// a tile or two instead of stopping dead at a tile's edge. What you see
/// keeps its full light; only the remembered side is lifted. 0 is a hard
/// edge; much past 0.6 and remembered tiles by the edge look half seen.
pub const LIGHT_FOG_EDGE_SPREAD: f32 = 0.5;
