//! Ability constants for class abilities.

// Fighter - Cleave
pub const CLEAVE_COOLDOWN: f32 = 25.0;
pub const CLEAVE_ENERGY_COST: i32 = 2;
pub const CLEAVE_DURATION: f32 = 1.0;

// Ranger - Sprint
pub const SPRINT_COOLDOWN: f32 = 30.0;
pub const SPRINT_DURATION: f32 = 10.0;
pub const SPRINT_ENERGY_COST: i32 = 1;
pub const SPRINT_ACTIVATION_DURATION: f32 = 0.3;

// Druid - Tame
pub const TAME_COOLDOWN: f32 = 60.0;
pub const TAME_ENERGY_COST: i32 = 1;
pub const TAME_RANGE: i32 = 5;
pub const TAME_DURATION: f32 = 6.0;

// Druid - Barkskin
pub const BARKSKIN_COOLDOWN: f32 = 45.0;
pub const BARKSKIN_ENERGY_COST: i32 = 1;
pub const BARKSKIN_DURATION: f32 = 15.0;
pub const BARKSKIN_ACTIVATION_DURATION: f32 = 0.3;

// Necromancer - Life Drain (channeled)
pub const LIFE_DRAIN_COOLDOWN: f32 = 10.0;
pub const LIFE_DRAIN_ENERGY_COST: i32 = 1;
pub const LIFE_DRAIN_RANGE: i32 = 4;
pub const LIFE_DRAIN_TICK_INTERVAL: f32 = 0.5; // Damage ticks every 0.5 seconds
pub const LIFE_DRAIN_DAMAGE_PER_TICK: i32 = 4; // Damage per tick
pub const LIFE_DRAIN_HEAL_PERCENT: f32 = 0.5; // Heal 50% of damage dealt

// Necromancer - Fear
pub const FEAR_ABILITY_COOLDOWN: f32 = 45.0;
pub const FEAR_ABILITY_ENERGY_COST: i32 = 2;
pub const FEAR_ABILITY_RADIUS: i32 = 5;
pub const FEAR_ABILITY_DURATION: f32 = 30.0;

// Fighter - Stun
pub const STUN_COOLDOWN: f32 = 30.0;
pub const STUN_ENERGY_COST: i32 = 2;
pub const STUN_ABILITY_RADIUS: i32 = 3;
pub const STUN_ABILITY_DURATION: f32 = 5.0;

// Ranger - Disengage
pub const DISENGAGE_COOLDOWN: f32 = 12.0;
pub const DISENGAGE_ENERGY_COST: i32 = 1;
pub const DISENGAGE_DISTANCE: i32 = 3;
pub const DISENGAGE_DURATION: f32 = 0.3;

// Ranger - Tumble
pub const TUMBLE_COOLDOWN: f32 = 10.0;
pub const TUMBLE_ENERGY_COST: i32 = 1;
pub const TUMBLE_DISTANCE: i32 = 2;
pub const TUMBLE_INVULN_DURATION: f32 = 0.5;
pub const TUMBLE_DURATION: f32 = 0.25;

// Ranger - Snare Trap
pub const SNARE_TRAP_COOLDOWN: f32 = 18.0;
pub const SNARE_TRAP_ENERGY_COST: i32 = 1;
pub const SNARE_TRAP_RANGE: i32 = 1;
pub const SNARE_TRAP_ROOT_DURATION: f32 = 5.0;
pub const SNARE_TRAP_DURATION: f32 = 0.5;

// Ranger - Crippling Shot
pub const CRIPPLING_SHOT_COOLDOWN: f32 = 15.0;
pub const CRIPPLING_SHOT_ENERGY_COST: i32 = 1;
pub const CRIPPLING_SHOT_SLOW_DURATION: f32 = 6.0;

// Range Bands (for bow attacks)
pub const RANGE_OPTIMAL_MIN: i32 = 3;
pub const RANGE_OPTIMAL_MAX: i32 = 5;
pub const RANGE_OPTIMAL_MULT: f32 = 1.2;  // +20% damage in optimal range
pub const RANGE_CLOSE_MULT: f32 = 0.9;    // -10% damage at close range (1-2 tiles)
pub const RANGE_FAR_MULT: f32 = 0.8;      // -20% damage at far range (6+ tiles)

/// Player bow maximum range
pub const BOW_RANGE: i32 = 10;

// =============================================================================
// INT SCALING FOR MAGIC
// =============================================================================

/// Extra magic power per point of INT above 10 (+5% per point).
pub const INT_POWER_PER_POINT: f32 = 0.05;
/// Floor on the INT power multiplier (very low INT still casts at half power).
pub const INT_POWER_MIN_MULT: f32 = 0.5;

/// Magic power multiplier for a given (effective) Intelligence score.
///
/// `1.0` at INT 10, +5% per point above, -5% per point below, floored at
/// [`INT_POWER_MIN_MULT`]. Applied to spell damage, magical effect durations,
/// and scroll magnitudes — staff *melee* damage is deliberately NOT scaled.
pub fn int_power_mult(int: i32) -> f32 {
    (1.0 + (int - 10) as f32 * INT_POWER_PER_POINT).max(INT_POWER_MIN_MULT)
}

// =============================================================================
// LEARNED SPELLS (studied from scrolls)
// =============================================================================

// Minimum effective INT required to Study each scroll into a permanent spell.
// Reveal/Mapping are pure utility and intentionally NOT learnable.
pub const LEARN_INT_BLINK: i32 = 14;
pub const LEARN_INT_FIREBALL: i32 = 16;
pub const LEARN_INT_FEAR: i32 = 15;
pub const LEARN_INT_SLOW: i32 = 13;
pub const LEARN_INT_PROTECTION: i32 = 13;
pub const LEARN_INT_SPEED: i32 = 12;
pub const LEARN_INT_INVISIBILITY: i32 = 15;

// Energy costs / cooldowns for learned casts (long cooldowns — the scroll
// stays the spammable path, the learned spell is the repeatable one).
pub const LEARNED_BLINK_ENERGY_COST: i32 = 1;
pub const LEARNED_BLINK_COOLDOWN: f32 = 45.0;
pub const LEARNED_FIREBALL_ENERGY_COST: i32 = 2;
pub const LEARNED_FIREBALL_COOLDOWN: f32 = 60.0;
pub const LEARNED_FEAR_ENERGY_COST: i32 = 2;
pub const LEARNED_FEAR_COOLDOWN: f32 = 50.0;
pub const LEARNED_SLOW_ENERGY_COST: i32 = 1;
pub const LEARNED_SLOW_COOLDOWN: f32 = 40.0;
pub const LEARNED_PROTECTION_ENERGY_COST: i32 = 1;
pub const LEARNED_PROTECTION_COOLDOWN: f32 = 50.0;
pub const LEARNED_SPEED_ENERGY_COST: i32 = 1;
pub const LEARNED_SPEED_COOLDOWN: f32 = 45.0;
pub const LEARNED_INVISIBILITY_ENERGY_COST: i32 = 2;
pub const LEARNED_INVISIBILITY_COOLDOWN: f32 = 70.0;

// =============================================================================
// NECROMANCER - RAISE DEAD
// =============================================================================

pub const RAISE_DEAD_COOLDOWN: f32 = 30.0;
pub const RAISE_DEAD_ENERGY_COST: i32 = 2;
/// Maximum distance (Chebyshev) to a targeted bones pile.
pub const RAISE_DEAD_RANGE: i32 = 5;
/// Channel time before the skeleton rises (mirrors the Tame channel).
pub const RAISE_DEAD_CHANNEL_DURATION: f32 = 3.0;
/// INT points above 10 per additional controllable skeleton.
pub const RAISE_DEAD_INT_PER_EXTRA: i32 = 6;

/// How many raised skeletons a caster with the given (effective) INT can
/// control at once: 1 at INT 10, +1 per [`RAISE_DEAD_INT_PER_EXTRA`] INT
/// (INT 16 = 2, INT 22 = 3). Never below 1.
pub fn raise_dead_cap(int: i32) -> usize {
    let extra = (int - 10).max(0) / RAISE_DEAD_INT_PER_EXTRA;
    (1 + extra) as usize
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_int_power_mult_curve() {
        assert!((int_power_mult(10) - 1.0).abs() < f32::EPSILON);
        assert!((int_power_mult(11) - 1.05).abs() < 1e-6);
        assert!((int_power_mult(16) - 1.3).abs() < 1e-6);
        assert!((int_power_mult(20) - 1.5).abs() < 1e-6);
        // Below 10 it shrinks, floored at the minimum.
        assert!((int_power_mult(8) - 0.9).abs() < 1e-6);
        assert!((int_power_mult(0) - INT_POWER_MIN_MULT).abs() < f32::EPSILON);
        assert!((int_power_mult(-100) - INT_POWER_MIN_MULT).abs() < f32::EPSILON);
    }

    #[test]
    fn test_raise_dead_cap_formula() {
        assert_eq!(raise_dead_cap(10), 1);
        assert_eq!(raise_dead_cap(15), 1);
        assert_eq!(raise_dead_cap(16), 2);
        assert_eq!(raise_dead_cap(21), 2);
        assert_eq!(raise_dead_cap(22), 3);
        // Low INT never drops below one skeleton.
        assert_eq!(raise_dead_cap(3), 1);
    }
}
