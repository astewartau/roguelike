//! UI rendering using egui.
//!
//! Handles all game UI: status bars, inventory, loot windows, etc.

use crate::camera::Camera;
use crate::components::{Container, Equipment, Health, Inventory, ItemType, Stats};
use crate::constants::DAMAGE_NUMBER_RISE;
use crate::systems;
use crate::tile::tile_ids;
use crate::tileset::Tileset;
use crate::vfx::{EffectType, VisualEffect};
use hecs::World;

/// Data needed to render the status bar
pub struct StatusBarData {
    pub health_current: i32,
    pub health_max: i32,
    pub xp_progress: f32,
    pub xp_level: u32,
    pub gold: u32,
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

/// Actions the UI wants to perform (returned to game logic)
#[derive(Default)]
pub struct UiActions {
    pub item_to_use: Option<usize>,
    pub chest_item_to_take: Option<usize>,
    pub chest_take_all: bool,
    pub chest_take_gold: bool,
    pub close_chest: bool,
}

// =============================================================================
// GAME UI STATE (event-driven)
// =============================================================================

use crate::events::GameEvent;
use hecs::Entity;

/// Game UI state that responds to events.
///
/// This centralizes UI state management and decouples it from game logic.
/// The UI reacts to game events rather than being set imperatively.
pub struct GameUiState {
    /// Currently open chest/container (for loot window)
    pub open_chest: Option<Entity>,
    /// Show inventory window
    pub show_inventory: bool,
    /// Show grid overlay
    pub show_grid_lines: bool,
    /// The player entity (needed to filter events)
    player_entity: Entity,
}

impl GameUiState {
    pub fn new(player_entity: Entity) -> Self {
        Self {
            open_chest: None,
            show_inventory: false,
            show_grid_lines: false,
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
            GameEvent::EntityMoved { entity, .. } => {
                // Close chest when player moves away
                if *entity == self.player_entity {
                    self.open_chest = None;
                }
            }
            _ => {}
        }
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
}

// =============================================================================
// DEVELOPER MENU
// =============================================================================

/// Tools available in the developer menu
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum DevTool {
    SpawnChest,
    SpawnEnemy,
}

impl DevTool {
    pub fn name(&self) -> &'static str {
        match self {
            DevTool::SpawnChest => "Chest",
            DevTool::SpawnEnemy => "Enemy",
        }
    }

    pub fn tile_id(&self) -> u32 {
        match self {
            DevTool::SpawnChest => tile_ids::CHEST_CLOSED,
            DevTool::SpawnEnemy => tile_ids::SKELETON,
        }
    }
}

/// State for the developer menu
pub struct DevMenu {
    pub visible: bool,
    pub selected_tool: Option<DevTool>,
}

impl Default for DevMenu {
    fn default() -> Self {
        Self {
            visible: false,
            selected_tool: None,
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
}

/// Draw the developer menu
pub fn draw_dev_menu(
    ctx: &egui::Context,
    dev_menu: &mut DevMenu,
    tileset_texture_id: egui::TextureId,
    tileset: &Tileset,
) {
    if !dev_menu.visible {
        return;
    }

    egui::Window::new("Developer Tools")
        .fixed_pos([10.0, 120.0])
        .fixed_size([200.0, 120.0])
        .title_bar(true)
        .collapsible(false)
        .show(ctx, |ui| {
            ui.horizontal(|ui| {
                let tools = [DevTool::SpawnChest, DevTool::SpawnEnemy];

                for tool in tools {
                    let is_selected = dev_menu.selected_tool == Some(tool);
                    let uv_rect = tileset.get_egui_uv(tool.tile_id());

                    // Create a button with the tile image
                    let button_size = egui::vec2(48.0, 48.0);
                    let (rect, response) = ui.allocate_exact_size(button_size, egui::Sense::click());

                    // Draw background (highlight if selected)
                    let bg_color = if is_selected {
                        egui::Color32::from_rgb(80, 120, 200)
                    } else if response.hovered() {
                        egui::Color32::from_rgb(60, 60, 80)
                    } else {
                        egui::Color32::from_rgb(40, 40, 50)
                    };
                    ui.painter().rect_filled(rect, 4.0, bg_color);

                    // Draw the tile image
                    let image_rect = rect.shrink(4.0);
                    ui.painter().image(
                        tileset_texture_id,
                        image_rect,
                        uv_rect,
                        egui::Color32::WHITE,
                    );

                    // Handle click
                    if response.clicked() {
                        if is_selected {
                            dev_menu.selected_tool = None;
                        } else {
                            dev_menu.selected_tool = Some(tool);
                        }
                    }

                    // Tooltip
                    response.on_hover_text(tool.name());
                }
            });

            ui.add_space(8.0);

            // Show current selection
            if let Some(tool) = dev_menu.selected_tool {
                ui.label(format!("Selected: {}", tool.name()));
                ui.label("Click on map to spawn");
            } else {
                ui.label("Select a tool above");
            }
        });
}

/// Render the status bar (health, XP, gold)
pub fn draw_status_bar(
    ctx: &egui::Context,
    data: &StatusBarData,
    tileset_texture_id: egui::TextureId,
    coins_uv: egui::Rect,
) {
    egui::Window::new("Status")
        .fixed_pos([10.0, 10.0])
        .fixed_size([200.0, 90.0])
        .title_bar(false)
        .show(ctx, |ui| {
            let health_percent = if data.health_max > 0 {
                data.health_current as f32 / data.health_max as f32
            } else {
                0.0
            };
            ui.add(
                egui::ProgressBar::new(health_percent)
                    .text(format!("HP: {}/{}", data.health_current, data.health_max)),
            );
            ui.add(
                egui::ProgressBar::new(data.xp_progress)
                    .fill(egui::Color32::from_rgb(100, 149, 237))
                    .text(format!(
                        "Lv {} - XP: {:.0}%",
                        data.xp_level,
                        data.xp_progress * 100.0
                    )),
            );
            ui.horizontal(|ui| {
                let coin_img = egui::Image::new(egui::load::SizedTexture::new(
                    tileset_texture_id,
                    egui::vec2(16.0, 16.0),
                ))
                .uv(coins_uv);
                ui.add(coin_img);
                ui.label(format!("{}", data.gold));
            });
        });
}

/// Render the loot window (chest/bones contents)
pub fn draw_loot_window(
    ctx: &egui::Context,
    data: &LootWindowData,
    tileset_texture_id: egui::TextureId,
    coins_uv: egui::Rect,
    potion_uv: egui::Rect,
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
        .show(ctx, |ui| {
            ui.heading("Contents");
            ui.separator();
            ui.add_space(10.0);

            let has_contents = !data.items.is_empty() || data.gold > 0;

            if !has_contents {
                ui.label(
                    egui::RichText::new("(empty)")
                        .italics()
                        .color(egui::Color32::GRAY),
                );
            } else {
                // Show gold if present
                if data.gold > 0 {
                    ui.horizontal(|ui| {
                        let coin_img = egui::Image::new(egui::load::SizedTexture::new(
                            tileset_texture_id,
                            egui::vec2(32.0, 32.0),
                        ))
                        .uv(coins_uv);

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
                        let uv = match item_type {
                            ItemType::HealthPotion => potion_uv,
                        };

                        let image = egui::Image::new(egui::load::SizedTexture::new(
                            tileset_texture_id,
                            egui::vec2(48.0, 48.0),
                        ))
                        .uv(uv);

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

/// Render the inventory/character window
pub fn draw_inventory_window(
    ctx: &egui::Context,
    world: &World,
    player_entity: hecs::Entity,
    data: &InventoryWindowData,
    tileset_texture_id: egui::TextureId,
    sword_uv: egui::Rect,
    bow_uv: egui::Rect,
    coins_uv: egui::Rect,
    potion_uv: egui::Rect,
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
        .show(ctx, |ui| {
            if let Ok(stats) = world.get::<&Stats>(player_entity) {
                ui.columns(2, |columns| {
                    // Left column: Stats + Equipment
                    draw_stats_column(&mut columns[0], world, player_entity, &stats, tileset_texture_id, sword_uv, bow_uv, coins_uv);

                    // Right column: Inventory
                    draw_inventory_column(&mut columns[1], world, player_entity, tileset_texture_id, potion_uv, actions);
                });
            }
        });
}

fn draw_stats_column(
    ui: &mut egui::Ui,
    world: &World,
    player_entity: hecs::Entity,
    stats: &Stats,
    tileset_texture_id: egui::TextureId,
    sword_uv: egui::Rect,
    bow_uv: egui::Rect,
    coins_uv: egui::Rect,
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
                let coin_img = egui::Image::new(egui::load::SizedTexture::new(
                    tileset_texture_id,
                    egui::vec2(24.0, 24.0),
                ))
                .uv(coins_uv);
                ui.add(coin_img);
                ui.label(format!("{} gold", inventory.gold));
            });
        }

        ui.add_space(20.0);
        ui.heading("EQUIPMENT");
        ui.separator();
        ui.add_space(10.0);

        if let Ok(equipment) = world.get::<&Equipment>(player_entity) {
            // Weapon slot (melee)
            ui.horizontal(|ui| {
                ui.label("Melee:");
                if let Some(weapon) = &equipment.weapon {
                    let image = egui::Image::new(egui::load::SizedTexture::new(
                        tileset_texture_id,
                        egui::vec2(48.0, 48.0),
                    ))
                    .uv(sword_uv);

                    ui.add(egui::ImageButton::new(image)).on_hover_text(format!(
                        "{}\n\nDamage: {} + {} = {}",
                        weapon.name,
                        weapon.base_damage,
                        weapon.damage_bonus,
                        systems::weapon_damage(weapon)
                    ));
                } else {
                    ui.label(
                        egui::RichText::new("(none)")
                            .italics()
                            .color(egui::Color32::GRAY),
                    );
                }
            });

            ui.add_space(5.0);

            // Ranged weapon slot
            ui.horizontal(|ui| {
                ui.label("Ranged:");
                if let Some(ranged) = &equipment.ranged_weapon {
                    let image = egui::Image::new(egui::load::SizedTexture::new(
                        tileset_texture_id,
                        egui::vec2(48.0, 48.0),
                    ))
                    .uv(bow_uv);

                    ui.add(egui::ImageButton::new(image)).on_hover_text(format!(
                        "{}\n\nDamage: {}\nSpeed: {:.0} tiles/sec\n\nRight-click to shoot",
                        ranged.name,
                        ranged.base_damage,
                        ranged.arrow_speed
                    ));
                } else {
                    ui.label(
                        egui::RichText::new("(none)")
                            .italics()
                            .color(egui::Color32::GRAY),
                    );
                }
            });
        }
    });
}

fn draw_inventory_column(
    ui: &mut egui::Ui,
    world: &World,
    player_entity: hecs::Entity,
    tileset_texture_id: egui::TextureId,
    potion_uv: egui::Rect,
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
                        .color(egui::Color32::GRAY),
                );
            } else {
                ui.horizontal_wrapped(|ui| {
                    for (i, item_type) in inventory.items.iter().enumerate() {
                        let uv = match item_type {
                            ItemType::HealthPotion => potion_uv,
                        };

                        let image = egui::Image::new(egui::load::SizedTexture::new(
                            tileset_texture_id,
                            egui::vec2(48.0, 48.0),
                        ))
                        .uv(uv);

                        let response = ui.add(egui::ImageButton::new(image));

                        if response
                            .on_hover_text(format!(
                                "{}\n\nClick to use",
                                systems::item_name(*item_type)
                            ))
                            .clicked()
                        {
                            actions.item_to_use = Some(i);
                        }
                    }
                });
            }
        }
    });
}

/// Helper struct containing pre-computed UV coordinates for UI icons
pub struct UiIcons {
    pub tileset_texture_id: egui::TextureId,
    pub sword_uv: egui::Rect,
    pub bow_uv: egui::Rect,
    pub potion_uv: egui::Rect,
    pub coins_uv: egui::Rect,
}

impl UiIcons {
    pub fn new(tileset: &Tileset, tileset_egui_id: egui::TextureId) -> Self {
        Self {
            tileset_texture_id: tileset_egui_id,
            sword_uv: tileset.get_egui_uv(tile_ids::SWORD),
            bow_uv: tileset.get_egui_uv(tile_ids::BOW),
            potion_uv: tileset.get_egui_uv(tile_ids::RED_POTION),
            coins_uv: tileset.get_egui_uv(tile_ids::COINS),
        }
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

    StatusBarData {
        health_current,
        health_max,
        xp_progress,
        xp_level,
        gold,
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

/// Render floating damage numbers
pub fn draw_damage_numbers(ctx: &egui::Context, effects: &[VisualEffect], camera: &Camera) {
    let painter = ctx.layer_painter(egui::LayerId::new(
        egui::Order::Foreground,
        egui::Id::new("damage_numbers"),
    ));

    // Get egui's pixels per point for HiDPI scaling
    let ppp = ctx.pixels_per_point();

    for effect in effects {
        let EffectType::DamageNumber { amount } = &effect.effect_type else {
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
        let font_id = egui::FontId::proportional(20.0);

        painter.text(
            egui::pos2(egui_x, egui_y),
            egui::Align2::CENTER_CENTER,
            text,
            font_id,
            color,
        );
    }
}
