//! World initialization - creates the game world and spawns initial entities.

use crate::components::{
    AbilityType, Actor, AnimatedSprite, Attackable, BlocksMovement, BlocksVision, ClassAbility,
    Container, Door, Equipment, Experience, Health, Inventory, ItemInstance, ItemType, Name, Player,
    PlayerClass, Position, ClassKit, SecondaryAbility, Sprite, Stats, StatusEffects, VisualPosition,
};
use crate::constants::*;
use crate::dungeon_gen::RoomTheme;
use crate::grid::Grid;
use crate::spawning;
use crate::systems::items::item_weight;
use crate::tile_occupancy::TileOccupancy;
use crate::tile::tile_ids;

use super::ActorCtx;
use hecs::{Entity, World};
use rand::seq::SliceRandom;
use rand::Rng;

/// Spawn all chests from grid positions with randomized contents. The chest
/// inside a sealed hidden room rolls as if it were `SECRET_CHEST_FLOOR_BONUS`
/// floors deeper and always contains a rolled gear piece.
fn spawn_chests(
    world: &mut World,
    grid: &Grid,
    floor: u32,
    occupancy: &mut TileOccupancy,
    rng: &mut impl Rng,
) {
    for (x, y) in &grid.chest_positions {
        if !occupancy.claim((*x, *y)) {
            continue;
        }
        let pos = Position::new(*x, *y);
        let in_secret_room = grid
            .secret_room
            .map(|room| room.contains(*x, *y))
            .unwrap_or(false);
        let container = if in_secret_room {
            let boosted_floor = floor + SECRET_CHEST_FLOOR_BONUS;
            let mut container = generate_chest_contents(boosted_floor, rng);
            // A hidden hoard always holds at least one piece of gear.
            if let Some(&kind) = GEAR_POOL.choose(rng) {
                container.items.push(roll_gear(kind, boosted_floor, rng));
            }
            container
        } else {
            generate_chest_contents(floor, rng)
        };
        world.spawn((
            pos,
            VisualPosition::from_position(&pos),
            Sprite::from_ref(tile_ids::CHEST_CLOSED),
            container,
            BlocksMovement,
        ));
    }
}

/// Spawn hidden floor traps at the positions rolled by dungeon generation.
/// Kinds are rolled here (spike is slightly favored).
fn spawn_dungeon_traps(world: &mut World, grid: &Grid, rng: &mut impl Rng) {
    use crate::components::DungeonTrapKind;

    for (x, y) in &grid.trap_positions {
        let roll: f32 = rng.gen();
        let kind = if roll < 0.30 {
            DungeonTrapKind::Spike
        } else if roll < 0.55 {
            DungeonTrapKind::Fire
        } else if roll < 0.80 {
            DungeonTrapKind::Snare
        } else {
            DungeonTrapKind::Alarm
        };
        spawning::spawn_dungeon_trap(world, *x, *y, kind);
    }
}

/// Spawn room furniture (fountain / altar / shrine) at rolled positions.
fn spawn_furniture_pieces(
    world: &mut World,
    grid: &Grid,
    occupancy: &mut TileOccupancy,
    rng: &mut impl Rng,
) {
    use crate::components::FurnitureKind;

    let kinds = [
        FurnitureKind::Fountain,
        FurnitureKind::Altar,
        FurnitureKind::Shrine,
    ];
    for (x, y) in &grid.furniture_positions {
        if !occupancy.claim((*x, *y)) {
            continue;
        }
        let Some(&kind) = kinds.choose(rng) else {
            continue;
        };
        spawning::spawn_furniture(world, *x, *y, kind);
    }
}

/// Wall sprite for a secret door at (x, y): copies an adjacent wall tile's
/// (possibly themed/oriented) sprite so the seam is invisible.
pub(crate) fn secret_door_wall_sprite(
    grid: &Grid,
    x: i32,
    y: i32,
) -> (crate::tile::SpriteSheet, u32) {
    [(0, -1), (0, 1), (-1, 0), (1, 0)]
        .iter()
        .find_map(|(dx, dy)| {
            grid.get(x + dx, y + dy).and_then(|tile| {
                if tile.tile_type == crate::tile::TileType::Wall {
                    Some(tile.sprite())
                } else {
                    None
                }
            })
        })
        .unwrap_or(tile_ids::WALL)
}

/// Spawn the secret door sealing this floor's hidden room, if any.
fn spawn_secret_door_entity(world: &mut World, grid: &Grid, occupancy: &mut TileOccupancy) {
    let Some((x, y)) = grid.secret_door_pos else {
        return;
    };
    if !occupancy.claim((x, y)) {
        return;
    }
    let wall_sprite = secret_door_wall_sprite(grid, x, y);
    spawning::spawn_secret_door(world, x, y, wall_sprite);
}

/// Spawn all doors from grid positions with theme-appropriate sprites.
fn spawn_doors(world: &mut World, grid: &Grid, occupancy: &mut TileOccupancy) {
    for ((x, y), theme) in &grid.door_positions {
        if !occupancy.claim((*x, *y)) {
            continue;
        }
        let pos = Position::new(*x, *y);
        let (sprite, door) = match theme {
            RoomTheme::Overgrown => (tile_ids::DOOR_GREEN, Door::green()),
            RoomTheme::Crypt => (tile_ids::DOOR_GRATED, Door::grated()),
            RoomTheme::Shop => (tile_ids::DOOR_SHOP, Door::shop()),
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
fn spawn_coffins(
    world: &mut World,
    grid: &Grid,
    floor: u32,
    occupancy: &mut TileOccupancy,
    rng: &mut impl Rng,
) {
    for (x, y) in &grid.coffin_positions {
        if !occupancy.claim((*x, *y)) {
            continue;
        }
        let pos = Position::new(*x, *y);

        // Generate coffin contents - gold and possibly a scroll/potion
        let gold = rng.gen_range(15..30);
        let items: Vec<ItemInstance> = if rng.gen_bool(0.25) {
            // 25% chance for a rolled gear piece (weapon or armor)
            match GEAR_POOL.choose(rng) {
                Some(&kind) => vec![roll_gear(kind, floor, rng)],
                None => vec![],
            }
        } else if rng.gen_bool(0.4) {
            // otherwise 40% chance for a rare consumable
            let rare_items = [
                ItemType::ScrollOfBlink,
                ItemType::ScrollOfFear,
                ItemType::ScrollOfFireball,
                ItemType::StrengthPotion,
                ItemType::ScrollOfProtection,
            ];
            rare_items
                .choose(rng)
                .map(|&kind| vec![ItemInstance::plain(kind)])
                .unwrap_or_default()
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

/// Spawn the cave dressing of every Cavern room: stalagmites (blocking cover
/// you can still shoot over), glowing fungus and crystal clusters (walkable
/// light sources). All three come straight from generation, which has already
/// checked they sit on open cave floor.
fn spawn_cave_features(world: &mut World, grid: &Grid, occupancy: &mut TileOccupancy) {
    for &(x, y) in &grid.stalagmite_positions {
        if !occupancy.claim((x, y)) {
            continue;
        }
        spawning::spawn_stalagmites(world, x, y);
    }
    for &(x, y) in &grid.mushroom_positions {
        spawning::spawn_glow_mushrooms(world, x, y);
    }
    for &(x, y) in &grid.crystal_positions {
        spawning::spawn_crystal_cluster(world, x, y);
    }
}

/// Whether `(x, y)` lies inside any room. Everything walkable outside every
/// room is corridor (how `spawn_oil_barrels` picks its corridor barrels, and
/// how `spawn_oil_spills` recognises them).
fn in_any_room(grid: &Grid, x: i32, y: i32) -> bool {
    grid.themed_rooms.iter().any(|r| r.rect.contains(x, y))
}

/// Spawn explosive oil barrels: 1-2 hide among the Storage-room food barrels
/// (their positions are returned so `spawn_barrels` can skip them), and some
/// floors also get 1-2 out in the corridors.
fn spawn_oil_barrels(
    world: &mut World,
    grid: &Grid,
    occupancy: &mut TileOccupancy,
    rng: &mut impl Rng,
) {
    let mut oil_positions: Vec<(i32, i32)> = Vec::new();

    // Storage rooms: convert 1-2 of the rolled barrel spots into oil barrels.
    // Both pools are narrowed to free tiles before drawing from them, so a
    // chest already standing on a barrel spot costs a *candidate* rather than
    // a barrel; the `claim` at the bottom is what actually holds the invariant.
    let storage_pool: Vec<(i32, i32)> = grid
        .barrel_positions
        .iter()
        .copied()
        .filter(|&tile| occupancy.is_free(tile))
        .collect();
    if !storage_pool.is_empty() {
        let count = rng
            .gen_range(OIL_BARRELS_STORAGE_MIN..=OIL_BARRELS_STORAGE_MAX)
            .min(storage_pool.len());
        let mut pool: Vec<(i32, i32)> = storage_pool;
        for _ in 0..count {
            if pool.is_empty() {
                break;
            }
            let idx = rng.gen_range(0..pool.len());
            oil_positions.push(pool.swap_remove(idx));
        }
    }

    // Corridors: occasionally 1-2 barrels stand out in the open. Corridor
    // tiles are walkable floor outside every room; skip doors and stairs.
    // Blocking a chokepoint is fine — barrels are destructible (they explode).
    if rng.gen_bool(OIL_BARREL_CORRIDOR_FLOOR_CHANCE) {
        let door_tiles: Vec<(i32, i32)> =
            grid.door_positions.iter().map(|((x, y), _)| (*x, *y)).collect();
        let corridor_tiles: Vec<(i32, i32)> = (0..grid.height as i32)
            .flat_map(|y| (0..grid.width as i32).map(move |x| (x, y)))
            .filter(|&(x, y)| {
                grid.get(x, y).map(|t| t.tile_type == crate::tile::TileType::Floor).unwrap_or(false)
                    && !in_any_room(grid, x, y)
                    && !door_tiles.contains(&(x, y))
                    && Some((x, y)) != grid.stairs_up_pos
                    && Some((x, y)) != grid.stairs_down_pos
                    && occupancy.is_free((x, y))
            })
            .collect();

        if !corridor_tiles.is_empty() {
            let count = rng
                .gen_range(OIL_BARRELS_CORRIDOR_MIN..=OIL_BARRELS_CORRIDOR_MAX)
                .min(corridor_tiles.len());
            let mut pool = corridor_tiles;
            for _ in 0..count {
                if pool.is_empty() {
                    break;
                }
                let idx = rng.gen_range(0..pool.len());
                oil_positions.push(pool.swap_remove(idx));
            }
        }
    }

    for (x, y) in oil_positions {
        if occupancy.claim((x, y)) {
            spawning::spawn_oil_barrel(world, x, y);
        }
    }
}

/// Spill unlit oil on the floor: a small spill (1-3 tiles) beside some oil
/// barrels, and an occasional stray spill in a room.
///
/// Only barrels standing in a room leak. A barrel out in a corridor gets no
/// spill: the corridor is one tile wide, so a puddle beside the barrel lies
/// across the only way past it, and a fire there (or the barrel blowing) would
/// turn the passage into a trap the player cannot route around. In a room
/// there is always space to step around a spill.
///
/// Runs last in floor construction, after every blocker is placed, so that
/// `occupancy` can keep puddles out from under chests and furniture. Puddles
/// themselves do not block movement, so they never claim tiles. A puddle only
/// lands on plain floor (not grass, water, stairs or doorways), never on
/// `avoid` (the player's spawn), never in the starting room or the shop, and
/// never within one tile of a standing fire (`CausesBurning`: braziers, the
/// campfire) — oil next to a flame would be alight before the player arrived.
fn spawn_oil_spills(
    world: &mut World,
    grid: &Grid,
    occupancy: &TileOccupancy,
    avoid: &[(i32, i32)],
    rng: &mut impl Rng,
) {
    use crate::components::{CausesBurning, OilBarrel};
    use std::collections::HashSet;

    let fires: Vec<(i32, i32)> = world
        .query::<&Position>()
        .with::<&CausesBurning>()
        .iter()
        .map(|(_, p)| (p.x, p.y))
        .collect();
    let door_tiles: HashSet<(i32, i32)> =
        grid.door_positions.iter().map(|(p, _)| *p).collect();
    let shop_or_start = |x: i32, y: i32| {
        grid.starting_room.map(|r| r.contains(x, y)).unwrap_or(false)
            || grid
                .themed_rooms
                .iter()
                .any(|r| r.theme == RoomTheme::Shop && r.rect.contains(x, y))
    };
    let valid = |(x, y): (i32, i32)| {
        grid.get(x, y).map(|t| t.tile_type == crate::tile::TileType::Floor).unwrap_or(false)
            && !grid.water_positions.contains(&(x, y))
            && !door_tiles.contains(&(x, y))
            && occupancy.is_free((x, y))
            && !avoid.contains(&(x, y))
            && !shop_or_start(x, y)
            && !fires.iter().any(|&(fx, fy)| (fx - x).abs().max((fy - y).abs()) <= 1)
    };

    let mut taken: HashSet<(i32, i32)> = HashSet::new();
    // Grow a spill of up to `size` orthogonally connected tiles from `start`.
    let grow = |start: (i32, i32), size: usize, taken: &mut HashSet<(i32, i32)>, rng: &mut _| {
        let mut spill = vec![start];
        taken.insert(start);
        while spill.len() < size {
            let candidates: Vec<(i32, i32)> = spill
                .iter()
                .flat_map(|&(x, y)| [(x + 1, y), (x - 1, y), (x, y + 1), (x, y - 1)])
                .filter(|t| valid(*t) && !taken.contains(t))
                .collect();
            let Some(&next) = candidates.choose(rng) else {
                break;
            };
            taken.insert(next);
            spill.push(next);
        }
        spill
    };

    let mut puddles: Vec<(i32, i32)> = Vec::new();

    // Leaks beside oil barrels standing in rooms (not corridor barrels; see
    // above). Sorted so a seed replays identically.
    let mut barrels: Vec<(i32, i32)> = world
        .query::<&Position>()
        .with::<&OilBarrel>()
        .iter()
        .map(|(_, p)| (p.x, p.y))
        .filter(|&(x, y)| in_any_room(grid, x, y))
        .collect();
    barrels.sort_unstable();
    for (bx, by) in barrels {
        if !rng.gen_bool(OIL_SPILL_BARREL_CHANCE) {
            continue;
        }
        let beside: Vec<(i32, i32)> = (-1..=1)
            .flat_map(|dy| (-1..=1).map(move |dx| (bx + dx, by + dy)))
            .filter(|&t| t != (bx, by) && valid(t) && !taken.contains(&t))
            .collect();
        let Some(&start) = beside.choose(rng) else {
            continue;
        };
        let size = rng.gen_range(OIL_SPILL_BARREL_MIN..=OIL_SPILL_BARREL_MAX);
        puddles.extend(grow(start, size, &mut taken, rng));
    }

    // Stray spills in rooms.
    for room in &grid.themed_rooms {
        if !rng.gen_bool(OIL_SPILL_ROOM_CHANCE) {
            continue;
        }
        let r = room.rect;
        let floor: Vec<(i32, i32)> = (r.y..r.y + r.height)
            .flat_map(|y| (r.x..r.x + r.width).map(move |x| (x, y)))
            .filter(|&t| valid(t) && !taken.contains(&t))
            .collect();
        let Some(&start) = floor.choose(rng) else {
            continue;
        };
        let size = rng.gen_range(OIL_SPILL_ROOM_MIN..=OIL_SPILL_ROOM_MAX);
        puddles.extend(grow(start, size, &mut taken, rng));
    }

    for (x, y) in puddles {
        spawning::spawn_oil_puddle(world, x, y);
    }
}

/// Spawn all barrels from grid positions with food items.
///
/// The oil-barrel pass runs first and claims the spots it converted, so those
/// are skipped here by the same guard that keeps barrels off chests — this used
/// to need its own `skip_positions` slice threaded in from the caller.
fn spawn_barrels(
    world: &mut World,
    grid: &Grid,
    occupancy: &mut TileOccupancy,
    rng: &mut impl Rng,
) {
    for (x, y) in &grid.barrel_positions {
        if !occupancy.claim((*x, *y)) {
            continue;
        }
        let pos = Position::new(*x, *y);

        // Barrels contain food items
        let food_items = [ItemType::Cheese, ItemType::Bread, ItemType::Apple];
        let items: Vec<ItemInstance> = if rng.gen_bool(0.7) {
            // 70% chance for food (plain bread if the pool is somehow empty)
            let kind = food_items.choose(rng).copied().unwrap_or(ItemType::Bread);
            vec![ItemInstance::plain(kind)]
        } else {
            vec![]
        };

        world.spawn((
            pos,
            VisualPosition::from_position(&pos),
            Sprite::from_ref(tile_ids::BARREL),
            Container::barrel(items),
            BlocksMovement,
            crate::components::Pushable,
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

/// Spawn shop decorations (jars, sacks) at shop decor positions.
fn spawn_shop_decorations(
    world: &mut World,
    grid: &Grid,
    occupancy: &mut TileOccupancy,
    rng: &mut impl Rng,
) {
    let decor_sprites = [
        tile_ids::JAR_CLOSED,
        tile_ids::JAR_OPEN,
        tile_ids::BARREL,
        tile_ids::ORE_SACK,
    ];

    for (x, y) in &grid.shop_decor_positions {
        if !occupancy.claim((*x, *y)) {
            continue;
        }
        let pos = Position::new(*x, *y);
        // Skip decoration if the sprite pool is somehow empty
        let Some(sprite) = decor_sprites.choose(rng) else {
            continue;
        };
        world.spawn((
            pos,
            VisualPosition::from_position(&pos),
            Sprite::from_ref(*sprite),
            BlocksMovement,
        ));
    }
}

/// Spawn the vendor in the shop room.
fn spawn_vendor(world: &mut World, grid: &Grid, occupancy: &mut TileOccupancy, floor_num: u32) {
    if let Some((x, y)) = grid.shop_position {
        if occupancy.claim((x, y)) {
            spawning::vendors::MERCHANT.spawn(world, x, y, floor_num);
        }
    }
}

// Gear roll tables (rarity weights, affix pools, legendary names) live in
// systems/item_defs.rs.
use crate::systems::item_defs::{roll_gear, roll_gear_with_rarity, roll_rarity, GEAR_POOL};

/// Spawn the floor boss (every 3rd floor) in the largest non-start, non-shop
/// room, awake, with a guaranteed chest beside it rolled at Rare or better.
fn spawn_boss_encounter(
    world: &mut World,
    grid: &Grid,
    floor: u32,
    occupancy: &mut TileOccupancy,
    rng: &mut impl Rng,
) {
    use crate::components::Rarity;

    if !spawning::is_boss_floor(floor) {
        return;
    }

    // Largest room that is neither the starting room nor the shop.
    let starting = grid.starting_room;
    let lair = grid
        .themed_rooms
        .iter()
        .filter(|room| {
            room.theme != RoomTheme::Shop
                && starting
                    .map(|s| s.x != room.rect.x || s.y != room.rect.y)
                    .unwrap_or(true)
        })
        .max_by_key(|room| room.rect.width * room.rect.height);
    let Some(lair) = lair else {
        return;
    };
    let rect = lair.rect;

    // Free = walkable terrain with no blocking entity (enemies, chests, ...).
    // This used to be a one-off `BlocksMovement` query of its own.
    let free = |occupancy: &TileOccupancy, x: i32, y: i32| {
        grid.is_walkable(x, y) && occupancy.is_free((x, y))
    };

    // Boss stands as close to the room's center as possible.
    let (cx, cy) = rect.center();
    let mut interior: Vec<(i32, i32)> = (1..rect.height - 1)
        .flat_map(|dy| (1..rect.width - 1).map(move |dx| (rect.x + dx, rect.y + dy)))
        .collect();
    interior.sort_by_key(|&(x, y)| (x - cx).abs() + (y - cy).abs());
    let Some(&(bx, by)) = interior.iter().find(|&&(x, y)| free(occupancy, x, y)) else {
        return;
    };
    if spawning::spawn_boss(world, floor, bx, by, rng).is_none() {
        return;
    }
    occupancy.claim((bx, by));

    // Guaranteed hoard next to the boss: a chest whose gear rolls at least
    // Rare (Legendary stays possible via the normal floor-scaled roll).
    let chest_spot = [
        (0, -1), (0, 1), (-1, 0), (1, 0),
        (-1, -1), (-1, 1), (1, -1), (1, 1),
    ]
    .iter()
    .map(|(dx, dy)| (bx + dx, by + dy))
    .find(|&(x, y)| free(occupancy, x, y) && rect.contains(x, y));
    if let Some((chx, chy)) = chest_spot {
        occupancy.claim((chx, chy));
        let mut items: Vec<ItemInstance> = Vec::new();
        if let Some(&kind) = GEAR_POOL.choose(rng) {
            let rarity = match roll_rarity(floor, rng) {
                Rarity::Common | Rarity::Magic => Rarity::Rare,
                better => better,
            };
            items.push(roll_gear_with_rarity(kind, rarity, rng));
        }
        let gold = rng.gen_range(30..=60);
        let pos = Position::new(chx, chy);
        world.spawn((
            pos,
            VisualPosition::from_position(&pos),
            Sprite::from_ref(tile_ids::CHEST_CLOSED),
            Container::chest(items, gold),
            BlocksMovement,
        ));
    }
}

/// Generate randomized chest contents.
fn generate_chest_contents(floor: u32, rng: &mut impl Rng) -> Container {
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

    let mut items = if roll < 0.35 {
        let item = common_items.choose(rng).copied().unwrap_or(ItemType::Bread);
        vec![item]
    } else if roll < 0.55 {
        let item = uncommon_items.choose(rng).copied().unwrap_or(ItemType::Bread);
        vec![item]
    } else if roll < 0.70 {
        let item = rare_items.choose(rng).copied().unwrap_or(ItemType::Bread);
        vec![item]
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
        vec![
            common_items.choose(rng).copied().unwrap_or(ItemType::Bread),
            all_items.choose(rng).copied().unwrap_or(ItemType::Bread),
        ]
    } else {
        vec![]
    };

    // 30% chance to include arrows (3-8 arrows)
    if rng.gen::<f32>() < 0.30 {
        let arrow_count = rng.gen_range(3..=8);
        for _ in 0..arrow_count {
            items.push(ItemType::Arrow);
        }
    }

    // Small chance to include a bundle of fire arrows
    if rng.gen::<f32>() < CHEST_FIRE_ARROW_CHANCE {
        let count = rng.gen_range(CHEST_FIRE_ARROW_MIN..=CHEST_FIRE_ARROW_MAX);
        for _ in 0..count {
            items.push(ItemType::FireArrow);
        }
    }

    let gold = match roll {
        r if r < 0.35 => rng.gen_range(5..15),
        r if r < 0.55 => rng.gen_range(8..20),
        r if r < 0.70 => rng.gen_range(10..25),
        r if r < 0.85 => rng.gen_range(10..25),
        _ => rng.gen_range(20..50),
    };

    let mut instances: Vec<ItemInstance> = items.into_iter().map(ItemInstance::plain).collect();

    // 30% chance to also contain a rolled gear piece (weapon or armor).
    if rng.gen::<f32>() < 0.30 {
        if let Some(&kind) = GEAR_POOL.choose(rng) {
            instances.push(roll_gear(kind, floor, rng));
        }
    }

    Container::chest(instances, gold)
}

/// Initialize the game world with player, enemies, and objects. `rng` drives
/// all loot/spawn rolls, so a seeded rng reproduces the floor exactly.
/// Returns (world, player_entity, player_start_position).
pub fn init_world(
    grid: &Grid,
    player_class: PlayerClass,
    rng: &mut impl Rng,
) -> (World, Entity, Position) {
    let mut world = World::new();

    // Find player spawn position (default to map center; overwritten below)
    let mut player_start = Position::new(grid.width as i32 / 2, grid.height as i32 / 2);
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
        starting_inventory.current_weight_kg += item_weight(item.kind);
        starting_inventory.items.push(item);
    }

    // Get class stats
    let (str, int, agi) = player_class.stats();

    // Spawn player with class-specific attributes
    let player_entity = world.spawn((
        player_start,
        VisualPosition::from_position(&player_start),
        Sprite::from_ref(player_class.sprite()),
        Name::new(player_class.name()),
        Player,
        Actor::new(PLAYER_SPEED),
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

    // The player can catch fire (e.g. standing in burning grass).
    let _ = world.insert_one(
        player_entity,
        crate::components::Combustible { flammability: PLAYER_FLAMMABILITY },
    );

    // Survival meters are player-only: enemies never hunger or tire.
    let _ = world.insert(
        player_entity,
        (
            crate::components::Hunger::new(),
            crate::components::Fatigue::new(),
        ),
    );

    // Fighter gets a secondary ability (Stun)
    if player_class == PlayerClass::Fighter {
        let _ = world.insert_one(player_entity, SecondaryAbility::new(AbilityType::Stun, STUN_COOLDOWN));
    }

    // Druid gets a secondary ability (Barkskin)
    if player_class == PlayerClass::Druid {
        let _ = world.insert_one(player_entity, SecondaryAbility::new(AbilityType::Barkskin, BARKSKIN_COOLDOWN));
    }

    // Necromancer gets a secondary ability (Fear)
    if player_class == PlayerClass::Necromancer {
        let _ = world.insert_one(player_entity, SecondaryAbility::new(AbilityType::Fear, FEAR_ABILITY_COOLDOWN));
    }

    // Every class carries a spell list (filled by studying scrolls); the
    // Necromancer starts with Raise Dead in it.
    {
        let mut learned = crate::components::LearnedAbilities::default();
        if player_class == PlayerClass::Necromancer {
            learned.learn(AbilityType::RaiseDead);
        }
        let _ = world.insert_one(player_entity, learned);
    }

    // Every class gets its kit of extra abilities (Guard; the Ranger's four;
    // Thorns/Entangle; Bone Ward/Sacrifice/Corpse Explosion).
    let _ = world.insert_one(player_entity, ClassKit::for_class(player_class));

    // Spawn chests, doors, braziers, coffins, barrels, water, and shop
    // (all rolls come from the caller's rng — seeded for reproducible floors).
    // init_world always builds the first floor (floor 0).
    //
    // One occupancy map runs the length of floor construction: every pass that
    // places a movement blocker claims its tile through it, so no two of them
    // can land on the same tile however generation rolled their positions.
    // Seeded from the world because the player is already spawned.
    let mut occupancy = TileOccupancy::from_world(&world);
    spawn_chests(&mut world, grid, 0, &mut occupancy, rng);
    spawn_doors(&mut world, grid, &mut occupancy);
    spawn_secret_door_entity(&mut world, grid, &mut occupancy);
    spawn_braziers(&mut world, grid);
    spawn_coffins(&mut world, grid, 0, &mut occupancy, rng);
    spawn_oil_barrels(&mut world, grid, &mut occupancy, rng);
    spawn_barrels(&mut world, grid, &mut occupancy, rng);
    spawn_water_entities(&mut world, grid);
    spawn_shop_decorations(&mut world, grid, &mut occupancy, rng);
    spawn_dungeon_traps(&mut world, grid, rng);
    spawn_furniture_pieces(&mut world, grid, &mut occupancy, rng);
    spawn_cave_features(&mut world, grid, &mut occupancy);
    spawn_vendor(&mut world, grid, &mut occupancy, 0); // Floor 0 for initial world

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
                if grid.is_walkable(x, y) && occupancy.claim((x, y)) {
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
                && occupancy.claim((cx, cy))
            {
                spawning::npcs::WIZARD.spawn(&mut world, cx, cy);
            }
        }
    }

    // Cave ecology: caverns get bats and spiders, and the floor roster stays
    // out of them. init_world always builds the first floor (floor 0).
    let cavern_tiles = spawning::spawn_cave_fauna(&mut world, grid, 0, &mut occupancy, rng);

    // Spawn enemies
    let walkable_tiles: Vec<(i32, i32)> = (0..grid.height as i32)
        .flat_map(|y| (0..grid.width as i32).map(move |x| (x, y)))
        .filter(|&(x, y)| grid.is_walkable(x, y))
        .filter(|p| !cavern_tiles.contains(p))
        .collect();

    let spawn_config = spawning::SpawnConfig::for_floor(0);
    spawn_config.spawn_all(
        &mut world,
        &walkable_tiles,
        &[(player_start.x, player_start.y)],
        grid.starting_room.as_ref(),
        &mut occupancy,
        rng,
    );

    // Floor 0 has no boss (see `is_boss_floor`), so there is no boss pass here.

    // Unlit oil spills go down last, once every blocker has its tile.
    spawn_oil_spills(
        &mut world,
        grid,
        &occupancy,
        &[(player_start.x, player_start.y)],
        rng,
    );

    #[cfg(debug_assertions)]
    crate::tile_occupancy::assert_one_blocker_per_tile(&world, "init_world");

    (world, player_entity, player_start)
}

/// Initialize all AI actors with their first action in the time system.
/// Only schedules entities that are currently active (within range of player).
pub fn initialize_ai_actors(ctx: &mut ActorCtx) {
    // Only initialize AI for entities that are active (within range).
    //
    // Sorted because the tracker stores them in a `HashSet`, whose iteration
    // order is randomized per process. Each `decide_action` draws from the run
    // rng, so an unsorted order would scramble the draw sequence and make a
    // seeded run unreproducible between launches.
    let mut active_entities: Vec<Entity> = ctx
        .tracker
        .get_active_entities()
        .iter()
        .copied()
        .collect();
    active_entities.sort_unstable();

    for entity in active_entities {
        crate::systems::ai::decide_action(ctx, entity);
    }
}

/// Initialize a single AI actor (used when spawning new enemies mid-game).
/// Only schedules the entity if it's within active range of the player.
pub fn initialize_single_ai_actor(ctx: &mut ActorCtx, entity: Entity) {
    // Register the entity with the tracker (starts as dormant)
    ctx.tracker.register_entity(entity);

    // decide_action will check distance and either process or mark dormant
    crate::systems::ai::decide_action(ctx, entity);
}

/// Spawn floor entities for a new (unsaved) floor. `ctx.rng` drives all loot
/// and spawn rolls; pass a context holding a per-floor seeded rng for
/// reproducible floors.
pub fn spawn_floor_entities(
    ctx: &mut ActorCtx,
    player_spawn_pos: (i32, i32),
    floor_num: u32,
) {
    let ActorCtx { world, grid, rng, player, .. } = ctx;
    let (world, grid, rng, player_entity) = (&mut **world, &**grid, &mut **rng, *player);

    // Update player position
    if let Ok(mut pos) = world.get::<&mut Position>(player_entity) {
        pos.x = player_spawn_pos.0;
        pos.y = player_spawn_pos.1;
    }
    if let Ok(mut vis_pos) = world.get::<&mut VisualPosition>(player_entity) {
        vis_pos.x = player_spawn_pos.0 as f32;
        vis_pos.y = player_spawn_pos.1 as f32;
    }

    // One occupancy map for the whole floor, so no two blocker-placing passes
    // can land on the same tile. Seeded from the world: a floor transition
    // clears the floor's entities but keeps the player and their companions.
    let mut occupancy = TileOccupancy::from_world(world);

    // Spawn chests, doors, braziers, coffins, barrels, and shop from the
    // per-floor rng. Same passes and same order as `init_world`: Crypt is a
    // required room theme on every floor, so every floor rolls coffin
    // positions, and this pass used to leave them unspawned — crypts below
    // floor 0 were decorated rooms with nothing in them.
    spawn_chests(world, grid, floor_num, &mut occupancy, rng);
    spawn_doors(world, grid, &mut occupancy);
    spawn_secret_door_entity(world, grid, &mut occupancy);
    spawn_braziers(world, grid);
    spawn_coffins(world, grid, floor_num, &mut occupancy, rng);
    spawn_oil_barrels(world, grid, &mut occupancy, rng);
    spawn_barrels(world, grid, &mut occupancy, rng);
    spawn_shop_decorations(world, grid, &mut occupancy, rng);
    spawn_dungeon_traps(world, grid, rng);
    spawn_furniture_pieces(world, grid, &mut occupancy, rng);
    spawn_cave_features(world, grid, &mut occupancy);
    spawn_vendor(world, grid, &mut occupancy, floor_num);

    // Cave ecology: caverns get bats and spiders, and the floor roster stays
    // out of them.
    let cavern_tiles = spawning::spawn_cave_fauna(world, grid, floor_num, &mut occupancy, rng);

    // Spawn enemies
    let walkable_tiles: Vec<(i32, i32)> = (0..grid.height as i32)
        .flat_map(|y| (0..grid.width as i32).map(move |x| (x, y)))
        .filter(|&(x, y)| grid.is_walkable(x, y))
        .filter(|p| !cavern_tiles.contains(p))
        .collect();

    let spawn_config = spawning::SpawnConfig::for_floor(floor_num);
    spawn_config.spawn_all(
        world,
        &walkable_tiles,
        &[player_spawn_pos],
        grid.starting_room.as_ref(),
        &mut occupancy,
        rng,
    );

    // Every 3rd floor: a named boss guarding a Rare+ chest in the largest room.
    spawn_boss_encounter(world, grid, floor_num, &mut occupancy, rng);

    // Unlit oil spills go down last, once every blocker has its tile.
    spawn_oil_spills(world, grid, &occupancy, &[player_spawn_pos], rng);

    #[cfg(debug_assertions)]
    crate::tile_occupancy::assert_one_blocker_per_tile(world, "spawn_floor_entities");

    // Initialize AI
    initialize_ai_actors(ctx);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::components::OilPuddle;
    use rand::SeedableRng;

    /// Oil leaks beside barrels that stand in rooms, never beside a corridor
    /// barrel (a spill across a one-tile corridor would make the barrel an
    /// inescapable trap).
    #[test]
    fn corridor_barrels_get_no_oil_spills() {
        let mut corridor_spills = 0;
        let mut room_spills = 0;
        for seed in 0..60u64 {
            let mut grid = crate::systems::actions::TestArena::new((0, 0)).grid;
            grid.themed_rooms.push(crate::dungeon_gen::ThemedRoom {
                rect: crate::dungeon_gen::Rect::new(1, 1, 7, 7),
                theme: RoomTheme::Storage,
            });
            let mut world = World::new();
            spawning::spawn_oil_barrel(&mut world, 4, 4);
            spawning::spawn_oil_barrel(&mut world, 12, 12);
            let occupancy = TileOccupancy::from_world(&world);
            let mut rng = rand::rngs::StdRng::seed_from_u64(seed);
            spawn_oil_spills(&mut world, &grid, &occupancy, &[], &mut rng);

            let near = |(bx, by): (i32, i32)| {
                world
                    .query::<&Position>()
                    .with::<&OilPuddle>()
                    .iter()
                    .filter(|(_, p)| (p.x - bx).abs().max((p.y - by).abs()) <= 1)
                    .count()
            };
            corridor_spills += near((12, 12));
            room_spills += near((4, 4));
        }
        assert_eq!(corridor_spills, 0, "a corridor barrel never leaks");
        assert!(room_spills > 0, "room barrels still do");
    }
}
