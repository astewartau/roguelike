//! Data-driven entity spawning system.
//!
//! Defines enemy types and their properties, allowing easy addition of new enemies
//! without modifying spawning code.

use crate::components::{
    Actor, Attackable, BlocksMovement, ChaseAI, Equipment, Health, OverlaySprite, Position,
    RangedWeapon, Sprite, Stats, VisualPosition, Weapon,
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
    /// Maximum energy pool
    pub max_energy: i32,
    /// Action speed multiplier (higher = faster)
    pub speed: f32,
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
            Actor::new(self.max_energy, self.speed),
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
        max_energy: SKELETON_MAX_ENERGY,
        speed: SKELETON_SPEED,
        sight_radius: SKELETON_SIGHT_RADIUS,
        damage: SKELETON_DAMAGE,
        strength: SKELETON_STRENGTH,
        intelligence: SKELETON_INTELLIGENCE,
        agility: SKELETON_AGILITY,
    };

    pub const RAT: EnemyDef = EnemyDef {
        name: "Rat",
        tile_id: tile_ids::RAT,
        health: RAT_HEALTH,
        max_energy: RAT_MAX_ENERGY,
        speed: RAT_SPEED,
        sight_radius: RAT_SIGHT_RADIUS,
        damage: RAT_DAMAGE,
        strength: RAT_STRENGTH,
        intelligence: RAT_INTELLIGENCE,
        agility: RAT_AGILITY,
    };

    /// Spawn a skeleton archer (special enemy with ranged attack and bow overlay)
    pub fn spawn_skeleton_archer(world: &mut World, x: i32, y: i32) -> hecs::Entity {
        let pos = Position::new(x, y);
        world.spawn((
            pos,
            VisualPosition::from_position(&pos),
            Sprite::new(tile_ids::SKELETON),
            OverlaySprite::new(tile_ids::BOW), // Bow displayed on top
            Actor::new(SKELETON_ARCHER_MAX_ENERGY, SKELETON_ARCHER_SPEED),
            ChaseAI::with_ranged(
                SKELETON_ARCHER_SIGHT_RADIUS,
                SKELETON_ARCHER_MIN_RANGE,
                SKELETON_ARCHER_MAX_RANGE,
            ),
            Health::new(SKELETON_ARCHER_HEALTH),
            Stats::new(
                SKELETON_ARCHER_STRENGTH,
                SKELETON_ARCHER_INTELLIGENCE,
                SKELETON_ARCHER_AGILITY,
            ),
            Equipment::with_weapons(
                Weapon::claws(SKELETON_ARCHER_MELEE_DAMAGE),
                RangedWeapon::enemy_bow(SKELETON_ARCHER_BOW_DAMAGE),
            ),
            Attackable,
            BlocksMovement,
        ))
    }
}

/// Spawn configuration for a dungeon level
pub struct SpawnConfig {
    pub entries: Vec<SpawnEntry>,
    /// Number of skeleton archers to spawn (handled separately due to custom spawn)
    pub skeleton_archer_count: usize,
}

/// A single spawn entry: which enemy and how many
pub struct SpawnEntry {
    pub enemy: EnemyDef,
    pub count: usize,
}

impl SpawnConfig {
    /// Create a default spawn config for the first dungeon level
    pub fn level_1() -> Self {
        use crate::constants::{RAT_SPAWN_COUNT, SKELETON_ARCHER_SPAWN_COUNT, SKELETON_SPAWN_COUNT};
        Self {
            entries: vec![
                SpawnEntry {
                    enemy: enemies::RAT.clone(),
                    count: RAT_SPAWN_COUNT,
                },
                SpawnEntry {
                    enemy: enemies::SKELETON.clone(),
                    count: SKELETON_SPAWN_COUNT,
                },
            ],
            skeleton_archer_count: SKELETON_ARCHER_SPAWN_COUNT,
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

        // Spawn regular enemies
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

        // Spawn skeleton archers (custom spawn function)
        for _ in 0..self.skeleton_archer_count {
            let available: Vec<_> = walkable_tiles
                .iter()
                .filter(|pos| !used_positions.contains(pos))
                .collect();

            if available.is_empty() {
                break;
            }

            let &(x, y) = available[rng.gen_range(0..available.len())];
            enemies::spawn_skeleton_archer(world, x, y);
            used_positions.push((x, y));
            spawned += 1;
        }

        spawned
    }
}
