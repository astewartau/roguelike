//! Core gameplay constants (player stats, XP, FOV).

/// Player's default starting health
pub const PLAYER_STARTING_HEALTH: i32 = 50;
/// Player's action speed multiplier (1.0 = baseline)
pub const PLAYER_SPEED: f32 = 1.0;

/// Default FOV radius for player
pub const FOV_RADIUS: i32 = 10;

/// Base XP formula multiplier (XP needed = level * this)
pub const XP_PER_LEVEL_MULTIPLIER: u32 = 100;

/// Unarmed attack damage
pub const UNARMED_DAMAGE: i32 = 2;

/// Player HP regenerated per regen event
pub const PLAYER_HP_REGEN_AMOUNT: i32 = 1;
/// Seconds between each player HP regen event
pub const PLAYER_HP_REGEN_INTERVAL: f32 = 10.0;

// =============================================================================
// EXERTION: HOW TIRING ACTIONS ARE (all actors, see time_system.rs)
// =============================================================================
// Effort is *not* a resource anyone spends — nothing is gated on it and there is
// no pool to run dry. It feeds the long-term fatigue meter, and that is all it
// does: a run spent fighting sends you to bed sooner than a run spent creeping
// down corridors.
//
// Effort accrues at a *rate* while an action runs: `effort = rate * duration`.
// A flat amount per action would make tiredness-per-second equal `1 / duration`,
// which for a one-second base action is the actor's speed stat — so a quick
// creature would tire in proportion to being quick, which is backwards. Charging
// per second of work makes it track the work rather than the speed.
//
// The rates below are per game-second of acting, in fatigue-feeding units.

/// Effort rate for actions that are not tiring at all: waiting, standing, the
/// free equip/unequip actions, and the post-shot recovery step.
pub const EXERTION_IDLE: f32 = 0.0;

/// Effort rate for ordinary movement and the small interactions that are no
/// more effortful than walking: opening a door or a chest, taking the stairs,
/// picking something up, talking.
///
/// A door is about as much work as a step, which is why they share a rate.
pub const EXERTION_LIGHT: f32 = 8.0;

/// Effort rate for fighting: melee swings and bow shots.
///
/// Several times the walking rate, so a run full of combat tires you much
/// sooner than a quiet one — but it costs nothing in the moment, so fighting
/// itself stays as consistent as your weapon and your speed make it.
pub const EXERTION_HEAVY: f32 = 30.0;


// =============================================================================
// SURVIVAL CLOCK: HUNGER (player-only, see systems/survival.rs)
// =============================================================================

/// Hunger meter cap (fully fed)
pub const HUNGER_MAX: f32 = 100.0;
/// Game-time seconds for the hunger meter to drain by 1 point
pub const HUNGER_DRAIN_SECONDS_PER_POINT: f32 = 12.0;
/// Hunger drains this much faster while resting (recovery burns calories)
pub const HUNGER_REST_DRAIN_MULT: f32 = 2.0;
/// Hunger drains this much faster while sleeping
pub const HUNGER_SLEEP_DRAIN_MULT: f32 = 1.5;
/// Below this the player is "Hungry": natural HP regen stops
pub const HUNGER_HUNGRY_THRESHOLD: f32 = 25.0;
/// Damage per starvation tick at hunger 0 (applied directly, ignores armor)
pub const STARVATION_DAMAGE: i32 = 1;
/// Game-time seconds between starvation damage ticks
pub const STARVATION_DAMAGE_INTERVAL: f32 = 5.0;

// =============================================================================
// SURVIVAL CLOCK: FATIGUE / SLEEP (player-only, see systems/survival.rs)
// =============================================================================

/// Fatigue meter cap (exhausted)
pub const FATIGUE_MAX: f32 = 100.0;

/// Fatigue added per point of effort expended.
///
/// Fatigue used to be a second clock: it grew purely with elapsed time, at a
/// rate that reached "exhausted" in about 33 minutes. Hunger is also a clock
/// and reaches "hungry" in about 15, so hunger always won the race and fatigue
/// never actually drove a decision — it was a slower copy of a meter the player
/// already had.
///
/// It now measures **effort** instead: it is the running integral of the same
/// exertion that drains energy. Energy is the short-term buffer of effort and
/// refills in seconds; fatigue is the long-term total and only clears by
/// sleeping. Same input, two timescales, and now the two long meters answer
/// different questions — hunger asks "when did you last eat", fatigue asks
/// "what have you been doing".
///
/// At this rate, steady walking reaches exhaustion in roughly twenty minutes
/// and continuous fighting in about five. Raise it to make sleep a more
/// frequent interruption; lower it to let a run go longer between rests.
pub const FATIGUE_PER_EFFORT: f32 = 0.01;

/// Fatigue added per game-second awake regardless of what the player is doing.
///
/// Small on purpose. Being awake should eventually tire you even if you do
/// nothing at all, so a cautious player cannot opt out of sleeping entirely,
/// but at this rate idling alone takes hours to exhaust — effort is what
/// matters. Set it to 0.0 for purely effort-driven fatigue.
pub const FATIGUE_IDLE_GAIN_PER_SECOND: f32 = 0.01;

// Sleep has to dominate the idle gain by a wide margin or resting would never
// get the player anywhere.
const _: () = assert!(
    SLEEP_FATIGUE_RECOVERY_PER_SECOND > FATIGUE_IDLE_GAIN_PER_SECOND * 10.0,
    "sleeping must recover fatigue far faster than being awake accrues it"
);

/// Above this the player is "Tired": enemies notice them faster and their
/// attacks hit softer. At `FATIGUE_MAX` they are "Exhausted" (slower actions,
/// and energy regenerates at [`EXHAUSTED_ENERGY_REGEN_MULT`]).
pub const FATIGUE_TIRED_THRESHOLD: f32 = 75.0;
/// Enemy alertness gain against a Tired player is multiplied by this (+25%)
pub const TIRED_ALERTNESS_MULT: f32 = 1.25;
/// A Tired attacker's damage is multiplied by this (-10%)
pub const TIRED_DAMAGE_MULT: f32 = 0.9;
/// An Exhausted actor's action speed is multiplied by this (slower actions)
pub const EXHAUSTED_SPEED_MULT: f32 = 0.75;
/// Fatigue points recovered per game-time second while sleeping. At the rest
/// fast-forward rate (48 game-sec / real-sec) a full 100-point recovery takes
/// ~300 game-seconds, i.e. roughly 6-7 real seconds of fast-forward — in the
/// same ballpark as a full rest heal.
pub const SLEEP_FATIGUE_RECOVERY_PER_SECOND: f32 = 1.0 / 3.0;
/// Enemy alertness gain against a *sleeping* player is multiplied by this —
/// lying unconscious in the open is the opposite of hiding.
pub const SLEEPING_PLAYER_ALERT_MULT: f32 = 3.0;
/// Discrete game-time step for survival meter updates (accumulator pattern)
pub const SURVIVAL_TICK_INTERVAL: f32 = 0.25;

// =============================================================================
// DUNGEON TRAPS: EFFECTS AND DETECTION (see systems::discovery)
// =============================================================================

/// Spike trap damage (through `apply_damage`, so armor applies)
pub const DUNGEON_SPIKE_TRAP_DAMAGE: i32 = 8;
/// Fire trap burst damage (Burning is applied on top)
pub const DUNGEON_FIRE_TRAP_DAMAGE: i32 = 5;
/// Snare trap root duration (seconds)
pub const DUNGEON_SNARE_ROOT_DURATION: f32 = 5.0;
/// Alarm trap wakes every enemy within this Chebyshev radius
pub const DUNGEON_ALARM_WAKE_RADIUS: i32 = 10;

/// Base per-step chance to spot an adjacent hidden trap / secret door at
/// Agility 10. Scaled by `detection_chance` (+8% of base per point of AGI).
pub const DETECT_BASE_CHANCE: f64 = 0.25;
/// Detection scaling per point of Agility above/below 10
pub const DETECT_AGI_SCALING: f64 = 0.08;
/// Clamp bounds so detection never becomes impossible or guaranteed
pub const DETECT_CHANCE_MIN: f64 = 0.05;
pub const DETECT_CHANCE_MAX: f64 = 0.95;

/// Sprite tints for revealed traps (per kind) and furniture
pub const TRAP_TINT_SPIKE: (f32, f32, f32) = (0.85, 0.85, 1.0);
pub const TRAP_TINT_FIRE: (f32, f32, f32) = (1.0, 0.45, 0.3);
pub const TRAP_TINT_SNARE: (f32, f32, f32) = (0.5, 0.95, 0.5);
pub const TRAP_TINT_ALARM: (f32, f32, f32) = (1.0, 0.9, 0.3);
pub const FOUNTAIN_TINT: (f32, f32, f32) = (0.45, 0.7, 1.0);
pub const ALTAR_TINT: (f32, f32, f32) = (1.0, 0.55, 0.55);
pub const SHRINE_TINT: (f32, f32, f32) = (1.0, 0.9, 0.5);
/// Grey tint for spent one-use furniture (dry fountain, inert shrine)
pub const FURNITURE_SPENT_TINT: (f32, f32, f32) = (0.55, 0.55, 0.55);
/// Tint for a discovered secret door (distinct from normal doors)
pub const SECRET_DOOR_TINT: (f32, f32, f32) = (0.8, 0.7, 1.0);

// =============================================================================
// ROOM FURNITURE: OUTCOMES (see systems::furniture)
// =============================================================================

/// Fountain outcome odds (checked in order; remainder is the mild debuff)
pub const FOUNTAIN_HEAL_CHANCE: f64 = 0.35;
pub const FOUNTAIN_FOOD_CHANCE: f64 = 0.25;
pub const FOUNTAIN_BUFF_CHANCE: f64 = 0.20;
/// Duration of a fountain-granted random buff (seconds)
pub const FOUNTAIN_BUFF_DURATION: f32 = 30.0;
/// Duration of the fountain's Confused debuff (seconds)
pub const FOUNTAIN_CONFUSED_DURATION: f32 = 10.0;
/// Duration of the fountain's Slowed debuff (seconds)
pub const FOUNTAIN_SLOWED_DURATION: f32 = 20.0;

/// Altar blessing odds: `clamp(BASE + value / DIVISOR, BASE, MAX)`
pub const ALTAR_BLESS_BASE: f64 = 0.25;
pub const ALTAR_BLESS_VALUE_DIVISOR: f64 = 400.0;
pub const ALTAR_BLESS_MAX: f64 = 0.9;
/// Duration of an altar curse debuff (seconds)
pub const ALTAR_CURSE_DURATION: f32 = 60.0;

/// Duration of the shrine's Protected ward (seconds)
pub const SHRINE_PROTECT_DURATION: f32 = 60.0;
