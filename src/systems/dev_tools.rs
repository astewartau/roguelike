//! Developer tool systems for spawning entities.
//!
//! This module handles dev menu spawn operations, keeping game logic
//! out of main.rs. Non-ECS operations (VFX, grid tile changes) remain
//! thin wrappers in main.rs.

use hecs::{Entity, World};

use crate::components::{BlocksMovement, Container, ItemType, Position, Sprite, VisualPosition};
use crate::events::EventQueue;
use crate::engine;
use crate::grid::Grid;
use crate::queries;
use crate::spawning;
use crate::tile;
use crate::time_system::{ActionScheduler, GameClock};
use crate::ui::DevTool;

/// Result of a dev spawn attempt
#[derive(Debug)]
pub enum DevSpawnResult {
    /// Entity was spawned successfully
    Spawned(Entity),
    /// Tile was modified (stairs)
    TileModified,
    /// VFX was requested (caller should spawn it)
    VfxRequested,
    /// Spawn failed - position not walkable
    NotWalkable,
    /// Spawn failed - position is blocked
    Blocked,
}

/// Execute a dev spawn at the given position.
///
/// Returns `DevSpawnResult` indicating what happened. For VFX requests,
/// the caller is responsible for spawning the effect.
pub fn execute_dev_spawn(
    world: &mut World,
    grid: &mut Grid,
    tool: DevTool,
    tile_x: i32,
    tile_y: i32,
    player_entity: Entity,
    clock: &GameClock,
    scheduler: &mut ActionScheduler,
    events: &mut EventQueue,
) -> DevSpawnResult {
    // Check if the tile is walkable
    let Some(tile) = grid.get(tile_x, tile_y) else {
        return DevSpawnResult::NotWalkable;
    };
    if !tile.tile_type.is_walkable() {
        return DevSpawnResult::NotWalkable;
    }

    // Check if something is already blocking this tile (except for stairs/fire)
    let needs_clear_tile = matches!(tool, DevTool::SpawnChest | DevTool::SpawnEnemy);
    if needs_clear_tile && queries::is_position_blocked(world, tile_x, tile_y, None) {
        return DevSpawnResult::Blocked;
    }

    match tool {
        DevTool::SpawnChest => {
            let pos = Position::new(tile_x, tile_y);
            let entity = world.spawn((
                pos,
                VisualPosition::from_position(&pos),
                Sprite::new(tile::tile_ids::CHEST_CLOSED),
                Container::new(vec![ItemType::HealthPotion]),
                BlocksMovement,
            ));
            DevSpawnResult::Spawned(entity)
        }
        DevTool::SpawnEnemy => {
            let enemy = spawning::enemies::SKELETON.spawn(world, tile_x, tile_y);
            // Initialize the AI actor's first action
            let mut rng = rand::thread_rng();
            engine::initialize_single_ai_actor(
                world,
                grid,
                enemy,
                player_entity,
                clock,
                scheduler,
                events,
                &mut rng,
            );
            DevSpawnResult::Spawned(enemy)
        }
        DevTool::SpawnFire => {
            // Fire is a VFX effect, not an ECS entity
            DevSpawnResult::VfxRequested
        }
        DevTool::SpawnStairsDown => {
            if let Some(tile) = grid.get_mut(tile_x, tile_y) {
                tile.tile_type = tile::TileType::StairsDown;
            }
            grid.stairs_down_pos = Some((tile_x, tile_y));
            DevSpawnResult::TileModified
        }
        DevTool::SpawnStairsUp => {
            if let Some(tile) = grid.get_mut(tile_x, tile_y) {
                tile.tile_type = tile::TileType::StairsUp;
            }
            grid.stairs_up_pos = Some((tile_x, tile_y));
            DevSpawnResult::TileModified
        }
    }
}

/// Give an item directly to the player's inventory (dev tool - no weight limit check).
pub fn give_item_to_player(world: &mut World, player_entity: Entity, item: ItemType) {
    use crate::components::Inventory;
    use crate::systems::items::item_weight;

    if let Ok(mut inv) = world.get::<&mut Inventory>(player_entity) {
        let weight = item_weight(item);
        inv.items.push(item);
        inv.current_weight_kg += weight;
    }
}
