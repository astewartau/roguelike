//! The developer tools panel, docked on the right (backquote toggles it),
//! and the ghost preview a cursor tool shows over the map.
//!
//! This reads the world and never changes it: choosing something arms a
//! `devtools::CursorTool` for the next map click, or queues a
//! `devtools::DevCommand` for the engine to carry out.

use crate::camera::Camera;
use crate::constants::{DEVTOOLS_GHOST_ALPHA, DEVTOOLS_PANEL_WIDTH};
use crate::devtools::paint::PAINTABLE;
use crate::devtools::spawn::{DoorKind, BOSSES, ENEMIES};
use crate::devtools::{CursorTool, DevCommand, DevTab, DevTools, Spawnable, Wakefulness};
use crate::components::{BlocksMovement, DungeonTrapKind, FurnitureKind, LanternStyle, Position, Rarity};
use crate::grid::Grid;
use crate::multi_tileset::MultiTileset;
use crate::tile::SpriteSheet;
use hecs::World;

use super::icons::UiIcons;
use super::style::{self, colors};
use super::vfx::{tile_rect, RainStyle};

/// What the panel needs to draw icons.
pub struct DevArt<'a> {
    pub icons: &'a UiIcons,
    pub tileset: &'a MultiTileset,
}

impl DevArt<'_> {
    fn icon(&self, sprite: (SpriteSheet, u32)) -> (egui::TextureId, egui::Rect) {
        (self.icons.texture_for_sheet(sprite.0), self.tileset.get_egui_uv(sprite.0, sprite.1))
    }
}

/// Draw the panel (if open) and record its width for the camera.
pub fn draw_devtools(ctx: &egui::Context, dev: &mut DevTools, art: &DevArt) {
    if !dev.visible {
        dev.panel_px = 0.0;
        return;
    }
    let screen = ctx.screen_rect();
    dev.panel_px = DEVTOOLS_PANEL_WIDTH * ctx.pixels_per_point();

    egui::Area::new(egui::Id::new("devtools_panel"))
        .order(egui::Order::Middle)
        .anchor(egui::Align2::RIGHT_TOP, egui::vec2(0.0, 0.0))
        .show(ctx, |ui| {
            egui::Frame::none()
                .fill(colors::PANEL_BG)
                .stroke(egui::Stroke::new(style::BORDER_WIDTH, colors::PANEL_BORDER))
                .inner_margin(egui::Margin::same(8.0))
                .show(ui, |ui| {
                    ui.set_width(DEVTOOLS_PANEL_WIDTH - 16.0);
                    ui.set_min_height(screen.height() - 16.0);
                    header(ui, dev);
                    ui.separator();
                    egui::ScrollArea::vertical()
                        .max_height(screen.height() - 120.0)
                        .auto_shrink([false, false])
                        .show(ui, |ui| match dev.tab {
                            DevTab::Spawn => spawn_tab(ui, dev, art),
                            DevTab::Paint => paint_tab(ui, dev, art),
                            DevTab::Effects => effects_tab(ui, dev),
                        });
                });
        });
}

fn header(ui: &mut egui::Ui, dev: &mut DevTools) {
    ui.horizontal(|ui| {
        ui.heading("Developer tools");
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui.button("✕").on_hover_text("Close (`)").clicked() {
                dev.toggle();
            }
        });
    });
    ui.horizontal_wrapped(|ui| {
        for tab in DevTab::ALL {
            ui.selectable_value(&mut dev.tab, tab, tab.name());
        }
    });
    match dev.tool {
        Some(tool) => {
            ui.horizontal(|ui| {
                ui.colored_label(colors::TEXT_PRIMARY, format!("Tool: {}", tool.describe()));
                if ui.small_button("Drop").on_hover_text("Or right-click the map, or Esc").clicked() {
                    dev.disarm();
                }
            });
        }
        None => {
            ui.colored_label(colors::TEXT_MUTED, "Pick something, then click the map.");
        }
    }
    if let Some(status) = &dev.status {
        ui.colored_label(colors::TEXT_MUTED, status);
    }
}

/// One selectable row: icon and name. Returns its response.
fn row(ui: &mut egui::Ui, art: &DevArt, sprite: (SpriteSheet, u32), label: &str, selected: bool) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(egui::vec2(ui.available_width(), 26.0), egui::Sense::click());
    let bg = if selected {
        colors::SELECTED
    } else if response.hovered() {
        colors::HOVERED
    } else {
        egui::Color32::TRANSPARENT
    };
    ui.painter().rect_filled(rect, 0.0, bg);
    let (tex, uv) = art.icon(sprite);
    let icon = egui::Rect::from_min_size(rect.min + egui::vec2(4.0, 1.0), egui::vec2(24.0, 24.0));
    ui.painter().image(tex, icon, uv, egui::Color32::WHITE);
    ui.painter().text(
        egui::pos2(icon.max.x + 8.0, rect.center().y),
        egui::Align2::LEFT_CENTER,
        label,
        egui::FontId::proportional(14.0),
        colors::TEXT_PRIMARY,
    );
    response
}

/// A list of spawnables: click one to arm the Place tool with it.
fn place_list(ui: &mut egui::Ui, dev: &mut DevTools, art: &DevArt, items: &[Spawnable]) {
    let search = dev.search.to_lowercase();
    for &what in items {
        let name = what.name();
        if !search.is_empty() && !name.to_lowercase().contains(&search) {
            continue;
        }
        let selected = dev.tool == Some(CursorTool::Place(what));
        if row(ui, art, what.sprite(), &name, selected).clicked() {
            dev.tool = if selected { None } else { Some(CursorTool::Place(what)) };
        }
    }
}

fn spawn_tab(ui: &mut egui::Ui, dev: &mut DevTools, art: &DevArt) {
    ui.horizontal(|ui| {
        ui.label("Search");
        ui.text_edit_singleline(&mut dev.search);
    });
    ui.add_space(4.0);

    egui::CollapsingHeader::new("Creatures").default_open(true).show(ui, |ui| {
        let options = &mut dev.spawn_options;
        ui.horizontal(|ui| {
            for (wake, label) in
                [(Wakefulness::Awake, "Awake"), (Wakefulness::Asleep, "Asleep"), (Wakefulness::AsRolled, "As rolled")]
            {
                ui.selectable_value(&mut options.wake, wake, label);
            }
        });
        ui.horizontal(|ui| {
            ui.checkbox(&mut options.companion, "Tamed companion");
            ui.label("Count");
            ui.add(egui::DragValue::new(&mut options.count).range(1..=30));
        });
        let creatures: Vec<Spawnable> = (0..ENEMIES.len())
            .map(Spawnable::Enemy)
            .chain(BOSSES.iter().map(|(a, _)| Spawnable::Boss(*a)))
            .chain([Spawnable::Wizard, Spawnable::Merchant])
            .collect();
        place_list(ui, dev, art, &creatures);
    });

    egui::CollapsingHeader::new("Items").default_open(false).show(ui, |ui| {
        ui.horizontal(|ui| {
            ui.label("Gear rarity");
            for (rarity, label) in [
                (Rarity::Common, "Common"),
                (Rarity::Magic, "Magic"),
                (Rarity::Rare, "Rare"),
                (Rarity::Legendary, "Legendary"),
            ] {
                ui.selectable_value(&mut dev.spawn_options.rarity, rarity, label);
            }
        });
        ui.label(egui::RichText::new("Click to drop on the map; Give puts it in your pack.").color(colors::TEXT_MUTED));
        let search = dev.search.to_lowercase();
        for def in crate::systems::item_defs::ITEM_DEFS {
            if !search.is_empty() && !def.name.to_lowercase().contains(&search) {
                continue;
            }
            let what = Spawnable::Item(def.item_type);
            let selected = dev.tool == Some(CursorTool::Place(what));
            ui.horizontal(|ui| {
                if ui.small_button("Give").clicked() {
                    dev.commands.push(DevCommand::GiveItem { kind: def.item_type, count: dev.spawn_options.count });
                }
                if row(ui, art, def.sprite, def.name, selected).clicked() {
                    dev.tool = if selected { None } else { Some(CursorTool::Place(what)) };
                }
            });
        }
    });

    egui::CollapsingHeader::new("Fire and light").default_open(false).show(ui, |ui| {
        place_list(
            ui,
            dev,
            art,
            &[
                Spawnable::Campfire,
                Spawnable::Brazier,
                Spawnable::Lantern(LanternStyle::Warm),
                Spawnable::Lantern(LanternStyle::Ghost),
                Spawnable::FirePatch,
                Spawnable::DroppedTorch,
                Spawnable::OilBarrel,
                Spawnable::OilPuddle,
                Spawnable::Web,
                Spawnable::RainCloud,
            ],
        );
    });

    egui::CollapsingHeader::new("Furniture and traps").default_open(false).show(ui, |ui| {
        ui.checkbox(&mut dev.spawn_options.revealed, "Traps start revealed");
        place_list(
            ui,
            dev,
            art,
            &[
                Spawnable::Furniture(FurnitureKind::Fountain),
                Spawnable::Furniture(FurnitureKind::Altar),
                Spawnable::Furniture(FurnitureKind::Shrine),
                Spawnable::Trap(DungeonTrapKind::Spike),
                Spawnable::Trap(DungeonTrapKind::Fire),
                Spawnable::Trap(DungeonTrapKind::Snare),
                Spawnable::Trap(DungeonTrapKind::Alarm),
            ],
        );
    });

    egui::CollapsingHeader::new("Doors, chests and scenery").default_open(false).show(ui, |ui| {
        place_list(
            ui,
            dev,
            art,
            &[
                Spawnable::Chest,
                Spawnable::Door(DoorKind::Wooden),
                Spawnable::Door(DoorKind::Green),
                Spawnable::Door(DoorKind::Grated),
                Spawnable::SecretDoor,
                Spawnable::Stalagmites,
                Spawnable::GlowMushrooms,
                Spawnable::CrystalCluster,
            ],
        );
    });
}

fn paint_tab(ui: &mut egui::Ui, dev: &mut DevTools, art: &DevArt) {
    ui.horizontal(|ui| {
        ui.label("Brush");
        for (radius, label) in [(0, "1x1"), (1, "3x3"), (2, "5x5")] {
            ui.selectable_value(&mut dev.brush, radius, label);
        }
    });
    ui.label(egui::RichText::new("Click or drag on the map to paint.").color(colors::TEXT_MUTED));
    for tile in PAINTABLE {
        let selected = dev.tool == Some(CursorTool::Paint(tile));
        if row(ui, art, tile.sprite_ref(), &format!("{tile:?}"), selected).clicked() {
            dev.tool = if selected { None } else { Some(CursorTool::Paint(tile)) };
        }
    }
}

fn effects_tab(ui: &mut egui::Ui, dev: &mut DevTools) {
    style::panel_header(ui, "Call Rain look");
    ui.horizontal(|ui| {
        for rain in RainStyle::ALL {
            ui.selectable_value(&mut dev.rain_style, rain, rain.name());
        }
    });
    ui.add_space(8.0);
    style::panel_header(ui, "Particle fire");
    ui.horizontal(|ui| {
        let armed = dev.tool == Some(CursorTool::ParticleFire);
        if ui.selectable_label(armed, "Place on the map").clicked() {
            dev.tool = if armed { None } else { Some(CursorTool::ParticleFire) };
        }
        if ui.button("Clear all").clicked() {
            dev.commands.push(DevCommand::ClearParticleFires);
        }
    });
    ui.label(egui::RichText::new("A looping fire drawn by the GPU: visual only, nothing burns.").color(colors::TEXT_MUTED));
}

/// The armed tool's preview at the hovered tile: what would be placed (red
/// if it can't go there), or the paint brush's square.
pub fn draw_cursor_ghost(
    ctx: &egui::Context,
    dev: &DevTools,
    camera: &Camera,
    art: &DevArt,
    world: &World,
    grid: &Grid,
    hover: (i32, i32),
) {
    let Some(tool) = dev.tool.filter(|_| dev.visible) else {
        return;
    };
    let painter = ctx.layer_painter(egui::LayerId::new(egui::Order::Background, egui::Id::new("devtools_ghost")));
    let ppp = ctx.pixels_per_point();
    let ghost = egui::Color32::from_white_alpha(DEVTOOLS_GHOST_ALPHA);
    match tool {
        CursorTool::Place(what) => {
            let rect = tile_rect(camera, ppp, hover.0 as f32 + 0.5, hover.1 as f32 + 0.5);
            let blocked = what.needs_clear_tile()
                && world
                    .query::<&Position>()
                    .with::<&BlocksMovement>()
                    .iter()
                    .any(|(_, p)| (p.x, p.y) == hover);
            let ok = grid.is_walkable(hover.0, hover.1) && !blocked;
            let (tex, uv) = art.icon(what.sprite());
            painter.image(tex, rect, uv, ghost);
            let edge = if ok { egui::Color32::from_rgb(120, 220, 140) } else { egui::Color32::from_rgb(230, 90, 80) };
            painter.rect_stroke(rect, 2.0, egui::Stroke::new(1.5_f32, edge));
        }
        CursorTool::ParticleFire => {
            let rect = tile_rect(camera, ppp, hover.0 as f32 + 0.5, hover.1 as f32 + 0.5);
            painter.rect_stroke(rect, 2.0, egui::Stroke::new(1.5_f32, egui::Color32::from_rgb(255, 170, 80)));
        }
        CursorTool::Paint(tile) => {
            let b = dev.brush as f32;
            let low = tile_rect(camera, ppp, hover.0 as f32 - b + 0.5, hover.1 as f32 - b + 0.5);
            let high = tile_rect(camera, ppp, hover.0 as f32 + b + 0.5, hover.1 as f32 + b + 0.5);
            let rect = low.union(high);
            let (tex, uv) = art.icon(tile.sprite_ref());
            for dy in -dev.brush..=dev.brush {
                for dx in -dev.brush..=dev.brush {
                    let cell = tile_rect(camera, ppp, (hover.0 + dx) as f32 + 0.5, (hover.1 + dy) as f32 + 0.5);
                    painter.image(tex, cell, uv, ghost);
                }
            }
            painter.rect_stroke(rect, 2.0, egui::Stroke::new(1.5_f32, egui::Color32::from_rgb(240, 220, 140)));
        }
    }
}
