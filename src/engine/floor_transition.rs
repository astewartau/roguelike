//! Floor transition and save/load logic for multi-floor dungeons.

use crate::components::{
    BlocksMovement, BlocksVision, ChaseAI, Container, Door, Health, ItemType,
    Position, Sprite, VisualPosition,
};
use crate::constants::*;
use crate::events::EventQueue;
use crate::grid::Grid;
use crate::spawning;
use crate::tile::tile_ids;
use crate::time_system::{ActionScheduler, GameClock};

use hecs::{Entity, World};
use std::collections::HashMap;

use super::initialization::spawn_floor_entities;

/// Saved state of a floor for when the player leaves and returns.
pub struct SavedFloor {
    pub grid: Grid,
    pub entities: Vec<SavedEntity>,
}

/// Saved entity data (non-player entities like enemies, chests, doors).
pub struct SavedEntity {
    pub pos: (i32, i32),
    pub entity_type: SavedEntityType,
}

/// Types of entities that can be saved.
pub enum SavedEntityType {
    Enemy {
        health_current: i32,
        health_max: i32,
    },
    Chest {
        is_open: bool,
        gold: u32,
        items: Vec<ItemType>,
    },
    Door {
        is_open: bool,
    },
    Bones {
        gold: u32,
        items: Vec<ItemType>,
    },
}

/// Result of a floor transition.
pub struct FloorTransitionResult {
    pub new_floor: u32,
    pub new_grid: Grid,
    pub player_visual_pos: (f32, f32),
}

/// Check if a floor transition is valid.
pub fn can_transition_floor(current_floor: u32, direction: crate::events::StairDirection) -> bool {
    use crate::events::StairDirection;
    match direction {
        StairDirection::Down => true,
        StairDirection::Up => current_floor > 0,
    }
}

/// Save the current floor state (non-player entities).
pub fn save_floor(world: &World, grid: Grid, player_entity: Entity) -> SavedFloor {
    let mut entities = Vec::new();

    // Save enemies
    for (id, (pos, health, _)) in world.query::<(&Position, &Health, &ChaseAI)>().iter() {
        if id == player_entity {
            continue;
        }
        entities.push(SavedEntity {
            pos: (pos.x, pos.y),
            entity_type: SavedEntityType::Enemy {
                health_current: health.current,
                health_max: health.max,
            },
        });
    }

    // Save containers
    for (id, (pos, container)) in world.query::<(&Position, &Container)>().iter() {
        if id == player_entity {
            continue;
        }
        let sprite = world.get::<&Sprite>(id).ok();
        let is_bones = sprite.map(|s| (s.sheet, s.tile_id) == tile_ids::BONES_4).unwrap_or(false);

        if is_bones {
            entities.push(SavedEntity {
                pos: (pos.x, pos.y),
                entity_type: SavedEntityType::Bones {
                    gold: container.gold,
                    items: container.items.clone(),
                },
            });
        } else {
            entities.push(SavedEntity {
                pos: (pos.x, pos.y),
                entity_type: SavedEntityType::Chest {
                    is_open: container.is_open,
                    gold: container.gold,
                    items: container.items.clone(),
                },
            });
        }
    }

    // Save doors
    for (id, (pos, door)) in world.query::<(&Position, &Door)>().iter() {
        if id == player_entity {
            continue;
        }
        entities.push(SavedEntity {
            pos: (pos.x, pos.y),
            entity_type: SavedEntityType::Door {
                is_open: door.is_open,
            },
        });
    }

    SavedFloor { grid, entities }
}

/// Clear all non-player entities from the world.
pub fn clear_floor_entities(world: &mut World, player_entity: Entity, scheduler: &mut ActionScheduler) {
    let to_remove: Vec<Entity> = world
        .iter()
        .map(|e| e.entity())
        .filter(|&id| id != player_entity)
        .collect();

    for entity in &to_remove {
        scheduler.cancel_for_entity(*entity);
    }

    for entity in to_remove {
        let _ = world.despawn(entity);
    }
}

/// Load a saved floor, spawning entities.
pub fn load_floor(
    world: &mut World,
    grid: &Grid,
    saved_entities: &[SavedEntity],
    player_entity: Entity,
    player_spawn_pos: (i32, i32),
    clock: &GameClock,
    scheduler: &mut ActionScheduler,
    events: &mut EventQueue,
) {
    // Update player position
    if let Ok(mut pos) = world.get::<&mut Position>(player_entity) {
        pos.x = player_spawn_pos.0;
        pos.y = player_spawn_pos.1;
    }
    if let Ok(mut vis_pos) = world.get::<&mut VisualPosition>(player_entity) {
        vis_pos.x = player_spawn_pos.0 as f32;
        vis_pos.y = player_spawn_pos.1 as f32;
    }

    let mut rng = rand::thread_rng();

    for saved_entity in saved_entities {
        let pos = Position::new(saved_entity.pos.0, saved_entity.pos.1);
        match &saved_entity.entity_type {
            SavedEntityType::Enemy { health_current, health_max } => {
                let enemy = spawning::enemies::SKELETON.spawn(world, pos.x, pos.y);
                if let Ok(mut health) = world.get::<&mut Health>(enemy) {
                    health.current = *health_current;
                    health.max = *health_max;
                }
                crate::systems::ai::decide_action(
                    world, grid, enemy, player_entity, clock, scheduler, events, &mut rng,
                );
            }
            SavedEntityType::Chest { is_open, gold, items } => {
                let sprite_ref = if *is_open { tile_ids::CHEST_OPEN } else { tile_ids::CHEST_CLOSED };
                let mut container = Container::new(items.clone());
                container.is_open = *is_open;
                container.gold = *gold;

                if *is_open && container.is_empty() {
                    world.spawn((
                        pos,
                        VisualPosition::from_position(&pos),
                        Sprite::from_ref(sprite_ref),
                        container,
                    ));
                } else {
                    world.spawn((
                        pos,
                        VisualPosition::from_position(&pos),
                        Sprite::from_ref(sprite_ref),
                        container,
                        BlocksMovement,
                    ));
                }
            }
            SavedEntityType::Door { is_open } => {
                if *is_open {
                    let mut door = Door::new();
                    door.is_open = true;
                    world.spawn((
                        pos,
                        VisualPosition::from_position(&pos),
                        Sprite::from_ref(tile_ids::DOOR),
                        door,
                    ));
                } else {
                    world.spawn((
                        pos,
                        VisualPosition::from_position(&pos),
                        Sprite::from_ref(tile_ids::DOOR),
                        Door::new(),
                        BlocksVision,
                        BlocksMovement,
                    ));
                }
            }
            SavedEntityType::Bones { gold, items } => {
                let mut container = Container::new(items.clone());
                container.is_open = true;
                container.gold = *gold;
                world.spawn((
                    pos,
                    VisualPosition::from_position(&pos),
                    Sprite::from_ref(tile_ids::BONES_4),
                    container,
                ));
            }
        }
    }
}

/// Handle a floor transition (going up or down stairs).
pub fn handle_floor_transition(
    world: &mut World,
    current_grid: Grid,
    floors: &mut HashMap<u32, SavedFloor>,
    current_floor: u32,
    direction: crate::events::StairDirection,
    player_entity: Entity,
    clock: &GameClock,
    scheduler: &mut ActionScheduler,
    events: &mut EventQueue,
) -> FloorTransitionResult {
    use crate::events::StairDirection;

    let target_floor = match direction {
        StairDirection::Down => current_floor + 1,
        StairDirection::Up => {
            assert!(current_floor > 0, "Cannot go up from floor 0");
            current_floor - 1
        }
    };

    // Save current floor
    let saved_floor = save_floor(world, current_grid, player_entity);
    floors.insert(current_floor, saved_floor);

    // Clear current floor entities
    clear_floor_entities(world, player_entity, scheduler);

    // Load or generate target floor
    let new_grid = if let Some(saved) = floors.remove(&target_floor) {
        let spawn_pos = match direction {
            StairDirection::Down => saved.grid.stairs_up_pos.unwrap_or((1, 1)),
            StairDirection::Up => saved.grid.stairs_down_pos.unwrap_or((1, 1)),
        };

        let grid = saved.grid;
        load_floor(
            world,
            &grid,
            &saved.entities,
            player_entity,
            spawn_pos,
            clock,
            scheduler,
            events,
        );
        grid
    } else {
        let grid = Grid::new_floor(DUNGEON_DEFAULT_WIDTH, DUNGEON_DEFAULT_HEIGHT, target_floor);

        let spawn_pos = match direction {
            StairDirection::Down => grid.stairs_up_pos.unwrap_or((1, 1)),
            StairDirection::Up => grid.stairs_down_pos.unwrap_or((1, 1)),
        };

        spawn_floor_entities(
            world,
            &grid,
            player_entity,
            spawn_pos,
            target_floor,
            clock,
            scheduler,
            events,
        );
        grid
    };

    let player_visual_pos = world
        .get::<&VisualPosition>(player_entity)
        .map(|vp| (vp.x, vp.y))
        .unwrap_or((1.0, 1.0));

    FloorTransitionResult {
        new_floor: target_floor,
        new_grid,
        player_visual_pos,
    }
}
