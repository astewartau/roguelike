//! Game event system for decoupled communication between systems.
//!
//! Actions and systems emit events, other systems consume them.
//! This allows VFX, audio, UI, etc. to react without tight coupling.

use hecs::Entity;

/// Direction of floor transition
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StairDirection {
    Up,
    Down,
}

/// Game events that systems can emit and subscribe to.
/// Many event fields exist for future handlers (VFX, audio, logging).
#[derive(Debug, Clone)]
#[allow(dead_code)] // Event fields reserved for future handlers
pub enum GameEvent {
    /// An entity moved to a new position
    EntityMoved {
        entity: Entity,
        from: (i32, i32),
        to: (i32, i32),
    },
    /// An entity attacked another entity
    AttackHit {
        attacker: Entity,
        target: Entity,
        target_pos: (f32, f32),
        damage: i32,
    },
    /// An entity died
    EntityDied {
        entity: Entity,
        position: (f32, f32),
    },
    /// An entity opened a door
    DoorOpened {
        door: Entity,
        opener: Entity,
    },
    /// An entity opened a container (chest, bones, etc.)
    ContainerOpened {
        container: Entity,
        opener: Entity,
    },
    /// An entity picked up an item
    ItemPickedUp {
        entity: Entity,
        item: crate::components::ItemType,
    },
    /// An entity picked up gold
    GoldPickedUp {
        entity: Entity,
        amount: u32,
    },
    /// An entity regenerated health
    HealthRegenerated {
        entity: Entity,
        amount: i32,
    },
    /// Player leveled up
    LevelUp {
        new_level: u32,
    },
    /// An entity spent energy to perform an action
    EnergySpent {
        entity: Entity,
        amount: i32,
        remaining: i32,
    },
    /// An entity regenerated energy
    EnergyRegenerated {
        entity: Entity,
        amount: i32,
    },
    /// AI state changed (for debugging/UI feedback)
    AIStateChanged {
        entity: Entity,
        new_state: crate::components::AIState,
    },
    /// A projectile was spawned
    ProjectileSpawned {
        projectile: Entity,
        source: Entity,
    },
    /// A projectile hit something
    ProjectileHit {
        projectile: Entity,
        target: Option<Entity>,
        position: (i32, i32),
        damage: i32,
    },
    /// Player used stairs to change floors
    FloorTransition {
        direction: StairDirection,
        from_floor: u32,
    },
    /// Player initiated dialogue with an NPC
    DialogueStarted {
        npc: Entity,
        player: Entity,
    },
    /// A fireball exploded at a location
    FireballExplosion {
        x: i32,
        y: i32,
        radius: i32,
    },
    /// A potion splashed at a location
    PotionSplash {
        x: i32,
        y: i32,
        potion_type: crate::components::ItemType,
    },
    /// An entity dropped an item on the ground
    ItemDropped {
        entity: Entity,
        item: crate::components::ItemType,
        position: (i32, i32),
    },
    /// A cleave attack was performed (fighter ability)
    CleavePerformed {
        center: (i32, i32),
    },
    /// A skeleton should spawn from a coffin at this position
    CoffinSkeletonSpawn {
        position: (i32, i32),
    },
    /// Taming has started
    TamingStarted {
        tamer: Entity,
        target: Entity,
    },
    /// Taming progress updated
    TamingProgress {
        tamer: Entity,
        target: Entity,
        progress: f32,
        required: f32,
    },
    /// Taming completed successfully
    TamingCompleted {
        tamer: Entity,
        target: Entity,
    },
    /// Taming failed (too far away)
    TamingFailed {
        tamer: Entity,
        target: Entity,
    },
    /// Barkskin ability activated (druid)
    BarkskinActivated {
        entity: Entity,
    },
    /// Player opened a shop with a vendor
    ShopOpened {
        vendor: Entity,
        player: Entity,
    },
    /// Player purchased an item from a vendor
    ItemPurchased {
        vendor: Entity,
        item: crate::components::ItemType,
        price: u32,
    },
    /// Player sold an item to a vendor
    ItemSold {
        vendor: Entity,
        item: crate::components::ItemType,
        value: u32,
    },
}

/// Simple event queue - events are pushed during update, processed at end of frame
#[derive(Default)]
pub struct EventQueue {
    events: Vec<GameEvent>,
}

impl EventQueue {
    pub fn new() -> Self {
        Self { events: Vec::new() }
    }

    /// Push an event to be processed later
    pub fn push(&mut self, event: GameEvent) {
        self.events.push(event);
    }

    /// Drain all events for processing
    pub fn drain(&mut self) -> impl Iterator<Item = GameEvent> + '_ {
        self.events.drain(..)
    }
}
