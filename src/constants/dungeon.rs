//! Dungeon generation constants.

/// Minimum size of a BSP leaf node
pub const DUNGEON_MIN_LEAF_SIZE: i32 = 10;
/// Minimum room size within a leaf
pub const DUNGEON_MIN_ROOM_SIZE: i32 = 4;
/// Margin around rooms within their leaf
pub const DUNGEON_ROOM_MARGIN: i32 = 1;
/// Default dungeon width
pub const DUNGEON_DEFAULT_WIDTH: usize = 40;
/// Default dungeon height
pub const DUNGEON_DEFAULT_HEIGHT: usize = 40;
/// Chance for a room to have a special theme (Overgrown, Flooded, etc.)
pub const THEMED_ROOM_CHANCE: f32 = 0.25;

// =============================================================================
// FLOOR TRAPS (hidden, placed by generation — see systems::discovery)
// =============================================================================

/// Minimum hidden traps per floor
pub const TRAPS_PER_FLOOR_MIN: u32 = 1;
/// Maximum hidden traps per floor (before the depth bonus)
pub const TRAPS_PER_FLOOR_MAX: u32 = 3;
/// One extra trap per this many floors of depth
pub const TRAP_EXTRA_PER_FLOORS: u32 = 2;
/// Traps never spawn within this Chebyshev distance of a staircase
pub const TRAP_MIN_DIST_FROM_STAIRS: i32 = 3;

// =============================================================================
// ROOM FURNITURE (fountains, altars, shrines)
// =============================================================================

/// Roughly one furniture piece per this many rooms
pub const FURNITURE_ROOMS_PER_PIECE: usize = 3;

// =============================================================================
// HIDDEN ROOMS
// =============================================================================

/// Chance per floor that one small room is sealed behind a secret door
pub const SECRET_ROOM_CHANCE: f64 = 0.35;
/// Only rooms up to this area (tiles) can be sealed
pub const SECRET_ROOM_MAX_AREA: i32 = 40;
/// The hidden room's chest rolls loot as if this many floors deeper
pub const SECRET_CHEST_FLOOR_BONUS: u32 = 2;

// =============================================================================
// CAVERN ROOMS (cellular-automata caves -- see dungeon_gen::carve_cavern)
// =============================================================================

/// Fraction of cells seeded as wall before smoothing. Tuned by hand: 45% came
/// out noticeably too open and blobby, 47% gives usable pinch points and
/// free-standing pillars.
pub const CAVERN_FILL_CHANCE: f64 = 0.47;
/// Smoothing passes over the seeded grid. Each pass turns a cell to wall when
/// at least `CAVERN_WALL_NEIGHBOURS` of its 8 neighbours are wall. Five passes
/// smoothed the pinch points away; four keeps them.
pub const CAVERN_SMOOTH_PASSES: u32 = 4;
/// Wall neighbours (of 8) needed for a cell to become wall during smoothing.
pub const CAVERN_WALL_NEIGHBOURS: u32 = 5;
/// Rooms smaller than this on either axis fall back to a plain rectangular
/// carve (with cave floor): cellular automata on a tiny rect just produces
/// rubble.
pub const CAVERN_MIN_DIM: i32 = 7;
/// Chance per step that a connector channel wanders perpendicular instead of
/// heading straight for its target -- straight 1-tile channels read as
/// obviously artificial against organic cave walls.
pub const CAVERN_PATH_JITTER_CHANCE: f64 = 0.35;
/// Chance per step that a connector channel also opens a neighbouring cell,
/// so passages vary between one and two tiles wide.
pub const CAVERN_PATH_WIDEN_CHANCE: f64 = 0.4;
/// Chance per tile that a corridor punched through a cavern by the normal
/// room-to-room corridor pass gets roughened outward, so it stops reading as
/// a ruler-straight line through the rock.
pub const CAVERN_INTRUSION_ROUGHEN_CHANCE: f64 = 0.45;
/// Chance per cavern-adjacent wall tile of showing an ore vein.
pub const CAVERN_ORE_WALL_CHANCE: f64 = 0.08;
/// Chance per cavern ceiling wall tile of growing stalactites.
pub const CAVERN_STALACTITE_CHANCE: f64 = 0.18;

/// Blocking stalagmite clusters per cavern (cover you can shoot over).
pub const CAVERN_STALAGMITES_MIN: usize = 3;
pub const CAVERN_STALAGMITES_MAX: usize = 7;
/// Glowing mushroom patches per cavern (light sources, flammable).
pub const CAVERN_MUSHROOMS_MIN: usize = 2;
pub const CAVERN_MUSHROOMS_MAX: usize = 4;
/// Crystal clusters per cavern (light sources).
pub const CAVERN_CRYSTALS_MIN: usize = 1;
pub const CAVERN_CRYSTALS_MAX: usize = 3;

// =============================================================================
// CAVE ECOLOGY (what lives in a cavern instead of the floor roster)
// =============================================================================

/// Giant bats per cavern room.
pub const CAVERN_BATS_MIN: usize = 2;
pub const CAVERN_BATS_MAX: usize = 4;
/// Lesser giant spiders per cavern room.
pub const CAVERN_SPIDERS_MIN: usize = 1;
pub const CAVERN_SPIDERS_MAX: usize = 3;
/// From this floor on, one venomous giant spider also lairs in each cavern.
pub const CAVERN_GIANT_SPIDER_FLOOR: u32 = 3;
