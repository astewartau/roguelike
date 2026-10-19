//! What creatures carry on them: water and oil.
//!
//! Terrain doesn't move, but creatures do. A creature that wades through water
//! comes out Wet and stays fireproof for a while; one that walks through an
//! oil puddle comes out Oiled, burns far more readily, and drips oil on the
//! tiles it leaves — so an oiled orc chasing you lays its own fuse. This is how
//! water and oil get carried around the map instead of only mattering on the
//! tile where they lie.
//!
//! The statuses themselves — where they come from (`tile_effects`, splashes),
//! which excludes which (`effects::add_effect`) and how they change ignition
//! (`fire::ignition_profile`) — live with the rest of the surface statuses.
//! This module is what they leave behind and how they wear off: the drip
//! trail, and drying off by a fire.

use hecs::{Entity, World};
use rand::Rng;

use crate::components::{CausesBurning, EffectType, OilPuddle, OnFire, Position, StatusEffects};
use crate::constants::*;
use crate::grid::Grid;
use crate::systems::effects;

/// An oiled walker just left `from`: it may drip oil there (`OILED_DRIP_CHANCE`),
/// leaving a small, short-burning puddle. Never on water, and never where oil
/// already lies.
pub fn drip_oil(
    world: &mut World,
    grid: &Grid,
    entity: Entity,
    from: (i32, i32),
    rng: &mut impl Rng,
) {
    if !effects::entity_has_effect(world, entity, EffectType::Oiled) {
        return;
    }
    if !grid.is_walkable(from.0, from.1) || grid.water_positions.contains(&from) {
        return;
    }
    let oil_there = world
        .query::<(&Position, &OilPuddle)>()
        .iter()
        .any(|(_, (p, _))| (p.x, p.y) == from);
    if oil_there || !rng.gen_bool(OILED_DRIP_CHANCE) {
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
    use crate::components::VisualPosition;
    use crate::engine::EffectCtx;
    use crate::events::EventQueue;
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

    fn walker(world: &mut World, x: i32, y: i32) -> Entity {
        let pos = Position::new(x, y);
        world.spawn((pos, VisualPosition::from_position(&pos), StatusEffects::new()))
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
        let e = walker(&mut world, 2, 2);

        step(&mut world, &mut grid, &mut rng, e, 1); // into the water
        step(&mut world, &mut grid, &mut rng, e, 1); // and out again
        assert!(has(&world, e, EffectType::Wet), "still dripping on dry land");
    }

    #[test]
    fn walking_through_oil_coats_you_and_leaves_a_trail() {
        let mut world = World::new();
        let mut grid = Grid::filled(20, 3, TileType::Floor);
        let mut rng = StdRng::seed_from_u64(9);
        let e = walker(&mut world, 0, 1);
        crate::spawning::spawn_oil_puddle(&mut world, 1, 1);

        for _ in 0..10 {
            step(&mut world, &mut grid, &mut rng, e, 1);
        }
        assert!(has(&world, e, EffectType::Oiled), "walked through the puddle");
        let drips = world
            .query::<(&Position, &OilPuddle)>()
            .iter()
            .filter(|(_, (p, _))| p.x > 1)
            .count();
        // Nine tiles left behind at 50% each; the seed fixes the outcome.
        assert!(drips > 0, "an oiled walker drips oil behind it");
    }

    #[test]
    fn water_washes_oil_off() {
        let mut world = World::new();
        let mut grid = grid_with_water();
        let mut rng = StdRng::seed_from_u64(1);
        let e = walker(&mut world, 2, 2);
        effects::add_effect_to_entity(&mut world, e, EffectType::Oiled, OILED_DURATION);

        step(&mut world, &mut grid, &mut rng, e, 1);
        assert!(!has(&world, e, EffectType::Oiled));
        assert!(has(&world, e, EffectType::Wet));
    }

    #[test]
    fn standing_by_a_fire_dries_you_faster() {
        let mut world = World::new();
        let near = walker(&mut world, 1, 0);
        let far = walker(&mut world, 9, 0);
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
