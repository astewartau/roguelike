//! The Prefabs tab: the prefab library, and the builder for the one being
//! edited.
//!
//! The builder is meant to be put together from what things do, not from
//! remembering component names: a preview card says what the prefab is so
//! far, building blocks add a recognisable set in one click, a searchable
//! picker lists every component by group with what it does, and anything
//! a component needs to work is flagged with a button that adds it. Sprites
//! are chosen from the sheet itself. What each component means lives in
//! `devtools::catalog`.

use crate::component_registry as registry;
use crate::devtools::catalog::{self, BLOCKS, GROUPS};
use crate::devtools::prefab::{self, Prefab};
use crate::devtools::{spawn, CursorTool, DevTools};
use crate::tile::SpriteSheet;

use super::devtools::{edit_value, row, DevArt};
use super::devtools_look as look;

/// Side of one tile in the sprite picker, in points.
const PICKER_TILE: f32 = 28.0;
const SHEETS: [(SpriteSheet, &str); 5] = [
    (SpriteSheet::Monsters, "Monsters"),
    (SpriteSheet::Rogues, "People"),
    (SpriteSheet::Items, "Items"),
    (SpriteSheet::Tiles, "Tiles"),
    (SpriteSheet::AnimatedTiles, "Animated"),
];

pub(super) fn prefabs_tab(ui: &mut egui::Ui, dev: &mut DevTools, art: &DevArt) {
    library(ui, dev, art);
    let Some(i) = dev.selected_prefab.filter(|i| *i < dev.prefabs.len()) else {
        return;
    };
    ui.separator();
    let mut changed = false;
    ui.horizontal(|ui| {
        ui.label("Name");
        changed |= ui.text_edit_singleline(&mut dev.prefabs[i].name).changed();
    });
    preview_card(ui, art, &dev.prefabs[i]);
    changed |= needs(ui, &mut dev.prefabs[i]);

    ui.horizontal(|ui| {
        egui::ComboBox::from_id_salt("prefab_blocks").selected_text("Add a building block").width(160.0).show_ui(ui, |ui| {
            for block in BLOCKS {
                if ui.selectable_label(false, block.name).on_hover_text(block.summary).clicked() {
                    for part in block.parts {
                        changed |= dev.prefabs[i].add(part);
                    }
                }
            }
        });
        let label = if dev.picker_open { "Done adding" } else { "+ Component" };
        if ui.button(label).clicked() {
            dev.picker_open = !dev.picker_open;
        }
    });
    if dev.picker_open {
        changed |= picker(ui, dev, i);
    }
    changed |= cards(ui, dev, art, i);
    changed |= sprite_picker(ui.ctx(), dev, art, i);
    if changed {
        dev.prefabs_dirty = true;
    }
}

/// New, from a template, and the list of prefabs with Place / Copy / delete.
fn library(ui: &mut egui::Ui, dev: &mut DevTools, art: &DevArt) {
    ui.horizontal(|ui| {
        if ui.button("New empty").clicked() {
            dev.prefabs.push(Prefab::empty("New prefab"));
            dev.selected_prefab = Some(dev.prefabs.len() - 1);
            dev.prefabs_dirty = true;
        }
        egui::ComboBox::from_id_salt("prefab_template").selected_text("From a template").show_ui(ui, |ui| {
            for what in spawn::all_spawnables() {
                if ui.selectable_label(false, what.name()).clicked() {
                    if let Some(template) = prefab::template(what) {
                        dev.prefabs.push(template);
                        dev.selected_prefab = Some(dev.prefabs.len() - 1);
                        dev.prefabs_dirty = true;
                    }
                }
            }
        });
    });
    ui.colored_label(
        look::pal(ui).muted,
        "Or Save as prefab from the Inspect tab. Saved to dev_prefabs.json; the Spawn tab's count applies.",
    );

    let mut delete = None;
    let mut duplicate = None;
    for i in 0..dev.prefabs.len() {
        let (name, sprite) = (dev.prefabs[i].name.clone(), dev.prefabs[i].sprite());
        ui.horizontal(|ui| {
            let armed = dev.tool == Some(CursorTool::PlacePrefab(i));
            if ui.selectable_label(armed, "Place").clicked() {
                dev.tool = if armed { None } else { Some(CursorTool::PlacePrefab(i)) };
            }
            if ui.small_button("Copy").clicked() {
                duplicate = Some(i);
            }
            if ui.small_button("✕").on_hover_text("Delete").clicked() {
                delete = Some(i);
            }
            let selected = dev.selected_prefab == Some(i);
            let clicked = match sprite {
                Some(sprite) => row(ui, art, sprite, &name, selected).clicked(),
                None => ui.selectable_label(selected, &name).clicked(),
            };
            if clicked {
                dev.selected_prefab = if selected { None } else { Some(i) };
                dev.sprite_pick = None;
            }
        });
    }
    if let Some(i) = duplicate {
        let mut copy = dev.prefabs[i].clone();
        copy.name = format!("{} copy", copy.name);
        dev.prefabs.push(copy);
        dev.prefabs_dirty = true;
    }
    if let Some(i) = delete {
        dev.prefabs.remove(i);
        dev.selected_prefab = None;
        dev.sprite_pick = None;
        if matches!(dev.tool, Some(CursorTool::PlacePrefab(_))) {
            dev.disarm();
        }
        dev.prefabs_dirty = true;
    }
}

/// What the prefab is so far: its look, name, health and light, and in a
/// few words what its components make it do.
fn preview_card(ui: &mut egui::Ui, art: &DevArt, prefab: &Prefab) {
    egui::Frame::none()
        .stroke(egui::Stroke::new(1.0_f32, look::pal(ui).border))
        .inner_margin(egui::Margin::same(6.0))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                let (rect, _) = ui.allocate_exact_size(egui::vec2(48.0, 48.0), egui::Sense::hover());
                ui.painter().rect_filled(rect, 2.0, egui::Color32::BLACK);
                match prefab.sprite() {
                    Some(sprite) => {
                        let (tex, uv) = art.icon(sprite);
                        ui.painter().image(tex, rect, uv, tint_of(prefab));
                    }
                    None => {
                        ui.painter().text(
                            rect.center(),
                            egui::Align2::CENTER_CENTER,
                            "?",
                            egui::FontId::proportional(20.0),
                            look::pal(ui).muted,
                        );
                    }
                }
                ui.vertical(|ui| {
                    let name = prefab.value("Name").and_then(|v| v.as_str()).unwrap_or(&prefab.name);
                    ui.strong(name);
                    ui.horizontal(|ui| {
                        if let Some(hp) = prefab.value("Health").and_then(|h| h.get("max")).and_then(|v| v.as_i64()) {
                            ui.colored_label(look::pal(ui).danger, format!("{hp} HP"));
                        }
                        if let Some(light) = prefab.value("LightSource") {
                            let rgb = light
                                .get("color")
                                .and_then(|c| c.as_array())
                                .map(|c| c.iter().map(|v| v.as_f64().unwrap_or(1.0) as f32).collect::<Vec<_>>())
                                .filter(|c| c.len() == 3)
                                .unwrap_or_else(|| vec![1.0, 1.0, 1.0]);
                            let colour = egui::Rgba::from_rgb(rgb[0], rgb[1], rgb[2]);
                            let (swatch, _) = ui.allocate_exact_size(egui::vec2(12.0, 12.0), egui::Sense::hover());
                            ui.painter().circle_filled(swatch.center(), 6.0, colour);
                            let radius = light.get("radius").and_then(|r| r.as_f64()).unwrap_or(0.0);
                            ui.colored_label(look::pal(ui).muted, format!("light {radius:.1}"));
                        }
                    });
                    let phrases: Vec<&str> = prefab
                        .names()
                        .iter()
                        .filter_map(|n| catalog::info_of(n))
                        .map(|i| i.phrase)
                        .filter(|p| !p.is_empty())
                        .collect();
                    let summary = if phrases.is_empty() { "does nothing yet".to_string() } else { phrases.join(" · ") };
                    ui.add(egui::Label::new(egui::RichText::new(summary).color(look::pal(ui).accent)).wrap());
                });
            });
        });
}

/// The tint its `SpriteTint` gives, or white.
fn tint_of(prefab: &Prefab) -> egui::Color32 {
    let Some(t) = prefab.value("SpriteTint") else {
        return egui::Color32::WHITE;
    };
    let c = |k: &str| (t.get(k).and_then(|v| v.as_f64()).unwrap_or(1.0).clamp(0.0, 1.0) * 255.0) as u8;
    egui::Color32::from_rgb(c("r"), c("g"), c("b"))
}

/// Everything a component needs and doesn't have, each with a button that
/// adds it. Returns whether anything was added.
fn needs(ui: &mut egui::Ui, prefab: &mut Prefab) -> bool {
    let mut add: Option<&'static str> = None;
    let unmet: Vec<(String, &'static str)> = prefab.unmet_needs().into_iter().map(|(c, n)| (c.to_string(), n)).collect();
    let invisible = !prefab.has("Sprite") && !prefab.has("AnimatedSprite") && !prefab.components.is_empty();
    if invisible {
        ui.horizontal_wrapped(|ui| {
            ui.colored_label(look::pal(ui).warning, "⚠ Nothing to see:");
            if ui.small_button("Add Sprite").clicked() {
                add = Some("Sprite");
            }
        });
    }
    for (component, need) in &unmet {
        ui.horizontal_wrapped(|ui| {
            let what = need.replace('|', " or ");
            ui.colored_label(look::pal(ui).warning, format!("⚠ {component} does nothing without {what}"));
            for option in need.split('|') {
                if let Some(entry) = registry::find(option) {
                    if ui.small_button(format!("Add {option}")).clicked() {
                        add = Some(entry.name);
                    }
                }
            }
        });
    }
    add.is_some_and(|name| prefab.add(name))
}

/// The add-component picker: a search box, then every component it hasn't
/// got, grouped by what it does, each with what it does in a sentence.
fn picker(ui: &mut egui::Ui, dev: &mut DevTools, i: usize) -> bool {
    let mut changed = false;
    egui::Frame::none()
        .stroke(egui::Stroke::new(1.0_f32, look::pal(ui).accent))
        .inner_margin(egui::Margin::same(6.0))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.add(egui::TextEdit::singleline(&mut dev.picker_search).hint_text("Search: burn, light, door…").desired_width(180.0));
                look::toggle(ui, &mut dev.picker_internal, "Internal").on_hover_text("Also list state the game keeps for itself");
            });
            let search = dev.picker_search.to_lowercase();
            egui::ScrollArea::vertical().id_salt("component_picker").max_height(260.0).show(ui, |ui| {
                for group in GROUPS {
                    if group == "Internal" && !dev.picker_internal {
                        continue;
                    }
                    let entries: Vec<&catalog::Info> = catalog::INFO
                        .iter()
                        .filter(|info| info.group == group && !dev.prefabs[i].has(info.name) && info.name != "Position")
                        .filter(|info| {
                            search.is_empty()
                                || info.name.to_lowercase().contains(&search)
                                || info.summary.to_lowercase().contains(&search)
                                || info.phrase.contains(&search)
                        })
                        .collect();
                    if entries.is_empty() {
                        continue;
                    }
                    ui.label(egui::RichText::new(group).strong().color(look::pal(ui).accent));
                    for info in entries {
                        let response = ui
                            .add(
                                egui::Button::new(
                                    egui::RichText::new(format!("{}  ", info.name)).strong(),
                                )
                                .frame(false),
                            )
                            .on_hover_text(info.summary);
                        ui.add(egui::Label::new(egui::RichText::new(info.summary).small().color(look::pal(ui).muted)).wrap());
                        if response.clicked() {
                            changed |= dev.prefabs[i].add(info.name);
                        }
                    }
                    ui.add_space(4.0);
                }
            });
        });
    changed
}

/// The prefab's components, as cards grouped like the picker, each with its
/// settings laid out (not collapsed) and a ✕ to remove it.
fn cards(ui: &mut egui::Ui, dev: &mut DevTools, art: &DevArt, i: usize) -> bool {
    let mut changed = false;
    let mut remove = None;
    let group_of = |name: &str| catalog::info_of(name).map(|i| i.group).unwrap_or("Internal");
    for group in GROUPS {
        let indices: Vec<usize> = (0..dev.prefabs[i].components.len())
            .filter(|&n| group_of(&dev.prefabs[i].components[n].component) == group)
            .collect();
        if indices.is_empty() {
            continue;
        }
        ui.add_space(4.0);
        ui.label(egui::RichText::new(group).strong().color(look::pal(ui).accent));
        for n in indices {
            let name = dev.prefabs[i].components[n].component.clone();
            let info = catalog::info_of(&name);
            egui::Frame::none()
                .stroke(egui::Stroke::new(1.0_f32, look::pal(ui).border))
                .inner_margin(egui::Margin::symmetric(6.0, 4.0))
                .show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.horizontal(|ui| {
                        let known = registry::find(&name).is_some();
                        let title = if known { name.clone() } else { format!("{name} (unknown)") };
                        ui.strong(title).on_hover_text(info.map(|i| i.summary).unwrap_or(""));
                        if let Some(info) = info.filter(|i| !i.phrase.is_empty()) {
                            ui.colored_label(look::pal(ui).muted, info.phrase);
                        }
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if ui.small_button("✕").on_hover_text("Remove").clicked() {
                                remove = Some(n);
                            }
                            if name == "Sprite" || name == "AnimatedSprite" {
                                let picking = dev.sprite_pick == Some(n);
                                if ui.selectable_label(picking, "Pick…").clicked() {
                                    dev.sprite_pick = if picking { None } else { Some(n) };
                                    if let Some(sheet) = dev.prefabs[i].components[n]
                                        .value
                                        .get("sheet")
                                        .and_then(|s| serde_json::from_value(s.clone()).ok())
                                    {
                                        dev.sprite_sheet = sheet;
                                    }
                                }
                                if let Some(sprite) = dev.prefabs[i].sprite() {
                                    let (tex, uv) = art.icon(sprite);
                                    ui.add(egui::Image::new(egui::load::SizedTexture::new(tex, egui::vec2(20.0, 20.0))).uv(uv));
                                }
                            }
                        });
                    });
                    let value = &mut dev.prefabs[i].components[n].value;
                    if !value.is_null() {
                        changed |= edit_value(ui, &name, "", value);
                    }
                });
        }
    }
    if let Some(n) = remove {
        dev.prefabs[i].components.remove(n);
        dev.sprite_pick = None;
        changed = true;
    }
    changed
}

/// A window showing a whole sprite sheet: click a tile and the component
/// being picked for takes it.
fn sprite_picker(ctx: &egui::Context, dev: &mut DevTools, art: &DevArt, i: usize) -> bool {
    let Some(n) = dev.sprite_pick.filter(|n| *n < dev.prefabs[i].components.len()) else {
        dev.sprite_pick = None;
        return false;
    };
    let mut open = true;
    let mut chosen: Option<(SpriteSheet, u32)> = None;
    egui::Window::new("Pick a sprite")
        .open(&mut open)
        .default_size(egui::vec2(420.0, 480.0))
        .collapsible(false)
        .show(ctx, |ui| {
            ui.horizontal_wrapped(|ui| {
                for (sheet, label) in SHEETS {
                    ui.selectable_value(&mut dev.sprite_sheet, sheet, label);
                }
            });
            let sheet = dev.sprite_sheet;
            let (columns, rows) = art.tileset.grid_size(sheet);
            if columns == 0 {
                ui.colored_label(look::pal(ui).muted, "That sheet isn't loaded.");
                return;
            }
            let tex = art.icons.texture_for_sheet(sheet);
            egui::ScrollArea::both().show_rows(ui, PICKER_TILE, rows as usize, |ui, visible| {
                for r in visible {
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = 0.0;
                        for c in 0..columns {
                            let id = r as u32 * columns + c;
                            let uv = art.tileset.get_egui_uv(sheet, id);
                            let (rect, response) =
                                ui.allocate_exact_size(egui::vec2(PICKER_TILE, PICKER_TILE), egui::Sense::click());
                            if response.hovered() {
                                ui.painter().rect_filled(rect, 0.0, look::pal(ui).hover);
                            }
                            ui.painter().image(tex, rect.shrink(2.0), uv, egui::Color32::WHITE);
                            if response.on_hover_text(format!("#{id}")).clicked() {
                                chosen = Some((sheet, id));
                            }
                        }
                    });
                }
            });
        });
    let mut changed = false;
    if let Some((sheet, id)) = chosen {
        let part = &mut dev.prefabs[i].components[n];
        let key = if part.component == "AnimatedSprite" { "base_tile_id" } else { "tile_id" };
        if let serde_json::Value::Object(fields) = &mut part.value {
            if let Ok(sheet) = serde_json::to_value(sheet) {
                fields.insert("sheet".into(), sheet);
            }
            fields.insert(key.into(), serde_json::Value::from(id));
            changed = true;
        }
        open = false;
    }
    if !open {
        dev.sprite_pick = None;
    }
    changed
}

#[cfg(test)]
mod tests {
    use crate::devtools::prefab::Prefab;

    #[test]
    fn a_creature_block_needs_nothing_more() {
        let mut p = Prefab::empty("Test");
        for part in super::BLOCKS.iter().find(|b| b.name == "Creature").unwrap().parts {
            p.add(part);
        }
        assert!(p.unmet_needs().is_empty(), "{:?}", p.unmet_needs());

        // Something that shatters wants to be breakable.
        let mut lamp = Prefab::empty("Lamp");
        lamp.add("Shatters");
        let unmet: Vec<_> = lamp.unmet_needs().into_iter().map(|(_, n)| n).collect();
        assert_eq!(unmet, vec!["Health", "Attackable"]);
        lamp.add("Health");
        lamp.add("Attackable");
        assert!(lamp.unmet_needs().is_empty());
    }
}
