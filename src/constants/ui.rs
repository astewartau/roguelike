//! UI and window constants.

/// Default window width
pub const WINDOW_DEFAULT_WIDTH: u32 = 1280;
/// Default window height
pub const WINDOW_DEFAULT_HEIGHT: u32 = 720;

/// Click drag threshold (pixels) to distinguish click from drag
pub const CLICK_DRAG_THRESHOLD: f32 = 5.0;

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
