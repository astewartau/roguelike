//! Shoving light furniture: the universal Push action and the object half of
//! the Fighter's Shield Bash.
//!
//! A `Pushable` that also `BlocksMovement` (oil barrels, unlooted storage
//! barrels) can be moved one tile at a time in a straight line. The pusher
//! never moves. Where an object may land is decided in one place,
//! [`object_can_enter`], so Push, Shield Bash and the right-click context menu
//! (via [`can_push`]) agree.
//!
//! # Where a pushed object may go
//!
//! - a walkable tile that nothing blocks (spatial cache), with no creature
//!   (anything `Attackable`, flyers included) and no door, open or closed (a
//!   barrel in a doorway would jam it), and not a staircase;
//! - an object that cannot be destroyed (no `Health`: storage barrels) must
//!   also stay inside a room. Corridors are one tile wide, so a shove down one
//!   can wedge an indestructible barrel at a bend where nobody can get behind
//!   it to push it back out, cutting the level in two. An oil barrel can always
//!   be blown up, so it may be shoved anywhere.
//!
//! # What landing does to the object
//!
//! Kept deliberately small ([`on_object_enter_tile`]): a combustible object
//! shoved onto fire (`CausesBurning`) catches (refused while it is soaked), and
//! one shoved into water is soaked. Traps, webs and discovery are for
//! creatures and do not fire.

use hecs::{Entity, World};
use rand::Rng;

use crate::components::{
    Attackable, BlocksMovement, CausesBurning, Combustible, Door, EffectType, Health, Position,
    Pushable,
};
use crate::constants::*;
use crate::engine::EffectCtx;
use crate::events::{EventQueue, GameEvent};
use crate::grid::Grid;
use crate::queries;
use crate::spatial_cache::SpatialCache;
use crate::systems::actions::ActionResult;
use crate::tile::TileType;

/// The pushable object standing on `(x, y)`, if any. Only objects that still
/// block movement count: a looted storage barrel is walkable scenery.
pub fn pushable_at(world: &World, x: i32, y: i32) -> Option<Entity> {
    world
        .query::<(&Position, &Pushable, &BlocksMovement)>()
        .iter()
        .find(|(_, (p, _, _))| p.x == x && p.y == y)
        .map(|(id, _)| id)
}

/// Whether `object` could be moved onto `dest` (see the module docs for the
/// rules). `object` itself is ignored when checking for blockers.
pub fn object_can_enter(
    world: &World,
    grid: &Grid,
    spatial: &SpatialCache,
    object: Entity,
    dest: (i32, i32),
) -> bool {
    let (x, y) = dest;
    let Some(tile) = grid.get(x, y) else {
        return false;
    };
    if !tile.tile_type.is_walkable()
        || matches!(tile.tile_type, TileType::StairsDown | TileType::StairsUp)
    {
        return false;
    }
    if queries::is_position_blocked(spatial, x, y, Some(object)) {
        return false;
    }
    if queries::get_attackable_at(world, x, y, Some(object)).is_some() {
        return false;
    }
    let door_here = grid.door_positions.iter().any(|(p, _)| *p == (x, y))
        || world.query::<(&Position, &Door)>().iter().any(|(_, (p, _))| p.x == x && p.y == y);
    if door_here {
        return false;
    }
    // Indestructible objects stay in rooms (see the module docs).
    let destructible = world.get::<&Health>(object).is_ok();
    if !destructible && !grid.themed_rooms.iter().any(|r| r.rect.contains(x, y)) {
        return false;
    }
    true
}

/// Whether `pusher` could push the object on `target_tile` right now: the
/// tile is adjacent (8-way), holds a pushable, and the tile beyond it in the
/// same direction will take it. For the context menu and click validation.
pub fn can_push(
    world: &World,
    grid: &Grid,
    spatial: &SpatialCache,
    pusher: Entity,
    target_tile: (i32, i32),
) -> bool {
    let Some(from) = queries::get_entity_position(world, pusher) else {
        return false;
    };
    let (dx, dy) = (target_tile.0 - from.0, target_tile.1 - from.1);
    if (dx, dy) == (0, 0) || dx.abs() > 1 || dy.abs() > 1 {
        return false;
    }
    let Some(object) = pushable_at(world, target_tile.0, target_tile.1) else {
        return false;
    };
    object_can_enter(world, grid, spatial, object, (target_tile.0 + dx, target_tile.1 + dy))
}

/// Move `object` one tile along `(dx, dy)` if it can go there. Updates the
/// spatial cache, emits `EntityMoved` + `ObjectPushed`, and applies
/// [`on_object_enter_tile`]. `VisualPosition` is left alone, so the sprite
/// slides over via the normal visual lerp. Returns whether it moved.
#[allow(clippy::too_many_arguments)]
pub fn shove_object(
    world: &mut World,
    grid: &Grid,
    spatial: &mut SpatialCache,
    pusher: Entity,
    object: Entity,
    (dx, dy): (i32, i32),
    events: &mut EventQueue,
    rng: &mut impl Rng,
) -> bool {
    let Some(from) = queries::get_entity_position(world, object) else {
        return false;
    };
    let to = (from.0 + dx, from.1 + dy);
    if !object_can_enter(world, grid, spatial, object, to) {
        return false;
    }
    if let Ok(mut pos) = world.get::<&mut Position>(object) {
        pos.x = to.0;
        pos.y = to.1;
    } else {
        return false;
    }
    spatial.update_position(object, from, to);
    events.push(GameEvent::EntityMoved { entity: object, from, to });
    events.push(GameEvent::ObjectPushed { pusher, object, from, to });
    on_object_enter_tile(world, grid, object, to, events, rng);
    true
}

/// What a tile does to a shoved object (see the module docs): fire lights a
/// combustible one, water soaks it.
pub fn on_object_enter_tile(
    world: &mut World,
    grid: &Grid,
    object: Entity,
    (x, y): (i32, i32),
    events: &mut EventQueue,
    _rng: &mut impl Rng,
) {
    if grid.water_positions.contains(&(x, y)) {
        let duration = if world.get::<&crate::components::OilBarrel>(object).is_ok() {
            BARREL_SOAK_DURATION
        } else {
            WET_DURATION
        };
        crate::systems::effects::add_effect_to_entity(world, object, EffectType::Wet, duration);
        return;
    }
    let on_fire = world
        .query::<(&Position, &CausesBurning)>()
        .iter()
        .any(|(id, (p, _))| id != object && p.x == x && p.y == y);
    if on_fire
        && world.get::<&Combustible>(object).is_ok()
        && crate::systems::effects::add_effect_to_entity(
            world,
            object,
            EffectType::Burning,
            BURNING_DURATION,
        )
    {
        events.push(GameEvent::CaughtFire { entity: object, position: (x, y) });
    }
}

/// The Push action completes: shove the pushable on the adjacent tile
/// `(dx, dy)` one tile further. "It won't budge." (`PushBlocked`) when there
/// is nothing pushable there or its way is blocked; the pusher never moves.
pub fn apply_push(ctx: &mut EffectCtx, pusher: Entity, dx: i32, dy: i32) -> ActionResult {
    let (dx, dy) = (dx.signum(), dy.signum());
    let Some(from) = queries::get_entity_position(ctx.world, pusher) else {
        return ActionResult::Invalid;
    };
    if (dx, dy) == (0, 0) {
        return ActionResult::Invalid;
    }
    let target = (from.0 + dx, from.1 + dy);
    let moved = match pushable_at(ctx.world, target.0, target.1) {
        Some(object) => shove_object(
            ctx.world, ctx.grid, ctx.spatial, pusher, object, (dx, dy), ctx.events, ctx.rng,
        ),
        None => false,
    };
    if !moved {
        ctx.events.push(GameEvent::PushBlocked { pusher });
        return ActionResult::Blocked;
    }
    ActionResult::Completed
}

/// Whether anything is `Attackable` or `Pushable` on `tile` (Shield Bash's
/// target test; the bash itself is in `systems::actions::kit`).
pub fn has_bash_target(world: &World, basher: Entity, tile: (i32, i32)) -> bool {
    pushable_at(world, tile.0, tile.1).is_some()
        || world
            .query::<(&Position, &Attackable)>()
            .iter()
            .any(|(id, (p, _))| id != basher && (p.x, p.y) == tile)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::components::{ActionType, OilBarrel};
    use crate::systems::actions::TestArena as Arena;

    fn barrel(arena: &mut Arena, x: i32, y: i32) -> Entity {
        let b = crate::spawning::spawn_oil_barrel(&mut arena.world, x, y);
        arena.cache.rebuild_in_place(&arena.world);
        b
    }

    fn pushed_blocked(arena: &Arena) -> bool {
        arena.seen.iter().any(|e| matches!(e, GameEvent::PushBlocked { .. }))
    }

    #[test]
    fn push_moves_a_barrel_one_tile_and_the_player_stays() {
        let mut arena = Arena::new((5, 5));
        let b = barrel(&mut arena, 6, 5);
        assert!(can_push(&arena.world, &arena.grid, &arena.cache, arena.player, (6, 5)));

        arena.player_does(ActionType::Push { dx: 1, dy: 0 });
        assert_eq!(arena.pos(b), (7, 5), "the barrel slid one tile");
        assert_eq!(arena.pos(arena.player), (5, 5), "the pusher stays put");
        assert!(arena.cache.is_blocked((7, 5)), "the cache follows the barrel");
        assert!(!arena.cache.is_blocked((6, 5)), "and frees its old tile");
        assert!((arena.clock.time - ACTION_PUSH_DURATION).abs() < 1e-4);
        assert!(arena.seen.iter().any(|e| matches!(e,
            GameEvent::ObjectPushed { object, to: (7, 5), .. } if *object == b)));
    }

    #[test]
    fn push_refuses_walls_creatures_and_closed_doors() {
        // Wall.
        let mut arena = Arena::new((5, 5));
        let b = barrel(&mut arena, 6, 5);
        arena.grid.tiles[5 * 16 + 7] = crate::tile::Tile::new(TileType::Wall);
        assert!(!can_push(&arena.world, &arena.grid, &arena.cache, arena.player, (6, 5)));
        arena.player_does(ActionType::Push { dx: 1, dy: 0 });
        assert_eq!(arena.pos(b), (6, 5));
        assert!(pushed_blocked(&arena));
        let mut log = crate::ui::MessageLog::new(arena.player);
        for ev in &arena.seen {
            log.record_event(ev, &arena.world);
        }
        assert!(log.lines().iter().any(|l| l == "It won't budge."), "{:?}", log.lines());

        // Creature.
        let mut arena = Arena::new((5, 5));
        let b = barrel(&mut arena, 6, 5);
        arena.rat(7, 5, 1.0);
        arena.player_does(ActionType::Push { dx: 1, dy: 0 });
        assert_eq!(arena.pos(b), (6, 5));
        assert!(pushed_blocked(&arena));

        // Closed door.
        let mut arena = Arena::new((5, 5));
        let b = barrel(&mut arena, 6, 5);
        let pos = Position::new(7, 5);
        arena.world.spawn((pos, Door::new(), BlocksMovement, crate::components::BlocksVision));
        arena.cache.rebuild_in_place(&arena.world);
        arena.player_does(ActionType::Push { dx: 1, dy: 0 });
        assert_eq!(arena.pos(b), (6, 5));
        assert!(pushed_blocked(&arena));
        assert_eq!(arena.pos(arena.player), (5, 5));
    }

    /// A storage barrel cannot be destroyed, so it is not shoved out of its
    /// room into a corridor (where it could wedge and cut the level).
    #[test]
    fn an_indestructible_barrel_stays_in_its_room() {
        let mut arena = Arena::new((3, 5));
        arena.grid.themed_rooms.push(crate::dungeon_gen::ThemedRoom {
            rect: crate::dungeon_gen::Rect::new(2, 2, 4, 6),
            theme: crate::dungeon_gen::RoomTheme::Storage,
        });
        let pos = Position::new(4, 5);
        let food = arena.world.spawn((
            pos,
            crate::components::Container::barrel(vec![]),
            BlocksMovement,
            Pushable,
        ));
        arena.cache.rebuild_in_place(&arena.world);
        arena.player_does(ActionType::Push { dx: 1, dy: 0 });
        assert_eq!(arena.pos(food), (5, 5), "inside the room it moves");
        arena.player_does(ActionType::Move { dx: 1, dy: 0, is_diagonal: false });
        arena.player_does(ActionType::Push { dx: 1, dy: 0 });
        assert_eq!(arena.pos(food), (5, 5), "but not out of it");
        assert!(pushed_blocked(&arena));
    }

    #[test]
    fn a_barrel_pushed_onto_fire_catches_unless_soaked() {
        let mut arena = Arena::new((5, 5));
        let b = barrel(&mut arena, 6, 5);
        arena.world.spawn((Position::new(7, 5), CausesBurning));
        arena.player_does(ActionType::Push { dx: 1, dy: 0 });
        assert!(crate::queries::has_status_effect(&arena.world, b, EffectType::Burning));

        let mut arena = Arena::new((5, 5));
        let b = barrel(&mut arena, 6, 5);
        crate::systems::effects::add_effect_to_entity(
            &mut arena.world, b, EffectType::Wet, BARREL_SOAK_DURATION,
        );
        arena.world.spawn((Position::new(7, 5), CausesBurning));
        arena.player_does(ActionType::Push { dx: 1, dy: 0 });
        assert_eq!(arena.pos(b), (7, 5));
        assert!(!crate::queries::has_status_effect(&arena.world, b, EffectType::Burning));
        assert!(arena.world.get::<&OilBarrel>(b).is_ok());
    }
}
