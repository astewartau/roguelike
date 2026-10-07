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
