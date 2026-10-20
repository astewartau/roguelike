//! Floor transition and save/load logic for multi-floor dungeons.

use crate::component_registry::{self, Snapshot};
use crate::components::{Position, VisualPosition};
use crate::constants::*;
use crate::grid::Grid;
use crate::time_system::ActionScheduler;

use hecs::{Entity, World};
use std::collections::HashMap;

use super::initialization::spawn_floor_entities;
use super::ActorCtx;

/// Saved state of a floor for when the player leaves and returns.
pub struct SavedFloor {
    pub grid: Grid,
    /// Everything on the floor but the player, each as its components
    /// (`component_registry::snapshot`), keyed by the entity it was so links
    /// between them survive the reload.
    pub entities: Vec<(Entity, Snapshot)>,
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

/// Save the current floor: every entity on it but the player, as it is now,
/// component for component. Nothing here knows what kinds of thing a floor
/// holds; anything the component registry can read comes back on a revisit
/// exactly as it was left.
///
/// An entity carrying a component the registry can't save (a projectile in
/// flight) can't come back as it was, so it isn't saved at all, rather than
/// coming back as a stray sprite. Passing visual state (a hit flash, a
/// lunge) doesn't count against it: it's simply dropped.
pub fn save_floor(world: &World, grid: Grid, player_entity: Entity) -> SavedFloor {
    let passing = passing_component_types();
    let entities = world
        .iter()
        .map(|e| e.entity())
        .filter(|&id| id != player_entity)
        .filter(|&id| {
            let (known, unknown) = component_registry::components_of(world, id);
            let passing_here = world
                .entity(id)
                .map(|e| e.component_types().filter(|t| passing.contains(t)).count())
                .unwrap_or(0);
            !known.is_empty() && unknown == passing_here
        })
        .map(|id| (id, component_registry::snapshot(world, id)))
        .collect();
    SavedFloor { grid, entities }
}

/// Components that are only a moment's presentation, never worth keeping
/// when the player leaves a floor.
fn passing_component_types() -> [std::any::TypeId; 3] {
    use crate::components::{HitFlash, LastDamageSource, LungeAnimation};
    use std::any::TypeId;
    [TypeId::of::<HitFlash>(), TypeId::of::<LungeAnimation>(), TypeId::of::<LastDamageSource>()]
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

/// Bring a saved floor back: every entity as it was left, and the player at
/// `player_spawn_pos`. The caller rebuilds the caches and schedules the
/// creatures.
pub fn load_floor(ctx: &mut ActorCtx, saved: &[(Entity, Snapshot)], player_spawn_pos: (i32, i32)) {
    let player_entity = ctx.player;
    if let Ok(mut pos) = ctx.world.get::<&mut Position>(player_entity) {
        pos.x = player_spawn_pos.0;
        pos.y = player_spawn_pos.1;
    }
    if let Ok(mut vis_pos) = ctx.world.get::<&mut VisualPosition>(player_entity) {
        vis_pos.x = player_spawn_pos.0 as f32;
        vis_pos.y = player_spawn_pos.1 as f32;
    }
    component_registry::restore(ctx.world, saved);
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
    transition_to_floor(
        ctx,
        floors,
        current_floor,
        target_floor,
        super::game_state::floor_seed(run_seed, target_floor),
        direction,
    )
}

/// Move the player to `target_floor` (restoring it if it was visited, else
/// generating it from `floor_seed`), arriving at its up stairs when going
/// `Down` and its down stairs when going `Up`. Stairs only ever go one floor;
/// the developer tools jump anywhere, and pass the current floor itself with
/// a fresh seed to reroll it (the floor being left is then dropped, not
/// saved).
pub fn transition_to_floor(
    ctx: &mut ActorCtx,
    floors: &mut HashMap<u32, SavedFloor>,
    current_floor: u32,
    target_floor: u32,
    floor_seed: u64,
    direction: crate::events::StairDirection,
) -> FloorTransitionResult {
    use crate::events::StairDirection;
    use rand::rngs::StdRng;
    use rand::SeedableRng;

    let player_entity = ctx.player;
    let mut floor_rng = StdRng::seed_from_u64(floor_seed);

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
    if target_floor != current_floor {
        floors.insert(current_floor, saved_floor);
    }
    clear_floor_entities(ctx.world, player_entity, ctx.scheduler);

    // Populate the new floor from the per-floor rng rather than the run rng.
    let mut floor_ctx = ActorCtx {
        rng: &mut floor_rng,
        ..ctx.reborrow()
    };
    match &saved_entities {
        Some(entities) => load_floor(&mut floor_ctx, entities, spawn_pos),
        None => spawn_floor_entities(&mut floor_ctx, spawn_pos, target_floor),
    }

    // Rebuild caches for the new floor, and only then give the enemies near
    // the player their first action. The order matters: the tracker still
    // holds the floor we just left until it is rebuilt, and an enemy the
    // rebuilt tracker already counts as active is never reported as "newly
    // active" by `update_on_player_move` — so anything not scheduled here
    // would never act (an asleep enemy would never even get to wake up).
    floor_ctx.spatial.rebuild_in_place(floor_ctx.world);
    let player_pos = floor_ctx
        .world
        .get::<&Position>(player_entity)
        .map(|p| (p.x, p.y))
        .unwrap_or((0, 0));
    floor_ctx.tracker.initialize_from_world(floor_ctx.world, player_pos);
    super::initialization::initialize_ai_actors(&mut floor_ctx);

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
    use crate::components::{
        Asleep, BlocksMovement, BlocksVision, Boss, BossAbility, ChaseAI, Door,
        FearImmune, Health, Sprite, SpriteTint,
        SupportAI, Venomous,
    };
    use crate::spawning;
    use crate::tile::tile_ids;
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
        assert!(world.get::<&crate::components::WebWalker>(spider).is_ok());
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
            Topples, CausesBurning, LightSource, Name, Explosive,
        };

        let (mut world, grid, player) = setup();
        let mut events = EventQueue::new();
        let mut rng = StdRng::seed_from_u64(3);

        // One brazier toppled through the real topple path, one left standing.
        let toppled = spawning::spawn_brazier(&mut world, 5, 5);
        assert!(crate::systems::fire::topple(
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
            .query::<(&Position, &Topples)>()
            .iter()
            .map(|(id, (p, b))| (id, (p.x, p.y), !b.toppled))
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
            .with::<&Explosive>()
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
            .query::<(&Position, &Name)>()
            .iter()
            .filter(|(_, (_, n))| n.0 == "Glowing Mushrooms")
            .map(|(_, (p, _))| (p.x, p.y))
            .collect();
        assert_eq!(mushrooms, vec![(14, 5)], "the mushroom patch comes back");
    }

    #[test]
    fn test_revisited_floor_keeps_its_lanterns() {
        use crate::components::{LanternStyle, LightSource, Shatters};

        let (mut world, grid, player) = setup();
        spawning::spawn_lantern(&mut world, 12, 5, LanternStyle::Ghost);
        let smashed = spawning::spawn_lantern(&mut world, 14, 5, LanternStyle::Warm);
        world.despawn(smashed).unwrap();

        save_and_reload(&mut world, grid, player);

        let ghost = LightSource::lantern(LanternStyle::Ghost);
        let lanterns: Vec<_> = world
            .query::<(&Position, &LightSource)>()
            .with::<&Shatters>()
            .iter()
            .map(|(_, (p, l))| ((p.x, p.y), l.color))
            .collect();
        assert_eq!(lanterns, vec![((12, 5), ghost.color)], "the standing one, in its light");
    }

    #[test]
    fn test_destroyed_scenery_stays_destroyed() {
        use crate::components::{Name, Explosive};

        let (mut world, grid, player) = setup();

        // Spawn then destroy: an exploded barrel and burnt fungus are simply
        // gone from the world, and must not be resurrected by a revisit.
        let barrel = spawning::spawn_oil_barrel(&mut world, 12, 5);
        let mushroom = spawning::spawn_glow_mushrooms(&mut world, 14, 5);
        world.despawn(barrel).unwrap();
        crate::systems::fire::ignite_fuel(&mut world, mushroom);
        assert!(!world.contains(mushroom), "fungus is consumed when it catches");

        save_and_reload(&mut world, grid, player);

        assert_eq!(world.query::<&Explosive>().iter().count(), 0);
        assert_eq!(world.query::<&Name>().iter().filter(|(_, n)| n.0 == "Glowing Mushrooms").count(), 0);
    }

    #[test]
    fn a_creature_comes_back_as_whatever_it_was() {
        let (mut world, grid, player) = setup();

        // Something put together by hand, from no template at all.
        let pos = Position::new(4, 4);
        world.spawn((
            pos,
            VisualPosition::from_position(&pos),
            crate::components::Name::new("Odd thing"),
            Health::new(9),
            ChaseAI::new(6),
            crate::components::Venomous { slow_duration: 1.0, poison_duration: 2.0 },
        ));

        save_and_reload(&mut world, grid, player);

        let odd = find_named(&world, "Odd thing");
        assert_eq!(world.get::<&Health>(odd).unwrap().current, 9);
        assert!(world.get::<&crate::components::Venomous>(odd).is_ok());
    }

    fn unlit_puddles(world: &World) -> Vec<(i32, i32)> {
        let mut v: Vec<(i32, i32)> = world
            .query::<&Position>()
            .with::<&crate::components::OilPuddle>()
            .without::<&crate::components::OnFire>()
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
        use crate::components::{BlocksMovement, Explosive, PlayerClass};
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
                let barrels = world.query::<&Explosive>().iter().count();
                save_and_reload(&mut world, grid, player);
                assert_eq!(unlit_puddles(&world), puddles, "seed {seed}: puddles survive");
                assert_eq!(
                    world.query::<&Explosive>().iter().count(),
                    barrels,
                    "seed {seed}: oil barrels survive"
                );
                checked_round_trip = true;
            }
        }
        assert!(total > 0, "twelve floors produced no oil spill at all");
        assert!(checked_round_trip);
    }

    /// A puddle burning when the player leaves is still burning on return,
    /// with as long left to burn as it had.
    #[test]
    fn burning_puddles_come_back_burning() {
        let (mut world, grid, player) = setup();
        let lit = spawning::spawn_oil_puddle(&mut world, 3, 3);
        crate::systems::fire::ignite_fuel(&mut world, lit);
        let left = world.get::<&crate::components::OnFire>(lit).unwrap().remaining;
        spawning::spawn_oil_puddle(&mut world, 4, 4);
        save_and_reload(&mut world, grid, player);
        assert_eq!(unlit_puddles(&world), vec![(4, 4)]);
        let burning: Vec<f32> = world
            .query::<&crate::components::OnFire>()
            .with::<&crate::components::OilPuddle>()
            .iter()
            .map(|(_, f)| f.remaining)
            .collect();
        assert_eq!(burning, vec![left]);
    }

    #[test]
    fn test_revisited_floor_restores_door_state_and_theme() {
        let (mut world, grid, player) = setup();

        let mut open_door = Door::shop();
        open_door.is_open = true;
        let open_pos = Position::new(4, 4);
        world.spawn((
            open_pos,
            VisualPosition::from_position(&open_pos),
            Sprite::from_ref(open_door.open_sprite),
            open_door,
        ));
        let shut_pos = Position::new(6, 4);
        world.spawn((
            shut_pos,
            VisualPosition::from_position(&shut_pos),
            Sprite::from_ref(tile_ids::DOOR_GREEN),
            Door::green(),
            SpriteTint { r: 0.5, g: 0.6, b: 0.7 },
            BlocksVision,
            BlocksMovement,
        ));

        save_and_reload(&mut world, grid, player);

        let door_at = |x: i32, y: i32| {
            world
                .query::<(&Position, &Door)>()
                .iter()
                .find(|(_, (p, _))| p.x == x && p.y == y)
                .map(|(id, _)| id)
                .expect("door restored")
        };

        // An open door comes back wearing its open sprite, and walkable.
        let open = door_at(4, 4);
        let sprite = *world.get::<&Sprite>(open).unwrap();
        assert_eq!((sprite.sheet, sprite.tile_id), tile_ids::DOOR_SHOP_OPEN);
        assert!(world.get::<&BlocksMovement>(open).is_err());
        assert!(world.get::<&BlocksVision>(open).is_err());

        // A shut door keeps its theme, its tint and its blocking.
        let shut = door_at(6, 4);
        let sprite = *world.get::<&Sprite>(shut).unwrap();
        assert_eq!((sprite.sheet, sprite.tile_id), tile_ids::DOOR_GREEN);
        assert_eq!(world.get::<&Door>(shut).unwrap().open_sprite, tile_ids::DOOR_GREEN_OPEN);
        assert!(world.get::<&SpriteTint>(shut).is_ok());
        assert!(world.get::<&BlocksMovement>(shut).is_ok());
        assert!(world.get::<&BlocksVision>(shut).is_ok());
    }

    /// Descending to a fresh floor must give every enemy the tracker counts as
    /// active a scheduled action. They used to be marked active without ever
    /// being scheduled, so they never acted — a sleeping one never woke, even
    /// when attacked.
    #[test]
    fn test_fresh_floor_schedules_enemies_near_the_stairs() {
        let mut saw_active_enemy = false;
        for seed in 0..8u64 {
            let (mut world, mut grid, player) = setup();
            let mut clock = GameClock::new();
            let mut scheduler = ActionScheduler::new();
            let mut tracker = crate::active_ai_tracker::ActiveAITracker::new();
            let mut cache = crate::spatial_cache::SpatialCache::rebuild_from_world(&world);
            let mut events = EventQueue::new();
            let mut rng = StdRng::seed_from_u64(seed);
            let mut floors = HashMap::new();

            handle_floor_transition(
                &mut ActorCtx {
                    world: &mut world,
                    grid: &mut grid,
                    player,
                    clock: &mut clock,
                    scheduler: &mut scheduler,
                    tracker: &mut tracker,
                    spatial: &mut cache,
                    events: &mut events,
                    rng: &mut rng,
                },
                &mut floors,
                1,
                seed,
                crate::events::StairDirection::Down,
            );

            let mut scheduled = std::collections::HashSet::new();
            while let Some((entity, _)) = scheduler.pop_next() {
                scheduled.insert(entity);
            }
            for (id, _) in world.query::<&ChaseAI>().iter() {
                if tracker.get_active_entities().contains(&id) {
                    saw_active_enemy = true;
                    assert!(scheduled.contains(&id), "active enemy {id:?} was never scheduled");
                }
            }
        }
        assert!(saw_active_enemy, "no seed put an enemy near the stairs; the test checked nothing");
    }

    /// Everything on every floor, after a while of play, is either saved or
    /// deliberately let go (passing visuals; a projectile in flight). A
    /// component missing from the registry would otherwise vanish, silently,
    /// the first time the player left its floor. And a save and reload of a
    /// real floor puts back exactly what was there.
    #[test]
    fn every_component_on_a_floor_is_saved() {
        use std::any::TypeId;
        let mut engine = crate::engine::GameEngine::new();
        let mut camera = crate::camera::Camera::new(800.0, 600.0);
        engine.start_game(crate::components::PlayerClass::Fighter, 77, &mut camera);
        let in_flight = [
            TypeId::of::<crate::components::Projectile>(),
            TypeId::of::<crate::components::ProjectileMarker>(),
        ];
        for floor in 0..10u32 {
            engine.dev.commands.push(crate::devtools::DevCommand::GoToFloor(floor));
            engine.dev.commands.push(crate::devtools::DevCommand::AdvanceTime(30.0));
            engine.process_ui_actions(&crate::ui::UiActions::default());
            let state = engine.state.as_mut().unwrap();
            let player = state.player_entity;
            let passing = passing_component_types();
            for e in state.world.iter().map(|e| e.entity()).filter(|&e| e != player) {
                let types: Vec<TypeId> = state.world.entity(e).unwrap().component_types().collect();
                if types.iter().any(|t| in_flight.contains(t)) {
                    continue;
                }
                let (known, unknown) = component_registry::components_of(&state.world, e);
                let passing_here = types.iter().filter(|t| passing.contains(t)).count();
                assert_eq!(
                    unknown,
                    passing_here,
                    "floor {floor}: an entity with {:?} carries a component the registry can't save",
                    known.iter().map(|k| k.name).collect::<Vec<_>>()
                );
            }

            // Round trip: everything saved comes back with every component,
            // value for value.
            let saved = save_floor(&state.world, Grid::filled(1, 1, crate::tile::TileType::Floor), player);
            let mut fresh = World::new();
            let restored = component_registry::restore(&mut fresh, &saved.entities);
            for ((_, before), e) in saved.entities.iter().zip(restored) {
                let after = component_registry::snapshot(&fresh, e);
                let names = |snap: &Snapshot| {
                    let mut n: Vec<&str> = snap.iter().map(|(n, _)| *n).collect();
                    n.sort();
                    n
                };
                assert_eq!(names(&after), names(before), "floor {floor}: the same components come back");
            }
        }
    }
}

