//! Action effect implementations.
//!
//! This module contains the logic for applying action effects when actions complete.
//! These are called by the time system after an action's duration has elapsed.
//!
//! Split by category:
//! - `movement`: moving, doors, stairs, directional interaction
//! - `combat`: melee attacks, cleave, stun
//! - `items`: equip/unequip/drop
//! - `abilities`: class abilities (blink, fireball, taming, life drain, etc.)
//! - `kit`: per-class kit abilities (guard, bone ward, sacrifice, corpse
//!   explosion, thorns, entangle), including the start-of-action hook that
//!   reactive abilities use
//! - `traps`: trap placement/triggering and container opening
//! - `projectiles`: bow shots, thrown potions, projectile paths

mod abilities;
mod combat;
mod items;
mod kit;
mod movement;
mod projectiles;
mod traps;

pub use abilities::*;
pub use combat::*;
pub use items::*;
pub use kit::*;
pub use movement::*;
pub use projectiles::*;
pub use traps::*;

/// The action-level test arena (player + full simulation context), shared
/// with tests outside this module (AI, grabs, splits).
#[cfg(test)]
pub(crate) use combat::tests::Arena as TestArena;

use hecs::Entity;

use crate::events::{EventQueue, GameEvent};

/// Result of applying an action's effects
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ActionResult {
    /// Action completed successfully
    Completed,
    /// Movement was blocked
    Blocked,
    /// Entity doesn't exist or has no action
    Invalid,
}

/// Apply talk to NPC effect - emits dialogue started event
pub fn apply_talk_to(player: Entity, npc: Entity, events: &mut EventQueue) -> ActionResult {
    events.push(GameEvent::DialogueStarted { npc, player });
    ActionResult::Completed
}
