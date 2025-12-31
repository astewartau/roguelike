//! Active AI entity tracking for performance optimization.
//!
//! Manages which AI entities are "active" (scheduled) vs "dormant" (too far from player).
//! Dormant entities don't get scheduled at all until the player approaches.

use std::collections::HashSet;

use hecs::{Entity, World};

use crate::components::{ChaseAI, CompanionAI, Position};
use crate::constants::AI_ACTIVE_RADIUS;

/// Tracks which AI entities are active (scheduled) vs dormant (not scheduled).
///
/// Instead of scheduling distant entities with a wakeup timer, we simply don't
/// schedule them at all. When the player moves, we scan for newly-in-range
/// entities and schedule them.
#[derive(Debug, Clone)]
pub struct ActiveAITracker {
    /// Entities currently active (within AI_ACTIVE_RADIUS of player)
    active_entities: HashSet<Entity>,

    /// Entities that are dormant (too far from player, not scheduled)
    dormant_entities: HashSet<Entity>,
}

impl ActiveAITracker {
    /// Create a new empty tracker.
    pub fn new() -> Self {
        Self {
            active_entities: HashSet::new(),
            dormant_entities: HashSet::new(),
        }
    }

    /// Register a new AI entity (starts as dormant, will be activated if in range).
    pub fn register_entity(&mut self, entity: Entity) {
        self.dormant_entities.insert(entity);
    }

    /// Mark an entity as active (scheduled).
    pub fn mark_active(&mut self, entity: Entity) {
        self.dormant_entities.remove(&entity);
        self.active_entities.insert(entity);
    }

    /// Mark an entity as dormant (not scheduled).
    pub fn mark_dormant(&mut self, entity: Entity) {
        self.active_entities.remove(&entity);
        self.dormant_entities.insert(entity);
    }

    /// Remove an entity entirely (on death).
    pub fn remove_entity(&mut self, entity: Entity) {
        self.active_entities.remove(&entity);
        self.dormant_entities.remove(&entity);
    }

    /// Check if an entity is currently active.
    #[inline]
    pub fn is_active(&self, entity: Entity) -> bool {
        self.active_entities.contains(&entity)
    }

    /// Check if an entity is dormant.
    #[inline]
    pub fn is_dormant(&self, entity: Entity) -> bool {
        self.dormant_entities.contains(&entity)
    }

    /// Clear all tracking (for floor transitions).
    pub fn clear(&mut self) {
        self.active_entities.clear();
        self.dormant_entities.clear();
    }

    /// Update the active set based on player position.
    /// Returns a list of entities that just became active and need to be scheduled.
    ///
    /// Called after player moves.
    pub fn update_on_player_move(
        &mut self,
        world: &World,
        player_pos: (i32, i32),
    ) -> Vec<Entity> {
        let mut newly_active = Vec::new();

        // Check all AI entities and update their status
        let mut new_active = HashSet::new();
        let mut new_dormant = HashSet::new();

        // Check ChaseAI entities (enemies)
        for (entity, pos) in world.query::<(&Position, &ChaseAI)>().iter().map(|(e, (p, _))| (e, (p.x, p.y))) {
            let distance = (pos.0 - player_pos.0).abs() + (pos.1 - player_pos.1).abs();

            if distance <= AI_ACTIVE_RADIUS {
                new_active.insert(entity);
                // Check if this entity was dormant and is now becoming active
                if self.dormant_entities.contains(&entity) {
                    newly_active.push(entity);
                }
            } else {
                new_dormant.insert(entity);
            }
        }

        // Check CompanionAI entities (tamed animals)
        for (entity, pos) in world.query::<(&Position, &CompanionAI)>().iter().map(|(e, (p, _))| (e, (p.x, p.y))) {
            let distance = (pos.0 - player_pos.0).abs() + (pos.1 - player_pos.1).abs();

            if distance <= AI_ACTIVE_RADIUS {
                new_active.insert(entity);
                if self.dormant_entities.contains(&entity) {
                    newly_active.push(entity);
                }
            } else {
                new_dormant.insert(entity);
            }
        }

        // Update our sets
        self.active_entities = new_active;
        self.dormant_entities = new_dormant;

        newly_active
    }

    /// Initialize from current world state.
    /// Scans all AI entities and categorizes them as active or dormant based on
    /// their distance from the player.
    pub fn initialize_from_world(
        &mut self,
        world: &World,
        player_pos: (i32, i32),
    ) {
        self.clear();

        // Check ChaseAI entities (enemies)
        for (entity, pos) in world.query::<(&Position, &ChaseAI)>().iter().map(|(e, (p, _))| (e, (p.x, p.y))) {
            let distance = (pos.0 - player_pos.0).abs() + (pos.1 - player_pos.1).abs();

            if distance <= AI_ACTIVE_RADIUS {
                self.active_entities.insert(entity);
            } else {
                self.dormant_entities.insert(entity);
            }
        }

        // Check CompanionAI entities (tamed animals)
        for (entity, pos) in world.query::<(&Position, &CompanionAI)>().iter().map(|(e, (p, _))| (e, (p.x, p.y))) {
            let distance = (pos.0 - player_pos.0).abs() + (pos.1 - player_pos.1).abs();

            if distance <= AI_ACTIVE_RADIUS {
                self.active_entities.insert(entity);
            } else {
                self.dormant_entities.insert(entity);
            }
        }
    }

    /// Get all currently active entities.
    pub fn get_active_entities(&self) -> &HashSet<Entity> {
        &self.active_entities
    }

    /// Get all currently dormant entities.
    pub fn get_dormant_entities(&self) -> &HashSet<Entity> {
        &self.dormant_entities
    }
}

impl Default for ActiveAITracker {
    fn default() -> Self {
        Self::new()
    }
}
