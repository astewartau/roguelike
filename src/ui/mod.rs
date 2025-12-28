//! UI rendering using egui.
//!
//! Handles all game UI: status bars, inventory, loot windows, etc.

pub mod style;

use crate::camera::Camera;
use crate::components::{Container, Dialogue, EffectType as StatusEffectType, Equipment, Health, Inventory, ItemType, PlayerClass, Stats, StatusEffects};
use crate::constants::DAMAGE_NUMBER_RISE;
use crate::grid::Grid;
use crate::input::TargetingMode;
use crate::multi_tileset::MultiTileset;
use crate::systems;
use crate::tile::tile_ids;
use crate::vfx::{VfxType, VisualEffect};
use egui_glow::EguiGlow;
use hecs::{Entity, World};
use winit::window::Window;

/// Data needed to render the status bar
pub struct StatusBarData {
    pub health_current: i32,
    pub health_max: i32,
    pub xp_progress: f32,
    pub xp_level: u32,
    pub gold: u32,
    /// Active status effects with remaining duration
    pub active_effects: Vec<(StatusEffectType, f32)>,
}

/// Data needed to render the loot window
pub struct LootWindowData {
    pub items: Vec<ItemType>,
    pub gold: u32,
    pub viewport_width: f32,
    pub viewport_height: f32,
}

/// Data needed to render the inventory window
pub struct InventoryWindowData {
    pub viewport_width: f32,
    pub viewport_height: f32,
}

/// Data needed to render the dialogue window
pub struct DialogueWindowData {
    pub npc_name: String,
    pub text: String,
    pub options: Vec<String>,
    pub viewport_width: f32,
    pub viewport_height: f32,
}

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
    /// Use class ability
    pub use_ability: bool,
}

// =============================================================================
// GAME UI STATE (event-driven)
// =============================================================================

use crate::events::GameEvent;

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
// DEVELOPER MENU
// =============================================================================

use crate::systems::items::{item_name, item_sprite};
use crate::tile::SpriteSheet;

/// Placement tools - click on map to spawn
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum DevTool {
    SpawnChest,
    SpawnEnemy,
    SpawnFire,
    SpawnStairsDown,
    SpawnStairsUp,
}

impl DevTool {
    pub fn name(&self) -> &'static str {
        match self {
            DevTool::SpawnChest => "Chest",
            DevTool::SpawnEnemy => "Enemy",
            DevTool::SpawnFire => "Fire",
            DevTool::SpawnStairsDown => "Stairs Down",
            DevTool::SpawnStairsUp => "Stairs Up",
        }
    }

    pub fn sprite(&self) -> (SpriteSheet, u32) {
        match self {
            DevTool::SpawnChest => tile_ids::CHEST_CLOSED,
            DevTool::SpawnEnemy => tile_ids::SKELETON,
            DevTool::SpawnFire => tile_ids::RED_POTION,
            DevTool::SpawnStairsDown => tile_ids::STAIRS_DOWN,
            DevTool::SpawnStairsUp => tile_ids::STAIRS_UP,
        }
    }

    pub const ALL: [DevTool; 5] = [
        DevTool::SpawnChest,
        DevTool::SpawnEnemy,
        DevTool::SpawnFire,
        DevTool::SpawnStairsDown,
        DevTool::SpawnStairsUp,
    ];
}

/// All item types for the dev menu
const ALL_ITEMS: [ItemType; 13] = [
    // Potions
    ItemType::HealthPotion,
    ItemType::RegenerationPotion,
    ItemType::StrengthPotion,
    ItemType::ConfusionPotion,
    // Scrolls
    ItemType::ScrollOfInvisibility,
    ItemType::ScrollOfSpeed,
    ItemType::ScrollOfProtection,
    ItemType::ScrollOfBlink,
    ItemType::ScrollOfFear,
    ItemType::ScrollOfFireball,
    ItemType::ScrollOfReveal,
    ItemType::ScrollOfMapping,
    ItemType::ScrollOfSlow,
];

/// State for the developer menu
pub struct DevMenu {
    pub visible: bool,
    pub selected_tool: Option<DevTool>,
    /// Item to add to player inventory (set when an item is clicked)
    pub item_to_give: Option<ItemType>,
}

impl Default for DevMenu {
    fn default() -> Self {
        Self {
            visible: false,
            selected_tool: None,
            item_to_give: None,
        }
    }
}

impl DevMenu {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn toggle(&mut self) {
        self.visible = !self.visible;
        if !self.visible {
            self.selected_tool = None;
        }
    }

    pub fn has_active_tool(&self) -> bool {
        self.visible && self.selected_tool.is_some()
    }

    /// Take the pending item to give (clears it after reading)
    pub fn take_item_to_give(&mut self) -> Option<ItemType> {
        self.item_to_give.take()
    }
}

/// Helper to draw a tile button
#[allow(dead_code)] // Reserved for dev tool palette expansion
fn draw_tile_button(
    ui: &mut egui::Ui,
    tileset_texture_id: egui::TextureId,
    uv_rect: egui::Rect,
    is_selected: bool,
    tooltip: &str,
) -> egui::Response {
    let button_size = egui::vec2(36.0, 36.0);
    let (rect, response) = ui.allocate_exact_size(button_size, egui::Sense::click());

    let bg_color = if is_selected {
        style::colors::SELECTED
    } else if response.hovered() {
        style::colors::HOVERED
    } else {
        style::colors::BUTTON_BG
    };
    ui.painter().rect_filled(rect, 0.0, bg_color);

    let image_rect = rect.shrink(3.0);
    ui.painter().image(
        tileset_texture_id,
        image_rect,
        uv_rect,
        egui::Color32::WHITE,
    );

    response.on_hover_text(tooltip)
}

/// Draw a list item with icon and label (for dev menu)
fn draw_list_item(
    ui: &mut egui::Ui,
    tileset_texture_id: egui::TextureId,
    uv_rect: egui::Rect,
    is_selected: bool,
    label: &str,
) -> egui::Response {
    let icon_size = 24.0;
    let total_height = 28.0;
    let available_width = ui.available_width();

    let (rect, response) = ui.allocate_exact_size(
        egui::vec2(available_width, total_height),
        egui::Sense::click(),
    );

    let bg_color = if is_selected {
        style::colors::SELECTED
    } else if response.hovered() {
        style::colors::HOVERED
    } else {
        egui::Color32::TRANSPARENT
    };

    ui.painter().rect_filled(rect, 0.0, bg_color);

    // Draw icon
    let icon_rect = egui::Rect::from_min_size(
        rect.min + egui::vec2(4.0, (total_height - icon_size) / 2.0),
        egui::vec2(icon_size, icon_size),
    );
    ui.painter().image(
        tileset_texture_id,
        icon_rect,
        uv_rect,
        egui::Color32::WHITE,
    );

    // Draw label
    let text_pos = egui::pos2(
        rect.min.x + icon_size + 12.0,
        rect.center().y,
    );
    ui.painter().text(
        text_pos,
        egui::Align2::LEFT_CENTER,
        label,
        egui::FontId::monospace(14.0),
        style::colors::TEXT_PRIMARY,
    );

    response
}

/// Draw the developer menu
pub fn draw_dev_menu(
    ctx: &egui::Context,
    dev_menu: &mut DevMenu,
    icons: &UiIcons,
    tileset: &MultiTileset,
) {
    if !dev_menu.visible {
        return;
    }

    egui::Window::new("Developer Tools")
        .fixed_pos([10.0, 120.0])
        .min_width(200.0)
        .max_height(500.0)
        .title_bar(true)
        .collapsible(true)
        .scroll([false, true])
        .frame(style::dungeon_window_frame())
        .show(ctx, |ui| {
            // === PLACEMENT TOOLS ===
            ui.heading("Placement (click map)");
            for tool in DevTool::ALL {
                let is_selected = dev_menu.selected_tool == Some(tool);
                let sprite = tool.sprite();
                let texture_id = icons.texture_for_sheet(sprite.0);
                let uv_rect = tileset.get_egui_uv(sprite.0, sprite.1);

                let response = draw_list_item(
                    ui,
                    texture_id,
                    uv_rect,
                    is_selected,
                    tool.name(),
                );

                if response.clicked() {
                    if is_selected {
                        dev_menu.selected_tool = None;
                    } else {
                        dev_menu.selected_tool = Some(tool);
                    }
                }
            }

            if let Some(tool) = dev_menu.selected_tool {
                ui.add_space(4.0);
                ui.label(format!("→ Click map to place {}", tool.name()));
            }

            ui.add_space(8.0);
            ui.separator();

            // === ITEMS (click to add to inventory) ===
            ui.heading("Items (click to add)");

            // Potions
            ui.label("Potions:");
            for item in ALL_ITEMS.iter().take(4) {
                let sprite = item_sprite(*item);
                let texture_id = icons.texture_for_sheet(sprite.0);
                let uv_rect = tileset.get_egui_uv(sprite.0, sprite.1);
                let response = draw_list_item(
                    ui,
                    texture_id,
                    uv_rect,
                    false,
                    item_name(*item),
                );

                if response.clicked() {
                    dev_menu.item_to_give = Some(*item);
                }
            }

            ui.add_space(4.0);

            // Scrolls
            ui.label("Scrolls:");
            for item in ALL_ITEMS.iter().skip(4) {
                let sprite = item_sprite(*item);
                let texture_id = icons.texture_for_sheet(sprite.0);
                let uv_rect = tileset.get_egui_uv(sprite.0, sprite.1);
                let response = draw_list_item(
                    ui,
                    texture_id,
                    uv_rect,
                    false,
                    item_name(*item),
                );

                if response.clicked() {
                    dev_menu.item_to_give = Some(*item);
                }
            }
        });
}

/// Render the status bar (health, XP, gold, status effects)
pub fn draw_status_bar(
    ctx: &egui::Context,
    data: &StatusBarData,
    icons: &UiIcons,
) {
    // Calculate window height based on number of status effects
    let base_height = 90.0;
    let effects_height = if data.active_effects.is_empty() { 0.0 } else { 25.0 };
    let window_height = base_height + effects_height;

    egui::Window::new("Status")
        .fixed_pos([10.0, 10.0])
        .fixed_size([220.0, window_height])
        .title_bar(false)
        .frame(style::dungeon_window_frame())
        .show(ctx, |ui| {
            let health_percent = if data.health_max > 0 {
                data.health_current as f32 / data.health_max as f32
            } else {
                0.0
            };

            // HP bar with heart icon
            ui.horizontal(|ui| {
                let heart_img = egui::Image::new(egui::load::SizedTexture::new(
                    icons.items_texture_id,
                    egui::vec2(16.0, 16.0),
                ))
                .uv(icons.heart_uv);
                ui.add(heart_img);
                ui.add_sized(
                    [180.0, 18.0],
                    egui::ProgressBar::new(health_percent)
                        .fill(style::colors::HP_BAR)
                        .text(format!("{}/{}", data.health_current, data.health_max)),
                );
            });

            // XP bar with diamond icon
            ui.horizontal(|ui| {
                let diamond_img = egui::Image::new(egui::load::SizedTexture::new(
                    icons.items_texture_id,
                    egui::vec2(16.0, 16.0),
                ))
                .uv(icons.diamond_uv);
                ui.add(diamond_img);
                ui.add_sized(
                    [180.0, 18.0],
                    egui::ProgressBar::new(data.xp_progress)
                        .fill(style::colors::XP_BAR)
                        .text(format!(
                            "Lv {} - {:.0}%",
                            data.xp_level,
                            data.xp_progress * 100.0
                        )),
                );
            });

            // Gold with coins icon
            ui.horizontal(|ui| {
                let coin_img = egui::Image::new(egui::load::SizedTexture::new(
                    icons.items_texture_id,
                    egui::vec2(16.0, 16.0),
                ))
                .uv(icons.coins_uv);
                ui.add(coin_img);
                ui.label(format!("{}", data.gold));
            });

            // Active status effects
            if !data.active_effects.is_empty() {
                ui.separator();
                ui.horizontal(|ui| {
                    for (effect_type, duration) in &data.active_effects {
                        let (label, color) = match effect_type {
                            StatusEffectType::Invisible => ("Invisible", egui::Color32::from_rgb(180, 180, 255)),
                            StatusEffectType::SpeedBoost => ("Speed", egui::Color32::from_rgb(255, 220, 100)),
                            StatusEffectType::Regenerating => ("Regen", egui::Color32::from_rgb(100, 255, 100)),
                            StatusEffectType::Strengthened => ("Strength", egui::Color32::from_rgb(255, 150, 50)),
                            StatusEffectType::Protected => ("Protected", egui::Color32::from_rgb(150, 150, 255)),
                            StatusEffectType::Confused => ("Confused", egui::Color32::from_rgb(200, 100, 200)),
                            StatusEffectType::Feared => ("Feared", egui::Color32::from_rgb(255, 100, 100)),
                            StatusEffectType::Slowed => ("Slowed", egui::Color32::from_rgb(100, 150, 200)),
                        };
                        ui.label(
                            egui::RichText::new(format!("{} ({:.0}s)", label, duration))
                                .color(color)
                                .small(),
                        );
                    }
                });
            }
        });
}

// =============================================================================
// ABILITY BAR
// =============================================================================

use crate::components::{AbilityType, ClassAbility};

/// Data needed to render the ability bar
pub struct AbilityBarData {
    pub ability_type: AbilityType,
    pub cooldown_remaining: f32,
    pub cooldown_total: f32,
    pub can_use: bool, // has energy and off cooldown
    pub viewport_height: f32,
}

/// Render the ability bar at the bottom of the screen.
/// Returns true if the ability button was clicked.
pub fn draw_ability_bar(
    ctx: &egui::Context,
    data: &AbilityBarData,
    icons: &UiIcons,
) -> bool {
    let mut clicked = false;

    egui::Window::new("Ability")
        .fixed_pos([10.0, data.viewport_height - 90.0])
        .fixed_size([80.0, 80.0])
        .title_bar(false)
        .frame(style::dungeon_window_frame())
        .show(ctx, |ui| {
            ui.vertical_centered(|ui| {
                // Get the appropriate icon UV based on ability type
                let (uv, tooltip) = match data.ability_type {
                    AbilityType::Cleave => (icons.cleave_uv, "Cleave\nAttack all adjacent enemies\n\n[Q]"),
                    AbilityType::Sprint => (icons.sprint_uv, "Sprint\nDouble movement speed for 10s\n\n[Q]"),
                };

                // Create the button size
                let button_size = egui::vec2(48.0, 48.0);

                // Allocate space for the button
                let (rect, response) = ui.allocate_exact_size(button_size, egui::Sense::click());

                // Draw background
                let bg_color = if !data.can_use {
                    egui::Color32::from_rgb(30, 25, 25) // Darker when unavailable
                } else if response.hovered() {
                    style::colors::BUTTON_HOVER
                } else {
                    style::colors::BUTTON_BG
                };
                ui.painter().rect_filled(rect, 0.0, bg_color);

                // Draw border
                let border_color = if data.can_use && data.cooldown_remaining <= 0.0 {
                    style::colors::DUNGEON_GOLD // Gold border when ready
                } else {
                    style::colors::BUTTON_BORDER
                };
                ui.painter().rect_stroke(rect, 0.0, egui::Stroke::new(2.0, border_color));

                // Draw the icon
                let image = egui::Image::new(egui::load::SizedTexture::new(
                    icons.items_texture_id,
                    button_size,
                ))
                .uv(uv);
                image.paint_at(ui, rect);

                // Draw cooldown overlay if on cooldown
                if data.cooldown_remaining > 0.0 {
                    // Dark overlay
                    let overlay_color = egui::Color32::from_rgba_unmultiplied(0, 0, 0, 180);
                    ui.painter().rect_filled(rect, 0.0, overlay_color);

                    // Cooldown text
                    let cd_text = format!("{:.0}s", data.cooldown_remaining);
                    ui.painter().text(
                        rect.center(),
                        egui::Align2::CENTER_CENTER,
                        cd_text,
                        egui::FontId::proportional(14.0),
                        egui::Color32::WHITE,
                    );
                }

                // Handle click
                if response.clicked() && data.can_use {
                    clicked = true;
                }

                // Tooltip
                response.on_hover_text(tooltip);

                // Hotkey hint
                ui.label(
                    egui::RichText::new("[Q]")
                        .color(style::colors::TEXT_MUTED)
                        .small(),
                );
            });
        });

    clicked
}

/// Render the loot window (chest/bones contents)
pub fn draw_loot_window(
    ctx: &egui::Context,
    data: &LootWindowData,
    icons: &UiIcons,
    actions: &mut UiActions,
) {
    egui::Window::new("Loot")
        .default_pos([
            data.viewport_width / 2.0 - 150.0,
            data.viewport_height / 2.0 - 100.0,
        ])
        .default_size([300.0, 200.0])
        .collapsible(false)
        .resizable(false)
        .frame(style::dungeon_window_frame())
        .show(ctx, |ui| {
            ui.heading("Contents");
            ui.separator();
            ui.add_space(10.0);

            let has_contents = !data.items.is_empty() || data.gold > 0;

            if !has_contents {
                ui.label(
                    egui::RichText::new("(empty)")
                        .italics()
                        .color(style::colors::TEXT_MUTED),
                );
            } else {
                // Show gold if present
                if data.gold > 0 {
                    ui.horizontal(|ui| {
                        let coin_img = egui::Image::new(egui::load::SizedTexture::new(
                            icons.items_texture_id,
                            egui::vec2(32.0, 32.0),
                        ))
                        .uv(icons.coins_uv)
                        .bg_fill(style::colors::PANEL_BG);

                        if ui
                            .add(egui::ImageButton::new(coin_img))
                            .on_hover_text(format!("{} Gold\n\nClick to take", data.gold))
                            .clicked()
                        {
                            actions.chest_take_gold = true;
                        }
                        ui.label(format!("{} gold", data.gold));
                    });
                    ui.add_space(5.0);
                }

                // Show items
                ui.horizontal_wrapped(|ui| {
                    for (i, item_type) in data.items.iter().enumerate() {
                        let uv = icons.get_item_uv(*item_type);

                        let image = egui::Image::new(egui::load::SizedTexture::new(
                            icons.items_texture_id,
                            egui::vec2(48.0, 48.0),
                        ))
                        .uv(uv)
                        .bg_fill(style::colors::PANEL_BG);

                        let response = ui.add(egui::ImageButton::new(image));

                        if response
                            .on_hover_text(format!(
                                "{}\n\nClick to take",
                                systems::item_name(*item_type)
                            ))
                            .clicked()
                        {
                            actions.chest_item_to_take = Some(i);
                        }
                    }
                });
            }

            ui.add_space(10.0);
            ui.separator();
            ui.horizontal(|ui| {
                if has_contents {
                    if ui.button("Take All").clicked() {
                        actions.chest_take_all = true;
                    }
                }
                if ui.button("Close").clicked() {
                    actions.close_chest = true;
                }
            });
        });
}

/// Render the dialogue window for NPC conversations
pub fn draw_dialogue_window(
    ctx: &egui::Context,
    data: &DialogueWindowData,
    actions: &mut UiActions,
) {
    egui::Window::new(&data.npc_name)
        .default_pos([
            data.viewport_width / 2.0 - 200.0,
            data.viewport_height / 2.0 - 100.0,
        ])
        .default_size([400.0, 200.0])
        .collapsible(false)
        .resizable(false)
        .frame(style::dungeon_window_frame())
        .show(ctx, |ui| {
            // NPC's dialogue text
            ui.add_space(5.0);
            ui.label(
                egui::RichText::new(&data.text)
                    .size(14.0),
            );
            ui.add_space(15.0);
            ui.separator();
            ui.add_space(10.0);

            // Response options as buttons
            for (i, option_text) in data.options.iter().enumerate() {
                if ui.button(option_text).clicked() {
                    actions.dialogue_option_selected = Some(i);
                }
                ui.add_space(3.0);
            }
        });
}

/// Extract dialogue window data from the world
pub fn get_dialogue_window_data(
    world: &World,
    talking_to: Option<hecs::Entity>,
    viewport_width: f32,
    viewport_height: f32,
) -> Option<DialogueWindowData> {
    let npc_id = talking_to?;
    let dialogue = world.get::<&Dialogue>(npc_id).ok()?;
    let node = systems::dialogue::current_node(&dialogue)?;

    Some(DialogueWindowData {
        npc_name: dialogue.name.clone(),
        text: node.text.clone(),
        options: node.options.iter().map(|o| o.label.clone()).collect(),
        viewport_width,
        viewport_height,
    })
}

/// Render the inventory/character window
pub fn draw_inventory_window(
    ctx: &egui::Context,
    world: &World,
    player_entity: hecs::Entity,
    data: &InventoryWindowData,
    icons: &UiIcons,
    ui_state: &mut GameUiState,
    actions: &mut UiActions,
) {
    egui::Window::new("Character")
        .default_pos([
            data.viewport_width / 2.0 - 300.0,
            data.viewport_height / 2.0 - 250.0,
        ])
        .default_size([600.0, 500.0])
        .collapsible(false)
        .resizable(true)
        .frame(style::dungeon_window_frame())
        .show(ctx, |ui| {
            if let Ok(stats) = world.get::<&Stats>(player_entity) {
                ui.columns(2, |columns| {
                    // Left column: Stats + Equipment
                    draw_stats_column(&mut columns[0], world, player_entity, &stats, icons, ui_state, actions);

                    // Right column: Inventory
                    draw_inventory_column(&mut columns[1], world, player_entity, icons, ui_state, actions);
                });
            }
        });

    // Draw context menu popup (outside the main window)
    if let Some((item_idx, pos)) = ui_state.item_context_menu {
        // Get the item type to show appropriate options
        let item_type = world
            .get::<&Inventory>(player_entity)
            .ok()
            .and_then(|inv| inv.items.get(item_idx).copied());

        if let Some(item_type) = item_type {
            let is_throwable = systems::items::item_is_throwable(item_type);

            egui::Area::new(egui::Id::new("item_context_menu"))
                .fixed_pos(pos)
                .order(egui::Order::Foreground)
                .show(ctx, |ui| {
                    style::dungeon_window_frame().show(ui, |ui| {
                        ui.set_min_width(120.0);

                        // Show options based on item type
                        if is_throwable {
                            if ui.button("Drink").clicked() {
                                actions.item_to_use = Some(item_idx);
                                ui_state.item_context_menu = None;
                            }
                            if ui.button("Throw").clicked() {
                                actions.item_to_throw = Some(item_idx);
                                ui_state.item_context_menu = None;
                            }
                        } else {
                            // Non-throwable items: Use/Equip
                            let is_weapon = matches!(item_type, crate::components::ItemType::Sword | crate::components::ItemType::Bow);
                            let button_text = if is_weapon { "Equip" } else { "Use" };
                            if ui.button(button_text).clicked() {
                                actions.item_to_use = Some(item_idx);
                                ui_state.item_context_menu = None;
                            }
                        }

                        // Drop option for all items
                        if ui.button("Drop").clicked() {
                            actions.item_to_drop = Some(item_idx);
                            ui_state.item_context_menu = None;
                        }

                        ui.separator();
                        if ui.button("Cancel").clicked() {
                            ui_state.item_context_menu = None;
                        }
                    });
                });

            // Close context menu on ESC key
            if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
                ui_state.item_context_menu = None;
            }

            // Close context menu if left-clicked elsewhere (not on the popup)
            // Using primary_clicked to avoid closing on the same right-click that opened the menu
            if ctx.input(|i| i.pointer.primary_clicked()) {
                // Check if click was outside the popup
                let popup_rect = egui::Rect::from_min_size(pos, egui::vec2(120.0, 100.0));
                if let Some(pointer_pos) = ctx.input(|i| i.pointer.interact_pos()) {
                    if !popup_rect.contains(pointer_pos) {
                        ui_state.item_context_menu = None;
                    }
                }
            }
        } else {
            // Item no longer exists, close menu
            ui_state.item_context_menu = None;
        }
    }

    // Draw equipped item context menu popup
    if let Some(pos) = ui_state.equipped_context_menu {
        // Check if player still has a weapon equipped
        let has_weapon = world
            .get::<&Equipment>(player_entity)
            .map(|eq| eq.weapon.is_some())
            .unwrap_or(false);

        if has_weapon {
            egui::Area::new(egui::Id::new("equipped_context_menu"))
                .fixed_pos(pos)
                .order(egui::Order::Foreground)
                .show(ctx, |ui| {
                    style::dungeon_window_frame().show(ui, |ui| {
                        ui.set_min_width(120.0);

                        if ui.button("Unequip").clicked() {
                            actions.unequip_weapon = true;
                            ui_state.equipped_context_menu = None;
                        }
                        if ui.button("Drop").clicked() {
                            actions.drop_equipped_weapon = true;
                            ui_state.equipped_context_menu = None;
                        }

                        ui.separator();
                        if ui.button("Cancel").clicked() {
                            ui_state.equipped_context_menu = None;
                        }
                    });
                });

            // Close context menu on ESC key
            if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
                ui_state.equipped_context_menu = None;
            }

            // Close context menu if left-clicked elsewhere
            if ctx.input(|i| i.pointer.primary_clicked()) {
                let popup_rect = egui::Rect::from_min_size(pos, egui::vec2(120.0, 100.0));
                if let Some(pointer_pos) = ctx.input(|i| i.pointer.interact_pos()) {
                    if !popup_rect.contains(pointer_pos) {
                        ui_state.equipped_context_menu = None;
                    }
                }
            }
        } else {
            // Weapon no longer equipped, close menu
            ui_state.equipped_context_menu = None;
        }
    }
}

fn draw_stats_column(
    ui: &mut egui::Ui,
    world: &World,
    player_entity: hecs::Entity,
    stats: &Stats,
    icons: &UiIcons,
    ui_state: &mut GameUiState,
    actions: &mut UiActions,
) {
    ui.vertical(|ui| {
        ui.heading("CHARACTER STATS");
        ui.separator();
        ui.add_space(10.0);
        ui.label(format!("Strength: {}", stats.strength));
        ui.add_space(5.0);
        ui.label(format!("Intelligence: {}", stats.intelligence));
        ui.add_space(5.0);
        ui.label(format!("Agility: {}", stats.agility));
        ui.add_space(10.0);
        ui.separator();

        let carry_capacity = stats.strength as f32 * 2.0;
        if let Ok(inventory) = world.get::<&Inventory>(player_entity) {
            ui.label(format!(
                "Weight: {:.1} / {:.1} kg",
                inventory.current_weight_kg, carry_capacity
            ));

            ui.add_space(10.0);
            ui.horizontal(|ui| {
                let size = egui::vec2(24.0, 24.0);
                let (rect, _response) = ui.allocate_exact_size(size, egui::Sense::hover());

                ui.painter().rect_filled(rect, 0.0, egui::Color32::BLACK);

                let coin_img = egui::Image::new(egui::load::SizedTexture::new(
                    icons.items_texture_id,
                    size,
                ))
                .uv(icons.coins_uv);
                coin_img.paint_at(ui, rect);

                ui.label(format!("{} gold", inventory.gold));
            });
        }

        ui.add_space(20.0);
        ui.heading("EQUIPMENT");
        ui.separator();
        ui.add_space(10.0);

        if let Ok(equipment) = world.get::<&Equipment>(player_entity) {
            // Single weapon slot
            ui.horizontal(|ui| {
                ui.label("Weapon:");
                match &equipment.weapon {
                    Some(crate::components::EquippedWeapon::Melee(weapon)) => {
                        let size = egui::vec2(48.0, 48.0);
                        let (rect, response) = ui.allocate_exact_size(size, egui::Sense::click());

                        ui.painter().rect_filled(rect, 0.0, egui::Color32::BLACK);

                        let image = egui::Image::new(egui::load::SizedTexture::new(
                            icons.items_texture_id,
                            size,
                        ))
                        .uv(icons.sword_uv);
                        image.paint_at(ui, rect);

                        let response = response.on_hover_text(format!(
                            "{}\n\nDamage: {} + {} = {}\n\nClick to unequip\nRight-click for options",
                            weapon.name,
                            weapon.base_damage,
                            weapon.damage_bonus,
                            systems::weapon_damage(weapon)
                        ));

                        // Left-click unequips
                        if response.clicked() {
                            actions.unequip_weapon = true;
                        }

                        // Right-click opens context menu
                        if response.secondary_clicked() {
                            ui_state.equipped_context_menu = Some(response.rect.right_top());
                        }
                    }
                    Some(crate::components::EquippedWeapon::Ranged(bow)) => {
                        let size = egui::vec2(48.0, 48.0);
                        let (rect, response) = ui.allocate_exact_size(size, egui::Sense::click());

                        ui.painter().rect_filled(rect, 0.0, egui::Color32::BLACK);

                        let image = egui::Image::new(egui::load::SizedTexture::new(
                            icons.items_texture_id,
                            size,
                        ))
                        .uv(icons.bow_uv);
                        image.paint_at(ui, rect);

                        let response = response.on_hover_text(format!(
                            "{}\n\nDamage: {}\nSpeed: {:.0} tiles/sec\n\nClick to unequip\nRight-click for options",
                            bow.name,
                            bow.base_damage,
                            bow.arrow_speed
                        ));

                        // Left-click unequips
                        if response.clicked() {
                            actions.unequip_weapon = true;
                        }

                        // Right-click opens context menu
                        if response.secondary_clicked() {
                            ui_state.equipped_context_menu = Some(response.rect.right_top());
                        }
                    }
                    None => {
                        ui.label(
                            egui::RichText::new("(none)")
                                .italics()
                                .color(style::colors::TEXT_MUTED),
                        );
                    }
                }
            });
        }
    });
}

fn draw_inventory_column(
    ui: &mut egui::Ui,
    world: &World,
    player_entity: hecs::Entity,
    icons: &UiIcons,
    ui_state: &mut GameUiState,
    actions: &mut UiActions,
) {
    ui.vertical(|ui| {
        ui.heading("INVENTORY");
        ui.separator();
        ui.add_space(10.0);

        if let Ok(inventory) = world.get::<&Inventory>(player_entity) {
            if inventory.items.is_empty() {
                ui.label(
                    egui::RichText::new("(empty)")
                        .italics()
                        .color(style::colors::TEXT_MUTED),
                );
            } else {
                ui.horizontal_wrapped(|ui| {
                    for (i, item_type) in inventory.items.iter().enumerate() {
                        let uv = icons.get_item_uv(*item_type);
                        let is_throwable = systems::items::item_is_throwable(*item_type);

                        // Allocate space and paint black background manually
                        let size = egui::vec2(48.0, 48.0);
                        let (rect, response) = ui.allocate_exact_size(size, egui::Sense::click());

                        // Paint black background first
                        ui.painter().rect_filled(rect, 0.0, egui::Color32::BLACK);

                        // Then paint the image on top
                        let image = egui::Image::new(egui::load::SizedTexture::new(
                            icons.items_texture_id,
                            size,
                        ))
                        .uv(uv);
                        image.paint_at(ui, rect);

                        // Build hover text based on item type
                        let hover_text = if is_throwable {
                            format!(
                                "{}\n\nLeft-click to drink\nRight-click for options",
                                systems::item_name(*item_type)
                            )
                        } else {
                            format!(
                                "{}\n\nLeft-click to use\nRight-click for options",
                                systems::item_name(*item_type)
                            )
                        };

                        let response = response.on_hover_text(hover_text);

                        // Left-click: use/drink the item
                        if response.clicked() {
                            actions.item_to_use = Some(i);
                        }

                        // Right-click: open context menu (for all items)
                        if response.secondary_clicked() {
                            // Get the screen position for the popup
                            let pos = response.rect.right_top();
                            ui_state.item_context_menu = Some((i, pos));
                        }
                    }
                });
            }
        }
    });
}

/// Helper struct containing pre-computed UV coordinates for UI icons
pub struct UiIcons {
    /// Texture ID for Tiles sheet (terrain, UI elements)
    pub tiles_texture_id: egui::TextureId,
    /// Texture ID for Rogues sheet (player, NPCs)
    pub rogues_texture_id: egui::TextureId,
    /// Texture ID for Monsters sheet (enemies)
    pub monsters_texture_id: egui::TextureId,
    /// Texture ID for Items sheet (weapons, potions, scrolls)
    pub items_texture_id: egui::TextureId,
    // Items sheet UVs
    pub sword_uv: egui::Rect,
    pub bow_uv: egui::Rect,
    pub dagger_uv: egui::Rect,
    pub red_potion_uv: egui::Rect,
    pub green_potion_uv: egui::Rect,
    pub amber_potion_uv: egui::Rect,
    pub blue_potion_uv: egui::Rect,
    pub scroll_uv: egui::Rect,
    pub coins_uv: egui::Rect,
    pub heart_uv: egui::Rect,
    pub diamond_uv: egui::Rect,
    // Ability icons
    pub cleave_uv: egui::Rect,
    pub sprint_uv: egui::Rect,
}

impl UiIcons {
    pub fn new(
        tileset: &MultiTileset,
        tiles_egui_id: egui::TextureId,
        rogues_egui_id: egui::TextureId,
        monsters_egui_id: egui::TextureId,
        items_egui_id: egui::TextureId,
    ) -> Self {
        Self {
            tiles_texture_id: tiles_egui_id,
            rogues_texture_id: rogues_egui_id,
            monsters_texture_id: monsters_egui_id,
            items_texture_id: items_egui_id,
            sword_uv: tileset.get_egui_uv(tile_ids::SWORD.0, tile_ids::SWORD.1),
            bow_uv: tileset.get_egui_uv(tile_ids::BOW.0, tile_ids::BOW.1),
            dagger_uv: tileset.get_egui_uv(tile_ids::DAGGER.0, tile_ids::DAGGER.1),
            red_potion_uv: tileset.get_egui_uv(tile_ids::RED_POTION.0, tile_ids::RED_POTION.1),
            green_potion_uv: tileset.get_egui_uv(tile_ids::GREEN_POTION.0, tile_ids::GREEN_POTION.1),
            amber_potion_uv: tileset.get_egui_uv(tile_ids::AMBER_POTION.0, tile_ids::AMBER_POTION.1),
            blue_potion_uv: tileset.get_egui_uv(tile_ids::BLUE_POTION.0, tile_ids::BLUE_POTION.1),
            scroll_uv: tileset.get_egui_uv(tile_ids::SCROLL.0, tile_ids::SCROLL.1),
            coins_uv: tileset.get_egui_uv(tile_ids::COINS.0, tile_ids::COINS.1),
            heart_uv: tileset.get_egui_uv(tile_ids::HEART.0, tile_ids::HEART.1),
            diamond_uv: tileset.get_egui_uv(tile_ids::DIAMOND.0, tile_ids::DIAMOND.1),
            // Ability icons: AXE for Cleave, BLUE_POTION for Sprint
            cleave_uv: tileset.get_egui_uv(tile_ids::AXE.0, tile_ids::AXE.1),
            sprint_uv: tileset.get_egui_uv(tile_ids::BLUE_POTION.0, tile_ids::BLUE_POTION.1),
        }
    }

    /// Get the texture ID for a specific sprite sheet
    pub fn texture_for_sheet(&self, sheet: SpriteSheet) -> egui::TextureId {
        match sheet {
            SpriteSheet::Tiles => self.tiles_texture_id,
            SpriteSheet::Rogues => self.rogues_texture_id,
            SpriteSheet::Monsters => self.monsters_texture_id,
            SpriteSheet::Items => self.items_texture_id,
        }
    }

    /// Get the UV for a specific item type
    pub fn get_item_uv(&self, item_type: ItemType) -> egui::Rect {
        match item_type {
            ItemType::Sword => self.sword_uv,
            ItemType::Bow => self.bow_uv,
            ItemType::Dagger => self.dagger_uv,
            ItemType::HealthPotion => self.red_potion_uv,
            ItemType::RegenerationPotion => self.green_potion_uv,
            ItemType::StrengthPotion => self.amber_potion_uv,
            ItemType::ConfusionPotion => self.blue_potion_uv,
            ItemType::ScrollOfInvisibility
            | ItemType::ScrollOfSpeed
            | ItemType::ScrollOfProtection
            | ItemType::ScrollOfBlink
            | ItemType::ScrollOfFear
            | ItemType::ScrollOfFireball
            | ItemType::ScrollOfReveal
            | ItemType::ScrollOfMapping
            | ItemType::ScrollOfSlow => self.scroll_uv,
        }
    }

    /// Get the texture ID for items (weapons, potions, scrolls)
    pub fn items_texture(&self) -> egui::TextureId {
        self.items_texture_id
    }
}

/// Extract status bar data from the world
pub fn get_status_bar_data(world: &World, player_entity: hecs::Entity) -> StatusBarData {
    let (health_current, health_max) = world
        .get::<&Health>(player_entity)
        .map(|h| (h.current, h.max))
        .unwrap_or((0, 0));

    let gold = world
        .get::<&Inventory>(player_entity)
        .map(|inv| inv.gold)
        .unwrap_or(0);

    let (xp_progress, xp_level) = world
        .get::<&crate::components::Experience>(player_entity)
        .map(|exp| (systems::xp_progress(&exp), exp.level))
        .unwrap_or((0.0, 1));

    // Collect active status effects
    let active_effects = world
        .get::<&StatusEffects>(player_entity)
        .map(|effects| {
            effects
                .effects
                .iter()
                .map(|e| (e.effect_type, e.remaining_duration))
                .collect()
        })
        .unwrap_or_default();

    StatusBarData {
        health_current,
        health_max,
        xp_progress,
        xp_level,
        gold,
        active_effects,
    }
}

/// Extract loot window data from the world
pub fn get_loot_window_data(
    world: &World,
    open_chest: Option<hecs::Entity>,
    viewport_width: f32,
    viewport_height: f32,
) -> Option<LootWindowData> {
    let chest_id = open_chest?;
    let container = world.get::<&Container>(chest_id).ok()?;

    Some(LootWindowData {
        items: container.items.clone(),
        gold: container.gold,
        viewport_width,
        viewport_height,
    })
}

/// Extract player buff aura data from the world
pub fn get_buff_aura_data(
    world: &World,
    player_entity: hecs::Entity,
    game_time: f32,
) -> Option<PlayerBuffAuraData> {
    use crate::components::VisualPosition;

    let player_vis_pos = world.get::<&VisualPosition>(player_entity).ok()?;
    let effects = world.get::<&StatusEffects>(player_entity).ok()?;

    Some(PlayerBuffAuraData {
        player_x: player_vis_pos.x,
        player_y: player_vis_pos.y,
        has_regen: systems::effects::has_effect(&effects, StatusEffectType::Regenerating),
        has_protected: systems::effects::has_effect(&effects, StatusEffectType::Protected),
        time: game_time,
    })
}

/// Extract targeting overlay data from targeting mode and world state
pub fn get_targeting_overlay_data(
    world: &World,
    player_entity: hecs::Entity,
    targeting_mode: Option<&crate::input::TargetingMode>,
    cursor_screen_pos: (f32, f32),
    camera: &Camera,
) -> Option<TargetingOverlayData> {
    use crate::components::Position;

    let targeting = targeting_mode?;

    let player_pos = world.get::<&Position>(player_entity).ok()?;

    // Convert screen cursor to world coordinates
    let world_pos = camera.screen_to_world(cursor_screen_pos.0, cursor_screen_pos.1);
    let cursor_x = world_pos.x.floor() as i32;
    let cursor_y = world_pos.y.floor() as i32;

    Some(TargetingOverlayData {
        player_x: player_pos.x,
        player_y: player_pos.y,
        cursor_x,
        cursor_y,
        max_range: targeting.max_range,
        radius: targeting.radius,
        is_blink: matches!(targeting.item_type, ItemType::ScrollOfBlink),
        item_type: Some(targeting.item_type),
    })
}

/// Extract enemy status effect data from the world
pub fn get_enemy_status_data(world: &World, grid: &crate::grid::Grid) -> Vec<EnemyStatusData> {
    use crate::components::{ChaseAI, VisualPosition, EffectType};

    world
        .query::<(&VisualPosition, &ChaseAI, &StatusEffects)>()
        .iter()
        .filter(|(_, (pos, _, _))| {
            // Only show for visible tiles
            grid.get(pos.x as i32, pos.y as i32)
                .map(|t| t.visible)
                .unwrap_or(false)
        })
        .map(|(_, (pos, _, status_effects))| EnemyStatusData {
            x: pos.x,
            y: pos.y,
            is_feared: systems::effects::has_effect(status_effects, EffectType::Feared),
            is_slowed: systems::effects::has_effect(status_effects, EffectType::Slowed),
            is_confused: systems::effects::has_effect(status_effects, EffectType::Confused),
        })
        .collect()
}

/// Render floating damage numbers
pub fn draw_damage_numbers(ctx: &egui::Context, effects: &[VisualEffect], camera: &Camera) {
    let painter = ctx.layer_painter(egui::LayerId::new(
        egui::Order::Foreground,
        egui::Id::new("damage_numbers"),
    ));

    // Get egui's pixels per point for HiDPI scaling
    let ppp = ctx.pixels_per_point();

    for effect in effects {
        let VfxType::DamageNumber { amount } = &effect.effect_type else {
            continue;
        };

        let progress = effect.progress();

        // Convert world position to screen position
        // The effect position is already centered on the tile
        let rise_offset = progress * DAMAGE_NUMBER_RISE;
        let world_x = effect.x;
        let world_y = effect.y + rise_offset; // Rise up (positive Y is up in world space)

        // Transform from world to screen coordinates (in physical pixels)
        let screen_pos = camera.world_to_screen(world_x, world_y);

        // Convert to egui points (egui uses logical points, not physical pixels)
        let egui_x = screen_pos.0 / ppp;
        let egui_y = screen_pos.1 / ppp;

        // Fade out as progress increases
        let alpha = ((1.0 - progress) * 255.0) as u8;

        // Red color for damage
        let color = egui::Color32::from_rgba_unmultiplied(255, 80, 80, alpha);

        // Draw the damage number
        let text = format!("{}", amount);
        let font_id = egui::FontId::monospace(20.0);

        painter.text(
            egui::pos2(egui_x, egui_y),
            egui::Align2::CENTER_CENTER,
            text,
            font_id,
            color,
        );
    }
}

/// Render alert indicators ("!") when enemies spot the player
pub fn draw_alert_indicators(ctx: &egui::Context, effects: &[VisualEffect], camera: &Camera) {
    let painter = ctx.layer_painter(egui::LayerId::new(
        egui::Order::Foreground,
        egui::Id::new("alert_indicators"),
    ));

    // Get egui's pixels per point for HiDPI scaling
    let ppp = ctx.pixels_per_point();

    for effect in effects {
        let VfxType::Alert = &effect.effect_type else {
            continue;
        };

        let progress = effect.progress();

        // Pop up animation: start small, grow to full size, then shrink slightly
        let scale = if progress < 0.2 {
            // Quick pop-in (0.0 to 0.2 progress -> 0.0 to 1.2 scale)
            progress * 6.0
        } else if progress < 0.4 {
            // Settle to normal size (0.2 to 0.4 progress -> 1.2 to 1.0 scale)
            1.2 - (progress - 0.2) * 1.0
        } else {
            // Hold at normal size, then fade
            1.0
        };

        // Rise slightly above the entity
        let rise_offset = 0.8;
        let world_x = effect.x;
        let world_y = effect.y + rise_offset;

        // Transform from world to screen coordinates
        let screen_pos = camera.world_to_screen(world_x, world_y);

        // Convert to egui points
        let egui_x = screen_pos.0 / ppp;
        let egui_y = screen_pos.1 / ppp;

        // Fade out near the end
        let alpha = if progress > 0.7 {
            ((1.0 - progress) / 0.3 * 255.0) as u8
        } else {
            255
        };

        // Yellow/orange color for alert
        let color = egui::Color32::from_rgba_unmultiplied(255, 200, 50, alpha);

        // Draw the "!"
        let font_size = 28.0 * scale;
        let font_id = egui::FontId::monospace(font_size);

        painter.text(
            egui::pos2(egui_x, egui_y),
            egui::Align2::CENTER_CENTER,
            "!",
            font_id,
            color,
        );
    }
}

/// Data for an enemy with active status effects
pub struct EnemyStatusData {
    pub x: f32,
    pub y: f32,
    pub is_feared: bool,
    pub is_slowed: bool,
    pub is_confused: bool,
}

/// Render persistent status effect indicators above enemies
pub fn draw_enemy_status_indicators(
    ctx: &egui::Context,
    camera: &Camera,
    enemies: &[EnemyStatusData],
    time: f32,
) {
    let painter = ctx.layer_painter(egui::LayerId::new(
        egui::Order::Foreground,
        egui::Id::new("enemy_status_indicators"),
    ));

    let ppp = ctx.pixels_per_point();

    for enemy in enemies {
        if !enemy.is_feared && !enemy.is_slowed && !enemy.is_confused {
            continue;
        }

        // Collect symbols to display
        let mut symbols: Vec<(&str, egui::Color32)> = Vec::new();
        if enemy.is_feared {
            symbols.push(("!", egui::Color32::from_rgb(255, 80, 80))); // Red for fear
        }
        if enemy.is_slowed {
            symbols.push(("❄", egui::Color32::from_rgb(100, 150, 255))); // Blue for slow
        }
        if enemy.is_confused {
            symbols.push(("?", egui::Color32::from_rgb(200, 100, 200))); // Purple for confusion
        }

        // Subtle pulsing animation
        let pulse = 0.85 + 0.15 * (time * 4.0).sin();

        // Position above the entity
        let rise_offset = 0.75;
        let world_x = enemy.x;
        let world_y = enemy.y + rise_offset;

        let screen_pos = camera.world_to_screen(world_x, world_y);
        let egui_x = screen_pos.0 / ppp;
        let egui_y = screen_pos.1 / ppp;

        // Draw each symbol, offset horizontally if multiple
        let total_width = symbols.len() as f32 * 16.0;
        let start_x = egui_x - total_width / 2.0 + 8.0;

        for (i, (symbol, color)) in symbols.iter().enumerate() {
            let x = start_x + i as f32 * 16.0;
            let font_size = 20.0 * pulse;
            let font_id = egui::FontId::monospace(font_size);

            painter.text(
                egui::pos2(x, egui_y),
                egui::Align2::CENTER_CENTER,
                *symbol,
                font_id,
                *color,
            );
        }
    }
}

/// Render explosion effects (fireballs)
pub fn draw_explosions(ctx: &egui::Context, effects: &[VisualEffect], camera: &Camera) {
    let painter = ctx.layer_painter(egui::LayerId::new(
        egui::Order::Foreground,
        egui::Id::new("explosions"),
    ));

    let ppp = ctx.pixels_per_point();
    let tile_size = camera.zoom / ppp;

    for effect in effects {
        let VfxType::Explosion { radius } = &effect.effect_type else {
            continue;
        };

        let progress = effect.progress();

        // Explosion expands outward then fades
        let expand = if progress < 0.3 {
            progress / 0.3
        } else {
            1.0
        };

        let alpha = if progress > 0.5 {
            ((1.0 - progress) / 0.5 * 200.0) as u8
        } else {
            200
        };

        // Draw expanding circles for the explosion
        for r in 0..=*radius {
            let r_progress = r as f32 / (*radius as f32).max(1.0);
            let current_expand = expand * (1.0 - r_progress * 0.3);

            // Calculate color: orange/red gradient
            let red = 255;
            let green = (150.0 * (1.0 - r_progress)) as u8;
            let blue = (50.0 * (1.0 - r_progress)) as u8;
            let ring_alpha = (alpha as f32 * (1.0 - r_progress * 0.5)) as u8;

            let color = egui::Color32::from_rgba_unmultiplied(red, green, blue, ring_alpha);

            // Draw tiles in this ring
            for dx in -r..=r {
                for dy in -r..=r {
                    let dist = dx.abs().max(dy.abs());
                    if dist != r {
                        continue;
                    }

                    let world_x = effect.x + dx as f32;
                    let world_y = effect.y + dy as f32;

                    let screen_pos = camera.world_to_screen(world_x - 0.5, world_y - 0.5);
                    let egui_x = screen_pos.0 / ppp;
                    let egui_y = screen_pos.1 / ppp;

                    let size = tile_size * current_expand;
                    let offset = (tile_size - size) / 2.0;

                    let rect = egui::Rect::from_min_size(
                        egui::pos2(egui_x + offset, egui_y - tile_size + offset),
                        egui::vec2(size, size),
                    );
                    painter.rect_filled(rect, size / 4.0, color);
                }
            }
        }
    }
}

/// Render potion splash effects
pub fn draw_potion_splashes(ctx: &egui::Context, effects: &[VisualEffect], camera: &Camera) {
    let painter = ctx.layer_painter(egui::LayerId::new(
        egui::Order::Foreground,
        egui::Id::new("potion_splashes"),
    ));

    let ppp = ctx.pixels_per_point();
    let tile_size = camera.zoom / ppp;

    for effect in effects {
        let VfxType::PotionSplash { potion_type } = &effect.effect_type else {
            continue;
        };

        let progress = effect.progress();

        // Determine color based on potion type
        let (base_r, base_g, base_b) = match potion_type {
            ItemType::HealthPotion => (220, 50, 50),      // Red
            ItemType::RegenerationPotion => (50, 200, 80), // Green
            ItemType::StrengthPotion => (220, 160, 50),    // Amber/Orange
            ItemType::ConfusionPotion => (80, 120, 220),   // Blue
            _ => (200, 200, 200),                          // Fallback gray
        };

        // Splash expands outward then fades
        let expand = if progress < 0.2 {
            progress / 0.2
        } else {
            1.0
        };

        let alpha = if progress > 0.4 {
            ((1.0 - progress) / 0.6 * 180.0) as u8
        } else {
            180
        };

        // Draw splash in the splash radius (1 tile)
        let radius = crate::constants::POTION_SPLASH_RADIUS;
        for r in 0..=radius {
            let r_progress = r as f32 / (radius as f32).max(1.0);
            let current_expand = expand * (1.0 - r_progress * 0.2);

            // Fade color slightly outward
            let red = (base_r as f32 * (1.0 - r_progress * 0.2)) as u8;
            let green = (base_g as f32 * (1.0 - r_progress * 0.2)) as u8;
            let blue = (base_b as f32 * (1.0 - r_progress * 0.2)) as u8;
            let ring_alpha = (alpha as f32 * (1.0 - r_progress * 0.4)) as u8;

            let color = egui::Color32::from_rgba_unmultiplied(red, green, blue, ring_alpha);

            // Draw tiles in this ring
            for dx in -r..=r {
                for dy in -r..=r {
                    let dist = dx.abs().max(dy.abs());
                    if dist != r {
                        continue;
                    }

                    let world_x = effect.x + dx as f32;
                    let world_y = effect.y + dy as f32;

                    let screen_pos = camera.world_to_screen(world_x - 0.5, world_y - 0.5);
                    let egui_x = screen_pos.0 / ppp;
                    let egui_y = screen_pos.1 / ppp;

                    let size = tile_size * current_expand;
                    let offset = (tile_size - size) / 2.0;

                    let rect = egui::Rect::from_min_size(
                        egui::pos2(egui_x + offset, egui_y - tile_size + offset),
                        egui::vec2(size, size),
                    );
                    painter.rect_filled(rect, size / 3.0, color);
                }
            }
        }
    }
}

/// Data for player buff aura visualization
pub struct PlayerBuffAuraData {
    pub player_x: f32,
    pub player_y: f32,
    pub has_regen: bool,
    pub has_protected: bool,
    pub time: f32, // For pulsing animation
}

/// Render glowing aura around player for active buffs (Regenerating, Protected)
pub fn draw_player_buff_auras(ctx: &egui::Context, camera: &Camera, data: Option<&PlayerBuffAuraData>) {
    let Some(data) = data else { return };
    if !data.has_regen && !data.has_protected {
        return;
    }

    let painter = ctx.layer_painter(egui::LayerId::new(
        egui::Order::Background,
        egui::Id::new("buff_auras"),
    ));

    let ppp = ctx.pixels_per_point();
    let tile_size = camera.zoom / ppp;

    // Convert player position to screen
    let screen_pos = camera.world_to_screen(data.player_x, data.player_y);
    let egui_x = screen_pos.0 / ppp;
    let egui_y = screen_pos.1 / ppp;

    // Center of the player tile
    let center = egui::pos2(egui_x + tile_size / 2.0, egui_y - tile_size / 2.0);

    // Pulsing effect
    let pulse = 0.7 + 0.3 * (data.time * 3.0).sin();

    // Draw regeneration aura (green glow)
    if data.has_regen {
        let base_alpha = (80.0 * pulse) as u8;
        let color = egui::Color32::from_rgba_unmultiplied(50, 255, 100, base_alpha);
        let radius = tile_size * 0.6 * (0.9 + 0.1 * pulse);
        painter.circle_filled(center, radius, color);

        // Inner brighter ring
        let inner_color = egui::Color32::from_rgba_unmultiplied(100, 255, 150, (40.0 * pulse) as u8);
        painter.circle_filled(center, radius * 0.7, inner_color);
    }

    // Draw protection aura (blue glow) - drawn on top if both active
    if data.has_protected {
        let base_alpha = (70.0 * pulse) as u8;
        let color = egui::Color32::from_rgba_unmultiplied(100, 150, 255, base_alpha);
        let radius = tile_size * 0.55 * (0.9 + 0.1 * pulse);
        painter.circle_stroke(center, radius, egui::Stroke::new(3.0 * pulse, color));

        // Shield icon effect - draw small diamond shapes around
        let shield_alpha = (100.0 * pulse) as u8;
        let shield_color = egui::Color32::from_rgba_unmultiplied(150, 180, 255, shield_alpha);
        let shield_radius = tile_size * 0.45;
        for i in 0..4 {
            let angle = (i as f32 * std::f32::consts::PI / 2.0) + data.time * 0.5;
            let px = center.x + angle.cos() * shield_radius;
            let py = center.y + angle.sin() * shield_radius;
            painter.circle_filled(egui::pos2(px, py), 2.0 * pulse, shield_color);
        }
    }
}

/// Data needed for the targeting overlay
pub struct TargetingOverlayData {
    pub player_x: i32,
    pub player_y: i32,
    pub cursor_x: i32,
    pub cursor_y: i32,
    pub max_range: i32,
    pub radius: i32,
    pub is_blink: bool,
    pub item_type: Option<ItemType>,
}

/// Draw the targeting overlay when in targeting mode
pub fn draw_targeting_overlay(
    ctx: &egui::Context,
    camera: &Camera,
    data: &TargetingOverlayData,
) {
    // Convert from screen pixels to egui points
    let ppp = ctx.pixels_per_point();

    // Tile size: camera.zoom pixels = 1 world unit, convert to egui points
    let tile_size = camera.zoom / ppp;

    // Get the painter for the foreground layer
    let painter = ctx.layer_painter(egui::LayerId::new(
        egui::Order::Foreground,
        egui::Id::new("targeting_overlay"),
    ));

    // Helper to convert world position to egui rect for a tile
    let tile_rect = |world_x: i32, world_y: i32| {
        let screen_pos = camera.world_to_screen(world_x as f32, world_y as f32);
        let egui_x = screen_pos.0 / ppp;
        let egui_y = screen_pos.1 / ppp;
        egui::Rect::from_min_size(
            egui::pos2(egui_x, egui_y - tile_size),
            egui::vec2(tile_size, tile_size),
        )
    };

    // Draw tiles in range with a subtle highlight
    let range_color = egui::Color32::from_rgba_unmultiplied(100, 150, 255, 40);

    for dx in -data.max_range..=data.max_range {
        for dy in -data.max_range..=data.max_range {
            // Use Chebyshev distance for range check
            let dist = dx.abs().max(dy.abs());
            if dist > data.max_range {
                continue;
            }

            let tile_x = data.player_x + dx;
            let tile_y = data.player_y + dy;

            let rect = tile_rect(tile_x, tile_y);
            painter.rect_filled(rect, 0.0, range_color);
        }
    }

    // Calculate distance from player to cursor
    let cursor_dist = (data.cursor_x - data.player_x).abs().max((data.cursor_y - data.player_y).abs());
    let in_range = cursor_dist <= data.max_range;

    // Draw cursor tile highlight
    let cursor_color = if in_range {
        if data.is_blink {
            egui::Color32::from_rgba_unmultiplied(100, 255, 100, 120) // Green for blink
        } else {
            egui::Color32::from_rgba_unmultiplied(255, 150, 50, 120) // Orange for fireball
        }
    } else {
        egui::Color32::from_rgba_unmultiplied(255, 50, 50, 120) // Red for out of range
    };

    let cursor_rect = tile_rect(data.cursor_x, data.cursor_y);
    painter.rect_filled(cursor_rect, 0.0, cursor_color);
    painter.rect_stroke(cursor_rect, 0.0, egui::Stroke::new(2.0, egui::Color32::WHITE));

    // For Fireball, draw the AoE radius around cursor
    if !data.is_blink && data.radius > 0 && in_range {
        let aoe_color = egui::Color32::from_rgba_unmultiplied(255, 100, 50, 60);

        for dx in -data.radius..=data.radius {
            for dy in -data.radius..=data.radius {
                // Skip the center tile (already highlighted)
                if dx == 0 && dy == 0 {
                    continue;
                }

                // Chebyshev distance for AoE
                let dist = dx.abs().max(dy.abs());
                if dist > data.radius {
                    continue;
                }

                let aoe_x = data.cursor_x + dx;
                let aoe_y = data.cursor_y + dy;

                let aoe_rect = tile_rect(aoe_x, aoe_y);
                painter.rect_filled(aoe_rect, 0.0, aoe_color);
            }
        }
    }

    // Draw info text near the cursor
    let info_text = if in_range {
        match data.item_type {
            Some(ItemType::ScrollOfBlink) => "Click to teleport",
            Some(ItemType::ScrollOfFireball) => "Click to cast fireball",
            Some(ItemType::HealthPotion) => "Click to throw healing potion",
            Some(ItemType::RegenerationPotion) => "Click to throw regen potion",
            Some(ItemType::StrengthPotion) => "Click to throw strength potion",
            Some(ItemType::ConfusionPotion) => "Click to throw confusion potion",
            _ => if data.is_blink { "Click to teleport" } else { "Click to use" },
        }
    } else {
        "Out of range"
    };

    let text_color = if in_range {
        egui::Color32::WHITE
    } else {
        egui::Color32::from_rgb(255, 100, 100)
    };

    // Position text above the cursor tile
    let text_pos = egui::pos2(
        cursor_rect.center().x,
        cursor_rect.min.y - 5.0,
    );

    painter.text(
        text_pos,
        egui::Align2::CENTER_BOTTOM,
        info_text,
        egui::FontId::monospace(14.0),
        text_color,
    );
}

/// Run the start screen UI for class selection.
/// Returns Some(PlayerClass) if the player clicked Start, None otherwise.
pub fn run_start_screen(
    egui_glow: &mut EguiGlow,
    window: &Window,
    tileset: &MultiTileset,
    icons: &UiIcons,
    selected_class: &mut Option<PlayerClass>,
) -> Option<PlayerClass> {
    let mut start_clicked = None;

    egui_glow.run(window, |ctx| {
        // Center the window
        egui::CentralPanel::default()
            .frame(egui::Frame::none().fill(egui::Color32::from_rgb(20, 20, 30)))
            .show(ctx, |ui| {
                ui.vertical_centered(|ui| {
                    ui.add_space(100.0);

                    // Title
                    ui.heading(
                        egui::RichText::new("Grid Roguelike")
                            .size(48.0)
                            .color(style::colors::DUNGEON_GOLD),
                    );

                    ui.add_space(40.0);

                    ui.label(
                        egui::RichText::new("Choose Your Class")
                            .size(24.0)
                            .color(egui::Color32::WHITE),
                    );

                    ui.add_space(30.0);

                    // Class selection buttons
                    ui.horizontal(|ui| {
                        ui.add_space((ui.available_width() - 300.0) / 2.0);

                        for class in PlayerClass::ALL {
                            let is_selected = *selected_class == Some(class);
                            let sprite = class.sprite();
                            let texture_id = icons.texture_for_sheet(sprite.0);
                            let uv_rect = tileset.get_egui_uv(sprite.0, sprite.1);

                            let (response, painter) = ui.allocate_painter(
                                egui::vec2(120.0, 150.0),
                                egui::Sense::click(),
                            );

                            // Background
                            let bg_color = if is_selected {
                                style::colors::DUNGEON_GOLD.gamma_multiply(0.3)
                            } else if response.hovered() {
                                egui::Color32::from_rgb(50, 50, 60)
                            } else {
                                egui::Color32::from_rgb(35, 35, 45)
                            };
                            painter.rect_filled(response.rect, 8.0, bg_color);

                            // Border
                            let border_color = if is_selected {
                                style::colors::DUNGEON_GOLD
                            } else {
                                egui::Color32::from_rgb(80, 80, 90)
                            };
                            painter.rect_stroke(
                                response.rect,
                                8.0,
                                egui::Stroke::new(2.0, border_color),
                            );

                            // Sprite (centered, larger)
                            let sprite_size = 64.0;
                            let sprite_rect = egui::Rect::from_center_size(
                                response.rect.center() - egui::vec2(0.0, 20.0),
                                egui::vec2(sprite_size, sprite_size),
                            );
                            painter.image(
                                texture_id,
                                sprite_rect,
                                uv_rect,
                                egui::Color32::WHITE,
                            );

                            // Class name
                            let text_pos = response.rect.center() + egui::vec2(0.0, 40.0);
                            painter.text(
                                text_pos,
                                egui::Align2::CENTER_CENTER,
                                class.name(),
                                egui::FontId::proportional(18.0),
                                egui::Color32::WHITE,
                            );

                            if response.clicked() {
                                *selected_class = Some(class);
                            }

                            ui.add_space(20.0);
                        }
                    });

                    ui.add_space(40.0);

                    // Start button
                    let start_enabled = selected_class.is_some();
                    let button = egui::Button::new(
                        egui::RichText::new("Start Game")
                            .size(24.0)
                            .color(if start_enabled {
                                egui::Color32::WHITE
                            } else {
                                egui::Color32::GRAY
                            }),
                    )
                    .min_size(egui::vec2(200.0, 50.0))
                    .fill(if start_enabled {
                        style::colors::DUNGEON_GREEN
                    } else {
                        egui::Color32::from_rgb(50, 50, 50)
                    });

                    if ui.add_enabled(start_enabled, button).clicked() {
                        start_clicked = *selected_class;
                    }

                    ui.add_space(20.0);

                    // Class description
                    if let Some(class) = selected_class {
                        let (str, int, agi) = class.stats();
                        let weapon = match class {
                            PlayerClass::Fighter => "Sword",
                            PlayerClass::Ranger => "Bow",
                        };
                        let inventory = match class {
                            PlayerClass::Fighter => "Bow",
                            PlayerClass::Ranger => "Dagger",
                        };

                        ui.label(
                            egui::RichText::new(format!(
                                "STR: {}  INT: {}  AGI: {}",
                                str, int, agi
                            ))
                            .size(16.0)
                            .color(egui::Color32::LIGHT_GRAY),
                        );
                        ui.label(
                            egui::RichText::new(format!(
                                "Equipped: {}  |  Inventory: {}",
                                weapon, inventory
                            ))
                            .size(14.0)
                            .color(egui::Color32::GRAY),
                        );
                    }
                });
            });
    });

    start_clicked
}

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
    mouse_pos: (f32, f32),
    game_time: f32,
) -> UiActions {
    let mut actions = UiActions::default();

    // Get status bar data
    let status_data = get_status_bar_data(world, player_entity);

    // Get ability bar data (if player has a class ability)
    let ability_data = world.get::<&ClassAbility>(player_entity).ok().map(|ability| {
        // Check if player CAN have enough energy (max_energy >= cost)
        // The actual waiting for energy happens when the action is executed
        let can_afford = world.get::<&crate::components::Actor>(player_entity)
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
    let buff_aura_data = get_buff_aura_data(world, player_entity, game_time);
    let targeting_data = get_targeting_overlay_data(
        world,
        player_entity,
        targeting_mode,
        mouse_pos,
        camera,
    );
    let enemy_status_data = get_enemy_status_data(world, grid);

    egui_glow.run(window, |ctx| {
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
