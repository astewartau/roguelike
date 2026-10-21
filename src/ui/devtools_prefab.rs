//! Prefabs in the Spawn panel: your own things to place, listed above the
//! built-in ones (any of which can be made into a prefab with a
//! right-click), and the builder for the one being edited, which takes over
//! the panel while it's open.
//!
//! The builder is meant to be put together from what things do, not from
//! remembering component names: a preview card says what the prefab is so
//! far, building blocks add a recognisable set in one click, a searchable
//! picker lists every component by group with what it does, and anything
//! a component needs to work is flagged with a button that adds it. Sprites
//! are chosen from the sheet itself. What each component means lives in
//! `devtools::catalog`.

use crate::component_registry as registry;
use crate::devtools::catalog::{self, GROUPS};
use crate::devtools::prefab::{self, Prefab};
use crate::devtools::{spawn, CursorTool, DevTools};

use super::devtools::{edit_value, row, DevArt};
use super::devtools_look as look;


/// Which prefab is being edited, if any (and still exists).
pub(super) fn editing(dev: &DevTools) -> Option<usize> {
    // The built-in ones aren't changed (a copy of one is).
    dev.selected_prefab.filter(|&i| dev.prefabs.get(i).is_some_and(|p| !p.built_in))
}

/// The part of the builder that stays put above the scrolling parts: a way
/// back to the Spawn list, the prefab's name, what it is so far, and the
/// buttons to place it and add to it. (Nothing unless a prefab is being
/// edited.)
pub(super) fn prefabs_pinned(ui: &mut egui::Ui, dev: &mut DevTools, art: &DevArt) {
    let Some(i) = editing(dev) else { return };
    let mut changed = false;
    ui.horizontal(|ui| {
        if ui.button("← Spawn").on_hover_text("Back to everything there is to place").clicked() {
            dev.selected_prefab = None;
            dev.palette = None;
            dev.expanded_part = None;
        }
        changed |= ui
            .add(egui::TextEdit::singleline(&mut dev.prefabs[i].name).desired_width(f32::INFINITY))
            .changed();
    });
    if editing(dev).is_none() {
        return;
    }
    preview_card(ui, art, &dev.prefabs[i]);
    changed |= needs(ui, &mut dev.prefabs[i]);
    ui.horizontal(|ui| {
        let armed = dev.tool == Some(CursorTool::PlacePrefab(i));
        let place = if armed { "Placing… (click the map)" } else { "Place" };
        if ui.selectable_label(armed, place).clicked() {
            dev.tool = if armed { None } else { Some(CursorTool::PlacePrefab(i)) };
        }
        if look::primary_button(ui, "+  Add").on_hover_text("Search components and building blocks").clicked() {
            dev.open_palette(crate::devtools::PaletteTarget::Prefab(i));
        }
    });
    if changed {
        dev.prefabs_dirty = true;
    }
}

/// The scrolling part of the prefab builder: the parts of the prefab being
/// edited.
pub(super) fn prefab_parts(ui: &mut egui::Ui, dev: &mut DevTools, art: &DevArt) {
    if let Some(i) = editing(dev) {
        if cards(ui, dev, art, i) {
            dev.prefabs_dirty = true;
        }
    }
}

/// Start building `prefab`: add it to the library and open it.
fn start_building(dev: &mut DevTools, prefab: Prefab) {
    dev.prefabs.push(prefab);
    dev.selected_prefab = Some(dev.prefabs.len() - 1);
    dev.prefabs_dirty = true;
}

/// A built-in's row in the Spawn panel: right-click to make a prefab of
/// your own from it.
pub(super) fn offer_as_prefab(response: &egui::Response, dev: &mut DevTools, what: spawn::Spawnable) {
    response.context_menu(|ui| {
        look::apply(ui);
        if ui.button("Make a prefab from this").clicked() {
            if let Some(template) = prefab::template(what) {
                start_building(dev, template);
            }
            ui.close_menu();
        }
    });
}

/// The prefabs, at the top of the Spawn panel: the game's own first (the
/// player classes, "Fighter (Player)"), then yours. Click one to place it
/// (one with Player makes it you); the pencil changes one of yours, or
/// makes your own copy of a built-in to change; right-click for the rest.
/// And a way to start a new one.
pub(super) fn my_prefabs(ui: &mut egui::Ui, dev: &mut DevTools, art: &DevArt) {
    let search = dev.search.to_lowercase();
    super::devtools::spawn_group(dev, "Prefabs").show(ui, |ui| {
        let mut delete = None;
        let mut duplicate = None;
        for i in 0..dev.prefabs.len() {
            let (name, sprite, built_in) =
                (dev.prefabs[i].name.clone(), dev.prefabs[i].sprite(), dev.prefabs[i].built_in);
            if !search.is_empty() && !name.to_lowercase().contains(&search) {
                continue;
            }
            ui.horizontal(|ui| {
                let pencil = if built_in { "Built in: make your own copy to change" } else { "Change it" };
                if ui.small_button("✏").on_hover_text(pencil).clicked() {
                    if built_in {
                        duplicate = Some(i);
                    } else {
                        dev.selected_prefab = Some(i);
                    }
                }
                let armed = dev.tool == Some(CursorTool::PlacePrefab(i));
                let mut response = match sprite {
                    Some(sprite) => row(ui, art, sprite, &name, armed),
                    None => ui.selectable_label(armed, &name),
                };
                if built_in {
                    response = response.on_hover_text("Built in. Click to place it; it can't be changed, but a copy can.");
                }
                if response.clicked() {
                    dev.tool = if armed { None } else { Some(CursorTool::PlacePrefab(i)) };
                }
                response.context_menu(|ui| {
                    look::apply(ui);
                    if !built_in && ui.button("Change it").clicked() {
                        dev.selected_prefab = Some(i);
                        ui.close_menu();
                    }
                    let copy = if built_in { "Make your own copy" } else { "Make a copy" };
                    if ui.button(copy).clicked() {
                        duplicate = Some(i);
                        ui.close_menu();
                    }
                    if !built_in && ui.button("Delete").clicked() {
                        delete = Some(i);
                        ui.close_menu();
                    }
                });
            });
        }
        if !dev.prefabs.iter().any(|p| !p.built_in) {
            ui.label(
                egui::RichText::new(
                    "Right-click anything below to make a prefab of your own from it, or use Save as prefab when inspecting.",
                )
                .small()
                .color(look::pal(ui).muted),
            );
        }
        if ui.button("+  New prefab").on_hover_text("Start from nothing and add parts").clicked() {
            start_building(dev, Prefab::empty("New prefab"));
        }
        if let Some(i) = duplicate {
            // A copy is opened to change: that's what one is made for.
            start_building(dev, prefab::copy_of(&dev.prefabs[i]));
        }
        if let Some(i) = delete {
            dev.prefabs.remove(i);
            dev.selected_prefab = None;
            if matches!(dev.tool, Some(CursorTool::PlacePrefab(_))) {
                dev.disarm();
            }
            dev.prefabs_dirty = true;
        }
    });
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

/// The prefab's parts, grouped like the add palette into accordions, each a
/// compact card (see [`part_card`]); only one is open at a time.
fn cards(ui: &mut egui::Ui, dev: &mut DevTools, art: &DevArt, i: usize) -> bool {
    let mut changed = false;
    let mut remove = None;
    let mut edits: Vec<PartEdit> = Vec::new();
    let current = dev.prefabs[i].clone();
    let has = |n: &str| current.has(n);
    let value_of = |m: &str| current.value(m).cloned();
    let group_of = |name: &str| catalog::info_of(name).map(|i| i.group).unwrap_or("Internal");
    if current.components.is_empty() {
        ui.colored_label(look::pal(ui).muted, "Nothing yet: + Add a building block or a component.");
    }
    for group in GROUPS {
        let indices: Vec<usize> = (0..current.components.len())
            .filter(|&n| group_of(&current.components[n].component) == group)
            // A modifier is a switch in its host's card, not a card itself.
            .filter(|&n| catalog::host_of(&current.components[n].component, &has).is_none())
            .collect();
        if indices.is_empty() {
            continue;
        }
        look::accordion(ui, ("prefab_group", i, group), group, indices.len(), false, |ui| {
            for n in indices {
                let name = current.components[n].component.clone();
                let needed_by: Vec<&str> = current.needed_by(&name);
                let expanded = dev.expanded_part.as_deref() == Some(name.as_str());
                let value = &mut dev.prefabs[i].components[n].value;
                let out = ui
                    .push_id(("part", n), |ui| part_card(ui, art, &name, value, &needed_by, expanded, &has, &value_of))
                    .inner;
                changed |= out.changed;
                edits.extend(out.edits);
                match out.action {
                    Some(CardAction::Toggle) => {
                        dev.expanded_part = if expanded { None } else { Some(name.clone()) };
                    }
                    Some(CardAction::Remove) => remove = Some(n),
                    None => {}
                }
            }
        });
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

/// What the user did with a card's header.
pub(super) enum CardAction {
    /// Open it, or close it.
    Toggle,
    Remove,
}

/// What came of drawing a card.
pub(super) struct CardOutcome {
    pub action: Option<CardAction>,
    /// Its own settings were edited (in place, in the value passed in).
    pub changed: bool,
    /// Changes it asked for to other parts (modifier switches, a trigger's
    /// missing need).
    pub edits: Vec<PartEdit>,
}

/// One part of an entity or prefab as a compact card: a single line (its
/// name, a few words of what it's set to, a ? and a remove cross) that
/// opens, on a click, to its settings, what sets it off and its modifier
/// switches. Shared by the prefab builder and the Inspect tab.
#[allow(clippy::too_many_arguments)]
pub(super) fn part_card(
    ui: &mut egui::Ui,
    art: &DevArt,
    name: &str,
    value: &mut serde_json::Value,
    needed_by: &[&str],
    expanded: bool,
    has: &dyn Fn(&str) -> bool,
    value_of: &dyn Fn(&str) -> Option<serde_json::Value>,
) -> CardOutcome {
    let p = look::pal(ui);
    let info = catalog::info_of(name);
    let mut out = CardOutcome { action: None, changed: false, edits: Vec::new() };
    egui::Frame::none()
        .fill(if expanded { p.card } else { egui::Color32::TRANSPARENT })
        .stroke(egui::Stroke::new(1.0_f32, if expanded { p.accent } else { p.border }))
        .rounding(egui::Rounding::same(8.0))
        .inner_margin(egui::Margin::symmetric(8.0, 5.0))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            let header = ui.horizontal(|ui| {
                let known = registry::find(name).is_some();
                ui.label(look::strong(if known { name.to_string() } else { format!("{name} (unknown)") }));
                if let Some((sheet, id)) = super::devtools_tiles::as_tile(value) {
                    let (tex, uv) = art.icon((sheet, id));
                    ui.add(egui::Image::new(egui::load::SizedTexture::new(tex, egui::vec2(18.0, 18.0))).uv(uv));
                }
                let brief = if !needed_by.is_empty() {
                    format!("for {}", needed_by.join(", "))
                } else {
                    brief(name, value, has)
                };
                ui.add(egui::Label::new(egui::RichText::new(brief).color(p.muted)).truncate());
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let tip = if needed_by.is_empty() {
                        "Remove".to_string()
                    } else {
                        format!("Remove (and {}, which need it)", needed_by.join(", "))
                    };
                    if look::cross_button(ui).on_hover_text(tip).clicked() {
                        out.action = Some(CardAction::Remove);
                    }
                    if let Some(info) = info {
                        look::hint(ui, info.summary);
                    }
                });
            });
            // The whole header line opens and closes it.
            let toggle = ui.interact(header.response.rect, ui.id().with("open"), egui::Sense::click());
            if toggle.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() && out.action.is_none() {
                out.action = Some(CardAction::Toggle);
            }
            if !expanded {
                return;
            }
            if !value.is_null() {
                out.changed |= edit_value(ui, art, name, "", value);
            }
            if let Some(need) = trigger_line(ui, name, has) {
                out.edits.push(PartEdit::Add(need));
            }
            out.edits.extend(modifier_switches(ui, art, name, has, value_of));
        });
    out
}

/// A few words of what a part is set to, for its card's line: its own key
/// numbers, then the modifiers switched on in it.
fn brief(name: &str, value: &serde_json::Value, has: &dyn Fn(&str) -> bool) -> String {
    let num = |k: &str| value.get(k).and_then(|v| v.as_f64());
    let int = |k: &str| num(k).map(|v| v.round() as i64);
    let mut words: Vec<String> = match name {
        "Name" => value.as_str().map(|s| vec![s.to_string()]).unwrap_or_default(),
        "Health" => {
            let mut w = vec![format!("{}/{} HP", int("current").unwrap_or(0), int("max").unwrap_or(0))];
            match value.get("targeting").and_then(|t| t.as_str()) {
                Some("OnPurpose") => w.push("hit on purpose".into()),
                Some("Never") => w.push("never attacked".into()),
                _ => {}
            }
            w
        }
        "Stats" => vec![format!(
            "STR {} · INT {} · AGI {}",
            int("strength").unwrap_or(0),
            int("intelligence").unwrap_or(0),
            int("agility").unwrap_or(0)
        )],
        "Actor" => num("speed").map(|s| vec![format!("speed {s:.2}")]).unwrap_or_default(),
        "ChaseAI" => int("sight_radius").map(|r| vec![format!("sees {r} tiles")]).unwrap_or_default(),
        "LightSource" => num("radius").map(|r| vec![format!("radius {r:.1}")]).unwrap_or_default(),
        "Fuel" => match value.get("burn_time") {
            Some(serde_json::Value::String(_)) => vec!["burns forever".into()],
            Some(t) => t.as_f64().map(|t| vec![format!("burns {t:.0}s")]).unwrap_or_default(),
            None => vec![],
        },
        _ => match value {
            serde_json::Value::Object(fields) => fields
                .iter()
                .filter(|(_, v)| v.is_number() || v.is_boolean() || v.is_string())
                .filter(|(k, _)| !super::devtools_tiles::is_tile_key(k))
                .take(2)
                .map(|(k, v)| format!("{} {}", crate::devtools::field_docs::label(k).to_lowercase(), short(v)))
                .collect(),
            _ => vec![],
        },
    };
    let switched_on: Vec<&str> = catalog::MODIFIERS
        .iter()
        .filter(|(m, hosts, _)| has(m) && hosts.split('|').any(|h| h == name) && catalog::host_of(m, has) == Some(name))
        .map(|(_, _, label)| *label)
        .collect();
    if !switched_on.is_empty() {
        words.push(switched_on.join(", "));
    }
    words.join(" · ")
}

fn short(v: &serde_json::Value) -> String {
    match v {
        serde_json::Value::Number(n) => match n.as_f64() {
            Some(f) if f.fract() != 0.0 => format!("{f:.2}"),
            _ => n.to_string(),
        },
        serde_json::Value::Bool(b) => if *b { "yes" } else { "no" }.to_string(),
        serde_json::Value::String(s) => s.clone(),
        _ => String::new(),
    }
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
        for part in crate::devtools::catalog::BLOCKS
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
