//! Spatial cache for efficient blocking position lookups.
//!
//! Maintains persistent per-tile blocker counts that are updated incrementally
//! rather than rebuilt on every query.

use std::collections::hash_map::Entry;
use std::collections::HashMap;

use hecs::{Entity, World};

use crate::components::{BlocksMovement, BlocksVision, Position};

/// How many entities block a given tile, keyed by tile.
///
/// A tile is present iff its count is at least 1, so the key set is exactly the
/// set of blocked tiles. See [`SpatialCache`] for why the count matters.
type BlockerCounts = HashMap<(i32, i32), u32>;

/// Add one blocker to `pos`.
fn acquire(counts: &mut BlockerCounts, pos: (i32, i32)) {
    *counts.entry(pos).or_insert(0) += 1;
}

/// Release one blocker from `pos`, dropping the tile once nothing blocks it.
///
/// Releasing a tile that holds no count means some earlier path registered a
/// blocker it never accounted for (or released the same one twice), which is a
/// programmer error, not missing content — so it trips a `debug_assert` and is
/// caught by `cargo test` rather than silently healing. Release builds leave
/// the tile unblocked, which is the same answer a fresh rebuild would give.
fn release(counts: &mut BlockerCounts, pos: (i32, i32)) {
    match counts.entry(pos) {
        Entry::Occupied(mut slot) => {
            if *slot.get() <= 1 {
                slot.remove();
            } else {
                *slot.get_mut() -= 1;
            }
        }
        Entry::Vacant(_) => {
            debug_assert!(
                false,
                "SpatialCache: released a blocker at {pos:?} that was never counted — \
                 some register/remove path is unbalanced"
            );
        }
    }
}

/// Cached spatial data for blocking position lookups.
///
/// Instead of iterating all entities on every query, we maintain persistent
/// per-tile counts that are updated incrementally when entities move, spawn or
/// die.
///
/// The counts are the whole point. Several entities can legitimately share a
/// tile — an opened coffin keeps its `BlocksMovement` and the skeleton it
/// releases spawns *on* it — so a plain `HashSet` of positions could not say
/// whether a tile still had a blocker after one of them left. It unblocked the
/// tile for both, and the per-tick coherence check in `GameEngine::tick` turned
/// that into a panic. A count only drops the tile when the last blocker goes.
#[derive(Debug, Clone)]
pub struct SpatialCache {
    /// Blockers per tile for movement (entities with BlocksMovement + Position)
    blocking_counts: BlockerCounts,

    /// Blockers per tile for vision (entities with BlocksVision + Position)
    vision_counts: BlockerCounts,

    /// Entity -> Position mapping for fast lookup during removal.
    ///
    /// Holds *every* registered entity, including ones that block nothing, so
    /// that "registered" means "tracked" and a later `set_blocking_flags` has
    /// a position to work from.
    entity_positions: HashMap<Entity, (i32, i32)>,

    /// Entity -> blocking flags for knowing what to update
    entity_flags: HashMap<Entity, (bool, bool)>, // (blocks_movement, blocks_vision)
}

impl SpatialCache {
    /// Create a new empty spatial cache.
    pub fn new() -> Self {
        Self {
            blocking_counts: BlockerCounts::new(),
            vision_counts: BlockerCounts::new(),
            entity_positions: HashMap::new(),
            entity_flags: HashMap::new(),
        }
    }

    /// Build cache from current world state.
    /// Called on initialization and floor transitions.
    pub fn rebuild_from_world(world: &World) -> Self {
        let mut cache = Self::new();
        cache.rebuild_in_place(world);
        cache
    }

    /// Rebuild cache in place from current world state.
    /// Called after floor transitions and other world-altering operations.
    ///
    /// Must agree with incremental maintenance down to the counts, since that
    /// equivalence is what the tick's coherence check compares.
    pub fn rebuild_in_place(&mut self, world: &World) {
        self.blocking_counts.clear();
        self.vision_counts.clear();
        self.entity_positions.clear();
        self.entity_flags.clear();

        // Register all entities with BlocksMovement
        for (entity, (pos, _)) in world.query::<(&Position, &BlocksMovement)>().iter() {
            let position = (pos.x, pos.y);
            acquire(&mut self.blocking_counts, position);
            self.entity_positions.insert(entity, position);

            // Check if also blocks vision
            let blocks_vision = world.get::<&BlocksVision>(entity).is_ok();
            self.entity_flags.insert(entity, (true, blocks_vision));

            if blocks_vision {
                acquire(&mut self.vision_counts, position);
            }
        }

        // Register entities that only block vision (no BlocksMovement)
        for (entity, (pos, _)) in world.query::<(&Position, &BlocksVision)>().iter() {
            let position = (pos.x, pos.y);

            // Skip if already registered (has BlocksMovement)
            if self.entity_flags.contains_key(&entity) {
                continue;
            }

            acquire(&mut self.vision_counts, position);
            self.entity_positions.insert(entity, position);
            self.entity_flags.insert(entity, (false, true));
        }
    }

    /// Register a new entity with the cache.
    /// Called when spawning entities.
    ///
    /// Tracks the position unconditionally, even for an entity that blocks
    /// nothing (a tamed companion, a raised skeleton). Those entities used to
    /// be dropped on the floor here, which made `set_blocking_flags` on them a
    /// silent no-op for want of a position to read.
    pub fn register_entity(
        &mut self,
        entity: Entity,
        position: (i32, i32),
        blocks_movement: bool,
        blocks_vision: bool,
    ) {
        if let Some(previous) = self.entity_positions.insert(entity, position) {
            // Re-registering an already-tracked entity: drop its old
            // contribution first so the counts don't drift upward.
            if let Some((was_movement, was_vision)) = self.entity_flags.get(&entity).copied() {
                if was_movement {
                    release(&mut self.blocking_counts, previous);
                }
                if was_vision {
                    release(&mut self.vision_counts, previous);
                }
            }
        }
        self.entity_flags
            .insert(entity, (blocks_movement, blocks_vision));

        if blocks_movement {
            acquire(&mut self.blocking_counts, position);
        }
        if blocks_vision {
            acquire(&mut self.vision_counts, position);
        }
    }

    /// Update an entity's position in the cache.
    /// Called when entities move (apply_move, apply_blink).
    pub fn update_position(&mut self, entity: Entity, old_pos: (i32, i32), new_pos: (i32, i32)) {
        let Some(&(blocks_movement, blocks_vision)) = self.entity_flags.get(&entity) else {
            return; // Entity not tracked
        };

        // `old_pos` is redundant with what the cache already recorded, and a
        // mismatch means some *earlier* path moved this entity without telling
        // the cache — releasing `old_pos` here would then decrement a tile the
        // entity never held. Catching it at the mutation site names the mover;
        // catching it at the tick's coherence check only says the cache drifted.
        debug_assert_eq!(
            self.entity_positions.get(&entity).copied(),
            Some(old_pos),
            "SpatialCache::update_position: {entity:?} is cached at a different tile \
             than the caller's old_pos — an earlier move skipped the cache"
        );

        // Update position mapping
        self.entity_positions.insert(entity, new_pos);

        // Move this entity's contribution from the old tile to the new one.
        // Co-located blockers keep the old tile blocked on their own count.
        if blocks_movement {
            release(&mut self.blocking_counts, old_pos);
            acquire(&mut self.blocking_counts, new_pos);
        }
        if blocks_vision {
            release(&mut self.vision_counts, old_pos);
            acquire(&mut self.vision_counts, new_pos);
        }
    }

    /// Remove an entity from the cache.
    /// Called when entities die or are despawned.
    pub fn remove_entity(&mut self, entity: Entity) {
        let Some(position) = self.entity_positions.remove(&entity) else {
            return; // Entity not tracked
        };

        let Some((blocks_movement, blocks_vision)) = self.entity_flags.remove(&entity) else {
            return;
        };

        if blocks_movement {
            release(&mut self.blocking_counts, position);
        }
        if blocks_vision {
            release(&mut self.vision_counts, position);
        }
    }

    /// Set blocking flags for an entity (e.g., when door closes).
    /// Inverse of `clear_blocking_flags`: adds the entity back into the counts.
    pub fn set_blocking_flags(
        &mut self,
        entity: Entity,
        blocks_movement: bool,
        blocks_vision: bool,
    ) {
        let Some(position) = self.entity_positions.get(&entity).copied() else {
            return;
        };

        if let Some(flags) = self.entity_flags.get_mut(&entity) {
            if blocks_movement && !flags.0 {
                acquire(&mut self.blocking_counts, position);
                flags.0 = true;
            }
            if blocks_vision && !flags.1 {
                acquire(&mut self.vision_counts, position);
                flags.1 = true;
            }
        }
    }

    /// Clear blocking flags for an entity (e.g., when door opens).
    /// Keeps the entity tracked but drops its contribution to the counts.
    pub fn clear_blocking_flags(&mut self, entity: Entity) {
        let Some(position) = self.entity_positions.get(&entity).copied() else {
            return;
        };

        if let Some((blocks_movement, blocks_vision)) = self.entity_flags.get_mut(&entity) {
            if *blocks_movement {
                release(&mut self.blocking_counts, position);
                *blocks_movement = false;
            }
            if *blocks_vision {
                release(&mut self.vision_counts, position);
                *blocks_vision = false;
            }
        }
    }

    /// Every tile blocked for movement.
    ///
    /// For callers that need an owned, *edited* set — AI pathfinding subtracts
    /// the actors it is willing to walk through. Membership tests should use
    /// [`Self::is_blocked`] instead of collecting this.
    #[inline]
    pub fn blocked_tiles(&self) -> impl Iterator<Item = (i32, i32)> + '_ {
        self.blocking_counts.keys().copied()
    }

    /// Check if a position is blocked for movement.
    #[inline]
    pub fn is_blocked(&self, pos: (i32, i32)) -> bool {
        self.blocking_counts.contains_key(&pos)
    }

    /// Check if a position blocks line of sight.
    #[inline]
    pub fn blocks_vision(&self, pos: (i32, i32)) -> bool {
        self.vision_counts.contains_key(&pos)
    }

    /// Check if a position is blocked for movement, ignoring one entity
    /// (normally the entity trying to move there).
    ///
    /// The counts say *how many* entities block a tile but not *which*, so the
    /// exclusion is resolved through `entity_positions` rather than by
    /// rescanning the world:
    ///
    /// - tile not blocked: nothing blocks it, regardless of `exclude`;
    /// - excluded entity is not standing on the tile: the blocker is somebody
    ///   else, so the tile is blocked. This is the case every real move takes,
    ///   since a mover's target tile is never its current tile;
    /// - excluded entity *is* standing on the tile: only then do we check
    ///   whether another tracked blocker shares it.
    pub fn is_blocked_excluding(&self, pos: (i32, i32), exclude: Option<Entity>) -> bool {
        if !self.is_blocked(pos) {
            return false;
        }
        let Some(excluded) = exclude else {
            return true;
        };
        if self.entity_positions.get(&excluded) != Some(&pos) {
            return true;
        }
        self.entity_positions.iter().any(|(&entity, &entity_pos)| {
            entity != excluded
                && entity_pos == pos
                && self.entity_flags.get(&entity).is_some_and(|&(moves, _)| moves)
        })
    }

    /// Debug-only coherence check: the incrementally-maintained counts must
    /// match what a fresh rebuild from the world would produce. Any divergence
    /// means some mutation path (spawn, move, despawn, flag change) failed to
    /// update the cache.
    ///
    /// Compares *counts*, not just which tiles are blocked, so a tile that two
    /// entities share has to be accounted for twice. A key-set comparison would
    /// pass while the cache was one release away from unblocking an occupied
    /// tile — exactly the bug the counts exist to prevent.
    ///
    /// `entity_positions` / `entity_flags` are deliberately not compared: they
    /// may legitimately hold entities a rebuild would drop — an opened door
    /// keeps its tracking entry with both flags cleared so `set_blocking_flags`
    /// can restore it when it shuts, and a companion that blocks nothing stays
    /// tracked so it keeps a position.
    ///
    /// Compiled out of release builds along with the tick's `debug_assert`,
    /// but kept for tests in any profile.
    #[cfg(any(test, debug_assertions))]
    pub fn assert_coherent_with_world(&self, world: &World, context: &str) {
        let fresh = Self::rebuild_from_world(world);
        assert_counts_match(
            &self.blocking_counts,
            &fresh.blocking_counts,
            "blocking_counts",
            context,
        );
        assert_counts_match(
            &self.vision_counts,
            &fresh.vision_counts,
            "vision_counts",
            context,
        );
    }
}

/// Compare one cached count map against a freshly rebuilt one, reporting every
/// tile that disagrees and by how much.
#[cfg(any(test, debug_assertions))]
fn assert_counts_match(
    cached: &BlockerCounts,
    fresh: &BlockerCounts,
    what: &str,
    context: &str,
) {
    let mut disagreements: Vec<((i32, i32), u32, u32)> = cached
        .keys()
        .chain(fresh.keys())
        .copied()
        .collect::<std::collections::BTreeSet<_>>()
        .into_iter()
        .filter_map(|pos| {
            let have = cached.get(&pos).copied().unwrap_or(0);
            let want = fresh.get(&pos).copied().unwrap_or(0);
            (have != want).then_some((pos, have, want))
        })
        .collect();
    disagreements.sort_unstable();
    assert!(
        disagreements.is_empty(),
        "{context}: {what} drifted from a fresh rebuild\n  \
         (tile, cached, world): {disagreements:?}"
    );
}

impl Default for SpatialCache {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use super::*;
    use crate::components::{BlocksMovement, ChaseAI, Position};
    use crate::pathfinding;

    /// The blocked tiles as an owned set, for `pathfinding::find_path`, which
    /// takes the same edited set AI builds via `ai_pathfinding_blocked`.
    fn blocked_set(cache: &SpatialCache) -> HashSet<(i32, i32)> {
        cache.blocked_tiles().collect()
    }

    /// Helper: create a simple floor grid for pathfinding tests.
    fn make_corridor_grid() -> crate::grid::Grid {
        use crate::tile::{Tile, TileType};
        // 10x3 grid: walls on top and bottom rows, floor corridor in the middle
        let width = 10;
        let height = 3;
        let mut tiles = Vec::with_capacity(width * height);
        for y in 0..height {
            for _x in 0..width {
                if y == 0 || y == 2 {
                    tiles.push(Tile::new(TileType::Wall));
                } else {
                    tiles.push(Tile::new(TileType::Floor));
                }
            }
        }
        crate::grid::Grid {
            width,
            height,
            tiles,
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

    #[test]
    fn test_is_blocked_excluding_matches_a_world_scan() {
        // The cache is now the single source of truth for movement blocking, so
        // its exclude-aware answer must match what a linear world scan (the old
        // `queries::is_position_blocked` implementation) would have returned.
        let mut world = World::new();
        let mover = world.spawn((Position { x: 2, y: 1 }, BlocksMovement));
        let other = world.spawn((Position { x: 5, y: 1 }, BlocksMovement));
        // A non-blocking entity must never make a tile look blocked.
        world.spawn((Position { x: 7, y: 1 },));

        let cache = SpatialCache::rebuild_from_world(&world);

        /// The pre-fix implementation, kept here as the oracle.
        fn world_scan(world: &World, pos: (i32, i32), exclude: Option<Entity>) -> bool {
            world
                .query::<(&Position, &BlocksMovement)>()
                .iter()
                .any(|(id, (p, _))| {
                    (p.x, p.y) == pos && (exclude != Some(id))
                })
        }

        for x in 0..10 {
            for exclude in [None, Some(mover), Some(other)] {
                let pos = (x, 1);
                assert_eq!(
                    cache.is_blocked_excluding(pos, exclude),
                    world_scan(&world, pos, exclude),
                    "cache and world scan disagree at {pos:?} excluding {exclude:?}"
                );
            }
        }

        // Spot-check the semantics the loop above encodes.
        assert!(!cache.is_blocked_excluding((0, 1), None), "empty tile");
        assert!(cache.is_blocked_excluding((2, 1), None), "blocker, no exclusion");
        assert!(
            !cache.is_blocked_excluding((2, 1), Some(mover)),
            "a tile blocked only by the excluded entity is passable for it"
        );
        assert!(
            cache.is_blocked_excluding((5, 1), Some(mover)),
            "excluding the mover must not unblock somebody else's tile"
        );
        assert!(
            !cache.is_blocked_excluding((7, 1), None),
            "entity without BlocksMovement must not block"
        );
    }

    #[test]
    fn test_co_located_blockers_stay_blocked_when_one_is_excluded() {
        // blocking_positions is a set, so two blockers on one tile collapse to a
        // single entry. Excluding one of them must still report the tile blocked
        // by the other, which is why is_blocked_excluding consults
        // entity_positions instead of trusting the set alone.
        let mut world = World::new();
        let first = world.spawn((Position { x: 4, y: 1 }, BlocksMovement));
        let second = world.spawn((Position { x: 4, y: 1 }, BlocksMovement));

        let cache = SpatialCache::rebuild_from_world(&world);

        assert!(cache.is_blocked_excluding((4, 1), Some(first)));
        assert!(cache.is_blocked_excluding((4, 1), Some(second)));
        assert!(cache.is_blocked_excluding((4, 1), None));
    }

    #[test]
    fn test_despawned_blocker_unblocks_tile_for_movement_and_pathfinding() {
        // Bug 1 + Bug 2 together: because movement now reads the cache, a
        // despawn that forgets remove_entity would block the *player* too, not
        // just AI pathfinding. Removing it properly must free the tile for both.
        let mut world = World::new();
        let blocker = world.spawn((Position { x: 5, y: 1 }, BlocksMovement));
        let mut cache = SpatialCache::rebuild_from_world(&world);

        assert!(crate::queries::is_position_blocked(&cache, 5, 1, None));

        cache.remove_entity(blocker);
        let _ = world.despawn(blocker);

        assert!(
            !crate::queries::is_position_blocked(&cache, 5, 1, None),
            "movement must see the despawned blocker's tile as free"
        );
        cache.assert_coherent_with_world(&world, "after despawning a blocker");

        let grid = make_corridor_grid();
        assert!(
            pathfinding::find_path(&grid, (1, 1), (8, 1), &blocked_set(&cache))
                .is_some(),
            "pathfinding must also see the tile as free"
        );
    }

    #[test]
    fn test_moved_entity_should_not_block_old_position() {
        // When an entity moves, its old position should no longer be blocked
        // and its new position should be blocked.
        let mut world = World::new();

        let entity = world.spawn((
            Position { x: 3, y: 1 },
            BlocksMovement,
        ));

        // Build spatial cache from world state
        let mut cache = SpatialCache::rebuild_from_world(&world);
        assert!(cache.is_blocked((3, 1)), "initial position should be blocked");

        // Simulate a move: update Position in ECS and spatial cache
        // (this is what apply_move now does)
        if let Ok(mut pos) = world.get::<&mut Position>(entity) {
            let from = (pos.x, pos.y);
            pos.x = 4;
            pos.y = 1;
            cache.update_position(entity, from, (4, 1));
        }

        // After a move, old position should be clear and new position should be blocked
        assert!(
            !cache.is_blocked((3, 1)),
            "old position should no longer be blocked after move"
        );
        assert!(
            cache.is_blocked((4, 1)),
            "new position should be blocked after move"
        );

        // Pathfinding through the old position should succeed (no ghost blocker)
        let grid = make_corridor_grid();
        let path = pathfinding::find_path(&grid, (1, 1), (3, 1), &blocked_set(&cache));
        assert!(
            path.is_some(),
            "pathfinding should succeed — no ghost blocker at old position"
        );
    }

    #[test]
    fn test_dead_entity_should_not_block_position() {
        // When an enemy dies (BlocksMovement removed), its position should
        // no longer be blocked in the spatial cache.
        let mut world = World::new();

        let enemy = world.spawn((
            Position { x: 5, y: 1 },
            BlocksMovement,
            ChaseAI::new(8),
        ));

        let mut cache = SpatialCache::rebuild_from_world(&world);
        assert!(cache.is_blocked((5, 1)), "enemy position should be blocked");

        // Simulate death: remove from spatial cache then remove components
        // (this is what remove_dead_entities now does)
        cache.remove_entity(enemy);
        let _ = world.remove_one::<BlocksMovement>(enemy);
        let _ = world.remove_one::<ChaseAI>(enemy);

        // Dead entity's position should no longer be blocked
        assert!(
            !cache.is_blocked((5, 1)),
            "dead enemy position should no longer be blocked"
        );

        // Pathfinding through the corridor should succeed
        let grid = make_corridor_grid();
        let path = pathfinding::find_path(&grid, (1, 1), (8, 1), &blocked_set(&cache));
        assert!(
            path.is_some(),
            "pathfinding should succeed — dead enemy doesn't block"
        );
    }

    #[test]
    fn test_player_corridor_walk_should_not_leave_ghost_blockers() {
        // When the player walks through a corridor, enemies should be able
        // to pathfind through tiles the player previously occupied.
        let mut world = World::new();

        let player = world.spawn((
            Position { x: 2, y: 1 },
            BlocksMovement,
        ));

        let mut cache = SpatialCache::rebuild_from_world(&world);
        assert!(cache.is_blocked((2, 1)));

        // Player walks from (2,1) -> (3,1) -> (4,1) -> (5,1)
        // Each move updates both Position and spatial cache (as apply_move now does)
        for new_x in 3..=5 {
            if let Ok(mut pos) = world.get::<&mut Position>(player) {
                let from = (pos.x, pos.y);
                pos.x = new_x;
                cache.update_position(player, from, (new_x, 1));
            }
        }

        // Only the player's current position (5,1) should be blocked
        assert!(!cache.is_blocked((2, 1)), "starting pos should be clear");
        assert!(!cache.is_blocked((3, 1)), "passed-through pos should be clear");
        assert!(!cache.is_blocked((4, 1)), "passed-through pos should be clear");
        assert!(cache.is_blocked((5, 1)), "current player pos should be blocked");

        // An enemy at (1,1) should be able to pathfind toward (4,1)
        let grid = make_corridor_grid();
        let path = pathfinding::find_path(&grid, (1, 1), (4, 1), &blocked_set(&cache));
        assert!(
            path.is_some(),
            "enemy should pathfind through corridor without ghost blockers"
        );
    }

    // =========================================================================
    // STACKED BLOCKERS
    //
    // Several entities can legitimately share a tile. The real case is an
    // opened coffin: it keeps its `BlocksMovement`, and the skeleton it
    // releases spawns *on* it (see `apply_open_chest` -> `CoffinSkeletonSpawn`,
    // which takes the container's own position). A set of positions could not
    // represent that, so whichever of the two left first unblocked the tile for
    // both.
    // =========================================================================

    /// Two blockers on one tile; one walks away. The tile stays blocked by the
    /// one still standing there.
    #[test]
    fn two_blockers_on_one_tile_stay_blocked_when_one_leaves() {
        let mut world = World::new();
        let p = Position::new(5, 5);
        let a = world.spawn((p, BlocksMovement));
        let _b = world.spawn((p, BlocksMovement));
        let mut cache = SpatialCache::rebuild_from_world(&world);

        if let Ok(mut pos) = world.get::<&mut Position>(a) {
            pos.x = 6;
        }
        cache.update_position(a, (5, 5), (6, 5));

        assert!(cache.is_blocked((5, 5)), "the second blocker still stands here");
        assert!(cache.is_blocked((6, 5)), "and the first one blocks where it went");
        cache.assert_coherent_with_world(&world, "after one of two stacked blockers moved");
    }

    /// Same tile, but one blocker is despawned rather than moved.
    #[test]
    fn two_blockers_on_one_tile_stay_blocked_when_one_is_removed() {
        let mut world = World::new();
        let p = Position::new(5, 5);
        let a = world.spawn((p, BlocksMovement));
        let b = world.spawn((p, BlocksMovement));
        let mut cache = SpatialCache::rebuild_from_world(&world);

        cache.remove_entity(a);
        let _ = world.despawn(a);
        assert!(cache.is_blocked((5, 5)), "the surviving blocker still blocks");
        cache.assert_coherent_with_world(&world, "after removing one of two stacked blockers");

        cache.remove_entity(b);
        let _ = world.despawn(b);
        assert!(!cache.is_blocked((5, 5)), "the last blocker leaving frees the tile");
        cache.assert_coherent_with_world(&world, "after removing the last blocker");
    }

    /// The coffin case end to end: an opened coffin keeps blocking movement
    /// while the skeleton it released stands on the same tile, so the tile
    /// survives the skeleton walking off it and the skeleton dying on it.
    #[test]
    fn a_skeleton_leaving_its_coffin_tile_does_not_unblock_the_coffin() {
        let mut world = World::new();
        let tile = Position::new(7, 3);
        let coffin = world.spawn((tile, BlocksMovement));
        let mut cache = SpatialCache::rebuild_from_world(&world);

        // The engine spawns the skeleton on the container's own position and
        // registers it as a blocker (see GameEngine::apply_deferred_spawns).
        let skeleton = world.spawn((tile, BlocksMovement));
        cache.register_entity(skeleton, (7, 3), true, false);
        cache.assert_coherent_with_world(&world, "coffin plus skeleton");

        // It steps off.
        if let Ok(mut pos) = world.get::<&mut Position>(skeleton) {
            pos.y = 4;
        }
        cache.update_position(skeleton, (7, 3), (7, 4));
        assert!(cache.is_blocked((7, 3)), "the coffin is still there");
        cache.assert_coherent_with_world(&world, "skeleton stepped off its coffin");

        // And dies.
        cache.remove_entity(skeleton);
        let _ = world.despawn(skeleton);
        assert!(cache.is_blocked((7, 3)), "the coffin outlives the skeleton");
        cache.assert_coherent_with_world(&world, "skeleton died");

        // Only the coffin going away frees the tile.
        cache.remove_entity(coffin);
        let _ = world.despawn(coffin);
        assert!(!cache.is_blocked((7, 3)));
        cache.assert_coherent_with_world(&world, "coffin removed");
    }

    /// Movement and vision are counted separately. A door-like entity that
    /// blocks both, stacked with one that blocks only movement, has to release
    /// each count on its own.
    #[test]
    fn stacked_mixed_blockers_release_movement_and_vision_independently() {
        let mut world = World::new();
        let p = Position::new(2, 2);
        let mover_only = world.spawn((p, BlocksMovement));
        let both = world.spawn((p, BlocksMovement, BlocksVision));
        let mut cache = SpatialCache::rebuild_from_world(&world);

        assert!(cache.is_blocked((2, 2)));
        assert!(cache.blocks_vision((2, 2)));

        // Drop the movement-only one: vision is untouched, movement survives on
        // the other entity's count.
        cache.remove_entity(mover_only);
        let _ = world.despawn(mover_only);
        assert!(cache.is_blocked((2, 2)), "`both` still blocks movement");
        assert!(cache.blocks_vision((2, 2)), "`both` still blocks vision");
        cache.assert_coherent_with_world(&world, "movement-only blocker removed");

        // Drop the other one: now both counts hit zero.
        cache.remove_entity(both);
        let _ = world.despawn(both);
        assert!(!cache.is_blocked((2, 2)));
        assert!(!cache.blocks_vision((2, 2)));
        cache.assert_coherent_with_world(&world, "both blockers removed");
    }

    /// Opening a door on a tile something else also blocks must not unblock the
    /// tile — `clear_blocking_flags` releases one count, not the tile.
    #[test]
    fn clearing_flags_on_one_stacked_blocker_leaves_the_tile_blocked() {
        let mut world = World::new();
        let p = Position::new(4, 8);
        let door = world.spawn((p, BlocksMovement, BlocksVision));
        let _squatter = world.spawn((p, BlocksMovement));
        let mut cache = SpatialCache::rebuild_from_world(&world);

        // The door opens.
        cache.clear_blocking_flags(door);
        let _ = world.remove_one::<BlocksMovement>(door);
        let _ = world.remove_one::<BlocksVision>(door);

        assert!(cache.is_blocked((4, 8)), "the entity in the doorway still blocks");
        assert!(!cache.blocks_vision((4, 8)), "but nothing blocks sight any more");
        cache.assert_coherent_with_world(&world, "door opened under a squatter");

        // And shuts again.
        cache.set_blocking_flags(door, true, true);
        let _ = world.insert(door, (BlocksMovement, BlocksVision));
        assert!(cache.is_blocked((4, 8)));
        assert!(cache.blocks_vision((4, 8)));
        cache.assert_coherent_with_world(&world, "door shut again");
    }

    /// `register_entity` tracks the position even for an entity that blocks
    /// nothing (a tamed companion, a raised skeleton). It used to drop those on
    /// the floor, which made a later `set_blocking_flags` a silent no-op for
    /// want of a position to read.
    #[test]
    fn a_non_blocking_entity_is_still_tracked_so_its_flags_can_be_set_later() {
        let mut world = World::new();
        let p = Position::new(9, 1);
        let companion = world.spawn((p,));
        let mut cache = SpatialCache::new();

        cache.register_entity(companion, (9, 1), false, false);
        assert!(!cache.is_blocked((9, 1)), "it blocks nothing yet");
        cache.assert_coherent_with_world(&world, "non-blocking entity registered");

        // It walks, and the cache keeps up — so a later flag change has the
        // right tile to work from.
        if let Ok(mut pos) = world.get::<&mut Position>(companion) {
            pos.x = 10;
        }
        cache.update_position(companion, (9, 1), (10, 1));

        cache.set_blocking_flags(companion, true, false);
        let _ = world.insert_one(companion, BlocksMovement);
        assert!(
            cache.is_blocked((10, 1)),
            "set_blocking_flags must land on the tile the entity actually moved to"
        );
        assert!(!cache.is_blocked((9, 1)), "and not on the one it started from");
        cache.assert_coherent_with_world(&world, "flags set on a tracked non-blocker");
    }

    /// Property test: hammer the cache with random spawns, moves, despawns and
    /// flag changes on a deliberately cramped grid so blockers stack often, and
    /// check the whole invariant — cache equals a fresh rebuild — after every
    /// single operation.
    ///
    /// This is the invariant `GameEngine::tick` asserts once per tick, so a
    /// failure here is a failure in the real game. Seeded, so a failure
    /// reproduces.
    #[test]
    fn random_operations_keep_the_cache_equal_to_a_rebuild() {
        use rand::{Rng, SeedableRng};

        // Small enough that stacking is the common case, not a rarity.
        const EXTENT: i32 = 4;
        const OPS: usize = 400;

        for seed in 0..12u64 {
            let mut rng = rand::rngs::StdRng::seed_from_u64(seed);
            let mut world = World::new();
            let mut cache = SpatialCache::new();
            // Entities the cache is tracking, with the flags it believes.
            let mut live: Vec<Entity> = Vec::new();

            for op in 0..OPS {
                let choice = if live.is_empty() { 0 } else { rng.gen_range(0..5) };
                let tile = (rng.gen_range(0..EXTENT), rng.gen_range(0..EXTENT));

                match choice {
                    // Spawn, with some mix of blocking flags.
                    0 => {
                        let blocks_movement = rng.gen_bool(0.75);
                        let blocks_vision = rng.gen_bool(0.4);
                        let pos = Position::new(tile.0, tile.1);
                        let entity = world.spawn((pos,));
                        if blocks_movement {
                            let _ = world.insert_one(entity, BlocksMovement);
                        }
                        if blocks_vision {
                            let _ = world.insert_one(entity, BlocksVision);
                        }
                        cache.register_entity(entity, tile, blocks_movement, blocks_vision);
                        live.push(entity);
                    }
                    // Move.
                    1 => {
                        let entity = live[rng.gen_range(0..live.len())];
                        let from = world
                            .get::<&Position>(entity)
                            .map(|p| (p.x, p.y))
                            .expect("tracked entity has a position");
                        if let Ok(mut pos) = world.get::<&mut Position>(entity) {
                            pos.x = tile.0;
                            pos.y = tile.1;
                        }
                        cache.update_position(entity, from, tile);
                    }
                    // Despawn.
                    2 => {
                        let entity = live.swap_remove(rng.gen_range(0..live.len()));
                        cache.remove_entity(entity);
                        let _ = world.despawn(entity);
                    }
                    // A door opening: stop blocking, stay tracked.
                    3 => {
                        let entity = live[rng.gen_range(0..live.len())];
                        cache.clear_blocking_flags(entity);
                        let _ = world.remove_one::<BlocksMovement>(entity);
                        let _ = world.remove_one::<BlocksVision>(entity);
                    }
                    // A door shutting: block again.
                    _ => {
                        let entity = live[rng.gen_range(0..live.len())];
                        cache.set_blocking_flags(entity, true, true);
                        let _ = world.insert(entity, (BlocksMovement, BlocksVision));
                    }
                }

                cache.assert_coherent_with_world(
                    &world,
                    &format!("seed {seed}, operation {op} (kind {choice})"),
                );
            }

            // The run should actually have exercised stacking, or it proves
            // nothing about the counts.
            assert!(
                !live.is_empty(),
                "seed {seed}: every entity was despawned, nothing was stacked"
            );
        }
    }

    /// Guards the guard: the coherence check has to compare counts, not just
    /// which tiles are blocked. A key-set comparison passes while the cache is
    /// one release away from unblocking an occupied tile.
    #[test]
    fn the_coherence_check_notices_a_count_that_is_too_low() {
        let mut world = World::new();
        let p = Position::new(1, 1);
        let a = world.spawn((p, BlocksMovement));
        let _b = world.spawn((p, BlocksMovement));

        let mut undercounted = SpatialCache::rebuild_from_world(&world);
        // Forget one of the two blockers, the way a set-based cache effectively
        // did. The tile still *reads* blocked, so is_blocked and a key-set
        // comparison both look fine.
        undercounted.remove_entity(a);
        assert!(undercounted.is_blocked((1, 1)));

        let fresh = SpatialCache::rebuild_from_world(&world);
        assert_eq!(
            undercounted.blocked_tiles().collect::<HashSet<_>>(),
            fresh.blocked_tiles().collect::<HashSet<_>>(),
            "the two disagree only in the counts, which is the point"
        );

        let drifted = std::panic::catch_unwind(move || {
            undercounted.assert_coherent_with_world(&world, "undercounted");
        });
        assert!(drifted.is_err(), "the count comparison must catch this");
    }
}
