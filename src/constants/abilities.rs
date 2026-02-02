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
