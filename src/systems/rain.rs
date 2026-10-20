//! Call Rain's downpour, which keeps falling for a while after the cast.
//!
//! A `RainCloud` soaks everything under it every `CALL_RAIN_SOAK_INTERVAL`
//! (`fire::splash_water`: creatures go Wet, fires go out, the ground stays
//! too wet to burn) until `CALL_RAIN_DURATION` runs out. It is paced by game
//! time like the fire tick, so it stops while the game waits on you and you
//! can walk about in it.

use hecs::Entity;

use crate::components::{Position, RainCloud};
use crate::constants::CALL_RAIN_SOAK_INTERVAL;
use crate::engine::EffectCtx;
use crate::events::{EventQueue, GameEvent};

/// Advance every rain cloud by `game_dt`, soaking under each one at fixed
/// intervals and clearing the ones that have run out.
pub fn tick_rain(ctx: &mut EffectCtx, game_dt: f32) {
    if game_dt <= 0.0 {
        return;
    }
    let clouds: Vec<(Entity, (i32, i32))> = ctx
        .world
        .query::<(&Position, &RainCloud)>()
        .iter()
        .map(|(id, (p, _))| (id, (p.x, p.y)))
        .collect();

    for (id, (x, y)) in clouds {
        let (soakings, radius, spent) = {
            let Ok(mut cloud) = ctx.world.get::<&mut RainCloud>(id) else {
                continue;
            };
            // Fixed-size steps: each soaking either happens or it doesn't.
            let step = game_dt.min(cloud.remaining.max(0.0));
            cloud.since_soak += step;
            cloud.remaining -= game_dt;
            let mut soakings = 0;
            while cloud.since_soak >= CALL_RAIN_SOAK_INTERVAL {
                cloud.since_soak -= CALL_RAIN_SOAK_INTERVAL;
                soakings += 1;
            }
            (soakings, cloud.radius, cloud.remaining <= 0.0)
        };
        for _ in 0..soakings {
            soak(ctx, (x, y), radius);
        }
        if spent {
            let _ = ctx.world.despawn(id);
        }
    }
}

/// Soak the patch again. The rain has been falling all along, so only news
/// is passed on: a barrel that was already soaked isn't reported again, but
/// one whose fuse the rain just put out is.
fn soak(ctx: &mut EffectCtx, (x, y): (i32, i32), radius: i32) {
    let mut events = EventQueue::new();
    crate::systems::fire::splash_water(ctx.world, ctx.grid, x, y, radius, &mut events);
    for event in events.drain() {
        if !matches!(event, GameEvent::BarrelSoaked { defused: false, .. }) {
            ctx.events.push(event);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::components::{EffectType, StatusEffects};
    use crate::constants::{CALL_RAIN_DURATION, CALL_RAIN_RADIUS};
    use crate::grid::Grid;
    use crate::tile::TileType;
    use rand::{rngs::StdRng, SeedableRng};

    #[test]
    fn rain_keeps_soaking_whatever_walks_in_until_it_stops() {
        let mut world = hecs::World::new();
        let mut grid = Grid::filled(12, 12, TileType::Floor);
        let mut spatial = crate::spatial_cache::SpatialCache::rebuild_from_world(&world);
        let mut events = EventQueue::new();
        let mut rng = StdRng::seed_from_u64(1);
        let cloud = world.spawn((
            Position::new(5, 5),
            RainCloud { radius: CALL_RAIN_RADIUS, remaining: CALL_RAIN_DURATION, since_soak: 0.0 },
        ));
        let mut ctx = EffectCtx { world: &mut world, grid: &mut grid, spatial: &mut spatial, events: &mut events, rng: &mut rng };

        // Something walks in a couple of seconds after the rain started.
        tick_rain(&mut ctx, 2.0);
        let walker = ctx.world.spawn((Position::new(6, 5), StatusEffects::new()));
        tick_rain(&mut ctx, 0.6);
        assert!(crate::systems::effects::entity_has_effect(ctx.world, walker, EffectType::Wet), "it gets wet");

        // Then the rain stops, and the cloud is gone.
        tick_rain(&mut ctx, CALL_RAIN_DURATION);
        assert!(ctx.world.get::<&RainCloud>(cloud).is_err());
    }

    #[test]
    fn rain_puts_out_carried_torches() {
        let mut world = hecs::World::new();
        let mut grid = Grid::filled(12, 12, TileType::Floor);
        let mut spatial = crate::spatial_cache::SpatialCache::rebuild_from_world(&world);
        let mut events = EventQueue::new();
        let mut rng = StdRng::seed_from_u64(2);
        let goblin = crate::spawning::enemies::GOBLIN.spawn(&mut world, 6, 5, &mut rng);
        let hero = world.spawn((
            Position::new(4, 5),
            crate::components::Equipment::empty(),
            StatusEffects::new(),
        ));
        world.get::<&mut crate::components::Equipment>(hero).unwrap().off_hand =
            Some(crate::components::ItemInstance::plain(crate::components::ItemType::Torch));
        crate::systems::torches::sync_held_torch(&mut world, hero);
        world.spawn((
            Position::new(5, 5),
            RainCloud { radius: CALL_RAIN_RADIUS, remaining: CALL_RAIN_DURATION, since_soak: 0.0 },
        ));
        let lit = |w: &hecs::World, e| w.get::<&crate::components::CarriedTorch>(e).is_ok_and(|t| t.lit);
        assert!(lit(&world, goblin) && lit(&world, hero));

        let mut ctx = EffectCtx { world: &mut world, grid: &mut grid, spatial: &mut spatial, events: &mut events, rng: &mut rng };
        tick_rain(&mut ctx, 0.6);
        let mut acc = 0.0;
        let mut dirty = false;
        crate::systems::fire::tick_fire(&mut ctx, 0.1, &mut acc, &mut dirty);
        assert!(!lit(ctx.world, goblin), "the goblin's torch is rained out");
        assert!(!lit(ctx.world, hero), "so is yours");
    }
}
