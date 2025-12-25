//! Game constants organized by domain.
//!
//! Centralizing magic numbers makes tuning easier and documents intent.
//! Constants are split into submodules by domain for easier navigation.

mod animation;
mod camera;
mod combat;
mod dungeon;
mod effects;
mod enemies;
mod gameplay;
mod items;
mod time;
mod ui;

// Re-export all constants at the module level for backward compatibility
pub use animation::*;
pub use camera::*;
pub use combat::*;
pub use dungeon::*;
pub use effects::*;
pub use enemies::*;
pub use gameplay::*;
pub use items::*;
pub use time::*;
pub use ui::*;
