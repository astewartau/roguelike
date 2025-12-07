//! Core game initialization and state management.
//!
//! Handles world creation, entity spawning, and game state.

use crate::camera::Camera;
use crate::components::{
    Actor, Attackable, BlocksMovement, BlocksVision, Container, Door, Equipment, Experience, Health,
    Inventory, ItemType, Player, Position, Sprite, Stats, VisualPosition, Weapon,
};
use crate::constants::*;
use crate::grid::Grid;
use crate::spawning;
use crate::tile::tile_ids;
use glam::Vec2;
use hecs::{Entity, World};

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

    // Spawn player
    let player_entity = world.spawn((
        player_start,
        VisualPosition::from_position(&player_start),
        Sprite::new(tile_ids::PLAYER),
        Player,
        Actor::new(PLAYER_SPEED),
        Health::new(PLAYER_STARTING_HEALTH),
        Stats::new(PLAYER_STRENGTH, PLAYER_INTELLIGENCE, PLAYER_AGILITY),
        Inventory::new(),
        Equipment::with_weapon(Weapon::sword()),
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

/// Use an item from the player's inventory
pub fn use_item(world: &mut World, player_entity: Entity, item_index: usize) {
    if let Ok(mut inv) = world.get::<&mut Inventory>(player_entity) {
        if item_index < inv.items.len() {
            let item = inv.items.remove(item_index);
            inv.current_weight_kg -= crate::systems::item_weight(item);
            if let Ok(mut health) = world.get::<&mut Health>(player_entity) {
                health.current =
                    (health.current + crate::systems::item_heal_amount(item)).min(health.max);
            }
        }
    }
}

/// Handle interactions with open chests/containers
pub fn handle_chest_interaction(
    world: &mut World,
    player_entity: Entity,
    chest_id: Entity,
    take_all: bool,
    take_gold: bool,
    item_to_take: Option<usize>,
) -> bool {
    // Returns true if chest should be closed
    if take_all {
        crate::systems::take_all_from_container(world, player_entity, chest_id);
        true
    } else if take_gold {
        crate::systems::take_gold_from_container(world, player_entity, chest_id);
        false
    } else if let Some(item_index) = item_to_take {
        crate::systems::take_item_from_container(world, player_entity, chest_id, item_index);
        false
    } else {
        false
    }
}
