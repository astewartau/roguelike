//! Floor transition and save/load logic for multi-floor dungeons.

use crate::components::{
    BlocksMovement, BlocksVision, ChaseAI, Container, ContainerType, Door, GroundItemPile, Health,
    ItemInstance, Position, Sprite, VisualPosition,
};
use crate::constants::*;
use crate::grid::Grid;
use crate::spawning;
use crate::tile::tile_ids;
use crate::time_system::ActionScheduler;

use hecs::{Entity, World};
use std::collections::HashMap;

use super::initialization::spawn_floor_entities;
use super::ActorCtx;

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

/// Boss role data saved with a boss enemy (name, ability, announced state).
pub struct SavedBoss {
    pub name: String,
    pub ability: crate::components::BossAbility,
    pub announced: bool,
}

/// Types of entities that can be saved.
pub enum SavedEntityType {
    Enemy {
        /// The full stat/trait template the enemy was spawned from, so a
        /// revisited floor restores the same enemy type (shaman, spider,
        /// archer, ...) instead of downgrading everything to a skeleton.
        def: spawning::EnemyDef,
        health_current: i32,
        health_max: i32,
        /// Whether the enemy was asleep when the player left the floor.
        asleep: bool,
        /// Present if this enemy is the floor boss.
        boss: Option<SavedBoss>,
    },
    /// Any container that is not a corpse: chest, coffin, barrel or a pile of
    /// dropped items.
    ///
    /// This used to be a `Chest` variant holding only `is_open`/`gold`/`items`,
    /// which meant a revisited floor restored *every* container as a chest — a
    /// coffin came back as a chest that could no longer release its skeleton
    /// (`spawn_chance` was gone), a barrel came back wearing a chest sprite,
    /// and a pile of dropped loot came back as a blocking chest.
    Container {
        container_type: ContainerType,
        is_open: bool,
        gold: u32,
        items: Vec<ItemInstance>,
        /// Chance to release an enemy when opened. Only coffins set it, and
        /// losing it was what defanged a revisited crypt.
        spawn_chance: f32,
    },
    Door {
        is_open: bool,
    },
    Bones {
        gold: u32,
        items: Vec<ItemInstance>,
        /// What died here, so the loot window can still say "Goblin's
        /// corpse" after the floor is revisited.
        name: Option<String>,
    },
    /// An undiscovered secret door (discovered ones are saved as `Door`)
    SecretDoor,
    /// A dungeon floor trap (triggered ones are despawned, so never saved)
    Trap {
        kind: crate::components::DungeonTrapKind,
        revealed: bool,
    },
    /// Room furniture (fountain / altar / shrine)
    Furniture {
        kind: crate::components::FurnitureKind,
        used: bool,
    },
    /// An unlit oil puddle. A puddle that is burning when the player leaves
    /// is not saved: it would have burned away long before they returned.
    OilPuddle,
    /// An intact explosive oil barrel (a lit fuse is not saved; the barrel
    /// comes back cold, at its saved health).
    OilBarrel {
        health_current: i32,
    },
}

/// Result of a floor transition. The new grid is written straight into the
/// context's `grid`, so it is not returned here.
pub struct FloorTransitionResult {
    pub new_floor: u32,
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

    // Save enemies (hostile ChaseAI actors; tamed companions use CompanionAI
    // and are not floor-bound). The spawn template stored on the entity
    // preserves its exact type; enemies spawned without one (dev tools,
    // legacy saves) fall back to a skeleton as before.
    for (id, (pos, health, _)) in world.query::<(&Position, &Health, &ChaseAI)>().iter() {
        if id == player_entity {
            continue;
        }
        let def = world
            .get::<&spawning::EnemyDef>(id)
            .map(|d| (*d).clone())
            .unwrap_or_else(|_| spawning::enemies::SKELETON.clone());
        let asleep = world
            .entity(id)
            .map(|e| e.has::<crate::components::Asleep>())
            .unwrap_or(false);
        let boss = world.get::<&crate::components::Boss>(id).ok().map(|b| SavedBoss {
            name: world
                .get::<&crate::components::Name>(id)
                .map(|n| n.0.clone())
                .unwrap_or_else(|_| def.name.to_string()),
            ability: b.ability,
            announced: b.announced,
        });
        entities.push(SavedEntity {
            pos: (pos.x, pos.y),
            entity_type: SavedEntityType::Enemy {
                def,
                health_current: health.current,
                health_max: health.max,
                asleep,
                boss,
            },
        });
    }

    // Save containers. Corpses keep their own variant because they restore
    // with different semantics (always open, never blocking); everything else
    // round-trips through `Container` with its kind intact. The corpse test
    // reads the component rather than sniffing for the bones sprite, which is
    // what it used to do.
    for (id, (pos, container)) in world.query::<(&Position, &Container)>().iter() {
        if id == player_entity {
            continue;
        }

        if container.container_type == ContainerType::Corpse {
            entities.push(SavedEntity {
                pos: (pos.x, pos.y),
                entity_type: SavedEntityType::Bones {
                    gold: container.gold,
                    items: container.items.clone(),
                    name: world
                        .get::<&crate::components::Name>(id)
                        .ok()
                        .map(|n| n.0.clone()),
                },
            });
        } else {
            entities.push(SavedEntity {
                pos: (pos.x, pos.y),
                entity_type: SavedEntityType::Container {
                    container_type: container.container_type,
                    is_open: container.is_open,
                    gold: container.gold,
                    items: container.items.clone(),
                    spawn_chance: container.spawn_chance,
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

    // Save undiscovered secret doors
    for (_, (pos, _)) in world
        .query::<(&Position, &crate::components::SecretDoor)>()
        .iter()
    {
        entities.push(SavedEntity {
            pos: (pos.x, pos.y),
            entity_type: SavedEntityType::SecretDoor,
        });
    }

    // Save untriggered dungeon traps (keeping their revealed state)
    for (_, (pos, trap)) in world
        .query::<(&Position, &crate::components::DungeonTrap)>()
        .iter()
    {
        entities.push(SavedEntity {
            pos: (pos.x, pos.y),
            entity_type: SavedEntityType::Trap {
                kind: trap.kind,
                revealed: trap.revealed,
            },
        });
    }

    // Save room furniture (keeping spent state)
    for (_, (pos, furniture)) in world
        .query::<(&Position, &crate::components::Furniture)>()
        .iter()
    {
        entities.push(SavedEntity {
            pos: (pos.x, pos.y),
            entity_type: SavedEntityType::Furniture {
                kind: furniture.kind,
                used: furniture.used,
            },
        });
    }

    // Save unlit oil puddles (burning ones would be gone by the next visit).
    for (_, pos) in world
        .query::<&Position>()
        .with::<&crate::components::OilPuddle>()
        .without::<&crate::components::BurningOil>()
        .iter()
    {
        entities.push(SavedEntity {
            pos: (pos.x, pos.y),
            entity_type: SavedEntityType::OilPuddle,
        });
    }

    // Save oil barrels that are still standing.
    for (_, (pos, health)) in world
        .query::<(&Position, &Health)>()
        .with::<&crate::components::OilBarrel>()
        .iter()
    {
        if health.current > 0 {
            entities.push(SavedEntity {
                pos: (pos.x, pos.y),
                entity_type: SavedEntityType::OilBarrel { health_current: health.current },
            });
        }
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

/// Sprite for a restored container.
///
/// Mirrors what each spawn pass uses and what `handle_container_opened` swaps
/// to, so a container looks the same after a revisit as it did when the player
/// left. A pile of dropped items wears its first item, like the code that
/// creates one.
fn container_sprite(
    container_type: ContainerType,
    is_open: bool,
    items: &[ItemInstance],
) -> (crate::tile::SpriteSheet, u32) {
    match container_type {
        ContainerType::Chest => {
            if is_open {
                tile_ids::CHEST_OPEN
            } else {
                tile_ids::CHEST_CLOSED
            }
        }
        ContainerType::Coffin => {
            if is_open {
                tile_ids::COFFIN_OPEN
            } else {
                tile_ids::COFFIN_CLOSED
            }
        }
        // Barrels keep one sprite open or shut.
        ContainerType::Barrel => tile_ids::BARREL,
        ContainerType::Corpse => tile_ids::BONES_4,
        ContainerType::GroundPile => items
            .first()
            .map(|item| crate::systems::item_defs::get_def(item.kind).sprite)
            .unwrap_or(tile_ids::COINS),
    }
}

/// Load a saved floor, spawning entities.
pub fn load_floor(
    ctx: &mut ActorCtx,
    saved_entities: &[SavedEntity],
    player_spawn_pos: (i32, i32),
) {
    let player_entity = ctx.player;

    // Update player position
    if let Ok(mut pos) = ctx.world.get::<&mut Position>(player_entity) {
        pos.x = player_spawn_pos.0;
        pos.y = player_spawn_pos.1;
    }
    if let Ok(mut vis_pos) = ctx.world.get::<&mut VisualPosition>(player_entity) {
        vis_pos.x = player_spawn_pos.0 as f32;
        vis_pos.y = player_spawn_pos.1 as f32;
    }

    for saved_entity in saved_entities {
        let pos = Position::new(saved_entity.pos.0, saved_entity.pos.1);
        match &saved_entity.entity_type {
            SavedEntityType::Enemy { def, health_current, health_max, asleep, boss } => {
                let enemy = def.spawn(ctx.world, pos.x, pos.y, ctx.rng);
                if let Ok(mut health) = ctx.world.get::<&mut Health>(enemy) {
                    health.current = *health_current;
                    health.max = *health_max;
                }
                // spawn() re-rolls the sleep chance; restore the state the
                // enemy was actually left in.
                if *asleep {
                    let _ = ctx.world.insert_one(enemy, crate::components::Asleep);
                } else {
                    let _ = ctx.world.remove_one::<crate::components::Asleep>(enemy);
                }
                if let Some(b) = boss {
                    spawning::apply_boss_role(ctx.world, enemy, &b.name, b.ability, b.announced);
                }
                crate::systems::ai::decide_action(ctx, enemy);
            }
            SavedEntityType::Container {
                container_type,
                is_open,
                gold,
                items,
                spawn_chance,
            } => {
                let container = Container {
                    container_type: *container_type,
                    items: items.clone(),
                    gold: *gold,
                    is_open: *is_open,
                    spawn_chance: *spawn_chance,
                };
                let sprite_ref = container_sprite(*container_type, *is_open, items);
                // Read before the container moves into the world. Same
                // predicate the live sweep uses, so staying on a floor and
                // revisiting it agree about what blocks.
                let looted = container.is_looted();
                let entity = ctx.world.spawn((
                    pos,
                    VisualPosition::from_position(&pos),
                    Sprite::from_ref(sprite_ref),
                    container,
                ));

                // A pile of dropped items is walkable and needs its marker so
                // the pickup prompt still finds it.
                if *container_type == ContainerType::GroundPile {
                    let _ = ctx.world.insert_one(entity, GroundItemPile);
                } else if !looted {
                    let _ = ctx.world.insert_one(entity, BlocksMovement);
                }
                // Storage barrels can be shoved (see `components::Pushable`).
                if *container_type == ContainerType::Barrel {
                    let _ = ctx.world.insert_one(entity, crate::components::Pushable);
                }
            }
            SavedEntityType::Door { is_open } => {
                if *is_open {
                    let mut door = Door::new();
                    door.is_open = true;
                    ctx.world.spawn((
                        pos,
                        VisualPosition::from_position(&pos),
                        Sprite::from_ref(tile_ids::DOOR),
                        door,
                    ));
                } else {
                    ctx.world.spawn((
                        pos,
                        VisualPosition::from_position(&pos),
                        Sprite::from_ref(tile_ids::DOOR),
                        Door::new(),
                        BlocksVision,
                        BlocksMovement,
                    ));
                }
            }
            SavedEntityType::Bones { gold, items, name } => {
                let mut container = Container::corpse(items.clone(), *gold);
                container.is_open = true;
                let corpse = ctx.world.spawn((
                    pos,
                    VisualPosition::from_position(&pos),
                    Sprite::from_ref(tile_ids::BONES_4),
                    container,
                ));
                if let Some(name) = name {
                    let _ = ctx
                        .world
                        .insert_one(corpse, crate::components::Name::new(name.clone()));
                }
            }
            SavedEntityType::SecretDoor => {
                let wall_sprite =
                    super::initialization::secret_door_wall_sprite(ctx.grid, pos.x, pos.y);
                spawning::spawn_secret_door(ctx.world, pos.x, pos.y, wall_sprite);
            }
            SavedEntityType::Trap { kind, revealed } => {
                let trap = spawning::spawn_dungeon_trap(ctx.world, pos.x, pos.y, *kind);
                if *revealed {
                    crate::systems::discovery::reveal_trap(ctx.world, trap, *kind);
                }
            }
            SavedEntityType::Furniture { kind, used } => {
                let piece = spawning::spawn_furniture(ctx.world, pos.x, pos.y, *kind);
                if *used {
                    crate::systems::furniture::mark_spent(ctx.world, piece);
                }
            }
            SavedEntityType::OilPuddle => {
                spawning::spawn_oil_puddle(ctx.world, pos.x, pos.y);
            }
            SavedEntityType::OilBarrel { health_current } => {
                let barrel = spawning::spawn_oil_barrel(ctx.world, pos.x, pos.y);
                if let Ok(mut health) = ctx.world.get::<&mut Health>(barrel) {
                    health.current = *health_current;
                }
            }
        }
    }
}

/// Handle a floor transition (going up or down stairs).
///
/// `run_seed` is the run's seed: brand-new floors derive a per-floor rng from
/// it (via `game_state::floor_seed`), so the same seed always produces the
/// same floors regardless of visit order. Revisited floors are restored from
/// their saved state instead.
pub fn handle_floor_transition(
    ctx: &mut ActorCtx,
    floors: &mut HashMap<u32, SavedFloor>,
    current_floor: u32,
    run_seed: u64,
    direction: crate::events::StairDirection,
) -> FloorTransitionResult {
    use crate::events::StairDirection;
    use rand::rngs::StdRng;
    use rand::SeedableRng;

    let player_entity = ctx.player;

    let target_floor = match direction {
        StairDirection::Down => current_floor + 1,
        StairDirection::Up => {
            assert!(current_floor > 0, "Cannot go up from floor 0");
            current_floor - 1
        }
    };

    // The target floor's layout, loot and AI-initialization rolls all come from
    // a per-floor rng derived from the run seed, so the same seed produces the
    // same floor regardless of visit order.
    let mut floor_rng =
        StdRng::seed_from_u64(super::game_state::floor_seed(run_seed, target_floor));

    // Resolve the target floor first - a revisit restores its saved grid and
    // entities, a first visit generates a fresh grid. `target_floor` is never
    // `current_floor`, so taking it out before saving below is independent of
    // the insert. Doing this up front means the grid we are leaving can be
    // swapped straight out of the context, with no placeholder grid.
    let (new_grid, saved_entities) = match floors.remove(&target_floor) {
        Some(saved) => (saved.grid, Some(saved.entities)),
        None => (
            Grid::new_floor(
                DUNGEON_DEFAULT_WIDTH,
                DUNGEON_DEFAULT_HEIGHT,
                target_floor,
                &mut floor_rng,
            ),
            None,
        ),
    };
    let spawn_pos = match direction {
        StairDirection::Down => new_grid.stairs_up_pos.unwrap_or((1, 1)),
        StairDirection::Up => new_grid.stairs_down_pos.unwrap_or((1, 1)),
    };

    // Save the floor we are leaving, then clear its entities.
    let current_grid = std::mem::replace(ctx.grid, new_grid);
    let saved_floor = save_floor(ctx.world, current_grid, player_entity);
    floors.insert(current_floor, saved_floor);
    clear_floor_entities(ctx.world, player_entity, ctx.scheduler);

    // Populate the new floor from the per-floor rng rather than the run rng.
    {
        let mut floor_ctx = ActorCtx {
            rng: &mut floor_rng,
            ..ctx.reborrow()
        };
        match &saved_entities {
            Some(entities) => load_floor(&mut floor_ctx, entities, spawn_pos),
            None => spawn_floor_entities(&mut floor_ctx, spawn_pos, target_floor),
        }
    }

    // Rebuild caches for the new floor
    ctx.spatial.rebuild_in_place(ctx.world);
    let player_pos = ctx
        .world
        .get::<&Position>(player_entity)
        .map(|p| (p.x, p.y))
        .unwrap_or((0, 0));
    ctx.tracker.initialize_from_world(ctx.world, player_pos);

    let player_visual_pos = ctx
        .world
        .get::<&VisualPosition>(player_entity)
        .map(|vp| (vp.x, vp.y))
        .unwrap_or((1.0, 1.0));

    FloorTransitionResult {
        new_floor: target_floor,
        player_visual_pos,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::components::{Asleep, Boss, BossAbility, FearImmune, SupportAI, Venomous};
    use crate::events::EventQueue;
    use crate::time_system::GameClock;
    use rand::rngs::StdRng;
    use rand::SeedableRng;

    fn setup() -> (World, Grid, Entity) {
        let mut rng = StdRng::seed_from_u64(99);
        let grid = Grid::new_floor(40, 40, 1, &mut rng);
        let mut world = World::new();
        let pos = Position::new(1, 1);
        let player = world.spawn((pos, VisualPosition::from_position(&pos), Health::new(30)));
        (world, grid, player)
    }

    /// Save the floor, clear it, and load it back — the round trip the player
    /// takes when descending and then returning up the stairs.
    fn save_and_reload(world: &mut World, grid: Grid, player: Entity) -> Grid {
        let saved = save_floor(world, grid, player);
        let mut scheduler = ActionScheduler::new();
        clear_floor_entities(world, player, &mut scheduler);

        let mut clock = GameClock::new();
        let mut tracker = crate::active_ai_tracker::ActiveAITracker::new();
        let mut cache = crate::spatial_cache::SpatialCache::rebuild_from_world(world);
        let mut events = EventQueue::new();
        let mut grid = saved.grid;
        let mut rng = StdRng::seed_from_u64(7);
        load_floor(
            &mut ActorCtx {
                world,
                grid: &mut grid,
                player,
                clock: &mut clock,
                scheduler: &mut scheduler,
                tracker: &mut tracker,
                spatial: &mut cache,
                events: &mut events,
                rng: &mut rng,
            },
            &saved.entities,
            (1, 1),
        );
        grid
    }

    /// Find the (unique) entity carrying the given display name.
    fn find_named(world: &World, name: &str) -> Entity {
        let matches: Vec<Entity> = world
            .query::<&crate::components::Name>()
            .iter()
            .filter(|(_, n)| n.0 == name)
            .map(|(id, _)| id)
            .collect();
        assert_eq!(matches.len(), 1, "expected exactly one '{name}'");
        matches[0]
    }

    #[test]
    fn test_revisited_floor_preserves_enemy_types() {
        let (mut world, grid, player) = setup();

        // A venomous spider, a support shaman, and an archer — the enemy
        // kinds the old save path used to downgrade to plain skeletons.
        let mut rng = StdRng::seed_from_u64(5);
        let spider = spawning::enemies::GIANT_SPIDER.spawn(&mut world, 5, 5, &mut rng);
        let shaman = spawning::enemies::GOBLIN_SHAMAN.spawn(&mut world, 7, 5, &mut rng);
        let archer = spawning::enemies::SKELETON_ARCHER.spawn(&mut world, 9, 5, &mut rng);

        // Deterministic health + sleep state to verify restoration.
        world.get::<&mut Health>(spider).unwrap().current = 3;
        let _ = world.remove_one::<Asleep>(spider);
        let _ = world.insert_one(shaman, Asleep);
        let _ = world.remove_one::<Asleep>(archer);

        save_and_reload(&mut world, grid, player);

        let spider = find_named(&world, "Giant Spider");
        assert!(world.get::<&crate::components::Spider>(spider).is_ok());
        assert!(world.get::<&Venomous>(spider).is_ok());
        assert_eq!(world.get::<&Health>(spider).unwrap().current, 3);
        assert!(world.get::<&Asleep>(spider).is_err(), "awake spider stays awake");

        let shaman = find_named(&world, "Goblin Shaman");
        assert!(world.get::<&SupportAI>(shaman).is_ok());
        assert!(world.get::<&Asleep>(shaman).is_ok(), "sleeping shaman stays asleep");

        let archer = find_named(&world, "Skeleton Archer");
        let has_bow = world
            .get::<&crate::components::Equipment>(archer)
            .map(|e| e.get_bow().is_some())
            .unwrap_or(false);
        assert!(has_bow, "archer keeps its ranged weapon");
    }

    #[test]
    fn test_revisited_floor_preserves_boss() {
        let (mut world, grid, player) = setup();

        let boss = spawning::spawn_boss(&mut world, 3, 6, 6, &mut StdRng::seed_from_u64(6))
            .expect("floor 3 boss");
        let scaled_max = world.get::<&Health>(boss).unwrap().max;
        world.get::<&mut Health>(boss).unwrap().current = scaled_max - 7;
        world.get::<&mut Boss>(boss).unwrap().announced = true;

        save_and_reload(&mut world, grid, player);

        let boss = find_named(&world, "Gnash, Orc Warlord");
        let role = world.get::<&Boss>(boss).map(|b| *b).expect("Boss role restored");
        assert_eq!(role.ability, BossAbility::GroundSlam);
        assert!(role.announced, "announcement state survives the round trip");
        assert!(world.get::<&FearImmune>(boss).is_ok());
        assert!(world.get::<&Asleep>(boss).is_err(), "bosses never sleep");
        let health = world.get::<&Health>(boss).map(|h| (h.current, h.max)).unwrap();
        assert_eq!(health, (scaled_max - 7, scaled_max), "scaled boss HP survives");
    }

    #[test]
    fn test_enemy_without_template_falls_back_to_skeleton() {
        let (mut world, grid, player) = setup();

        // A bare hostile without a spawn template (dev tools / legacy saves).
        let pos = Position::new(4, 4);
        world.spawn((
            pos,
            VisualPosition::from_position(&pos),
            Health::new(9),
            ChaseAI::new(6),
        ));

        save_and_reload(&mut world, grid, player);

        let skeleton = find_named(&world, "Skeleton");
        assert_eq!(world.get::<&Health>(skeleton).unwrap().current, 9);
    }

    fn unlit_puddles(world: &World) -> Vec<(i32, i32)> {
        let mut v: Vec<(i32, i32)> = world
            .query::<&Position>()
            .with::<&crate::components::OilPuddle>()
            .without::<&crate::components::BurningOil>()
            .iter()
            .map(|(_, p)| (p.x, p.y))
            .collect();
        v.sort_unstable();
        v
    }

    /// Floor construction spills unlit oil (beside barrels and in rooms), on
    /// plain floor only and never under a blocker; and the puddles — and the
    /// barrels — come back when the floor is revisited.
    #[test]
    fn generated_oil_spills_are_unlit_valid_and_survive_a_revisit() {
        use crate::components::{BlocksMovement, OilBarrel, PlayerClass};
        let mut total = 0;
        let mut checked_round_trip = false;
        for seed in 0..12u64 {
            let mut rng = StdRng::seed_from_u64(seed);
            let grid = Grid::new_floor(DUNGEON_DEFAULT_WIDTH, DUNGEON_DEFAULT_HEIGHT, 0, &mut rng);
            let (mut world, player, _) =
                super::super::initialization::init_world(&grid, PlayerClass::Fighter, &mut rng);
            let puddles = unlit_puddles(&world);
            total += puddles.len();
            for &(x, y) in &puddles {
                assert_eq!(
                    grid.get(x, y).map(|t| t.tile_type),
                    Some(crate::tile::TileType::Floor),
                    "seed {seed}: puddle on plain floor"
                );
                assert!(!grid.water_positions.contains(&(x, y)));
                let blocked = world
                    .query::<(&Position, &BlocksMovement)>()
                    .iter()
                    .any(|(id, (p, _))| id != player && (p.x, p.y) == (x, y));
                assert!(!blocked, "seed {seed}: no puddle under a blocker at {x},{y}");
            }

            if !puddles.is_empty() && !checked_round_trip {
                let barrels = world.query::<&OilBarrel>().iter().count();
                save_and_reload(&mut world, grid, player);
                assert_eq!(unlit_puddles(&world), puddles, "seed {seed}: puddles survive");
                assert_eq!(
                    world.query::<&OilBarrel>().iter().count(),
                    barrels,
                    "seed {seed}: oil barrels survive"
                );
                checked_round_trip = true;
            }
        }
        assert!(total > 0, "twelve floors produced no oil spill at all");
        assert!(checked_round_trip);
    }

    /// A puddle that is burning when the player leaves is not saved.
    #[test]
    fn burning_puddles_are_not_saved() {
        let (mut world, grid, player) = setup();
        let lit = spawning::spawn_oil_puddle(&mut world, 3, 3);
        crate::systems::fire::ignite_oil_puddle(&mut world, lit);
        spawning::spawn_oil_puddle(&mut world, 4, 4);
        save_and_reload(&mut world, grid, player);
        assert_eq!(unlit_puddles(&world), vec![(4, 4)]);
        assert_eq!(world.query::<&crate::components::OilPuddle>().iter().count(), 1);
    }
}
