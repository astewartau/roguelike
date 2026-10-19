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

/// How long a water splash keeps tiles too wet for damp-sensitive fuel
/// (grass, fungus) to ignite.
pub const WET_TILE_DURATION: f32 = 30.0;
// =============================================================================
// COATINGS: WET AND OILY (what creatures carry on them — see systems/coatings.rs)
// =============================================================================

/// How long (game seconds) a soaking lasts once you are out of the water.
/// Longer makes water a more durable fire shield; standing in water keeps
/// refreshing it.
pub const WET_DURATION: f32 = 30.0;
/// How many times faster you dry off standing on or next to a fire. Higher
/// makes "dry off by the brazier" quicker and a wet shield thinner in a blaze.
pub const WET_DRY_NEAR_FIRE_MULT: f32 = 4.0;
/// How long (game seconds) an oil coating lasts. Longer means a longer drip
/// trail and a longer window to set the victim alight.
pub const OILY_DURATION: f32 = 40.0;
/// Flammability multiplier while Oily (result capped at 1.0). Higher makes
/// an oily creature catch from fire nearby almost every step.
pub const OILY_FLAMMABILITY_MULT: f32 = 3.0;
/// Minimum flammability while Oily, so things that never burn on their own
/// (bone, slime) will burn once coated.
pub const OILY_MIN_FLAMMABILITY: f32 = 0.6;
/// Chance per step that an oily walker drips oil on the tile it leaves.
/// Higher makes an unbroken fuse trail; lower leaves gaps fire has to jump.
pub const OILY_DRIP_CHANCE: f64 = 0.5;
/// How long a drip of oil burns: shorter than a full puddle
/// (`OIL_BURN_DURATION`), since there is less fuel in it.
pub const OIL_DRIP_BURN_DURATION: f32 = 4.0;

/// Splash radius (Chebyshev) of a thrown water flask.
pub const WATER_SPLASH_RADIUS: i32 = 1;
