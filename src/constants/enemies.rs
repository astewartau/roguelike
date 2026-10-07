//! Enemy stats and spawning constants.

/// Maximum distance from player for AI to be active (Manhattan distance)
/// Enemies further than this skip their turns entirely for performance
pub const AI_ACTIVE_RADIUS: i32 = 25;

// SKELETON
/// Skeleton health
pub const SKELETON_HEALTH: i32 = 40;
/// Skeleton maximum energy pool
pub const SKELETON_MAX_ENERGY: i32 = 3;
/// Skeleton action speed multiplier (1.5 = 50% faster than player)
pub const SKELETON_SPEED: f32 = 1.5;
/// Skeleton sight radius for chase AI
pub const SKELETON_SIGHT_RADIUS: i32 = 8;
/// Skeleton strength
pub const SKELETON_STRENGTH: i32 = 10;
/// Skeleton intelligence
pub const SKELETON_INTELLIGENCE: i32 = 1;
/// Skeleton agility
pub const SKELETON_AGILITY: i32 = 3;
/// Skeleton attack damage
pub const SKELETON_DAMAGE: i32 = 6;

// RAT
/// Rat health (weak)
pub const RAT_HEALTH: i32 = 30;
/// Rat maximum energy pool
pub const RAT_MAX_ENERGY: i32 = 4;
/// Rat action speed multiplier (fast and nimble)
pub const RAT_SPEED: f32 = 1.5;
/// Rat sight radius (poor eyesight)
pub const RAT_SIGHT_RADIUS: i32 = 5;
/// Rat strength (weak)
pub const RAT_STRENGTH: i32 = 3;
/// Rat intelligence
pub const RAT_INTELLIGENCE: i32 = 1;
/// Rat agility (quick)
pub const RAT_AGILITY: i32 = 8;
/// Rat attack damage (weak bite)
pub const RAT_DAMAGE: i32 = 5;

// SKELETON ARCHER
/// Skeleton archer health (slightly weaker than melee skeleton)
pub const SKELETON_ARCHER_HEALTH: i32 = 40;
/// Skeleton archer maximum energy pool
pub const SKELETON_ARCHER_MAX_ENERGY: i32 = 3;
/// Skeleton archer action speed (slower than melee skeletons - careful aim)
pub const SKELETON_ARCHER_SPEED: f32 = 0.7;
/// Skeleton archer sight radius (good vision for ranged)
pub const SKELETON_ARCHER_SIGHT_RADIUS: i32 = 10;
/// Skeleton archer strength
pub const SKELETON_ARCHER_STRENGTH: i32 = 6;
/// Skeleton archer intelligence
pub const SKELETON_ARCHER_INTELLIGENCE: i32 = 3;
/// Skeleton archer agility
pub const SKELETON_ARCHER_AGILITY: i32 = 5;
/// Skeleton archer melee damage (weak, prefers ranged)
pub const SKELETON_ARCHER_MELEE_DAMAGE: i32 = 3;
/// Skeleton archer bow damage
pub const SKELETON_ARCHER_BOW_DAMAGE: i32 = 8;
/// Minimum range for skeleton archer to use bow (won't shoot if closer)
pub const SKELETON_ARCHER_MIN_RANGE: i32 = 2;
/// Maximum range for skeleton archer bow
pub const SKELETON_ARCHER_MAX_RANGE: i32 = 8;
/// Cooldown between ranged attacks (seconds) - total time between shots ~3s
pub const RANGED_ATTACK_COOLDOWN: f32 = 1.5;

// GOBLIN - weak, fast melee swarmer for early floors
pub const GOBLIN_HEALTH: i32 = 22;
pub const GOBLIN_MAX_ENERGY: i32 = 4;
pub const GOBLIN_SPEED: f32 = 1.4;
pub const GOBLIN_SIGHT_RADIUS: i32 = 7;
pub const GOBLIN_STRENGTH: i32 = 6;
pub const GOBLIN_INTELLIGENCE: i32 = 2;
pub const GOBLIN_AGILITY: i32 = 7;
pub const GOBLIN_DAMAGE: i32 = 4;

// ORC - slow, heavy-hitting bruiser
pub const ORC_HEALTH: i32 = 75;
pub const ORC_MAX_ENERGY: i32 = 3;
pub const ORC_SPEED: f32 = 0.8;
pub const ORC_SIGHT_RADIUS: i32 = 8;
pub const ORC_STRENGTH: i32 = 15;
pub const ORC_INTELLIGENCE: i32 = 2;
pub const ORC_AGILITY: i32 = 3;
pub const ORC_DAMAGE: i32 = 13;

// ZOMBIE - very slow, high HP, relentless
pub const ZOMBIE_HEALTH: i32 = 60;
pub const ZOMBIE_MAX_ENERGY: i32 = 2;
pub const ZOMBIE_SPEED: f32 = 0.55;
pub const ZOMBIE_SIGHT_RADIUS: i32 = 7;
pub const ZOMBIE_STRENGTH: i32 = 12;
pub const ZOMBIE_INTELLIGENCE: i32 = 1;
pub const ZOMBIE_AGILITY: i32 = 1;
pub const ZOMBIE_DAMAGE: i32 = 8;

// GIANT BAT - very fast, fragile harasser
pub const BAT_HEALTH: i32 = 16;
pub const BAT_MAX_ENERGY: i32 = 5;
pub const BAT_SPEED: f32 = 2.2;
pub const BAT_SIGHT_RADIUS: i32 = 9;
pub const BAT_STRENGTH: i32 = 3;
pub const BAT_INTELLIGENCE: i32 = 2;
pub const BAT_AGILITY: i32 = 13;
pub const BAT_DAMAGE: i32 = 3;

// SLIME - slow, weak chip-damage fodder
pub const SLIME_HEALTH: i32 = 24;
pub const SLIME_MAX_ENERGY: i32 = 3;
pub const SLIME_SPEED: f32 = 0.7;
pub const SLIME_SIGHT_RADIUS: i32 = 5;
pub const SLIME_STRENGTH: i32 = 5;
pub const SLIME_INTELLIGENCE: i32 = 1;
pub const SLIME_AGILITY: i32 = 2;
pub const SLIME_DAMAGE: i32 = 4;

// GOBLIN SHAMAN - fragile support caster: heals/hastes allies, kites, raises the alarm
pub const GOBLIN_SHAMAN_HEALTH: i32 = 20;
pub const GOBLIN_SHAMAN_MAX_ENERGY: i32 = 4;
/// Slower than the goblins it patches up (1.0 = player-speed).
pub const GOBLIN_SHAMAN_SPEED: f32 = 1.0;
pub const GOBLIN_SHAMAN_SIGHT_RADIUS: i32 = 8;
pub const GOBLIN_SHAMAN_STRENGTH: i32 = 4;
pub const GOBLIN_SHAMAN_INTELLIGENCE: i32 = 5;
pub const GOBLIN_SHAMAN_AGILITY: i32 = 6;
pub const GOBLIN_SHAMAN_DAMAGE: i32 = 3;
/// Seconds between shaman support casts (heal or haste).
pub const SHAMAN_SUPPORT_COOLDOWN: f32 = 4.0;
/// HP restored by a shaman heal.
pub const SHAMAN_HEAL_AMOUNT: i32 = 8;
/// Range (Chebyshev tiles) within which the shaman can support an ally.
pub const SHAMAN_SUPPORT_RANGE: i32 = 8;
/// Duration of the shaman's haste (SpeedBoost) on an ally.
pub const SHAMAN_HASTE_DURATION: f32 = 6.0;
/// The shaman tries to stay at least this far from its threat target...
pub const SHAMAN_KITE_MIN: i32 = 3;
/// ...and no farther than this (closes back in to keep allies in support range).
pub const SHAMAN_KITE_MAX: i32 = 5;

// LESSER GIANT SPIDER - fast, fragile webspinner (early floors)
pub const LESSER_SPIDER_HEALTH: i32 = 18;
pub const LESSER_SPIDER_MAX_ENERGY: i32 = 4;
pub const LESSER_SPIDER_SPEED: f32 = 1.8;
pub const LESSER_SPIDER_SIGHT_RADIUS: i32 = 8;
pub const LESSER_SPIDER_STRENGTH: i32 = 4;
pub const LESSER_SPIDER_INTELLIGENCE: i32 = 1;
pub const LESSER_SPIDER_AGILITY: i32 = 9;
pub const LESSER_SPIDER_DAMAGE: i32 = 4;

// GIANT SPIDER - venomous webspinner (floor 3+); its bite Slows
pub const GIANT_SPIDER_HEALTH: i32 = 45;
pub const GIANT_SPIDER_MAX_ENERGY: i32 = 3;
pub const GIANT_SPIDER_SPEED: f32 = 1.3;
pub const GIANT_SPIDER_SIGHT_RADIUS: i32 = 8;
pub const GIANT_SPIDER_STRENGTH: i32 = 10;
pub const GIANT_SPIDER_INTELLIGENCE: i32 = 1;
pub const GIANT_SPIDER_AGILITY: i32 = 6;
pub const GIANT_SPIDER_DAMAGE: i32 = 9;
/// Duration of the Slowed venom applied by a giant spider's bite.
pub const SPIDER_VENOM_SLOW_DURATION: f32 = 4.0;

// WEBS (see systems/webs.rs and the web arm of systems/fire.rs)
/// Seconds between a spider laying webs (while chasing or idling).
pub const WEB_LAY_COOLDOWN: f32 = 8.0;
/// Maximum live webs a single spider maintains.
pub const WEB_MAX_PER_SPIDER: usize = 6;
/// Hard cap on live webs across the whole floor.
pub const WEB_TOTAL_CAP: usize = 24;
/// How long a non-spider is Rooted when it blunders into a web.
pub const WEB_ROOT_DURATION: f32 = 2.0;
/// Per-fire-step chance a web adjacent to any fire source ignites (webs are
/// tinder — fire leaps through a web-choked room).
pub const WEB_IGNITE_CHANCE: f64 = 0.8;
/// How long an ignited web burns before it (and the web) are consumed.
pub const WEB_BURN_DURATION: f32 = 3.0;
/// White gauze tint for web sprites (repurposed tall-grass sprite).
pub const WEB_TINT: (f32, f32, f32) = (1.6, 1.6, 1.8);

// BOSSES (every 3rd floor — see spawning::bosses)
/// Boss max-health multiplier over its base enemy.
pub const BOSS_HEALTH_MULT: f32 = 2.5;
/// Boss melee-damage multiplier over its base enemy.
pub const BOSS_DAMAGE_MULT: f32 = 1.5;
/// Boss stat (STR/INT/AGI) multiplier — also inflates its XP value.
pub const BOSS_STAT_MULT: f32 = 1.3;
/// Extra sight radius over the base enemy (slightly larger threat range).
pub const BOSS_SIGHT_BONUS: i32 = 2;
/// Additional flat scaling applied per boss cycle past the first three
/// (floor 12+ repeats the roster, tougher each lap).
pub const BOSS_CYCLE_HEALTH_MULT: f32 = 1.5;
/// Boss kills grant this multiple of the normal stat-derived XP.
pub const BOSS_XP_MULT: u32 = 2;
/// Gold multiplier on a boss corpse.
pub const BOSS_GOLD_MULT: u32 = 5;
/// Gnash's ground slam: cooldown / Chebyshev radius / stun / raw damage.
pub const BOSS_SLAM_COOLDOWN: f32 = 12.0;
pub const BOSS_SLAM_RADIUS: i32 = 2;
pub const BOSS_SLAM_STUN_DURATION: f32 = 2.0;
pub const BOSS_SLAM_DAMAGE: i32 = 6;
/// Silkrot's brood: cooldown / spiders per cast / max alive at once.
pub const BOSS_SPIDER_SPAWN_COOLDOWN: f32 = 20.0;
pub const BOSS_SPIDER_SPAWN_COUNT: usize = 2;
pub const BOSS_SPIDER_MINION_CAP: usize = 4;
/// Silkrot lays webs far faster than a normal spider.
pub const BOSS_WEB_LAY_COOLDOWN: f32 = 3.0;
/// Vhal's raise-dead: cooldown / bones-search radius (Chebyshev).
pub const BOSS_RAISE_COOLDOWN: f32 = 15.0;
pub const BOSS_RAISE_RANGE: i32 = 6;

/// Gold dropped by enemies (min, before the depth bonus)
/// (balance: was 1, out of line with 25-140g vendor prices)
pub const ENEMY_GOLD_DROP_MIN: u32 = 2;
/// Gold dropped by enemies (max, before the depth bonus)
pub const ENEMY_GOLD_DROP_MAX: u32 = 12;
/// Extra gold per floor of depth on every enemy drop, so kill income keeps
/// pace with the vendor's deeper (pricier) stock instead of staying flat
pub const ENEMY_GOLD_PER_FLOOR: u32 = 3;

// THREAT SYSTEM
/// Threat generated per point of damage dealt
pub const THREAT_PER_DAMAGE: f32 = 1.0;
/// Passive threat added per AI decision cycle when target is visible (enemies only)
pub const THREAT_PASSIVE_VISIBILITY: f32 = 0.5;
/// Threat decay rate per second for visible targets (slow)
pub const THREAT_DECAY_VISIBLE: f32 = 0.5;
/// Threat decay rate per second for non-visible targets (fast)
pub const THREAT_DECAY_HIDDEN: f32 = 5.0;
/// Minimum threat floor — threat decays to this instead of zero
pub const THREAT_MINIMUM: f32 = 0.1;
/// How long (seconds) an entry can sit at the minimum before being pruned.
/// Kept short so an enemy that loses the player (and can't reach the last-known
/// spot) gives up and returns to wandering instead of lingering.
pub const THREAT_MEMORY_DURATION: f32 = 8.0;
/// Multiplier for companion threat when assisting player's target (lower = less priority)
pub const THREAT_COMPANION_ASSIST_MULT: f32 = 0.5;

// =============================================================================
// AWARENESS: SLEEP, STEALTH, NOISE, SHOUT
// =============================================================================

/// Chance (0-1) that an enemy spawns asleep rather than awake-but-unaware.
pub const SLEEP_CHANCE: f64 = 0.75;
/// Threat seeded on a target when an enemy is woken by noise or a shout
/// (enough to sit above the minimum so it investigates).
pub const WAKE_THREAT: f32 = 3.0;

// --- Gradual detection (alertness meter) ---
// An unaware enemy with line-of-sight builds "alertness" each turn based on how
// close the player is; crossing the threshold wakes it. This makes waking
// gradual (a ripple, not a flash) and distance/agility dependent.

/// Alertness needed to fully wake.
pub const ALERTNESS_WAKE_THRESHOLD: f32 = 100.0;
/// Base alertness gained per turn at point-blank range, before stealth/sleep
/// modifiers (scaled down by distance, player agility, and sleep depth).
pub const ALERTNESS_BASE_GAIN: f32 = 120.0;
/// Alertness lost per turn when the enemy currently can't detect the player.
pub const ALERTNESS_DECAY: f32 = 25.0;
/// Each point of player agility above 10 adds this to the stealth divisor
/// (higher agility => slower detection). agi 14 => /1.24, agi 20 => /1.6.
pub const AGILITY_STEALTH_FACTOR: f32 = 0.06;
/// Multiplier on alertness gain while the enemy is asleep (deeper sleep = slower).
pub const ASLEEP_ALERT_MULT: f32 = 0.6;

// --- Sneak (player crouch toggle) ---
/// Movement speed multiplier while sneaking (slower = the tradeoff).
pub const SNEAK_SPEED_MULT: f32 = 0.6;
/// Multiplier on how fast unaware enemies gain alertness on a sneaking player.
pub const SNEAK_ALERTNESS_MULT: f32 = 0.35;
/// Multiplier on a not-yet-alerted (Idle) enemy's sight range vs a sneaking
/// player — lets you slip past awake-but-unalerted wanderers.
pub const SNEAK_SIGHT_MULT: f32 = 0.5;

/// Effective detection radius (tiles) when the target is standing in concealing
/// terrain (tall grass). At 1, only an adjacent enemy can pick you out of the
/// grass — everyone else loses your exact position (a ranged enemy will still
/// fire at the tile it last saw you on, so keep moving).
pub const CONCEAL_SIGHT_RADIUS: i32 = 1;

/// Damage multiplier for hitting an unaware enemy (sneak attack).
pub const SNEAK_ATTACK_MULT: f32 = 2.0;

/// Noise radii (tiles) for combat actions — louder actions wake sleepers farther.
pub const MELEE_NOISE_RADIUS: i32 = 4;
pub const RANGED_NOISE_RADIUS: i32 = 6;
pub const EXPLOSION_NOISE_RADIUS: i32 = 10;

/// Alarm shout: channel duration (seconds), wake radius (tiles), cooldown (seconds).
pub const SHOUT_DURATION: f32 = 1.5;
pub const SHOUT_WAKE_RADIUS: i32 = 8;
pub const SHOUT_COOLDOWN: f32 = 12.0;

// =============================================================================
// MORALE
// =============================================================================

/// Enemy flees (becomes Feared) when below this fraction of max HP and a morale
/// check fails.
pub const MORALE_HP_THRESHOLD: f32 = 0.3;
/// Chance per damaging hit (while below the HP threshold) that the enemy panics.
pub const MORALE_FLEE_CHANCE: f64 = 0.4;
/// How long a panicked enemy flees (seconds).
pub const MORALE_FEAR_DURATION: f32 = 4.0;
/// Radius (tiles) within which allies make a morale check when a comrade dies.
pub const ALLY_DEATH_MORALE_RADIUS: i32 = 5;
/// Chance an in-radius ally panics when a comrade dies.
pub const ALLY_DEATH_MORALE_CHANCE: f64 = 0.3;
