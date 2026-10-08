//! UI and window constants.

/// Default window width
pub const WINDOW_DEFAULT_WIDTH: u32 = 1280;
/// Default window height
pub const WINDOW_DEFAULT_HEIGHT: u32 = 720;

/// Click drag threshold (pixels) to distinguish click from drag
pub const CLICK_DRAG_THRESHOLD: f32 = 5.0;

// =============================================================================
// PANEL CHROME
// =============================================================================
// Detail density on top of the flat panel style, not a replacement for it:
// the panels stay square-cornered, hard-bordered and brown, they just stop
// reading as unstyled rectangles. See `src/ui/style.rs` for the painting and
// `style::colors` for the colours these numbers shape.

/// How far down-right the panel drop shadow falls, in points. The shadow has
/// no blur on purpose — a hard offset is what a pixel-art game's panels cast,
/// and a soft penumbra is the modern look this style is avoiding. Raising
/// this lifts panels further off the dungeon; past about 6 they stop reading
/// as slabs sitting on the floor and start reading as floating cards.
pub const PANEL_SHADOW_OFFSET: f32 = 4.0;

/// How much lighter than `PANEL_BG` the top/left inner bevel line is, per
/// channel. This is what makes a panel read as lit from the top-left. Too
/// high and the line stops looking like a lit edge and starts looking like a
/// highlight stuck onto a flat shape.
pub const PANEL_BEVEL_LIGHT_DELTA: u8 = 24;

/// How much darker than `PANEL_BG` the bottom/right inner bevel line is, per
/// channel. Deliberately smaller than the light delta: `PANEL_BG` is already
/// nearly black, so there is far less room to darken than to lighten, and
/// matching the two just loses the bottom edge entirely.
pub const PANEL_BEVEL_DARK_DELTA: u8 = 12;

/// Edge length in pixels of the generated noise tile laid over panel
/// backgrounds. The tile repeats, so this sets how long the mottling takes to
/// visibly recur, and costs `PANEL_TEXTURE_SIZE²` RGBA pixels of texture.
pub const PANEL_TEXTURE_SIZE: usize = 64;

/// Peak deviation of the panel texture from `PANEL_BG`, in colour levels per
/// channel. The texture is opaque — it *is* the panel background, mottled —
/// rather than a translucent overlay, because `PANEL_BG` is so dark that the
/// smallest alpha an 8-bit overlay can express already lands around +13
/// levels, which is far past "barely perceptible". Working in levels makes
/// what lands on screen exactly what is written here. At 3 the texture is
/// felt as the background not being perfectly flat; by about 8 it is visible
/// as noise and fights the flat style.
pub const PANEL_TEXTURE_AMPLITUDE: u8 = 3;

/// How many noise periods fit across one panel texture tile. Higher is finer,
/// busier mottling; below about 2 the whole tile is one soft blob and reads as
/// uneven lighting rather than texture.
pub const PANEL_TEXTURE_FREQUENCY: f64 = 3.0;

/// Seed for the panel noise. Fixed rather than run-derived so every launch
/// gets the same panel texture — this is part of the art, not a dungeon.
pub const PANEL_TEXTURE_SEED: u32 = 0x5713;

/// Seconds a panel takes to scale and fade in when it opens. Short enough to
/// feel like the panel responded instantly rather than animated; much past
/// 0.2 and opening the inventory starts to feel like waiting for it.
pub const PANEL_ENTRY_DURATION: f32 = 0.12;

/// Scale a panel starts its entry animation at. Just under 1.0 — the point is
/// a hint of movement, and `ease::out_back` overshoots a hair past full size
/// on the way in. Lower values read as a zoom rather than a settle.
pub const PANEL_ENTRY_SCALE: f32 = 0.95;

/// Size in points of the corner rivet sprite. The source tile is 32px, so
/// only exact integer reductions keep it on the pixel grid — 8 is 4:1, and
/// anything else gives the rivet the smeared look this project avoids
/// everywhere else. It also has to fit inside `PANEL_INNER_MARGIN`, or the
/// rivets sit on top of the contents of a narrow panel like the status bar.
pub const PANEL_RIVET_SIZE: f32 = 8.0;

/// How far inside the panel border the corner rivets sit, in points. Kept at
/// zero so the rivet stays within the panel's inner margin and never reaches
/// the contents; it reads as a stud through the frame rather than something
/// floating inside it.
pub const PANEL_RIVET_INSET: f32 = 0.0;

/// Vertical space in points reserved for the hairline rule under a panel
/// header, the rule itself centred in it. Sets how far the rule sits from
/// both the title above and the content below.
pub const PANEL_HEADER_RULE_HEIGHT: f32 = 6.0;

// =============================================================================
// HUD RESOURCE BARS
// =============================================================================
// Geometry for the hand-painted HP / XP / hunger / fatigue bars in
// `src/ui/status_bar.rs`. Their timings live in `constants::animation`.

/// Width of a resource bar, in egui points. The status window is sized around
/// this plus the 16pt icon gutter, so widening it widens the window.
pub const HUD_BAR_WIDTH: f32 = 180.0;
/// Height of a resource bar, in egui points. Tall enough for the inset recess,
/// the fill and a 12pt label; shrinking it below ~14 clips the text.
pub const HUD_BAR_HEIGHT: f32 = 18.0;
/// Thickness of the inset bevel drawn inside a bar's frame, in points. One
/// pixel reads as a recess at any sane DPI; more starts to look like a border.
pub const HUD_BAR_BEVEL: f32 = 1.0;
/// Width of the brighter leading edge drawn at the fill boundary, in points.
/// This is the "designed" tell — up to 3 makes it a highlight band, down to 1
/// and it disappears into the fill.
pub const HUD_BAR_LEADING_EDGE_WIDTH: f32 = 2.0;
/// Brightness multiplier applied to the fill colour to get the leading edge.
/// Up for a hotter edge; 1.0 switches the effect off without removing it.
pub const HUD_BAR_LEADING_EDGE_LIFT: f32 = 1.7;
/// Max HP covered by one segment notch, so the bar is countable at a glance.
/// Smaller means more notches (finer counting, busier bar).
pub const HUD_BAR_HP_PER_NOTCH: i32 = 10;
/// Ceiling on how many notches a bar draws, so a very large max HP degrades to
/// a coarse scale rather than a solid hatched block.
pub const HUD_BAR_MAX_NOTCHES: i32 = 20;
/// Opacity of a segment notch, 0-255. Up makes the bar read as segmented
/// chunks, down as a continuous bar with faint guides.
pub const HUD_BAR_NOTCH_ALPHA: u8 = 90;
/// Font size for a bar's inline label, in points. Must stay comfortably under
/// [`HUD_BAR_HEIGHT`] or the text clips.
pub const HUD_BAR_FONT_SIZE: f32 = 12.0;
/// Horizontal gap between a bar's icon and the bar itself, in points.
pub const HUD_BAR_ICON_GAP: f32 = 4.0;
/// Size of the icon in a bar's gutter, in points.
pub const HUD_BAR_ICON_SIZE: f32 = 16.0;

/// Inner width of the status window, in points: the icon gutter plus
/// [`HUD_BAR_WIDTH`]. Set explicitly because the window otherwise auto-sizes
/// to its content and would jump about as status labels come and go.
pub const HUD_STATUS_WIDTH: f32 = HUD_BAR_ICON_SIZE + HUD_BAR_ICON_GAP + HUD_BAR_WIDTH;

// =============================================================================
// STATUS EFFECT PIPS
// =============================================================================

/// Side length of a status-effect icon pip, in points. The radial cooldown
/// sweep covers the whole pip, so bigger pips make the sweep readable at the
/// cost of HUD width.
pub const EFFECT_PIP_SIZE: f32 = 24.0;
/// Gap between adjacent status-effect pips, in points.
pub const EFFECT_PIP_SPACING: f32 = 4.0;
/// Number of triangles the radial cooldown sweep is approximated with. Up is
/// smoother and costs more geometry; below about 12 the arc visibly facets.
pub const EFFECT_PIP_SWEEP_SEGMENTS: usize = 24;
/// Opacity of the sweep covering the spent part of an effect, 0-255. Up dims
/// an almost-expired pip harder.
pub const EFFECT_PIP_SWEEP_ALPHA: u8 = 165;
/// Font size for the remaining-seconds readout on a pip, in points.
pub const EFFECT_PIP_FONT_SIZE: f32 = 10.0;

/// Minimum width of the message-log panel, in points. Wide enough that a
/// typical combat line does not wrap, so lines keep a stable height.
pub const LOG_WIDTH: f32 = 360.0;
/// Gap between a log line's text and its "(xN)" repeat badge, in points.
pub const LOG_COUNT_GAP: f32 = 6.0;

// =============================================================================
// LOOT MARKERS
// =============================================================================
// The small diamond floated over a corpse or item pile that still holds
// something. See `ui::vfx::draw_loot_indicators`.

/// Half-width of the loot marker diamond, in points. Kept small on purpose:
/// it is a hint that a tile is worth stepping on, not a label. Past about 5
/// it starts competing with enemy health bars for attention.
pub const LOOT_MARKER_HALF_SIZE: f32 = 3.5;
/// Height of the marker above the tile's bottom edge, in tiles. 1.0 sits it
/// on the tile's top edge; up floats it higher over the sprite.
pub const LOOT_MARKER_HEIGHT: f32 = 1.05;
/// How far the marker bobs up and down, in points. 0.0 holds it still; much
/// past 2 the movement pulls the eye across the whole screen.
pub const LOOT_MARKER_BOB_AMPLITUDE: f32 = 1.5;
/// Bob speed, in radians per second of wall-clock time. Purely presentation,
/// so it runs off the UI clock rather than game time.
pub const LOOT_MARKER_BOB_SPEED: f32 = 2.5;
/// Opacity of the marker, 0-255. Down makes it fade into the floor.
pub const LOOT_MARKER_ALPHA: u8 = 200;
