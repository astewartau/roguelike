//! Core game state - owns the simulation data.

use crate::components::{PlayerClass, Position};
use crate::constants::*;
use crate::events::EventQueue;
use crate::grid::Grid;
use crate::time_system::{ActionScheduler, GameClock};

use hecs::{Entity, World};
use std::collections::HashMap;

use super::initialization;
use super::floor_transition::SavedFloor;

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
}

impl GameState {
    /// Create a new game state with initialized world for the given player class.
    pub fn new(player_class: PlayerClass) -> Self {
        let grid = Grid::new(DUNGEON_DEFAULT_WIDTH, DUNGEON_DEFAULT_HEIGHT);
        let (world, player_entity, _player_start) = initialization::init_world(&grid, player_class);

        let game_clock = GameClock::new();
        let action_scheduler = ActionScheduler::new();

        Self {
            world,
            grid,
            player_entity,
            current_floor: 0,
            floors: HashMap::new(),
            game_clock,
            action_scheduler,
            fov_dirty: true, // Always calculate FOV on first frame
        }
    }

    /// Initialize AI actors after world creation.
    /// Must be called separately because it needs mutable access to events.
    pub fn initialize_ai(&mut self, events: &mut EventQueue) {
        let mut rng = rand::thread_rng();
        initialization::initialize_ai_actors(
            &mut self.world,
            &self.grid,
            self.player_entity,
            &self.game_clock,
            &mut self.action_scheduler,
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
