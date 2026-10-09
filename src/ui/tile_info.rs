//! The hovered tile: a faint lightening under the cursor, a "what's here"
//! panel in the top-right corner, and the Shift+right-click action menu.
//!
//! What the panel says and which actions the menu lists are decided in
//! [`crate::systems::tile_context`]; this module only draws them. A menu
//! choice is handed back as `UiActions::tile_choice` and executed by the
//! engine through the normal input paths.

use egui::Color32;

use super::style::{self, colors};
use crate::constants::*;
use crate::systems::tile_context::{ContextAction, ContextCommand, Relation, TileInfo, TileKnowledge};

/// An open right-click menu.
#[derive(Debug, Clone)]
pub struct TileMenu {
    /// The tile it was opened on.
    pub tile: (i32, i32),
    /// Where the cursor was, in physical pixels (the engine's mouse space).
    pub screen_pos: (f32, f32),
    /// The entries, refreshed by the engine every frame while open.
    pub actions: Vec<ContextAction>,
}

/// What the player did with the menu this frame.
pub enum TileMenuOutcome {
    /// Nothing; leave it open.
    Open,
    /// Dismissed (clicked elsewhere).
    Closed,
    /// Picked an enabled entry.
    Chose(ContextCommand),
}

fn relation_color(relation: Relation) -> Color32 {
    match relation {
        Relation::You => colors::TEXT_PRIMARY,
        Relation::Companion => colors::RELATION_COMPANION,
        Relation::Friendly => colors::RELATION_FRIENDLY,
        Relation::Hostile => colors::RELATION_HOSTILE,
    }
}

/// A thin HP gauge with the numbers beside it.
fn hp_row(ui: &mut egui::Ui, current: i32, max: i32) {
    ui.horizontal(|ui| {
        let bar_width = ui.available_width() * 0.6;
        let (rect, _) = ui.allocate_exact_size(
            egui::vec2(bar_width, TILE_INFO_HP_BAR_HEIGHT),
            egui::Sense::hover(),
        );
        let painter = ui.painter();
        painter.rect_filled(rect, 0.0, colors::HP_BAR_BG);
        let frac = if max > 0 { (current as f32 / max as f32).clamp(0.0, 1.0) } else { 0.0 };
        let fill = egui::Rect::from_min_size(rect.min, egui::vec2(rect.width() * frac, rect.height()));
        painter.rect_filled(fill, 0.0, colors::HP_BAR);
        painter.rect_stroke(rect, 0.0, egui::Stroke::new(style::BORDER_WIDTH, colors::BAR_FRAME));
        ui.label(egui::RichText::new(format!("{current}/{max} HP")).small().color(colors::TEXT_MUTED));
    });
}

/// The "what's here" panel, pinned to the top-right corner. Not interactable,
/// so it never swallows a click meant for the map underneath it.
pub fn draw_tile_info_panel(ctx: &egui::Context, info: &TileInfo) {
    egui::Area::new(egui::Id::new("tile_info_panel"))
        .anchor(egui::Align2::RIGHT_TOP, egui::vec2(-TILE_INFO_MARGIN, TILE_INFO_MARGIN))
        .order(egui::Order::Middle)
        .interactable(false)
        .show(ctx, |ui| {
            style::dungeon_window_frame().show(ui, |ui| {
                ui.set_width(TILE_INFO_WIDTH);
                let Some(terrain) = &info.terrain else {
                    ui.label(egui::RichText::new("Unexplored").strong().color(colors::TEXT_ACCENT));
                    ui.label(egui::RichText::new("You haven't explored there.").color(colors::TEXT_MUTED));
                    return;
                };
                // Heading: the ground itself; whatever stands on it is listed
                // underneath.
                ui.label(egui::RichText::new(terrain).strong().color(colors::TEXT_ACCENT));
                if info.knowledge == TileKnowledge::Remembered {
                    ui.label(
                        egui::RichText::new("Remembered — out of sight").small().color(colors::TEXT_MUTED),
                    );
                }

                for c in &info.creatures {
                    ui.add_space(4.0);
                    ui.horizontal(|ui| {
                        ui.label(egui::RichText::new(&c.name).color(relation_color(c.relation)));
                        if c.relation != Relation::You {
                            ui.label(
                                egui::RichText::new(crate::systems::tile_context::relation_word(c.relation))
                                    .small()
                                    .color(colors::TEXT_MUTED),
                            );
                        }
                    });
                    if let Some((cur, max)) = c.hp {
                        hp_row(ui, cur, max);
                    }
                    if c.asleep || !c.statuses.is_empty() {
                        ui.horizontal_wrapped(|ui| {
                            ui.spacing_mut().item_spacing.x = 6.0;
                            if c.asleep {
                                ui.label(egui::RichText::new("Asleep").small().color(colors::FATIGUE_ICON));
                            }
                            for s in &c.statuses {
                                ui.label(
                                    egui::RichText::new(crate::systems::tile_context::effect_name(*s))
                                        .small()
                                        .color(style::effect_color(*s)),
                                );
                            }
                        });
                    }
                }

                if !info.features.is_empty() {
                    ui.add_space(4.0);
                    for f in &info.features {
                        ui.label(egui::RichText::new(format!("· {f}")).color(colors::TEXT_PRIMARY));
                    }
                }
            });
        });
}

/// Draw the open right-click menu at the cursor and report what happened.
pub fn draw_tile_context_menu(ctx: &egui::Context, menu: &TileMenu, title: &str) -> TileMenuOutcome {
    let ppp = ctx.pixels_per_point();
    let pos = egui::pos2(
        menu.screen_pos.0 / ppp + CONTEXT_MENU_CURSOR_OFFSET,
        menu.screen_pos.1 / ppp + CONTEXT_MENU_CURSOR_OFFSET,
    );
    let mut outcome = TileMenuOutcome::Open;

    let area = egui::Area::new(egui::Id::new("tile_context_menu"))
        .fixed_pos(pos)
        .order(egui::Order::Foreground)
        .constrain(true)
        .show(ctx, |ui| {
            style::dungeon_window_frame().show(ui, |ui| {
                ui.set_min_width(CONTEXT_MENU_MIN_WIDTH);
                ui.label(egui::RichText::new(title).strong().color(colors::TEXT_ACCENT));
                ui.separator();
                ui.with_layout(egui::Layout::top_down_justified(egui::Align::LEFT), |ui| {
                    for action in &menu.actions {
                        let button = egui::Button::new(&action.label);
                        let response = ui.add_enabled(action.enabled, button);
                        let response = match &action.reason {
                            Some(reason) => response.on_disabled_hover_text(reason),
                            None => response,
                        };
                        if response.clicked() {
                            outcome = TileMenuOutcome::Chose(action.command.clone());
                        }
                    }
                });
            });
        });

    // A left click anywhere outside the menu dismisses it. (Clicks on the
    // map are also caught by the engine, which swallows them so the click
    // that closes the menu doesn't also walk the player somewhere.)
    if matches!(outcome, TileMenuOutcome::Open) {
        let clicked_outside = ctx.input(|i| {
            i.pointer.button_pressed(egui::PointerButton::Primary)
                && i.pointer.interact_pos().map(|p| !area.response.rect.contains(p)).unwrap_or(false)
        });
        if clicked_outside {
            outcome = TileMenuOutcome::Closed;
        }
    }
    outcome
}
