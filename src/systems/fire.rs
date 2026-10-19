//! Spreading fire system.
//!
//! Fire is carried by entities, in two forms:
//!
//! - **Creatures** (and oil barrels) burn through the `Burning` status, and
//!   catch at a rate scaled by their flammability: `Combustible`, adjusted by
//!   what they are coated in (`ignition_profile` — Wet things can't catch,
//!   Oiled things catch readily, even if they wouldn't burn on their own).
//! - **Fuel** — anything that sits on a tile and burns: grass fires, oil
//!   puddles, webs, glowing fungus. Each carries a `Fuel` component that says
//!   as data how readily it catches, how long it burns, what water does to it
//!   and what it leaves behind; it gains `OnFire` while alight. Every kind of
//!   fuel goes through the same ignite / spread / douse / burnout code here,
//!   so a new burnable object needs a `Fuel` constructor and nothing else.
//!
//! Tall grass is terrain rather than an entity: when it catches, a grass-fire
//! patch (`Fuel::grass_fire`) is spawned on the tile, and that patch's
//! `Burnout::ScorchGrass` burns the tile down to floor when it is spent.
//!
//! This system also ticks oil-barrel fuses (and detonates them) and dries
//! soaked tiles. Spread happens in discrete game-time steps.
//!
//! It runs from the engine tick (not the deep sim) so it can take `&mut Grid`,
//! and is paced by the game-clock delta this frame (so it freezes when paused).

use std::collections::{HashMap, HashSet};

use hecs::{Entity, World};
use rand::Rng;

use crate::components::{
    AnimatedSprite, BarrelFuse, Brazier, Burnout, CausesBurning, Combustible, Doused, EffectType,
    Fuel, FuseCause, Health, Ignition, LightSource, OilBarrel, OilPuddle, OnFire, Position,
    Sprite, StatusEffects, WetTile,
};
use crate::constants::*;
use crate::engine::EffectCtx;
use crate::events::{EventQueue, GameEvent};
use crate::grid::Grid;
use crate::spatial_cache::SpatialCache;
use crate::tile::{tile_ids, TileType};

const NEIGHBORS: [(i32, i32); 8] = [
    (-1, 0), (1, 0), (0, -1), (0, 1),
    (-1, -1), (-1, 1), (1, -1), (1, 1),
];

/// Advance burning fuel, barrel fuses, drying tiles, and fire spread.
/// `game_dt` is elapsed game-time this frame; `accumulator` carries
/// fractional game-time between discrete spread steps.
pub fn tick_fire(
    ctx: &mut EffectCtx,
    game_dt: f32,
    accumulator: &mut f32,
    fov_dirty: &mut bool,
) {
    let EffectCtx { world, grid, spatial: spatial_cache, events, rng } = ctx;
    let (world, grid) = (&mut **world, &mut **grid);
    let (spatial_cache, events, rng) = (&mut **spatial_cache, &mut **events, &mut **rng);

    // A barrel hit while its fuse was running was marked to blow at once (see
    // `on_barrel_damaged`). That does not wait on elapsed game time: it goes
    // off this frame even if the clock did not move.
    detonate_ready_barrels(world, grid, spatial_cache, events, rng);

    if game_dt <= 0.0 {
        return;
    }

    // 0. Surface statuses: anything standing in water stays Wet (so it can't
    //    stay alight), anything standing in unlit oil stays Oiled. Anything
    //    wet beside a fire dries off faster, and a soaked torch goes out.
    crate::systems::tile_effects::refresh_standing_effects(world, grid, events);
    crate::systems::coatings::dry_near_fire(world, game_dt);
    crate::systems::torches::douse_wet_torches(world, events);

    // 1. Burnout: every burning fuel counts down, then leaves what its
    //    `Burnout` says it leaves.
    let mut spent: Vec<(Entity, (i32, i32), Burnout)> = Vec::new();
    for (id, (pos, fuel, fire)) in world.query_mut::<(&Position, &Fuel, &mut OnFire)>() {
        fire.remaining -= game_dt;
        if fire.remaining <= 0.0 {
            spent.push((id, (pos.x, pos.y), fuel.burnout));
        }
    }
    for (id, (x, y), burnout) in spent {
        if let Burnout::LeavesItem(kind) = burnout {
            crate::systems::torches::put_out_dropped_torch(world, id, Some(kind));
            continue;
        }
        let _ = world.despawn(id);
        if burnout == Burnout::ScorchGrass {
            if let Some(tile) = grid.get_mut(x, y) {
                if tile.tile_type == TileType::TallGrass {
                    tile.tile_type = TileType::Floor;
                    tile.sprite_override = None;
                    *fov_dirty = true; // grass blocked vision; floor doesn't
                }
            }
        }
    }

    // 1b. A floor torch whose torch was picked up stops burning.
    crate::systems::torches::tick_dropped_torches(world);

    // 2. Soaked tiles dry out.
    let mut dried: Vec<Entity> = Vec::new();
    for (id, wet) in world.query_mut::<&mut WetTile>() {
        wet.remaining -= game_dt;
        if wet.remaining <= 0.0 {
            dried.push(id);
        }
    }
    for id in dried {
        let _ = world.despawn(id);
    }

    // 3. Oil barrels: a burning barrel lights its (fire) fuse. A barrel at
    //    0 HP without a fuse was broken by something that bypassed
    //    `apply_damage` (burn ticks subtract health directly): it gets its
    //    break fuse here. Then every fuse burns down in game time, and the
    //    ones that run out blow (chain reactions resolve in the same pass).
    let to_fuse: Vec<Entity> = world
        .query::<(&OilBarrel, &StatusEffects)>()
        .without::<&BarrelFuse>()
        .iter()
        .filter(|(_, (_, s))| s.effects.iter().any(|e| e.effect_type == EffectType::Burning))
        .map(|(id, _)| id)
        .collect();
    for id in to_fuse {
        let _ = world.insert_one(id, BarrelFuse::new(OIL_BARREL_FUSE_SECONDS, FuseCause::Fire));
    }
    let broken: Vec<Entity> = world
        .query::<(&OilBarrel, &Health)>()
        .without::<&BarrelFuse>()
        .iter()
        .filter(|(_, (_, h))| h.current <= 0)
        .map(|(id, _)| id)
        .collect();
    for id in broken {
        start_break_fuse(world, id, events);
    }

    for (_, (_, fuse)) in world.query_mut::<(&OilBarrel, &mut BarrelFuse)>() {
        fuse.remaining -= game_dt;
    }
    detonate_ready_barrels(world, grid, spatial_cache, events, rng);

    // 4. Spread in fixed game-time steps so behaviour is frame-rate independent.
    *accumulator += game_dt;
    while *accumulator >= FIRE_STEP_INTERVAL {
        *accumulator -= FIRE_STEP_INTERVAL;
        spread_step(world, grid, events, rng);
    }
}

/// Effective flammability of a creature for ignition rolls, or `None` if it
/// cannot catch fire right now.
///
/// - Already Burning, or Wet: `None`.
/// - `Combustible`: its flammability.
/// - Oiled: the oil burns even if the creature would not, so at least
///   `OILED_MIN_FLAMMABILITY` — this is what lets an oiled skeleton or slime
///   burn.
///
/// The second value is the chance multiplier: `OILED_IGNITE_MULT` while Oiled,
/// else 1.
fn ignition_profile(comb: Option<&Combustible>, status: &StatusEffects) -> Option<(f64, f64)> {
    let has = |t: EffectType| status.effects.iter().any(|e| e.effect_type == t);
    if has(EffectType::Burning) || has(EffectType::Wet) {
        return None;
    }
    let base = comb.map(|c| c.flammability).unwrap_or(0.0);
    if has(EffectType::Oiled) {
        Some((base.max(OILED_MIN_FLAMMABILITY) as f64, OILED_IGNITE_MULT))
    } else if comb.is_some() && base > 0.0 {
        Some((base as f64, 1.0))
    } else {
        None
    }
}

/// Per-roll chance that a creature with this ignition profile catches from a
/// source with `base` chance.
fn ignite_chance(base: f64, (flammability, mult): (f64, f64)) -> f64 {
    (base * flammability * mult).clamp(0.0, 1.0)
}

/// On a melee hit, a burning combatant may set the other alight (medium chance
/// scaled by the target's flammability, much higher if the target is Oiled).
/// No-op unless `from` is burning and `to` can ignite (see `ignition_profile`).
pub fn try_combat_ignite(world: &mut World, from: Entity, to: Entity, rng: &mut impl Rng) {
    let from_burning = world
        .get::<&StatusEffects>(from)
        .map(|s| s.effects.iter().any(|e| e.effect_type == EffectType::Burning))
        .unwrap_or(false);
    if !from_burning {
        return;
    }
    let profile = {
        let Ok(status) = world.get::<&StatusEffects>(to) else {
            return;
        };
        let comb = world.get::<&Combustible>(to).ok();
        ignition_profile(comb.as_deref(), &status)
    };
    let Some(profile) = profile else {
        return;
    };
    if rng.gen_bool(ignite_chance(FIRE_COMBAT_IGNITE_CHANCE, profile)) {
        crate::systems::effects::add_effect_to_entity(world, to, EffectType::Burning, BURNING_DURATION);
    }
}

/// Positions of all soaked tiles.
fn wet_positions(world: &World) -> HashSet<(i32, i32)> {
    world
        .query::<(&Position, &WetTile)>()
        .iter()
        .map(|(_, (p, _))| (p.x, p.y))
        .collect()
}

/// Tiles whose tall grass is already burning (a grass-fire patch sits on them).
fn grass_fire_tiles(world: &World) -> HashSet<(i32, i32)> {
    world
        .query::<(&Position, &Fuel, &OnFire)>()
        .iter()
        .filter(|(_, (_, fuel, _))| fuel.burnout == Burnout::ScorchGrass)
        .map(|(_, (p, _, _))| (p.x, p.y))
        .collect()
}

fn adjacent_or_on(a: (i32, i32), b: (i32, i32)) -> bool {
    (a.0 - b.0).abs() <= 1 && (a.1 - b.1).abs() <= 1
}

fn spread_step(world: &mut World, grid: &mut Grid, events: &mut EventQueue, rng: &mut impl Rng) {
    // Two kinds of source:
    //  - burning fuel (grass fires, oil, webs) spreads to adjacent grass (the
    //    wildfire front), to adjacent fuel, and to creatures beside it;
    //  - burning creatures readily light grass they stand IN, but only rarely
    //    grass they're merely next to.
    let fuel_sources: Vec<(i32, i32)> = world
        .query::<(&Position, &Fuel, &OnFire)>()
        .iter()
        .filter(|(_, (_, fuel, _))| fuel.spreads)
        .map(|(_, (p, _, _))| (p.x, p.y))
        .collect();
    let creature_sources: Vec<(i32, i32)> = world
        .query::<(&Position, &StatusEffects)>()
        .iter()
        .filter(|(_, (_, s))| s.effects.iter().any(|e| e.effect_type == EffectType::Burning))
        .map(|(_, (p, _))| (p.x, p.y))
        .collect();
    if fuel_sources.is_empty() && creature_sources.is_empty() {
        return;
    }

    let wet = wet_positions(world);
    let grass_burning = grass_fire_tiles(world);
    let flammable_free = |x: i32, y: i32, pending: &HashSet<(i32, i32)>| {
        !grass_burning.contains(&(x, y))
            && !pending.contains(&(x, y))
            && !wet.contains(&(x, y))
            && grid.get(x, y).map(|t| t.tile_type.is_flammable()).unwrap_or(false)
    };

    let mut grass_to_ignite: HashSet<(i32, i32)> = HashSet::new();

    // Burning fuel -> adjacent grass (slow front).
    for &(sx, sy) in &fuel_sources {
        for (dx, dy) in NEIGHBORS {
            let (nx, ny) = (sx + dx, sy + dy);
            if flammable_free(nx, ny, &grass_to_ignite) && rng.gen_bool(FIRE_GRASS_TO_GRASS_CHANCE) {
                grass_to_ignite.insert((nx, ny));
            }
        }
    }

    // Burning creature -> grass it stands in (high) / grass adjacent (low).
    for &(cx, cy) in &creature_sources {
        if flammable_free(cx, cy, &grass_to_ignite) && rng.gen_bool(FIRE_ENTITY_ON_GRASS_CHANCE) {
            grass_to_ignite.insert((cx, cy));
        }
        for (dx, dy) in NEIGHBORS {
            let (nx, ny) = (cx + dx, cy + dy);
            if flammable_free(nx, ny, &grass_to_ignite)
                && rng.gen_bool(FIRE_ENTITY_ADJACENT_GRASS_CHANCE)
            {
                grass_to_ignite.insert((nx, ny));
            }
        }
    }

    let all_sources: HashSet<(i32, i32)> =
        fuel_sources.iter().chain(creature_sources.iter()).copied().collect();
    let near_fire = |p: (i32, i32)| all_sources.iter().any(|&s| adjacent_or_on(p, s));

    // Fire -> unlit fuel on or next to it, each at its own catch chance.
    let mut fuel_to_ignite: Vec<Entity> = Vec::new();
    for (id, (pos, fuel)) in world.query::<(&Position, &Fuel)>().without::<&OnFire>().iter() {
        if fuel.catch_chance <= 0.0 || (fuel.dampable && wet.contains(&(pos.x, pos.y))) {
            continue;
        }
        if near_fire((pos.x, pos.y)) && rng.gen_bool(fuel.catch_chance.clamp(0.0, 1.0)) {
            fuel_to_ignite.push(id);
        }
    }

    // Fire -> creatures. Standing IN burning fuel that says so (oil) is a
    // high chance; being merely next to any fire is a small one. Both scale
    // by the creature's ignition profile: Oiled creatures catch far more
    // readily, Wet or already-burning ones not at all (see
    // `ignition_profile`). A flyer hovers above whatever burns on its own
    // tile (and isn't standing in the water either): only neighbouring fire
    // reaches it.
    let mut stand_chance: HashMap<(i32, i32), f64> = HashMap::new();
    for (_, (pos, fuel, _)) in world.query::<(&Position, &Fuel, &OnFire)>().iter() {
        if fuel.spreads && fuel.stand_ignite_chance > 0.0 {
            let c = stand_chance.entry((pos.x, pos.y)).or_insert(0.0);
            *c = c.max(fuel.stand_ignite_chance);
        }
    }
    let mut creatures_to_ignite: Vec<(Entity, (i32, i32))> = Vec::new();
    for (id, (pos, status, comb)) in
        world.query::<(&Position, &StatusEffects, Option<&Combustible>)>().iter()
    {
        let grounded = crate::systems::tile_effects::touches_ground(world, id);
        if grounded && grid.water_positions.contains(&(pos.x, pos.y)) {
            continue;
        }
        let Some(profile) = ignition_profile(comb, status) else {
            continue;
        };

        if grounded {
            if let Some(&base) = stand_chance.get(&(pos.x, pos.y)) {
                if rng.gen_bool(ignite_chance(base, profile)) {
                    creatures_to_ignite.push((id, (pos.x, pos.y)));
                    continue;
                }
            }
        }

        let near = all_sources.iter().any(|&(sx, sy)| {
            adjacent_or_on((pos.x, pos.y), (sx, sy)) && (grounded || (sx, sy) != (pos.x, pos.y))
        });
        if !near {
            continue;
        }
        if rng.gen_bool(ignite_chance(FIRE_SPREAD_ENTITY_CHANCE, profile)) {
            creatures_to_ignite.push((id, (pos.x, pos.y)));
        }
    }

    // Apply (after queries are dropped).
    for (x, y) in grass_to_ignite {
        crate::spawning::spawn_fire_patch(world, x, y);
    }
    for id in fuel_to_ignite {
        ignite_fuel(world, id);
    }
    for (id, pos) in creatures_to_ignite {
        if crate::systems::effects::add_effect_to_entity(world, id, EffectType::Burning, BURNING_DURATION) {
            events.push(GameEvent::CaughtFire { entity: id, position: pos });
        }
    }
}

/// Set a fuel entity alight, the way its `Fuel::ignition` says. No-op if the
/// entity isn't fuel or is already burning.
///
/// `InPlace` fuel gains `OnFire`, `CausesBurning` (stepping in ignites), a
/// fire sprite and a light. `Replace` fuel is consumed and a fire patch takes
/// its tile.
pub fn ignite_fuel(world: &mut World, id: Entity) {
    let Ok(fuel) = world.get::<&Fuel>(id).map(|f| *f) else {
        return;
    };
    if world.get::<&OnFire>(id).is_ok() {
        return;
    }
    match fuel.ignition {
        Ignition::InPlace => {
            let _ = world.insert(
                id,
                (
                    OnFire { remaining: fuel.burn_time },
                    CausesBurning,
                    AnimatedSprite::fire_pit(),
                    LightSource::brazier(),
                ),
            );
        }
        Ignition::Replace => {
            let Some((x, y)) = crate::queries::get_entity_position(world, id) else {
                return;
            };
            let _ = world.despawn(id);
            crate::spawning::spawn_fire_patch(world, x, y);
        }
    }
}

/// Put out a burning fuel entity, the way its `Fuel::doused` says. No-op if
/// it isn't burning.
fn douse_fuel(world: &mut World, id: Entity) {
    let Ok(fuel) = world.get::<&Fuel>(id).map(|f| *f) else {
        return;
    };
    if world.get::<&OnFire>(id).is_err() {
        return;
    }
    match fuel.doused {
        Doused::Extinguish => {
            let _ = world.remove::<(OnFire, CausesBurning, AnimatedSprite, LightSource)>(id);
        }
        Doused::Despawn => {
            let _ = world.despawn(id);
        }
        Doused::LeavesItem(kind) => {
            crate::systems::torches::put_out_dropped_torch(world, id, Some(kind));
        }
    }
}

/// Set a tile alight without touching what stands on it: dry tall grass
/// catches (unless it is already burning), and every unlit fuel on the tile
/// ignites (damp-sensitive fuel only if the tile is dry). Water tiles never
/// catch. Used by incendiary projectiles and, through `spill_fire_at`, by
/// brazier topples and fire traps.
pub fn ignite_tile(world: &mut World, grid: &Grid, x: i32, y: i32) {
    if grid.water_positions.contains(&(x, y)) {
        return;
    }
    let wet = wet_positions(world).contains(&(x, y));
    let flammable = grid
        .get(x, y)
        .map(|t| t.tile_type.is_flammable())
        .unwrap_or(false);
    if flammable && !wet && !grass_fire_tiles(world).contains(&(x, y)) {
        crate::spawning::spawn_fire_patch(world, x, y);
    }

    ignite_fuel_at(world, grid, x, y);
}

/// Ignite every unlit fuel lying on (x, y) — oil, webs, fungus — but not the
/// terrain. Damp-sensitive fuel stays unlit on a soaked tile, and nothing on a
/// water tile catches. Used by `ignite_tile` and by carried torches.
pub fn ignite_fuel_at(world: &mut World, grid: &Grid, x: i32, y: i32) {
    if grid.water_positions.contains(&(x, y)) {
        return;
    }
    let wet = wet_positions(world).contains(&(x, y));
    let fuel: Vec<Entity> = world
        .query::<(&Position, &Fuel)>()
        .without::<&OnFire>()
        .iter()
        .filter(|(_, (p, f))| p.x == x && p.y == y && !(f.dampable && wet))
        .map(|(id, _)| id)
        .collect();
    for id in fuel {
        ignite_fuel(world, id);
    }
}

/// Splash of water centered at (cx, cy) with the given Chebyshev radius:
/// soaks every creature (Wet — which puts out Burning and washes off Oiled),
/// washes away unlit oil, douses every burning fuel (each as its
/// `Fuel::doused` says — grass fires go out and leave the grass, oil and webs
/// stay to be relit), soaks oil barrels so they can't catch fire for
/// `BARREL_SOAK_DURATION` (putting out a burning barrel's fuse), and soaks the
/// walkable tiles so damp-sensitive fuel can't catch for a while. Used by
/// thrown water flasks and the Druid's Call Rain.
///
/// Returns how many fires were put out (burning creatures, fuel and barrels).
pub fn splash_water(
    world: &mut World,
    grid: &Grid,
    cx: i32,
    cy: i32,
    radius: i32,
    events: &mut EventQueue,
) -> u32 {
    let in_radius =
        |x: i32, y: i32| (x - cx).abs() <= radius && (y - cy).abs() <= radius;
    let mut doused: u32 = 0;

    // Soak every creature in the splash (Wet extinguishes Burning). Oil
    // barrels are soaked separately below, for longer.
    let soaked: Vec<(Entity, bool)> = world
        .query::<(&Position, &StatusEffects)>()
        .without::<&OilBarrel>()
        .iter()
        .filter(|(_, (p, _))| in_radius(p.x, p.y))
        .map(|(id, (_, s))| (id, s.effects.iter().any(|e| e.effect_type == EffectType::Burning)))
        .collect();
    for (id, was_burning) in soaked {
        crate::systems::effects::add_effect_announced(world, events, id, EffectType::Wet, WET_DURATION);
        if was_burning {
            doused += 1;
        }
    }

    // Wash away unlit oil: water neutralises a spill before anyone lights it.
    // Collected before the dousing below, so a puddle put out by this same
    // splash survives as a re-ignitable puddle.
    let washed_oil: Vec<Entity> = world
        .query::<(&Position, &OilPuddle)>()
        .without::<&OnFire>()
        .iter()
        .filter(|(_, (p, _))| in_radius(p.x, p.y))
        .map(|(id, _)| id)
        .collect();
    for id in washed_oil {
        let _ = world.despawn(id);
    }

    // Soak oil barrels: Wet for BARREL_SOAK_DURATION, so fire cannot take
    // hold of them (Wet refuses Burning in `effects::add_effect`). A barrel
    // already burning has its fire fuse put out with the flames; a barrel
    // that was broken open keeps its break fuse (damage still works on a
    // soaked barrel, and a broken one still blows).
    let barrels: Vec<(Entity, (i32, i32))> = world
        .query::<(&Position, &OilBarrel)>()
        .iter()
        .filter(|(_, (p, _))| in_radius(p.x, p.y))
        .map(|(id, (p, _))| (id, (p.x, p.y)))
        .collect();
    for (id, position) in barrels {
        let fire_fuse = world
            .get::<&BarrelFuse>(id)
            .map(|f| f.cause == FuseCause::Fire)
            .unwrap_or(false);
        let was_burning =
            crate::systems::effects::entity_has_effect(world, id, EffectType::Burning);
        if fire_fuse {
            let _ = world.remove_one::<BarrelFuse>(id);
        }
        crate::systems::effects::add_effect_to_entity(world, id, EffectType::Wet, BARREL_SOAK_DURATION);
        let defused = fire_fuse || was_burning;
        if defused {
            doused += 1;
        }
        events.push(GameEvent::BarrelSoaked { barrel: id, position, defused });
    }

    // Douse burning fuel.
    let burning: Vec<Entity> = world
        .query::<(&Position, &OnFire)>()
        .iter()
        .filter(|(_, (p, _))| in_radius(p.x, p.y))
        .map(|(id, _)| id)
        .collect();
    for id in burning {
        douse_fuel(world, id);
        doused += 1;
    }

    // Soak the tiles in the radius: refresh existing timers, spawn new ones.
    let mut refreshed: HashSet<(i32, i32)> = HashSet::new();
    for (_, (pos, wet)) in world.query_mut::<(&Position, &mut WetTile)>() {
        if in_radius(pos.x, pos.y) {
            wet.remaining = WET_TILE_DURATION;
            refreshed.insert((pos.x, pos.y));
        }
    }
    for dy in -radius..=radius {
        for dx in -radius..=radius {
            let (x, y) = (cx + dx, cy + dy);
            if refreshed.contains(&(x, y))
                || !grid.is_walkable(x, y)
                || grid.water_positions.contains(&(x, y))
            {
                continue;
            }
            world.spawn((Position::new(x, y), WetTile { remaining: WET_TILE_DURATION }));
        }
    }
    doused
}

/// Topple a lit brazier: snuff its stand (light + hazard removed, sprite goes
/// to the tipped/unlit frame) and spill its coals onto its own tile plus one
/// random adjacent walkable tile — igniting grass and oil there and setting
/// anything standing there alight. Returns false if the entity isn't a lit
/// brazier.
pub fn topple_brazier(
    world: &mut World,
    grid: &Grid,
    brazier: Entity,
    events: &mut EventQueue,
    rng: &mut impl Rng,
) -> bool {
    let lit = world.get::<&Brazier>(brazier).map(|b| b.lit).unwrap_or(false);
    if !lit {
        return false;
    }
    let Some((bx, by)) = crate::queries::get_entity_position(world, brazier) else {
        return false;
    };

    snuff_brazier(world, brazier);

    events.push(GameEvent::BrazierToppled { position: (bx, by) });

    // Spill fire on the brazier's own tile plus one random adjacent walkable tile.
    let mut spill_tiles = vec![(bx, by)];
    let adjacent: Vec<(i32, i32)> = NEIGHBORS
        .iter()
        .map(|(dx, dy)| (bx + dx, by + dy))
        .filter(|&(x, y)| grid.is_walkable(x, y))
        .collect();
    if !adjacent.is_empty() {
        spill_tiles.push(adjacent[rng.gen_range(0..adjacent.len())]);
    }
    for (x, y) in spill_tiles {
        spill_fire_at(world, grid, x, y, events);
    }
    true
}

/// Smash a lantern: it's gone, and its oil spills burning across its tile
/// (and whoever is there is set alight). Returns false if the entity isn't a
/// lantern.
pub fn smash_lantern(
    world: &mut World,
    grid: &Grid,
    spatial_cache: &mut SpatialCache,
    lantern: Entity,
    events: &mut EventQueue,
) -> bool {
    if world.get::<&crate::components::Lantern>(lantern).is_err() {
        return false;
    }
    let Some((x, y)) = crate::queries::get_entity_position(world, lantern) else {
        return false;
    };
    // It carried BlocksMovement: out of the cache in the same breath as the
    // despawn, or its tile stays blocked for pathfinding.
    spatial_cache.remove_entity(lantern);
    let _ = world.despawn(lantern);
    events.push(GameEvent::LanternSmashed { position: (x, y) });

    if !grid.water_positions.contains(&(x, y)) {
        let oil = crate::spawning::spawn_oil_puddle(world, x, y);
        ignite_fuel(world, oil);
    }
    spill_fire_at(world, grid, x, y, events);
    true
}

/// Put a brazier into its toppled state: no more light, no more contact
/// burns, static tipped sprite. Only the state change — no event, no spilled
/// fire — so a revisited floor can restore a brazier that was toppled earlier.
pub fn snuff_brazier(world: &mut World, brazier: Entity) {
    if let Ok(mut b) = world.get::<&mut Brazier>(brazier) {
        b.lit = false;
    }
    let _ = world.remove::<(LightSource, CausesBurning, AnimatedSprite)>(brazier);
    let _ = world.insert_one(brazier, Sprite::from_ref(tile_ids::BRAZIER_UNLIT));
}

/// Spill fire onto a tile: set the tile alight (`ignite_tile`) and set
/// anything standing there burning. Used by brazier topples and fire traps.
pub fn spill_fire_at(world: &mut World, grid: &Grid, x: i32, y: i32, events: &mut EventQueue) {
    if grid.water_positions.contains(&(x, y)) {
        return;
    }
    ignite_tile(world, grid, x, y);

    // Anything standing there catches fire (a flyer hovers above it).
    let victims: Vec<Entity> = world
        .query::<(&Position, &StatusEffects)>()
        .iter()
        .filter(|(_, (p, _))| p.x == x && p.y == y)
        .map(|(id, _)| id)
        .collect();
    for id in victims {
        if !crate::systems::tile_effects::touches_ground(world, id) {
            continue;
        }
        // Announce only a fresh ignition: a Wet victim refuses the fire, and
        // one already alight has already been told.
        let was_burning = crate::systems::effects::entity_has_effect(world, id, EffectType::Burning);
        if crate::systems::effects::add_effect_to_entity(world, id, EffectType::Burning, BURNING_DURATION)
            && !was_burning
        {
            events.push(GameEvent::CaughtFire { entity: id, position: (x, y) });
        }
    }
}

/// An oil barrel just took a hit (called from the `apply_damage` chokepoint,
/// so melee, arrows, Grave Bolt, explosions, fireballs and Shield Bash all
/// land here).
///
/// - Fuse already running (it is burning, or was broken open earlier): the
///   hit sets it off. The fuse is zeroed, and the barrel blows on this frame's
///   fire tick (`detonate_ready_barrels`), which is the next point at which
///   the grid and spatial cache are to hand.
/// - No fuse, and the hit took it to 0 HP: it cracks open and its break fuse
///   (`OIL_BARREL_BREAK_FUSE_SECONDS`) starts.
///
/// No-op for anything that is not an oil barrel.
pub fn on_barrel_damaged(world: &mut World, barrel: Entity, events: &mut EventQueue) {
    if world.get::<&OilBarrel>(barrel).is_err() {
        return;
    }
    if let Ok(mut fuse) = world.get::<&mut BarrelFuse>(barrel) {
        fuse.remaining = 0.0;
        return;
    }
    let broken = world.get::<&Health>(barrel).map(|h| h.current <= 0).unwrap_or(false);
    if broken {
        start_break_fuse(world, barrel, events);
    }
}

/// Light a broken barrel's fuse and announce it ("The barrel cracks and
/// starts to hiss!"). No-op if a fuse is already running.
fn start_break_fuse(world: &mut World, barrel: Entity, events: &mut EventQueue) {
    if world.get::<&BarrelFuse>(barrel).is_ok() {
        return;
    }
    let _ = world.insert_one(
        barrel,
        BarrelFuse::new(OIL_BARREL_BREAK_FUSE_SECONDS, FuseCause::Broken),
    );
    if let Some(position) = crate::queries::get_entity_position(world, barrel) {
        events.push(GameEvent::BarrelCracked { barrel, position });
    }
}

/// Explode every oil barrel whose fuse has run out, repeating until none is
/// left, so a blast that hits another fusing barrel sets it off in the same
/// pass (any damage to a fusing barrel detonates it).
pub fn detonate_ready_barrels(
    world: &mut World,
    grid: &Grid,
    spatial_cache: &mut SpatialCache,
    events: &mut EventQueue,
    rng: &mut impl Rng,
) {
    loop {
        let mut ready: Vec<Entity> = world
            .query::<(&OilBarrel, &BarrelFuse)>()
            .iter()
            .filter(|(_, (_, f))| f.remaining <= 0.0)
            .map(|(id, _)| id)
            .collect();
        if ready.is_empty() {
            return;
        }
        // Query order is archetype order; sort so a seed replays identically.
        ready.sort_unstable_by_key(|e| e.to_bits());
        for id in ready {
            if world.contains(id) {
                explode_barrel(world, grid, spatial_cache, id, events, rng);
            }
        }
    }
}

/// Detonate an oil barrel: damage in a radius (through `apply_damage`), a
/// spray of already-burning oil puddles over walkable tiles around it,
/// explosion feedback (reuses the fireball explosion VFX/audio event), and a
/// wide noise wake. Nearby oil barrels are set burning (chain reactions).
fn explode_barrel(
    world: &mut World,
    grid: &Grid,
    spatial_cache: &mut SpatialCache,
    barrel: Entity,
    events: &mut EventQueue,
    rng: &mut impl Rng,
) {
    // The barrel carries BlocksMovement: drop it from the spatial cache in the
    // same breath as the despawn, or its tile stays blocked for AI pathfinding
    // until the next full rebuild on floor transition.
    let Some((bx, by)) = crate::queries::get_entity_position(world, barrel) else {
        spatial_cache.remove_entity(barrel);
        let _ = world.despawn(barrel);
        return;
    };
    spatial_cache.remove_entity(barrel);
    let _ = world.despawn(barrel);

    events.push(GameEvent::BarrelExploded { position: (bx, by) });
    events.push(GameEvent::FireballExplosion {
        x: bx,
        y: by,
        radius: OIL_BARREL_EXPLOSION_RADIUS,
    });

    // The blast is deafening — wake enemies over a wide area.
    crate::systems::noise::make_noise(world, grid, (bx, by), EXPLOSION_NOISE_RADIUS);

    // Damage everything attackable in the blast radius.
    let victims: Vec<(Entity, (i32, i32))> = world
        .query::<(&Position, &crate::components::Attackable)>()
        .iter()
        .filter(|(_, (p, _))| {
            (p.x - bx).abs() <= OIL_BARREL_EXPLOSION_RADIUS
                && (p.y - by).abs() <= OIL_BARREL_EXPLOSION_RADIUS
        })
        .map(|(id, (p, _))| (id, (p.x, p.y)))
        .collect();
    for (id, (x, y)) in victims {
        let dealt =
            crate::systems::combat::apply_damage(world, grid, id, OIL_BARREL_EXPLOSION_DAMAGE, rng, events);
        if dealt > 0 {
            events.push(GameEvent::BurnDamage {
                entity: id,
                position: (x as f32 + 0.5, y as f32 + 0.5),
                damage: dealt,
            });
        }
    }

    // Chain reaction: other barrels in the blast start burning (fuse next tick).
    let chained: Vec<Entity> = world
        .query::<(&Position, &OilBarrel)>()
        .iter()
        .filter(|(_, (p, _))| {
            (p.x - bx).abs() <= OIL_BARREL_EXPLOSION_RADIUS
                && (p.y - by).abs() <= OIL_BARREL_EXPLOSION_RADIUS
        })
        .map(|(id, _)| id)
        .collect();
    for id in chained {
        crate::systems::effects::add_effect_to_entity(world, id, EffectType::Burning, BURNING_DURATION);
    }

    // The spray coats everything nearby in oil before it lights.
    let sprayed: Vec<Entity> = world
        .query::<(&Position, &StatusEffects)>()
        .iter()
        .filter(|(_, (p, _))| {
            (p.x - bx).abs() <= OIL_BARREL_PUDDLE_RADIUS
                && (p.y - by).abs() <= OIL_BARREL_PUDDLE_RADIUS
        })
        .map(|(id, _)| id)
        .collect();
    for id in sprayed {
        crate::systems::effects::add_effect_announced(world, events, id, EffectType::Oiled, OILED_DURATION);
    }

    // Spray burning oil over the surrounding tiles (partial coverage). Skips
    // water, blocked tiles, and tiles that already hold a puddle; also lights
    // any existing puddle caught in the spray.
    let existing_puddles: HashSet<(i32, i32)> = world
        .query::<(&Position, &OilPuddle)>()
        .iter()
        .map(|(_, (p, _))| (p.x, p.y))
        .collect();
    let blocked: HashSet<(i32, i32)> = world
        .query::<(&Position, &crate::components::BlocksMovement)>()
        .iter()
        .map(|(_, (p, _))| (p.x, p.y))
        .collect();

    let mut new_puddles: Vec<(i32, i32)> = Vec::new();
    for dy in -OIL_BARREL_PUDDLE_RADIUS..=OIL_BARREL_PUDDLE_RADIUS {
        for dx in -OIL_BARREL_PUDDLE_RADIUS..=OIL_BARREL_PUDDLE_RADIUS {
            let dist = dx.abs().max(dy.abs());
            if !(1..=OIL_BARREL_PUDDLE_RADIUS).contains(&dist) {
                continue;
            }
            let (x, y) = (bx + dx, by + dy);
            if !grid.is_walkable(x, y)
                || grid.water_positions.contains(&(x, y))
                || existing_puddles.contains(&(x, y))
                || blocked.contains(&(x, y))
            {
                continue;
            }
            if rng.gen_bool(OIL_BARREL_PUDDLE_COVERAGE) {
                new_puddles.push((x, y));
            }
        }
    }
    for (x, y) in new_puddles {
        let puddle = crate::spawning::spawn_oil_puddle(world, x, y);
        ignite_fuel(world, puddle);
    }

    // Existing puddles caught in the spray radius ignite too.
    let caught: Vec<Entity> = world
        .query::<(&Position, &OilPuddle)>()
        .without::<&OnFire>()
        .iter()
        .filter(|(_, (p, _))| {
            (p.x - bx).abs() <= OIL_BARREL_PUDDLE_RADIUS
                && (p.y - by).abs() <= OIL_BARREL_PUDDLE_RADIUS
        })
        .map(|(id, _)| id)
        .collect();
    for id in caught {
        ignite_fuel(world, id);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::components::Combustible;
    use crate::tile::Tile;

    fn make_grid(width: usize, height: usize, tile: TileType) -> Grid {
        Grid {
            width,
            height,
            tiles: vec![Tile::new(tile); width * height],
            chest_positions: vec![],
            door_positions: vec![],
            brazier_positions: vec![],
            decals: vec![],
            stairs_up_pos: None,
            stairs_down_pos: None,
            starting_room: None,
            illumination: vec![0.0; width * height],
            themed_rooms: vec![],
            water_positions: vec![],
            coffin_positions: vec![],
            barrel_positions: vec![],
            shop_position: None,
            shop_decor_positions: vec![],
            trap_positions: vec![],
            furniture_positions: vec![],
            secret_room: None,
            secret_door_pos: None,
            stalagmite_positions: Vec::new(),
            mushroom_positions: Vec::new(),
            crystal_positions: Vec::new(),
        }
    }

    fn run_fire(world: &mut World, grid: &mut Grid, seconds: f32) {
        let mut cache = SpatialCache::rebuild_from_world(world);
        run_fire_cached(world, grid, &mut cache, seconds);
    }

    /// As `run_fire`, but from a seeded rng, for tests that assert on a chain
    /// of per-step ignition rolls rather than on a near-certain outcome.
    fn run_fire_seeded(world: &mut World, grid: &mut Grid, seconds: f32, seed: u64) {
        use rand::SeedableRng;

        let mut cache = SpatialCache::rebuild_from_world(world);
        let mut rng = rand::rngs::StdRng::seed_from_u64(seed);
        let mut events = EventQueue::new();
        let mut acc = 0.0;
        let mut fov_dirty = false;
        let mut remaining = seconds;
        while remaining > 0.0 {
            let dt = remaining.min(FIRE_STEP_INTERVAL);
            tick_fire(
                &mut EffectCtx {
                    world,
                    grid,
                    spatial: &mut cache,
                    events: &mut events,
                    rng: &mut rng,
                },
                dt,
                &mut acc,
                &mut fov_dirty,
            );
            remaining -= dt;
        }
    }

    /// As `run_fire`, but against a caller-owned spatial cache so a test can
    /// assert the cache stayed coherent across the fire tick.
    fn run_fire_cached(
        world: &mut World,
        grid: &mut Grid,
        cache: &mut SpatialCache,
        seconds: f32,
    ) {
        let mut events = EventQueue::new();
        let mut acc = 0.0;
        let mut fov_dirty = false;
        // Fresh entropy per call, as `thread_rng` gave before `EffectCtx` fixed
        // the rng type: these helpers drive probabilistic spread tests that
        // loop until fire catches, so the stream must differ between steps.
        // `run_fire_seeded` is the deterministic variant.
        let mut rng = {
            use rand::SeedableRng;
            rand::rngs::StdRng::from_entropy()
        };
        // Advance in FIRE_STEP_INTERVAL chunks so spread steps actually run.
        let mut remaining = seconds;
        while remaining > 0.0 {
            let dt = remaining.min(FIRE_STEP_INTERVAL);
            tick_fire(
                &mut EffectCtx {
                    world,
                    grid,
                    spatial: cache,
                    events: &mut events,
                    rng: &mut rng,
                },
                dt,
                &mut acc,
                &mut fov_dirty,
            );
            remaining -= dt;
        }
    }

    #[test]
    fn test_exploded_barrel_leaves_no_phantom_blocking_tile() {
        // An oil barrel carries BlocksMovement. When it detonates it is
        // despawned, so its tile must drop out of the spatial cache too —
        // otherwise AI pathfinding routes around empty floor until the next
        // full rebuild on floor transition.
        let mut world = World::new();
        let mut grid = make_grid(9, 9, TileType::Floor);

        let barrel = crate::spawning::spawn_oil_barrel(&mut world, 4, 4);
        let mut cache = SpatialCache::rebuild_from_world(&world);
        assert!(cache.is_blocked((4, 4)), "intact barrel should block its tile");

        crate::systems::effects::add_effect_to_entity(
            &mut world,
            barrel,
            EffectType::Burning,
            BURNING_DURATION,
        );

        run_fire_cached(&mut world, &mut grid, &mut cache, 0.5);
        assert!(world.contains(barrel), "barrel must survive the fuse period");
        cache.assert_coherent_with_world(&world, "barrel fuse lit");

        run_fire_cached(
            &mut world,
            &mut grid,
            &mut cache,
            OIL_BARREL_FUSE_SECONDS + 0.5,
        );
        assert!(!world.contains(barrel), "barrel should have exploded");

        assert!(
            !cache.is_blocked((4, 4)),
            "exploded barrel's tile must no longer be blocked — \
             this is the phantom blocker regression"
        );
        cache.assert_coherent_with_world(&world, "after barrel explosion");
    }

    #[test]
    fn test_fire_tick_keeps_spatial_cache_coherent() {
        // Broader coherence guard over the whole fire tick: grass burnout, oil
        // burnout, web burnout and a chained barrel detonation all despawn
        // entities. None of them may leave the cache disagreeing with a fresh
        // rebuild.
        let mut world = World::new();
        let mut grid = make_grid(13, 13, TileType::Floor);

        // Two adjacent barrels so the first detonation chains into the second.
        let barrel_a = crate::spawning::spawn_oil_barrel(&mut world, 6, 6);
        crate::spawning::spawn_oil_barrel(&mut world, 8, 6);
        // Non-blocking fire furniture that also gets despawned during the tick.
        crate::spawning::spawn_fire_patch(&mut world, 2, 2);
        crate::spawning::spawn_oil_puddle(&mut world, 3, 9);
        crate::spawning::spawn_web(&mut world, 10, 10, None);

        let mut cache = SpatialCache::rebuild_from_world(&world);
        cache.assert_coherent_with_world(&world, "initial build");

        crate::systems::effects::add_effect_to_entity(
            &mut world,
            barrel_a,
            EffectType::Burning,
            BURNING_DURATION,
        );

        // Step through in small slices, checking coherence at every step so a
        // failure points at the tick that broke it.
        for step in 0..40 {
            run_fire_cached(&mut world, &mut grid, &mut cache, FIRE_STEP_INTERVAL);
            cache.assert_coherent_with_world(&world, &format!("fire step {step}"));
        }

        assert!(!world.contains(barrel_a), "the lit barrel should be gone");
    }

    #[test]
    fn test_barrel_fuse_then_explosion_spawns_burning_puddles() {
        let mut world = World::new();
        let mut grid = make_grid(9, 9, TileType::Floor);

        let barrel = crate::spawning::spawn_oil_barrel(&mut world, 4, 4);
        crate::systems::effects::add_effect_to_entity(
            &mut world,
            barrel,
            EffectType::Burning,
            BURNING_DURATION,
        );

        // First tick lights the fuse; the barrel must still exist.
        run_fire(&mut world, &mut grid, 0.5);
        assert!(world.get::<&BarrelFuse>(barrel).is_ok(), "fuse should be lit");
        assert!(world.contains(barrel), "barrel must survive the fuse period");

        // After the fuse runs out the barrel is gone and burning oil surrounds
        // the blast site (coverage is random but ~60% of 24 tiles; zero would
        // be a ~1e-10 fluke).
        run_fire(&mut world, &mut grid, OIL_BARREL_FUSE_SECONDS);
        assert!(!world.contains(barrel), "barrel should have exploded");

        let burning_puddles = world.query::<(&OilPuddle, &OnFire)>().iter().count();
        assert!(
            burning_puddles > 0,
            "explosion should spray at least one ignited oil puddle"
        );
    }

    /// A barrel brought to 0 HP outside `apply_damage` (burn ticks subtract
    /// health directly) still cracks and runs the break fuse, rather than
    /// blowing at once or lingering forever.
    #[test]
    fn test_broken_barrel_runs_the_break_fuse() {
        let mut world = World::new();
        let mut grid = make_grid(7, 7, TileType::Floor);

        let barrel = crate::spawning::spawn_oil_barrel(&mut world, 3, 3);
        if let Ok(mut health) = world.get::<&mut Health>(barrel) {
            health.current = 0;
        }

        run_fire(&mut world, &mut grid, 0.1);
        assert!(world.contains(barrel), "a broken barrel does not blow at once");
        let fuse = *world.get::<&BarrelFuse>(barrel).expect("break fuse lit");
        assert_eq!(fuse.cause, FuseCause::Broken);
        assert!((fuse.total - OIL_BARREL_BREAK_FUSE_SECONDS).abs() < 1e-5);

        run_fire(&mut world, &mut grid, OIL_BARREL_BREAK_FUSE_SECONDS);
        assert!(!world.contains(barrel), "the break fuse runs out and it explodes");
    }

    /// One seeded fire tick of `dt` game seconds; returns the events.
    fn tick(world: &mut World, grid: &mut Grid, cache: &mut SpatialCache, dt: f32) -> Vec<GameEvent> {
        use rand::SeedableRng;
        let mut events = EventQueue::new();
        let mut rng = rand::rngs::StdRng::seed_from_u64(3);
        let (mut acc, mut fov_dirty) = (0.0, false);
        tick_fire(
            &mut EffectCtx { world, grid, spatial: cache, events: &mut events, rng: &mut rng },
            dt,
            &mut acc,
            &mut fov_dirty,
        );
        events.drain().collect()
    }

    fn hit(world: &mut World, target: Entity, raw: i32) -> Vec<GameEvent> {
        use rand::SeedableRng;
        let mut events = EventQueue::new();
        let mut rng = rand::rngs::StdRng::seed_from_u64(5);
        let grid = Grid::filled(16, 16, TileType::Floor);
        crate::systems::combat::apply_damage(world, &grid, target, raw, &mut rng, &mut events);
        events.drain().collect()
    }

    /// Breaking a barrel starts its 2s fuse (announced, telegraphed); a second
    /// hit while it hisses sets it off at once, long before the fuse is out.
    #[test]
    fn a_broken_barrel_hisses_and_the_next_hit_detonates_it() {
        let mut world = World::new();
        let mut grid = make_grid(9, 9, TileType::Floor);
        let barrel = crate::spawning::spawn_oil_barrel(&mut world, 4, 4);
        let mut cache = SpatialCache::rebuild_from_world(&world);

        let seen = hit(&mut world, barrel, OIL_BARREL_HEALTH + 5);
        assert!(seen.iter().any(|e| matches!(e, GameEvent::BarrelCracked { barrel: b, .. } if *b == barrel)));
        let mut log = crate::ui::MessageLog::new(Entity::DANGLING);
        for ev in &seen {
            log.record_event(ev, &world);
        }
        assert!(log.lines().iter().any(|l| l == "The barrel cracks and starts to hiss!"), "{:?}", log.lines());

        // Half a second in: still standing, telegraphed, still a melee target.
        let events = tick(&mut world, &mut grid, &mut cache, 0.5);
        assert!(world.contains(barrel));
        assert!(!events.iter().any(|e| matches!(e, GameEvent::BarrelExploded { .. })));
        let t = crate::systems::telegraph::barrel_fuse_telegraphs(&world);
        assert_eq!(t.len(), 1);
        assert_eq!(
            t[0].shape,
            crate::systems::telegraph::TelegraphShape::Area {
                center: (4, 4),
                radius: OIL_BARREL_EXPLOSION_RADIUS
            }
        );
        let expected = 0.5 / OIL_BARREL_BREAK_FUSE_SECONDS;
        assert!((t[0].progress - expected).abs() < 1e-4, "progress {}", t[0].progress);
        assert!((t[0].remaining - (OIL_BARREL_BREAK_FUSE_SECONDS - 0.5)).abs() < 1e-4);
        let attacker = world.spawn((Position::new(4, 5),));
        assert!(crate::systems::actions::melee_reach_check(&world, attacker, barrel).is_ok());

        // The second hit: it goes off on the very next fire tick, even a
        // zero-length one.
        hit(&mut world, barrel, 1);
        let events = tick(&mut world, &mut grid, &mut cache, 0.0);
        assert!(!world.contains(barrel), "the second hit detonates it immediately");
        assert!(events.iter().any(|e| matches!(e, GameEvent::BarrelExploded { position: (4, 4) })));
        assert!(!cache.is_blocked((4, 4)));
    }

    /// The dead-entity sweep leaves a broken barrel standing (it must stay
    /// targetable while its fuse runs).
    #[test]
    fn the_dead_sweep_leaves_a_broken_barrel_alone() {
        let mut arena = crate::systems::actions::TestArena::new((2, 2));
        let barrel = crate::spawning::spawn_oil_barrel(&mut arena.world, 6, 6);
        hit(&mut arena.world, barrel, 50);
        crate::systems::combat::remove_dead_entities(&mut arena.ctx(), 0);
        assert!(arena.world.contains(barrel));
        assert!(arena.world.get::<&crate::components::Attackable>(barrel).is_ok());
        assert!(arena.world.get::<&BarrelFuse>(barrel).is_ok());
    }

    /// A burning barrel's fire fuse is also cut short by any hit.
    #[test]
    fn a_hit_on_a_burning_barrel_detonates_it() {
        let mut world = World::new();
        let mut grid = make_grid(9, 9, TileType::Floor);
        let barrel = crate::spawning::spawn_oil_barrel(&mut world, 4, 4);
        let mut cache = SpatialCache::rebuild_from_world(&world);
        crate::systems::effects::add_effect_to_entity(&mut world, barrel, EffectType::Burning, BURNING_DURATION);
        tick(&mut world, &mut grid, &mut cache, 0.1);
        let fuse = *world.get::<&BarrelFuse>(barrel).expect("fire fuse lit");
        assert_eq!(fuse.cause, FuseCause::Fire);
        assert!(fuse.remaining > 2.0);

        let seen = hit(&mut world, barrel, 1);
        assert!(!seen.iter().any(|e| matches!(e, GameEvent::BarrelCracked { .. })));
        tick(&mut world, &mut grid, &mut cache, 0.0);
        assert!(!world.contains(barrel), "the hit set the burning barrel off");
    }

    /// A blast that catches a fusing barrel sets it off in the same pass; one
    /// that catches an intact barrel breaks it (2s fuse) instead.
    #[test]
    fn an_explosion_detonates_fusing_neighbours_and_cracks_intact_ones() {
        let mut world = World::new();
        let mut grid = make_grid(12, 9, TileType::Floor);
        let a = crate::spawning::spawn_oil_barrel(&mut world, 3, 4);
        let fusing = crate::spawning::spawn_oil_barrel(&mut world, 4, 4);
        let intact = crate::spawning::spawn_oil_barrel(&mut world, 2, 4);
        let mut cache = SpatialCache::rebuild_from_world(&world);
        let _ = world.insert_one(fusing, BarrelFuse::new(OIL_BARREL_FUSE_SECONDS, FuseCause::Fire));

        hit(&mut world, a, OIL_BARREL_HEALTH);
        hit(&mut world, a, 1);
        tick(&mut world, &mut grid, &mut cache, 0.0);
        assert!(!world.contains(a));
        assert!(!world.contains(fusing), "the fusing neighbour went up with it");
        assert!(world.contains(intact), "the intact neighbour was only broken");
        assert_eq!(world.get::<&BarrelFuse>(intact).map(|f| f.cause).ok(), Some(FuseCause::Broken));
    }

    /// Water washes away unlit oil (lit oil is doused and stays), and soaks
    /// barrels so fire cannot take them; a burning barrel's fuse goes out.
    #[test]
    fn splash_washes_unlit_oil_and_soaks_barrels() {
        let mut world = World::new();
        let mut grid = make_grid(12, 9, TileType::Floor);
        let mut events = EventQueue::new();
        let unlit = crate::spawning::spawn_oil_puddle(&mut world, 3, 3);
        let lit = crate::spawning::spawn_oil_puddle(&mut world, 4, 3);
        ignite_fuel(&mut world, lit);
        let outside = crate::spawning::spawn_oil_puddle(&mut world, 9, 3);
        let cold = crate::spawning::spawn_oil_barrel(&mut world, 3, 4);
        let burning = crate::spawning::spawn_oil_barrel(&mut world, 4, 4);
        crate::systems::effects::add_effect_to_entity(&mut world, burning, EffectType::Burning, BURNING_DURATION);
        let _ = world.insert_one(burning, BarrelFuse::new(OIL_BARREL_FUSE_SECONDS, FuseCause::Fire));

        let doused = splash_water(&mut world, &grid, 3, 3, 1, &mut events);
        assert!(!world.contains(unlit), "unlit oil is washed away");
        assert!(world.contains(lit) && world.get::<&OnFire>(lit).is_err(), "lit oil is doused, kept");
        assert!(world.contains(outside));
        assert!(world.get::<&BarrelFuse>(burning).is_err(), "the fire fuse is out");
        assert!(doused >= 2, "the lit puddle and the burning barrel count, got {doused}");
        let seen: Vec<GameEvent> = events.drain().collect();
        assert!(seen.iter().any(|e| matches!(e, GameEvent::BarrelSoaked { barrel, defused: true, .. } if *barrel == burning)));
        assert!(seen.iter().any(|e| matches!(e, GameEvent::BarrelSoaked { barrel, defused: false, .. } if *barrel == cold)));

        // Fire right next to both barrels, for a long while: neither catches.
        let fire = crate::spawning::spawn_oil_puddle(&mut world, 3, 5);
        ignite_fuel(&mut world, fire);
        spill_fire_at(&mut world, &grid, 3, 4, &mut events);
        run_fire_seeded(&mut world, &mut grid, 5.0, 9);
        for b in [cold, burning] {
            assert!(world.contains(b));
            assert!(world.get::<&BarrelFuse>(b).is_err(), "a soaked barrel does not light");
            assert!(!crate::systems::effects::entity_has_effect(&world, b, EffectType::Burning));
        }

        // Damage still works on a soaked barrel, and its break fuse still blows.
        hit(&mut world, cold, OIL_BARREL_HEALTH);
        assert_eq!(world.get::<&BarrelFuse>(cold).map(|f| f.cause).ok(), Some(FuseCause::Broken));
        run_fire(&mut world, &mut grid, OIL_BARREL_BREAK_FUSE_SECONDS + 0.2);
        assert!(!world.contains(cold), "a soaked barrel still explodes once broken");
    }

    /// How many of `n` seeded runs set a creature standing in burning oil
    /// alight within `seconds`, for a creature built by `make`.
    fn ignitions_in_burning_oil(n: u64, seconds: f32, make: impl Fn(&mut World) -> Entity) -> u64 {
        (0..n)
            .filter(|&seed| {
                let mut world = World::new();
                let mut grid = make_grid(7, 7, TileType::Floor);
                let puddle = crate::spawning::spawn_oil_puddle(&mut world, 3, 3);
                ignite_fuel(&mut world, puddle);
                let victim = make(&mut world);
                run_fire_seeded(&mut world, &mut grid, seconds, seed);
                crate::systems::effects::entity_has_effect(&world, victim, EffectType::Burning)
            })
            .count() as u64
    }

    /// A Wet creature standing in burning oil never catches, however long it
    /// stands there.
    #[test]
    fn wet_creatures_never_ignite() {
        let caught = ignitions_in_burning_oil(20, 3.0, |w| {
            let mut s = StatusEffects::new();
            crate::systems::effects::add_effect(&mut s, EffectType::Wet, 60.0);
            w.spawn((Position::new(3, 3), s, Combustible { flammability: 1.0 }))
        });
        assert_eq!(caught, 0);
    }

    /// A flyer hovering over burning oil is above the flames: it never
    /// catches from the puddle under it (a walker in the same spot does).
    #[test]
    fn flyers_over_burning_oil_never_ignite() {
        let make = |flying: bool| {
            move |w: &mut World| {
                let e = w.spawn((Position::new(3, 3), StatusEffects::new(), Combustible { flammability: 1.0 }));
                if flying {
                    let _ = w.insert_one(e, crate::components::Flying);
                }
                e
            }
        };
        assert_eq!(ignitions_in_burning_oil(20, 3.0, make(true)), 0);
        assert!(ignitions_in_burning_oil(20, 3.0, make(false)) > 0, "control: walkers burn");
    }

    /// Oil makes anything burn: a non-`Combustible` creature (a skeleton, a
    /// slime) never ignites in burning oil, but the same creature Oiled does,
    /// and a barely flammable one catches far more often once Oiled.
    #[test]
    fn oiled_creatures_ignite_far_more_readily() {
        let fireproof = |oiled: bool| {
            move |w: &mut World| {
                let mut s = StatusEffects::new();
                if oiled {
                    crate::systems::effects::add_effect(&mut s, EffectType::Oiled, 60.0);
                }
                w.spawn((Position::new(3, 3), s))
            }
        };
        assert_eq!(ignitions_in_burning_oil(20, 1.0, fireproof(false)), 0);
        assert!(ignitions_in_burning_oil(20, 1.0, fireproof(true)) >= 15);

        // One spread step (0.5s) at low flammability: 0.6*0.1 = 6% dry,
        // 0.6*0.5*3 = 90% oiled.
        let barely = |oiled: bool| {
            move |w: &mut World| {
                let mut s = StatusEffects::new();
                if oiled {
                    crate::systems::effects::add_effect(&mut s, EffectType::Oiled, 60.0);
                }
                w.spawn((Position::new(3, 3), s, Combustible { flammability: 0.1 }))
            }
        };
        let dry = ignitions_in_burning_oil(40, FIRE_STEP_INTERVAL, barely(false));
        let oily = ignitions_in_burning_oil(40, FIRE_STEP_INTERVAL, barely(true));
        assert!(oily > dry + 20, "oiled {oily}/40 vs dry {dry}/40");
    }

    /// Burning while Oiled hurts more per tick.
    #[test]
    fn oiled_burning_takes_extra_damage() {
        let mut world = World::new();
        let mut events = EventQueue::new();
        let spawn = |w: &mut World, oiled: bool| {
            let mut s = StatusEffects::new();
            if oiled {
                crate::systems::effects::add_effect(&mut s, EffectType::Oiled, 60.0);
            }
            crate::systems::effects::add_effect(&mut s, EffectType::Burning, 60.0);
            w.spawn((Position::new(1, 1), Health::new(100), s))
        };
        let dry = spawn(&mut world, false);
        let oily = spawn(&mut world, true);
        crate::time_system::tick_burn_damage(&mut world, 5.0, &mut events);
        let lost = |e| 100 - world.get::<&Health>(e).unwrap().current;
        assert_eq!(lost(dry), BURNING_DAMAGE_PER_SECOND);
        assert_eq!(
            lost(oily),
            (BURNING_DAMAGE_PER_SECOND as f32 * OILED_BURN_DAMAGE_MULT).round() as i32
        );
    }

    /// Standing in water keeps a creature Wet; standing in unlit oil keeps it
    /// Oiled — both refreshed by the fire tick, on game time.
    #[test]
    fn standing_in_water_or_oil_keeps_the_status_up() {
        let mut world = World::new();
        let mut grid = make_grid(7, 7, TileType::Floor);
        grid.water_positions.push((1, 1));
        crate::spawning::spawn_oil_puddle(&mut world, 5, 5);
        let swimmer = world.spawn((Position::new(1, 1), StatusEffects::new()));
        let slick = world.spawn((Position::new(5, 5), StatusEffects::new()));

        // Long past both durations: the statuses tick down between fire
        // ticks, the standing refresh tops them back up.
        for _ in 0..40 {
            crate::time_system::tick_status_effects(&mut world, 1.0);
            run_fire_seeded(&mut world, &mut grid, 1.0, 1);
        }
        assert!(crate::systems::effects::entity_has_effect(&world, swimmer, EffectType::Wet));
        assert!(crate::systems::effects::entity_has_effect(&world, slick, EffectType::Oiled));
    }

    /// A water splash soaks creatures as well as putting them out.
    #[test]
    fn splash_soaks_creatures_and_washes_off_oil() {
        let mut world = World::new();
        let grid = make_grid(7, 7, TileType::Floor);
        let mut s = StatusEffects::new();
        crate::systems::effects::add_effect(&mut s, EffectType::Oiled, 60.0);
        let victim = world.spawn((Position::new(3, 3), s));
        let mut events = EventQueue::new();
        splash_water(&mut world, &grid, 3, 3, WATER_SPLASH_RADIUS, &mut events);
        assert!(crate::systems::effects::entity_has_effect(&world, victim, EffectType::Wet));
        assert!(!crate::systems::effects::entity_has_effect(&world, victim, EffectType::Oiled));
    }

    #[test]
    fn test_wet_grass_blocks_ignition() {
        let mut world = World::new();
        let mut grid = make_grid(5, 5, TileType::Floor);
        // A dry fire source at (2,2) next to a grass tile at (2,3).
        if let Some(t) = grid.get_mut(2, 3) {
            t.tile_type = TileType::TallGrass;
        }
        crate::spawning::spawn_fire_patch(&mut world, 2, 2);
        // Soak the grass tile.
        world.spawn((Position::new(2, 3), WetTile { remaining: 1000.0 }));

        // Many spread steps: at 22%/step dry grass would all but certainly
        // catch; wet grass must never.
        run_fire(&mut world, &mut grid, GRASS_BURN_DURATION - 1.0);

        let caught = grass_fire_tiles(&world).contains(&(2, 3));
        assert!(!caught, "wet grass must not ignite");
    }

    #[test]
    fn test_fire_spreads_into_adjacent_oil() {
        let mut world = World::new();
        let mut grid = make_grid(5, 5, TileType::Floor);
        // Grass fire at (2,2), oil puddle beside it at (3,2).
        if let Some(t) = grid.get_mut(2, 2) {
            t.tile_type = TileType::TallGrass;
        }
        crate::spawning::spawn_fire_patch(&mut world, 2, 2);
        let puddle = crate::spawning::spawn_oil_puddle(&mut world, 3, 2);

        // 90% per step over many steps: failure odds are astronomically small.
        run_fire(&mut world, &mut grid, 5.0);

        assert!(
            world.get::<&OnFire>(puddle).is_ok(),
            "oil adjacent to fire should ignite"
        );
    }

    #[test]
    fn test_water_splash_douses_but_keeps_puddle_and_grass() {
        let mut world = World::new();
        let mut grid = make_grid(7, 7, TileType::Floor);
        if let Some(t) = grid.get_mut(3, 4) {
            t.tile_type = TileType::TallGrass;
        }

        // Burning oil puddle at (3,3), burning grass at (3,4), burning victim at (4,3).
        let puddle = crate::spawning::spawn_oil_puddle(&mut world, 3, 3);
        ignite_fuel(&mut world, puddle);
        crate::spawning::spawn_fire_patch(&mut world, 3, 4);
        let victim = world.spawn((Position::new(4, 3), {
            let mut s = StatusEffects::new();
            crate::systems::effects::add_effect(&mut s, EffectType::Burning, BURNING_DURATION);
            s
        }));

        splash_water(&mut world, &grid, 3, 3, WATER_SPLASH_RADIUS, &mut EventQueue::new());

        // Fires are out ...
        assert!(world.get::<&OnFire>(puddle).is_err(), "oil fire doused");
        assert_eq!(
            grass_fire_tiles(&world).len(),
            0,
            "grass fire doused"
        );
        let still_burning = world
            .get::<&StatusEffects>(victim)
            .map(|s| s.effects.iter().any(|e| e.effect_type == EffectType::Burning))
            .unwrap_or(true);
        assert!(!still_burning, "burning entity doused");

        // ... but the fuel remains: puddle still there, grass tile unburnt & wet.
        assert!(world.get::<&OilPuddle>(puddle).is_ok(), "puddle survives dousing");
        assert_eq!(
            grid.get(3, 4).map(|t| t.tile_type),
            Some(TileType::TallGrass),
            "doused grass stays as grass"
        );
        let wet = wet_positions(&world).contains(&(3, 4));
        assert!(wet, "splashed grass is wet");
    }

    #[test]
    fn test_glow_mushrooms_burn_away_and_carry_the_fire() {
        let mut world = World::new();
        let mut grid = make_grid(9, 9, TileType::Floor);

        // A fire on a grass tile, with a run of fungus leading away from it.
        if let Some(t) = grid.get_mut(2, 2) {
            t.tile_type = TileType::TallGrass;
        }
        crate::spawning::spawn_fire_patch(&mut world, 2, 2);
        let patch_a = crate::spawning::spawn_glow_mushrooms(&mut world, 3, 2);
        let patch_b = crate::spawning::spawn_glow_mushrooms(&mut world, 4, 2);

        // Seeded: the chain is a run of 35% rolls, so an unseeded run is a
        // coin toss rather than a test.
        run_fire_seeded(&mut world, &mut grid, 6.0, 7);

        assert!(!world.contains(patch_a), "fungus next to fire burns away");
        assert!(
            !world.contains(patch_b),
            "the fire carries along the run of fungus"
        );
        // Each patch leaves a fire behind on its own tile, so the blaze keeps
        // going -- and the light the fungus was shedding goes with it.
        let fires = grass_fire_tiles(&world);
        assert!(
            fires.contains(&(3, 2)) || fires.contains(&(4, 2)),
            "a burnt patch leaves fire on its tile, got {fires:?}"
        );
        assert_eq!(
            world.query::<&crate::components::GlowMushroom>().iter().count(),
            0,
            "no fungus survives the fire"
        );
    }

    #[test]
    fn test_webs_ignite_from_adjacent_fire_and_burn_away() {
        let mut world = World::new();
        let mut grid = make_grid(9, 9, TileType::Floor);

        // A chain of webs leading away from a grass fire.
        if let Some(t) = grid.get_mut(2, 2) {
            t.tile_type = TileType::TallGrass;
        }
        crate::spawning::spawn_fire_patch(&mut world, 2, 2);
        let web_a = crate::spawning::spawn_web(&mut world, 3, 2, None);
        let web_b = crate::spawning::spawn_web(&mut world, 4, 2, None);

        // At 80% per step, a handful of steps all but guarantees both catch
        // (web B via web A acting as a source).
        run_fire(&mut world, &mut grid, 3.0);
        assert!(
            world.get::<&OnFire>(web_a).is_ok() || !world.contains(web_a),
            "web adjacent to fire must ignite (or have already burnt away)"
        );

        // After the burn duration both webs are consumed entirely.
        run_fire(&mut world, &mut grid, WEB_BURN_DURATION + 3.0);
        assert!(!world.contains(web_a), "burnt web is consumed");
        assert!(!world.contains(web_b), "fire leaps web-to-web and consumes both");
    }

    /// The full fire ecosystem chain in one scene: burning grass ignites an
    /// adjacent web, the web passes fire to an oil puddle, the burning oil
    /// lights the barrel's fuse, and the explosion sprays ignited puddles.
    #[test]
    fn test_fire_chain_grass_web_oil_barrel_explosion() {
        let mut world = World::new();
        let mut grid = make_grid(11, 11, TileType::Floor);

        // The chain, left to right: burning grass -> web -> oil -> barrel.
        if let Some(t) = grid.get_mut(2, 5) {
            t.tile_type = TileType::TallGrass;
        }
        crate::spawning::spawn_fire_patch(&mut world, 2, 5);
        let web = crate::spawning::spawn_web(&mut world, 3, 5, None);
        let puddle = crate::spawning::spawn_oil_puddle(&mut world, 4, 5);
        let barrel = crate::spawning::spawn_oil_barrel(&mut world, 5, 5);

        // Step until fire crosses the web into the oil (80% then 90% per
        // step): the bound exists only as a hard failure stop.
        let mut steps = 0;
        while world.get::<&OnFire>(puddle).is_err() && steps < 60 {
            run_fire(&mut world, &mut grid, FIRE_STEP_INTERVAL);
            steps += 1;
        }
        assert!(
            world.get::<&OnFire>(puddle).is_ok(),
            "fire must reach the oil via the web"
        );
        // The web caught along the way (or already burnt away entirely).
        assert!(world.get::<&OnFire>(web).is_ok() || !world.contains(web));

        // Adjacent blaze lights the barrel at FIRE_SPREAD_ENTITY_CHANCE per
        // step; keep the heat on (re-lighting a fresh puddle when one burns
        // away) until the fuse catches. 400 steps at 15% cannot miss.
        let mut steps = 0;
        while world.contains(barrel) && world.get::<&BarrelFuse>(barrel).is_err() && steps < 400 {
            let oil_burning = world
                .query::<(&Position, &OilPuddle, &OnFire)>()
                .iter()
                .any(|(_, (p, _, _))| p.x == 4 && p.y == 5);
            if !oil_burning {
                let fresh = crate::spawning::spawn_oil_puddle(&mut world, 4, 5);
                ignite_fuel(&mut world, fresh);
            }
            run_fire(&mut world, &mut grid, FIRE_STEP_INTERVAL);
            steps += 1;
        }
        assert!(
            !world.contains(barrel) || world.get::<&BarrelFuse>(barrel).is_ok(),
            "burning oil must light the barrel fuse"
        );

        // Fuse burns down: the barrel detonates and sprays ignited oil.
        run_fire(&mut world, &mut grid, OIL_BARREL_FUSE_SECONDS + 0.5);
        assert!(!world.contains(barrel), "barrel explodes at the end of the chain");
        let burning_puddles = world.query::<(&OilPuddle, &OnFire)>().iter().count();
        assert!(burning_puddles > 0, "explosion sprays ignited oil puddles");
    }

    #[test]
    fn test_wet_creature_in_burning_oil_never_catches_but_oiled_one_does() {
        let mut world = World::new();
        let mut grid = make_grid(9, 9, TileType::Floor);
        let spawn = |world: &mut World, x: i32| {
            world.spawn((Position::new(x, 4), StatusEffects::new(), Combustible { flammability: 0.35 }))
        };
        let wet = spawn(&mut world, 2);
        let oily = spawn(&mut world, 6);
        crate::systems::effects::add_effect_to_entity(&mut world, wet, EffectType::Wet, 1000.0);
        crate::systems::effects::add_effect_to_entity(&mut world, oily, EffectType::Oiled, 1000.0);
        for x in [2, 6] {
            let oil = crate::spawning::spawn_oil_puddle(&mut world, x, 4);
            ignite_fuel(&mut world, oil);
        }

        run_fire_seeded(&mut world, &mut grid, 3.0, 5);
        let burning = |e| crate::systems::effects::entity_has_effect(&world, e, EffectType::Burning);
        assert!(!burning(wet), "soaked through, it stands in the flames unharmed");
        assert!(burning(oily), "oil-soaked, it goes up");
    }

    #[test]
    fn test_smashed_lantern_spills_burning_oil_and_frees_its_tile() {
        let mut world = World::new();
        let grid = make_grid(7, 7, TileType::Floor);
        let lantern = crate::spawning::spawn_lantern(
            &mut world,
            3,
            3,
            crate::components::LanternStyle::Warm,
        );
        let mut cache = SpatialCache::rebuild_from_world(&world);
        assert!(cache.is_blocked((3, 3)));

        assert!(smash_lantern(&mut world, &grid, &mut cache, lantern, &mut EventQueue::new()));
        assert!(!world.contains(lantern), "the lantern is gone");
        let burning_oil = world
            .query::<(&Position, &OilPuddle, &OnFire)>()
            .iter()
            .any(|(_, (p, _, _))| (p.x, p.y) == (3, 3));
        assert!(burning_oil, "its oil burns where it stood");
        cache.assert_coherent_with_world(&world, "after smashing a lantern");
    }

    #[test]
    fn test_toppled_brazier_spills_fire_and_unlights() {
        let mut world = World::new();
        let grid = make_grid(5, 5, TileType::Floor);
        let mut events = EventQueue::new();

        let brazier = crate::spawning::spawn_brazier(&mut world, 2, 2);
        let victim = world.spawn((Position::new(2, 2), StatusEffects::new()));

        assert!(topple_brazier(&mut world, &grid, brazier, &mut events, &mut rand::thread_rng()));

        // Unlit: no more light or contact hazard, and a second topple is a no-op.
        assert!(world.get::<&LightSource>(brazier).is_err());
        assert!(world.get::<&CausesBurning>(brazier).is_err());
        assert!(!topple_brazier(&mut world, &grid, brazier, &mut events, &mut rand::thread_rng()));

        // Whatever stood on the brazier tile is now burning.
        let burning = world
            .get::<&StatusEffects>(victim)
            .map(|s| s.effects.iter().any(|e| e.effect_type == EffectType::Burning))
            .unwrap_or(false);
        assert!(burning, "entity on the spill tile catches fire");
    }
}
