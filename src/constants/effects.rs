//! Status effect durations and parameters.

/// Duration of invisibility effect in game-time seconds
pub const INVISIBILITY_DURATION: f32 = 60.0;

/// Duration of speed boost effect in game-time seconds
pub const SPEED_BOOST_DURATION: f32 = 45.0;
/// Speed multiplier when speed boost is active (2.0 = twice as fast)
pub const SPEED_BOOST_MULTIPLIER: f32 = 2.0;

/// Duration of regeneration effect in game-time seconds
pub const REGENERATION_DURATION: f32 = 60.0;
/// HP regenerated per tick when Regenerating effect is active
pub const REGENERATION_BOOST_AMOUNT: i32 = 3;
/// Seconds between regen ticks when Regenerating effect is active
pub const REGENERATION_BOOST_INTERVAL: f32 = 3.0;

/// Duration of strength effect in game-time seconds
pub const STRENGTH_DURATION: f32 = 45.0;
/// Damage multiplier when Strengthened effect is active
pub const STRENGTH_DAMAGE_MULTIPLIER: f32 = 1.5;

/// Duration of protection effect in game-time seconds
pub const PROTECTION_DURATION: f32 = 60.0;
/// Damage reduction multiplier when Protected effect is active (0.5 = 50% reduction)
pub const PROTECTION_DAMAGE_REDUCTION: f32 = 0.5;

/// Duration of confusion effect in game-time seconds
pub const CONFUSION_DURATION: f32 = 30.0;

/// Duration of fear effect in game-time seconds
pub const FEAR_DURATION: f32 = 45.0;

/// Duration of slow effect in game-time seconds
pub const SLOW_DURATION: f32 = 45.0;
/// Speed multiplier when Slowed effect is active (0.5 = half speed)
pub const SLOW_MULTIPLIER: f32 = 0.5;

/// Maximum teleport range for Blink scroll
pub const BLINK_RANGE: i32 = 8;
/// Maximum casting range for Fireball scroll
pub const FIREBALL_RANGE: i32 = 10;
/// Explosion radius for Fireball scroll
pub const FIREBALL_RADIUS: i32 = 2;
/// Damage dealt by Fireball scroll
pub const FIREBALL_DAMAGE: i32 = 25;
/// Duration of Scroll of Reveal effect (game-time seconds)
pub const REVEAL_DURATION: f32 = 10.0;
/// Radius around each enemy revealed by Scroll of Reveal
pub const REVEAL_RADIUS: i32 = 3;

/// Duration of burning effect in game-time seconds
pub const BURNING_DURATION: f32 = 10.0;
/// Damage dealt per second while burning
pub const BURNING_DAMAGE_PER_SECOND: i32 = 2;
/// Interval between burn damage ticks in game-time seconds
pub const BURNING_DAMAGE_INTERVAL: f32 = 1.0;

// =============================================================================
// WET / OILED (surface statuses — see systems::tile_effects and systems::effects)
// =============================================================================

/// How long (game seconds) a creature stays Wet after leaving water, being
/// splashed, or being rained on. Standing in water keeps refreshing it. While
/// Wet a creature cannot catch fire, and gaining Wet puts out Burning and
/// washes off Oiled. Longer makes water a stronger fire-proofing.
pub const WET_DURATION: f32 = 8.0;
/// How long (game seconds) a creature stays Oiled after stepping in an unlit
/// oil puddle. Standing in the puddle keeps refreshing it. Longer leaves more
/// time to set the oiled creature alight.
pub const OILED_DURATION: f32 = 15.0;
/// Multiplier on every per-step chance (spread, burning oil, combat) that an
/// Oiled creature catches fire. Higher makes oiled targets near-certain to
/// ignite near any flame.
pub const OILED_IGNITE_MULT: f64 = 3.0;
/// Flammability floor for an Oiled creature. The oil burns even when the
/// creature underneath does not, so a non-`Combustible` (or barely flammable)
/// creature that is Oiled ignites as if it had at least this flammability.
pub const OILED_MIN_FLAMMABILITY: f32 = 0.5;
/// Burn damage multiplier while a creature is both Oiled and Burning. The oil
/// burns off with the fire: catching fire caps the Oiled timer at the burn's
/// duration (see `systems::effects::add_effect`).
pub const OILED_BURN_DAMAGE_MULT: f32 = 2.0;
/// Sprite tint multiplied into a Wet creature (cool, bluish). `(1,1,1)` is
/// untinted; lower red/green pushes it bluer.
pub const WET_SPRITE_TINT: (f32, f32, f32) = (0.7, 0.85, 1.0);
/// Sprite tint multiplied into an Oiled creature (dark and greasy brown).
pub const OILED_SPRITE_TINT: (f32, f32, f32) = (0.65, 0.55, 0.4);

// =============================================================================
// DAMAGE OVER TIME (Poisoned, Bleeding — see time_system::tick_dot_damage)
// =============================================================================
// DoT ticks go through `combat::apply_damage_dot`, which skips armor: venom and
// open wounds are not stopped by a breastplate. Protection/Barkskin and
// invulnerability still apply.

/// Damage per Poisoned tick.
pub const POISON_DAMAGE: i32 = 1;
/// Game seconds between Poisoned ticks. Lower makes poison bite harder.
pub const POISON_TICK_INTERVAL: f32 = 1.0;
/// Poisoned duration (game seconds) from a full-strength venomous bite (the
/// Giant Spider). Re-poisoning keeps the longer timer rather than stacking.
pub const POISON_DURATION: f32 = 6.0;
/// Damage per Bleeding tick.
pub const BLEED_DAMAGE: i32 = 1;
/// Game seconds between Bleeding ticks.
pub const BLEED_TICK_INTERVAL: f32 = 1.5;
/// Bleeding duration (game seconds) per wound. Re-bleeding keeps the longer
/// of the two timers rather than stacking.
pub const BLEED_DURATION: f32 = 6.0;

// =============================================================================
// FIRE SPREAD
// =============================================================================

/// How long (game-time seconds) a tall-grass tile burns before reverting to floor.
/// Long enough that a burning field smoulders for a while rather than flashing.
pub const GRASS_BURN_DURATION: f32 = 12.0;
/// Game-time between fire spread rolls (discrete steps regardless of frame rate).
pub const FIRE_STEP_INTERVAL: f32 = 0.5;
/// Per-step chance a burning grass tile ignites an adjacent grass tile (the main
/// wildfire propagation — kept slow so the fire front creeps).
pub const FIRE_GRASS_TO_GRASS_CHANCE: f64 = 0.22;
/// Per-step chance a burning creature ignites the grass tile it is standing in.
pub const FIRE_ENTITY_ON_GRASS_CHANCE: f64 = 0.5;
/// Per-step chance a burning creature ignites grass it is merely adjacent to (low
/// — fire spreads slowly from a body that isn't actually in the grass).
pub const FIRE_ENTITY_ADJACENT_GRASS_CHANCE: f64 = 0.06;
/// Per-step base chance a fire source ignites an adjacent creature, scaled by the
/// target's flammability.
pub const FIRE_SPREAD_ENTITY_CHANCE: f64 = 0.15;
/// Chance a burning combatant ignites the other on a melee hit, scaled by the
/// target's flammability.
pub const FIRE_COMBAT_IGNITE_CHANCE: f64 = 0.4;
/// Player flammability (chance multiplier for catching fire).
pub const PLAYER_FLAMMABILITY: f32 = 0.3;

// =============================================================================
// OIL, BARRELS, BRAZIERS, WATER (fire ecosystem — see systems/fire.rs)
// =============================================================================

/// How long (game-time seconds) an ignited oil puddle burns before the fuel
/// is spent and the puddle burns away.
pub const OIL_BURN_DURATION: f32 = 8.0;
/// Per-step chance a fire source (burning grass/oil/creature) ignites an oil
/// puddle it is on or adjacent to. Oil catches almost immediately.
pub const FIRE_TO_OIL_IGNITE_CHANCE: f64 = 0.9;
/// Per-step base chance burning oil ignites an entity standing IN it, scaled
/// by the entity's flammability. Much higher than mere adjacency.
pub const BURNING_OIL_STAND_IGNITE_CHANCE: f64 = 0.6;
/// Dark brown-black tint for oil puddle sprites (multiplied with the pool decal).
pub const OIL_PUDDLE_TINT: (f32, f32, f32) = (0.35, 0.25, 0.2);

/// Seconds between an oil barrel catching fire and its explosion.
pub const OIL_BARREL_FUSE_SECONDS: f32 = 3.0;
/// Seconds between an oil barrel being broken open (reduced to 0 HP) and its
/// explosion. The cracked barrel hisses and its blast radius is telegraphed
/// for this long; any further damage while the fuse runs detonates it at once.
/// Up gives more time to back off (or to line up a second hit); down makes a
/// broken barrel nearly as instant as the old behaviour. Keep it shorter than
/// [`OIL_BARREL_FUSE_SECONDS`]: a split barrel is the more urgent hazard.
pub const OIL_BARREL_BREAK_FUSE_SECONDS: f32 = 2.0;
/// How long a water splash (thrown flask, Call Rain) keeps an oil barrel
/// soaked, in game seconds. A soaked barrel is Wet, so fire cannot take hold
/// of it (it can still be broken by damage, and a broken one still blows). A
/// burning barrel that is soaked has its fire fuse put out. Up makes water a
/// longer-lasting way to defuse a room; down makes it a brief window.
pub const BARREL_SOAK_DURATION: f32 = 20.0;
/// Raw damage dealt by an oil barrel explosion (through `apply_damage`).
pub const OIL_BARREL_EXPLOSION_DAMAGE: i32 = 15;
/// Chebyshev radius of the explosion's damage.
pub const OIL_BARREL_EXPLOSION_RADIUS: i32 = 1;
/// Chebyshev radius over which the explosion sprays oil puddles (1..=this).
pub const OIL_BARREL_PUDDLE_RADIUS: i32 = 2;
/// Fraction of walkable tiles in the spray radius that receive a puddle.
pub const OIL_BARREL_PUDDLE_COVERAGE: f64 = 0.6;
/// Oil barrel flammability (catches from any nearby fire almost every step).
pub const OIL_BARREL_FLAMMABILITY: f32 = 1.0;
/// Hit points of an oil barrel (destroying one by damage sets it off).
pub const OIL_BARREL_HEALTH: i32 = 10;
/// Red/dark tint distinguishing oil barrels from food storage barrels.
pub const OIL_BARREL_TINT: (f32, f32, f32) = (1.0, 0.5, 0.4);
/// Oil barrels placed per floor among the Storage-room food barrels (min).
pub const OIL_BARRELS_STORAGE_MIN: usize = 1;
/// Oil barrels placed per floor among the Storage-room food barrels (max).
pub const OIL_BARRELS_STORAGE_MAX: usize = 2;
/// Chance per floor that 1-2 oil barrels also appear on corridor tiles.
pub const OIL_BARREL_CORRIDOR_FLOOR_CHANCE: f64 = 0.5;
/// Corridor oil barrels placed when the floor roll succeeds (min).
pub const OIL_BARRELS_CORRIDOR_MIN: usize = 1;
/// Corridor oil barrels placed when the floor roll succeeds (max).
pub const OIL_BARRELS_CORRIDOR_MAX: usize = 2;

/// How long a water splash keeps grass tiles too wet to ignite.
pub const WET_GRASS_DURATION: f32 = 30.0;
/// Splash radius (Chebyshev) of a thrown water flask.
pub const WATER_SPLASH_RADIUS: i32 = 1;
