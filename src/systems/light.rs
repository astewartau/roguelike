//! Light as a gameplay system, not just a rendering one.
//!
//! Every light in the world — braziers, campfires, burning fuel, glowing
//! fungus, crystals, and anything currently on fire — lights the tiles around
//! it. There is one model of how far that light gets, and everything reads it:
//!
//! - the renderer, through the per-frame [`LightMap`] it samples as a texture;
//! - the fog pass and the "seen because it's lit" visibility pass in
//!   `systems::rendering`;
//! - the AI, through the player's [`LightExposure`]. An unaware enemy notices
//!   a player standing in light faster and one in shadow slower
//!   (`DARK_ALERTNESS_MULT`..`LIT_ALERTNESS_MULT`), unless it `SeesInDark`.
//!
//! So what looks lit is what counts as lit, and existing content turns into
//! stealth tools without knowing it: douse or topple a brazier and the room
//! goes dark; stand beside fungus and you're picked out; catch fire and you're
//! a beacon.
//!
//! **Occlusion.** Light travels in straight lines from the source, and every
//! tile it crosses passes on some fraction of it ([`LightOcclusion`]): walls
//! and closed doors none, tall grass and solid objects (barrels, chests,
//! stalagmites) some, so they cast soft shadows rather than hard ones.
//!
//! **Falloff.** A light's brightness at distance `d` within radius `r` is
//! `intensity * (FOCUSED * t^2 + SPREAD * t)` with `t = 1 - d/r`: a bright
//! pool near the source plus a gentle wash reaching out to the edge.
//!
//! The player's own sight radius is deliberately *not* a light here. It is
//! what you see by, and it covers your whole view, so counting it would make
//! every tile you stand on fully lit.

use hecs::{Entity, World};

use crate::components::{
    Actor, BlocksMovement, BlocksVision, Door, EffectType, LightExposure, LightSource, Position,
    SecretDoor, StatusEffects,
};
use crate::constants::*;
use crate::grid::Grid;
use crate::tile::TileType;

/// Every light in the world as `(tile, light)`: entities with a
/// `LightSource`, plus a synthesised glow (`LightSource::burning`) for
/// anything Burning that has no light of its own. The light map and the AI
/// both read lights through this, so they always agree on what is lit.
pub fn light_emitters(world: &World) -> Vec<((i32, i32), LightSource)> {
    let mut lights: Vec<((i32, i32), LightSource)> = world
        .query::<(&Position, &LightSource)>()
        .iter()
        .map(|(_, (pos, light))| ((pos.x, pos.y), *light))
        .collect();
    for (id, (pos, status)) in world
        .query::<(&Position, &StatusEffects)>()
        .without::<&LightSource>()
        .iter()
    {
        if status.effects.iter().any(|e| e.effect_type == EffectType::Burning) {
            lights.push(((pos.x, pos.y), LightSource::burning(stable_phase(id))));
        }
    }
    lights
}

/// A flicker phase that stays the same for an entity from frame to frame, so
/// a synthesised light doesn't strobe. Golden-angle spacing keeps
/// neighbouring ids out of step.
fn stable_phase(id: Entity) -> f32 {
    (id.id() as f32 * 2.399_963).rem_euclid(std::f32::consts::TAU)
}

/// How much light each tile lets through, for one frame's worth of lighting.
///
/// 1.0 is open floor; 0.0 stops light dead (walls, closed and secret doors);
/// in between absorbs some (tall grass, solid objects). Build it once and
/// trace every light through it.
pub struct LightOcclusion {
    width: i32,
    height: i32,
    transmit: Vec<f32>,
}

impl LightOcclusion {
    pub fn build(world: &World, grid: &Grid) -> Self {
        let (width, height) = (grid.width as i32, grid.height as i32);
        let mut transmit: Vec<f32> = grid
            .tiles
            .iter()
            .map(|t| match t.tile_type {
                TileType::Wall | TileType::Empty => 0.0,
                TileType::TallGrass => LIGHT_TALL_GRASS_TRANSMITTANCE,
                _ => 1.0,
            })
            .collect();
        let mut set = |x: i32, y: i32, value: f32| {
            if x >= 0 && y >= 0 && x < width && y < height {
                let i = (y * width + x) as usize;
                transmit[i] = transmit[i].min(value);
            }
        };
        // Solid objects (barrels, chests, stalagmites...) and anything else
        // that blocks sight absorb some light. Creatures don't: they move
        // every turn and their shadows would only flicker around.
        for (_, pos) in world
            .query::<&Position>()
            .with::<&BlocksMovement>()
            .without::<&Actor>()
            .iter()
        {
            set(pos.x, pos.y, LIGHT_OBJECT_TRANSMITTANCE);
        }
        for (_, pos) in world.query::<&Position>().with::<&BlocksVision>().iter() {
            set(pos.x, pos.y, LIGHT_OBJECT_TRANSMITTANCE);
        }
        // Closed and secret doors are as solid as the wall they sit in.
        for (_, (pos, door)) in world.query::<(&Position, &Door)>().iter() {
            if !door.is_open {
                set(pos.x, pos.y, 0.0);
            }
        }
        for (_, pos) in world.query::<&Position>().with::<&SecretDoor>().iter() {
            set(pos.x, pos.y, 0.0);
        }
        Self { width, height, transmit }
    }

    fn at(&self, x: i32, y: i32) -> f32 {
        if x < 0 || y < 0 || x >= self.width || y >= self.height {
            return 0.0;
        }
        self.transmit[(y * self.width + x) as usize]
    }

    /// The fraction of light leaving `from` that arrives at `to`: the product
    /// of what every tile strictly between them lets through, along a
    /// Bresenham line. The endpoints don't count, so a wall's face and the
    /// grass you stand in are lit by a light that can see them.
    pub fn between(&self, from: (i32, i32), to: (i32, i32)) -> f32 {
        let (x1, y1) = to;
        let dx = (x1 - from.0).abs();
        let dy = (y1 - from.1).abs();
        let sx = if from.0 < x1 { 1 } else { -1 };
        let sy = if from.1 < y1 { 1 } else { -1 };
        let mut err = dx - dy;
        let (mut x, mut y) = from;
        let mut carried = 1.0;
        loop {
            let e2 = 2 * err;
            if e2 > -dy {
                err -= dy;
                x += sx;
            }
            if e2 < dx {
                err += dx;
                y += sy;
            }
            if (x, y) == to {
                return carried;
            }
            carried *= self.at(x, y);
            if carried <= 0.0 {
                return 0.0;
            }
        }
    }
}

/// The two parts of a light's falloff at distance `dist`, before occlusion:
/// `(focused, spread)` = `(intensity * t^2, intensity * t)`, `t = 1 - d/r`.
/// Zero outside the radius.
fn falloff(dist: f32, radius: f32, intensity: f32) -> (f32, f32) {
    if radius <= 0.0 || dist >= radius {
        return (0.0, 0.0);
    }
    let t = 1.0 - dist / radius;
    let i = intensity.max(0.0);
    (i * t * t, i * t)
}

/// One light's brightness as the eye sees it, before occlusion: the focused
/// and spread parts weighted the way the shader weights them.
fn brightness(dist: f32, radius: f32, intensity: f32) -> f32 {
    let (focused, spread) = falloff(dist, radius, intensity);
    LIGHT_FOCUSED_WEIGHT * focused + LIGHT_SPREAD_WEIGHT * spread
}

fn distance(a: (i32, i32), b: (i32, i32)) -> f32 {
    let (dx, dy) = ((a.0 - b.0) as f32, (a.1 - b.1) as f32);
    (dx * dx + dy * dy).sqrt()
}

/// One frame's light from the world, per tile, as linear RGB: what every
/// light (with this frame's flicker) contributes to each tile it reaches,
/// after occlusion. The renderer uploads it as a texture and samples it with
/// linear filtering, so light still blends smoothly across tiles.
pub struct LightMap {
    pub width: usize,
    pub height: usize,
    /// `width * height` RGB triples, row-major from tile (0, 0).
    pub rgb: Vec<f32>,
}

impl LightMap {
    /// Accumulate `lights` over the grid. Each light's colour carries its
    /// focused part fully; its spread part is mostly white, taking only
    /// `LIGHT_AMBIENT_TINT` of the colour, so a brazier lays a warm pool over
    /// stone that still reads as stone further out.
    pub fn build(occlusion: &LightOcclusion, lights: &[((i32, i32), LightSource)]) -> Self {
        let (width, height) = (occlusion.width.max(0) as usize, occlusion.height.max(0) as usize);
        let mut rgb = vec![0.0f32; width * height * 3];
        for &(src, light) in lights {
            let (r, g, b) = light.color_or_default();
            let tint = |c: f32| 1.0 + (c - 1.0) * LIGHT_AMBIENT_TINT;
            let reach = light.radius.ceil() as i32;
            for y in (src.1 - reach).max(0)..=(src.1 + reach).min(height as i32 - 1) {
                for x in (src.0 - reach).max(0)..=(src.0 + reach).min(width as i32 - 1) {
                    let (focused, spread) = falloff(distance(src, (x, y)), light.radius, light.intensity);
                    if focused <= 0.0 && spread <= 0.0 {
                        continue;
                    }
                    let carried = occlusion.between(src, (x, y));
                    if carried <= 0.0 {
                        continue;
                    }
                    let f = LIGHT_FOCUSED_WEIGHT * focused * carried;
                    let s = LIGHT_SPREAD_WEIGHT * spread * carried;
                    let i = (y as usize * width + x as usize) * 3;
                    rgb[i] += r * f + tint(r) * s;
                    rgb[i + 1] += g * f + tint(g) * s;
                    rgb[i + 2] += b * f + tint(b) * s;
                }
            }
        }
        Self { width, height, rgb }
    }
}

/// How much of one light reaches tile `pos`: its brightness there (see the
/// module docs for the falloff), times what the tiles in between let through.
pub fn light_reaching(
    occlusion: &LightOcclusion,
    src: (i32, i32),
    light: &LightSource,
    pos: (i32, i32),
) -> f32 {
    let b = brightness(distance(src, pos), light.radius, light.intensity);
    if b <= 0.0 { 0.0 } else { b * occlusion.between(src, pos) }
}

/// How much light from the world reaches tile `pos`: every light's
/// brightness there, after occlusion. Uses steady intensities, not this
/// frame's flicker, so seeded runs stay deterministic. 0 is pitch dark;
/// values above 1 are possible right beside strong lights.
pub fn light_at(world: &World, grid: &Grid, pos: (i32, i32)) -> f32 {
    let occlusion = LightOcclusion::build(world, grid);
    light_emitters(world)
        .into_iter()
        .map(|(src, light)| light_reaching(&occlusion, src, &light, pos))
        .sum()
}

/// Refresh the player's `LightExposure` from where they stand now. Called
/// once per frame from the engine tick, so the AI reads a cached value rather
/// than tracing every light for every enemy.
pub fn update_light_exposure(world: &mut World, grid: &Grid, player: Entity) {
    let Some(pos) = crate::queries::get_entity_position(world, player) else {
        return;
    };
    let level = light_at(world, grid, pos);
    let _ = world.insert_one(player, LightExposure(level));
}

/// How lit a given exposure is, 0..1: 0 in darkness, 1 at `LIGHT_LIT_LEVEL`
/// and above. What the HUD's light bar shows and what the AI scales by.
pub fn lit_fraction(exposure: f32) -> f32 {
    (exposure / LIGHT_LIT_LEVEL).clamp(0.0, 1.0)
}

/// Multiplier on an unaware enemy's alertness gain for a target at this
/// exposure: `DARK_ALERTNESS_MULT` in darkness, rising linearly to
/// `LIT_ALERTNESS_MULT` when fully lit.
pub fn alertness_mult(exposure: f32) -> f32 {
    DARK_ALERTNESS_MULT + (LIT_ALERTNESS_MULT - DARK_ALERTNESS_MULT) * lit_fraction(exposure)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A 15x5 floor with a wall column at x = 6 (gap at y = 2 if `gap`).
    fn walled(gap: bool) -> Grid {
        let mut grid = Grid::filled(15, 5, TileType::Floor);
        for y in 0..5 {
            if !(gap && y == 2) {
                grid.get_mut(6, y).unwrap().tile_type = TileType::Wall;
            }
        }
        grid
    }

    #[test]
    fn a_campfire_lights_a_wide_pool_that_fades_out() {
        let world = {
            let mut w = World::new();
            crate::spawning::spawn_campfire(&mut w, 0, 2);
            w
        };
        let grid = Grid::filled(15, 5, TileType::Floor);
        let at = |x| lit_fraction(light_at(&world, &grid, (x, 2)));
        assert_eq!(at(3), 1.0, "three tiles from a campfire is fully lit");
        assert!(at(5) > 0.4 && at(5) < 0.7, "five tiles out is about half lit, got {}", at(5));
        assert!(at(6) < at(5), "and it keeps fading");
        assert_eq!(at(9), 0.0, "past its radius is dark");
    }

    #[test]
    fn walls_and_closed_doors_stop_light_open_doors_do_not() {
        let mut world = World::new();
        crate::spawning::spawn_brazier(&mut world, 4, 2);
        let grid = walled(true);
        let door = world.spawn((Position::new(6, 2), Door::new()));

        assert_eq!(light_at(&world, &grid, (7, 2)), 0.0, "a closed door blocks light");
        assert_eq!(light_at(&world, &grid, (7, 0)), 0.0, "so does the wall");
        world.get::<&mut Door>(door).unwrap().is_open = true;
        assert!(light_at(&world, &grid, (7, 2)) > 0.0, "an open doorway lets it through");
    }

    #[test]
    fn objects_and_grass_cast_soft_shadows() {
        let mut world = World::new();
        let mut grid = Grid::filled(15, 5, TileType::Floor);
        crate::spawning::spawn_brazier(&mut world, 2, 1);
        crate::spawning::spawn_brazier(&mut world, 2, 3);
        let open = light_at(&world, &grid, (5, 1));

        // A barrel-like object between the lower brazier and (5, 3).
        world.spawn((Position::new(3, 3), BlocksMovement));
        let behind_object = light_at(&world, &grid, (5, 3));
        assert!(behind_object < open && behind_object > 0.0, "dimmed, not blacked out");

        // Tall grass between the upper brazier and (5, 1).
        grid.get_mut(3, 1).unwrap().tile_type = TileType::TallGrass;
        let behind_grass = light_at(&world, &grid, (5, 1));
        assert!(behind_grass < open && behind_grass > 0.0, "grass dims light too");
    }

    #[test]
    fn creatures_cast_no_shadow() {
        let mut world = World::new();
        let grid = Grid::filled(15, 5, TileType::Floor);
        crate::spawning::spawn_brazier(&mut world, 2, 2);
        let before = light_at(&world, &grid, (5, 2));
        let mut rng = <rand::rngs::StdRng as rand::SeedableRng>::seed_from_u64(1);
        crate::spawning::enemies::ORC.spawn(&mut world, 3, 2, &mut rng);
        assert_eq!(light_at(&world, &grid, (5, 2)), before);
    }

    #[test]
    fn burning_things_glow() {
        let mut world = World::new();
        let grid = Grid::filled(9, 9, TileType::Floor);
        let e = world.spawn((Position::new(4, 4), StatusEffects::new()));
        assert_eq!(light_at(&world, &grid, (4, 4)), 0.0);

        crate::systems::effects::add_effect_to_entity(&mut world, e, EffectType::Burning, 5.0);
        assert_eq!(lit_fraction(light_at(&world, &grid, (4, 4))), 1.0, "lit by its own flames");
        assert!(light_at(&world, &grid, (5, 4)) > 0.0, "and lights its neighbours");
    }

    #[test]
    fn the_light_map_matches_the_gameplay_light() {
        let mut world = World::new();
        let grid = walled(false);
        crate::spawning::spawn_campfire(&mut world, 2, 2);
        let occlusion = LightOcclusion::build(&world, &grid);
        let map = LightMap::build(&occlusion, &light_emitters(&world));
        let rgb = |x: usize, y: usize| {
            let i = (y * map.width + x) * 3;
            (map.rgb[i], map.rgb[i + 1], map.rgb[i + 2])
        };
        assert!(rgb(4, 2).0 > 0.0, "lit in front of the wall");
        assert_eq!(rgb(8, 2), (0.0, 0.0, 0.0), "dark behind it, on screen too");
    }

    #[test]
    fn darkness_slows_noticing_and_light_speeds_it() {
        assert_eq!(alertness_mult(0.0), DARK_ALERTNESS_MULT);
        assert_eq!(alertness_mult(LIGHT_LIT_LEVEL * 3.0), LIT_ALERTNESS_MULT);
        let mid = alertness_mult(LIGHT_LIT_LEVEL / 2.0);
        assert!(DARK_ALERTNESS_MULT < mid && mid < LIT_ALERTNESS_MULT);
    }
}
