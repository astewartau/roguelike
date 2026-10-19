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
    /// A brazier, standing (`lit`) or toppled.
    Brazier {
        lit: bool,
    },
    /// The starting-room campfire.
    Campfire,
    /// A room lantern still standing. Smashed lanterns are despawned, so
    /// they are never saved.
    Lantern {
        style: crate::components::LanternStyle,
    },
    /// An oil barrel that has not gone off yet. Exploded barrels are
    /// despawned, so they are simply never saved.
    OilBarrel {
        health_current: i32,
        health_max: i32,
        /// Seconds left on a lit fuse, if the barrel was burning when the
        /// player left. It keeps counting down on return.
        fuse: Option<f32>,
    },
    /// A patch of glowing fungus. Burnt patches are consumed by the fire, so
    /// they are never saved.
    GlowMushroom,
    CrystalCluster,
    Stalagmites,
    /// A decorative blocking prop (shop jar, sack) and the sprite it wore.
    Decoration {
        sprite: Sprite,
    },
    /// An unlit oil puddle. Its `Fuel` is kept so a short-lived drip stays a
    /// drip. Burning puddles are mid-way to burning out and are not saved,
    /// like every other fire.
    OilPuddle {
        fuel: crate::components::Fuel,
    },
    /// An unlit spider web. The spinner link is dropped: enemies come back as
    /// new entities, so the old one would point at nothing.
    Web,
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

    save_scenery(world, &mut entities);

    SavedFloor { grid, entities }
}

/// Save the floor's static scenery: light sources, oil barrels, cave dressing,
/// shop props and loose fuel on the floor. Each is saved in the state it is in
/// now, and anything already destroyed is gone from the world and so is not
/// saved — re-running the spawn passes on a revisit would resurrect it.
///
/// Water animation entities are deliberately absent: they carry no state, and
/// `load_floor` respawns them from `grid.water_positions`.
fn save_scenery(world: &World, entities: &mut Vec<SavedEntity>) {
    use crate::components::{
        BarrelFuse, Brazier, Campfire, CrystalCluster, Decoration, Fuel, GlowMushroom, OilBarrel,
        OilPuddle, OnFire, Stalagmite, Web,
    };

    let mut push = |pos: &Position, entity_type| {
        entities.push(SavedEntity {
            pos: (pos.x, pos.y),
            entity_type,
        });
    };

    for (_, (pos, brazier)) in world.query::<(&Position, &Brazier)>().iter() {
        push(pos, SavedEntityType::Brazier { lit: brazier.lit });
    }
    for (_, (pos, _)) in world.query::<(&Position, &Campfire)>().iter() {
        push(pos, SavedEntityType::Campfire);
    }
    for (_, (pos, lantern)) in world.query::<(&Position, &crate::components::Lantern)>().iter() {
        push(pos, SavedEntityType::Lantern { style: lantern.style });
    }
    for (_, (pos, health, fuse)) in world
        .query::<(&Position, &Health, Option<&BarrelFuse>)>()
        .with::<&OilBarrel>()
        .iter()
    {
        // A barrel at zero health detonates on the next fire tick; the player
        // left before it did, so let it go off on return rather than lose it.
        push(
            pos,
            SavedEntityType::OilBarrel {
                health_current: health.current,
                health_max: health.max,
                fuse: fuse.map(|f| f.remaining),
            },
        );
    }
    for (_, (pos, _)) in world.query::<(&Position, &GlowMushroom)>().iter() {
        push(pos, SavedEntityType::GlowMushroom);
    }
    for (_, (pos, _)) in world.query::<(&Position, &CrystalCluster)>().iter() {
        push(pos, SavedEntityType::CrystalCluster);
    }
    for (_, (pos, _)) in world.query::<(&Position, &Stalagmite)>().iter() {
        push(pos, SavedEntityType::Stalagmites);
    }
    for (_, (pos, sprite)) in world.query::<(&Position, &Sprite)>().with::<&Decoration>().iter() {
        push(pos, SavedEntityType::Decoration { sprite: *sprite });
    }
    for (_, (pos, fuel)) in world
        .query::<(&Position, &Fuel)>()
        .with::<&OilPuddle>()
        .without::<&OnFire>()
        .iter()
    {
        push(pos, SavedEntityType::OilPuddle { fuel: *fuel });
    }
    for (_, (pos, _)) in world.query::<(&Position, &Web)>().without::<&OnFire>().iter() {
        push(pos, SavedEntityType::Web);
    }
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

    // Stateless, so rebuilt from the grid rather than saved.
    super::initialization::spawn_water_entities(ctx.world, ctx.grid);

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
            SavedEntityType::Brazier { lit } => {
                let brazier = spawning::spawn_brazier(ctx.world, pos.x, pos.y);
                if !*lit {
                    crate::systems::fire::snuff_brazier(ctx.world, brazier);
                }
            }
            SavedEntityType::Campfire => {
                spawning::spawn_campfire(ctx.world, pos.x, pos.y);
            }
            SavedEntityType::Lantern { style } => {
                spawning::spawn_lantern(ctx.world, pos.x, pos.y, *style);
            }
            SavedEntityType::OilBarrel { health_current, health_max, fuse } => {
                let barrel = spawning::spawn_oil_barrel(ctx.world, pos.x, pos.y);
                if let Ok(mut health) = ctx.world.get::<&mut Health>(barrel) {
                    health.current = *health_current;
                    health.max = *health_max;
                }
                if let Some(remaining) = fuse {
                    let _ = ctx.world.insert_one(
                        barrel,
                        crate::components::BarrelFuse { remaining: *remaining },
                    );
                }
            }
            SavedEntityType::GlowMushroom => {
                spawning::spawn_glow_mushrooms(ctx.world, pos.x, pos.y);
            }
            SavedEntityType::CrystalCluster => {
                spawning::spawn_crystal_cluster(ctx.world, pos.x, pos.y);
            }
            SavedEntityType::Stalagmites => {
                spawning::spawn_stalagmites(ctx.world, pos.x, pos.y);
            }
            SavedEntityType::Decoration { sprite } => {
                spawning::spawn_decoration(ctx.world, pos.x, pos.y, *sprite);
            }
            SavedEntityType::OilPuddle { fuel } => {
                let puddle = spawning::spawn_oil_puddle(ctx.world, pos.x, pos.y);
                let _ = ctx.world.insert_one(puddle, *fuel);
            }
            SavedEntityType::Web => {
                spawning::spawn_web(ctx.world, pos.x, pos.y, None);
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
    fn test_revisited_floor_keeps_scenery_in_its_left_state() {
        use crate::components::{
            Brazier, CausesBurning, GlowMushroom, LightSource, OilBarrel,
        };

        let (mut world, grid, player) = setup();
        let mut events = EventQueue::new();
        let mut rng = StdRng::seed_from_u64(3);

        // One brazier toppled through the real topple path, one left standing.
        let toppled = spawning::spawn_brazier(&mut world, 5, 5);
        assert!(crate::systems::fire::topple_brazier(
            &mut world, &grid, toppled, &mut events, &mut rng
        ));
        spawning::spawn_brazier(&mut world, 9, 9);

        // An intact but scuffed oil barrel, and a mushroom patch.
        let barrel = spawning::spawn_oil_barrel(&mut world, 12, 5);
        world.get::<&mut Health>(barrel).unwrap().current -= 1;
        let barrel_health = *world.get::<&Health>(barrel).unwrap();
        spawning::spawn_glow_mushrooms(&mut world, 14, 5);

        // The topple spilled fire patches; those are transient and are not
        // part of what a revisit restores.
        save_and_reload(&mut world, grid, player);

        let braziers: Vec<(Entity, (i32, i32), bool)> = world
            .query::<(&Position, &Brazier)>()
            .iter()
            .map(|(id, (p, b))| (id, (p.x, p.y), b.lit))
            .collect();
        assert_eq!(braziers.len(), 2, "both braziers come back");
        let (toppled, _, lit) = braziers
            .iter()
            .copied()
            .find(|(_, pos, _)| *pos == (5, 5))
            .expect("toppled brazier restored at its tile");
        assert!(!lit, "a toppled brazier stays toppled");
        assert!(world.get::<&LightSource>(toppled).is_err(), "and stays dark");
        assert!(world.get::<&CausesBurning>(toppled).is_err(), "and stays cold");
        let (standing, _, lit) = braziers
            .iter()
            .copied()
            .find(|(_, pos, _)| *pos == (9, 9))
            .expect("standing brazier restored at its tile");
        assert!(lit, "a standing brazier stays lit");
        assert!(world.get::<&LightSource>(standing).is_ok());

        let barrels: Vec<(Entity, (i32, i32))> = world
            .query::<&Position>()
            .with::<&OilBarrel>()
            .iter()
            .map(|(id, p)| (id, (p.x, p.y)))
            .collect();
        assert_eq!(barrels.len(), 1, "the oil barrel comes back");
        assert_eq!(barrels[0].1, (12, 5));
        let health = *world.get::<&Health>(barrels[0].0).unwrap();
        assert_eq!(
            (health.current, health.max),
            (barrel_health.current, barrel_health.max),
            "barrel damage survives the round trip"
        );

        let mushrooms: Vec<(i32, i32)> = world
            .query::<&Position>()
            .with::<&GlowMushroom>()
            .iter()
            .map(|(_, p)| (p.x, p.y))
            .collect();
        assert_eq!(mushrooms, vec![(14, 5)], "the mushroom patch comes back");
    }

    #[test]
    fn test_revisited_floor_keeps_its_lanterns() {
        use crate::components::{Lantern, LanternStyle};

        let (mut world, grid, player) = setup();
        spawning::spawn_lantern(&mut world, 12, 5, LanternStyle::Ghost);
        let smashed = spawning::spawn_lantern(&mut world, 14, 5, LanternStyle::Warm);
        world.despawn(smashed).unwrap();

        save_and_reload(&mut world, grid, player);

        let lanterns: Vec<((i32, i32), LanternStyle)> = world
            .query::<(&Position, &Lantern)>()
            .iter()
            .map(|(_, (p, l))| ((p.x, p.y), l.style))
            .collect();
        assert_eq!(lanterns, vec![((12, 5), LanternStyle::Ghost)], "the standing one, in its style");
    }

    #[test]
    fn test_destroyed_scenery_stays_destroyed() {
        use crate::components::{GlowMushroom, OilBarrel};

        let (mut world, grid, player) = setup();

        // Spawn then destroy: an exploded barrel and burnt fungus are simply
        // gone from the world, and must not be resurrected by a revisit.
        let barrel = spawning::spawn_oil_barrel(&mut world, 12, 5);
        let mushroom = spawning::spawn_glow_mushrooms(&mut world, 14, 5);
        world.despawn(barrel).unwrap();
        crate::systems::fire::ignite_fuel(&mut world, mushroom);
        assert!(!world.contains(mushroom), "fungus is consumed when it catches");

        save_and_reload(&mut world, grid, player);

        assert_eq!(world.query::<&OilBarrel>().iter().count(), 0);
        assert_eq!(world.query::<&GlowMushroom>().iter().count(), 0);
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
}
