//! Core game initialization and state management.
//!
//! Handles world creation, entity spawning, and game state.

use crate::camera::Camera;
use crate::components::{
    Actor, Attackable, BlocksMovement, BlocksVision, ChaseAI, Container, Door, Equipment,
    Experience, Health, Inventory, ItemType, Player, Position, RangedWeapon, Sprite, Stats,
    StatusEffects, VisualPosition, Weapon,
};
use crate::constants::*;
use crate::grid::Grid;
use crate::spawning;
use crate::tile::tile_ids;
use crate::time_system::{ActionScheduler, GameClock};
use glam::Vec2;
use hecs::{Entity, World};
use rand::seq::SliceRandom;
use rand::Rng;

/// Saved state of a floor for when the player leaves and returns
pub struct SavedFloor {
    pub grid: Grid,
    /// Saved entity data for restoration
    pub entities: Vec<SavedEntity>,
}

/// Saved entity data (non-player entities like enemies, chests, doors)
pub struct SavedEntity {
    pub pos: (i32, i32),
    pub entity_type: SavedEntityType,
}

/// Types of entities that can be saved
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

/// Generate randomized chest contents
/// Chests can contain: potions, scrolls, or just gold (more than bones drop)
fn generate_chest_contents(rng: &mut impl Rng) -> Container {
    let roll: f32 = rng.gen();

    if roll < 0.3 {
        // 30% chance: Health potion
        Container::with_gold(vec![ItemType::HealthPotion], rng.gen_range(5..15))
    } else if roll < 0.5 {
        // 20% chance: Scroll of Invisibility
        Container::with_gold(vec![ItemType::ScrollOfInvisibility], rng.gen_range(5..15))
    } else if roll < 0.7 {
        // 20% chance: Scroll of Speed
        Container::with_gold(vec![ItemType::ScrollOfSpeed], rng.gen_range(5..15))
    } else if roll < 0.85 {
        // 15% chance: Two random items
        let items = vec![
            *[ItemType::HealthPotion, ItemType::ScrollOfInvisibility, ItemType::ScrollOfSpeed]
                .choose(rng).unwrap(),
            *[ItemType::HealthPotion, ItemType::ScrollOfInvisibility, ItemType::ScrollOfSpeed]
                .choose(rng).unwrap(),
        ];
        Container::with_gold(items, rng.gen_range(10..25))
    } else {
        // 15% chance: Just gold (more than bones)
        Container::with_gold(vec![], rng.gen_range(20..50))
    }
}

/// Initialize the game world with player, enemies, and objects
/// Returns (world, player_entity, player_start_position)
pub fn init_world(grid: &Grid) -> (World, Entity, Position) {
    let mut world = World::new();

    // Find a walkable tile to spawn the player - prefer center of starting room
    let mut player_start = Position::new(50, 50);
    if let Some(starting_room) = &grid.starting_room {
        // Spawn player at center of starting room
        let (cx, cy) = starting_room.center();
        if grid.get(cx, cy).map(|t| t.tile_type.is_walkable()).unwrap_or(false) {
            player_start = Position::new(cx, cy);
        } else {
            // Fallback: find any walkable tile in the starting room
            'find_in_room: for dy in 1..starting_room.height - 1 {
                for dx in 1..starting_room.width - 1 {
                    let x = starting_room.x + dx;
                    let y = starting_room.y + dy;
                    if grid.get(x, y).map(|t| t.tile_type.is_walkable()).unwrap_or(false) {
                        player_start = Position::new(x, y);
                        break 'find_in_room;
                    }
                }
            }
        }
    } else {
        // No starting room - fall back to first walkable tile
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
        StatusEffects::new(),
    ));

    // Spawn chests with randomized contents
    let mut rng = rand::thread_rng();
    for (x, y) in &grid.chest_positions {
        let pos = Position::new(*x, *y);
        let container = generate_chest_contents(&mut rng);
        world.spawn((
            pos,
            VisualPosition::from_position(&pos),
            Sprite::new(tile_ids::CHEST_CLOSED),
            container,
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

    // Spawn the wizard NPC in the starting room
    if let Some(starting_room) = &grid.starting_room {
        // Find a walkable tile in the starting room that isn't the player position
        // Scan the room interior (excluding walls) for a valid position
        let mut npc_spawned = false;
        'find_npc_pos: for dy in 1..starting_room.height - 1 {
            for dx in 1..starting_room.width - 1 {
                let x = starting_room.x + dx;
                let y = starting_room.y + dy;
                // Skip the player's position
                if x == player_start.x && y == player_start.y {
                    continue;
                }
                if grid.get(x, y).map(|t| t.tile_type.is_walkable()).unwrap_or(false) {
                    spawning::npcs::WIZARD.spawn(&mut world, x, y);
                    npc_spawned = true;
                    break 'find_npc_pos;
                }
            }
        }
        // Fallback: try center of room if interior scan failed
        if !npc_spawned {
            let (cx, cy) = starting_room.center();
            if (cx != player_start.x || cy != player_start.y)
                && grid.get(cx, cy).map(|t| t.tile_type.is_walkable()).unwrap_or(false)
            {
                spawning::npcs::WIZARD.spawn(&mut world, cx, cy);
            }
        }
    }

    // Spawn enemies using data-driven spawning system (excluding starting room)
    let walkable_tiles: Vec<(i32, i32)> = (0..grid.height as i32)
        .flat_map(|y| (0..grid.width as i32).map(move |x| (x, y)))
        .filter(|&(x, y)| grid.get(x, y).map(|t| t.tile_type.is_walkable()).unwrap_or(false))
        .collect();

    let spawn_config = spawning::SpawnConfig::level_1();
    spawn_config.spawn_all(
        &mut world,
        &walkable_tiles,
        &[(player_start.x, player_start.y)],
        grid.starting_room.as_ref(),
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

/// Save the current floor state (non-player entities)
pub fn save_floor(world: &World, grid: Grid, player_entity: Entity) -> SavedFloor {
    let mut entities = Vec::new();

    // Save enemies (entities with ChaseAI and Health)
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

    // Save chests/containers
    for (id, (pos, container)) in world.query::<(&Position, &Container)>().iter() {
        if id == player_entity {
            continue;
        }
        // Check if it's bones (has no BlocksMovement when open and empty, but we check the name via sprite)
        let sprite = world.get::<&Sprite>(id).ok();
        let is_bones = sprite.map(|s| s.tile_id == tile_ids::BONES).unwrap_or(false);

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

/// Clear all non-player entities from the world
pub fn clear_floor_entities(world: &mut World, player_entity: Entity, scheduler: &mut ActionScheduler) {
    // Collect all entities except player
    let to_remove: Vec<Entity> = world
        .iter()
        .map(|e| e.entity())
        .filter(|&id| id != player_entity)
        .collect();

    // Cancel scheduled actions for all removed entities
    for entity in &to_remove {
        scheduler.cancel_for_entity(*entity);
    }

    // Despawn all non-player entities
    for entity in to_remove {
        let _ = world.despawn(entity);
    }
}

/// Load a saved floor, spawning entities
pub fn load_floor(
    world: &mut World,
    grid: &Grid,
    saved_entities: &[SavedEntity],
    player_entity: Entity,
    player_spawn_pos: (i32, i32),
    clock: &GameClock,
    scheduler: &mut ActionScheduler,
    events: &mut crate::events::EventQueue,
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

    // Spawn saved entities
    for saved_entity in saved_entities {
        let pos = Position::new(saved_entity.pos.0, saved_entity.pos.1);
        match &saved_entity.entity_type {
            SavedEntityType::Enemy { health_current, health_max } => {
                // Spawn enemy with saved health
                let enemy = spawning::enemies::SKELETON.spawn(world, pos.x, pos.y);
                if let Ok(mut health) = world.get::<&mut Health>(enemy) {
                    health.current = *health_current;
                    health.max = *health_max;
                }
                // Initialize AI
                crate::systems::ai::decide_action(
                    world, grid, enemy, player_entity, clock, scheduler, events, &mut rng,
                );
            }
            SavedEntityType::Chest { is_open, gold, items } => {
                let sprite_id = if *is_open { tile_ids::CHEST_OPEN } else { tile_ids::CHEST_CLOSED };
                let mut container = Container::new(items.clone());
                container.is_open = *is_open;
                container.gold = *gold;

                if *is_open && container.is_empty() {
                    // Open empty chest - no BlocksMovement
                    world.spawn((
                        pos,
                        VisualPosition::from_position(&pos),
                        Sprite::new(sprite_id),
                        container,
                    ));
                } else {
                    world.spawn((
                        pos,
                        VisualPosition::from_position(&pos),
                        Sprite::new(sprite_id),
                        container,
                        BlocksMovement,
                    ));
                }
            }
            SavedEntityType::Door { is_open } => {
                if *is_open {
                    // Open door - no BlocksMovement or BlocksVision
                    let mut door = Door::new();
                    door.is_open = true;
                    world.spawn((
                        pos,
                        VisualPosition::from_position(&pos),
                        Sprite::new(tile_ids::DOOR),
                        door,
                    ));
                } else {
                    world.spawn((
                        pos,
                        VisualPosition::from_position(&pos),
                        Sprite::new(tile_ids::DOOR),
                        Door::new(),
                        BlocksVision,
                        BlocksMovement,
                    ));
                }
            }
            SavedEntityType::Bones { gold, items } => {
                let mut container = Container::new(items.clone());
                container.is_open = true; // Bones are always "open"
                container.gold = *gold;
                world.spawn((
                    pos,
                    VisualPosition::from_position(&pos),
                    Sprite::new(tile_ids::BONES),
                    container,
                ));
            }
        }
    }
}

/// Spawn floor entities for a new (unsaved) floor
pub fn spawn_floor_entities(
    world: &mut World,
    grid: &Grid,
    player_entity: Entity,
    player_spawn_pos: (i32, i32),
    clock: &GameClock,
    scheduler: &mut ActionScheduler,
    events: &mut crate::events::EventQueue,
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

    // Spawn chests with randomized contents
    let mut rng = rand::thread_rng();
    for (x, y) in &grid.chest_positions {
        let pos = Position::new(*x, *y);
        let container = generate_chest_contents(&mut rng);
        world.spawn((
            pos,
            VisualPosition::from_position(&pos),
            Sprite::new(tile_ids::CHEST_CLOSED),
            container,
            BlocksMovement,
        ));
    }

    // Spawn doors
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

    // Spawn enemies (excluding starting room for safety)
    let mut rng = rand::thread_rng();
    let walkable_tiles: Vec<(i32, i32)> = (0..grid.height as i32)
        .flat_map(|y| (0..grid.width as i32).map(move |x| (x, y)))
        .filter(|&(x, y)| grid.get(x, y).map(|t| t.tile_type.is_walkable()).unwrap_or(false))
        .collect();

    let spawn_config = spawning::SpawnConfig::level_1();
    spawn_config.spawn_all(
        world,
        &walkable_tiles,
        &[player_spawn_pos],
        grid.starting_room.as_ref(),
        &mut rng,
    );

    // Initialize AI for all enemies
    initialize_ai_actors(world, grid, player_entity, clock, scheduler, events, &mut rng);
}
