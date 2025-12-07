//! Data-driven entity spawning system.
//!
//! Defines enemy types and their properties, allowing easy addition of new enemies
//! without modifying spawning code.

use crate::components::{
    Actor, Attackable, BlocksMovement, ChaseAI, Equipment, Health, Position, Sprite, Stats,
    VisualPosition, Weapon,
};
use crate::tile::tile_ids;
use hecs::World;

/// Definition of an enemy type - all the data needed to spawn one
#[derive(Clone)]
pub struct EnemyDef {
    /// Display name (for future UI/logs)
    pub name: &'static str,
    /// Tile ID from the tileset
    pub tile_id: u32,
    /// Maximum health
    pub health: i32,
    /// Action speed (lower = faster, costs less energy to act)
    pub speed: i32,
    /// Sight radius for chase AI
    pub sight_radius: i32,
    /// Base attack damage
    pub damage: i32,
    /// Base stats
    pub strength: i32,
    pub intelligence: i32,
    pub agility: i32,
}

impl EnemyDef {
    /// Spawn this enemy type at the given position
    pub fn spawn(&self, world: &mut World, x: i32, y: i32) -> hecs::Entity {
        let pos = Position::new(x, y);
        world.spawn((
            pos,
            VisualPosition::from_position(&pos),
            Sprite::new(self.tile_id),
            Actor::new(self.speed),
            ChaseAI::new(self.sight_radius),
            Health::new(self.health),
            Stats::new(self.strength, self.intelligence, self.agility),
            Equipment::with_weapon(Weapon::claws(self.damage)),
            Attackable,
            BlocksMovement,
        ))
    }
}

/// Predefined enemy types
pub mod enemies {
    use super::*;
    use crate::constants::*;

    pub const SKELETON: EnemyDef = EnemyDef {
        name: "Skeleton",
        tile_id: tile_ids::SKELETON,
        health: SKELETON_HEALTH,
        speed: SKELETON_SPEED,
        sight_radius: SKELETON_SIGHT_RADIUS,
        damage: SKELETON_DAMAGE,
        strength: SKELETON_STRENGTH,
        intelligence: SKELETON_INTELLIGENCE,
        agility: SKELETON_AGILITY,
    };

    // Easy to add more enemy types:
    // pub const ZOMBIE: EnemyDef = EnemyDef { ... };
    // pub const ORC: EnemyDef = EnemyDef { ... };
    // pub const GOBLIN: EnemyDef = EnemyDef { ... };
}

/// Spawn configuration for a dungeon level
pub struct SpawnConfig {
    pub entries: Vec<SpawnEntry>,
}

/// A single spawn entry: which enemy and how many
pub struct SpawnEntry {
    pub enemy: EnemyDef,
    pub count: usize,
}

impl SpawnConfig {
    /// Create a default spawn config for the first dungeon level
    pub fn level_1() -> Self {
        use crate::constants::SKELETON_SPAWN_COUNT;
        Self {
            entries: vec![SpawnEntry {
                enemy: enemies::SKELETON.clone(),
                count: SKELETON_SPAWN_COUNT,
            }],
        }
    }

    /// Spawn all enemies according to this config
    /// Returns the number of enemies spawned
    pub fn spawn_all(
        &self,
        world: &mut World,
        walkable_tiles: &[(i32, i32)],
        excluded_positions: &[(i32, i32)],
        rng: &mut impl rand::Rng,
    ) -> usize {
        let mut spawned = 0;
        let mut used_positions: Vec<(i32, i32)> = excluded_positions.to_vec();

        for entry in &self.entries {
            for _ in 0..entry.count {
                // Find a valid spawn position
                let available: Vec<_> = walkable_tiles
                    .iter()
                    .filter(|pos| !used_positions.contains(pos))
                    .collect();

                if available.is_empty() {
                    break;
                }

                let &(x, y) = available[rng.gen_range(0..available.len())];
                entry.enemy.spawn(world, x, y);
                used_positions.push((x, y));
                spawned += 1;
            }
        }

        spawned
    }
}
