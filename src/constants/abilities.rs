//! Ability constants for class abilities.
//!
//! Ability energy costs are **flat**, unlike movement and fighting, which are
//! charged at a rate per second of acting. The distinction is deliberate: a
//! continuous effort should cost the same per second however fast you are, but
//! a deliberate ability is a discrete commitment — nobody thinks that casting a
//! spell quickly ought to make it cheaper. See `ActionCost` in `components.rs`.
//!
//! These were 1-2 on the old 5-point energy pool and are scaled x20 for the
//! 0..=100 pool, so relative ability balance is unchanged.

// Fighter - Cleave
pub const CLEAVE_COOLDOWN: f32 = 25.0;
pub const CLEAVE_ENERGY_COST: f32 = 40.0;
pub const CLEAVE_DURATION: f32 = 1.0;

// Ranger - Sprint
pub const SPRINT_COOLDOWN: f32 = 30.0;
pub const SPRINT_DURATION: f32 = 10.0;
pub const SPRINT_ENERGY_COST: f32 = 20.0;
pub const SPRINT_ACTIVATION_DURATION: f32 = 0.3;

// Druid - Tame
pub const TAME_COOLDOWN: f32 = 60.0;
pub const TAME_ENERGY_COST: f32 = 20.0;
pub const TAME_RANGE: i32 = 5;
pub const TAME_DURATION: f32 = 6.0;

// Druid - Barkskin
pub const BARKSKIN_COOLDOWN: f32 = 45.0;
pub const BARKSKIN_ENERGY_COST: f32 = 20.0;
pub const BARKSKIN_DURATION: f32 = 15.0;
pub const BARKSKIN_ACTIVATION_DURATION: f32 = 0.3;

// Necromancer - Life Drain (channeled)
pub const LIFE_DRAIN_COOLDOWN: f32 = 10.0;
pub const LIFE_DRAIN_ENERGY_COST: f32 = 20.0;
pub const LIFE_DRAIN_RANGE: i32 = 4;
pub const LIFE_DRAIN_TICK_INTERVAL: f32 = 0.5; // Damage ticks every 0.5 seconds
pub const LIFE_DRAIN_DAMAGE_PER_TICK: i32 = 4; // Damage per tick
pub const LIFE_DRAIN_HEAL_PERCENT: f32 = 0.5; // Heal 50% of damage dealt

// Necromancer - Fear
pub const FEAR_ABILITY_COOLDOWN: f32 = 45.0;
pub const FEAR_ABILITY_ENERGY_COST: f32 = 40.0;
pub const FEAR_ABILITY_RADIUS: i32 = 5;
pub const FEAR_ABILITY_DURATION: f32 = 30.0;

// Fighter - Stun
pub const STUN_COOLDOWN: f32 = 30.0;
pub const STUN_ENERGY_COST: f32 = 40.0;
pub const STUN_ABILITY_RADIUS: i32 = 3;
pub const STUN_ABILITY_DURATION: f32 = 5.0;

// Ranger - Disengage
pub const DISENGAGE_COOLDOWN: f32 = 12.0;
pub const DISENGAGE_ENERGY_COST: f32 = 20.0;
pub const DISENGAGE_DISTANCE: i32 = 3;
pub const DISENGAGE_DURATION: f32 = 0.3;

// Ranger - Tumble
pub const TUMBLE_COOLDOWN: f32 = 10.0;
pub const TUMBLE_ENERGY_COST: f32 = 20.0;
pub const TUMBLE_DISTANCE: i32 = 2;
pub const TUMBLE_INVULN_DURATION: f32 = 0.5;
pub const TUMBLE_DURATION: f32 = 0.25;

// Ranger - Snare Trap
pub const SNARE_TRAP_COOLDOWN: f32 = 18.0;
pub const SNARE_TRAP_ENERGY_COST: f32 = 20.0;
pub const SNARE_TRAP_RANGE: i32 = 1;
pub const SNARE_TRAP_ROOT_DURATION: f32 = 5.0;
pub const SNARE_TRAP_DURATION: f32 = 0.5;

// Ranger - Crippling Shot
pub const CRIPPLING_SHOT_COOLDOWN: f32 = 15.0;
pub const CRIPPLING_SHOT_ENERGY_COST: f32 = 20.0;
pub const CRIPPLING_SHOT_SLOW_DURATION: f32 = 6.0;

// =============================================================================
// FIGHTER - GUARD (reactive)
// =============================================================================
//
// Guard is a *reactive* tool: enemy swings land when their action completes
// (0.36s for a bat up to ~1.45s for a zombie), so a guard that only went up when
// its own action completed would arrive after the blow it was meant to answer.
// The Guarding effect is therefore applied the instant the action STARTS (see
// `systems::actions::kit::apply_action_start_effects`) and dropped when it
// completes. A future "Parry" upgrade would hook in at the same place
// (`apply_attack`'s guard check): e.g. a perfect-timing window that negates the
// hit entirely and ripostes. Not implemented.

/// How long one Guard lasts, in game seconds. It is both the action's base
/// duration (you are busy guarding) and the window in which melee hits are
/// blocked. Longer makes Guard easier to time but leaves you committed for
/// longer; shorter rewards reading the swing telegraph.
pub const GUARD_DURATION: f32 = 0.6;
/// Fraction of a blocked melee hit that Guard removes (0.75 => you take 25%).
/// Applied to the raw hit before armor. Up makes Guard a hard counter; down
/// makes it a mitigation tool rather than a wall.
pub const GUARD_DAMAGE_REDUCTION: f32 = 0.75;
/// How long an attacker whose blow was blocked is Staggered (Stunned) for.
/// Up gives the Fighter a bigger counter-attack window after a good block.
pub const GUARD_STAGGER_DURATION: f32 = 0.6;
/// Guard cooldown in game seconds. Short: it is meant to be used every fight.
pub const GUARD_COOLDOWN: f32 = 5.0;
/// Fatigue effort for one Guard (flat, like every ability).
pub const GUARD_ENERGY_COST: f32 = 10.0;

// =============================================================================
// NECROMANCER - BONE WARD (reactive)
// =============================================================================

/// Chebyshev radius around the caster in which corpses (bone piles) add a
/// Bone Ward charge. Bigger makes the ward easier to fully charge.
pub const BONE_WARD_CORPSE_RADIUS: i32 = 3;
/// Most hits one Bone Ward can absorb (1 base + 1 per nearby corpse, capped).
pub const BONE_WARD_MAX_CHARGES: u32 = 3;
/// How long the ward lasts if its charges are not spent, in game seconds.
pub const BONE_WARD_DURATION: f32 = 12.0;
/// Bone Ward cooldown in game seconds.
pub const BONE_WARD_COOLDOWN: f32 = 20.0;
/// Bone Ward cast time. Near-instant; the ward itself goes up at action start.
pub const BONE_WARD_CAST_DURATION: f32 = 0.15;
/// Fatigue effort for one Bone Ward cast.
pub const BONE_WARD_ENERGY_COST: f32 = 20.0;

// =============================================================================
// NECROMANCER - SACRIFICE (reactive)
// =============================================================================

/// Maximum Chebyshev distance to the raised skeleton you swap places with.
pub const SACRIFICE_RANGE: i32 = 6;
/// Sacrifice action time. The swap itself happens at action start; this is
/// just how long the necromancer is busy afterwards.
pub const SACRIFICE_DURATION: f32 = 0.15;
/// Sacrifice cooldown in game seconds.
pub const SACRIFICE_COOLDOWN: f32 = 15.0;
/// Fatigue effort for one Sacrifice.
pub const SACRIFICE_ENERGY_COST: f32 = 20.0;
/// Threat a swapped-in skeleton gains, *on top of* the player's own threat, on
/// every enemy that had a melee swing in flight at the player. Enough to make
/// the skeleton their new top target. Up makes the taunt stick longer as the
/// player keeps fighting.
pub const SACRIFICE_TAUNT_THREAT: f32 = 25.0;

// =============================================================================
// NECROMANCER - CORPSE EXPLOSION
// =============================================================================

/// Maximum Chebyshev distance to the targeted corpse.
pub const CORPSE_EXPLOSION_RANGE: i32 = 5;
/// Chebyshev radius of the blast around the corpse.
pub const CORPSE_EXPLOSION_RADIUS: i32 = 1;
/// Raw blast damage before INT scaling (`queries::int_power`) and armor.
pub const CORPSE_EXPLOSION_DAMAGE: i32 = 12;
/// Corpse Explosion cooldown in game seconds.
pub const CORPSE_EXPLOSION_COOLDOWN: f32 = 20.0;
/// Fatigue effort for one Corpse Explosion.
pub const CORPSE_EXPLOSION_ENERGY_COST: f32 = 40.0;

// =============================================================================
// DRUID - THORNS
// =============================================================================

/// Base Thorns duration in game seconds, multiplied by the caster's INT power.
pub const THORNS_DURATION: f32 = 10.0;
/// Raw damage reflected at a melee attacker per hit that lands on the druid
/// (goes through `apply_damage`, so the attacker's armor applies).
pub const THORNS_DAMAGE: i32 = 3;
/// Thorns cooldown in game seconds.
pub const THORNS_COOLDOWN: f32 = 30.0;
/// Thorns cast time (quick self-buff, like Barkskin).
pub const THORNS_ACTIVATION_DURATION: f32 = 0.3;
/// Fatigue effort for one Thorns cast.
pub const THORNS_ENERGY_COST: f32 = 20.0;

// =============================================================================
// DRUID - ENTANGLE
// =============================================================================

/// Maximum Chebyshev distance to the targeted tile.
pub const ENTANGLE_RANGE: i32 = 5;
/// Chebyshev radius of the entangling patch around the targeted tile.
pub const ENTANGLE_RADIUS: i32 = 1;
/// Root duration for a hostile standing on bare ground, in game seconds.
pub const ENTANGLE_ROOT_DURATION: f32 = 3.0;
/// Root duration for a hostile standing in grass or tall grass: the vines have
/// something to grow from. Keep above [`ENTANGLE_ROOT_DURATION`].
pub const ENTANGLE_ROOT_DURATION_GRASS: f32 = 5.0;
/// Entangle cooldown in game seconds.
pub const ENTANGLE_COOLDOWN: f32 = 20.0;
/// Entangle cast time (a targeted spell, like a bow shot).
pub const ENTANGLE_CAST_DURATION: f32 = 0.5;
/// Fatigue effort for one Entangle.
pub const ENTANGLE_ENERGY_COST: f32 = 20.0;

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
pub const LEARNED_BLINK_ENERGY_COST: f32 = 20.0;
pub const LEARNED_BLINK_COOLDOWN: f32 = 45.0;
pub const LEARNED_FIREBALL_ENERGY_COST: f32 = 40.0;
pub const LEARNED_FIREBALL_COOLDOWN: f32 = 60.0;
pub const LEARNED_FEAR_ENERGY_COST: f32 = 40.0;
pub const LEARNED_FEAR_COOLDOWN: f32 = 50.0;
pub const LEARNED_SLOW_ENERGY_COST: f32 = 20.0;
pub const LEARNED_SLOW_COOLDOWN: f32 = 40.0;
pub const LEARNED_PROTECTION_ENERGY_COST: f32 = 20.0;
pub const LEARNED_PROTECTION_COOLDOWN: f32 = 50.0;
pub const LEARNED_SPEED_ENERGY_COST: f32 = 20.0;
pub const LEARNED_SPEED_COOLDOWN: f32 = 45.0;
pub const LEARNED_INVISIBILITY_ENERGY_COST: f32 = 40.0;
pub const LEARNED_INVISIBILITY_COOLDOWN: f32 = 70.0;

// =============================================================================
// NECROMANCER - RAISE DEAD
// =============================================================================

pub const RAISE_DEAD_COOLDOWN: f32 = 30.0;
pub const RAISE_DEAD_ENERGY_COST: f32 = 40.0;
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
