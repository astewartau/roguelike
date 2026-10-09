//! Enemy stats and spawning constants.

/// Maximum distance from player for AI to be active (Manhattan distance)
/// Enemies further than this skip their turns entirely for performance
pub const AI_ACTIVE_RADIUS: i32 = 25;


// SKELETON
/// Skeleton health
pub const SKELETON_HEALTH: i32 = 40;
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
/// Chance a connecting rat bite opens a wound (Bleeding for `BLEED_DURATION`).
/// Higher makes rat packs a slow attrition threat rather than chip damage.
pub const RAT_BLEED_CHANCE: f32 = 0.3;
/// Smallest rat pack spawned together on a floor. Rats are counted
/// individually against the floor roster: a pack spends as many roster rats
/// as it has members.
pub const RAT_PACK_MIN: usize = 2;
/// Largest rat pack spawned together.
pub const RAT_PACK_MAX: usize = 4;
/// How far (in steps over walkable spawn tiles, from the first rat) the rest
/// of a pack may be placed. Small keeps packs huddled in one spot.
pub const RAT_PACK_SPAWN_SPREAD: i32 = 3;
/// Chebyshev radius within which pack rats count as together. A pack rat
/// with no living packmate this close is alone: it flees instead of
/// fighting (unless cornered). With its pack it ignores the wounded-morale
/// panic, and an alert spreads to every packmate this close. Larger makes
/// packs harder to split up.
pub const RAT_PACK_RADIUS: i32 = 4;

// SKELETON ARCHER
/// Skeleton archer health (slightly weaker than melee skeleton)
pub const SKELETON_ARCHER_HEALTH: i32 = 40;
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
pub const GOBLIN_SPEED: f32 = 1.4;
pub const GOBLIN_SIGHT_RADIUS: i32 = 7;
pub const GOBLIN_STRENGTH: i32 = 6;
pub const GOBLIN_INTELLIGENCE: i32 = 2;
pub const GOBLIN_AGILITY: i32 = 7;
pub const GOBLIN_DAMAGE: i32 = 4;

// ORC - slow, heavy-hitting bruiser
pub const ORC_HEALTH: i32 = 75;
pub const ORC_SPEED: f32 = 0.8;
pub const ORC_SIGHT_RADIUS: i32 = 8;
pub const ORC_STRENGTH: i32 = 15;
pub const ORC_INTELLIGENCE: i32 = 2;
pub const ORC_AGILITY: i32 = 3;
pub const ORC_DAMAGE: i32 = 13;
/// Closest a target can be (Chebyshev tiles, on one of the eight straight
/// lines from the orc) for it to start a charge. Below this it just walks up
/// and swings. Lower makes the charge an opener at almost any distance.
pub const ORC_CHARGE_MIN_RANGE: i32 = 3;
/// Farthest a charge target can be. The dash itself runs one tile further
/// (`ORC_CHARGE_MAX_RANGE + 1`) so a target that backs straight away along
/// the lane is still caught. Higher makes orcs threaten whole rooms.
pub const ORC_CHARGE_MAX_RANGE: i32 = 6;
/// Base wind-up of the charge in game seconds (divided by the orc's speed,
/// like every action): the window to step out of the telegraphed lane.
/// Shorter makes the charge much harder to dodge.
pub const ORC_CHARGE_WINDUP: f32 = 0.8;
/// Damage multiplier on a charge that connects, over the orc's normal melee
/// hit (guard, armour and wards still apply after it).
pub const ORC_CHARGE_DAMAGE_MULT: f32 = 1.5;
/// How long (game seconds) an orc that charges into a wall or furniture is
/// Stunned: the punishment for baiting it into one. Longer turns a dodged
/// charge into a free beating.
pub const ORC_WALL_STUN: f32 = 1.5;
/// How long (game seconds) an orc that charges the full distance without
/// hitting anything (or is stopped by one of its own) stumbles, Stunned.
pub const ORC_STUMBLE_DURATION: f32 = 0.8;
/// Game seconds between charges, counted from the wind-up starting.
pub const ORC_CHARGE_COOLDOWN: f32 = 10.0;

// ZOMBIE - very slow, high HP, relentless
pub const ZOMBIE_HEALTH: i32 = 60;
pub const ZOMBIE_SPEED: f32 = 0.55;
pub const ZOMBIE_SIGHT_RADIUS: i32 = 7;
pub const ZOMBIE_STRENGTH: i32 = 12;
pub const ZOMBIE_INTELLIGENCE: i32 = 1;
pub const ZOMBIE_AGILITY: i32 = 1;
pub const ZOMBIE_DAMAGE: i32 = 8;
/// How long (game seconds) a zombie's connecting hit holds its victim Grabbed:
/// unable to walk away, though still free to swing back or use abilities. The
/// grab also ends early if the zombie dies, is stunned, or stops being
/// adjacent. Longer makes zombies far more dangerous to melee; shorter turns
/// the grab into a mere stutter-step.
pub const ZOMBIE_GRAB_DURATION: f32 = 1.5;

// GIANT BAT - very fast, fragile harasser
pub const BAT_HEALTH: i32 = 16;
pub const BAT_SPEED: f32 = 2.2;
pub const BAT_SIGHT_RADIUS: i32 = 9;
pub const BAT_STRENGTH: i32 = 3;
pub const BAT_INTELLIGENCE: i32 = 2;
pub const BAT_AGILITY: i32 = 13;
pub const BAT_DAMAGE: i32 = 3;
/// Hit-and-run: after every melee swing (hit or miss) a bat breaks off and
/// flutters away from its target for this many game seconds before diving
/// back in. Longer means fewer bites and easier ranged shots at it; shorter
/// makes bats behave like ordinary (very fast) chasers.
pub const BAT_RETREAT_DURATION: f32 = 2.5;

// SLIME - slow, weak chip-damage fodder
pub const SLIME_HEALTH: i32 = 24;
pub const SLIME_SPEED: f32 = 0.7;
pub const SLIME_SIGHT_RADIUS: i32 = 5;
pub const SLIME_STRENGTH: i32 = 5;
pub const SLIME_INTELLIGENCE: i32 = 1;
pub const SLIME_AGILITY: i32 = 2;
pub const SLIME_DAMAGE: i32 = 4;
/// A slime splits the first time a hit leaves it at or below this fraction of
/// its max HP (and it survives). Higher splits earlier, while there is more HP
/// left to share between the halves.
pub const SLIME_SPLIT_HP_FRACTION: f32 = 0.5;
/// How many generations of splitting a slime lineage allows. 1 = an original
/// slime splits once into two smaller ones, and those never split again.
/// Raising it multiplies the number of slimes a single kill can produce.
pub const SLIME_MAX_SPLITS: u8 = 1;
/// Tint applied to split (smaller) slimes so they read as the lesser halves.
/// Multiplies the sprite (1.0 = untinted); a paler, washed-out green.
pub const SLIME_SPLIT_TINT: (f32, f32, f32) = (0.75, 1.0, 0.75);

// GOBLIN SHAMAN - fragile support caster: heals/hastes allies, kites, raises the alarm
pub const GOBLIN_SHAMAN_HEALTH: i32 = 20;
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
pub const LESSER_SPIDER_SPEED: f32 = 1.8;
pub const LESSER_SPIDER_SIGHT_RADIUS: i32 = 8;
pub const LESSER_SPIDER_STRENGTH: i32 = 4;
pub const LESSER_SPIDER_INTELLIGENCE: i32 = 1;
pub const LESSER_SPIDER_AGILITY: i32 = 9;
pub const LESSER_SPIDER_DAMAGE: i32 = 4;

// GIANT SPIDER - venomous webspinner (floor 3+); its bite Slows
pub const GIANT_SPIDER_HEALTH: i32 = 45;
pub const GIANT_SPIDER_SPEED: f32 = 1.3;
pub const GIANT_SPIDER_SIGHT_RADIUS: i32 = 8;
pub const GIANT_SPIDER_STRENGTH: i32 = 10;
pub const GIANT_SPIDER_INTELLIGENCE: i32 = 1;
pub const GIANT_SPIDER_AGILITY: i32 = 6;
pub const GIANT_SPIDER_DAMAGE: i32 = 9;
/// Duration of the Slowed venom applied by a giant spider's bite.
pub const SPIDER_VENOM_SLOW_DURATION: f32 = 4.0;
/// Poisoned duration from a lesser giant spider's bite (weaker venom than the
/// Giant Spider's `POISON_DURATION`). 0 would make lesser spiders non-venomous.
pub const LESSER_SPIDER_POISON_DURATION: f32 = 3.0;

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
/// Seconds Gnash spends winding up a ground slam before it lands (scaled by
/// his speed like any action). The shockwave hits whatever is inside
/// `BOSS_SLAM_RADIUS` of him when the wind-up *completes*, so this is the
/// player's window to step out. Longer is more forgiving; at 0 the slam is
/// unavoidable again.
pub const BOSS_SLAM_WINDUP: f32 = 1.0;
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
/// Multiplier on how fast unaware enemies gain alertness on a Wet player:
/// dripping, squelching footsteps are noisy. Stacks with sneaking (a wet sneak
/// is still quieter than a wet walk). 1.0 would make water stealth-neutral.
pub const WET_STEALTH_PENALTY: f32 = 1.5;

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
