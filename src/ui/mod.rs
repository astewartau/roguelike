//! UI rendering using egui.
//!
//! Handles all game UI: status bars, inventory, loot windows, etc.

pub mod style;

mod altar_window;
mod dev_menu;
mod dialogue;
mod game_over_screen;
mod hotbar;
mod icons;
mod inventory;
mod loot_window;
mod message_log;
mod pause_screen;
mod shop_window;
mod start_screen;
mod status_bar;
mod targeting;
mod vfx;

// Re-export public items from submodules
pub use altar_window::{draw_altar_window, get_altar_window_data};
pub use dev_menu::{draw_dev_menu, DevMenu, DevTool};
pub use dialogue::{draw_dialogue_window, get_dialogue_window_data};
pub use game_over_screen::{run_game_over_screen, GameOverChoice, GameOverStats};
pub use hotbar::{
    ability_icon, ability_status, draw_drag_ghost, draw_hotbars, HotbarAnim, HotbarDrag,
    HotbarEntry,
};
pub use icons::UiIcons;
pub use inventory::{draw_inventory_window, InventoryWindowData};
pub use loot_window::{draw_loot_window, get_loot_window_data};
pub use message_log::{draw_message_log, MessageLog};
pub use pause_screen::{run_pause_screen, PauseChoice};
pub use shop_window::{draw_shop_window, get_shop_window_data};
pub use start_screen::run_start_screen;
pub use status_bar::{draw_status_bar, get_status_bar_data, StatusBarAnim};
pub use targeting::{draw_targeting_overlay, get_ability_targeting_overlay_data, get_targeting_overlay_data};
pub use vfx::{
    draw_alert_indicators, draw_attack_telegraphs, draw_damage_numbers, draw_enemy_health_bars, draw_loot_indicators,
    draw_enemy_status_indicators, draw_explosions, draw_life_drain_beams, draw_player_buff_auras,
    draw_resting_indicators,
    draw_potion_splashes, draw_taming_beams, get_buff_aura_data, get_enemy_health_data,
    get_attack_telegraph_data, get_enemy_status_data, get_life_drain_beam_data, get_loot_indicator_data, get_taming_beam_data, LifeDrainBeamData,
    TamingBeamData,
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
    /// Study a scroll (permanently learn its spell if INT allows)
    pub item_to_study: Option<usize>,
    /// Drop an item from inventory onto the ground
    pub item_to_drop: Option<usize>,
    /// Drop the currently equipped weapon onto the ground
    pub drop_equipped_weapon: bool,
    /// Unequip the currently equipped weapon (put back in inventory)
    pub unequip_weapon: bool,
    /// Unequip an armor slot (put the piece back in inventory)
    pub unequip_armor: Option<crate::systems::item_defs::ArmorSlot>,
    /// Take an item (the whole stack, for stackables) from one of the
    /// containers in the loot window: (container, index in its items)
    pub chest_item_to_take: Option<(Entity, usize)>,
    /// Take everything from every container in the loot window
    pub chest_take_all: bool,
    /// Take the gold from one of the containers in the loot window
    pub chest_take_gold: Option<Entity>,
    pub close_chest: bool,
    /// Index of dialogue option selected by player
    pub dialogue_option_selected: Option<usize>,
    /// Close the dialogue window without choosing an option (Esc)
    pub close_dialogue: bool,
    /// Start the game with selected class (from start screen)
    pub start_game: Option<crate::components::PlayerClass>,
    /// Activate an ability (from a hotbar slot)
    pub ability_to_use: Option<crate::components::AbilityType>,
    /// Buy item from vendor (index in vendor inventory)
    pub buy_item: Option<usize>,
    /// Sell item to vendor (index in player inventory)
    pub sell_item: Option<usize>,
    /// Close the shop window
    pub close_shop: bool,
    /// Restart the run with the same class (from the game over screen)
    pub retry_game: bool,
    /// Return to the class selection screen (from the game over / pause screen)
    pub return_to_menu: bool,
    /// Quit the application (from the pause menu)
    pub exit_game: bool,
    /// Select which ammo type the bow loads next (Arrow / FireArrow)
    pub set_active_ammo: Option<crate::components::ItemType>,
    /// Sacrifice the inventory item at this index on the open altar
    pub altar_sacrifice: Option<usize>,
    /// Close the altar window without sacrificing
    pub close_altar: bool,
}

// =============================================================================
// GAME UI STATE (event-driven)
// =============================================================================

/// Which tab is shown in the Character window.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CharacterTab {
    #[default]
    Inventory,
    Spellbook,
}

/// Game UI state that responds to events.
///
/// This centralizes UI state management and decouples it from game logic.
/// The UI reacts to game events rather than being set imperatively.
pub struct GameUiState {
    /// Currently open chest/container (for loot window)
    pub open_chest: Option<Entity>,
    /// Tile the open loot window is looting. Every walkable container on it
    /// is shown alongside `open_chest` (see `systems::loot_sources`).
    pub loot_tile: (i32, i32),
    /// Currently talking to NPC (for dialogue window)
    pub talking_to: Option<Entity>,
    /// Highlighted dialogue response option (for keyboard navigation)
    pub dialogue_selected: usize,
    /// Currently shopping at vendor (for shop window)
    pub shopping_at: Option<Entity>,
    /// Currently open altar (for the sacrifice window)
    pub open_altar: Option<Entity>,
    /// Show inventory window
    pub show_inventory: bool,
    /// Show grid overlay
    pub show_grid_lines: bool,
    /// Context menu for inventory item (item index, screen position)
    pub item_context_menu: Option<(usize, egui::Pos2)>,
    /// Context menu for equipped weapon (screen position)
    pub equipped_context_menu: Option<egui::Pos2>,
    /// Which tab the Character window shows
    pub character_tab: CharacterTab,
    /// Main hotbar (keys 1-5)
    pub hotbar_main: [Option<HotbarEntry>; 5],
    /// Shift hotbar (keys Shift+1-5), auto-filled with abilities at run start
    pub hotbar_shift: [Option<HotbarEntry>; 5],
    /// Q/E/R hotbar (keys Q, E and R). The R slot is pre-filled with Rest.
    pub hotbar_qer: [Option<HotbarEntry>; 3],
    /// Frame-to-frame animation state for the hotbar slots (cooldown sweeps,
    /// ready and denied flashes). Presentation only; see [`HotbarAnim`].
    hotbar_anim: HotbarAnim,
    /// Scrolling combat/message log
    pub message_log: MessageLog,
    /// Frame-to-frame animation state for the status bar (the HP chip bar and
    /// the low-HP pulse). Presentation only; see [`StatusBarAnim`].
    status_anim: StatusBarAnim,
    /// The player entity (needed to filter events)
    player_entity: Entity,
}

impl GameUiState {
    pub fn new(player_entity: Entity) -> Self {
        Self {
            open_chest: None,
            loot_tile: (0, 0),
            talking_to: None,
            dialogue_selected: 0,
            shopping_at: None,
            open_altar: None,
            show_inventory: false,
            show_grid_lines: false,
            item_context_menu: None,
            equipped_context_menu: None,
            character_tab: CharacterTab::default(),
            hotbar_main: [None; 5],
            hotbar_shift: [None; 5],
            hotbar_qer: [None; 3],
            hotbar_anim: HotbarAnim::new(),
            message_log: MessageLog::new(player_entity),
            status_anim: StatusBarAnim::new(),
            player_entity,
        }
    }

    /// Handle a game event, updating UI state as needed
    pub fn handle_event(&mut self, event: &GameEvent) {
        match event {
            // Only open loot window if player opened the container
            GameEvent::ContainerOpened { container, opener, position, .. }
                if *opener == self.player_entity =>
            {
                self.open_chest = Some(*container);
                self.loot_tile = *position;
            }
            // Open dialogue window if player started the conversation
            GameEvent::DialogueStarted { npc, player } if *player == self.player_entity => {
                self.talking_to = Some(*npc);
                self.dialogue_selected = 0;
            }
            // Open shop window if player started shopping
            GameEvent::ShopOpened { vendor, player } if *player == self.player_entity => {
                self.shopping_at = Some(*vendor);
                self.talking_to = None; // Close dialogue when shop opens
            }
            // Open the sacrifice window if the player used the altar
            GameEvent::AltarOpened { altar, player } if *player == self.player_entity => {
                self.open_altar = Some(*altar);
            }
            // Close windows when player moves away
            GameEvent::EntityMoved { entity, .. } if *entity == self.player_entity => {
                self.open_chest = None;
                self.talking_to = None;
                self.shopping_at = None;
                self.open_altar = None;
            }
            _ => {}
        }
    }

    /// Close the dialogue window
    pub fn close_dialogue(&mut self) {
        self.talking_to = None;
    }

    /// Close any open UI window/popup. Returns true if something was closed
    /// (used by Escape to back out one layer at a time before pausing).
    pub fn close_open_menus(&mut self) -> bool {
        let mut closed = false;
        // Context menus first (they sit on top of the inventory).
        if self.item_context_menu.is_some() {
            self.item_context_menu = None;
            closed = true;
        }
        if self.equipped_context_menu.is_some() {
            self.equipped_context_menu = None;
            closed = true;
        }
        if closed {
            return true;
        }
        if self.show_inventory {
            self.show_inventory = false;
            closed = true;
        }
        if self.open_chest.is_some() {
            self.open_chest = None;
            closed = true;
        }
        if self.shopping_at.is_some() {
            self.shopping_at = None;
            closed = true;
        }
        if self.open_altar.is_some() {
            self.open_altar = None;
            closed = true;
        }
        if self.talking_to.is_some() {
            self.talking_to = None;
            closed = true;
        }
        closed
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

    /// Close the shop window
    pub fn close_shop(&mut self) {
        self.shopping_at = None;
    }

    /// Close the altar sacrifice window
    pub fn close_altar(&mut self) {
        self.open_altar = None;
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
/// The read-only game state the UI reads to draw itself.
pub struct UiWorld<'a> {
    pub world: &'a World,
    pub player_entity: Entity,
    pub grid: &'a Grid,
}

/// Shared drawing resources, owned by the application shell.
pub struct UiResources<'a> {
    pub camera: &'a Camera,
    pub tileset: &'a MultiTileset,
    pub icons: &'a UiIcons,
}

/// What this frame has to draw over the world, and what the player is aiming at.
pub struct UiOverlays<'a> {
    pub vfx_effects: &'a [VisualEffect],
    pub resting_bubble: Option<&'a crate::vfx::RestingBubble>,
    pub life_drain_beams: &'a [LifeDrainBeamData],
    pub taming_beams: &'a [TamingBeamData],
    pub targeting_mode: Option<&'a TargetingMode>,
    pub ability_targeting_mode: Option<&'a AbilityTargetingMode>,
}

/// Everything about the frame being drawn, as opposed to the egui host and the
/// mutable UI state that outlives it.
pub struct UiFrame<'a> {
    pub game: UiWorld<'a>,
    pub resources: UiResources<'a>,
    pub overlays: UiOverlays<'a>,
    pub mouse_pos: (f32, f32),
    pub game_time: f32,
}

pub fn run_ui(
    egui_glow: &mut EguiGlow,
    window: &Window,
    ui_state: &mut GameUiState,
    dev_menu: &mut DevMenu,
    frame: UiFrame<'_>,
) -> UiActions {
    let UiFrame { game, resources, overlays, mouse_pos, game_time } = frame;
    let UiWorld { world, player_entity, grid } = game;
    let UiResources { camera, tileset, icons } = resources;
    let UiOverlays {
        vfx_effects,
        resting_bubble,
        life_drain_beams,
        taming_beams,
        targeting_mode,
        ability_targeting_mode,
    } = overlays;

    let mut actions = UiActions::default();

    // Get status bar data
    let status_data = get_status_bar_data(world, player_entity, grid);

    // Get loot window data if chest is open
    let loot_data = get_loot_window_data(
        world,
        ui_state.open_chest,
        ui_state.loot_tile,
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

    // Get shop window data if shopping at a vendor
    let shop_data = get_shop_window_data(
        world,
        ui_state.shopping_at,
        player_entity,
        camera.viewport_width,
        camera.viewport_height,
    );

    // Get altar window data if an altar is open
    let altar_data = get_altar_window_data(
        world,
        ui_state.open_altar,
        player_entity,
        camera.viewport_width,
        camera.viewport_height,
    );

    let show_inventory = ui_state.show_inventory;
    let viewport_width = camera.viewport_width;
    let viewport_height = camera.viewport_height;

    // Extract UI data using helper functions
    let buff_aura_data = get_buff_aura_data(world, player_entity);
    // Try ability targeting first, then item targeting
    let targeting_data = get_ability_targeting_overlay_data(world, player_entity, ability_targeting_mode, mouse_pos, camera, Some(grid))
        .or_else(|| get_targeting_overlay_data(world, player_entity, targeting_mode, mouse_pos, camera));
    let enemy_status_data = get_enemy_status_data(world, grid);
    let enemy_health_data = get_enemy_health_data(world, grid, player_entity);
    let loot_indicator_data = get_loot_indicator_data(world, grid, player_entity);
    let attack_telegraph_data = get_attack_telegraph_data(world, grid, game_time);
    // The countdown is for deciding; while the player's own action is
    // resolving (auto-path, rest) it would only flicker.
    let player_idle = world
        .get::<&crate::components::Actor>(player_entity)
        .map(|a| a.can_act())
        .unwrap_or(false);

    egui_glow.run(window, |ctx| {
        // Enemy health bars (draw early so they're behind other indicators)
        draw_enemy_health_bars(ctx, camera, &enemy_health_data);

        // Markers over corpses and item piles with something left in them
        draw_loot_indicators(ctx, camera, &loot_indicator_data);

        // Red markers on tiles hostiles are about to hit
        draw_attack_telegraphs(ctx, camera, &attack_telegraph_data, player_idle);

        // Player buff auras (draw first so they're behind everything)
        draw_player_buff_auras(ctx, camera, buff_aura_data.as_ref());

        // Targeting overlay (draw first so it's behind other UI)
        if let Some(ref data) = targeting_data {
            draw_targeting_overlay(ctx, camera, data);
        }

        // Status bar (always visible)
        draw_status_bar(ctx, &status_data, &mut ui_state.status_anim, icons, game_time);

        // Scrolling combat/message log (bottom-left)
        draw_message_log(ctx, &ui_state.message_log);

        // Quick-use hotbars (items + abilities, bottom-center)
        draw_hotbars(
            ctx,
            world,
            player_entity,
            icons,
            &mut ui_state.hotbar_main,
            &mut ui_state.hotbar_shift,
            &mut ui_state.hotbar_qer,
            &mut ui_state.hotbar_anim,
            &mut actions,
        );

        // Floating damage numbers
        draw_damage_numbers(ctx, vfx_effects, camera);

        // Alert indicators (enemy spotted player)
        draw_alert_indicators(ctx, vfx_effects, camera);

        // Resting indicator ("Zzz" bubble)
        draw_resting_indicators(ctx, resting_bubble, camera);

        // Enemy status effect indicators (fear, slow, confusion)
        draw_enemy_status_indicators(ctx, camera, &enemy_status_data);

        // Explosion effects (fireball)
        draw_explosions(ctx, vfx_effects, camera);

        // Potion splash effects
        draw_potion_splashes(ctx, vfx_effects, camera);

        // Life drain beams
        draw_life_drain_beams(ctx, camera, life_drain_beams);

        // Taming channels
        draw_taming_beams(ctx, camera, taming_beams);

        // Developer menu
        draw_dev_menu(ctx, dev_menu, icons, tileset);

        // Loot window (if chest is open)
        if let Some(ref data) = loot_data {
            draw_loot_window(ctx, data, icons, &mut actions);
        }

        // Dialogue window (if talking to NPC)
        if let Some(ref data) = dialogue_data {
            draw_dialogue_window(ctx, data, icons, tileset, &mut ui_state.dialogue_selected, &mut actions);
        }

        // Shop window (if shopping at vendor)
        if let Some(ref data) = shop_data {
            draw_shop_window(ctx, data, icons, &mut actions);
        }

        // Altar sacrifice window (if an altar is open)
        if let Some(ref data) = altar_data {
            draw_altar_window(ctx, data, icons, &mut actions);
        }

        // Inventory window (if toggled)
        if show_inventory {
            let inv_data = InventoryWindowData {
                viewport_width,
                viewport_height,
            };
            draw_inventory_window(ctx, world, player_entity, &inv_data, icons, ui_state, &mut actions);
        }

        // Drag preview icon under the cursor (drawn last so it's on top)
        draw_drag_ghost(ctx, icons);
    });

    actions
}
