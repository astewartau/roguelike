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
