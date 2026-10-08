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
