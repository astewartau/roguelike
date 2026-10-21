//! The add palette: a search box that floats over the map, for adding
//! components and building blocks to a prefab (or, from the Inspect tab, to
//! an entity), the way a command palette works. Type to filter, Up/Down to
//! choose, Enter to add and close (Shift+Enter, or Shift+click, to add and
//! keep it open for more), Esc or a click elsewhere to close.
//!
//! It keeps the panel itself short: the list of everything you could add
//! never lands in the sidebar.

use crate::component_registry as registry;
use crate::devtools::catalog::{self, BLOCKS, GROUPS};
use crate::devtools::{DevCommand, DevTools, PaletteTarget};

use super::devtools::DevView;
use super::devtools_look as look;

/// How wide the palette is, in points.
const PALETTE_WIDTH: f32 = 460.0;
/// How many results show without scrolling.
const PALETTE_ROWS: f32 = 9.0;

/// One thing the palette can add.
struct Choice {
    name: &'static str,
    summary: String,
    /// What group it's in (or "Building block").
    tag: &'static str,
    /// What adding it adds.
    adds: Vec<&'static str>,
}

/// Everything addable to `has`, matching `search`: building blocks first,
/// then components group by group. Modifiers of something already there
/// aren't offered (they're switches in its card), nor internal state
/// unless asked for.
fn choices(search: &str, internal: bool, has: &dyn Fn(&str) -> bool) -> Vec<Choice> {
    let search = search.to_lowercase();
    let matches = |name: &str, summary: &str, phrase: &str| {
        search.is_empty()
            || name.to_lowercase().contains(&search)
            || summary.to_lowercase().contains(&search)
            || phrase.contains(&search)
    };
    let mut out = Vec::new();
    for block in BLOCKS {
        let adds: Vec<&'static str> = block.parts.iter().copied().filter(|p| !has(p)).collect();
        if !adds.is_empty() && matches(block.name, block.summary, "") {
            out.push(Choice {
                name: block.name,
                summary: format!("{} Adds {}.", block.summary, adds.join(", ")),
                tag: "Building block",
                adds,
            });
        }
    }
    for group in GROUPS {
        if group == "Internal" && !internal {
            continue;
        }
        for info in catalog::INFO.iter().filter(|i| i.group == group) {
            if has(info.name) || info.name == "Position" || catalog::host_of(info.name, has).is_some() {
                continue;
            }
            if !matches(info.name, info.summary, info.phrase) {
                continue;
            }
            let brings: Vec<&'static str> =
                registry::all_requirements(info.name).into_iter().filter(|n| !has(n)).collect();
            let summary = if brings.is_empty() {
                info.summary.to_string()
            } else {
                format!("{} Also adds {}.", info.summary, brings.join(", "))
            };
            out.push(Choice { name: info.name, summary, tag: group, adds: vec![info.name] });
        }
    }
    out
}

/// Draw the palette if it's open, and add what's chosen to its target.
pub fn draw_palette(ctx: &egui::Context, dev: &mut DevTools, view: &DevView) {
    let Some(target) = dev.palette else { return };
    // The click that opened it isn't a click outside it.
    let just_opened = dev.palette_focus;
    // What the target already has.
    let present: Vec<String> = match target {
        PaletteTarget::Prefab(i) => match dev.prefabs.get(i) {
            Some(p) => p.names().into_iter().map(String::from).collect(),
            None => {
                dev.palette = None;
                return;
            }
        },
        PaletteTarget::Entity(e) => {
            if !view.world.contains(e) {
                dev.palette = None;
                return;
            }
            registry::components_of(view.world, e).0.iter().map(|c| c.name.to_string()).collect()
        }
    };
    let has = |n: &str| present.iter().any(|p| p == n);
    let found = choices(&dev.picker_search, dev.picker_internal, &has);
    dev.palette_selected = dev.palette_selected.min(found.len().saturating_sub(1));

    let (up, down, enter, shift, escape) = ctx.input(|i| {
        (
            i.key_pressed(egui::Key::ArrowUp),
            i.key_pressed(egui::Key::ArrowDown),
            i.key_pressed(egui::Key::Enter),
            i.modifiers.shift,
            i.key_pressed(egui::Key::Escape),
        )
    });
    if down && dev.palette_selected + 1 < found.len() {
        dev.palette_selected += 1;
    }
    if up {
        dev.palette_selected = dev.palette_selected.saturating_sub(1);
    }

    let mut picked: Option<(usize, bool)> = if enter && !found.is_empty() { Some((dev.palette_selected, shift)) } else { None };
    let screen = ctx.screen_rect();
    // Centred over the map left uncovered by the rail and the open panel.
    let map_centre = (screen.width() - dev.hud_right) / 2.0;
    let area = egui::Area::new(egui::Id::new("dev_add_palette"))
        .order(egui::Order::Foreground)
        .fixed_pos(egui::pos2((map_centre - PALETTE_WIDTH / 2.0).max(8.0), screen.height() * 0.12))
        .show(ctx, |ui| {
            look::apply(ui);
            let p = look::pal(ui);
            look::panel_frame().show(ui, |ui| {
                ui.set_width(PALETTE_WIDTH);
                let title = match target {
                    PaletteTarget::Prefab(i) => format!("Add to {}", dev.prefabs[i].name),
                    PaletteTarget::Entity(_) => "Add to it".to_string(),
                };
                ui.horizontal(|ui| {
                    ui.label(look::strong(title));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if look::cross_button(ui).on_hover_text("Close (Esc)").clicked() {
                            dev.palette = None;
                        }
                    });
                });
                let search = look::search_box(ui, &mut dev.picker_search, "Search: burn, light, door, creature…");
                if dev.palette_focus {
                    search.request_focus();
                    dev.palette_focus = false;
                }
                if search.changed() {
                    dev.palette_selected = 0;
                }
                ui.add_space(4.0);
                let row_height = 44.0;
                egui::ScrollArea::vertical().max_height(row_height * PALETTE_ROWS).show(ui, |ui| {
                    ui.spacing_mut().item_spacing.y = 0.0;
                    if found.is_empty() {
                        ui.colored_label(p.muted, "Nothing matches that.");
                    }
                    for (n, choice) in found.iter().enumerate() {
                        let selected = n == dev.palette_selected;
                        let response = palette_row(ui, choice, selected);
                        if selected && (up || down) {
                            response.scroll_to_me(None);
                        }
                        if response.hovered() && ui.input(|i| i.pointer.delta() != egui::Vec2::ZERO) {
                            dev.palette_selected = n;
                        }
                        if response.clicked() {
                            picked = Some((n, ui.input(|i| i.modifiers.shift)));
                        }
                    }
                });
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    look::toggle(ui, &mut dev.picker_internal, "Show internal state");
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.label(egui::RichText::new("↵ add · ⇧↵ add more · Esc close").small().color(p.muted));
                    });
                });
            });
        });

    if let Some((n, keep_open)) = picked {
        if let Some(choice) = found.get(n) {
            match target {
                PaletteTarget::Prefab(i) => {
                    for name in &choice.adds {
                        dev.prefabs[i].add(name);
                    }
                    dev.prefabs_dirty = true;
                    // Open the new part's card, so its settings are right there.
                    dev.expanded_part = choice.adds.first().map(|n| n.to_string());
                }
                PaletteTarget::Entity(e) => {
                    for name in &choice.adds {
                        if let Some(entry) = registry::find(name) {
                            dev.commands.push(DevCommand::AddComponent { entity: e, name: entry.name });
                        }
                    }
                    dev.expanded_component = choice.adds.first().copied();
                }
            }
        }
        if !keep_open {
            dev.palette = None;
        } else {
            dev.palette_focus = true;
        }
    }
    let clicked_elsewhere = !just_opened
        && ctx.input(|i| i.pointer.any_pressed())
        && ctx.pointer_interact_pos().is_some_and(|at| !area.response.rect.contains(at));
    if escape || clicked_elsewhere {
        dev.palette = None;
    }
}

/// One result: its name and what group it's in on top, what it does
/// underneath; the highlighted one is filled.
fn palette_row(ui: &mut egui::Ui, choice: &Choice, selected: bool) -> egui::Response {
    let p = look::pal(ui);
    let width = ui.available_width();
    let pad = egui::vec2(10.0, 6.0);
    let name = ui.painter().layout_no_wrap(
        choice.name.to_owned(),
        egui::FontId::new(14.0, egui::FontFamily::Name(look::SANS_STRONG.into())),
        if selected { p.on_accent } else { p.text },
    );
    let tag = ui.painter().layout_no_wrap(
        choice.tag.to_owned(),
        egui::FontId::new(11.0, egui::FontFamily::Name(look::SANS.into())),
        if selected { p.on_accent } else { p.muted },
    );
    let summary = ui.painter().layout(
        choice.summary.clone(),
        egui::FontId::new(12.0, egui::FontFamily::Name(look::SANS.into())),
        if selected { p.on_accent } else { p.muted },
        width - pad.x * 2.0,
    );
    let height = pad.y * 2.0 + name.size().y + 2.0 + summary.size().y;
    let (rect, response) = ui.allocate_exact_size(egui::vec2(width, height), egui::Sense::click());
    if selected {
        ui.painter().rect_filled(rect, egui::Rounding::same(8.0), p.accent);
    } else if response.hovered() {
        ui.painter().rect_filled(rect, egui::Rounding::same(8.0), p.hover);
    }
    let at = rect.min + pad;
    let name_size = name.size();
    ui.painter().galley(at, name, p.text);
    ui.painter().galley(egui::pos2(rect.right() - pad.x - tag.size().x, at.y + 2.0), tag, p.muted);
    ui.painter().galley(at + egui::vec2(0.0, name_size.y + 2.0), summary, p.muted);
    response.on_hover_cursor(egui::CursorIcon::PointingHand)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_palette_offers_what_can_be_added_and_says_what_comes_with_it() {
        let has_nothing = |_: &str| false;
        let all = choices("", false, &has_nothing);
        assert_eq!(all.first().map(|c| c.tag), Some("Building block"), "blocks first");
        assert!(all.iter().all(|c| c.tag != "Internal"), "internal state hidden");

        let chase = choices("hostile", false, &has_nothing);
        let entry = chase.iter().find(|c| c.name == "ChaseAI").expect("found by what it does");
        assert!(entry.summary.contains("Also adds Actor"));

        // With a hostile mind there, its modifiers are switches, not results.
        let mind = |n: &str| n == "ChaseAI" || n == "Actor";
        assert!(!choices("charge", false, &mind).iter().any(|c| c.name == "Charger"));
        assert!(choices("charge", false, &has_nothing).iter().any(|c| c.name == "Charger"));
    }
}
