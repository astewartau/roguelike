//! Game systems organized by domain.
//!
//! This module contains all game logic systems, split into focused submodules:
//! - `action_dispatch`: Action type determination and duration calculation
//! - `actions`: Action effect implementations (move, attack, etc.)
//! - `ai`: AI decision-making and behavior
//! - `animation`: Visual interpolation and animation updates
//! - `camera_shake`: Per-event camera shake amplitudes
//! - `effects`: Status effect application
//! - `experience`: XP, leveling, and stats calculations
//! - `items`: Item properties and utilities
//! - `combat`: Damage, attacks, and death handling
//! - `inventory`: Container and inventory interactions
//! - `rendering`: FOV, visibility, and render data collection
//! - `projectile`: Arrow and projectile movement
//! - `charge`: Orc charges (wind-up lane, dash, impact / wall stun / stumble)
//! - `grab`: Zombie grabs (hold a victim in place; early release)
//! - `hitstop`: Brief real-time freeze of visual animation on heavy blows
//! - `pack`: Rat packs (alone-means-flee, pack morale, shared alerts)
//! - `split`: Slimes splitting in two when badly hurt
//! - `telegraph`: Which tiles in-progress hostile attacks threaten (for UI/animation)
//! - `tile_effects`: What a tile does to whoever enters or stands on it

pub mod action_dispatch;
pub mod actions;
pub mod ai;
pub mod animation;
pub mod camera_shake;
pub mod charge;
pub mod combat;
pub mod dev_tools;
pub mod dialogue;
pub mod discovery;
pub mod effects;
pub mod experience;
pub mod fire;
pub mod furniture;
pub mod hitstop;
pub mod grab;
pub mod identify;
pub mod inventory;
pub mod item_defs;
pub mod items;
pub mod pack;
pub mod player_input;
pub mod projectile;
pub mod rendering;
pub mod split;
pub mod survival;
pub mod telegraph;
pub mod tile_effects;
pub mod webs;

// Re-export commonly used items
pub use animation::{
    flash_on_damage, update_hit_flashes, update_lunge_animations, visual_lerp,
};
pub use combat::{handle_container_opened, handle_door_closed, handle_door_opened, remove_dead_entities, weapon_damage};
pub use experience::xp_progress;
pub use inventory::{
    cleanup_empty_ground_piles, find_container_at_player, loot_source_label, loot_sources,
    stack_items, take_all_from_sources, take_gold_from_container, take_item_from_container,
    unblock_emptied_containers,
};
pub use items::{item_name, use_item, remove_item_from_inventory, item_targeting_params, ItemUseResult};
pub use projectile::{cleanup_finished_projectiles, despawn_projectiles, lerp_projectiles_realtime, update_projectiles};
pub use rendering::{calculate_illumination, collect_renderables, update_fov, reveal_entire_map, reveal_enemies, RenderEntity};
