//! Game event system for decoupled communication between systems.
//!
//! Actions and systems emit events, other systems consume them.
//! This allows VFX, audio, UI, etc. to react without tight coupling.

use hecs::Entity;

/// Game events that systems can emit and subscribe to
#[derive(Debug, Clone)]
pub enum GameEvent {
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
    /// An entity opened a chest/container
    ChestOpened {
        chest: Entity,
        opener: Entity,
    },
    /// Player leveled up
    LevelUp {
        new_level: u32,
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
