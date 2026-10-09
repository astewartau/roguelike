//! What a tile does to whoever arrives on it, or stands on it.
//!
//! [`on_enter_tile`] is the one hook every relocation goes through: a normal
//! step (`apply_move`), Blink (scroll and learned), Disengage, Tumble's
//! landing, a knockback shove, and both ends of a Sacrifice swap. Before this
//! existed only `apply_move` ran tile effects, so teleporting into water left
//! you burning and a shove into a fire trap never sprang it.
//!
//! [`refresh_standing_effects`] covers the other half: surface statuses that
//! persist while you stay put (Wet in water, Oiled in an unlit oil puddle).
//! It is driven from `fire::tick_fire` each engine tick, so it advances on game
//! time like the rest of the fire ecosystem.

use hecs::{Entity, World};
use rand::Rng;

use crate::components::{
    BurningOil, CausesBurning, EffectType, OilPuddle, Player, Position, StatusEffects,
};
use crate::constants::*;
use crate::events::{EventQueue, GameEvent};
use crate::grid::Grid;
use crate::systems::actions::{
    check_dungeon_trap_trigger, check_fire_trap_trigger, check_snare_trap_trigger,
};
use crate::systems::effects;

/// Whether this entity touches the ground, and so is affected by what is on
/// the floor (water, oil, fire, webs, traps).
///
/// The single place flight will hook in: Phase 4 adds a `Flying` component,
/// and flyers skip every ground effect below.
// TODO(phase 4): `return world.get::<&Flying>(entity).is_err();`
fn touches_ground(_world: &World, _entity: Entity) -> bool {
    true
}

/// Whether an unlit oil puddle lies on `(x, y)`. Burning puddles set you
/// alight (`CausesBurning`) instead of oiling you.
pub fn unlit_oil_at(world: &World, x: i32, y: i32) -> bool {
    world
        .query::<&Position>()
        .with::<&OilPuddle>()
        .without::<&BurningOil>()
        .iter()
        .any(|(_, p)| p.x == x && p.y == y)
}

/// Apply everything that happens when `entity` arrives on `(x, y)`, however it
/// got there. Call after its `Position` (and spatial cache) are updated.
///
/// In order:
/// 1. water soaks (Wet: puts out Burning, washes off Oil);
/// 2. an unlit oil puddle oils (Oiled, refused while Wet);
/// 3. a fire source (`CausesBurning`: braziers, burning grass/oil/webs) sets
///    it alight, unless Wet;
/// 4. webs, fire traps, snare traps and dungeon floor traps trigger;
/// 5. a player rolls passive discovery for nearby secrets.
pub fn on_enter_tile(
    world: &mut World,
    grid: &Grid,
    entity: Entity,
    (x, y): (i32, i32),
    events: &mut EventQueue,
    rng: &mut impl Rng,
) {
    if touches_ground(world, entity) {
        // Water: soaked through, which also puts out fire.
        if grid.water_positions.contains(&(x, y)) {
            effects::add_effect_announced(world, events, entity, EffectType::Wet, WET_DURATION);
        }

        // Unlit oil: slick with oil.
        if unlit_oil_at(world, x, y) {
            effects::add_effect_announced(world, events, entity, EffectType::Oiled, OILED_DURATION);
        }

        // Fire source (brazier, campfire, burning grass/oil/web).
        let stepped_on_fire = world
            .query::<(&Position, &CausesBurning)>()
            .iter()
            .any(|(_, (pos, _))| pos.x == x && pos.y == y);
        if stepped_on_fire
            && effects::add_effect_to_entity(world, entity, EffectType::Burning, BURNING_DURATION)
        {
            events.push(GameEvent::CaughtFire { entity, position: (x, y) });
        }

        // Webs root non-spiders (and are consumed).
        crate::systems::webs::trigger_web_at(world, entity, x, y, events);

        // Placed traps, then dungeon-generated floor traps (no owner
        // exemption: enemies set those off too).
        check_fire_trap_trigger(world, entity, x, y, events, rng);
        check_snare_trap_trigger(world, entity, x, y, events);
        check_dungeon_trap_trigger(world, grid, entity, x, y, events, rng);
    }

    // After a player arrives: roll passive detection for hidden traps and
    // secret doors within one tile (Agility-scaled). Not a ground effect.
    if world.get::<&Player>(entity).is_ok() {
        crate::systems::discovery::roll_player_discovery(world, entity, events, rng);
    }
}

/// Keep surface statuses topped up for everything standing still on water or
/// unlit oil: Wet never runs out while you stand in a pool (and nothing in it
/// can stay alight), Oiled never runs out while you stand in the spill.
///
/// Idempotent — it only refreshes to the full duration — so calling it every
/// engine tick is safe; it is paced by `tick_fire`, i.e. by game time.
pub fn refresh_standing_effects(world: &mut World, grid: &Grid, events: &mut EventQueue) {
    let standing: Vec<(Entity, (i32, i32))> = world
        .query::<(&Position, &StatusEffects)>()
        .iter()
        .map(|(id, (p, _))| (id, (p.x, p.y)))
        .collect();
    if standing.is_empty() {
        return;
    }
    let oil_tiles: std::collections::HashSet<(i32, i32)> = world
        .query::<&Position>()
        .with::<&OilPuddle>()
        .without::<&BurningOil>()
        .iter()
        .map(|(_, p)| (p.x, p.y))
        .collect();

    for (id, tile) in standing {
        if !touches_ground(world, id) {
            continue;
        }
        if grid.water_positions.contains(&tile) {
            effects::add_effect_announced(world, events, id, EffectType::Wet, WET_DURATION);
        }
        if oil_tiles.contains(&tile) {
            effects::add_effect_announced(world, events, id, EffectType::Oiled, OILED_DURATION);
        }
    }
}
