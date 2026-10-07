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
    /// Display name (used by the message log)
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
    /// Whether this enemy is smart enough to open doors and raise an alarm shout
    pub can_open_doors: bool,
    /// How readily this enemy catches fire (0 = fireproof, e.g. skeletons/slimes)
    pub flammability: f32,
    /// Venomous bite: melee hits apply Slowed for this many seconds (0 = none)
    pub venom_slow: f32,
    /// Support caster (Goblin Shaman): kites and heals/hastes allies
    pub support: bool,
    /// Spider-kin: ignores webs and periodically lays them
    pub spider: bool,
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
        let (mut chase_ai, equipment) = if let Some(ranged) = &self.ranged {
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
        // Enemies start unaware of the player (asleep or idly patrolling).
        chase_ai.state = crate::components::AIState::Unaware;

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

        // Name so the message log can refer to this enemy
        let _ = world.insert_one(entity, crate::components::Name::new(self.name));

        // Keep the full template on the entity so floor save/load can restore
        // this exact enemy type when the player revisits the floor.
        let _ = world.insert_one(entity, self.clone());

        // Add Tameable component for animals that can be tamed
        if self.tameable {
            let _ = world.insert_one(entity, Tameable);
        }

        // Most enemies start asleep; the rest are awake but unaware (patrolling).
        if rand::random::<f64>() < crate::constants::SLEEP_CHANCE {
            let _ = world.insert_one(entity, crate::components::Asleep);
        }

        // Smart enemies can open doors and raise alarm shouts.
        if self.can_open_doors {
            let _ = world.insert_one(entity, crate::components::CanOpenDoors);
        }

        // Flammable enemies can catch fire.
        if self.flammability > 0.0 {
            let _ = world.insert_one(
                entity,
                crate::components::Combustible { flammability: self.flammability },
            );
        }

        // Venomous biters apply Slowed on melee hits (enemy melee path).
        if self.venom_slow > 0.0 {
            let _ = world.insert_one(
                entity,
                crate::components::Venomous { slow_duration: self.venom_slow },
            );
        }

        // Support casters (shamans) heal/haste allies and kite.
        if self.support {
            let _ = world.insert_one(
                entity,
                crate::components::SupportAI { cooldown: crate::constants::SHAMAN_SUPPORT_COOLDOWN },
            );
        }

        // Spider-kin ignore webs and periodically lay them.
        if self.spider {
            let _ = world.insert(
                entity,
                (
                    crate::components::Spider,
                    crate::components::WebSpinner {
                        cooldown: crate::constants::WEB_LAY_COOLDOWN,
                        interval: crate::constants::WEB_LAY_COOLDOWN,
                    },
                ),
            );
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
        can_open_doors: true,
        flammability: 0.0, // bone doesn't burn
        venom_slow: 0.0,
        support: false,
        spider: false,
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
        can_open_doors: false,
        flammability: 0.35,
        venom_slow: 0.0,
        support: false,
        spider: false,
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
        can_open_doors: true,
        flammability: 0.0, // bone doesn't burn
        venom_slow: 0.0,
        support: false,
        spider: false,
    };

    pub const GOBLIN: EnemyDef = EnemyDef {
        name: "Goblin",
        sprite: tile_ids::GOBLIN,
        overlay_sprite: None,
        health: GOBLIN_HEALTH,
        max_energy: GOBLIN_MAX_ENERGY,
        speed: GOBLIN_SPEED,
        sight_radius: GOBLIN_SIGHT_RADIUS,
        damage: GOBLIN_DAMAGE,
        strength: GOBLIN_STRENGTH,
        intelligence: GOBLIN_INTELLIGENCE,
        agility: GOBLIN_AGILITY,
        ranged: None,
        tameable: false,
        can_open_doors: true,
        flammability: 0.35,
        venom_slow: 0.0,
        support: false,
        spider: false,
    };

    pub const ORC: EnemyDef = EnemyDef {
        name: "Orc",
        sprite: tile_ids::ORC,
        overlay_sprite: None,
        health: ORC_HEALTH,
        max_energy: ORC_MAX_ENERGY,
        speed: ORC_SPEED,
        sight_radius: ORC_SIGHT_RADIUS,
        damage: ORC_DAMAGE,
        strength: ORC_STRENGTH,
        intelligence: ORC_INTELLIGENCE,
        agility: ORC_AGILITY,
        ranged: None,
        tameable: false,
        can_open_doors: true,
        flammability: 0.35,
        venom_slow: 0.0,
        support: false,
        spider: false,
    };

    pub const ZOMBIE: EnemyDef = EnemyDef {
        name: "Zombie",
        sprite: tile_ids::ZOMBIE,
        overlay_sprite: None,
        health: ZOMBIE_HEALTH,
        max_energy: ZOMBIE_MAX_ENERGY,
        speed: ZOMBIE_SPEED,
        sight_radius: ZOMBIE_SIGHT_RADIUS,
        damage: ZOMBIE_DAMAGE,
        strength: ZOMBIE_STRENGTH,
        intelligence: ZOMBIE_INTELLIGENCE,
        agility: ZOMBIE_AGILITY,
        ranged: None,
        tameable: false,
        can_open_doors: false,
        flammability: 0.2, // rotting flesh, smoulders
        venom_slow: 0.0,
        support: false,
        spider: false,
    };

    pub const BAT: EnemyDef = EnemyDef {
        name: "Giant Bat",
        sprite: tile_ids::BAT,
        overlay_sprite: None,
        health: BAT_HEALTH,
        max_energy: BAT_MAX_ENERGY,
        speed: BAT_SPEED,
        sight_radius: BAT_SIGHT_RADIUS,
        damage: BAT_DAMAGE,
        strength: BAT_STRENGTH,
        intelligence: BAT_INTELLIGENCE,
        agility: BAT_AGILITY,
        ranged: None,
        tameable: true, // Beasts can be tamed by Druids; a fast scout companion
        can_open_doors: false,
        flammability: 0.35,
        venom_slow: 0.0,
        support: false,
        spider: false,
    };

    pub const SLIME: EnemyDef = EnemyDef {
        name: "Slime",
        sprite: tile_ids::SLIME,
        overlay_sprite: None,
        health: SLIME_HEALTH,
        max_energy: SLIME_MAX_ENERGY,
        speed: SLIME_SPEED,
        sight_radius: SLIME_SIGHT_RADIUS,
        damage: SLIME_DAMAGE,
        strength: SLIME_STRENGTH,
        intelligence: SLIME_INTELLIGENCE,
        agility: SLIME_AGILITY,
        ranged: None,
        tameable: false,
        can_open_doors: false,
        flammability: 0.0, // wet, doesn't burn
        venom_slow: 0.0,
        support: false,
        spider: false,
    };

    pub const GOBLIN_SHAMAN: EnemyDef = EnemyDef {
        name: "Goblin Shaman",
        sprite: tile_ids::GOBLIN_SHAMAN,
        overlay_sprite: None,
        health: GOBLIN_SHAMAN_HEALTH,
        max_energy: GOBLIN_SHAMAN_MAX_ENERGY,
        speed: GOBLIN_SHAMAN_SPEED,
        sight_radius: GOBLIN_SHAMAN_SIGHT_RADIUS,
        damage: GOBLIN_SHAMAN_DAMAGE,
        strength: GOBLIN_SHAMAN_STRENGTH,
        intelligence: GOBLIN_SHAMAN_INTELLIGENCE,
        agility: GOBLIN_SHAMAN_AGILITY,
        ranged: None,
        tameable: false,
        can_open_doors: true,
        flammability: 0.35,
        venom_slow: 0.0,
        support: true,
        spider: false,
    };

    pub const LESSER_GIANT_SPIDER: EnemyDef = EnemyDef {
        name: "Lesser Giant Spider",
        sprite: tile_ids::LESSER_GIANT_SPIDER,
        overlay_sprite: None,
        health: LESSER_SPIDER_HEALTH,
        max_energy: LESSER_SPIDER_MAX_ENERGY,
        speed: LESSER_SPIDER_SPEED,
        sight_radius: LESSER_SPIDER_SIGHT_RADIUS,
        damage: LESSER_SPIDER_DAMAGE,
        strength: LESSER_SPIDER_STRENGTH,
        intelligence: LESSER_SPIDER_INTELLIGENCE,
        agility: LESSER_SPIDER_AGILITY,
        ranged: None,
        tameable: false,
        can_open_doors: false,
        flammability: 0.5, // bristly and dry — catches easily
        venom_slow: 0.0,
        support: false,
        spider: true,
    };

    pub const GIANT_SPIDER: EnemyDef = EnemyDef {
        name: "Giant Spider",
        sprite: tile_ids::GIANT_SPIDER,
        overlay_sprite: None,
        health: GIANT_SPIDER_HEALTH,
        max_energy: GIANT_SPIDER_MAX_ENERGY,
        speed: GIANT_SPIDER_SPEED,
        sight_radius: GIANT_SPIDER_SIGHT_RADIUS,
        damage: GIANT_SPIDER_DAMAGE,
        strength: GIANT_SPIDER_STRENGTH,
        intelligence: GIANT_SPIDER_INTELLIGENCE,
        agility: GIANT_SPIDER_AGILITY,
        ranged: None,
        tameable: false,
        can_open_doors: false,
        flammability: 0.5,
        venom_slow: SPIDER_VENOM_SLOW_DURATION,
        support: false,
        spider: true,
    };
}

/// How many Goblin Shamans spawn on a floor: none on floor 0, two on floor 1,
/// then +1 per two floors (floor 3 -> 3, floor 5 -> 4, ...).
pub fn shaman_count_for_floor(floor: u32) -> usize {
    if floor == 0 {
        0
    } else {
        2 + ((floor - 1) / 2) as usize
    }
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
    /// Build a spawn roster appropriate to the given floor depth.
    ///
    /// Early floors lean on weak, fast fodder (rats, goblins, bats); deeper
    /// floors swap in tougher bruisers (orcs, zombies) and more archers, and
    /// scale up the count of heavy enemies so the dungeon gets harder as the
    /// player descends.
    pub fn for_floor(floor: u32) -> Self {
        // Each tuple is (enemy template, count).
        // Counts are tuned for the ~40x40 / ~8-room default floor so density
        // stays comfortable; the smaller map means fewer enemies than a raw
        // count would suggest.
        let mut roster: Vec<(EnemyDef, usize)> = match floor {
            // Floor 0 — gentle introduction
            0 => vec![
                (enemies::RAT.clone(), 10),
                (enemies::GOBLIN.clone(), 8),
                (enemies::BAT.clone(), 5),
                (enemies::SKELETON.clone(), 4),
            ],
            // Floor 1 — skeletons and the first archers show up
            1 => vec![
                (enemies::RAT.clone(), 6),
                (enemies::GOBLIN.clone(), 9),
                (enemies::BAT.clone(), 5),
                (enemies::SKELETON.clone(), 9),
                (enemies::SLIME.clone(), 5),
                (enemies::SKELETON_ARCHER.clone(), 3),
            ],
            // Floor 2 — bruisers arrive
            2 => vec![
                (enemies::GOBLIN.clone(), 6),
                (enemies::SKELETON.clone(), 10),
                (enemies::SLIME.clone(), 8),
                (enemies::ORC.clone(), 4),
                (enemies::ZOMBIE.clone(), 4),
                (enemies::SKELETON_ARCHER.clone(), 4),
            ],
            // Floor 3+ — heavy, and ramps with depth
            deep => {
                let extra = (deep.saturating_sub(3)) as usize;
                vec![
                    (enemies::SKELETON.clone(), 10),
                    (enemies::SLIME.clone(), 6),
                    (enemies::ORC.clone(), 6 + extra),
                    (enemies::ZOMBIE.clone(), 8 + extra),
                    (enemies::BAT.clone(), 4),
                    (enemies::SKELETON_ARCHER.clone(), 5 + extra),
                ]
            }
        };

        // Goblin Shamans: priority-target support casters from floor 1 on.
        let shamans = shaman_count_for_floor(floor);
        if shamans > 0 {
            roster.push((enemies::GOBLIN_SHAMAN.clone(), shamans));
        }

        // Spiders: lesser webspinners from floor 1, venomous giants from floor 3.
        if floor >= 1 {
            roster.push((enemies::LESSER_GIANT_SPIDER.clone(), 3));
        }
        if floor >= 3 {
            roster.push((enemies::GIANT_SPIDER.clone(), 2));
        }

        Self {
            entries: roster
                .into_iter()
                .map(|(enemy, count)| SpawnEntry { enemy, count })
                .collect(),
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
                (ItemType::LeatherArmor, 1),
                (ItemType::Helmet, 1),
                (ItemType::Arrow, 10),
                (ItemType::FireArrow, crate::constants::FIRE_ARROW_BUNDLE_COUNT),
                (ItemType::WaterFlaskFull, 1),
                (ItemType::WaterFlaskEmpty, 1),
            ],
            2..=3 => vec![
                (ItemType::HealthPotion, 2),
                (ItemType::StrengthPotion, 2),
                (ItemType::ScrollOfInvisibility, 1),
                (ItemType::ScrollOfBlink, 1),
                (ItemType::Dagger, 1),
                (ItemType::LeatherArmor, 1),
                (ItemType::Ring, 1),
                (ItemType::Arrow, 15),
                (ItemType::FireArrow, crate::constants::FIRE_ARROW_BUNDLE_COUNT),
                (ItemType::WaterFlaskFull, 1),
                (ItemType::WaterFlaskEmpty, 1),
            ],
            _ => vec![
                (ItemType::HealthPotion, 3),
                (ItemType::StrengthPotion, 2),
                (ItemType::ScrollOfFireball, 1),
                (ItemType::ScrollOfFear, 1),
                (ItemType::Sword, 1),
                (ItemType::ChainMail, 1),
                (ItemType::Ring, 1),
                (ItemType::Amulet, 1),
                (ItemType::Arrow, 20),
                (ItemType::FireArrow, crate::constants::FIRE_ARROW_BUNDLE_COUNT * 2),
                (ItemType::WaterFlaskFull, 2),
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

/// Spawn a burning-grass fire at a tile: a short-lived hazard entity that shows
/// flames, ignites things stepping on it (CausesBurning), and is avoided by AI.
/// When it burns out (see `tick_fire`) its tile reverts to floor.
pub fn spawn_burning_grass(world: &mut World, x: i32, y: i32) -> hecs::Entity {
    use crate::components::{AnimatedSprite, BurningGrass, CausesBurning};

    let pos = Position::new(x, y);
    world.spawn((
        pos,
        VisualPosition::from_position(&pos),
        AnimatedSprite::fire_pit(),
        LightSource::brazier(),
        CausesBurning,
        BurningGrass { remaining: crate::constants::GRASS_BURN_DURATION },
    ))
}

/// Spawn a brazier entity with light source and animated fire sprite.
/// Braziers can be toppled (see `systems::fire::topple_brazier`).
pub fn spawn_brazier(world: &mut World, x: i32, y: i32) -> hecs::Entity {
    use crate::components::{AnimatedSprite, Brazier, CausesBurning};

    let pos = Position::new(x, y);
    world.spawn((
        pos,
        VisualPosition::from_position(&pos),
        AnimatedSprite::brazier(),
        LightSource::brazier(),
        CausesBurning,
        Brazier { lit: true },
    ))
}

/// Spawn an (unlit) oil puddle on a floor tile: walkable, doesn't block
/// vision, and harmless until fire reaches it (see `systems::fire`).
pub fn spawn_oil_puddle(world: &mut World, x: i32, y: i32) -> hecs::Entity {
    use crate::components::{OilPuddle, Sprite, SpriteTint};
    use crate::constants::OIL_PUDDLE_TINT;

    let pos = Position::new(x, y);
    let (r, g, b) = OIL_PUDDLE_TINT;
    world.spawn((
        pos,
        VisualPosition::from_position(&pos),
        Sprite::from_ref(tile_ids::OIL_PUDDLE),
        SpriteTint { r, g, b },
        OilPuddle,
    ))
}

/// Spawn a hidden dungeon floor trap. No sprite: it is invisible until the
/// player detects it (see `systems::discovery`), at which point it gains a
/// tinted trap-door sprite. Walkable — stepping on it triggers it.
pub fn spawn_dungeon_trap(
    world: &mut World,
    x: i32,
    y: i32,
    kind: crate::components::DungeonTrapKind,
) -> hecs::Entity {
    use crate::components::DungeonTrap;

    let pos = Position::new(x, y);
    world.spawn((
        pos,
        VisualPosition::from_position(&pos),
        DungeonTrap { kind, revealed: false },
    ))
}

/// Spawn a piece of room furniture (fountain / altar / shrine). Blocks
/// movement; interacting (bump or Ctrl+direction) uses it — see
/// `systems::furniture`.
pub fn spawn_furniture(
    world: &mut World,
    x: i32,
    y: i32,
    kind: crate::components::FurnitureKind,
) -> hecs::Entity {
    use crate::components::{Furniture, FurnitureKind, Name, SpriteTint};
    use crate::constants::{ALTAR_TINT, FOUNTAIN_TINT, SHRINE_TINT};

    let (sprite, tint, name) = match kind {
        FurnitureKind::Fountain => (tile_ids::FOUNTAIN, FOUNTAIN_TINT, "Fountain"),
        FurnitureKind::Altar => (tile_ids::ALTAR, ALTAR_TINT, "Altar"),
        FurnitureKind::Shrine => (tile_ids::SHRINE, SHRINE_TINT, "Shrine"),
    };

    let pos = Position::new(x, y);
    world.spawn((
        pos,
        VisualPosition::from_position(&pos),
        Sprite::from_ref(sprite),
        SpriteTint { r: tint.0, g: tint.1, b: tint.2 },
        Name::new(name),
        Furniture { kind, used: false },
        BlocksMovement,
    ))
}

/// Spawn a secret door sealing a hidden room: renders as the given wall
/// sprite and blocks movement + vision like a wall. Discovery (see
/// `systems::discovery`) converts it into a normal openable door.
pub fn spawn_secret_door(
    world: &mut World,
    x: i32,
    y: i32,
    wall_sprite: (SpriteSheet, u32),
) -> hecs::Entity {
    use crate::components::{BlocksVision, SecretDoor};

    let pos = Position::new(x, y);
    world.spawn((
        pos,
        VisualPosition::from_position(&pos),
        Sprite::from_ref(wall_sprite),
        SecretDoor,
        BlocksVision,
        BlocksMovement,
    ))
}

/// Spawn a spider web on a floor tile: walkable, renders as pale gauze under
/// actors, roots non-spiders that enter (see `systems::webs`), and is highly
/// flammable (see `systems::fire`).
pub fn spawn_web(
    world: &mut World,
    x: i32,
    y: i32,
    spinner: Option<hecs::Entity>,
) -> hecs::Entity {
    use crate::components::{SpriteTint, Web};
    use crate::constants::WEB_TINT;

    let pos = Position::new(x, y);
    let (r, g, b) = WEB_TINT;
    world.spawn((
        pos,
        VisualPosition::from_position(&pos),
        Sprite::from_ref(tile_ids::WEB),
        SpriteTint { r, g, b },
        Web { spinner },
    ))
}

// =============================================================================
// BOSSES (every 3rd floor)
// =============================================================================

/// The boss roster entry for a floor: base enemy, unique name, and ability.
/// Floors 3/6/9 introduce Gnash/Silkrot/Vhal; deeper floors cycle the roster
/// (with extra scaling per lap — see `boss_def_for_floor`).
pub fn boss_for_floor(floor: u32) -> Option<(&'static str, crate::components::BossAbility)> {
    use crate::components::BossAbility;
    if !is_boss_floor(floor) {
        return None;
    }
    let index = (floor / 3 - 1) % 3;
    Some(match index {
        0 => ("Gnash, Orc Warlord", BossAbility::GroundSlam),
        1 => ("Mother Silkrot", BossAbility::SummonSpiders),
        _ => ("King Vhal the Undying", BossAbility::RaiseDead),
    })
}

/// Bosses appear on every 3rd floor (3, 6, 9, ...).
pub fn is_boss_floor(floor: u32) -> bool {
    floor > 0 && floor.is_multiple_of(3)
}

/// Build the scaled `EnemyDef` for the given boss floor.
fn boss_def_for_floor(floor: u32) -> Option<(EnemyDef, &'static str, crate::components::BossAbility)> {
    use crate::components::BossAbility;
    use crate::constants::*;

    let (name, ability) = boss_for_floor(floor)?;
    let base = match ability {
        BossAbility::GroundSlam => enemies::ORC.clone(),
        BossAbility::SummonSpiders => enemies::GIANT_SPIDER.clone(),
        BossAbility::RaiseDead => enemies::SKELETON.clone(),
    };

    // Extra scaling for each full lap of the roster beyond floors 3/6/9.
    let laps = (floor / 3).saturating_sub(1) / 3;
    let cycle_mult = BOSS_CYCLE_HEALTH_MULT.powi(laps as i32);

    let mut def = base;
    def.health = ((def.health as f32) * BOSS_HEALTH_MULT * cycle_mult).round() as i32;
    def.damage = ((def.damage as f32) * BOSS_DAMAGE_MULT * cycle_mult).round() as i32;
    def.strength = ((def.strength as f32) * BOSS_STAT_MULT).round() as i32;
    def.intelligence = ((def.intelligence as f32) * BOSS_STAT_MULT).round() as i32;
    def.agility = ((def.agility as f32) * BOSS_STAT_MULT).round() as i32;
    def.sight_radius += BOSS_SIGHT_BONUS;
    Some((def, name, ability))
}

/// Ability cooldown for a boss kind.
fn boss_cooldown(ability: crate::components::BossAbility) -> f32 {
    use crate::components::BossAbility;
    use crate::constants::*;
    match ability {
        BossAbility::GroundSlam => BOSS_SLAM_COOLDOWN,
        BossAbility::SummonSpiders => BOSS_SPIDER_SPAWN_COOLDOWN,
        BossAbility::RaiseDead => BOSS_RAISE_COOLDOWN,
    }
}

/// Spawn the boss for this floor at (x, y): the scaled base enemy plus the
/// `Boss` role, its unique name, and fear immunity. Bosses start awake
/// (never asleep) but unaware, prowling their lair. Returns None on
/// non-boss floors.
pub fn spawn_boss(world: &mut World, floor: u32, x: i32, y: i32) -> Option<hecs::Entity> {
    let (def, name, ability) = boss_def_for_floor(floor)?;
    let boss = def.spawn(world, x, y);
    apply_boss_role(world, boss, name, ability, false);
    Some(boss)
}

/// Attach the boss role to an already-spawned enemy: unique display name,
/// `Boss` ability + cooldown, fear immunity, wakefulness, and (for Silkrot)
/// the faster web-lay interval. Shared by `spawn_boss` and floor save/load,
/// which restores bosses on revisited floors.
pub fn apply_boss_role(
    world: &mut World,
    entity: hecs::Entity,
    name: &str,
    ability: crate::components::BossAbility,
    announced: bool,
) {
    use crate::components::{Asleep, Boss, BossAbility, FearImmune, Name, WebSpinner};

    // Awake (asleep=false), just not yet alerted.
    let _ = world.remove_one::<Asleep>(entity);
    // Unique display name replaces the base enemy's.
    let _ = world.insert_one(entity, Name::new(name));
    let _ = world.insert(
        entity,
        (
            Boss { ability, cooldown: boss_cooldown(ability), announced },
            FearImmune,
        ),
    );

    // Silkrot webs her lair constantly.
    if ability == BossAbility::SummonSpiders {
        if let Ok(mut spinner) = world.get::<&mut WebSpinner>(entity) {
            spinner.interval = crate::constants::BOSS_WEB_LAY_COOLDOWN;
            spinner.cooldown = crate::constants::BOSS_WEB_LAY_COOLDOWN;
        }
    }
}

/// Spawn an explosive oil barrel: blocks movement, highly combustible, and
/// attackable (destroying it by damage sets it off — see `systems::fire`).
/// Distinguished from food storage barrels by a red/dark tint.
pub fn spawn_oil_barrel(world: &mut World, x: i32, y: i32) -> hecs::Entity {
    use crate::components::{Combustible, Health, Name, OilBarrel, Sprite, SpriteTint};
    use crate::constants::{OIL_BARREL_FLAMMABILITY, OIL_BARREL_HEALTH, OIL_BARREL_TINT};

    let pos = Position::new(x, y);
    let (r, g, b) = OIL_BARREL_TINT;
    world.spawn((
        pos,
        VisualPosition::from_position(&pos),
        Sprite::from_ref(tile_ids::BARREL),
        SpriteTint { r, g, b },
        Name::new("Oil Barrel"),
        OilBarrel,
        Health::new(OIL_BARREL_HEALTH),
        StatusEffects::new(),
        Combustible { flammability: OIL_BARREL_FLAMMABILITY },
        Attackable,
        BlocksMovement,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::components::BossAbility;

    #[test]
    fn test_boss_floor_selection() {
        // Non-boss floors
        assert!(boss_for_floor(0).is_none());
        assert!(boss_for_floor(1).is_none());
        assert!(boss_for_floor(2).is_none());
        assert!(boss_for_floor(4).is_none());

        // The introductory trio
        assert_eq!(
            boss_for_floor(3),
            Some(("Gnash, Orc Warlord", BossAbility::GroundSlam))
        );
        assert_eq!(
            boss_for_floor(6),
            Some(("Mother Silkrot", BossAbility::SummonSpiders))
        );
        assert_eq!(
            boss_for_floor(9),
            Some(("King Vhal the Undying", BossAbility::RaiseDead))
        );

        // Cycles beyond floor 9
        assert_eq!(
            boss_for_floor(12),
            Some(("Gnash, Orc Warlord", BossAbility::GroundSlam))
        );
        assert_eq!(
            boss_for_floor(18),
            Some(("King Vhal the Undying", BossAbility::RaiseDead))
        );
    }

    #[test]
    fn test_boss_scaling() {
        let (def, _, _) = boss_def_for_floor(3).expect("floor 3 is a boss floor");
        let base = enemies::ORC;
        assert_eq!(def.health, (base.health as f32 * crate::constants::BOSS_HEALTH_MULT).round() as i32);
        assert_eq!(def.damage, (base.damage as f32 * crate::constants::BOSS_DAMAGE_MULT).round() as i32);
        assert_eq!(def.sight_radius, base.sight_radius + crate::constants::BOSS_SIGHT_BONUS);

        // A second lap through the roster is tougher than the first.
        let (lap1, _, _) = boss_def_for_floor(3).expect("boss floor");
        let (lap2, _, _) = boss_def_for_floor(12).expect("boss floor");
        assert!(lap2.health > lap1.health);
    }

    #[test]
    fn test_spawned_boss_has_role_components() {
        let mut world = World::new();
        let boss = spawn_boss(&mut world, 3, 5, 5).expect("boss spawns on floor 3");
        assert!(world.get::<&crate::components::Boss>(boss).is_ok());
        assert!(world.get::<&crate::components::FearImmune>(boss).is_ok());
        // Awake (never asleep), per spec.
        assert!(world.get::<&crate::components::Asleep>(boss).is_err());
        let name = world.get::<&crate::components::Name>(boss).map(|n| n.0.clone());
        assert_eq!(name.ok().as_deref(), Some("Gnash, Orc Warlord"));

        // Non-boss floors spawn nothing.
        assert!(spawn_boss(&mut world, 4, 5, 5).is_none());
    }

    #[test]
    fn test_shaman_count_scaling() {
        assert_eq!(shaman_count_for_floor(0), 0);
        assert_eq!(shaman_count_for_floor(1), 2);
        assert_eq!(shaman_count_for_floor(2), 2);
        assert_eq!(shaman_count_for_floor(3), 3);
        assert_eq!(shaman_count_for_floor(5), 4);
    }
}
