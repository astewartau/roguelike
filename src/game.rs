//! Core game initialization and state management.
//!
//! Handles world creation, entity spawning, and game state.

use crate::camera::Camera;
use crate::components::{
    Actor, Attackable, BlocksMovement, BlocksVision, ChaseAI, Container, Door, Equipment,
    Experience, Health, Inventory, ItemType, Player, Position, RangedWeapon, Sprite, Stats,
    VisualPosition, Weapon,
};
use crate::constants::*;
use crate::grid::Grid;
use crate::spawning;
use crate::tile::tile_ids;
use crate::time_system::{ActionScheduler, GameClock};
use glam::Vec2;
use hecs::{Entity, World};
use rand::Rng;

/// Initialize the game world with player, enemies, and objects
/// Returns (world, player_entity, player_start_position)
pub fn init_world(grid: &Grid) -> (World, Entity, Position) {
    let mut world = World::new();

    // Find a walkable tile to spawn the player
    let mut player_start = Position::new(50, 50);
    'find_spawn: for y in 0..grid.height as i32 {
        for x in 0..grid.width as i32 {
            if let Some(tile) = grid.get(x, y) {
                if tile.tile_type.is_walkable() {
                    player_start = Position::new(x, y);
                    break 'find_spawn;
                }
            }
        }
    }

    // Spawn player with new time system Actor
    let player_entity = world.spawn((
        player_start,
        VisualPosition::from_position(&player_start),
        Sprite::new(tile_ids::PLAYER),
        Player,
        Actor::new(PLAYER_MAX_ENERGY, PLAYER_SPEED),
        Health::with_regen(
            PLAYER_STARTING_HEALTH,
            PLAYER_HP_REGEN_AMOUNT,
            PLAYER_HP_REGEN_INTERVAL,
        ),
        Stats::new(PLAYER_STRENGTH, PLAYER_INTELLIGENCE, PLAYER_AGILITY),
        Inventory::new(),
        Equipment::with_weapons(Weapon::sword(), RangedWeapon::bow()),
        BlocksMovement,
        Experience::new(),
        Attackable,
    ));

    // Spawn chests (block movement until opened)
    for (x, y) in &grid.chest_positions {
        let pos = Position::new(*x, *y);
        world.spawn((
            pos,
            VisualPosition::from_position(&pos),
            Sprite::new(tile_ids::CHEST_CLOSED),
            Container::new(vec![ItemType::HealthPotion]),
            BlocksMovement,
        ));
    }

    // Spawn doors (closed by default, block vision and movement)
    for (x, y) in &grid.door_positions {
        let pos = Position::new(*x, *y);
        world.spawn((
            pos,
            VisualPosition::from_position(&pos),
            Sprite::new(tile_ids::DOOR),
            Door::new(),
            BlocksVision,
            BlocksMovement,
        ));
    }

    // Spawn enemies using data-driven spawning system
    let mut rng = rand::thread_rng();
    let walkable_tiles: Vec<(i32, i32)> = (0..grid.height as i32)
        .flat_map(|y| (0..grid.width as i32).map(move |x| (x, y)))
        .filter(|&(x, y)| grid.get(x, y).map(|t| t.tile_type.is_walkable()).unwrap_or(false))
        .collect();

    let spawn_config = spawning::SpawnConfig::level_1();
    spawn_config.spawn_all(
        &mut world,
        &walkable_tiles,
        &[(player_start.x, player_start.y)],
        &mut rng,
    );

    (world, player_entity, player_start)
}

/// Set up the camera to track the player
pub fn setup_camera(camera: &mut Camera, player_start: &Position) {
    camera.set_tracking_target(Vec2::new(player_start.x as f32, player_start.y as f32));
}

/// Handle interactions with open chests/containers
pub fn handle_chest_interaction(
    world: &mut World,
    player_entity: Entity,
    chest_id: Entity,
    take_all: bool,
    take_gold: bool,
    item_to_take: Option<usize>,
    events: Option<&mut crate::events::EventQueue>,
) -> bool {
    // Returns true if chest should be closed
    if take_all {
        crate::systems::take_all_from_container(world, player_entity, chest_id, events);
        true
    } else if take_gold {
        crate::systems::take_gold_from_container(world, player_entity, chest_id, events);
        false
    } else if let Some(item_index) = item_to_take {
        crate::systems::take_item_from_container(world, player_entity, chest_id, item_index, events);
        false
    } else {
        false
    }
}

/// Initialize all AI actors with their first action in the time system
pub fn initialize_ai_actors(
    world: &mut World,
    grid: &Grid,
    player_entity: Entity,
    clock: &GameClock,
    scheduler: &mut ActionScheduler,
    events: &mut crate::events::EventQueue,
    rng: &mut impl Rng,
) {
    // Collect all AI entities (entities with both Actor and ChaseAI)
    let ai_entities: Vec<Entity> = world
        .query::<(&Actor, &ChaseAI)>()
        .iter()
        .map(|(id, _)| id)
        .collect();

    // Initialize each AI entity's first action
    for entity in ai_entities {
        crate::systems::ai::decide_action(world, grid, entity, player_entity, clock, scheduler, events, rng);
    }
}

/// Initialize a single AI actor (used when spawning new enemies mid-game)
pub fn initialize_single_ai_actor(
    world: &mut World,
    grid: &Grid,
    entity: Entity,
    player_entity: Entity,
    clock: &GameClock,
    scheduler: &mut ActionScheduler,
    events: &mut crate::events::EventQueue,
    rng: &mut impl Rng,
) {
    crate::systems::ai::decide_action(world, grid, entity, player_entity, clock, scheduler, events, rng);
}
