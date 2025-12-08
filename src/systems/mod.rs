//! Game systems organized by domain.
//!
//! This module contains all game logic systems, split into focused submodules:
//! - `animation`: Visual interpolation and animation updates
//! - `experience`: XP, leveling, and stats calculations
//! - `items`: Item properties and utilities
//! - `combat`: Damage, attacks, and death handling
//! - `inventory`: Container and inventory interactions
//! - `rendering`: FOV, visibility, and render data collection

pub mod animation;
pub mod combat;
pub mod experience;
pub mod inventory;
pub mod items;
pub mod rendering;

// Re-export commonly used items
pub use animation::{update_lunge_animations, visual_lerp};
pub use combat::{get_attack_damage, handle_chest_opened, open_chest, open_door, remove_dead_entities, weapon_damage};
pub use experience::{calculate_xp_value, grant_xp, stats_total, xp_for_level, xp_progress};
pub use inventory::{
    find_container_at_player, take_all_from_container, take_gold_from_container,
    take_item_from_container,
};
pub use items::{item_heal_amount, item_name, item_weight};
pub use rendering::{collect_renderables, effects, update_fov, RenderEntity};
