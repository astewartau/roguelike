//! UI rendering using egui.
//!
//! Handles all game UI: status bars, inventory, loot windows, etc.

pub mod style;

mod ability_bar;
mod dev_menu;
mod dialogue;
mod icons;
mod inventory;
mod loot_window;
mod start_screen;
mod status_bar;
mod targeting;
mod vfx;

// Re-export public items from submodules
pub use ability_bar::{draw_ability_bar, draw_secondary_ability_bar, AbilityBarData};
pub use dev_menu::{draw_dev_menu, DevMenu, DevTool};
pub use dialogue::{draw_dialogue_window, get_dialogue_window_data, DialogueWindowData};
pub use icons::UiIcons;
pub use inventory::{draw_inventory_window, InventoryWindowData};
pub use loot_window::{draw_loot_window, get_loot_window_data, LootWindowData};
pub use start_screen::run_start_screen;
pub use status_bar::{draw_status_bar, get_status_bar_data, StatusBarData};
pub use targeting::{draw_targeting_overlay, get_ability_targeting_overlay_data, get_targeting_overlay_data, TargetingOverlayData};
pub use vfx::{
    draw_alert_indicators, draw_damage_numbers, draw_enemy_health_bars,
    draw_enemy_status_indicators, draw_explosions, draw_player_buff_auras, draw_potion_splashes,
    get_buff_aura_data, get_enemy_health_data, get_enemy_status_data, EnemyHealthData,
    EnemyStatusData, PlayerBuffAuraData,
};

use crate::camera::Camera;
use crate::events::GameEvent;
use crate::grid::Grid;
use crate::input::{AbilityTargetingMode, TargetingMode};
use crate::multi_tileset::MultiTileset;
use crate::vfx::VisualEffect;
use egui_glow::EguiGlow;
use hecs::{Entity, World};
use winit::window::Window;

/// Actions the UI wants to perform (returned to game logic)
#[derive(Default)]
pub struct UiActions {
    pub item_to_use: Option<usize>,
    /// Throw a potion at a target (enters targeting mode)
    pub item_to_throw: Option<usize>,
    /// Drop an item from inventory onto the ground
    pub item_to_drop: Option<usize>,
    /// Drop the currently equipped weapon onto the ground
    pub drop_equipped_weapon: bool,
    /// Unequip the currently equipped weapon (put back in inventory)
    pub unequip_weapon: bool,
    pub chest_item_to_take: Option<usize>,
    pub chest_take_all: bool,
    pub chest_take_gold: bool,
    pub close_chest: bool,
    /// Index of dialogue option selected by player
    pub dialogue_option_selected: Option<usize>,
    /// Start the game with selected class (from start screen)
    pub start_game: Option<crate::components::PlayerClass>,
    /// Use class ability (Q)
    pub use_ability: bool,
    /// Use secondary ability (E) - Druid only
    pub use_secondary_ability: bool,
}

// =============================================================================
// GAME UI STATE (event-driven)
// =============================================================================

/// Game UI state that responds to events.
///
/// This centralizes UI state management and decouples it from game logic.
/// The UI reacts to game events rather than being set imperatively.
pub struct GameUiState {
    /// Currently open chest/container (for loot window)
    pub open_chest: Option<Entity>,
    /// Currently talking to NPC (for dialogue window)
    pub talking_to: Option<Entity>,
    /// Show inventory window
    pub show_inventory: bool,
    /// Show grid overlay
    pub show_grid_lines: bool,
    /// Context menu for inventory item (item index, screen position)
    pub item_context_menu: Option<(usize, egui::Pos2)>,
    /// Context menu for equipped weapon (screen position)
    pub equipped_context_menu: Option<egui::Pos2>,
    /// The player entity (needed to filter events)
    player_entity: Entity,
}

impl GameUiState {
    pub fn new(player_entity: Entity) -> Self {
        Self {
            open_chest: None,
            talking_to: None,
            show_inventory: false,
            show_grid_lines: false,
            item_context_menu: None,
            equipped_context_menu: None,
            player_entity,
        }
    }

    /// Handle a game event, updating UI state as needed
    pub fn handle_event(&mut self, event: &GameEvent) {
        match event {
            GameEvent::ContainerOpened { container, opener } => {
                // Only open loot window if player opened the container
                if *opener == self.player_entity {
                    self.open_chest = Some(*container);
                }
            }
            GameEvent::DialogueStarted { npc, player } => {
                // Open dialogue window if player started the conversation
                if *player == self.player_entity {
                    self.talking_to = Some(*npc);
                }
            }
            GameEvent::EntityMoved { entity, .. } => {
                // Close windows when player moves away
                if *entity == self.player_entity {
                    self.open_chest = None;
                    self.talking_to = None;
                }
            }
            _ => {}
        }
    }

    /// Close the dialogue window
    pub fn close_dialogue(&mut self) {
        self.talking_to = None;
    }

    /// Toggle inventory visibility
    pub fn toggle_inventory(&mut self) {
        self.show_inventory = !self.show_inventory;
    }

    /// Toggle grid lines visibility
    pub fn toggle_grid_lines(&mut self) {
        self.show_grid_lines = !self.show_grid_lines;
    }

    /// Close the currently open chest
    pub fn close_chest(&mut self) {
        self.open_chest = None;
    }

    /// Close the item context menu
    pub fn close_context_menu(&mut self) {
        self.item_context_menu = None;
        self.equipped_context_menu = None;
    }
}

// =============================================================================
// MAIN UI RUNNER
// =============================================================================

/// Run all UI rendering for a single frame.
///
/// This function orchestrates drawing all UI elements and collects
/// any actions the player triggered through the UI.
pub fn run_ui(
    egui_glow: &mut EguiGlow,
    window: &Window,
    world: &World,
    player_entity: Entity,
    grid: &Grid,
    ui_state: &mut GameUiState,
    dev_menu: &mut DevMenu,
    camera: &Camera,
    tileset: &MultiTileset,
    icons: &UiIcons,
    vfx_effects: &[VisualEffect],
    targeting_mode: Option<&TargetingMode>,
    ability_targeting_mode: Option<&AbilityTargetingMode>,
    mouse_pos: (f32, f32),
    game_time: f32,
) -> UiActions {
    let mut actions = UiActions::default();

    // Get status bar data
    let status_data = get_status_bar_data(world, player_entity);

    // Get ability bar data (if player has a class ability)
    let ability_data = world
        .get::<&crate::components::ClassAbility>(player_entity)
        .ok()
        .map(|ability| {
            // Check if player CAN have enough energy (max_energy >= cost)
            // The actual waiting for energy happens when the action is executed
            let can_afford = world
                .get::<&crate::components::Actor>(player_entity)
                .map(|actor| actor.max_energy >= ability.ability_type.energy_cost())
                .unwrap_or(false);
            AbilityBarData {
                ability_type: ability.ability_type,
                cooldown_remaining: ability.cooldown_remaining,
                cooldown_total: ability.cooldown_total,
                can_use: can_afford && ability.is_ready(),
                viewport_height: camera.viewport_height,
            }
        });

    // Get secondary ability bar data (Druid's Barkskin)
    let secondary_ability_data = world
        .get::<&crate::components::SecondaryAbility>(player_entity)
        .ok()
        .map(|ability| {
            let can_afford = world
                .get::<&crate::components::Actor>(player_entity)
                .map(|actor| actor.max_energy >= ability.ability_type.energy_cost())
                .unwrap_or(false);
            AbilityBarData {
                ability_type: ability.ability_type,
                cooldown_remaining: ability.cooldown_remaining,
                cooldown_total: ability.cooldown_total,
                can_use: can_afford && ability.is_ready(),
                viewport_height: camera.viewport_height,
            }
        });

    // Get loot window data if chest is open
    let loot_data = get_loot_window_data(
        world,
        ui_state.open_chest,
        camera.viewport_width,
        camera.viewport_height,
    );

    // Get dialogue window data if talking to an NPC
    let dialogue_data = get_dialogue_window_data(
        world,
        ui_state.talking_to,
        camera.viewport_width,
        camera.viewport_height,
    );

    let show_inventory = ui_state.show_inventory;
    let viewport_width = camera.viewport_width;
    let viewport_height = camera.viewport_height;

    // Extract UI data using helper functions
    let buff_aura_data = get_buff_aura_data(world, player_entity);
    // Try ability targeting first, then item targeting
    let targeting_data = get_ability_targeting_overlay_data(world, player_entity, ability_targeting_mode, mouse_pos, camera)
        .or_else(|| get_targeting_overlay_data(world, player_entity, targeting_mode, mouse_pos, camera));
    let enemy_status_data = get_enemy_status_data(world, grid);
    let enemy_health_data = get_enemy_health_data(world, grid, player_entity);

    egui_glow.run(window, |ctx| {
        // Enemy health bars (draw early so they're behind other indicators)
        draw_enemy_health_bars(ctx, camera, &enemy_health_data);

        // Player buff auras (draw first so they're behind everything)
        draw_player_buff_auras(ctx, camera, buff_aura_data.as_ref());

        // Targeting overlay (draw first so it's behind other UI)
        if let Some(ref data) = targeting_data {
            draw_targeting_overlay(ctx, camera, data);
        }

        // Status bar (always visible)
        draw_status_bar(ctx, &status_data, icons);

        // Ability bar (if player has a class ability)
        if let Some(ref data) = ability_data {
            if draw_ability_bar(ctx, data, icons) {
                actions.use_ability = true;
            }
        }

        // Secondary ability bar (Druid only - Barkskin)
        if let Some(ref data) = secondary_ability_data {
            if draw_secondary_ability_bar(ctx, data, icons) {
                actions.use_secondary_ability = true;
            }
        }

        // Floating damage numbers
        draw_damage_numbers(ctx, vfx_effects, camera);

        // Alert indicators (enemy spotted player)
        draw_alert_indicators(ctx, vfx_effects, camera);

        // Enemy status effect indicators (fear, slow, confusion)
        draw_enemy_status_indicators(ctx, camera, &enemy_status_data, game_time);

        // Explosion effects (fireball)
        draw_explosions(ctx, vfx_effects, camera);

        // Potion splash effects
        draw_potion_splashes(ctx, vfx_effects, camera);

        // Developer menu
        draw_dev_menu(ctx, dev_menu, icons, tileset);

        // Loot window (if chest is open)
        if let Some(ref data) = loot_data {
            draw_loot_window(ctx, data, icons, &mut actions);
        }

        // Dialogue window (if talking to NPC)
        if let Some(ref data) = dialogue_data {
            draw_dialogue_window(ctx, data, &mut actions);
        }

        // Inventory window (if toggled)
        if show_inventory {
            let inv_data = InventoryWindowData {
                viewport_width,
                viewport_height,
            };
            draw_inventory_window(ctx, world, player_entity, &inv_data, icons, ui_state, &mut actions);
        }
    });

    actions
}
