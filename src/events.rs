//! Game event system for decoupled communication between systems.
//!
//! Actions and systems emit events, other systems consume them.
//! This allows VFX, audio, UI, etc. to react without tight coupling.

use hecs::Entity;

/// Game events that systems can emit and subscribe to
#[derive(Debug, Clone)]
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

    /// Check if there are pending events
    pub fn is_empty(&self) -> bool {
        self.events.is_empty()
    }
}
