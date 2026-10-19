//! Spreading fire system.
//!
//! Fire is carried by entities, in two forms:
//!
//! - **Creatures** (and oil barrels) burn through the `Burning` status, and
//!   catch at a rate scaled by their flammability: `Combustible`, adjusted by
//!   what they are coated in (`coatings::effective_flammability` — Wet things
//!   can't catch, Oily things catch readily).
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
    AnimatedSprite, BarrelFuse, Brazier, Burnout, CausesBurning, Doused, EffectType,
    Fuel, Health, Ignition, LightSource, OilBarrel, OilPuddle, OnFire, Position, Sprite,
    StatusEffects, WetTile,
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

    if game_dt <= 0.0 {
        return;
    }

    // 0. Water soaks whatever stands in it (which also puts out fire and
    //    washes off oil), and anything wet near a fire dries off faster.
    let in_water: Vec<Entity> = world
        .query::<(&Position, &StatusEffects)>()
        .iter()
        .filter(|(_, (p, _))| grid.water_positions.contains(&(p.x, p.y)))
        .map(|(id, _)| id)
        .collect();
    for id in in_water {
        crate::systems::coatings::soak(world, id, events);
    }
    crate::systems::coatings::dry_near_fire(world, game_dt);

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

    // 3. Oil barrels: a burning barrel lights its fuse; destroyed barrels
    //    (health <= 0) skip the fuse and detonate immediately.
    let to_fuse: Vec<Entity> = world
        .query::<(&OilBarrel, &StatusEffects)>()
        .without::<&BarrelFuse>()
        .iter()
        .filter(|(_, (_, s))| s.effects.iter().any(|e| e.effect_type == EffectType::Burning))
        .map(|(id, _)| id)
        .collect();
    for id in to_fuse {
        let _ = world.insert_one(id, BarrelFuse { remaining: OIL_BARREL_FUSE_SECONDS });
    }

    let mut to_explode: Vec<Entity> = Vec::new();
    for (id, (_, fuse)) in world.query_mut::<(&OilBarrel, &mut BarrelFuse)>() {
        fuse.remaining -= game_dt;
        if fuse.remaining <= 0.0 {
            to_explode.push(id);
        }
    }
    for (id, (_, health)) in world.query::<(&OilBarrel, &Health)>().iter() {
        if health.current <= 0 && !to_explode.contains(&id) {
            to_explode.push(id);
        }
    }
    for id in to_explode {
        explode_barrel(world, grid, spatial_cache, id, events, rng);
    }

    // 4. Spread in fixed game-time steps so behaviour is frame-rate independent.
    *accumulator += game_dt;
    while *accumulator >= FIRE_STEP_INTERVAL {
        *accumulator -= FIRE_STEP_INTERVAL;
        spread_step(world, grid, events, rng);
    }
}

/// On a melee hit, a burning combatant may set the other alight (medium chance
/// scaled by the target's effective flammability — see
/// `coatings::effective_flammability`). No-op unless `from` is burning and
/// `to` can burn and isn't already.
pub fn try_combat_ignite(world: &mut World, from: Entity, to: Entity, rng: &mut impl Rng) {
    let from_burning = world
        .get::<&StatusEffects>(from)
        .map(|s| s.effects.iter().any(|e| e.effect_type == EffectType::Burning))
        .unwrap_or(false);
    if !from_burning {
        return;
    }
    let flammability = crate::systems::coatings::effective_flammability(world, to);
    if flammability <= 0.0 {
        return;
    }
    let already = world
        .get::<&StatusEffects>(to)
        .map(|s| s.effects.iter().any(|e| e.effect_type == EffectType::Burning))
        .unwrap_or(false);
    if already {
        return;
    }
    let chance = (FIRE_COMBAT_IGNITE_CHANCE * flammability as f64).clamp(0.0, 1.0);
    if rng.gen_bool(chance) {
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
        .query::<(&Position, &OnFire)>()
        .iter()
        .map(|(_, (p, _))| (p.x, p.y))
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
    // by effective flammability (wet things can't catch, oily things catch
    // readily). Creatures in water can't be ignited; already-burning ones
    // are skipped.
    let mut stand_chance: HashMap<(i32, i32), f64> = HashMap::new();
    for (_, (pos, fuel, _)) in world.query::<(&Position, &Fuel, &OnFire)>().iter() {
        if fuel.stand_ignite_chance > 0.0 {
            let c = stand_chance.entry((pos.x, pos.y)).or_insert(0.0);
            *c = c.max(fuel.stand_ignite_chance);
        }
    }
    let mut creatures_to_ignite: Vec<(Entity, (i32, i32))> = Vec::new();
    for (id, (pos, status)) in world.query::<(&Position, &StatusEffects)>().iter() {
        if grid.water_positions.contains(&(pos.x, pos.y)) {
            continue;
        }
        if status.effects.iter().any(|e| e.effect_type == EffectType::Burning) {
            continue;
        }
        let flammability = crate::systems::coatings::effective_flammability(world, id) as f64;
        if flammability <= 0.0 {
            continue;
        }

        if let Some(&base) = stand_chance.get(&(pos.x, pos.y)) {
            if rng.gen_bool((base * flammability).clamp(0.0, 1.0)) {
                creatures_to_ignite.push((id, (pos.x, pos.y)));
                continue;
            }
        }

        if !near_fire((pos.x, pos.y)) {
            continue;
        }
        if rng.gen_bool((FIRE_SPREAD_ENTITY_CHANCE * flammability).clamp(0.0, 1.0)) {
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
/// soaks every entity in it (Wet: puts out Burning, washes off oil, and keeps
/// them from catching for a while), douses every burning fuel (each as its
/// `Fuel::doused` says — grass fires go out and leave the grass, oil and webs
/// stay to be relit), and soaks the walkable tiles so damp-sensitive fuel
/// can't catch for a while.
pub fn splash_water(
    world: &mut World,
    grid: &Grid,
    cx: i32,
    cy: i32,
    radius: i32,
    events: &mut EventQueue,
) {
    let in_radius =
        |x: i32, y: i32| (x - cx).abs() <= radius && (y - cy).abs() <= radius;

    // Soak everything caught in the splash (puts out Burning, washes off oil).
    let soaked: Vec<Entity> = world
        .query::<(&Position, &StatusEffects)>()
        .iter()
        .filter(|(_, (p, _))| in_radius(p.x, p.y))
        .map(|(id, _)| id)
        .collect();
    for id in soaked {
        crate::systems::coatings::soak(world, id, events);
    }

    // Douse burning fuel.
    let doused: Vec<Entity> = world
        .query::<(&Position, &OnFire)>()
        .iter()
        .filter(|(_, (p, _))| in_radius(p.x, p.y))
        .map(|(id, _)| id)
        .collect();
    for id in doused {
        douse_fuel(world, id);
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

    // Snuff the stand: no more light, no more contact burns, static tipped sprite.
    if let Ok(mut b) = world.get::<&mut Brazier>(brazier) {
        b.lit = false;
    }
    let _ = world.remove::<(LightSource, CausesBurning, AnimatedSprite)>(brazier);
    let _ = world.insert_one(brazier, Sprite::from_ref(tile_ids::BRAZIER_UNLIT));

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

/// Spill fire onto a tile: set the tile alight (`ignite_tile`) and set
/// anything standing there burning. Used by brazier topples and fire traps.
pub fn spill_fire_at(world: &mut World, grid: &Grid, x: i32, y: i32, events: &mut EventQueue) {
    if grid.water_positions.contains(&(x, y)) {
        return;
    }
    ignite_tile(world, grid, x, y);

    // Anything standing there catches fire.
    let victims: Vec<Entity> = world
        .query::<(&Position, &StatusEffects)>()
        .iter()
        .filter(|(_, (p, _))| p.x == x && p.y == y)
        .map(|(id, _)| id)
        .collect();
    for id in victims {
        if crate::systems::effects::add_effect_to_entity(world, id, EffectType::Burning, BURNING_DURATION) {
            events.push(GameEvent::CaughtFire { entity: id, position: (x, y) });
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
            crate::systems::combat::apply_damage(world, grid, id, OIL_BARREL_EXPLOSION_DAMAGE, rng);
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
        crate::systems::coatings::coat_in_oil(world, id, events);
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

    #[test]
    fn test_destroyed_barrel_explodes_without_fuse() {
        let mut world = World::new();
        let mut grid = make_grid(7, 7, TileType::Floor);

        let barrel = crate::spawning::spawn_oil_barrel(&mut world, 3, 3);
        if let Ok(mut health) = world.get::<&mut Health>(barrel) {
            health.current = 0;
        }

        run_fire(&mut world, &mut grid, 0.1);
        assert!(!world.contains(barrel), "a destroyed barrel detonates immediately");
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
    fn test_wet_creature_in_burning_oil_never_catches_but_oily_one_does() {
        let mut world = World::new();
        let mut grid = make_grid(9, 9, TileType::Floor);
        let spawn = |world: &mut World, x: i32| {
            world.spawn((Position::new(x, 4), StatusEffects::new(), Combustible { flammability: 0.35 }))
        };
        let wet = spawn(&mut world, 2);
        let oily = spawn(&mut world, 6);
        crate::systems::effects::add_effect_to_entity(&mut world, wet, EffectType::Wet, 1000.0);
        crate::systems::effects::add_effect_to_entity(&mut world, oily, EffectType::Oily, 1000.0);
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
