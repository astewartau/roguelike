//! Spreading fire system.
//!
//! Fire is carried by entities: burning creatures (the `Burning` status),
//! burning-grass hazard entities (`BurningGrass` + `CausesBurning`), and
//! burning oil puddles (`OilPuddle` + `BurningOil`). This system advances
//! burnouts (grass reverts to floor, spent oil burns away), ticks oil-barrel
//! fuses (and detonates them), dries wet grass, and — in discrete game-time
//! steps — spreads fire to adjacent flammable tiles, oil, and creatures.
//!
//! It runs from the engine tick (not the deep sim) so it can take `&mut Grid`,
//! and is paced by the game-clock delta this frame (so it freezes when paused).

use std::collections::HashSet;

use hecs::{Entity, World};
use rand::Rng;

use crate::components::{
    AnimatedSprite, BarrelFuse, Brazier, BurningGrass, BurningOil, BurningWeb, CausesBurning,
    Combustible, EffectType, Health, LightSource, OilBarrel, OilPuddle, Position, Sprite,
    StatusEffects, Web, WetGrass,
};
use crate::constants::*;
use crate::events::{EventQueue, GameEvent};
use crate::grid::Grid;
use crate::spatial_cache::SpatialCache;
use crate::tile::{tile_ids, TileType};

const NEIGHBORS: [(i32, i32); 8] = [
    (-1, 0), (1, 0), (0, -1), (0, 1),
    (-1, -1), (-1, 1), (1, -1), (1, 1),
];

/// Advance burning grass/oil, barrel fuses, and fire spread. `game_dt` is
/// elapsed game-time this frame; `accumulator` carries fractional game-time
/// between discrete spread steps.
pub fn tick_fire(
    world: &mut World,
    grid: &mut Grid,
    spatial_cache: &mut SpatialCache,
    events: &mut EventQueue,
    game_dt: f32,
    accumulator: &mut f32,
    fov_dirty: &mut bool,
    rng: &mut impl Rng,
) {
    if game_dt <= 0.0 {
        return;
    }

    // 0. Water douses fire: anything standing on a water tile can't stay alight.
    let to_douse: Vec<Entity> = world
        .query::<(&Position, &StatusEffects)>()
        .iter()
        .filter(|(_, (p, s))| {
            grid.water_positions.contains(&(p.x, p.y))
                && s.effects.iter().any(|e| e.effect_type == EffectType::Burning)
        })
        .map(|(id, _)| id)
        .collect();
    for id in to_douse {
        crate::systems::effects::remove_effect_from_entity(world, id, EffectType::Burning);
    }

    // 1. Burnout: advance each burning-grass timer; revert tile on expiry.
    let mut burned_out: Vec<(Entity, (i32, i32))> = Vec::new();
    for (id, (pos, bg)) in world.query_mut::<(&Position, &mut BurningGrass)>() {
        bg.remaining -= game_dt;
        if bg.remaining <= 0.0 {
            burned_out.push((id, (pos.x, pos.y)));
        }
    }
    for (id, (x, y)) in burned_out {
        let _ = world.despawn(id);
        if let Some(tile) = grid.get_mut(x, y) {
            if tile.tile_type == TileType::TallGrass {
                tile.tile_type = TileType::Floor;
                tile.sprite_override = None;
                *fov_dirty = true; // grass blocked vision; floor doesn't
            }
        }
    }

    // 2. Oil burnout: spent puddles burn away entirely.
    let mut oil_spent: Vec<Entity> = Vec::new();
    for (id, oil) in world.query_mut::<&mut BurningOil>() {
        oil.remaining -= game_dt;
        if oil.remaining <= 0.0 {
            oil_spent.push(id);
        }
    }
    for id in oil_spent {
        let _ = world.despawn(id);
    }

    // 2b. Web burnout: an ignited web is consumed in seconds.
    let mut web_spent: Vec<Entity> = Vec::new();
    for (id, web) in world.query_mut::<&mut BurningWeb>() {
        web.remaining -= game_dt;
        if web.remaining <= 0.0 {
            web_spent.push(id);
        }
    }
    for id in web_spent {
        let _ = world.despawn(id);
    }

    // 3. Wet grass dries out.
    let mut dried: Vec<Entity> = Vec::new();
    for (id, wet) in world.query_mut::<&mut WetGrass>() {
        wet.remaining -= game_dt;
        if wet.remaining <= 0.0 {
            dried.push(id);
        }
    }
    for id in dried {
        let _ = world.despawn(id);
    }

    // 4. Oil barrels: a burning barrel lights its fuse; destroyed barrels
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

    // 5. Spread in fixed game-time steps so behaviour is frame-rate independent.
    *accumulator += game_dt;
    while *accumulator >= FIRE_STEP_INTERVAL {
        *accumulator -= FIRE_STEP_INTERVAL;
        spread_step(world, grid, events, rng);
    }
}

/// On a melee hit, a burning combatant may set the other alight (medium chance
/// scaled by the target's flammability). No-op unless `from` is burning and `to`
/// is combustible and not already burning.
pub fn try_combat_ignite(world: &mut World, from: Entity, to: Entity, rng: &mut impl Rng) {
    let from_burning = world
        .get::<&StatusEffects>(from)
        .map(|s| s.effects.iter().any(|e| e.effect_type == EffectType::Burning))
        .unwrap_or(false);
    if !from_burning {
        return;
    }
    let flammability = match world.get::<&Combustible>(to) {
        Ok(c) => c.flammability,
        Err(_) => return,
    };
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

/// Positions of all wet-grass timer entities (unignitable tiles).
fn wet_positions(world: &World) -> HashSet<(i32, i32)> {
    world
        .query::<(&Position, &WetGrass)>()
        .iter()
        .map(|(_, (p, _))| (p.x, p.y))
        .collect()
}

fn spread_step(world: &mut World, grid: &mut Grid, events: &mut EventQueue, rng: &mut impl Rng) {
    // Three source kinds:
    //  - burning grass tiles spread to adjacent grass (the wildfire front);
    //  - burning oil puddles burn hot: they spread to grass like a grass fire,
    //    leap to adjacent oil, and readily ignite anything standing in them;
    //  - burning creatures readily light grass they stand IN, but only rarely
    //    grass they're merely next to.
    let grass_sources: Vec<(i32, i32)> = world
        .query::<(&Position, &BurningGrass)>()
        .iter()
        .map(|(_, (p, _))| (p.x, p.y))
        .collect();
    let oil_sources: Vec<(i32, i32)> = world
        .query::<(&Position, &BurningOil)>()
        .iter()
        .map(|(_, (p, _))| (p.x, p.y))
        .collect();
    let creature_sources: Vec<(i32, i32)> = world
        .query::<(&Position, &StatusEffects)>()
        .iter()
        .filter(|(_, (_, s))| s.effects.iter().any(|e| e.effect_type == EffectType::Burning))
        .map(|(_, (p, _))| (p.x, p.y))
        .collect();
    let web_sources: Vec<(i32, i32)> = world
        .query::<(&Position, &BurningWeb)>()
        .iter()
        .map(|(_, (p, _))| (p.x, p.y))
        .collect();
    if grass_sources.is_empty()
        && oil_sources.is_empty()
        && creature_sources.is_empty()
        && web_sources.is_empty()
    {
        return;
    }

    let wet = wet_positions(world);
    let burning_tiles: HashSet<(i32, i32)> =
        grass_sources.iter().chain(oil_sources.iter()).copied().collect();
    let flammable_free = |x: i32, y: i32, pending: &HashSet<(i32, i32)>| {
        !burning_tiles.contains(&(x, y))
            && !pending.contains(&(x, y))
            && !wet.contains(&(x, y))
            && grid.get(x, y).map(|t| t.tile_type.is_flammable()).unwrap_or(false)
    };

    let mut grass_to_ignite: HashSet<(i32, i32)> = HashSet::new();

    // Grass/oil/web fire -> adjacent grass (slow front).
    for &(sx, sy) in grass_sources.iter().chain(oil_sources.iter()).chain(web_sources.iter()) {
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

    let all_sources: HashSet<(i32, i32)> = grass_sources
        .iter()
        .chain(oil_sources.iter())
        .chain(creature_sources.iter())
        .chain(web_sources.iter())
        .copied()
        .collect();

    // Fire -> webs it is on or next to. Webs are tinder: they catch at a very
    // high per-step chance and the blaze leaps web-to-web through a lair.
    let mut webs_to_ignite: Vec<Entity> = Vec::new();
    for (id, (pos, _)) in world
        .query::<(&Position, &Web)>()
        .without::<&BurningWeb>()
        .iter()
    {
        let near = all_sources
            .iter()
            .any(|&(sx, sy)| (pos.x - sx).abs() <= 1 && (pos.y - sy).abs() <= 1);
        if near && rng.gen_bool(WEB_IGNITE_CHANCE) {
            webs_to_ignite.push(id);
        }
    }

    // Fire -> oil puddles it is on or next to (oil catches almost instantly).
    let mut oil_to_ignite: Vec<Entity> = Vec::new();
    for (id, (pos, _)) in world
        .query::<(&Position, &OilPuddle)>()
        .without::<&BurningOil>()
        .iter()
    {
        let near = all_sources
            .iter()
            .any(|&(sx, sy)| (pos.x - sx).abs() <= 1 && (pos.y - sy).abs() <= 1);
        if near && rng.gen_bool(FIRE_TO_OIL_IGNITE_CHANCE) {
            oil_to_ignite.push(id);
        }
    }

    // Fire -> adjacent creatures (small, scaled by flammability), and burning
    // oil -> creatures standing IN it (high, scaled by flammability).
    // Creatures in water can't be ignited; already-burning ones are skipped.
    let burning_oil_tiles: HashSet<(i32, i32)> = oil_sources.iter().copied().collect();
    let mut creatures_to_ignite: Vec<(Entity, (i32, i32))> = Vec::new();
    for (id, (pos, comb, status)) in
        world.query::<(&Position, &Combustible, Option<&StatusEffects>)>().iter()
    {
        if grid.water_positions.contains(&(pos.x, pos.y)) {
            continue;
        }
        let already_burning = status
            .map(|s| s.effects.iter().any(|e| e.effect_type == EffectType::Burning))
            .unwrap_or(false);
        if already_burning {
            continue;
        }

        // Standing in burning oil: high per-step ignite chance.
        if burning_oil_tiles.contains(&(pos.x, pos.y)) {
            let chance =
                (BURNING_OIL_STAND_IGNITE_CHANCE * comb.flammability as f64).clamp(0.0, 1.0);
            if rng.gen_bool(chance) {
                creatures_to_ignite.push((id, (pos.x, pos.y)));
                continue;
            }
        }

        let near = all_sources
            .iter()
            .any(|&(sx, sy)| (pos.x - sx).abs() <= 1 && (pos.y - sy).abs() <= 1);
        if !near {
            continue;
        }
        let chance = (FIRE_SPREAD_ENTITY_CHANCE * comb.flammability as f64).clamp(0.0, 1.0);
        if rng.gen_bool(chance) {
            creatures_to_ignite.push((id, (pos.x, pos.y)));
        }
    }

    // Apply (after queries are dropped).
    for (x, y) in grass_to_ignite {
        crate::spawning::spawn_burning_grass(world, x, y);
    }
    for id in oil_to_ignite {
        ignite_oil_puddle(world, id);
    }
    for id in webs_to_ignite {
        ignite_web(world, id);
    }
    for (id, pos) in creatures_to_ignite {
        crate::systems::effects::add_effect_to_entity(world, id, EffectType::Burning, BURNING_DURATION);
        events.push(GameEvent::CaughtFire { entity: id, position: pos });
    }
}

/// Set an oil puddle alight: adds `BurningOil`, a fire sprite, a light, and
/// `CausesBurning` (so stepping into it ignites). No-op if already burning
/// or not an oil puddle.
pub fn ignite_oil_puddle(world: &mut World, puddle: Entity) {
    if world.get::<&OilPuddle>(puddle).is_err() || world.get::<&BurningOil>(puddle).is_ok() {
        return;
    }
    let _ = world.insert(
        puddle,
        (
            BurningOil { remaining: OIL_BURN_DURATION },
            CausesBurning,
            AnimatedSprite::fire_pit(),
            LightSource::brazier(),
        ),
    );
}

/// Put out a burning oil puddle, leaving the (re-ignitable) puddle behind.
fn extinguish_oil_puddle(world: &mut World, puddle: Entity) {
    let _ = world.remove::<(BurningOil, CausesBurning, AnimatedSprite, LightSource)>(puddle);
}

/// Set a web alight: it burns fast (`WEB_BURN_DURATION`), sheds light, and
/// ignites anything on/entering its tile (`CausesBurning`) until it is
/// consumed. No-op if not a web or already burning.
pub fn ignite_web(world: &mut World, web: Entity) {
    if world.get::<&Web>(web).is_err() || world.get::<&BurningWeb>(web).is_ok() {
        return;
    }
    let _ = world.insert(
        web,
        (
            BurningWeb { remaining: WEB_BURN_DURATION },
            CausesBurning,
            AnimatedSprite::fire_pit(),
            LightSource::brazier(),
        ),
    );
}

/// Splash of water centered at (cx, cy) with the given Chebyshev radius:
/// extinguishes Burning on entities, stops burning grass (tile stays as
/// unburnt grass), douses burning oil (puddle stays), and soaks grass tiles
/// so they can't ignite for a while.
pub fn splash_water(world: &mut World, grid: &Grid, cx: i32, cy: i32, radius: i32) {
    let in_radius =
        |x: i32, y: i32| (x - cx).abs() <= radius && (y - cy).abs() <= radius;

    // Douse burning entities.
    let burning: Vec<Entity> = world
        .query::<(&Position, &StatusEffects)>()
        .iter()
        .filter(|(_, (p, s))| {
            in_radius(p.x, p.y)
                && s.effects.iter().any(|e| e.effect_type == EffectType::Burning)
        })
        .map(|(id, _)| id)
        .collect();
    for id in burning {
        crate::systems::effects::remove_effect_from_entity(world, id, EffectType::Burning);
    }

    // Stop burning grass without consuming the grass (tile stays TallGrass).
    let doused_grass: Vec<Entity> = world
        .query::<(&Position, &BurningGrass)>()
        .iter()
        .filter(|(_, (p, _))| in_radius(p.x, p.y))
        .map(|(id, _)| id)
        .collect();
    for id in doused_grass {
        let _ = world.despawn(id);
    }

    // Douse burning oil; the puddle remains and can be re-lit.
    let doused_oil: Vec<Entity> = world
        .query::<(&Position, &BurningOil)>()
        .iter()
        .filter(|(_, (p, _))| in_radius(p.x, p.y))
        .map(|(id, _)| id)
        .collect();
    for id in doused_oil {
        extinguish_oil_puddle(world, id);
    }

    // Douse burning webs; the (soggy) web survives and can be re-lit later.
    let doused_webs: Vec<Entity> = world
        .query::<(&Position, &BurningWeb)>()
        .iter()
        .filter(|(_, (p, _))| in_radius(p.x, p.y))
        .map(|(id, _)| id)
        .collect();
    for id in doused_webs {
        let _ = world.remove::<(BurningWeb, CausesBurning, AnimatedSprite, LightSource)>(id);
    }

    // Soak grass tiles in the radius: refresh existing timers, spawn new ones.
    let mut refreshed: HashSet<(i32, i32)> = HashSet::new();
    for (_, (pos, wet)) in world.query_mut::<(&Position, &mut WetGrass)>() {
        if in_radius(pos.x, pos.y) {
            wet.remaining = WET_GRASS_DURATION;
            refreshed.insert((pos.x, pos.y));
        }
    }
    for dy in -radius..=radius {
        for dx in -radius..=radius {
            let (x, y) = (cx + dx, cy + dy);
            if refreshed.contains(&(x, y)) {
                continue;
            }
            let is_grass = grid
                .get(x, y)
                .map(|t| t.tile_type.is_flammable())
                .unwrap_or(false);
            if is_grass {
                let pos = Position::new(x, y);
                world.spawn((pos, WetGrass { remaining: WET_GRASS_DURATION }));
            }
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

/// Spill fire onto a tile: ignite grass (if dry), ignite any oil puddle, and
/// set entities standing there burning. Used by brazier topples and fire traps.
pub fn spill_fire_at(world: &mut World, grid: &Grid, x: i32, y: i32, events: &mut EventQueue) {
    if grid.water_positions.contains(&(x, y)) {
        return;
    }

    // Grass catches (unless wet or already burning).
    let flammable = grid
        .get(x, y)
        .map(|t| t.tile_type.is_flammable())
        .unwrap_or(false);
    if flammable && !wet_positions(world).contains(&(x, y)) {
        let already = world
            .query::<(&Position, &BurningGrass)>()
            .iter()
            .any(|(_, (p, _))| p.x == x && p.y == y);
        if !already {
            crate::spawning::spawn_burning_grass(world, x, y);
        }
    }

    // Oil catches.
    let puddles: Vec<Entity> = world
        .query::<(&Position, &OilPuddle)>()
        .iter()
        .filter(|(_, (p, _))| p.x == x && p.y == y)
        .map(|(id, _)| id)
        .collect();
    for id in puddles {
        ignite_oil_puddle(world, id);
    }

    // Webs catch.
    let webs: Vec<Entity> = world
        .query::<(&Position, &Web)>()
        .iter()
        .filter(|(_, (p, _))| p.x == x && p.y == y)
        .map(|(id, _)| id)
        .collect();
    for id in webs {
        ignite_web(world, id);
    }

    // Anything standing there catches fire.
    let victims: Vec<Entity> = world
        .query::<(&Position, &StatusEffects)>()
        .iter()
        .filter(|(_, (p, _))| p.x == x && p.y == y)
        .map(|(id, _)| id)
        .collect();
    for id in victims {
        crate::systems::effects::add_effect_to_entity(world, id, EffectType::Burning, BURNING_DURATION);
        events.push(GameEvent::CaughtFire { entity: id, position: (x, y) });
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
    crate::systems::ai::wake_enemies_in_radius(world, (bx, by), EXPLOSION_NOISE_RADIUS);

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
        let dealt = crate::systems::combat::apply_damage(world, id, OIL_BARREL_EXPLOSION_DAMAGE);
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
        ignite_oil_puddle(world, puddle);
    }

    // Existing puddles caught in the spray radius ignite too.
    let caught: Vec<Entity> = world
        .query::<(&Position, &OilPuddle)>()
        .without::<&BurningOil>()
        .iter()
        .filter(|(_, (p, _))| {
            (p.x - bx).abs() <= OIL_BARREL_PUDDLE_RADIUS
                && (p.y - by).abs() <= OIL_BARREL_PUDDLE_RADIUS
        })
        .map(|(id, _)| id)
        .collect();
    for id in caught {
        ignite_oil_puddle(world, id);
    }
}

/// Is there an oil puddle currently burning at this tile? (test helper /
/// query for AI or UI use)
#[allow(dead_code)]
pub fn burning_oil_at(world: &World, x: i32, y: i32) -> bool {
    world
        .query::<(&Position, &BurningOil)>()
        .iter()
        .any(|(_, (p, _))| p.x == x && p.y == y)
}

#[cfg(test)]
mod tests {
    use super::*;
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
        }
    }

    fn run_fire(world: &mut World, grid: &mut Grid, seconds: f32) {
        let mut cache = SpatialCache::rebuild_from_world(world);
        run_fire_cached(world, grid, &mut cache, seconds);
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
        // Advance in FIRE_STEP_INTERVAL chunks so spread steps actually run.
        let mut remaining = seconds;
        while remaining > 0.0 {
            let dt = remaining.min(FIRE_STEP_INTERVAL);
            tick_fire(
                world,
                grid,
                cache,
                &mut events,
                dt,
                &mut acc,
                &mut fov_dirty,
                &mut rand::thread_rng(),
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
        crate::spawning::spawn_burning_grass(&mut world, 2, 2);
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

        let burning_puddles = world.query::<(&OilPuddle, &BurningOil)>().iter().count();
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
        crate::spawning::spawn_burning_grass(&mut world, 2, 2);
        // Soak the grass tile.
        world.spawn((Position::new(2, 3), WetGrass { remaining: 1000.0 }));

        // Many spread steps: at 22%/step dry grass would all but certainly
        // catch; wet grass must never.
        run_fire(&mut world, &mut grid, GRASS_BURN_DURATION - 1.0);

        let caught = world
            .query::<(&Position, &BurningGrass)>()
            .iter()
            .any(|(_, (p, _))| p.x == 2 && p.y == 3);
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
        crate::spawning::spawn_burning_grass(&mut world, 2, 2);
        let puddle = crate::spawning::spawn_oil_puddle(&mut world, 3, 2);

        // 90% per step over many steps: failure odds are astronomically small.
        run_fire(&mut world, &mut grid, 5.0);

        assert!(
            world.get::<&BurningOil>(puddle).is_ok(),
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
        ignite_oil_puddle(&mut world, puddle);
        crate::spawning::spawn_burning_grass(&mut world, 3, 4);
        let victim = world.spawn((Position::new(4, 3), {
            let mut s = StatusEffects::new();
            crate::systems::effects::add_effect(&mut s, EffectType::Burning, BURNING_DURATION);
            s
        }));

        splash_water(&mut world, &grid, 3, 3, WATER_SPLASH_RADIUS);

        // Fires are out ...
        assert!(world.get::<&BurningOil>(puddle).is_err(), "oil fire doused");
        assert_eq!(
            world.query::<&BurningGrass>().iter().count(),
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
        let wet = world
            .query::<(&Position, &WetGrass)>()
            .iter()
            .any(|(_, (p, _))| p.x == 3 && p.y == 4);
        assert!(wet, "splashed grass is wet");
    }

    #[test]
    fn test_webs_ignite_from_adjacent_fire_and_burn_away() {
        let mut world = World::new();
        let mut grid = make_grid(9, 9, TileType::Floor);

        // A chain of webs leading away from a grass fire.
        if let Some(t) = grid.get_mut(2, 2) {
            t.tile_type = TileType::TallGrass;
        }
        crate::spawning::spawn_burning_grass(&mut world, 2, 2);
        let web_a = crate::spawning::spawn_web(&mut world, 3, 2, None);
        let web_b = crate::spawning::spawn_web(&mut world, 4, 2, None);

        // At 80% per step, a handful of steps all but guarantees both catch
        // (web B via web A acting as a source).
        run_fire(&mut world, &mut grid, 3.0);
        assert!(
            world.get::<&BurningWeb>(web_a).is_ok() || !world.contains(web_a),
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
        crate::spawning::spawn_burning_grass(&mut world, 2, 5);
        let web = crate::spawning::spawn_web(&mut world, 3, 5, None);
        let puddle = crate::spawning::spawn_oil_puddle(&mut world, 4, 5);
        let barrel = crate::spawning::spawn_oil_barrel(&mut world, 5, 5);

        // Step until fire crosses the web into the oil (80% then 90% per
        // step): the bound exists only as a hard failure stop.
        let mut steps = 0;
        while world.get::<&BurningOil>(puddle).is_err() && steps < 60 {
            run_fire(&mut world, &mut grid, FIRE_STEP_INTERVAL);
            steps += 1;
        }
        assert!(
            world.get::<&BurningOil>(puddle).is_ok(),
            "fire must reach the oil via the web"
        );
        // The web caught along the way (or already burnt away entirely).
        assert!(world.get::<&BurningWeb>(web).is_ok() || !world.contains(web));

        // Adjacent blaze lights the barrel at FIRE_SPREAD_ENTITY_CHANCE per
        // step; keep the heat on (re-lighting a fresh puddle when one burns
        // away) until the fuse catches. 400 steps at 15% cannot miss.
        let mut steps = 0;
        while world.contains(barrel) && world.get::<&BarrelFuse>(barrel).is_err() && steps < 400 {
            let oil_burning = world
                .query::<(&Position, &BurningOil)>()
                .iter()
                .any(|(_, (p, _))| p.x == 4 && p.y == 5);
            if !oil_burning {
                let fresh = crate::spawning::spawn_oil_puddle(&mut world, 4, 5);
                ignite_oil_puddle(&mut world, fresh);
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
        let burning_puddles = world.query::<(&OilPuddle, &BurningOil)>().iter().count();
        assert!(burning_puddles > 0, "explosion sprays ignited oil puddles");
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
