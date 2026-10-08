//! Tile occupancy while a floor is being built.
//!
//! The invariant this exists to hold is a world-level one over components:
//! **at most one entity with [`BlocksMovement`] per tile, at the moment floor
//! construction finishes.** Two blockers on a tile is a tile neither occupant
//! can be pushed off, a monster standing inside the scenery, and — before the
//! [`crate::spatial_cache::SpatialCache`] learned to reference-count — a tile
//! that silently unblocked for both as soon as the first of them moved.
//!
//! Nothing in the grid's position lists guarantees it. `chest_positions`,
//! `barrel_positions`, `coffin_positions`, `furniture_positions` and the rest
//! are rolled independently by generation, and the spawn passes that consume
//! them each place entities on walkable *terrain* with no idea what the
//! previous pass put there.
//!
//! So the invariant is enforced where the entities are created. Every spawn
//! pass that places a movement blocker runs its candidate tile past
//! [`TileOccupancy::claim`] first, which tests and records in one call — there
//! is no separate "remember that I used this tile" step to forget. Passes that
//! place walkable things (traps, water, braziers, mushrooms, decals) do not
//! need it and do not take it.
//!
//! This replaced four separate ad-hoc versions of the same idea: a
//! `skip_positions` slice threaded from oil barrels into barrels, and a
//! one-off `BlocksMovement` world query each in `spawn_all`,
//! `spawn_cave_fauna` and `spawn_boss_encounter`.
//!
//! Distinct from `SpatialCache`, deliberately. The cache answers "what blocks
//! this tile" for the *running* game and has to track entity identity, vision
//! as well as movement, removal, and movement between tiles. Construction-time
//! placement needs none of that: tiles only ever go from free to taken, and
//! nothing is ever asked which entity took one.

use std::collections::HashSet;

use hecs::World;

use crate::components::{BlocksMovement, Position};

/// Which tiles already hold a movement blocker, while a floor is being built.
///
/// Build one per floor with [`Self::from_world`], thread it through the spawn
/// passes, and guard each placement with [`Self::claim`].
#[derive(Debug, Clone, Default)]
pub struct TileOccupancy {
    taken: HashSet<(i32, i32)>,
}

impl TileOccupancy {
    /// Seed from everything already blocking a tile in `world`.
    ///
    /// Floor construction does not start from an empty world: the player is
    /// already spawned, and a floor transition keeps them and their companions
    /// while clearing the rest. Those tiles are taken before the first prop
    /// pass runs.
    pub fn from_world(world: &World) -> Self {
        Self {
            taken: world
                .query::<(&Position, &BlocksMovement)>()
                .iter()
                .map(|(_, (pos, _))| (pos.x, pos.y))
                .collect(),
        }
    }

    /// Whether `tile` is still free for a blocker.
    ///
    /// For picking among many candidates before committing to one; a pass that
    /// has settled on a tile should call [`Self::claim`], which cannot be
    /// separated from recording the result.
    #[inline]
    pub fn is_free(&self, tile: (i32, i32)) -> bool {
        !self.taken.contains(&tile)
    }

    /// Claim `tile` for a movement blocker, returning whether it was free.
    ///
    /// Test and record in one call, so a pass cannot check a tile and then
    /// forget to mark it used. The intended shape is a guard:
    ///
    /// ```ignore
    /// for &(x, y) in &grid.barrel_positions {
    ///     if !occupancy.claim((x, y)) {
    ///         continue; // a chest got here first
    ///     }
    ///     // ... spawn the barrel ...
    /// }
    /// ```
    #[inline]
    pub fn claim(&mut self, tile: (i32, i32)) -> bool {
        self.taken.insert(tile)
    }
}

/// Assert that no tile carries more than one [`BlocksMovement`] entity.
///
/// **A construction-time invariant, not a runtime one.** Gameplay stacks
/// blockers on purpose — an opened coffin keeps blocking while the skeleton it
/// releases stands on it — so this must only be called at the end of floor
/// construction, never per tick. The running game's equivalent check is
/// `SpatialCache::assert_coherent_with_world`, which asserts the cache matches
/// the world however many blockers share a tile.
///
/// Compiled out of release builds.
#[cfg(any(test, debug_assertions))]
pub fn assert_one_blocker_per_tile(world: &World, context: &str) {
    use std::collections::HashMap;

    let mut per_tile: HashMap<(i32, i32), usize> = HashMap::new();
    for (_, (pos, _)) in world.query::<(&Position, &BlocksMovement)>().iter() {
        *per_tile.entry((pos.x, pos.y)).or_default() += 1;
    }

    let mut doubled: Vec<((i32, i32), usize)> =
        per_tile.into_iter().filter(|&(_, n)| n > 1).collect();
    doubled.sort_unstable();

    assert!(
        doubled.is_empty(),
        "{context}: {} tile(s) finished floor construction with more than one \
         movement blocker — some spawn pass placed a blocker without claiming \
         its tile through TileOccupancy\n  (tile, blockers): {doubled:?}",
        doubled.len()
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::components::VisualPosition;

    #[test]
    fn claim_reports_the_first_taker_and_refuses_the_rest() {
        let mut occupancy = TileOccupancy::default();

        assert!(occupancy.is_free((2, 3)));
        assert!(occupancy.claim((2, 3)), "first claim on a free tile wins");
        assert!(!occupancy.is_free((2, 3)));
        assert!(
            !occupancy.claim((2, 3)),
            "a second claim on the same tile must be refused"
        );

        // Neighbours are unaffected.
        assert!(occupancy.is_free((2, 4)));
        assert!(occupancy.claim((2, 4)));
    }

    #[test]
    fn from_world_takes_the_tiles_existing_blockers_stand_on() {
        let mut world = World::new();
        let blocked = Position::new(5, 5);
        world.spawn((
            blocked,
            VisualPosition::from_position(&blocked),
            BlocksMovement,
        ));
        // A walkable entity must not reserve its tile.
        let walkable = Position::new(6, 5);
        world.spawn((walkable, VisualPosition::from_position(&walkable)));

        let mut occupancy = TileOccupancy::from_world(&world);

        assert!(!occupancy.is_free((5, 5)), "an existing blocker holds its tile");
        assert!(
            occupancy.is_free((6, 5)),
            "an entity that blocks nothing must not hold a tile"
        );
        assert!(!occupancy.claim((5, 5)));
        assert!(occupancy.claim((6, 5)));
    }

    #[test]
    fn the_invariant_check_accepts_one_blocker_per_tile_and_rejects_two() {
        let mut world = World::new();
        let a = Position::new(1, 1);
        let b = Position::new(2, 2);
        world.spawn((a, VisualPosition::from_position(&a), BlocksMovement));
        world.spawn((b, VisualPosition::from_position(&b), BlocksMovement));
        assert_one_blocker_per_tile(&world, "one each");

        // Stack a second blocker on an occupied tile.
        world.spawn((a, VisualPosition::from_position(&a), BlocksMovement));
        let doubled = std::panic::catch_unwind(move || {
            assert_one_blocker_per_tile(&world, "stacked");
        });
        assert!(doubled.is_err(), "two blockers on a tile must be caught");
    }
}
