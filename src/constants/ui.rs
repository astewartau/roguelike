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

/// Starting width of the message-log panel, in points (it can be resized).
/// Wide enough that a typical combat line does not wrap.
pub const LOG_WIDTH: f32 = 360.0;
/// How far the message log's top fade reaches, in lines: while the cursor
/// isn't over the log, lines fade to nothing over this much of its top.
pub const LOG_TOP_FADE_LINES: f32 = 2.5;
/// How far around the message log the cursor still counts as over it, in
/// points: enough to reach the resize edges just outside it.
pub const LOG_HOVER_MARGIN: f32 = 8.0;
/// The narrowest and shortest the message log can be resized to, in points.
pub const LOG_MIN_SIZE: [f32; 2] = [180.0, 40.0];
/// The widest the message log can be resized to, as a share of the screen's
/// width, and the tallest, as a share of its height.
pub const LOG_MAX_SCREEN_SHARE: [f32; 2] = [0.6, 0.7];
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

// =============================================================================
// ATTACK TELEGRAPHS
// =============================================================================
// The red marker drawn on a tile a hostile is about to hit: a melee swing's
// target, or every tile under a boss ground slam. Progress is measured in game
// time, so the marker is frozen while the player decides. See
// `ui::vfx::draw_attack_telegraphs`.

/// Base colour of a telegraph, RGB. Red because it is a threat to the
/// player; keep it clearly apart from the gold loot marker and the green
/// heal numbers.
pub const ATTACK_TELEGRAPH_COLOR: (u8, u8, u8) = (225, 40, 30);
/// Fill opacity (0-255) of a telegraph whose attack has only just started.
/// Up makes fresh swings louder; at 0 a new swing shows only its outline.
pub const ATTACK_TELEGRAPH_ALPHA_MIN: u8 = 25;
/// Fill opacity (0-255) of a telegraph whose attack is about to land. Past
/// about 180 it hides the sprite standing on the tile.
pub const ATTACK_TELEGRAPH_ALPHA_MAX: u8 = 140;
/// Opacity (0-255) of the tile outline, which is drawn at full size from the
/// first moment so the threatened tile is legible before the fill grows.
pub const ATTACK_TELEGRAPH_OUTLINE_ALPHA: u8 = 170;
/// Width of the tile outline, in points.
pub const ATTACK_TELEGRAPH_OUTLINE_WIDTH: f32 = 1.5;
/// Fraction of the tile the growing fill starts at. The fill scales from this
/// up to the whole tile as the attack progresses; 0.0 grows it from a point.
pub const ATTACK_TELEGRAPH_MIN_FILL: f32 = 0.25;
/// Multiplier on the fill opacity for the ground-slam area, which covers up
/// to 25 tiles at once; under 1.0 so the area does not drown the room.
pub const SLAM_TELEGRAPH_ALPHA_SCALE: f32 = 0.75;
/// Font size of the "0.3s" time-until-it-lands label, in points.
pub const ATTACK_TELEGRAPH_LABEL_FONT_SIZE: f32 = 12.0;
/// Where the label sits, in tiles above the threatened tile's bottom edge.
/// 0.5 centres it on the tile; lower tucks it under the sprite's feet.
pub const ATTACK_TELEGRAPH_LABEL_HEIGHT: f32 = 0.15;

// =============================================================================
// HOVERED TILE: HIGHLIGHT, INFO PANEL, CONTEXT MENU
// =============================================================================
// The faint lightening of the tile under the cursor (drawn by the tile
// renderer, see `HOVER_TILE_BRIGHTEN`), the "what's here" panel
// in the top-right corner and the Shift+right-click action menu. See
// `ui::tile_info` for the drawing and `systems::tile_context` for what they
// say.

/// Brightness multiplier for the tile under the cursor, applied by the tile
/// renderer to the terrain sprite (1.0 = unchanged). It lightens the tile
/// itself rather than painting over it, so it scales with the tile's own
/// lighting: a lit floor gets a little lighter, a fogged one barely changes,
/// and unexplored tiles (never drawn) are never marked. Up makes the cursor
/// easier to spot; past about 1.3 it starts to read as a selection.
pub const HOVER_TILE_BRIGHTEN: f32 = 1.12;

/// Width of the tile info panel's contents, in points. Fixed so the panel
/// does not jump about as the cursor sweeps over tiles with longer names;
/// long lines wrap instead.
pub const TILE_INFO_WIDTH: f32 = 220.0;
/// Gap between the tile info panel and the top-right screen corner, in
/// points. Matches the status bar's 10pt inset in the opposite corner.
pub const TILE_INFO_MARGIN: f32 = 10.0;
/// Height of the small HP bar under each creature in the info panel, in
/// points. Much thinner than the HUD bars: it is a gauge, not a readout.
pub const TILE_INFO_HP_BAR_HEIGHT: f32 = 6.0;
/// How many distinct item names an item pile lists before "+N more". Up lists
/// more of a big pile at the cost of a taller panel.
pub const TILE_INFO_ITEM_NAMES_SHOWN: usize = 3;

/// Minimum width of the right-click menu, in points, so short menus ("Walk
/// here", "Examine") still look like a menu rather than a tooltip.
pub const CONTEXT_MENU_MIN_WIDTH: f32 = 150.0;
/// Offset of the right-click menu from the cursor, in points, so the cursor
/// does not sit on top of the first entry's label.
pub const CONTEXT_MENU_CURSOR_OFFSET: f32 = 4.0;

/// Starting width of a developer tools panel (the one a rail icon opens), in
/// egui points. It floats over the map beside the rail, as tall as what it
/// shows, and can be made wider or narrower.
pub const DEVTOOLS_PANEL_WIDTH: f32 = 340.0;
/// The narrowest a developer tools panel can be made, in egui points.
pub const DEVTOOLS_PANEL_MIN_WIDTH: f32 = 260.0;
/// Gap between the developer tools' bars and panels and the screen's edge,
/// and between each other, in egui points.
pub const DEVTOOLS_EDGE_GAP: f32 = 8.0;
/// Size of one square icon button on the tool rail, in egui points.
pub const DEVTOOLS_RAIL_BUTTON: f32 = 42.0;
/// How long the toolbar's status line ("Placed 3 Goblins", "Can't move it
/// there") stays up, in real seconds.
pub const DEVTOOLS_STATUS_SECONDS: f32 = 4.0;
/// Widest the toolbar's status line ("Placed 3 Goblins") gets before it's
/// cut short, in egui points (the whole line shows on hover).
pub const DEVTOOLS_STATUS_MAX_WIDTH: f32 = 240.0;
/// Opacity of the cursor's ghost preview of what a dev tool will place
/// (0-255). Enough to see where it lands, little enough to see under it.
pub const DEVTOOLS_GHOST_ALPHA: u8 = 150;
/// Most Wait steps the developer tools' effects bench takes to let a shot
/// it fired land (a stray arrow can fly a long way). Each is half a second
/// of game time.
pub const DEV_MAX_SHOT_WAITS: usize = 16;

/// The speeds the developer tools' world clock offers, in game seconds per
/// real second.
pub const DEV_CLOCK_SPEEDS: [f32; 5] = [0.25, 0.5, 1.0, 2.0, 4.0];
/// Most game time the world clock catches up in one frame. A long frame (a
/// hitch, the window dragged) would otherwise be simulated all at once;
/// lower keeps the world from lurching, higher keeps high speeds honest on
/// slow frames.
pub const DEV_CLOCK_MAX_STEP: f32 = 0.5;

// =============================================================================
// MENUS (start, pause, game over)
// =============================================================================
// Shared by the full-screen menus in `src/ui/start_screen.rs`,
// `pause_screen.rs` and `game_over_screen.rs`, painted through the helpers in
// `src/ui/style.rs` and `src/ui/menu_backdrop.rs`.

/// Font size of a menu's title ("Roguelike", "Paused", "You Died"), in points.
pub const MENU_TITLE_SIZE: f32 = 56.0;
/// Extra space between the title's letters, in points. Spaced capitals read
/// as carved lettering; past about 10 the word falls apart into letters.
pub const MENU_TITLE_LETTER_SPACING: f32 = 6.0;
/// How far down-right the title's hard shadow falls, in points. Same idea as
/// [`PANEL_SHADOW_OFFSET`]: a hard offset, never a blur.
pub const MENU_TITLE_SHADOW_OFFSET: f32 = 3.0;
/// Width of the ornamental rule drawn under a menu title, in points.
pub const MENU_RULE_WIDTH: f32 = 320.0;
/// Half-size of the brass diamond in the middle of that rule, in points.
pub const MENU_RULE_DIAMOND: f32 = 4.0;

/// Size of a seed-mode chip on the start screen, in points.
pub const MENU_CHIP_SIZE: [f32; 2] = [76.0, 24.0];
/// Size of a menu button, in points.
pub const MENU_BUTTON_SIZE: [f32; 2] = [260.0, 40.0];
/// Size of a secondary menu button (Sandbox, Past Runs), in points.
pub const MENU_BUTTON_SMALL_SIZE: [f32; 2] = [124.0, 30.0];
/// Font size of a menu button's label, in points.
pub const MENU_BUTTON_FONT_SIZE: f32 = 20.0;
/// Font size of a secondary menu button's label, in points.
pub const MENU_BUTTON_SMALL_FONT_SIZE: f32 = 14.0;
/// Vertical gap between stacked menu buttons, in points.
pub const MENU_BUTTON_SPACING: f32 = 8.0;
/// Seconds a menu button takes to warm up to its highlight and cool back
/// down. Short enough that keyboard navigation never feels laggy.
pub const MENU_HIGHLIGHT_FADE: f32 = 0.08;
/// How far down-right a menu button's hard shadow falls, in points. Half the
/// panels' [`PANEL_SHADOW_OFFSET`], since a button sits lower than a panel.
pub const MENU_BUTTON_SHADOW_OFFSET: f32 = 2.0;
/// Half-height of the brass pointer triangles beside a highlighted menu
/// button, in points.
pub const MENU_POINTER_SIZE: f32 = 5.0;

/// Edge length of one backdrop tile on the start screen, in points. The
/// sheet's tiles are 32px, so this must stay an integer multiple of 32 to
/// keep the pixels square.
pub const MENU_BACKDROP_TILE_SIZE: f32 = 64.0;
/// Rows of wall across the top of the backdrop. The last of them is drawn as
/// the wall's face, the rest as its top.
pub const MENU_BACKDROP_WALL_ROWS: usize = 2;
/// Horizontal distance of each brazier from the screen centre, in points.
pub const MENU_BRAZIER_OFFSET_X: f32 = 340.0;
/// Radius of a brazier's glow on the start screen, in points.
pub const MENU_BRAZIER_GLOW_RADIUS: f32 = 360.0;
/// Peak strength of a brazier's glow, 0-1. The glow is added to what is under
/// it, so this is how far the floor next to a brazier is lifted towards the
/// glow colour.
pub const MENU_BRAZIER_GLOW_STRENGTH: f32 = 0.55;
/// How much the brazier glow's radius swells and shrinks with the flicker, as a
/// fraction of [`MENU_BRAZIER_GLOW_RADIUS`]. Uses the in-game light flicker, so
/// the braziers breathe like the lights in the dungeon.
pub const MENU_BRAZIER_FLICKER: f32 = 0.08;
/// Embers rising from each brazier at any moment.
pub const MENU_EMBER_COUNT: usize = 7;
/// Seconds an ember lives, from leaving the flame to going out.
pub const MENU_EMBER_LIFETIME: f32 = 3.2;
/// How fast an ember rises, in points per second.
pub const MENU_EMBER_RISE_SPEED: f32 = 34.0;
/// How far an ember sways side to side as it rises, in points.
pub const MENU_EMBER_SWAY: f32 = 10.0;
/// Edge length of an ember, in points. Square, and pinned to whole pixels.
pub const MENU_EMBER_SIZE: f32 = 2.0;
/// Segments around a round glow or vignette. Up is smoother and costs more
/// triangles; below about 24 the glow's edge visibly facets.
pub const UI_GLOW_SEGMENTS: usize = 48;
/// Cells along each side of the vignette's vertex grid. More is a smoother
/// falloff; below about 8 the grid shows as faint diagonal banding.
pub const UI_VIGNETTE_GRID: usize = 16;
/// Opacity of the vignette at the very edges of a menu screen, 0-255.
pub const MENU_VIGNETTE_ALPHA: u8 = 235;
/// How far in from the edges the vignette reaches before it is fully clear,
/// as a fraction of the half-width/half-height of the screen.
pub const MENU_VIGNETTE_REACH: f32 = 0.55;

/// Size of a class card on the start screen, in points.
pub const MENU_CLASS_CARD_SIZE: [f32; 2] = [144.0, 132.0];
/// Gap between class cards, in points.
pub const MENU_CLASS_CARD_GAP: f32 = 16.0;
/// Size of the class sprite on its card, in points. 2x the 32px sheet tile.
pub const MENU_CLASS_SPRITE_SIZE: f32 = 64.0;
/// How far the selected class's sprite bobs, in points. Whole pixels only, so
/// the bob reads as a two-frame idle rather than a smooth float.
pub const MENU_CLASS_BOB_AMPLITUDE: f32 = 2.0;
/// How fast the selected class's sprite bobs, in radians per second.
pub const MENU_CLASS_BOB_SPEED: f32 = 4.0;
/// Radius of the warm glow behind the selected class's sprite, in points.
pub const MENU_CLASS_GLOW_RADIUS: f32 = 56.0;
/// Strength of that glow, 0-1, as for [`MENU_BRAZIER_GLOW_STRENGTH`].
pub const MENU_CLASS_GLOW_STRENGTH: f32 = 0.22;
/// Width of the selected class's details panel, in points. Matches the row
/// of class cards above it (four cards and three gaps), and is wide enough
/// for the Necromancer's seven starting abilities beside the stat pips.
pub const MENU_DETAILS_WIDTH: f32 = 624.0;
/// The stat value a full row of pips stands for on the class details panel.
/// Starting stats run 10-18, so 20 leaves headroom and keeps the classes'
/// differences visible.
pub const MENU_STAT_PIPS: i32 = 20;
/// Size of one stat pip, in points.
pub const MENU_STAT_PIP_SIZE: [f32; 2] = [6.0, 10.0];
/// Gap between stat pips, in points.
pub const MENU_STAT_PIP_GAP: f32 = 2.0;
/// Size of an item or ability icon on the class details panel, in points.
/// The sheet's 32px tiles at 1:1.
pub const MENU_KIT_ICON_SIZE: f32 = 32.0;

/// Width of the centred panel holding the pause and game over menus, in
/// points.
pub const MENU_PANEL_WIDTH: f32 = 360.0;

// =============================================================================
// CHARACTER WINDOW
// =============================================================================
// The paper doll, stats and inventory grid in `src/ui/inventory.rs`.

/// Edge length of an item socket (equipment slot or inventory cell), in
/// points. Holds a [`CHARACTER_SOCKET_ICON_SIZE`] icon with a recess around it.
pub const CHARACTER_SOCKET_SIZE: f32 = 40.0;
/// Edge length of the item icon inside a socket, in points. 1:1 with the
/// sheet's 32px tiles, so icons stay crisp.
pub const CHARACTER_SOCKET_ICON_SIZE: f32 = 32.0;
/// Gap between inventory cells, in points.
pub const CHARACTER_GRID_GAP: f32 = 4.0;
/// Cells per row of the inventory grid.
pub const CHARACTER_GRID_COLUMNS: usize = 7;
/// Rows the inventory grid always shows, filled or not, so a near-empty pack
/// still reads as a pack with room in it rather than a stray icon.
pub const CHARACTER_GRID_MIN_ROWS: usize = 5;
/// Opacity of the silhouette in an empty equipment socket, 0-1. Enough to say
/// what goes there; any more and an empty slot looks filled.
pub const CHARACTER_SILHOUETTE_ALPHA: f32 = 0.16;
/// Width of the left-hand column (paper doll and stats), in points.
pub const CHARACTER_LEFT_WIDTH: f32 = 236.0;
/// Height of the paper doll, in points.
pub const CHARACTER_DOLL_HEIGHT: f32 = 180.0;
/// Size of the character sprite in the middle of the paper doll, in points.
/// 2x the 32px sheet tile.
pub const CHARACTER_DOLL_SPRITE_SIZE: f32 = 64.0;
/// Height of a tab in the character window, in points.
pub const CHARACTER_TAB_HEIGHT: f32 = 26.0;
/// Height of the weight bar, in points.
pub const CHARACTER_WEIGHT_BAR_HEIGHT: f32 = 12.0;
/// Size of an ability icon in the spellbook list, in points.
pub const CHARACTER_SPELL_ICON_SIZE: f32 = 32.0;
