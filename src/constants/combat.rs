//! Combat system constants.

/// Minimum damage multiplier from dice roll (percentage)
pub const COMBAT_DAMAGE_MIN_MULT: f32 = 0.8;
/// Maximum damage multiplier from dice roll (percentage)
pub const COMBAT_DAMAGE_MAX_MULT: f32 = 1.2;
/// Chance to deal a critical hit (0.0 - 1.0)
pub const COMBAT_CRIT_CHANCE: f32 = 0.1;
/// Critical hit damage multiplier
/// (balance: was 1.1, which sat inside the 0.8-1.2 variance band — a "crit"
/// could deal less than a lucky normal hit; 1.5 makes crits actually land)
pub const COMBAT_CRIT_MULTIPLIER: f32 = 1.5;

// =============================================================================
// WEAPON ON-HIT AFFIXES (resolved in systems/combat.rs)
// =============================================================================

/// Duration of the Slowed effect applied by OnHitSlow weapon affixes
pub const ON_HIT_SLOW_DURATION: f32 = 5.0;
/// Duration of the Feared effect applied by OnHitFear weapon affixes
pub const ON_HIT_FEAR_DURATION: f32 = 4.0;
/// Health fraction below which LowHealthDamage affixes activate
pub const LOW_HEALTH_DAMAGE_THRESHOLD: f32 = 0.3;

/// How far a locked-target melee attack (`ActionType::Attack`) reaches, as a
/// Chebyshev distance in tiles, checked when the swing *lands* rather than
/// when it starts. 1 means the eight surrounding tiles, diagonals included. A
/// target that has stepped further away than this by the time the attack
/// completes is missed. Raising it would let every melee attacker hit across
/// a gap; nothing in the game has a longer weapon today.
pub const MELEE_REACH: i32 = 1;
