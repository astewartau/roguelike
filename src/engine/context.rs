//! Borrow-splitting contexts for the simulation.
//!
//! The simulation's mutable state is split across two owners: `GameState`
//! holds the world, grid, clock, scheduler, caches and rng, while `GameEngine`
//! holds the event queue, vfx, UI state, input state and audio. The borrow
//! checker therefore forces every call site to spell the split out by hand,
//! which is why simulation functions grew to 10-14 parameters each.
//!
//! These two structs bundle the split once so call sites don't have to:
//!
//! * [`ActorCtx`] - what it takes to schedule and drive actors: AI decisions,
//!   spawn initialization, floor loading, time advancement. No presentation
//!   state, so AI and spawning code never sees vfx/UI/audio.
//! * [`SimCtx`] - `ActorCtx` plus the presentation state that player-facing
//!   paths need (event vfx, the message log, targeting mode, sound).
//!
//! `SimCtx::actors` reborrows a `SimCtx` as an `ActorCtx`, so an outer
//! player-turn function can hand its context to the inner actor machinery.
//!
//! # Why `grid`, `clock` and `spatial` are `&mut`
//!
//! Most paths only read them, but a few genuinely write: `tick_fire` burns
//! grass tiles and `update_fov` writes visibility (grid), time advancement
//! calls `advance_to` (clock), and spawning registers entities (spatial
//! cache). Rather than carry a second near-identical context for those, the
//! fields are `&mut` and read-only uses reborrow immutably (`&*ctx.grid`).
//! That costs nothing and the borrow checker still rules out aliasing.

use crate::active_ai_tracker::ActiveAITracker;
use crate::audio::AudioManager;
use crate::events::EventQueue;
use crate::grid::Grid;
use crate::input::InputState;
use crate::spatial_cache::SpatialCache;
use crate::time_system::{ActionScheduler, GameClock};
use crate::ui::GameUiState;
use crate::vfx::VfxManager;

use hecs::{Entity, World};
use rand::rngs::StdRng;

/// Everything needed to make actors act: decide AI actions, complete actions,
/// advance the clock, and spawn/initialize new actors.
pub struct ActorCtx<'a> {
    pub world: &'a mut World,
    pub grid: &'a mut Grid,
    /// The player entity. Stable for the whole run, including across floors.
    pub player: Entity,
    pub clock: &'a mut GameClock,
    pub scheduler: &'a mut ActionScheduler,
    pub tracker: &'a mut ActiveAITracker,
    pub spatial: &'a mut SpatialCache,
    pub events: &'a mut EventQueue,
    pub rng: &'a mut StdRng,
}

impl ActorCtx<'_> {
    /// Shorten the borrow so a context can be handed to a nested call without
    /// moving it.
    pub fn reborrow(&mut self) -> ActorCtx<'_> {
        ActorCtx {
            world: self.world,
            grid: self.grid,
            player: self.player,
            clock: self.clock,
            scheduler: self.scheduler,
            tracker: self.tracker,
            spatial: self.spatial,
            events: self.events,
            rng: self.rng,
        }
    }
}

/// An [`ActorCtx`] plus the presentation state that player-facing simulation
/// paths touch: visual effects, the message log and other UI state, targeting
/// mode, and (optionally) sound.
pub struct SimCtx<'a> {
    pub world: &'a mut World,
    pub grid: &'a mut Grid,
    /// The player entity. Stable for the whole run, including across floors.
    pub player: Entity,
    pub clock: &'a mut GameClock,
    pub scheduler: &'a mut ActionScheduler,
    pub tracker: &'a mut ActiveAITracker,
    pub spatial: &'a mut SpatialCache,
    pub events: &'a mut EventQueue,
    pub rng: &'a mut StdRng,
    pub vfx: &'a mut VfxManager,
    pub ui: &'a mut GameUiState,
    /// Targeting mode and click-to-move path: abilities set the former, and a
    /// blocked or interrupted turn clears the latter.
    pub input: &'a mut InputState,
    /// `None` when audio failed to initialize.
    pub audio: Option<&'a AudioManager>,
}

impl SimCtx<'_> {
    /// Reborrow the actor-facing subset, dropping the presentation state.
    pub fn actors(&mut self) -> ActorCtx<'_> {
        ActorCtx {
            world: self.world,
            grid: self.grid,
            player: self.player,
            clock: self.clock,
            scheduler: self.scheduler,
            tracker: self.tracker,
            spatial: self.spatial,
            events: self.events,
            rng: self.rng,
        }
    }
}
