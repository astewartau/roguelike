//! World initialization - creates the game world and spawns initial entities.

use crate::components::{
    Actor, AnimatedSprite, Attackable, BlocksMovement, BlocksVision, ChaseAI, ClassAbility,
    Container, Door, Equipment, Experience, Health, Inventory, ItemType, Player, PlayerClass,
    Position, Sprite, Stats, StatusEffects, VisualPosition,
};
use crate::constants::*;
use crate::dungeon_gen::RoomTheme;
use crate::events::EventQueue;
use crate::grid::Grid;
use crate::spawning;
use crate::systems::items::item_weight;
use crate::tile::tile_ids;
use crate::time_system::{ActionScheduler, GameClock};

use hecs::{Entity, World};
use rand::seq::SliceRandom;
use rand::Rng;

/// Spawn all chests from grid positions with randomized contents.
fn spawn_chests(world: &mut World, grid: &Grid, rng: &mut impl Rng) {
    for (x, y) in &grid.chest_positions {
        let pos = Position::new(*x, *y);
        let container = generate_chest_contents(rng);
        world.spawn((
            pos,
            VisualPosition::from_position(&pos),
            Sprite::from_ref(tile_ids::CHEST_CLOSED),
            container,
            BlocksMovement,
        ));
    }
}

/// Spawn all doors from grid positions with theme-appropriate sprites.
fn spawn_doors(world: &mut World, grid: &Grid) {
    for ((x, y), theme) in &grid.door_positions {
        let pos = Position::new(*x, *y);
        let (sprite, door) = match theme {
            RoomTheme::Overgrown => (tile_ids::DOOR_GREEN, Door::green()),
            RoomTheme::Crypt => (tile_ids::DOOR_GRATED, Door::grated()),
            _ => (tile_ids::DOOR, Door::new()),
        };
        world.spawn((
            pos,
            VisualPosition::from_position(&pos),
            Sprite::from_ref(sprite),
            door,
            BlocksVision,
            BlocksMovement,
        ));
    }
}

/// Spawn all braziers from grid positions.
fn spawn_braziers(world: &mut World, grid: &Grid) {
    for (x, y) in &grid.brazier_positions {
        spawning::spawn_brazier(world, *x, *y);
    }
}

/// Spawn all coffins from grid positions with randomized contents.
fn spawn_coffins(world: &mut World, grid: &Grid, rng: &mut impl Rng) {
    for (x, y) in &grid.coffin_positions {
        let pos = Position::new(*x, *y);

        // Generate coffin contents - gold and possibly a scroll/potion
        let gold = rng.gen_range(15..30);
        let items: Vec<ItemType> = if rng.gen_bool(0.4) {
            // 40% chance for an item
            let rare_items = [
                ItemType::ScrollOfBlink,
                ItemType::ScrollOfFear,
                ItemType::ScrollOfFireball,
                ItemType::StrengthPotion,
                ItemType::ScrollOfProtection,
            ];
            vec![*rare_items.choose(rng).unwrap()]
        } else {
            vec![]
        };

        // 40% chance to spawn a skeleton when opened
        let spawn_chance = 0.4;

        world.spawn((
            pos,
            VisualPosition::from_position(&pos),
            Sprite::from_ref(tile_ids::COFFIN_CLOSED),
            Container::coffin(items, gold, spawn_chance),
            BlocksMovement,
        ));
    }
}

/// Spawn all barrels from grid positions with food items.
fn spawn_barrels(world: &mut World, grid: &Grid, rng: &mut impl Rng) {
    for (x, y) in &grid.barrel_positions {
        let pos = Position::new(*x, *y);

        // Barrels contain food items
        let food_items = [ItemType::Cheese, ItemType::Bread, ItemType::Apple];
        let items: Vec<ItemType> = if rng.gen_bool(0.7) {
            // 70% chance for food
            vec![*food_items.choose(rng).unwrap()]
        } else {
            vec![]
        };

        world.spawn((
            pos,
            VisualPosition::from_position(&pos),
            Sprite::from_ref(tile_ids::BARREL),
            Container::barrel(items),
            BlocksMovement,
        ));
    }
}

/// Spawn animated water entities at water positions.
fn spawn_water_entities(world: &mut World, grid: &Grid) {
    for (x, y) in &grid.water_positions {
        let pos = Position::new(*x, *y);
        world.spawn((
            pos,
            VisualPosition::from_position(&pos),
            AnimatedSprite::water(),
        ));
    }
}

/// Generate randomized chest contents.
fn generate_chest_contents(rng: &mut impl Rng) -> Container {
    // Common items (higher weight)
    let common_items = [
        ItemType::HealthPotion,
        ItemType::RegenerationPotion,
        ItemType::ScrollOfSpeed,
        ItemType::ScrollOfProtection,
    ];

    // Uncommon items
    let uncommon_items = [
        ItemType::StrengthPotion,
        ItemType::ScrollOfInvisibility,
        ItemType::ScrollOfSlow,
        ItemType::ScrollOfMapping,
    ];

    // Rare items
    let rare_items = [
        ItemType::ConfusionPotion,
        ItemType::ScrollOfBlink,
        ItemType::ScrollOfFear,
        ItemType::ScrollOfReveal,
        ItemType::ScrollOfFireball,
    ];

    let roll: f32 = rng.gen();

    if roll < 0.35 {
        let item = *common_items.choose(rng).unwrap();
        Container::with_gold(vec![item], rng.gen_range(5..15))
    } else if roll < 0.55 {
        let item = *uncommon_items.choose(rng).unwrap();
        Container::with_gold(vec![item], rng.gen_range(8..20))
    } else if roll < 0.70 {
        let item = *rare_items.choose(rng).unwrap();
        Container::with_gold(vec![item], rng.gen_range(10..25))
    } else if roll < 0.85 {
        let all_items = [
            ItemType::HealthPotion,
            ItemType::RegenerationPotion,
            ItemType::StrengthPotion,
            ItemType::ConfusionPotion,
            ItemType::ScrollOfInvisibility,
            ItemType::ScrollOfSpeed,
            ItemType::ScrollOfProtection,
            ItemType::ScrollOfBlink,
            ItemType::ScrollOfFear,
            ItemType::ScrollOfFireball,
            ItemType::ScrollOfReveal,
            ItemType::ScrollOfMapping,
            ItemType::ScrollOfSlow,
        ];
        let items = vec![
            *common_items.choose(rng).unwrap(),
            *all_items.choose(rng).unwrap(),
        ];
        Container::with_gold(items, rng.gen_range(10..25))
    } else {
        Container::with_gold(vec![], rng.gen_range(20..50))
    }
}

/// Initialize the game world with player, enemies, and objects.
/// Returns (world, player_entity, player_start_position).
pub fn init_world(grid: &Grid, player_class: PlayerClass) -> (World, Entity, Position) {
    let mut world = World::new();

    // Find player spawn position
    let mut player_start = Position::new(50, 50);
    if let Some(starting_room) = &grid.starting_room {
        let (cx, cy) = starting_room.center();
        if grid.is_walkable(cx, cy) {
            player_start = Position::new(cx, cy);
        } else {
            'find_in_room: for dy in 1..starting_room.height - 1 {
                for dx in 1..starting_room.width - 1 {
                    let x = starting_room.x + dx;
                    let y = starting_room.y + dy;
                    if grid.is_walkable(x, y) {
                        player_start = Position::new(x, y);
                        break 'find_in_room;
                    }
                }
            }
        }
    } else {
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

    // Build starting inventory from class definition
    let mut starting_inventory = Inventory::new();
    for item in player_class.starting_inventory() {
        starting_inventory.current_weight_kg += item_weight(item);
        starting_inventory.items.push(item);
    }

    // Get class stats
    let (str, int, agi) = player_class.stats();

    // Spawn player with class-specific attributes
    let player_entity = world.spawn((
        player_start,
        VisualPosition::from_position(&player_start),
        Sprite::from_ref(player_class.sprite()),
        Player,
        Actor::new(PLAYER_MAX_ENERGY, PLAYER_SPEED),
        Health::with_regen(
            PLAYER_STARTING_HEALTH,
            PLAYER_HP_REGEN_AMOUNT,
            PLAYER_HP_REGEN_INTERVAL,
        ),
        Stats::new(str, int, agi),
        starting_inventory,
        Equipment::with_equipped(player_class.starting_weapon()),
        BlocksMovement,
        Experience::new(),
        Attackable,
        StatusEffects::new(),
        ClassAbility::new(player_class.ability(), player_class.ability_cooldown()),
    ));

    // Spawn chests, doors, braziers, coffins, barrels, and water
    let mut rng = rand::thread_rng();
    spawn_chests(&mut world, grid, &mut rng);
    spawn_doors(&mut world, grid);
    spawn_braziers(&mut world, grid);
    spawn_coffins(&mut world, grid, &mut rng);
    spawn_barrels(&mut world, grid, &mut rng);
    spawn_water_entities(&mut world, grid);

    // Spawn wizard NPC
    if let Some(starting_room) = &grid.starting_room {
        let mut npc_spawned = false;
        'find_npc_pos: for dy in 1..starting_room.height - 1 {
            for dx in 1..starting_room.width - 1 {
                let x = starting_room.x + dx;
                let y = starting_room.y + dy;
                if x == player_start.x && y == player_start.y {
                    continue;
                }
                if grid.is_walkable(x, y) {
                    spawning::npcs::WIZARD.spawn(&mut world, x, y);
                    npc_spawned = true;
                    break 'find_npc_pos;
                }
            }
        }
        if !npc_spawned {
            let (cx, cy) = starting_room.center();
            if (cx != player_start.x || cy != player_start.y)
                && grid.is_walkable(cx, cy)
            {
                spawning::npcs::WIZARD.spawn(&mut world, cx, cy);
            }
        }
    }

    // Spawn enemies
    let walkable_tiles: Vec<(i32, i32)> = (0..grid.height as i32)
        .flat_map(|y| (0..grid.width as i32).map(move |x| (x, y)))
        .filter(|&(x, y)| grid.is_walkable(x, y))
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

/// Initialize all AI actors with their first action in the time system.
pub fn initialize_ai_actors(
    world: &mut World,
    grid: &Grid,
    player_entity: Entity,
    clock: &GameClock,
    scheduler: &mut ActionScheduler,
    events: &mut EventQueue,
    rng: &mut impl Rng,
) {
    let ai_entities: Vec<Entity> = world
        .query::<(&Actor, &ChaseAI)>()
        .iter()
        .map(|(id, _)| id)
        .collect();

    for entity in ai_entities {
        crate::systems::ai::decide_action(world, grid, entity, player_entity, clock, scheduler, events, rng);
    }
}

/// Initialize a single AI actor (used when spawning new enemies mid-game).
pub fn initialize_single_ai_actor(
    world: &mut World,
    grid: &Grid,
    entity: Entity,
    player_entity: Entity,
    clock: &GameClock,
    scheduler: &mut ActionScheduler,
    events: &mut EventQueue,
    rng: &mut impl Rng,
) {
    crate::systems::ai::decide_action(world, grid, entity, player_entity, clock, scheduler, events, rng);
}

/// Spawn floor entities for a new (unsaved) floor.
pub fn spawn_floor_entities(
    world: &mut World,
    grid: &Grid,
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

    // Spawn chests, doors, and braziers
    let mut rng = rand::thread_rng();
    spawn_chests(world, grid, &mut rng);
    spawn_doors(world, grid);
    spawn_braziers(world, grid);

    // Spawn enemies
    let walkable_tiles: Vec<(i32, i32)> = (0..grid.height as i32)
        .flat_map(|y| (0..grid.width as i32).map(move |x| (x, y)))
        .filter(|&(x, y)| grid.is_walkable(x, y))
        .collect();

    let spawn_config = spawning::SpawnConfig::level_1();
    spawn_config.spawn_all(
        world,
        &walkable_tiles,
        &[player_spawn_pos],
        grid.starting_room.as_ref(),
        &mut rng,
    );

    // Initialize AI
    initialize_ai_actors(world, grid, player_entity, clock, scheduler, events, &mut rng);
}
