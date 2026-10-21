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

use super::devtools::{edit_value, row, DevArt};
use super::devtools_look as look;


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

    ui.add_space(4.0);
    let label = if dev.picker_open {
        "Done adding"
    } else {
        "+  Add to it"
    };
    if look::primary_button(ui, label).clicked() {
        dev.picker_open = !dev.picker_open;
    }
    if dev.picker_open {
        let current = dev.prefabs[i].clone();
        for name in picker(ui, dev, &|n| current.has(n)) {
            changed |= dev.prefabs[i].add(name);
        }
    }
    changed |= cards(ui, dev, art, i);
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
        egui::ComboBox::from_id_salt("prefab_template")
            .selected_text("From a template")
            .show_ui(ui, |ui| {
                look::apply(ui);
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
                dev.tool = if armed {
                    None
                } else {
                    Some(CursorTool::PlacePrefab(i))
                };
            }
            if ui.small_button("Copy").clicked() {
                duplicate = Some(i);
            }
            if look::cross_button(ui).on_hover_text("Delete").clicked() {
                delete = Some(i);
            }
            let selected = dev.selected_prefab == Some(i);
            let clicked = match sprite {
                Some(sprite) => row(ui, art, sprite, &name, selected).clicked(),
                None => ui.selectable_label(selected, &name).clicked(),
            };
            if clicked {
                dev.selected_prefab = if selected { None } else { Some(i) };
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
                let (rect, _) =
                    ui.allocate_exact_size(egui::vec2(48.0, 48.0), egui::Sense::hover());
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
                    let name = prefab
                        .value("Name")
                        .and_then(|v| v.as_str())
                        .unwrap_or(&prefab.name);
                    ui.label(look::strong(name));
                    ui.horizontal(|ui| {
                        if let Some(hp) = prefab
                            .value("Health")
                            .and_then(|h| h.get("max"))
                            .and_then(|v| v.as_i64())
                        {
                            ui.colored_label(look::pal(ui).danger, format!("{hp} HP"));
                        }
                        if let Some(light) = prefab.value("LightSource") {
                            let rgb = light
                                .get("color")
                                .and_then(|c| c.as_array())
                                .map(|c| {
                                    c.iter()
                                        .map(|v| v.as_f64().unwrap_or(1.0) as f32)
                                        .collect::<Vec<_>>()
                                })
                                .filter(|c| c.len() == 3)
                                .unwrap_or_else(|| vec![1.0, 1.0, 1.0]);
                            let colour = egui::Rgba::from_rgb(rgb[0], rgb[1], rgb[2]);
                            let (swatch, _) = ui
                                .allocate_exact_size(egui::vec2(12.0, 12.0), egui::Sense::hover());
                            ui.painter().circle_filled(swatch.center(), 6.0, colour);
                            let radius =
                                light.get("radius").and_then(|r| r.as_f64()).unwrap_or(0.0);
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
                    let summary = if phrases.is_empty() {
                        "does nothing yet".to_string()
                    } else {
                        phrases.join(" · ")
                    };
                    ui.add(
                        egui::Label::new(egui::RichText::new(summary).color(look::pal(ui).accent))
                            .wrap(),
                    );
                });
            });
        });
}

/// The tint its `SpriteTint` gives, or white.
fn tint_of(prefab: &Prefab) -> egui::Color32 {
    let Some(t) = prefab.value("SpriteTint") else {
        return egui::Color32::WHITE;
    };
    let c = |k: &str| {
        (t.get(k)
            .and_then(|v| v.as_f64())
            .unwrap_or(1.0)
            .clamp(0.0, 1.0)
            * 255.0) as u8
    };
    egui::Color32::from_rgb(c("r"), c("g"), c("b"))
}

/// Everything a component needs and doesn't have, each with a button that
/// adds it. Returns whether anything was added.
fn needs(ui: &mut egui::Ui, prefab: &mut Prefab) -> bool {
    let mut add: Option<&'static str> = None;
    let unmet: Vec<(String, &'static str)> = prefab
        .unmet_needs()
        .into_iter()
        .map(|(c, n)| (c.to_string(), n))
        .collect();
    let invisible =
        !prefab.has("Sprite") && !prefab.has("AnimatedSprite") && !prefab.components.is_empty();
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
            ui.colored_label(
                look::pal(ui).warning,
                format!("⚠ {component} does nothing without {what}"),
            );
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

/// The add picker: a search box, building blocks as chips, then every
/// component not yet there (`has`), grouped by what it does, each with what
/// it does in a sentence. Returns the components picked this frame (all of
/// a block's parts, for a block). Used by the prefab builder and the
/// inspector alike; it stays open to add more.
pub(super) fn picker(
    ui: &mut egui::Ui,
    dev: &mut DevTools,
    has: &dyn Fn(&str) -> bool,
) -> Vec<&'static str> {
    let mut picked = Vec::new();
    look::card(ui, |ui| {
        look::search_box(ui, &mut dev.picker_search, "Search: burn, light, door…");
        let search = dev.picker_search.to_lowercase();
        let matches = |name: &str, summary: &str, phrase: &str| {
            search.is_empty()
                || name.to_lowercase().contains(&search)
                || summary.to_lowercase().contains(&search)
                || phrase.contains(&search)
        };

        let blocks: Vec<_> = BLOCKS
            .iter()
            .filter(|b| matches(b.name, b.summary, ""))
            .collect();
        if !blocks.is_empty() {
            look::section(ui, "Building blocks");
            ui.horizontal_wrapped(|ui| {
                ui.spacing_mut().item_spacing = egui::vec2(6.0, 6.0);
                for block in blocks {
                    if look::chip(ui, block.name)
                        .on_hover_text(block.summary)
                        .clicked()
                    {
                        picked.extend(block.parts.iter().copied().filter(|p| !has(p)));
                    }
                }
            });
        }

        let mut any = false;
        for group in GROUPS {
            if group == "Internal" && !dev.picker_internal {
                continue;
            }
            let entries: Vec<&catalog::Info> = catalog::INFO
                .iter()
                .filter(|info| info.group == group && !has(info.name) && info.name != "Position")
                // Modifiers of something already here are switches in its card.
                .filter(|info| catalog::host_of(info.name, has).is_none())
                .filter(|info| matches(info.name, info.summary, info.phrase))
                .collect();
            if entries.is_empty() {
                continue;
            }
            any = true;
            let searching = !search.is_empty();
            look::accordion(
                ui,
                ("picker_group", group),
                group,
                entries.len(),
                searching,
                |ui| {
                    ui.spacing_mut().item_spacing.y = 0.0;
                    for info in entries {
                        let brings: Vec<&str> =
                            crate::component_registry::all_requirements(info.name).into_iter().filter(|n| !has(n)).collect();
                        let summary = if brings.is_empty() {
                            info.summary.to_string()
                        } else {
                            format!("{}  Also adds {}.", info.summary, brings.join(", "))
                        };
                        if look::add_row(ui, info.name, &summary).clicked() {
                            picked.push(info.name);
                        }
                    }
                },
            );
        }
        if !any && !search.is_empty() {
            ui.colored_label(look::pal(ui).muted, "Nothing matches that.");
        }
        ui.add_space(6.0);
        look::toggle(ui, &mut dev.picker_internal, "Show internal state")
            .on_hover_text("Also list state the game keeps for itself");
    });
    picked
}

/// The prefab's components, as cards grouped like the picker, each with its
/// settings laid out (not collapsed) and a ✕ to remove it.
fn cards(ui: &mut egui::Ui, dev: &mut DevTools, art: &DevArt, i: usize) -> bool {
    let mut changed = false;
    let mut remove = None;
    let mut edits: Vec<PartEdit> = Vec::new();
    let group_of = |name: &str| {
        catalog::info_of(name)
            .map(|i| i.group)
            .unwrap_or("Internal")
    };
    for group in GROUPS {
        let current = dev.prefabs[i].clone();
        let has = |n: &str| current.has(n);
        let indices: Vec<usize> = (0..dev.prefabs[i].components.len())
            .filter(|&n| group_of(&dev.prefabs[i].components[n].component) == group)
            // A modifier is a switch in its host's card, not a card itself.
            .filter(|&n| catalog::host_of(&dev.prefabs[i].components[n].component, &has).is_none())
            .collect();
        if indices.is_empty() {
            continue;
        }
        look::accordion(
            ui,
            ("prefab_group", i, group),
            group,
            indices.len(),
            false,
            |ui| {
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
                                let title = if known {
                                    name.clone()
                                } else {
                                    format!("{name} (unknown)")
                                };
                                ui.label(look::strong(title));
                                if let Some(info) = info {
                                    look::hint(ui, info.summary);
                                }
                                let needed_by: Vec<String> =
                                    dev.prefabs[i].needed_by(&name).into_iter().map(String::from).collect();
                                if !needed_by.is_empty() {
                                    ui.colored_label(look::pal(ui).muted, format!("for {}", needed_by.join(", ")));
                                } else if let Some(info) = info.filter(|i| !i.phrase.is_empty()) {
                                    ui.colored_label(look::pal(ui).muted, info.phrase);
                                }
                                ui.with_layout(
                                    egui::Layout::right_to_left(egui::Align::Center),
                                    |ui| {
                                        let tip = if needed_by.is_empty() {
                                            "Remove".to_string()
                                        } else {
                                            format!("Remove (and {}, which need it)", needed_by.join(", "))
                                        };
                                        if look::cross_button(ui).on_hover_text(tip).clicked() {
                                            remove = Some(n);
                                        }
                                    },
                                );
                            });
                            let value = &mut dev.prefabs[i].components[n].value;
                            if !value.is_null() {
                                changed |= ui.push_id(("part", n), |ui| edit_value(ui, art, &name, "", value)).inner;
                            }
                            if let Some(need) = trigger_line(ui, &name, &has) {
                                edits.push(PartEdit::Add(need));
                            }
                            let value_of = |m: &str| current.value(m).cloned();
                            edits.extend(ui.push_id(("mods", n), |ui| modifier_switches(ui, art, &name, &has, &value_of)).inner);
                        });
                }
            },
        );
    }
    if let Some(n) = remove {
        dev.prefabs[i].remove_at(n);
        changed = true;
    }
    for edit in edits {
        let prefab = &mut dev.prefabs[i];
        match edit {
            PartEdit::Add(name) => {
                prefab.add(name);
            }
            PartEdit::Remove(name) => {
                if let Some(n) = prefab.components.iter().position(|c| c.component == name) {
                    prefab.remove_at(n);
                }
            }
            PartEdit::Set(name, value) => {
                if let Some(part) = prefab.components.iter_mut().find(|c| c.component == name) {
                    part.value = value;
                }
            }
        }
        changed = true;
    }
    changed
}

/// A change to the parts of an entity (or a prefab) asked for from inside a
/// card: the builder applies it to the prefab, the inspector sends it to
/// the world.
pub(super) enum PartEdit {
    Add(&'static str),
    Remove(&'static str),
    Set(&'static str, serde_json::Value),
}

/// Inside `host`'s card: a switch for each modifier that belongs to it (see
/// `catalog::MODIFIERS`), and, under each one switched on, its settings.
pub(super) fn modifier_switches(
    ui: &mut egui::Ui,
    art: &DevArt,
    host: &str,
    has: &dyn Fn(&str) -> bool,
    value_of: &dyn Fn(&str) -> Option<serde_json::Value>,
) -> Vec<PartEdit> {
    let mine: Vec<&(&'static str, &'static str, &'static str)> = catalog::MODIFIERS
        .iter()
        .filter(|(m, hosts, _)| {
            // Where it sits: the host it's under if it's on, else the first
            // of its hosts that's here.
            catalog::host_of(m, has) == Some(host) && hosts.split('|').any(|h| h == host)
        })
        .collect();
    let mut edits = Vec::new();
    if mine.is_empty() {
        return edits;
    }
    ui.add_space(2.0);
    ui.horizontal_wrapped(|ui| {
        for (modifier, _, label) in &mine {
            let mut on = has(modifier);
            let info = catalog::info_of(modifier).map(|i| i.summary).unwrap_or("");
            if look::toggle(ui, &mut on, *label).on_hover_text(info).changed() {
                edits.push(if on { PartEdit::Add(modifier) } else { PartEdit::Remove(modifier) });
            }
        }
    });
    for (modifier, _, label) in &mine {
        let Some(mut value) = value_of(modifier).filter(|v| !v.is_null()) else { continue };
        ui.label(egui::RichText::new(*label).small().color(look::pal(ui).muted));
        if ui.push_id(("modifier", *modifier), |ui| edit_value(ui, art, modifier, "", &mut value)).inner {
            edits.push(PartEdit::Set(modifier, value));
        }
    }
    edits
}

/// For a reaction (`catalog::TRIGGERS`): "Goes off when", each trigger
/// with a dot (filled if it can happen here) and, if it can't, a button to
/// add what it needs. Returns the component asked for.
pub(super) fn trigger_line(ui: &mut egui::Ui, name: &str, has: &dyn Fn(&str) -> bool) -> Option<&'static str> {
    let triggers = catalog::triggers_of(name);
    if triggers.is_empty() {
        return None;
    }
    let p = look::pal(ui);
    let mut add = None;
    ui.horizontal_wrapped(|ui| {
        ui.label(egui::RichText::new("Goes off when").small().color(p.muted));
        for (label, need) in triggers {
            let possible = need.is_empty() || has(need);
            let (dot, _) = ui.allocate_exact_size(egui::vec2(10.0, 10.0), egui::Sense::hover());
            if possible {
                ui.painter().circle_filled(dot.center(), 4.0, p.accent);
            } else {
                ui.painter().circle_stroke(dot.center(), 3.5, egui::Stroke::new(1.0_f32, p.muted));
            }
            ui.label(egui::RichText::new(*label).color(if possible { p.text } else { p.muted }));
            if !possible && ui.small_button(format!("+ {need}")).on_hover_text(format!("Add {need} so this can happen")).clicked() {
                add = crate::component_registry::find(need).map(|e| e.name);
            }
        }
    });
    add
}

#[cfg(test)]
mod tests {
    use crate::devtools::prefab::Prefab;

    #[test]
    fn a_creature_block_needs_nothing_more() {
        let mut p = Prefab::empty("Test");
        for part in super::BLOCKS
            .iter()
            .find(|b| b.name == "Creature")
            .unwrap()
            .parts
        {
            p.add(part);
        }
        assert!(p.unmet_needs().is_empty(), "{:?}", p.unmet_needs());

        // Something that shatters can be knocked into from the start, and
        // broken once it has Health: its triggers say which are possible.
        let mut lamp = Prefab::empty("Lamp");
        lamp.add("Shatters");
        let possible = |p: &Prefab| {
            crate::devtools::catalog::triggers_of("Shatters")
                .iter()
                .filter(|(_, need)| need.is_empty() || p.has(need))
                .map(|(label, _)| *label)
                .collect::<Vec<_>>()
        };
        assert_eq!(possible(&lamp), vec!["knocked into"]);
        lamp.add("Health");
        assert_eq!(possible(&lamp), vec!["broken (0 HP)", "knocked into"]);
    }
}
