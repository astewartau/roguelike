//! Core game state - owns the simulation data.

use crate::active_ai_tracker::ActiveAITracker;
use crate::components::{PlayerClass, Position};
use crate::constants::*;
use crate::events::EventQueue;
use crate::grid::Grid;
use crate::spatial_cache::SpatialCache;
use crate::time_system::{ActionScheduler, GameClock};

use hecs::{Entity, World};
use rand::rngs::StdRng;
use rand::SeedableRng;
use std::collections::HashMap;

use super::initialization;
use super::floor_transition::SavedFloor;

/// Derive the seed for one floor's layout + loot from the run seed
/// (splitmix64-style mix). Each floor gets an independent stream, so the
/// order floors are visited (or revisited) never shifts their layouts.
pub fn floor_seed(run_seed: u64, floor: u32) -> u64 {
    let mut z = run_seed.wrapping_add((floor as u64 + 1).wrapping_mul(0x9E37_79B9_7F4A_7C15));
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

/// Core game state - owns all simulation data.
pub struct GameState {
    /// The ECS world
    pub world: World,

    /// Current floor grid
    pub grid: Grid,

    /// Player entity handle
    pub player_entity: Entity,

    /// Current floor number
    pub current_floor: u32,

    /// Saved floors for multi-level dungeon
    pub floors: HashMap<u32, SavedFloor>,

    /// Game clock (simulation time)
    pub game_clock: GameClock,

    /// Action scheduler for turn-based time
    pub action_scheduler: ActionScheduler,

    /// Whether FOV needs recalculation (dirty flag for performance)
    pub fov_dirty: bool,

    /// Spatial cache for efficient blocking position lookups
    pub spatial_cache: SpatialCache,

    /// Active AI tracker for dormant entity management
    pub active_ai_tracker: ActiveAITracker,

    /// Accumulated game-time toward the next fire-spread step.
    pub fire_accumulator: f32,

    /// Accumulated game-time toward the next identification step.
    pub identify_accumulator: f32,

    /// Accumulated game-time toward the next hunger/fatigue survival step.
    pub survival_accumulator: f32,

    /// The seed this run was generated from (shown on the game-over screen).
    pub seed: u64,

    /// Seeded gameplay rng for rolls made during play (enemy loot drops,
    /// altar outcomes, ...). Dungeon layout and floor loot use per-floor
    /// rngs derived via `floor_seed` instead, so they only depend on the seed.
    pub rng: StdRng,

    /// The class this run was started with (for the run-history record).
    pub player_class: PlayerClass,

    /// Hostile enemies slain this run.
    pub kills: u32,

    /// Whether this run has already been written to the history file
    /// (guards against double-recording on death + return-to-menu).
    pub run_recorded: bool,
}

impl GameState {
    /// Create a new game state for the given player class and run seed.
    pub fn new(player_class: PlayerClass, seed: u64) -> Self {
        // Floor 0's layout and loot come from a per-floor rng derived from
        // the run seed (deeper floors derive theirs on transition).
        let mut floor_rng = StdRng::seed_from_u64(floor_seed(seed, 0));
        let grid = Grid::new_floor(
            DUNGEON_DEFAULT_WIDTH,
            DUNGEON_DEFAULT_HEIGHT,
            0,
            &mut floor_rng,
        );
        let (world, player_entity, _player_start) =
            initialization::init_world(&grid, player_class, &mut floor_rng);

        let game_clock = GameClock::new();
        let action_scheduler = ActionScheduler::new();

        // Build spatial cache from initial world state
        let spatial_cache = SpatialCache::rebuild_from_world(&world);

        // Initialize active AI tracker (will be populated in initialize_ai)
        let active_ai_tracker = ActiveAITracker::new();

        Self {
            world,
            grid,
            player_entity,
            current_floor: 0,
            floors: HashMap::new(),
            game_clock,
            action_scheduler,
            fov_dirty: true, // Always calculate FOV on first frame
            spatial_cache,
            active_ai_tracker,
            fire_accumulator: 0.0,
            identify_accumulator: 0.0,
            survival_accumulator: 0.0,
            seed,
            rng: StdRng::seed_from_u64(floor_seed(seed, u32::MAX)),
            player_class,
            kills: 0,
            run_recorded: false,
        }
    }

    /// Initialize AI actors after world creation.
    /// Must be called separately because it needs mutable access to events.
    pub fn initialize_ai(&mut self, events: &mut EventQueue) {
        // Get player position for active AI tracking
        let player_pos = self
            .world
            .get::<&Position>(self.player_entity)
            .map(|p| (p.x, p.y))
            .unwrap_or((0, 0));

        // Initialize active AI tracker based on player position
        self.active_ai_tracker
            .initialize_from_world(&self.world, player_pos);

        let mut rng = rand::thread_rng();
        initialization::initialize_ai_actors(
            &mut self.world,
            &self.grid,
            self.player_entity,
            &self.game_clock,
            &mut self.action_scheduler,
            &mut self.active_ai_tracker,
            &self.spatial_cache,
            events,
            &mut rng,
        );
    }

    /// Get the player's starting position for camera setup.
    pub fn player_start_position(&self) -> Option<(f32, f32)> {
        self.world
            .get::<&Position>(self.player_entity)
            .ok()
            .map(|p| (p.x as f32, p.y as f32))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_floor_seed_is_stable() {
        // The derivation must never change between calls (or releases would
        // silently break shared seeds).
        assert_eq!(floor_seed(12345, 0), floor_seed(12345, 0));
        assert_eq!(floor_seed(u64::MAX, 99), floor_seed(u64::MAX, 99));
    }

    #[test]
    fn test_floor_seed_varies_by_floor_and_run() {
        let run = 42;
        let seeds: Vec<u64> = (0..8).map(|f| floor_seed(run, f)).collect();
        for i in 0..seeds.len() {
            for j in (i + 1)..seeds.len() {
                assert_ne!(seeds[i], seeds[j], "floors {i} and {j} collided");
            }
        }
        assert_ne!(floor_seed(1, 3), floor_seed(2, 3));
    }

    #[test]
    fn test_same_seed_same_floor_layout() {
        use rand::rngs::StdRng;
        use rand::SeedableRng;

        let seed = 777u64;
        let make = |floor: u32| {
            let mut rng = StdRng::seed_from_u64(floor_seed(seed, floor));
            Grid::new_floor(60, 60, floor, &mut rng)
        };

        let a = make(1);
        let b = make(1);
        let tiles_a: Vec<_> = a.tiles.iter().map(|t| t.tile_type).collect();
        let tiles_b: Vec<_> = b.tiles.iter().map(|t| t.tile_type).collect();
        assert_eq!(tiles_a, tiles_b, "same seed+floor must give identical tiles");
        assert_eq!(a.chest_positions, b.chest_positions);
        assert_eq!(a.stairs_down_pos, b.stairs_down_pos);
        assert_eq!(a.trap_positions, b.trap_positions);

        // A different floor of the same run gets its own layout.
        let c = make(2);
        let tiles_c: Vec<_> = c.tiles.iter().map(|t| t.tile_type).collect();
        assert_ne!(tiles_a, tiles_c, "different floors should differ");
    }

    #[test]
    fn test_same_seed_same_floor_loot() {
        use rand::rngs::StdRng;
        use rand::SeedableRng;

        let seed = 4242u64;
        let build = || {
            let mut rng = StdRng::seed_from_u64(floor_seed(seed, 0));
            let grid = Grid::new_floor(
                DUNGEON_DEFAULT_WIDTH,
                DUNGEON_DEFAULT_HEIGHT,
                0,
                &mut rng,
            );
            let (world, _, _) =
                initialization::init_world(&grid, PlayerClass::Fighter, &mut rng);

            // Snapshot every container's position, contents, and gold.
            let mut loot: Vec<((i32, i32), Vec<String>, u32)> = world
                .query::<(&Position, &crate::components::Container)>()
                .iter()
                .map(|(_, (pos, container))| {
                    (
                        (pos.x, pos.y),
                        container
                            .items
                            .iter()
                            .map(|item| item.display_name())
                            .collect(),
                        container.gold,
                    )
                })
                .collect();
            loot.sort();
            loot
        };

        let a = build();
        assert!(!a.is_empty(), "floor 0 should have containers");
        assert_eq!(a, build(), "same seed must produce identical floor loot");
    }

    #[test]
    fn test_same_seed_same_enemies_and_boss() {
        use rand::rngs::StdRng;
        use rand::SeedableRng;

        let seed = 31337u64;
        // Floor 3 is a boss floor: the roster AND the named boss must be
        // identical across two builds of the same seed.
        let build = || {
            let mut rng = StdRng::seed_from_u64(floor_seed(seed, 3));
            let grid = Grid::new_floor(
                DUNGEON_DEFAULT_WIDTH,
                DUNGEON_DEFAULT_HEIGHT,
                3,
                &mut rng,
            );
            let mut world = World::new();
            let pos = Position::new(1, 1);
            let player = world.spawn((
                pos,
                crate::components::VisualPosition::from_position(&pos),
            ));

            let clock = GameClock::new();
            let mut scheduler = ActionScheduler::new();
            let mut tracker = ActiveAITracker::new();
            let cache = SpatialCache::rebuild_from_world(&world);
            let mut events = EventQueue::new();
            initialization::spawn_floor_entities(
                &mut world, &grid, player, (1, 1), 3,
                &clock, &mut scheduler, &mut tracker, &cache, &mut events, &mut rng,
            );

            // Snapshot every hostile: name + position; and the boss by name.
            let mut roster: Vec<(String, i32, i32)> = world
                .query::<(&Position, &crate::components::Name, &crate::components::ChaseAI)>()
                .iter()
                .map(|(_, (p, n, _))| (n.0.clone(), p.x, p.y))
                .collect();
            roster.sort();
            let boss: Vec<String> = world
                .query::<(&crate::components::Boss, &crate::components::Name)>()
                .iter()
                .map(|(_, (_, n))| n.0.clone())
                .collect();
            (roster, boss)
        };

        let (roster_a, boss_a) = build();
        assert!(!roster_a.is_empty(), "floor 3 should have enemies");
        assert_eq!(
            boss_a,
            vec!["Gnash, Orc Warlord".to_string()],
            "floor 3 spawns its named boss"
        );
        let (roster_b, boss_b) = build();
        assert_eq!(roster_a, roster_b, "same seed must spawn identical enemies");
        assert_eq!(boss_a, boss_b, "same seed must spawn the same boss");
    }
}
