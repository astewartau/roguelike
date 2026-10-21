//! The developer tools panel, docked on the right (backquote toggles it),
//! and the ghost preview a cursor tool shows over the map.
//!
//! This reads the world and never changes it: choosing something arms a
//! `devtools::CursorTool` for the next map click, or queues a
//! `devtools::DevCommand` for the engine to carry out.

use crate::camera::Camera;
use crate::constants::{
    CAMERA_SHAKE_BOSS_ABILITY, CAMERA_SHAKE_EXPLOSION, CAMERA_SHAKE_MAX_AMPLITUDE, CAMERA_SHAKE_PLAYER_SWING,
    DEVTOOLS_GHOST_ALPHA, DEVTOOLS_PANEL_WIDTH,
};
use crate::devtools::paint::PAINTABLE;
use crate::devtools::spawn::{DoorKind, BOSSES, ENEMIES};
use crate::devtools::{CursorTool, DevCommand, DevTab, DevTools, Spawnable, Wakefulness};
use crate::components::{BlocksMovement, FurnitureKind, TrapEffect, LanternStyle, Position, Rarity};
use crate::grid::Grid;
use crate::multi_tileset::MultiTileset;
use crate::tile::SpriteSheet;
use hecs::World;

use super::icons::UiIcons;
use super::devtools_look as look;
use super::vfx::{tile_rect, RainStyle};

/// What the panel needs to draw icons.
pub struct DevArt<'a> {
    pub icons: &'a UiIcons,
    pub tileset: &'a MultiTileset,
}

impl DevArt<'_> {
    pub(super) fn icon(&self, sprite: (SpriteSheet, u32)) -> (egui::TextureId, egui::Rect) {
        (self.icons.texture_for_sheet(sprite.0), self.tileset.get_egui_uv(sprite.0, sprite.1))
    }
}

/// The run as the panel sees it (read only).
pub struct DevView<'a> {
    pub world: &'a World,
    pub grid: &'a Grid,
    pub player: hecs::Entity,
    pub game_time: f32,
}

/// Draw the panel (if open) and record its width for the camera.
pub fn draw_devtools(ctx: &egui::Context, dev: &mut DevTools, art: &DevArt, view: &DevView) {
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
            look::apply(ui);
            let frame = look::panel_frame();
            let chrome = frame.total_margin().sum();
            frame.show(ui, |ui| {
                    ui.set_width(DEVTOOLS_PANEL_WIDTH - chrome.x);
                    ui.set_min_height(screen.height() - chrome.y);
                    let top = ui.cursor().top();
                    header(ui, dev, view);
                    ui.add_space(4.0);
                    let used = ui.cursor().top() - top;
                    egui::ScrollArea::vertical()
                        .max_height(screen.height() - chrome.y - used)
                        .auto_shrink([false, false])
                        .show(ui, |ui| match dev.tab {
                            DevTab::Spawn => spawn_tab(ui, dev, art),
                            DevTab::Paint => paint_tab(ui, dev, art),
                            DevTab::Player => player_tab(ui, dev, view),
                            DevTab::World => world_tab(ui, dev, view),
                            DevTab::Inspect => inspect_tab(ui, dev, view, art),
                            DevTab::Prefabs => super::devtools_prefab::prefabs_tab(ui, dev, art),
                            DevTab::Overlays => overlays_tab(ui, dev),
                            DevTab::Effects => effects_tab(ui, dev),
                        });
                });
        });
}

fn header(ui: &mut egui::Ui, dev: &mut DevTools, view: &DevView) {
    let p = look::pal(ui);
    ui.horizontal(|ui| {
        ui.vertical(|ui| {
            ui.heading("Developer tools");
            ui.label(
                egui::RichText::new(format!("Floor {} · seed {} · {:.0} fps", dev.floor, dev.seed, dev.fps))
                    .small()
                    .color(p.muted),
            );
        });
        ui.with_layout(egui::Layout::right_to_left(egui::Align::TOP), |ui| {
            if look::cross_button(ui).on_hover_text("Close (`)").clicked() {
                dev.toggle();
            }
        });
    });
    let _ = view;
    ui.add_space(4.0);
    let tabs: Vec<(DevTab, &str, &str)> = DevTab::ALL.iter().map(|&t| (t, t.icon(), t.name())).collect();
    if let Some(tab) = look::pill_tabs(ui, dev.tab, &tabs) {
        dev.tab = tab;
    }
    ui.add_space(4.0);
    look::card(ui, |ui| {
        match dev.tool {
            Some(tool) => {
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new("●").color(p.accent));
                    ui.label(tool.describe());
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.small_button("Drop").on_hover_text("Or right-click the map, or Esc").clicked() {
                            dev.disarm();
                        }
                    });
                });
            }
            None => {
                ui.colored_label(p.muted, "Pick something, then click the map.");
            }
        }
        if let Some(status) = &dev.status {
            ui.label(egui::RichText::new(status).small().color(p.muted));
        }
    });
}

/// One selectable row: icon and name. Returns its response.
pub(super) fn row(ui: &mut egui::Ui, art: &DevArt, sprite: (SpriteSheet, u32), label: &str, selected: bool) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(egui::vec2(ui.available_width(), 26.0), egui::Sense::click());
    let p = look::pal(ui);
    let bg = if selected {
        p.accent
    } else if response.hovered() {
        p.hover
    } else {
        egui::Color32::TRANSPARENT
    };
    ui.painter().rect_filled(rect, 6.0, bg);
    let (tex, uv) = art.icon(sprite);
    let icon = egui::Rect::from_min_size(rect.min + egui::vec2(4.0, 1.0), egui::vec2(24.0, 24.0));
    ui.painter().image(tex, icon, uv, egui::Color32::WHITE);
    ui.painter().text(
        egui::pos2(icon.max.x + 8.0, rect.center().y),
        egui::Align2::LEFT_CENTER,
        label,
        egui::TextStyle::Body.resolve(ui.style()),
        if selected { p.on_accent } else { p.text },
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
            look::toggle(ui, &mut options.companion, "Tamed companion");
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
        ui.label(egui::RichText::new("Click to drop on the map; Give puts it in your pack.").color(look::pal(ui).muted));
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
        look::toggle(ui, &mut dev.spawn_options.revealed, "Traps start revealed");
        place_list(
            ui,
            dev,
            art,
            &[
                Spawnable::Furniture(FurnitureKind::Fountain),
                Spawnable::Furniture(FurnitureKind::Altar),
                Spawnable::Furniture(FurnitureKind::Shrine),
                Spawnable::Trap(TrapEffect::spike()),
                Spawnable::Trap(TrapEffect::fire()),
                Spawnable::Trap(TrapEffect::snare()),
                Spawnable::Trap(TrapEffect::alarm()),
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
    ui.label(egui::RichText::new("Click or drag on the map to paint.").color(look::pal(ui).muted));
    for tile in PAINTABLE {
        let selected = dev.tool == Some(CursorTool::Paint(tile));
        if row(ui, art, tile.sprite_ref(), &format!("{tile:?}"), selected).clicked() {
            dev.tool = if selected { None } else { Some(CursorTool::Paint(tile)) };
        }
    }
}

/// A tool button: armed while selected; clicking toggles it.
fn tool_button(ui: &mut egui::Ui, dev: &mut DevTools, tool: CursorTool, label: &str, tip: &str) {
    let armed = dev.tool == Some(tool);
    if ui.selectable_label(armed, label).on_hover_text(tip).clicked() {
        dev.tool = if armed { None } else { Some(tool) };
    }
}

fn player_tab(ui: &mut egui::Ui, dev: &mut DevTools, view: &DevView) {
    use crate::components::{Experience, Fatigue, Health, Hunger, Inventory, LearnedAbilities, Stats, StatusEffects};
    use crate::devtools::cheats::{ALL_EFFECTS, LEARNABLE};
    let (world, player) = (view.world, view.player);

    ui.horizontal(|ui| {
        look::toggle(ui, &mut dev.god_mode, "God mode");
        look::toggle(ui, &mut dev.keep_fed, "Never hungry or tired");
    });

    look::section(ui, "Health");
    if let Ok(health) = world.get::<&Health>(player) {
        let (mut current, mut max) = (health.current, health.max);
        ui.horizontal(|ui| {
            let a = ui.add(egui::DragValue::new(&mut current).range(0..=9999).prefix("HP "));
            let b = ui.add(egui::DragValue::new(&mut max).range(1..=9999).prefix("max "));
            if a.changed() || b.changed() {
                dev.commands.push(DevCommand::SetHealth { current, max });
            }
            if ui.button("Full heal").clicked() {
                dev.commands.push(DevCommand::FullHeal);
            }
        });
    }

    look::section(ui, "Stats");
    if let Ok(stats) = world.get::<&Stats>(player) {
        let (mut strength, mut intelligence, mut agility) = (stats.strength, stats.intelligence, stats.agility);
        ui.horizontal(|ui| {
            let a = ui.add(egui::DragValue::new(&mut strength).range(1..=99).prefix("STR "));
            let b = ui.add(egui::DragValue::new(&mut intelligence).range(1..=99).prefix("INT "));
            let c = ui.add(egui::DragValue::new(&mut agility).range(1..=99).prefix("AGI "));
            if a.changed() || b.changed() || c.changed() {
                dev.commands.push(DevCommand::SetStats { strength, intelligence, agility });
            }
        });
    }

    look::section(ui, "Experience and gold");
    let level = world.get::<&Experience>(player).map(|e| (e.level, e.current)).ok();
    let gold = world.get::<&Inventory>(player).map(|i| i.gold).unwrap_or(0);
    if let Some((level, xp)) = level {
        ui.label(format!("Level {level}, {xp} XP, {gold} gold"));
    }
    ui.horizontal(|ui| {
        for amount in [100, 1000] {
            if ui.button(format!("+{amount} XP")).clicked() {
                dev.commands.push(DevCommand::AddXp(amount));
            }
        }
        for amount in [100, 1000] {
            if ui.button(format!("+{amount} gold")).clicked() {
                dev.commands.push(DevCommand::AddGold(amount));
            }
        }
    });

    look::section(ui, "Hunger and fatigue");
    let hunger = world.get::<&Hunger>(player).map(|h| h.value).ok();
    let fatigue = world.get::<&Fatigue>(player).map(|f| f.value).ok();
    if let (Some(mut hunger), Some(mut fatigue)) = (hunger, fatigue) {
        let a = ui.add(egui::Slider::new(&mut hunger, 0.0..=crate::constants::HUNGER_MAX).text("fullness"));
        let b = ui.add(egui::Slider::new(&mut fatigue, 0.0..=crate::constants::FATIGUE_MAX).text("fatigue"));
        if a.changed() || b.changed() {
            dev.commands.push(DevCommand::SetMeters { hunger, fatigue });
        }
    }

    look::section(ui, "Spells and cooldowns");
    let known = world.get::<&LearnedAbilities>(player).ok();
    ui.horizontal_wrapped(|ui| {
        for ability in LEARNABLE {
            let has = known.as_ref().is_some_and(|k| k.knows(ability));
            if ui.add_enabled(!has, egui::Button::new(ability.name())).clicked() {
                dev.commands.push(DevCommand::Learn(ability));
            }
        }
    });
    drop(known);
    if ui.button("Reset all cooldowns").clicked() {
        dev.commands.push(DevCommand::ResetCooldowns);
    }

    look::section(ui, "Status effects");
    ui.horizontal(|ui| {
        ui.label("Add for");
        ui.add(egui::DragValue::new(&mut dev.effect_seconds).range(1.0..=600.0).suffix(" s"));
        if ui.button("Clear all").clicked() {
            dev.commands.push(DevCommand::ClearEffects);
        }
    });
    let active: Vec<(crate::components::EffectType, f32)> = world
        .get::<&StatusEffects>(player)
        .map(|s| s.effects.iter().map(|e| (e.effect_type, e.remaining_duration)).collect())
        .unwrap_or_default();
    ui.horizontal_wrapped(|ui| {
        for effect in ALL_EFFECTS {
            let on = active.iter().find(|(e, _)| *e == effect);
            let label = match on {
                Some((_, left)) if *left > 1.0e5 => format!("{effect:?} (on)"),
                Some((_, left)) => format!("{effect:?} {left:.0}s"),
                None => format!("{effect:?}"),
            };
            if ui.selectable_label(on.is_some(), label).on_hover_text("Click to add, again to remove").clicked() {
                dev.commands.push(if on.is_some() {
                    DevCommand::RemoveEffect(effect)
                } else {
                    DevCommand::AddEffect(effect, dev.effect_seconds)
                });
            }
        }
    });
}

fn world_tab(ui: &mut egui::Ui, dev: &mut DevTools, view: &DevView) {
    look::section(ui, "Click tools");
    ui.horizontal_wrapped(|ui| {
        tool_button(ui, dev, CursorTool::Teleport, "Teleport", "Move yourself to the clicked tile");
        tool_button(ui, dev, CursorTool::Ignite, "Ignite", "Set the tile alight, and whoever is on it");
        tool_button(ui, dev, CursorTool::Douse, "Douse", "Splash water over the tile and its neighbours");
        tool_button(ui, dev, CursorTool::Blast, "Blast", "A fireball's blast there (it can hurt you)");
        tool_button(ui, dev, CursorTool::Kill, "Kill", "Whatever has health there drops to none");
        tool_button(ui, dev, CursorTool::Delete, "Delete", "Remove everything on the tile but you");
    });

    look::section(ui, "Map");
    ui.horizontal(|ui| {
        if ui.button("Reveal map").clicked() {
            dev.commands.push(DevCommand::RevealMap);
        }
        look::toggle(ui, &mut dev.no_fog, "See everything");
    });

    look::section(ui, "Creatures");
    ui.horizontal_wrapped(|ui| {
        if ui.button("Kill visible").clicked() {
            dev.commands.push(DevCommand::KillHostiles { visible_only: true });
        }
        if ui.button("Kill all hostiles").clicked() {
            dev.commands.push(DevCommand::KillHostiles { visible_only: false });
        }
        let mut frozen = dev.ai_frozen;
        if look::toggle(ui, &mut frozen, "Freeze AI").changed() {
            dev.commands.push(DevCommand::FreezeAi(frozen));
        }
        if ui.button("Wake all").clicked() {
            dev.commands.push(DevCommand::WakeAll);
        }
    });

    look::section(ui, "Floors");
    ui.horizontal(|ui| {
        ui.label(format!("On floor {}.", dev.floor));
        ui.add(egui::DragValue::new(&mut dev.target_floor).range(0..=99).prefix("floor "));
        if ui.button("Go").clicked() {
            dev.commands.push(DevCommand::GoToFloor(dev.target_floor));
        }
        if ui.button("Reroll this floor").on_hover_text("Regenerate it from a fresh seed").clicked() {
            dev.commands.push(DevCommand::RerollFloor);
        }
    });

    look::section(ui, "Time");
    ui.horizontal(|ui| {
        ui.add(egui::DragValue::new(&mut dev.advance_seconds).range(0.5..=600.0).suffix(" s"));
        if ui.button("Let it pass").on_hover_text("You wait while game time runs on").clicked() {
            dev.commands.push(DevCommand::AdvanceTime(dev.advance_seconds));
        }
    });
    ui.add(egui::Slider::new(&mut dev.anim_speed, 0.05..=2.0).text("animation speed"));

    look::section(ui, "Run");
    ui.horizontal(|ui| {
        ui.label(format!("Seed {}", dev.seed));
        if ui.small_button("Copy").clicked() {
            ui.ctx().copy_text(dev.seed.to_string());
        }
    });
    ui.label(format!(
        "Game time {:.1} s, {} entities, {:.0} fps",
        view.game_time,
        view.world.len(),
        dev.fps
    ));
}

/// A short name for an entity: its `Name`, else what it's made of.
fn entity_label(world: &World, e: hecs::Entity) -> String {
    if let Ok(name) = world.get::<&crate::components::Name>(e) {
        return name.0.clone();
    }
    let (known, _) = crate::component_registry::components_of(world, e);
    let parts: Vec<&str> = known.iter().map(|c| c.name).filter(|n| *n != "Position").take(2).collect();
    if parts.is_empty() { format!("#{}", e.id()) } else { parts.join(" + ") }
}

fn inspect_tab(ui: &mut egui::Ui, dev: &mut DevTools, view: &DevView, art: &DevArt) {
    use crate::component_registry as registry;
    let world = view.world;
    ui.horizontal(|ui| {
        tool_button(ui, dev, CursorTool::Inspect, "Pick on the map", "Click a tile to look at what's there");
        if ui.button("Me").clicked() {
            let at = world.get::<&Position>(view.player).map(|p| (p.x, p.y)).unwrap_or((0, 0));
            dev.select(at, Some(view.player));
        }
    });
    let Some(tile) = dev.inspect_tile else {
        ui.colored_label(look::pal(ui).muted, "Nothing picked yet.");
        return;
    };

    look::section(ui, &format!("Tile {}, {}", tile.0, tile.1));
    if let Some(cell) = view.grid.get(tile.0, tile.1) {
        let light = view
            .grid
            .illumination
            .get(tile.1 as usize * view.grid.width + tile.0 as usize)
            .copied()
            .unwrap_or(0.0);
        ui.label(format!(
            "{:?}{}{}, light {:.2}",
            cell.tile_type,
            if cell.visible { ", in sight" } else { "" },
            if cell.explored { ", explored" } else { ", unexplored" },
            light
        ));
    }
    let here: Vec<hecs::Entity> = world
        .query::<&Position>()
        .iter()
        .filter(|(_, p)| (p.x, p.y) == tile)
        .map(|(id, _)| id)
        .collect();
    for e in &here {
        let sprite = world.get::<&crate::components::Sprite>(*e).map(|s| (s.sheet, s.tile_id)).ok();
        let label = entity_label(world, *e);
        let selected = dev.inspected == Some(*e);
        let clicked = match sprite {
            Some(sprite) => row(ui, art, sprite, &label, selected).clicked(),
            None => ui.selectable_label(selected, &label).clicked(),
        };
        if clicked {
            dev.select(tile, Some(*e));
        }
    }

    let Some(e) = dev.inspected.filter(|e| world.contains(*e)) else {
        return;
    };
    ui.separator();
    ui.horizontal(|ui| {
        ui.label(look::strong(entity_label(world, e)));
        ui.colored_label(look::pal(ui).muted, format!("#{}", e.id()));
    });
    ui.horizontal_wrapped(|ui| {
        if e != view.player {
            if ui.button("Delete").clicked() {
                dev.commands.push(DevCommand::DeleteEntity(e));
            }
            if ui.button("Kill").clicked() {
                dev.commands.push(DevCommand::KillEntity(e));
            }
            if ui.button("Save as prefab").clicked() {
                let name = entity_label(world, e);
                dev.prefabs.push(crate::devtools::prefab::capture(world, e, &name));
                dev.selected_prefab = Some(dev.prefabs.len() - 1);
                dev.prefabs_dirty = true;
                dev.tab = DevTab::Prefabs;
            }
            // Placed from a prefab and tweaked here: write the tweaks back.
            let origin = dev.placed_from.get(&e).and_then(|name| dev.prefabs.iter().position(|p| &p.name == name));
            if let Some(index) = origin {
                let label = format!("Update “{}” from this", dev.prefabs[index].name);
                if ui.button(label).on_hover_text("Replace the prefab's components with this one's").clicked() {
                    let name = dev.prefabs[index].name.clone();
                    dev.prefabs[index] = crate::devtools::prefab::capture(world, e, &name);
                    dev.selected_prefab = Some(index);
                    dev.prefabs_dirty = true;
                }
            }
            if ui.button("Teleport me beside it").clicked() {
                if let Some(at) = world.get::<&Position>(e).ok().map(|p| (p.x, p.y)) {
                    dev.commands.push(DevCommand::TeleportPlayerTo((at.0 + 1, at.1)));
                }
            }
        }
    });

    let (known, unknown) = registry::components_of(world, e);
    for entry in &known {
        let current = (entry.read)(world, e).unwrap_or(serde_json::Value::Null);
        let (mut value, error) = match dev.drafts.get(entry.name) {
            Some((draft, why)) => (draft.clone(), Some(why.clone())),
            None => (current, None),
        };
        egui::CollapsingHeader::new(entry.name).id_salt(("component", entry.name)).default_open(false).show(ui, |ui| {
            if edit_value(ui, entry.name, "", &mut value) {
                dev.commands.push(DevCommand::SetComponent { entity: e, name: entry.name, value });
            }
            if let Some(why) = error {
                ui.colored_label(egui::Color32::from_rgb(230, 110, 100), why);
            }
            if ui.small_button("Remove").clicked() {
                dev.commands.push(DevCommand::RemoveComponent { entity: e, name: entry.name });
            }
        });
    }
    if unknown > 0 {
        ui.colored_label(look::pal(ui).muted, format!("and {unknown} more the tools can't edit"));
    }
    ui.add_space(4.0);
    let label = if dev.inspect_picker_open { "Done adding" } else { "+  Add to it" };
    if look::primary_button(ui, label).clicked() {
        dev.inspect_picker_open = !dev.inspect_picker_open;
    }
    if dev.inspect_picker_open {
        let have: Vec<&str> = known.iter().map(|k| k.name).collect();
        for name in super::devtools_prefab::picker(ui, dev, &|n| have.contains(&n)) {
            if let Some(entry) = registry::find(name) {
                dev.commands.push(DevCommand::AddComponent { entity: e, name: entry.name });
            }
        }
    }
}

/// The variants a string field can take, where it holds an enum: by the
/// component it's in and the field's name.
fn enum_options(component: &str, key: &str) -> Option<Vec<String>> {
    let list = |names: &[&str]| Some(names.iter().map(|n| n.to_string()).collect());
    match (component, key) {
        (_, "sheet") => list(&["Tiles", "Rogues", "Monsters", "Items", "AnimatedTiles"]),
        ("ChaseAI", "state") => list(&["Unaware", "Idle", "Chasing", "Investigating"]),
        (_, "style") => list(&["Warm", "Ghost"]),
        (_, "ignition") => list(&["InPlace", "Replace"]),
        (_, "rarity") => list(&["Common", "Magic", "Rare", "Legendary"]),
        ("Furniture", "kind") => list(&["Fountain", "Altar", "Shrine"]),
        ("Container", "kind") => Some(
            crate::systems::item_defs::ITEM_DEFS.iter().map(|d| format!("{:?}", d.item_type)).collect(),
        ),
        (_, "effect_type") => {
            Some(crate::devtools::cheats::ALL_EFFECTS.iter().map(|e| format!("{e:?}")).collect())
        }
        _ => None,
    }
}

/// Edit a JSON value in place: numbers, switches, text (a dropdown for an
/// enum), and nested objects and lists. Returns whether it changed.
/// The three channels of a colour, if `value` is one: a `color` tuple
/// (serialised as `[r, g, b]`) or an `{r, g, b}` object like `SpriteTint`.
fn as_rgb(key: &str, value: &serde_json::Value) -> Option<[f32; 3]> {
    use serde_json::Value;
    let num = |v: Option<&Value>| v.and_then(Value::as_f64).map(|f| f as f32);
    match value {
        Value::Array(items) if key.contains("color") && items.len() == 3 => {
            Some([num(items.first())?, num(items.get(1))?, num(items.get(2))?])
        }
        Value::Object(fields) if fields.len() == 3 => {
            Some([num(fields.get("r"))?, num(fields.get("g"))?, num(fields.get("b"))?])
        }
        _ => None,
    }
}

/// A colour picker for a colour value, written back in the shape it came in.
fn edit_rgb(ui: &mut egui::Ui, value: &mut serde_json::Value, mut rgb: [f32; 3]) -> bool {
    use serde_json::Value;
    let mut shown = rgb.map(|c| c.clamp(0.0, 1.0));
    let mut changed = egui::color_picker::color_edit_button_rgb(ui, &mut shown).changed();
    if changed {
        rgb = shown;
    }
    for c in &mut rgb {
        changed |= ui.add(egui::DragValue::new(c).speed(0.01).range(0.0..=4.0)).changed();
    }
    if changed {
        let n = |c: f32| serde_json::Number::from_f64(c as f64).map(Value::Number).unwrap_or(Value::Null);
        match value {
            Value::Array(items) => *items = rgb.iter().map(|&c| n(c)).collect(),
            Value::Object(fields) => {
                for (k, c) in ["r", "g", "b"].iter().zip(rgb) {
                    fields.insert(k.to_string(), n(c));
                }
            }
            _ => {}
        }
    }
    changed
}

/// How wide a number box is in the settings editor, so a column of them
/// lines up.
const NUMBER_WIDTH: f32 = 72.0;

pub(super) fn edit_value(ui: &mut egui::Ui, component: &str, key: &str, value: &mut serde_json::Value) -> bool {
    use serde_json::Value;
    if let Some(rgb) = as_rgb(key, value) {
        return ui.horizontal(|ui| edit_rgb(ui, value, rgb)).inner;
    }
    match value {
        Value::Null => {
            ui.colored_label(look::pal(ui).muted, "(a marker: no settings)");
            false
        }
        Value::Bool(b) => look::toggle(ui, b, "").changed(),
        Value::Number(n) => {
            if let Some(mut i) = n.as_i64() {
                let changed = ui.add_sized([NUMBER_WIDTH, 22.0], egui::DragValue::new(&mut i)).changed();
                if changed {
                    *value = Value::from(i);
                }
                changed
            } else {
                let mut f = n.as_f64().unwrap_or(0.0);
                let changed = ui.add_sized([NUMBER_WIDTH, 22.0], egui::DragValue::new(&mut f).speed(0.05)).changed();
                if changed {
                    if let Some(num) = serde_json::Number::from_f64(f) {
                        *value = Value::Number(num);
                    }
                }
                changed
            }
        }
        Value::String(text) => match enum_options(component, key).filter(|opts| opts.contains(text)) {
            Some(options) => {
                let mut changed = false;
                egui::ComboBox::from_id_salt(("enum", component, key)).selected_text(text.as_str()).show_ui(ui, |ui| {
                    look::apply(ui);
                    for option in options {
                        if ui.selectable_label(*text == option, &option).clicked() && *text != option {
                            *text = option;
                            changed = true;
                        }
                    }
                });
                changed
            }
            None => ui.text_edit_singleline(text).changed(),
        },
        Value::Array(items) => {
            let mut changed = false;
            let mut remove = None;
            for (i, item) in items.iter_mut().enumerate() {
                ui.push_id(i, |ui| {
                    ui.horizontal_top(|ui| {
                        if look::cross_button(ui).clicked() {
                            remove = Some(i);
                        }
                        ui.vertical(|ui| changed |= edit_value(ui, component, key, item));
                    });
                });
            }
            if let Some(i) = remove {
                items.remove(i);
                changed = true;
            }
            if items.is_empty() {
                ui.colored_label(look::pal(ui).muted, "(empty)");
            }
            changed
        }
        Value::Object(fields) => {
            use crate::devtools::field_docs;
            let mut changed = false;
            let is_scalar = |name: &str, field: &Value| {
                !matches!(field, Value::Object(_) | Value::Array(_)) || as_rgb(name, field).is_some()
            };
            // Plain settings in a two-column grid, so the inputs line up:
            // the setting's name (with a ? explaining it, where the code
            // documents it), then its value.
            egui::Grid::new(ui.id().with(("fields", component, key)))
                .num_columns(2)
                .spacing(egui::vec2(12.0, 6.0))
                .show(ui, |ui| {
                    for (name, field) in fields.iter_mut() {
                        if !is_scalar(name, field) {
                            continue;
                        }
                        ui.horizontal(|ui| {
                            ui.spacing_mut().item_spacing.x = 4.0;
                            ui.label(egui::RichText::new(field_docs::label(name)).color(look::pal(ui).muted));
                            if let Some(doc) = field_docs::doc(component, name) {
                                look::hint(ui, doc);
                            }
                        });
                        ui.push_id(name.as_str(), |ui| changed |= edit_value(ui, component, name, field));
                        ui.end_row();
                    }
                });
            // Nested values (a fire's lit look, a list of items) fold away
            // underneath.
            for (name, field) in fields.iter_mut() {
                if is_scalar(name, field) {
                    continue;
                }
                ui.push_id(name.as_str(), |ui| {
                    let title = egui::RichText::new(field_docs::label(name)).color(look::pal(ui).muted);
                    let header = egui::CollapsingHeader::new(title).id_salt(name.as_str()).default_open(false);
                    let shown = header.show(ui, |ui| changed |= edit_value(ui, component, name, field));
                    if let Some(doc) = field_docs::doc(component, name) {
                        shown.header_response.on_hover_text(doc);
                    }
                });
            }
            changed
        }
    }
}

fn overlays_tab(ui: &mut egui::Ui, dev: &mut DevTools) {
    let o = &mut dev.overlays;
    let muted = |ui: &mut egui::Ui, text: &str| {
        ui.label(egui::RichText::new(text).color(look::pal(ui).muted));
    };
    look::section(ui, "Map");
    look::toggle(ui, &mut o.light, "Light level");
    muted(ui, "Dark blue is pitch black, yellow is fully lit (explored tiles).");
    look::toggle(ui, &mut o.blockers, "Blockers");
    muted(ui, "Red cross blocks movement, dark square blocks sight.");
    look::toggle(ui, &mut o.fire_and_water, "Fire and water");
    muted(ui, "Blue is wet, orange outline is fuel, bright outline is burning, rain clouds boxed.");
    look::toggle(ui, &mut o.coords, "Tile coordinates");
    muted(ui, "Shown when zoomed in far enough to read.");

    look::section(ui, "Noise");
    look::toggle(ui, &mut o.noise, "Noise reach from the cursor");
    ui.add(egui::Slider::new(&mut o.noise_radius, 1..=20).text("radius"));
    muted(ui, "Each tile shows the muffled distance a sound travels to reach it.");

    look::section(ui, "Creatures");
    look::toggle(ui, &mut o.ai, "AI state and sight");
    muted(ui, "Label, sight ring (halved for idle ones while you sneak), and a red line from any that can see you.");
    look::toggle(ui, &mut o.path, "Path to the player");
    muted(ui, "From the creature selected on the Inspect tab.");
}

fn effects_tab(ui: &mut egui::Ui, dev: &mut DevTools) {
    use crate::devtools::Shot;
    let muted = |ui: &mut egui::Ui, text: &str| {
        ui.label(egui::RichText::new(text).color(look::pal(ui).muted));
    };

    look::section(ui, "Projectiles");
    muted(ui, "Click a tile to fire from the player through the real game rules.");
    ui.horizontal_wrapped(|ui| {
        for shot in Shot::ALL {
            tool_button(ui, dev, CursorTool::Shoot(shot), shot.name(), "Fire at the clicked tile");
        }
    });

    look::section(ui, "Explosions (look only)");
    ui.horizontal(|ui| {
        tool_button(ui, dev, CursorTool::LookOfExplosion { fiery: true }, "Fiery", "The barrel blast and fireball look");
        tool_button(ui, dev, CursorTool::LookOfExplosion { fiery: false }, "Shockwave", "A blast with no fire");
    });
    ui.add(egui::Slider::new(&mut dev.explosion_radius, 1..=6).text("radius"));
    muted(ui, "Nothing is harmed: use the Blast tool on the World tab for a real one.");

    look::section(ui, "Camera and numbers");
    ui.horizontal(|ui| {
        for (label, amount) in [
            ("Swing", CAMERA_SHAKE_PLAYER_SWING),
            ("Boss", CAMERA_SHAKE_BOSS_ABILITY),
            ("Explosion", CAMERA_SHAKE_EXPLOSION),
            ("Max", CAMERA_SHAKE_MAX_AMPLITUDE),
        ] {
            if ui.button(label).clicked() {
                dev.commands.push(DevCommand::Shake(amount));
            }
        }
    });
    ui.horizontal(|ui| {
        for amount in [3, 12, 40] {
            if ui.button(format!("Damage {amount}")).on_hover_text("A number over the player").clicked() {
                dev.commands.push(DevCommand::DamageNumber(amount));
            }
        }
    });

    look::section(ui, "Call Rain look");
    ui.horizontal(|ui| {
        for rain in RainStyle::ALL {
            ui.selectable_value(&mut dev.rain_style, rain, rain.name());
        }
    });

    look::section(ui, "Particle fire");
    ui.horizontal(|ui| {
        tool_button(ui, dev, CursorTool::ParticleFire, "Place on the map", "A looping fire");
        if ui.button("Clear all").clicked() {
            dev.commands.push(DevCommand::ClearParticleFires);
        }
    });
    muted(ui, "A looping fire drawn by the GPU: visual only, nothing burns.");
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
        CursorTool::PlacePrefab(index) => {
            let rect = tile_rect(camera, ppp, hover.0 as f32 + 0.5, hover.1 as f32 + 0.5);
            if let Some(sprite) = dev.prefabs.get(index).and_then(|p| p.sprite()) {
                let (tex, uv) = art.icon(sprite);
                painter.image(tex, rect, uv, ghost);
            }
            painter.rect_stroke(rect, 2.0, egui::Stroke::new(1.5_f32, egui::Color32::from_rgb(200, 160, 255)));
        }
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
        CursorTool::Blast => {
            let r = crate::constants::FIREBALL_RADIUS as f32;
            let low = tile_rect(camera, ppp, hover.0 as f32 - r + 0.5, hover.1 as f32 - r + 0.5);
            let high = tile_rect(camera, ppp, hover.0 as f32 + r + 0.5, hover.1 as f32 + r + 0.5);
            painter.rect_stroke(low.union(high), 2.0, egui::Stroke::new(1.5_f32, egui::Color32::from_rgb(255, 120, 60)));
        }
        CursorTool::LookOfExplosion { fiery } => {
            let r = dev.explosion_radius as f32;
            let low = tile_rect(camera, ppp, hover.0 as f32 - r + 0.5, hover.1 as f32 - r + 0.5);
            let high = tile_rect(camera, ppp, hover.0 as f32 + r + 0.5, hover.1 as f32 + r + 0.5);
            let colour = if fiery { egui::Color32::from_rgb(255, 170, 80) } else { egui::Color32::from_rgb(200, 200, 220) };
            painter.rect_stroke(low.union(high), 2.0, egui::Stroke::new(1.0_f32, colour));
        }
        CursorTool::Shoot(_) => {
            let shooter = world
                .query::<(&crate::components::Player, &crate::components::Position)>()
                .iter()
                .map(|(_, (_, p))| (p.x, p.y))
                .next();
            let to = tile_rect(camera, ppp, hover.0 as f32 + 0.5, hover.1 as f32 + 0.5);
            if let Some((x, y)) = shooter {
                let from = tile_rect(camera, ppp, x as f32 + 0.5, y as f32 + 0.5).center();
                painter.line_segment([from, to.center()], egui::Stroke::new(1.0_f32, egui::Color32::from_rgb(255, 220, 120)));
            }
            painter.rect_stroke(to, 2.0, egui::Stroke::new(1.5_f32, egui::Color32::from_rgb(255, 220, 120)));
        }
        CursorTool::Douse => {
            let low = tile_rect(camera, ppp, hover.0 as f32 - 0.5, hover.1 as f32 - 0.5);
            let high = tile_rect(camera, ppp, hover.0 as f32 + 1.5, hover.1 as f32 + 1.5);
            painter.rect_stroke(low.union(high), 2.0, egui::Stroke::new(1.5_f32, egui::Color32::from_rgb(110, 170, 240)));
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
        other => {
            let colour = match other {
                CursorTool::Teleport => egui::Color32::from_rgb(150, 200, 255),
                CursorTool::Inspect => egui::Color32::from_rgb(235, 235, 235),
                CursorTool::Ignite | CursorTool::ParticleFire => egui::Color32::from_rgb(255, 170, 80),
                _ => egui::Color32::from_rgb(230, 90, 80),
            };
            let rect = tile_rect(camera, ppp, hover.0 as f32 + 0.5, hover.1 as f32 + 0.5);
            painter.rect_stroke(rect, 2.0, egui::Stroke::new(1.5_f32, colour));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::as_rgb;
    use serde_json::json;

    #[test]
    fn colours_are_recognised_in_either_shape() {
        assert_eq!(as_rgb("color", &json!([1.0, 0.5, 0.0])), Some([1.0, 0.5, 0.0]));
        assert_eq!(as_rgb("value", &json!({"r": 0.2, "g": 0.3, "b": 0.4})), Some([0.2, 0.3, 0.4]));
        assert_eq!(as_rgb("position", &json!([1.0, 2.0, 3.0])), None, "only a colour by name");
        assert_eq!(as_rgb("value", &json!({"x": 1, "y": 2, "z": 3})), None);
    }
}
