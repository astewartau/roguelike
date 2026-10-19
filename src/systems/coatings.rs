//! What creatures carry on them: water and oil.
//!
//! Terrain doesn't move, but creatures do. A creature that wades through water
//! comes out `Wet` and stays fireproof for a while; one that walks through an
//! oil puddle comes out `Oily`, burns far more readily, and drips oil on the
//! tiles it leaves — so an oily orc chasing you lays its own fuse. This is how
//! water and oil get carried around the map instead of only mattering on the
//! tile where they lie.
//!
//! The rules on which effects exclude which (Wet blocks Burning, the newest
//! coating replaces the other) live in `effects::add_effect_to_entity`, so
//! they hold however a coating is applied. This module is the sources, the
//! flammability maths, the drip trail, and drying off by a fire.

use hecs::{Entity, World};
use rand::Rng;

use crate::components::{
    BarrelFuse, CausesBurning, Combustible, EffectType, OilPuddle, OnFire, Position,
    StatusEffects,
};
use crate::constants::*;
use crate::events::{EventQueue, GameEvent};
use crate::grid::Grid;
use crate::systems::effects;

/// Soak an entity: Wet for `WET_DURATION`, which also puts out any Burning
/// and washes off oil. A lit barrel fuse fizzles too, and so does a carried
/// torch. Emits `Soaked` the first
/// time (not on every refresh while it stands in water).
pub fn soak(world: &mut World, entity: Entity, events: &mut EventQueue) {
    let was_wet = effects::entity_has_effect(world, entity, EffectType::Wet);
    if !effects::add_effect_to_entity(world, entity, EffectType::Wet, WET_DURATION) {
        return;
    }
    let _ = world.remove_one::<BarrelFuse>(entity);
    crate::systems::torches::douse_torch(world, entity, events);
    if !was_wet {
        events.push(GameEvent::Soaked { entity });
    }
}

/// Coat an entity in oil: Oily for `OILY_DURATION` (which also dries it off).
/// Emits `CoatedInOil` the first time.
pub fn coat_in_oil(world: &mut World, entity: Entity, events: &mut EventQueue) {
    let was_oily = effects::entity_has_effect(world, entity, EffectType::Oily);
    if !effects::add_effect_to_entity(world, entity, EffectType::Oily, OILY_DURATION) {
        return;
    }
    if !was_oily {
        events.push(GameEvent::CoatedInOil { entity });
    }
}

/// How readily an entity catches fire right now: its `Combustible`
/// flammability, zero while Wet, and multiplied up while Oily — with a floor,
/// so oil makes even bone and slime burn. Zero means it can't catch at all.
pub fn effective_flammability(world: &World, entity: Entity) -> f32 {
    let (wet, oily) = world
        .get::<&StatusEffects>(entity)
        .map(|s| {
            (
                effects::has_effect(&s, EffectType::Wet),
                effects::has_effect(&s, EffectType::Oily),
            )
        })
        .unwrap_or((false, false));
    if wet {
        return 0.0;
    }
    let base = world.get::<&Combustible>(entity).map(|c| c.flammability).unwrap_or(0.0);
    if oily {
        (base * OILY_FLAMMABILITY_MULT).clamp(OILY_MIN_FLAMMABILITY, 1.0)
    } else {
        base
    }
}

/// An oily walker just left `from`: it may drip oil there (`OILY_DRIP_CHANCE`),
/// leaving a small, short-burning puddle. Never on water, and never where oil
/// already lies.
pub fn drip_oil(
    world: &mut World,
    grid: &Grid,
    entity: Entity,
    from: (i32, i32),
    rng: &mut impl Rng,
) {
    if !effects::entity_has_effect(world, entity, EffectType::Oily) {
        return;
    }
    if !grid.is_walkable(from.0, from.1) || grid.water_positions.contains(&from) {
        return;
    }
    let oil_there = world
        .query::<(&Position, &OilPuddle)>()
        .iter()
        .any(|(_, (p, _))| (p.x, p.y) == from);
    if oil_there || !rng.gen_bool(OILY_DRIP_CHANCE) {
        return;
    }
    crate::spawning::spawn_oil_drip(world, from.0, from.1);
}

/// Wet things standing on or next to a fire dry off `WET_DRY_NEAR_FIRE_MULT`
/// times as fast. Called from the fire tick with the frame's game time; the
/// normal 1x countdown happens with every other effect, so this takes off the
/// extra.
pub fn dry_near_fire(world: &mut World, game_dt: f32) {
    let fires: Vec<(i32, i32)> = world
        .query::<(&Position, &CausesBurning)>()
        .iter()
        .map(|(_, (p, _))| (p.x, p.y))
        .chain(
            world
                .query::<(&Position, &OnFire)>()
                .iter()
                .map(|(_, (p, _))| (p.x, p.y)),
        )
        .collect();
    if fires.is_empty() {
        return;
    }
    let extra = game_dt * (WET_DRY_NEAR_FIRE_MULT - 1.0).max(0.0);
    for (_, (pos, status)) in world.query_mut::<(&Position, &mut StatusEffects)>() {
        let near = fires
            .iter()
            .any(|&(fx, fy)| (pos.x - fx).abs() <= 1 && (pos.y - fy).abs() <= 1);
        if !near {
            continue;
        }
        if let Some(wet) = status.effects.iter_mut().find(|e| e.effect_type == EffectType::Wet) {
            wet.remaining_duration -= extra;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::components::{Combustible, VisualPosition};
    use crate::engine::EffectCtx;
    use crate::spatial_cache::SpatialCache;
    use crate::tile::TileType;
    use rand::{rngs::StdRng, SeedableRng};

    /// A 12x5 floor with a column of water at x = 3.
    fn grid_with_water() -> Grid {
        let mut grid = Grid::filled(12, 5, TileType::Floor);
        for y in 0..5 {
            grid.get_mut(3, y).unwrap().tile_type = TileType::Water;
            grid.water_positions.push((3, y));
        }
        grid
    }

    fn walker(world: &mut World, x: i32, y: i32, flammability: Option<f32>) -> Entity {
        let pos = Position::new(x, y);
        let e = world.spawn((pos, VisualPosition::from_position(&pos), StatusEffects::new()));
        if let Some(f) = flammability {
            world.insert_one(e, Combustible { flammability: f }).unwrap();
        }
        e
    }

    fn step(world: &mut World, grid: &mut Grid, rng: &mut StdRng, e: Entity, dx: i32) {
        let mut spatial = SpatialCache::rebuild_from_world(world);
        let mut events = EventQueue::new();
        let mut ctx = EffectCtx { world, grid, spatial: &mut spatial, events: &mut events, rng };
        crate::systems::actions::apply_move(&mut ctx, e, dx, 0);
    }

    fn has(world: &World, e: Entity, t: EffectType) -> bool {
        effects::entity_has_effect(world, e, t)
    }

    #[test]
    fn wading_leaves_you_wet_after_you_step_out() {
        let mut world = World::new();
        let mut grid = grid_with_water();
        let mut rng = StdRng::seed_from_u64(1);
        let e = walker(&mut world, 2, 2, Some(0.35));

        step(&mut world, &mut grid, &mut rng, e, 1); // into the water
        step(&mut world, &mut grid, &mut rng, e, 1); // and out again
        assert!(has(&world, e, EffectType::Wet), "still dripping on dry land");
        assert_eq!(effective_flammability(&world, e), 0.0, "wet things can't catch");
    }

    #[test]
    fn walking_through_oil_coats_you_and_leaves_a_trail() {
        let mut world = World::new();
        let mut grid = Grid::filled(20, 3, TileType::Floor);
        let mut rng = StdRng::seed_from_u64(9);
        let e = walker(&mut world, 0, 1, Some(0.35));
        crate::spawning::spawn_oil_puddle(&mut world, 1, 1);

        for _ in 0..15 {
            step(&mut world, &mut grid, &mut rng, e, 1);
        }
        assert!(has(&world, e, EffectType::Oily), "walked through the puddle");
        assert!(
            (effective_flammability(&world, e) - (0.35 * OILY_FLAMMABILITY_MULT).min(1.0)).abs() < 1e-6,
            "oil multiplies flammability"
        );
        let drips = world
            .query::<(&Position, &OilPuddle)>()
            .iter()
            .filter(|(_, (p, _))| p.x > 1)
            .count();
        // 14 tiles left behind at 50% each: no drips at all is a 1-in-16000 fluke,
        // and the seed fixes it either way.
        assert!(drips > 0, "an oily walker drips oil behind it");
    }

    #[test]
    fn water_washes_oil_off() {
        let mut world = World::new();
        let mut grid = grid_with_water();
        let mut rng = StdRng::seed_from_u64(1);
        let e = walker(&mut world, 2, 2, Some(0.35));
        coat_in_oil(&mut world, e, &mut EventQueue::new());

        step(&mut world, &mut grid, &mut rng, e, 1);
        assert!(!has(&world, e, EffectType::Oily));
        assert!(has(&world, e, EffectType::Wet));
    }

    #[test]
    fn oil_makes_even_bone_burn() {
        let mut world = World::new();
        let e = walker(&mut world, 0, 0, None); // no Combustible: a skeleton
        assert_eq!(effective_flammability(&world, e), 0.0);
        coat_in_oil(&mut world, e, &mut EventQueue::new());
        assert!(effective_flammability(&world, e) >= OILY_MIN_FLAMMABILITY);
    }

    #[test]
    fn soaking_a_lit_barrel_puts_out_its_fuse() {
        let mut world = World::new();
        let barrel = crate::spawning::spawn_oil_barrel(&mut world, 2, 2);
        world.insert_one(barrel, BarrelFuse { remaining: 1.0 }).unwrap();
        soak(&mut world, barrel, &mut EventQueue::new());
        assert!(world.get::<&BarrelFuse>(barrel).is_err(), "the fuse fizzles");
    }

    #[test]
    fn standing_by_a_fire_dries_you_faster() {
        let mut world = World::new();
        let near = walker(&mut world, 1, 0, None);
        let far = walker(&mut world, 9, 0, None);
        crate::spawning::spawn_campfire(&mut world, 0, 0);
        for e in [near, far] {
            effects::add_effect_to_entity(&mut world, e, EffectType::Wet, WET_DURATION);
        }
        dry_near_fire(&mut world, 1.0);
        let left = |e| {
            world.get::<&StatusEffects>(e).unwrap().effects.iter()
                .find(|x| x.effect_type == EffectType::Wet).unwrap().remaining_duration
        };
        assert!(left(near) < left(far), "the one by the fire dries off quicker");
    }
}
