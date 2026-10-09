//! Movement, doors, stairs, and directional interaction.

use crate::engine::EffectCtx;
use hecs::{Entity, World};

use crate::components::{BlocksMovement, Container, Door, Player, Position};
use crate::events::{EventQueue, GameEvent, StairDirection};
use crate::queries;
use crate::spatial_cache::SpatialCache;

use super::{apply_open_chest, interrupt_raise_dead, interrupt_taming, ActionResult};

/// Apply movement effect
pub fn apply_move(ctx: &mut EffectCtx, entity: Entity, dx: i32, dy: i32) -> ActionResult {
    let EffectCtx { world, grid, spatial: spatial_cache, events, rng } = ctx;
    let (world, grid) = (&mut **world, &mut **grid);
    let (spatial_cache, events, rng) = (&mut **spatial_cache, &mut **events, &mut **rng);

    // Moving breaks any taming channel - the druid must stand still to tame.
    interrupt_taming(world, entity, events);
    // Likewise, moving breaks a Raise Dead channel.
    interrupt_raise_dead(world, entity, events);

    // Get current position
    let current_pos = match queries::get_entity_position(world, entity) {
        Some(p) => p,
        None => return ActionResult::Invalid,
    };

    let target_x = current_pos.0 + dx;
    let target_y = current_pos.1 + dy;

    // Check tile walkability
    if !grid.is_walkable(target_x, target_y) {
        return ActionResult::Blocked;
    }

    // Check for attackable entity at target - block movement unless it's our own companion
    let mut passing_through_own_companion = false;
    if let Some(target_entity) = queries::get_attackable_at(world, target_x, target_y, Some(entity)) {
        // Allow walking through our own tamed companions
        let is_our_companion = world
            .get::<&crate::components::TamedBy>(target_entity)
            .map(|t| t.owner == entity)
            .unwrap_or(false);
        if !is_our_companion {
            return ActionResult::Blocked;
        }
        passing_through_own_companion = true;
    }

    // Check for closed door at target
    let mut door_to_open: Option<Entity> = None;
    for (id, (door_pos, door)) in world.query::<(&Position, &Door)>().iter() {
        if door_pos.x == target_x && door_pos.y == target_y && !door.is_open {
            door_to_open = Some(id);
            break;
        }
    }
    if let Some(door_id) = door_to_open {
        // Only the player and door-capable enemies can open doors; dumb enemies
        // are stopped by a closed door.
        let can_open = world.get::<&Player>(entity).is_ok()
            || world.get::<&crate::components::CanOpenDoors>(entity).is_ok();
        if can_open {
            return apply_open_door(world, entity, door_id, events);
        }
        return ActionResult::Blocked;
    }

    // Check for container (chest) at target
    let mut chest_action: Option<(Entity, bool, bool)> = None;
    for (id, (chest_pos, container, _)) in
        world.query::<(&Position, &Container, &BlocksMovement)>().iter()
    {
        if chest_pos.x == target_x && chest_pos.y == target_y {
            chest_action = Some((id, container.is_open, container.is_empty()));
            break;
        }
    }
    if let Some((chest_id, is_open, is_empty)) = chest_action {
        if !is_open || !is_empty {
            return apply_open_chest(world, entity, chest_id, events);
        }
    }

    // Check for room furniture (fountain/altar/shrine) at target — bumping
    // into it interacts, but only for the player.
    if world.get::<&Player>(entity).is_ok() {
        let furniture: Option<Entity> = world
            .query::<(&Position, &crate::components::Furniture)>()
            .iter()
            .find(|(_, (p, _))| p.x == target_x && p.y == target_y)
            .map(|(id, _)| id);
        if let Some(furniture_id) = furniture {
            return crate::systems::furniture::use_furniture(world, entity, furniture_id, events, rng);
        }
    }

    // Check for any other blocking entity. A raised skeleton keeps
    // BlocksMovement so it screens enemies, so the cache reports its tile as
    // blocked — but its owner walks through it, same as any other companion.
    // Without this exception a Necromancer trailing skeletons at follow
    // distance 2 would wall itself into any corridor it backed down.
    if !passing_through_own_companion
        && queries::is_position_blocked(spatial_cache, target_x, target_y, Some(entity))
    {
        return ActionResult::Blocked;
    }

    // Execute the move
    if let Ok(mut pos) = world.get::<&mut Position>(entity) {
        let from = (pos.x, pos.y);
        pos.x = target_x;
        pos.y = target_y;
        spatial_cache.update_position(entity, from, (target_x, target_y));
        events.push(GameEvent::EntityMoved {
            entity,
            from,
            to: (target_x, target_y),
        });
    }

    // Everything the destination tile does to an arrival (water, oil, fire,
    // webs, traps, discovery) lives in one hook shared by every relocation.
    crate::systems::tile_effects::on_enter_tile(
        world,
        grid,
        entity,
        (target_x, target_y),
        events,
        rng,
    );

    ActionResult::Completed
}

/// Apply open door effect
pub fn apply_open_door(
    world: &mut World,
    opener: Entity,
    door: Entity,
    events: &mut EventQueue,
) -> ActionResult {
    if let Ok(mut door_comp) = world.get::<&mut Door>(door) {
        door_comp.is_open = true;
    }

    // Remove blocks movement/vision when door opens
    let _ = world.remove_one::<BlocksMovement>(door);
    let _ = world.remove_one::<crate::components::BlocksVision>(door);

    let position = world
        .get::<&Position>(door)
        .map(|p| (p.x, p.y))
        .unwrap_or((0, 0));
    events.push(GameEvent::DoorOpened { door, opener, position });

    ActionResult::Completed
}

/// Apply close door effect
pub fn apply_close_door(
    world: &mut World,
    closer: Entity,
    door: Entity,
    events: &mut EventQueue,
) -> ActionResult {
    // Verify door exists and is open
    let is_open = if let Ok(door_comp) = world.get::<&Door>(door) {
        door_comp.is_open
    } else {
        return ActionResult::Blocked;
    };

    if !is_open {
        return ActionResult::Blocked;
    }

    // Check nobody is standing on the door tile
    let door_pos = match world.get::<&Position>(door) {
        Ok(p) => (p.x, p.y),
        Err(_) => return ActionResult::Blocked,
    };

    // Query for any entity with Position + BlocksMovement at the door's position (exclude the door itself)
    for (id, (pos, _)) in world.query::<(&Position, &BlocksMovement)>().iter() {
        if id != door && pos.x == door_pos.0 && pos.y == door_pos.1 {
            return ActionResult::Blocked;
        }
    }

    // Close the door
    if let Ok(mut door_comp) = world.get::<&mut Door>(door) {
        door_comp.is_open = false;
    }

    // Add BlocksMovement and BlocksVision components back
    let _ = world.insert_one(door, BlocksMovement);
    let _ = world.insert_one(door, crate::components::BlocksVision);

    events.push(GameEvent::DoorClosed {
        door,
        closer,
        position: door_pos,
    });

    ActionResult::Completed
}

/// Apply interact in a direction (Ctrl+movement).
/// Checks for doors (open/close), toppleable braziers, and containers at the
/// target tile.
pub fn apply_interact_direction(
    ctx: &mut EffectCtx,
    entity: Entity,
    dx: i32,
    dy: i32,
) -> ActionResult {
    let EffectCtx { world, grid, spatial: _spatial_cache, events, rng } = ctx;
    let (world, grid) = (&mut **world, &mut **grid);
    let (_spatial_cache, events, rng) = (&mut **_spatial_cache, &mut **events, &mut **rng);

    // Get entity position
    let pos = match world.get::<&Position>(entity) {
        Ok(p) => (p.x, p.y),
        Err(_) => return ActionResult::Invalid,
    };

    let target_x = pos.0 + dx;
    let target_y = pos.1 + dy;

    // Check for door at target
    let door_info: Option<(hecs::Entity, bool)> = world
        .query::<(&Position, &Door)>()
        .iter()
        .find(|(_, (p, _))| p.x == target_x && p.y == target_y)
        .map(|(id, (_, door))| (id, door.is_open));

    if let Some((door_id, is_open)) = door_info {
        if is_open {
            return apply_close_door(world, entity, door_id, events);
        } else {
            return apply_open_door(world, entity, door_id, events);
        }
    }

    // Check for a lit brazier at target — interacting topples it, spilling
    // fire onto its tile and one adjacent tile (see systems::fire).
    let brazier: Option<hecs::Entity> = world
        .query::<(&Position, &crate::components::Brazier)>()
        .iter()
        .find(|(_, (p, b))| b.lit && p.x == target_x && p.y == target_y)
        .map(|(id, _)| id);
    if let Some(brazier_id) = brazier {
        if crate::systems::fire::topple_brazier(world, grid, brazier_id, events, rng) {
            return ActionResult::Completed;
        }
    }

    // Check for room furniture (fountain/altar/shrine) at target
    let furniture: Option<hecs::Entity> = world
        .query::<(&Position, &crate::components::Furniture)>()
        .iter()
        .find(|(_, (p, _))| p.x == target_x && p.y == target_y)
        .map(|(id, _)| id);
    if let Some(furniture_id) = furniture {
        if world.get::<&Player>(entity).is_ok() {
            return crate::systems::furniture::use_furniture(world, entity, furniture_id, events, rng);
        }
    }

    // Check for closed or non-empty container at target
    let container_id: Option<hecs::Entity> = world
        .query::<(&Position, &Container)>()
        .iter()
        .find(|(_, (p, container))| {
            p.x == target_x && p.y == target_y && (!container.is_open || !container.is_empty())
        })
        .map(|(id, _)| id);

    if let Some(chest_id) = container_id {
        return apply_open_chest(world, entity, chest_id, events);
    }

    ActionResult::Blocked
}

/// Apply use stairs effect - moves entity to stairs and emits floor transition event
///
/// Takes the `SpatialCache` because this is a real move, not just a transition
/// trigger: the entity ends up standing on the stairs tile. A floor transition
/// rebuilds the cache from scratch and would paper over a missed update, but
/// the transition can be *refused* — `can_transition_floor` turns down
/// `StairDirection::Up` on floor 0, so walking onto the dungeon entrance
/// leaves the entity on the stairs with no rebuild coming. Skipping the cache
/// here left it pointing at the tile the entity had just left, which then
/// unblocked the wrong tile on the next step.
pub fn apply_use_stairs(
    world: &mut World,
    spatial_cache: &mut SpatialCache,
    entity: Entity,
    x: i32,
    y: i32,
    direction: StairDirection,
    events: &mut EventQueue,
) -> ActionResult {
    // Get current position
    let current_pos = match queries::get_entity_position(world, entity) {
        Some(p) => p,
        None => return ActionResult::Invalid,
    };

    // Move entity to the stairs position
    if let Ok(mut pos) = world.get::<&mut Position>(entity) {
        pos.x = x;
        pos.y = y;
    }
    spatial_cache.update_position(entity, current_pos, (x, y));

    // Emit movement event
    events.push(GameEvent::EntityMoved {
        entity,
        from: current_pos,
        to: (x, y),
    });

    // Emit floor transition event
    events.push(GameEvent::FloorTransition {
        direction,
        from_floor: 0,
    });

    ActionResult::Completed
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::components::{Attackable, CompanionAI, TamedBy, VisualPosition};
    use rand::SeedableRng;

    // =========================================================================
    // The on-enter-tile hook runs for every way of arriving on a tile.
    // =========================================================================

    use crate::components::{ActionType, EffectType};
    use crate::systems::actions::combat::tests::Arena;

    fn has(arena: &Arena, effect: EffectType) -> bool {
        crate::queries::has_status_effect(&arena.world, arena.player, effect)
    }

    fn set_burning(arena: &mut Arena) {
        crate::systems::effects::add_effect_to_entity(
            &mut arena.world,
            arena.player,
            EffectType::Burning,
            crate::constants::BURNING_DURATION,
        );
    }

    /// A normal step into water still puts the fire out (as before the hook
    /// existed), and now also leaves the walker Wet.
    #[test]
    fn stepping_into_water_soaks_and_extinguishes() {
        let mut arena = Arena::new((5, 5));
        arena.grid.water_positions.push((6, 5));
        set_burning(&mut arena);

        arena.player_does(ActionType::Move { dx: 1, dy: 0, is_diagonal: false });
        assert_eq!(arena.pos(arena.player), (6, 5));
        assert!(!has(&arena, EffectType::Burning), "water puts the fire out");
        assert!(has(&arena, EffectType::Wet), "and soaks the walker");
        assert!(arena.seen.iter().any(|e| matches!(e,
            GameEvent::StatusEffectGained { effect: EffectType::Wet, .. })));
    }

    /// Blinking into a pool behaves exactly like walking into it: before the
    /// hook, a teleport left you burning in the water.
    #[test]
    fn blinking_into_water_soaks_and_extinguishes() {
        let mut arena = Arena::new((3, 3));
        arena.grid.water_positions.push((7, 3));
        set_burning(&mut arena);

        arena.player_does(ActionType::Blink { target_x: 7, target_y: 3 });
        assert_eq!(arena.pos(arena.player), (7, 3), "the blink landed");
        assert!(!has(&arena, EffectType::Burning));
        assert!(has(&arena, EffectType::Wet));
    }

    /// Tumble's landing tile acts on the ranger: rolling into an unlit oil
    /// puddle leaves them Oiled.
    #[test]
    fn tumbling_into_oil_leaves_you_oiled() {
        let mut arena = Arena::new((3, 3));
        crate::spawning::spawn_oil_puddle(&mut arena.world, 5, 3);

        arena.player_does(ActionType::Tumble { target_x: 5, target_y: 3 });
        assert_eq!(arena.pos(arena.player), (5, 3));
        assert!(has(&arena, EffectType::Oiled));
        assert!(crate::queries::is_slippery(&arena.world, arena.player), "oiled means slippery");
    }

    /// A burning puddle sets you alight instead of oiling you.
    #[test]
    fn stepping_into_burning_oil_ignites_rather_than_oils() {
        let mut arena = Arena::new((5, 5));
        let puddle = crate::spawning::spawn_oil_puddle(&mut arena.world, 6, 5);
        crate::systems::fire::ignite_oil_puddle(&mut arena.world, puddle);

        arena.player_does(ActionType::Move { dx: 1, dy: 0, is_diagonal: false });
        assert!(has(&arena, EffectType::Burning));
        assert!(!has(&arena, EffectType::Oiled));
    }

    /// A Wet walker steps into a fire source and does not catch — and is not
    /// told it did.
    #[test]
    fn a_wet_walker_does_not_catch_fire() {
        let mut arena = Arena::new((5, 5));
        let puddle = crate::spawning::spawn_oil_puddle(&mut arena.world, 6, 5);
        crate::systems::fire::ignite_oil_puddle(&mut arena.world, puddle);
        crate::systems::effects::add_effect_to_entity(
            &mut arena.world,
            arena.player,
            EffectType::Wet,
            crate::constants::WET_DURATION,
        );

        arena.player_does(ActionType::Move { dx: 1, dy: 0, is_diagonal: false });
        assert_eq!(arena.pos(arena.player), (6, 5));
        assert!(!has(&arena, EffectType::Burning));
        assert!(!arena.seen.iter().any(|e| matches!(e, GameEvent::CaughtFire { .. })));
    }

    /// Walking onto a staircase is a real move, and the entity can end up
    /// standing there: `can_transition_floor` refuses `Up` on floor 0, so
    /// stepping onto the dungeon entrance moves the player and then *declines*
    /// the transition that would otherwise have rebuilt the cache.
    ///
    /// Before the cache update below, the player's cached tile stayed behind at
    /// the tile they had left. The next step then released a count the player
    /// never held there, while the tile they were actually standing on read as
    /// walkable.
    #[test]
    fn using_stairs_moves_the_entity_in_the_spatial_cache() {
        let mut world = World::new();
        let start = Position::new(6, 9);
        let player = world.spawn((
            start,
            VisualPosition::from_position(&start),
            BlocksMovement,
            Player,
        ));
        let mut cache = SpatialCache::rebuild_from_world(&world);
        let mut events = EventQueue::new();

        let result = apply_use_stairs(
            &mut world,
            &mut cache,
            player,
            7,
            9,
            StairDirection::Up,
            &mut events,
        );
        assert_eq!(result, ActionResult::Completed);

        assert!(
            !cache.is_blocked((6, 9)),
            "the tile the player left must not keep a phantom blocker"
        );
        assert!(
            cache.is_blocked((7, 9)),
            "the stairs tile the player now occupies must be blocked"
        );
        cache.assert_coherent_with_world(&world, "after stepping onto stairs");

        // The real symptom: the *next* step used to release a count the player
        // had never taken at its cached tile.
        if let Ok(mut pos) = world.get::<&mut Position>(player) {
            pos.y = 10;
        }
        cache.update_position(player, (7, 9), (7, 10));
        cache.assert_coherent_with_world(&world, "after stepping off the stairs");
    }

    /// A raised skeleton keeps `BlocksMovement` so it screens enemies, which
    /// means the SpatialCache reports its tile as blocked. Its owner still has
    /// to be able to walk through it — a Necromancer trailing skeletons at
    /// follow distance 2 would otherwise wall itself into any corridor it
    /// backed down.
    #[test]
    fn owner_walks_through_a_blocking_companion_but_a_stranger_does_not() {
        let mut rng = rand::rngs::StdRng::seed_from_u64(7);
        let mut grid = crate::grid::Grid::new_floor(20, 20, 0, &mut rng);
        // Carve a clear strip so walkability isn't what's under test.
        for x in 1..6 {
            if let Some(tile) = grid.get_mut(x, 1) {
                tile.tile_type = crate::tile::TileType::Floor;
            }
        }

        let mut world = World::new();
        let owner_pos = Position::new(1, 1);
        let owner = world.spawn((owner_pos, VisualPosition::from_position(&owner_pos)));
        let comp_pos = Position::new(2, 1);
        let companion = world.spawn((
            comp_pos,
            VisualPosition::from_position(&comp_pos),
            BlocksMovement,
            Attackable,
            TamedBy { owner },
            CompanionAI { owner, follow_distance: 2, threat_table: Vec::new() },
        ));
        let stranger_pos = Position::new(3, 1);
        let stranger = world.spawn((stranger_pos, VisualPosition::from_position(&stranger_pos)));

        let mut cache = SpatialCache::rebuild_from_world(&world);
        assert!(cache.is_blocked((2, 1)), "companion blocks its tile");

        let mut events = crate::events::EventQueue::new();
        let result = apply_move(
            &mut EffectCtx {
                world: &mut world,
                grid: &mut grid,
                spatial: &mut cache,
                events: &mut events,
                rng: &mut rng,
            },
            owner,
            1,
            0,
        );
        assert_eq!(result, ActionResult::Completed, "the owner walks through its own companion");
        let moved = world.get::<&Position>(owner).map(|p| (p.x, p.y)).unwrap();
        assert_eq!(moved, (2, 1));

        // Someone else is still stopped by it — that is the whole point.
        let result = apply_move(
            &mut EffectCtx {
                world: &mut world,
                grid: &mut grid,
                spatial: &mut cache,
                events: &mut events,
                rng: &mut rng,
            },
            stranger,
            -1,
            0,
        );
        assert_eq!(result, ActionResult::Blocked, "a non-owner is screened by the companion");
        let _ = companion;
    }
}
