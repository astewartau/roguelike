//! Attack telegraphs: what each hostile is about to hit, and how soon.
//!
//! An attack's effect is applied when its action *completes*, and the game
//! clock only moves while the player is acting, so a swing in progress is a
//! fixed, readable fact while the player decides. This module turns the
//! `ActionInProgress` of every hostile into a description of the tiles it
//! threatens; the UI draws it and the animation system leans the attacker
//! with it. Fusing oil barrels are telegraphed the same way (their blast
//! radius, filling as the fuse burns). Nothing here changes simulation state.

use hecs::{Entity, World};

use crate::components::{ActionType, Actor, BarrelFuse, ChaseAI, OilBarrel, Position, TamedBy};
use crate::constants::*;

/// Which tiles an in-progress attack will hit when it lands.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TelegraphShape {
    /// A single-tile melee swing.
    Tile((i32, i32)),
    /// Every tile within `radius` (Chebyshev) of `center`.
    Area { center: (i32, i32), radius: i32 },
    /// A charge lane: the `len` tiles stepping from `from` (exclusive) along
    /// the unit direction `dir`. Not clipped at walls here (this module has
    /// no grid); the UI stops drawing it at the first one.
    Lane { from: (i32, i32), dir: (i32, i32), len: i32 },
}

impl TelegraphShape {
    /// Every tile the shape covers.
    pub fn tiles(&self) -> Vec<(i32, i32)> {
        match *self {
            TelegraphShape::Tile(t) => vec![t],
            TelegraphShape::Area { center, radius } => {
                let mut out = Vec::with_capacity(((2 * radius + 1) * (2 * radius + 1)) as usize);
                for dy in -radius..=radius {
                    for dx in -radius..=radius {
                        out.push((center.0 + dx, center.1 + dy));
                    }
                }
                out
            }
            TelegraphShape::Lane { from, dir, len } => {
                (1..=len).map(|i| (from.0 + dir.0 * i, from.1 + dir.1 * i)).collect()
            }
        }
    }
}

/// One hostile attack in progress.
#[derive(Debug, Clone, Copy)]
pub struct Telegraph {
    pub attacker: Entity,
    /// Where the attacker stands.
    pub attacker_pos: (i32, i32),
    pub shape: TelegraphShape,
    /// 0.0 when the attack started, 1.0 when it lands (game time).
    pub progress: f32,
    /// Game seconds until it lands.
    pub remaining: f32,
}

/// Every hostile (ChaseAI, not tamed) attack in progress at game time `now`.
///
/// A locked-target melee swing is only reported while it would still
/// connect (see `actions::melee_reach_check`): once the target has stepped
/// out of reach the swing will whiff, and drawing a threat on the target
/// would lie about it.
pub fn hostile_telegraphs(world: &World, now: f32) -> Vec<Telegraph> {
    let mut out = Vec::new();
    for (attacker, (actor, pos, _)) in world
        .query::<(&Actor, &Position, &ChaseAI)>()
        .without::<&TamedBy>()
        .iter()
    {
        let Some(action) = actor.current_action else {
            continue;
        };
        let attacker_pos = (pos.x, pos.y);
        let shape = match action.action_type {
            ActionType::Attack { target } => {
                if crate::systems::actions::melee_reach_check(world, attacker, target).is_err() {
                    continue;
                }
                match world.get::<&Position>(target) {
                    Ok(t) => TelegraphShape::Tile((t.x, t.y)),
                    Err(_) => continue,
                }
            }
            ActionType::AttackDirection { dx, dy } => {
                TelegraphShape::Tile((pos.x + dx, pos.y + dy))
            }
            ActionType::BossGroundSlam => TelegraphShape::Area {
                center: attacker_pos,
                radius: BOSS_SLAM_RADIUS,
            },
            // The whole dash: anything hostile standing anywhere in it when
            // the wind-up completes is charged down.
            ActionType::OrcChargeWindup { dx, dy } => TelegraphShape::Lane {
                from: attacker_pos,
                dir: (dx, dy),
                len: ORC_CHARGE_MAX_RANGE + 1,
            },
            _ => continue,
        };
        out.push(Telegraph {
            attacker,
            attacker_pos,
            shape,
            progress: progress(action.start_time, action.completion_time, now),
            remaining: (action.completion_time - now).max(0.0),
        });
    }
    out
}

/// Every oil barrel with a running fuse, as an area telegraph over its blast
/// radius (`OIL_BARREL_EXPLOSION_RADIUS`) that fills as the fuse burns down.
/// The barrel stands in for the "attacker". Fuses tick in game time (see
/// `systems::fire`), so this is frozen while the player decides.
pub fn barrel_fuse_telegraphs(world: &World) -> Vec<Telegraph> {
    world
        .query::<(&Position, &BarrelFuse, &OilBarrel)>()
        .iter()
        .map(|(barrel, (pos, fuse, _))| Telegraph {
            attacker: barrel,
            attacker_pos: (pos.x, pos.y),
            shape: TelegraphShape::Area {
                center: (pos.x, pos.y),
                radius: OIL_BARREL_EXPLOSION_RADIUS,
            },
            progress: fuse.progress(),
            remaining: fuse.remaining.max(0.0),
        })
        .collect()
}

/// Everything about to hurt someone: hostile attacks in progress plus
/// fusing oil barrels. This is what the UI draws.
pub fn all_telegraphs(world: &World, now: f32) -> Vec<Telegraph> {
    let mut out = hostile_telegraphs(world, now);
    out.extend(barrel_fuse_telegraphs(world));
    out
}

/// Fraction of `[start, end]` elapsed at `now`, clamped to 0..=1. A
/// zero-length action is complete.
pub fn progress(start: f32, end: f32, now: f32) -> f32 {
    let span = end - start;
    if span <= f32::EPSILON {
        return 1.0;
    }
    ((now - start) / span).clamp(0.0, 1.0)
}

/// How far each telegraphing melee attacker should lean toward its target
/// right now, in tiles: a unit vector toward the target tile scaled by
/// `ATTACK_TELEGRAPH_LEAN * progress`. A charging orc leans down its lane.
/// Area attacks (the slam) do not lean.
pub fn lean_offsets(world: &World, now: f32) -> Vec<(Entity, (f32, f32))> {
    hostile_telegraphs(world, now)
        .into_iter()
        .filter_map(|t| {
            let (dx, dy) = match t.shape {
                TelegraphShape::Tile(tile) => {
                    (tile.0 - t.attacker_pos.0, tile.1 - t.attacker_pos.1)
                }
                TelegraphShape::Lane { dir, .. } => dir,
                TelegraphShape::Area { .. } => return None,
            };
            let (dx, dy) = (dx as f32, dy as f32);
            let len = (dx * dx + dy * dy).sqrt();
            if len <= f32::EPSILON {
                return None;
            }
            let amount = ATTACK_TELEGRAPH_LEAN * t.progress;
            Some((t.attacker, (dx / len * amount, dy / len * amount)))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::components::{ActionInProgress, Attackable, Health};

    fn hostile(world: &mut World, x: i32, y: i32) -> Entity {
        world.spawn((Position::new(x, y), Actor::new(1.0), ChaseAI::new(8), Health::new(5)))
    }

    fn set_action(world: &mut World, e: Entity, action_type: ActionType, start: f32, end: f32) {
        world.get::<&mut Actor>(e).unwrap().current_action =
            Some(ActionInProgress { action_type, start_time: start, completion_time: end });
    }

    #[test]
    fn melee_telegraph_tracks_progress_in_game_time_and_marks_the_target_tile() {
        let mut world = World::new();
        let target = world.spawn((Position::new(5, 6), Attackable, Health::new(10)));
        let orc = hostile(&mut world, 5, 5);
        set_action(&mut world, orc, ActionType::Attack { target }, 1.0, 2.0);

        let t = hostile_telegraphs(&world, 1.25);
        assert_eq!(t.len(), 1);
        assert_eq!(t[0].shape, TelegraphShape::Tile((5, 6)));
        assert!((t[0].progress - 0.25).abs() < 1e-5);
        assert!((t[0].remaining - 0.75).abs() < 1e-5);

        // Leaning toward the target, a quarter of the full lean.
        let lean = lean_offsets(&world, 1.25);
        assert_eq!(lean.len(), 1);
        assert!((lean[0].1 .1 - ATTACK_TELEGRAPH_LEAN * 0.25).abs() < 1e-5);
    }

    #[test]
    fn a_swing_that_will_whiff_is_not_telegraphed() {
        let mut world = World::new();
        let target = world.spawn((Position::new(5, 8), Attackable, Health::new(10)));
        let orc = hostile(&mut world, 5, 5);
        set_action(&mut world, orc, ActionType::Attack { target }, 0.0, 1.0);
        assert!(hostile_telegraphs(&world, 0.5).is_empty());
    }

    #[test]
    fn tamed_companions_and_idle_hostiles_are_not_telegraphed() {
        let mut world = World::new();
        let target = world.spawn((Position::new(5, 6), Attackable, Health::new(10)));
        // Adjacent but not swinging.
        hostile(&mut world, 4, 6);
        // Swinging, but a tamed companion.
        let pet = hostile(&mut world, 5, 5);
        world.insert_one(pet, TamedBy { owner: target }).unwrap();
        set_action(&mut world, pet, ActionType::Attack { target }, 0.0, 1.0);
        assert!(hostile_telegraphs(&world, 0.5).is_empty());
    }

    #[test]
    fn slam_wind_up_telegraphs_the_full_radius() {
        let mut world = World::new();
        let boss = hostile(&mut world, 10, 10);
        set_action(&mut world, boss, ActionType::BossGroundSlam, 0.0, 1.0);
        let t = hostile_telegraphs(&world, 0.5);
        assert_eq!(t.len(), 1);
        let tiles = t[0].shape.tiles();
        let side = (2 * BOSS_SLAM_RADIUS + 1) as usize;
        assert_eq!(tiles.len(), side * side);
        assert!(tiles.contains(&(10 + BOSS_SLAM_RADIUS, 10 - BOSS_SLAM_RADIUS)));
        assert!(lean_offsets(&world, 0.5).is_empty(), "the slam does not lean");
    }
}
