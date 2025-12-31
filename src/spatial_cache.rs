//! Spatial cache for efficient blocking position lookups.
//!
//! Maintains persistent HashSets of blocking positions that are updated
//! incrementally rather than rebuilt on every query.

use std::collections::{HashMap, HashSet};

use hecs::{Entity, World};

use crate::components::{BlocksMovement, BlocksVision, Position};

/// Cached spatial data for blocking position lookups.
///
/// Instead of iterating all entities on every query, we maintain persistent
/// HashSets that are updated incrementally when entities move, spawn, or die.
#[derive(Debug, Clone)]
pub struct SpatialCache {
    /// Positions blocked for movement (entities with BlocksMovement + Position)
    blocking_positions: HashSet<(i32, i32)>,

    /// Positions blocking vision (entities with BlocksVision + Position)
    vision_blocking: HashSet<(i32, i32)>,

    /// Entity -> Position mapping for fast lookup during removal
    entity_positions: HashMap<Entity, (i32, i32)>,

    /// Entity -> blocking flags for knowing what to update
    entity_flags: HashMap<Entity, (bool, bool)>, // (blocks_movement, blocks_vision)
}

impl SpatialCache {
    /// Create a new empty spatial cache.
    pub fn new() -> Self {
        Self {
            blocking_positions: HashSet::new(),
            vision_blocking: HashSet::new(),
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
    pub fn rebuild_in_place(&mut self, world: &World) {
        self.blocking_positions.clear();
        self.vision_blocking.clear();
        self.entity_positions.clear();
        self.entity_flags.clear();

        // Register all entities with BlocksMovement
        for (entity, (pos, _)) in world.query::<(&Position, &BlocksMovement)>().iter() {
            let position = (pos.x, pos.y);
            self.blocking_positions.insert(position);
            self.entity_positions.insert(entity, position);

            // Check if also blocks vision
            let blocks_vision = world.get::<&BlocksVision>(entity).is_ok();
            self.entity_flags.insert(entity, (true, blocks_vision));

            if blocks_vision {
                self.vision_blocking.insert(position);
            }
        }

        // Register entities that only block vision (no BlocksMovement)
        for (entity, (pos, _)) in world.query::<(&Position, &BlocksVision)>().iter() {
            let position = (pos.x, pos.y);

            // Skip if already registered (has BlocksMovement)
            if self.entity_flags.contains_key(&entity) {
                continue;
            }

            self.vision_blocking.insert(position);
            self.entity_positions.insert(entity, position);
            self.entity_flags.insert(entity, (false, true));
        }
    }

    /// Register a new entity with blocking properties.
    /// Called when spawning entities.
    pub fn register_entity(
        &mut self,
        entity: Entity,
        position: (i32, i32),
        blocks_movement: bool,
        blocks_vision: bool,
    ) {
        if !blocks_movement && !blocks_vision {
            return; // Nothing to track
        }

        self.entity_positions.insert(entity, position);
        self.entity_flags.insert(entity, (blocks_movement, blocks_vision));

        if blocks_movement {
            self.blocking_positions.insert(position);
        }
        if blocks_vision {
            self.vision_blocking.insert(position);
        }
    }

    /// Update an entity's position in the cache.
    /// Called when entities move (apply_move, apply_blink).
    pub fn update_position(&mut self, entity: Entity, old_pos: (i32, i32), new_pos: (i32, i32)) {
        let Some(&(blocks_movement, blocks_vision)) = self.entity_flags.get(&entity) else {
            return; // Entity not tracked
        };

        // Update position mapping
        self.entity_positions.insert(entity, new_pos);

        // Update blocking sets
        if blocks_movement {
            self.blocking_positions.remove(&old_pos);
            self.blocking_positions.insert(new_pos);
        }
        if blocks_vision {
            self.vision_blocking.remove(&old_pos);
            self.vision_blocking.insert(new_pos);
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
            self.blocking_positions.remove(&position);
        }
        if blocks_vision {
            self.vision_blocking.remove(&position);
        }
    }

    /// Clear blocking flags for an entity (e.g., when door opens).
    /// Keeps the entity in tracking but removes from blocking sets.
    pub fn clear_blocking_flags(&mut self, entity: Entity) {
        let Some(position) = self.entity_positions.get(&entity).copied() else {
            return;
        };

        if let Some((blocks_movement, blocks_vision)) = self.entity_flags.get_mut(&entity) {
            if *blocks_movement {
                self.blocking_positions.remove(&position);
                *blocks_movement = false;
            }
            if *blocks_vision {
                self.vision_blocking.remove(&position);
                *blocks_vision = false;
            }
        }
    }

    /// Get all positions blocked for movement.
    /// Returns a reference - no allocation!
    #[inline]
    pub fn get_blocking_positions(&self) -> &HashSet<(i32, i32)> {
        &self.blocking_positions
    }

    /// Get all positions blocking vision.
    /// Returns a reference - no allocation!
    #[inline]
    pub fn get_vision_blocking(&self) -> &HashSet<(i32, i32)> {
        &self.vision_blocking
    }

    /// Check if a position is blocked for movement.
    #[inline]
    pub fn is_blocked(&self, pos: (i32, i32)) -> bool {
        self.blocking_positions.contains(&pos)
    }

    /// Check if a position blocks vision.
    #[inline]
    pub fn blocks_vision(&self, pos: (i32, i32)) -> bool {
        self.vision_blocking.contains(&pos)
    }
}

impl Default for SpatialCache {
    fn default() -> Self {
        Self::new()
    }
}
