//! Game systems organized by domain.
//!
//! This module contains all game logic systems, split into focused submodules:
//! - `actions`: Action effect implementations (move, attack, etc.)
//! - `ai`: AI decision-making and behavior
//! - `animation`: Visual interpolation and animation updates
//! - `effects`: Status effect application
//! - `experience`: XP, leveling, and stats calculations
//! - `items`: Item properties and utilities
//! - `combat`: Damage, attacks, and death handling
//! - `inventory`: Container and inventory interactions
//! - `rendering`: FOV, visibility, and render data collection
//! - `projectile`: Arrow and projectile movement

pub mod actions;
pub mod ai;
pub mod animation;
pub mod combat;
pub mod dev_tools;
pub mod effects;
pub mod experience;
pub mod inventory;
pub mod items;
pub mod player_input;
pub mod projectile;
pub mod rendering;

// Re-export commonly used items
pub use animation::{update_lunge_animations, visual_lerp};
pub use combat::{handle_container_opened, remove_dead_entities, weapon_damage};
pub use experience::xp_progress;
pub use inventory::{
    find_container_at_player, take_all_from_container, take_gold_from_container,
    take_item_from_container,
};
pub use items::{item_name, item_weight, item_tile_id, use_item, remove_item_from_inventory, ItemUseResult};
pub use projectile::{cleanup_finished_projectiles, despawn_projectiles, lerp_projectiles_realtime, update_projectiles};
pub use rendering::{collect_renderables, update_fov, reveal_entire_map, reveal_enemies, RenderEntity};
