//! Data-driven entity spawning system.
//!
//! Defines enemy types and their properties, allowing easy addition of new enemies
//! without modifying spawning code. Also defines NPC types with dialogue.

use crate::components::{
    Actor, Attackable, BlocksMovement, ChaseAI, Dialogue, DialogueAction, DialogueNode, DialogueOption,
    Equipment, FriendlyNPC, Health, LightSource, OverlaySprite, Position, RangedWeapon, Sprite, Stats,
    StatusEffects, Tameable, VisualPosition, Vendor, Weapon,
};
use crate::tile::{tile_ids, SpriteSheet};
use hecs::World;

/// Ranged attack configuration for enemies
#[derive(Clone, Copy)]
pub struct RangedConfig {
    /// Minimum range to use ranged attack
    pub min_range: i32,
    /// Maximum range for ranged attack
    pub max_range: i32,
    /// Ranged weapon damage
    pub damage: i32,
}

/// Definition of an enemy type - all the data needed to spawn one
#[derive(Clone)]
pub struct EnemyDef {
    /// Display name (for future UI/logs)
    #[allow(dead_code)] // Reserved for combat log/bestiary
    pub name: &'static str,
    /// Sprite sheet and tile ID
    pub sprite: (SpriteSheet, u32),
    /// Optional overlay sprite (e.g., bow for archers)
    pub overlay_sprite: Option<(SpriteSheet, u32)>,
    /// Maximum health
    pub health: i32,
    /// Maximum energy pool
    pub max_energy: i32,
    /// Action speed multiplier (higher = faster)
    pub speed: f32,
    /// Sight radius for chase AI
    pub sight_radius: i32,
    /// Base melee attack damage
    pub damage: i32,
    /// Base stats
    pub strength: i32,
    pub intelligence: i32,
    pub agility: i32,
    /// Optional ranged attack configuration
    pub ranged: Option<RangedConfig>,
    /// Whether this enemy can be tamed (for Druid ability)
    pub tameable: bool,
}

impl EnemyDef {
    /// Spawn this enemy type at the given position
    pub fn spawn(&self, world: &mut World, x: i32, y: i32) -> hecs::Entity {
        let pos = Position::new(x, y);

        // Build base components
        let sprite = Sprite::from_ref(self.sprite);
        let actor = Actor::new(self.max_energy, self.speed);
        let health = Health::new(self.health);
        let stats = Stats::new(self.strength, self.intelligence, self.agility);
        let status_effects = StatusEffects::new();

        // Build AI and equipment based on whether enemy has ranged capability
        let (chase_ai, equipment) = if let Some(ranged) = &self.ranged {
            (
                ChaseAI::with_ranged(self.sight_radius, ranged.min_range, ranged.max_range),
                Equipment::with_weapons(
                    Weapon::claws(self.damage),
                    RangedWeapon::enemy_bow(ranged.damage),
                ),
            )
        } else {
            (
                ChaseAI::new(self.sight_radius),
                Equipment::with_weapon(Weapon::claws(self.damage)),
            )
        };

        // Spawn with or without overlay sprite
        let entity = if let Some(overlay_ref) = self.overlay_sprite {
            world.spawn((
                pos,
                VisualPosition::from_position(&pos),
                sprite,
                OverlaySprite::from_ref(overlay_ref),
                actor,
                chase_ai,
                health,
                stats,
                equipment,
                status_effects,
                Attackable,
                BlocksMovement,
            ))
        } else {
            world.spawn((
                pos,
                VisualPosition::from_position(&pos),
                sprite,
                actor,
                chase_ai,
                health,
                stats,
                equipment,
                status_effects,
                Attackable,
                BlocksMovement,
            ))
        };

        // Add Tameable component for animals that can be tamed
        if self.tameable {
            let _ = world.insert_one(entity, Tameable);
        }

        entity
    }
}

/// Predefined enemy types
pub mod enemies {
    use super::*;
    use crate::constants::*;

    pub const SKELETON: EnemyDef = EnemyDef {
        name: "Skeleton",
        sprite: tile_ids::SKELETON,
        overlay_sprite: None,
        health: SKELETON_HEALTH,
        max_energy: SKELETON_MAX_ENERGY,
        speed: SKELETON_SPEED,
        sight_radius: SKELETON_SIGHT_RADIUS,
        damage: SKELETON_DAMAGE,
        strength: SKELETON_STRENGTH,
        intelligence: SKELETON_INTELLIGENCE,
        agility: SKELETON_AGILITY,
        ranged: None,
        tameable: false,
    };

    pub const RAT: EnemyDef = EnemyDef {
        name: "Rat",
        sprite: tile_ids::RAT,
        overlay_sprite: None,
        health: RAT_HEALTH,
        max_energy: RAT_MAX_ENERGY,
        speed: RAT_SPEED,
        sight_radius: RAT_SIGHT_RADIUS,
        damage: RAT_DAMAGE,
        strength: RAT_STRENGTH,
        intelligence: RAT_INTELLIGENCE,
        agility: RAT_AGILITY,
        ranged: None,
        tameable: true, // Rats are animals and can be tamed by Druids
    };

    pub const SKELETON_ARCHER: EnemyDef = EnemyDef {
        name: "Skeleton Archer",
        sprite: tile_ids::SKELETON,
        overlay_sprite: Some(tile_ids::BOW),
        health: SKELETON_ARCHER_HEALTH,
        max_energy: SKELETON_ARCHER_MAX_ENERGY,
        speed: SKELETON_ARCHER_SPEED,
        sight_radius: SKELETON_ARCHER_SIGHT_RADIUS,
        damage: SKELETON_ARCHER_MELEE_DAMAGE,
        strength: SKELETON_ARCHER_STRENGTH,
        intelligence: SKELETON_ARCHER_INTELLIGENCE,
        agility: SKELETON_ARCHER_AGILITY,
        ranged: Some(RangedConfig {
            min_range: SKELETON_ARCHER_MIN_RANGE,
            max_range: SKELETON_ARCHER_MAX_RANGE,
            damage: SKELETON_ARCHER_BOW_DAMAGE,
        }),
        tameable: false,
    };
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
                SpawnEntry {
                    enemy: enemies::SKELETON_ARCHER.clone(),
                    count: SKELETON_ARCHER_SPAWN_COUNT,
                },
            ],
        }
    }

    /// Spawn all enemies according to this config
    /// Returns the number of enemies spawned
    ///
    /// - `excluded_positions`: Individual tiles to exclude (e.g., player spawn)
    /// - `excluded_room`: Optional room rectangle to exclude entirely (e.g., starting room)
    pub fn spawn_all(
        &self,
        world: &mut World,
        walkable_tiles: &[(i32, i32)],
        excluded_positions: &[(i32, i32)],
        excluded_room: Option<&crate::dungeon_gen::Rect>,
        rng: &mut impl rand::Rng,
    ) -> usize {
        let mut spawned = 0;
        let mut used_positions: Vec<(i32, i32)> = excluded_positions.to_vec();

        // Helper to check if a position is in the excluded room
        let is_in_excluded_room = |x: i32, y: i32| -> bool {
            excluded_room.map(|r| r.contains(x, y)).unwrap_or(false)
        };

        // Spawn all enemies using the unified template system
        for entry in &self.entries {
            for _ in 0..entry.count {
                // Find a valid spawn position (not in used positions and not in excluded room)
                let available: Vec<_> = walkable_tiles
                    .iter()
                    .filter(|&&(x, y)| !used_positions.contains(&(x, y)) && !is_in_excluded_room(x, y))
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

// =============================================================================
// NPC SPAWNING
// =============================================================================

/// Definition of an NPC type - data needed to spawn a friendly NPC
pub struct NPCDef {
    /// Display name (shown in dialogue window)
    #[allow(dead_code)] // Reserved for dialogue header
    pub name: &'static str,
    /// Sprite sheet and tile ID
    pub sprite: (SpriteSheet, u32),
    /// Function to create the NPC's dialogue tree
    pub dialogue_fn: fn() -> Dialogue,
}

impl NPCDef {
    /// Spawn this NPC type at the given position
    pub fn spawn(&self, world: &mut World, x: i32, y: i32) -> hecs::Entity {
        let pos = Position::new(x, y);
        world.spawn((
            pos,
            VisualPosition::from_position(&pos),
            Sprite::from_ref(self.sprite),
            FriendlyNPC,
            (self.dialogue_fn)(),
            BlocksMovement,
        ))
    }
}

/// Predefined NPC types
pub mod npcs {
    use super::*;

    /// Create the wizard's dialogue tree
    fn wizard_dialogue() -> Dialogue {
        Dialogue::new(
            "Old Wizard",
            vec![
                // Node 0: Greeting
                DialogueNode {
                    text: "Greetings, adventurer! I am the last survivor of this cursed dungeon. \
                           Beware - the creatures here grow stronger the deeper you venture."
                        .to_string(),
                    options: vec![
                        DialogueOption {
                            label: "Any advice for survival?".to_string(),
                            next_node: Some(1),
                            action: DialogueAction::None,
                        },
                        DialogueOption {
                            label: "Farewell".to_string(),
                            next_node: None,
                            action: DialogueAction::None,
                        },
                    ],
                },
                // Node 1: Advice
                DialogueNode {
                    text: "Collect potions and scrolls from chests. The invisibility scroll can \
                           save your life when surrounded. And watch out for the skeleton archers!"
                        .to_string(),
                    options: vec![
                        DialogueOption {
                            label: "Thank you".to_string(),
                            next_node: None,
                            action: DialogueAction::None,
                        },
                        DialogueOption {
                            label: "Tell me more".to_string(),
                            next_node: Some(2),
                            action: DialogueAction::None,
                        },
                    ],
                },
                // Node 2: More info
                DialogueNode {
                    text: "The stairs lead deeper into the dungeon. Each floor is more dangerous \
                           than the last. Good luck, you'll need it."
                        .to_string(),
                    options: vec![DialogueOption {
                        label: "Farewell".to_string(),
                        next_node: None,
                        action: DialogueAction::None,
                    }],
                },
            ],
        )
    }

    pub const WIZARD: NPCDef = NPCDef {
        name: "Old Wizard",
        sprite: tile_ids::WIZARD,
        dialogue_fn: wizard_dialogue,
    };
}

// =============================================================================
// VENDOR DEFINITIONS
// =============================================================================

/// Definition of a vendor NPC - sells/buys items
pub struct VendorDef {
    #[allow(dead_code)] // Reserved for future vendor-specific UI
    pub name: &'static str,
    pub sprite: (SpriteSheet, u32),
    pub dialogue_fn: fn() -> crate::components::Dialogue,
    pub inventory_fn: fn(u32) -> Vec<(crate::components::ItemType, u32)>,
    pub starting_gold: u32,
}

impl VendorDef {
    /// Spawn this vendor at the given position
    pub fn spawn(&self, world: &mut World, x: i32, y: i32, floor_num: u32) -> hecs::Entity {
        let pos = Position::new(x, y);
        let inventory = (self.inventory_fn)(floor_num);
        world.spawn((
            pos,
            VisualPosition::from_position(&pos),
            Sprite::from_ref(self.sprite),
            FriendlyNPC,
            (self.dialogue_fn)(),
            Vendor::new(inventory, self.starting_gold),
            BlocksMovement,
        ))
    }
}

pub mod vendors {
    use super::*;
    use crate::components::ItemType;

    fn merchant_dialogue() -> crate::components::Dialogue {
        use crate::components::{Dialogue, DialogueNode, DialogueOption};

        Dialogue::new(
            "Wandering Merchant",
            vec![DialogueNode {
                text: "Welcome, traveler! I've got rare goods from the surface. \
                       Care to browse my wares?"
                    .to_string(),
                options: vec![
                    DialogueOption {
                        label: "Show me what you have".to_string(),
                        next_node: None,
                        action: DialogueAction::OpenShop,
                    },
                    DialogueOption {
                        label: "Not right now".to_string(),
                        next_node: None,
                        action: DialogueAction::None,
                    },
                ],
            }],
        )
    }

    fn merchant_inventory(floor_num: u32) -> Vec<(ItemType, u32)> {
        match floor_num {
            0..=1 => vec![
                (ItemType::HealthPotion, 3),
                (ItemType::RegenerationPotion, 1),
                (ItemType::Bread, 2),
                (ItemType::ScrollOfSpeed, 1),
                (ItemType::ScrollOfProtection, 1),
            ],
            2..=3 => vec![
                (ItemType::HealthPotion, 2),
                (ItemType::StrengthPotion, 2),
                (ItemType::ScrollOfInvisibility, 1),
                (ItemType::ScrollOfBlink, 1),
                (ItemType::Dagger, 1),
            ],
            _ => vec![
                (ItemType::HealthPotion, 3),
                (ItemType::StrengthPotion, 2),
                (ItemType::ScrollOfFireball, 1),
                (ItemType::ScrollOfFear, 1),
                (ItemType::Sword, 1),
            ],
        }
    }

    pub const MERCHANT: VendorDef = VendorDef {
        name: "Wandering Merchant",
        sprite: tile_ids::DWARF,
        dialogue_fn: merchant_dialogue,
        inventory_fn: merchant_inventory,
        starting_gold: 500,
    };
}

/// Spawn a campfire entity with light source and animated fire sprite
pub fn spawn_campfire(world: &mut World, x: i32, y: i32) -> hecs::Entity {
    use crate::components::{AnimatedSprite, CausesBurning};

    let pos = Position::new(x, y);
    world.spawn((
        pos,
        VisualPosition::from_position(&pos),
        AnimatedSprite::fire_pit(),
        LightSource::campfire(),
        CausesBurning,
    ))
}

/// Spawn a brazier entity with light source and animated fire sprite
pub fn spawn_brazier(world: &mut World, x: i32, y: i32) -> hecs::Entity {
    use crate::components::{AnimatedSprite, CausesBurning};

    let pos = Position::new(x, y);
    world.spawn((
        pos,
        VisualPosition::from_position(&pos),
        AnimatedSprite::brazier(),
        LightSource::brazier(),
        CausesBurning,
    ))
}
