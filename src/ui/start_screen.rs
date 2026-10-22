//! Start screen UI component.
//!
//! The main menu: a torch-lit room (see [`super::menu_backdrop`]) with the
//! title on its wall, a card per class, the chosen class's details, and the
//! run options — seed, sandbox and past runs — beneath.

use super::icons::UiIcons;
use super::inventory::{paint_item_socket, paint_stack_count};
use super::menu_backdrop;
use super::style::{self, colors, ButtonWeight};
use crate::components::{AbilityType, ClassKit, EquippedWeapon, ItemType, PlayerClass};
use crate::constants::*;
use crate::multi_tileset::MultiTileset;
use crate::run_history::{RunRecord, SeedMode, PAST_RUNS_SHOWN};
use egui::RichText;
use egui_glow::EguiGlow;
use winit::window::Window;

/// What the start screen was left with.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StartChoice {
    /// A run, as this class.
    Run(PlayerClass),
    /// The sandbox; a player placed there is this class.
    Sandbox(PlayerClass),
}

/// Run the start screen UI for class selection.
///
/// `seed_mode` picks how the run is seeded (random / daily / custom);
/// `seed_input` is the custom seed text (numbers parse directly, other text
/// is hashed). `past_runs` (all recorded runs, newest first) backs the Past
/// Runs panel, toggled by `stats_open`, whose "use seed" buttons copy a past
/// seed into the custom field.
///
/// Returns what was chosen, if anything: a run as the selected class, or
/// the sandbox (an empty world for the developer tools, where placing a
/// player makes them that class).
#[allow(clippy::too_many_arguments)]
pub fn run_start_screen(
    egui_glow: &mut EguiGlow,
    window: &Window,
    tileset: &MultiTileset,
    icons: &UiIcons,
    selected_class: &mut Option<PlayerClass>,
    seed_mode: &mut SeedMode,
    seed_input: &mut String,
    stats_open: &mut bool,
    past_runs: &[RunRecord],
) -> Option<StartChoice> {
    let mut start_clicked = None;

    egui_glow.run(window, |ctx| {
        // Keyboard navigation: arrows/WASD step through the classes, 1-4 pick
        // one, Enter/Space starts. A class is always selected, so the menu
        // always has a highlight and Enter works straight away. Suppressed
        // while a widget (the seed text field) has keyboard focus, so typing a
        // seed doesn't flip classes.
        let classes = PlayerClass::ALL;
        let mut class = selected_class.unwrap_or(classes[0]);
        if !ctx.wants_keyboard_input() {
            ctx.input(|i| {
                use egui::Key;
                let cur = classes.iter().position(|&x| x == class).unwrap_or(0);
                let next = [Key::ArrowRight, Key::D, Key::ArrowDown, Key::S]
                    .iter()
                    .any(|&k| i.key_pressed(k));
                let prev = [Key::ArrowLeft, Key::A, Key::ArrowUp, Key::W]
                    .iter()
                    .any(|&k| i.key_pressed(k));
                if next {
                    class = classes[(cur + 1) % classes.len()];
                } else if prev {
                    class = classes[(cur + classes.len() - 1) % classes.len()];
                }
                for (key, &c) in [Key::Num1, Key::Num2, Key::Num3, Key::Num4].iter().zip(&classes) {
                    if i.key_pressed(*key) {
                        class = c;
                    }
                }
                if i.key_pressed(Key::Enter) || i.key_pressed(Key::Space) {
                    start_clicked = Some(StartChoice::Run(class));
                }
            });
        }

        // The torches flicker and the embers rise whether or not anything
        // else on the screen changes.
        let time = ctx.input(|i| i.time) as f32;
        ctx.request_repaint();

        egui::CentralPanel::default()
            .frame(egui::Frame::none())
            .show(ctx, |ui| {
                let screen = ui.max_rect();
                menu_backdrop::paint_menu_backdrop(ui.painter(), screen, tileset, icons, time);

                ui.vertical_centered(|ui| {
                    // The title sits on the face of the wall, between the
                    // torches.
                    let wall = MENU_BACKDROP_TILE_SIZE * MENU_BACKDROP_WALL_ROWS as f32;
                    ui.add_space(((wall - MENU_TITLE_SIZE) / 2.0).max(0.0));
                    style::menu_title(ui, "Roguelike", colors::DUNGEON_GOLD);
                    ui.add_space(6.0);
                    style::ornament_rule(ui, MENU_RULE_WIDTH);
                    ui.add_space(14.0);
                    ui.label(
                        RichText::new("CHOOSE YOUR CLASS")
                            .size(13.0)
                            .color(colors::TEXT_MUTED),
                    );
                    ui.add_space(8.0);

                    class_cards(ui, tileset, icons, &mut class, time);
                    ui.add_space(14.0);
                    class_details(ui, icons, class);
                    ui.add_space(18.0);

                    if style::menu_button(ui, "Enter the Dungeon", ButtonWeight::Primary, true)
                        .clicked()
                    {
                        start_clicked = Some(StartChoice::Run(class));
                    }
                    ui.add_space(10.0);
                    centred_row(ui, MENU_BUTTON_SMALL_SIZE[0] * 2.0 + 8.0, |ui| {
                        ui.spacing_mut().item_spacing.x = 8.0;
                        if style::menu_button(ui, "Sandbox", ButtonWeight::Small, false)
                            .on_hover_text(
                                "An empty world and the developer tools: place yourself, \
                                 creatures and things, and watch",
                            )
                            .clicked()
                        {
                            start_clicked = Some(StartChoice::Sandbox(class));
                        }
                        if style::menu_button(ui, "Past Runs", ButtonWeight::Small, *stats_open)
                            .clicked()
                        {
                            *stats_open = !*stats_open;
                        }
                    });
                    ui.add_space(16.0);
                    seed_row(ui, seed_mode, seed_input);
                });

                ui.painter().text(
                    screen.center_bottom() - egui::vec2(0.0, 12.0),
                    egui::Align2::CENTER_BOTTOM,
                    "←/→ or 1-4  class    •    Enter  begin",
                    egui::FontId::proportional(12.0),
                    colors::TEXT_MUTED,
                );
            });

        *selected_class = Some(class);

        if *stats_open {
            past_runs_window(ctx, icons, stats_open, seed_mode, seed_input, past_runs);
        }
    });

    start_clicked
}

/// Lay `add_contents` out left to right in a row `width` wide, centred in
/// the parent.
fn centred_row(ui: &mut egui::Ui, width: f32, add_contents: impl FnOnce(&mut egui::Ui)) {
    ui.allocate_ui_with_layout(
        egui::vec2(width, 0.0),
        egui::Layout::left_to_right(egui::Align::Center),
        add_contents,
    );
}

/// A line on what playing each class is like, from what its kit does.
fn class_tagline(class: PlayerClass) -> &'static str {
    match class {
        PlayerClass::Fighter => "Sword and shield. Stands in the thick of it.",
        PlayerClass::Ranger => "Bow, snares and quick feet. Never where the blow lands.",
        PlayerClass::Druid => "Vines, rain and beasts. Turns the dungeon against its own.",
        PlayerClass::Necromancer => "Bones and stolen life. The dead fight beside you.",
    }
}

/// The row of class cards. Clicking one selects it.
fn class_cards(
    ui: &mut egui::Ui,
    tileset: &MultiTileset,
    icons: &UiIcons,
    class: &mut PlayerClass,
    time: f32,
) {
    let [card_w, card_h] = MENU_CLASS_CARD_SIZE;
    let count = PlayerClass::ALL.len() as f32;
    let width = count * card_w + (count - 1.0) * MENU_CLASS_CARD_GAP;
    centred_row(ui, width, |ui| {
        ui.spacing_mut().item_spacing.x = MENU_CLASS_CARD_GAP;
        for (index, candidate) in PlayerClass::ALL.into_iter().enumerate() {
            let (rect, response) =
                ui.allocate_exact_size(egui::vec2(card_w, card_h), egui::Sense::click());
            let selected = *class == candidate;
            let ctx = ui.ctx();
            let lit = ctx.animate_bool_with_time(
                response.id.with("class_card_lit"),
                selected,
                MENU_HIGHLIGHT_FADE,
            );
            let hover = ctx.animate_bool_with_time(
                response.id.with("class_card_hover"),
                response.hovered(),
                MENU_HIGHLIGHT_FADE,
            );

            let painter = ui.painter();
            let rect = painter.round_rect_to_pixels(rect);
            let border = style::mix(colors::PANEL_BORDER, colors::TEXT_ACCENT, lit.max(hover * 0.5));
            painter.add(style::stone_shape(ctx, rect, border));
            style::paint_panel_edges(painter, icons, rect);
            if lit > 0.0 {
                // A second brass line inside the first, so the selection
                // reads at a glance across the row.
                painter.rect_stroke(
                    rect.shrink(1.0),
                    0.0,
                    egui::Stroke::new(style::BORDER_WIDTH, colors::TEXT_ACCENT.gamma_multiply(lit)),
                );
            }

            let sprite_centre = rect.center() - egui::vec2(0.0, 12.0);
            style::paint_glow(
                painter,
                sprite_centre,
                MENU_CLASS_GLOW_RADIUS,
                colors::TORCH_GLOW,
                MENU_CLASS_GLOW_STRENGTH * lit,
            );
            // The chosen class idles: a whole-pixel bob, as a sprite would.
            let bob = if selected {
                ((time * MENU_CLASS_BOB_SPEED).sin() * MENU_CLASS_BOB_AMPLITUDE).round()
            } else {
                0.0
            };
            let (sheet, id) = candidate.sprite();
            let size = egui::vec2(MENU_CLASS_SPRITE_SIZE, MENU_CLASS_SPRITE_SIZE);
            // The others stand back in the dark until picked.
            let shade = style::mix(colors::TEXT_MUTED, egui::Color32::WHITE, lit.max(hover * 0.6));
            painter.image(
                icons.texture_for_sheet(sheet),
                painter.round_rect_to_pixels(egui::Rect::from_center_size(
                    sprite_centre + egui::vec2(0.0, bob),
                    size,
                )),
                tileset.get_egui_uv(sheet, id),
                shade,
            );

            painter.text(
                egui::pos2(rect.center().x, rect.bottom() - 14.0),
                egui::Align2::CENTER_CENTER,
                candidate.name(),
                egui::FontId::proportional(16.0),
                style::mix(colors::TEXT_PRIMARY, colors::DUNGEON_GOLD, lit),
            );
            painter.text(
                rect.left_top() + egui::vec2(12.0, 8.0),
                egui::Align2::LEFT_TOP,
                (index + 1).to_string(),
                egui::FontId::proportional(11.0),
                colors::TEXT_MUTED,
            );

            if response.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                *class = candidate;
            }
        }
    });
}

/// The chosen class's details: a line on how it plays, its starting stats as
/// rows of pips, and what it starts with — weapon, pack and abilities — read
/// from the same functions that set up a run, so this can never disagree
/// with what the player actually gets.
fn class_details(ui: &mut egui::Ui, icons: &UiIcons, class: PlayerClass) {
    let inner_width = MENU_DETAILS_WIDTH - 2.0 * style::PANEL_INNER_MARGIN;
    centred_row(ui, MENU_DETAILS_WIDTH, |ui| {
        style::stone_panel(ui, icons, |ui| {
            ui.set_width(inner_width);
            ui.vertical_centered(|ui| {
                ui.label(
                    RichText::new(class_tagline(class))
                        .italics()
                        .size(13.0)
                        .color(colors::TEXT_PRIMARY),
                );
            });
            ui.add_space(4.0);
            style::panel_rule(ui);
            ui.add_space(2.0);

            ui.horizontal_top(|ui| {
                ui.vertical(|ui| {
                    ui.add_space(6.0);
                    let (str, int, agi) = class.stats();
                    for (label, value) in [("STR", str), ("INT", int), ("AGI", agi)] {
                        stat_pips(ui, label, value);
                        ui.add_space(6.0);
                    }
                });
                ui.add_space(18.0);
                ui.vertical(|ui| kit_rows(ui, icons, class));
            });
        });
    });
}

/// A starting stat as a labelled row of pips out of [`MENU_STAT_PIPS`].
fn stat_pips(ui: &mut egui::Ui, label: &str, value: i32) {
    ui.horizontal(|ui| {
        ui.label(RichText::new(label).size(12.0).color(colors::TEXT_MUTED));
        let [pip_w, pip_h] = MENU_STAT_PIP_SIZE;
        let width = MENU_STAT_PIPS as f32 * (pip_w + MENU_STAT_PIP_GAP) - MENU_STAT_PIP_GAP;
        let (rect, _) = ui.allocate_exact_size(egui::vec2(width, pip_h), egui::Sense::hover());
        let painter = ui.painter();
        for i in 0..MENU_STAT_PIPS {
            let min = rect.left_top() + egui::vec2(i as f32 * (pip_w + MENU_STAT_PIP_GAP), 0.0);
            let pip = painter.round_rect_to_pixels(egui::Rect::from_min_size(min, egui::vec2(pip_w, pip_h)));
            if i < value {
                painter.rect_filled(pip, 0.0, colors::DUNGEON_GOLD);
                // A lit top edge, the same detail as a HUD bar's fill.
                let top = egui::Rect::from_min_size(pip.min, egui::vec2(pip.width(), 1.0));
                painter.rect_filled(top, 0.0, style::brighten(colors::DUNGEON_GOLD, 1.25));
            } else {
                painter.rect_filled(pip, 0.0, colors::PIP_EMPTY);
            }
        }
        ui.label(RichText::new(value.to_string()).size(13.0).color(colors::TEXT_PRIMARY));
    });
}

/// Weapon, pack and abilities, one labelled row of icons each.
fn kit_rows(ui: &mut egui::Ui, icons: &UiIcons, class: PlayerClass) {
    let socket = egui::vec2(MENU_KIT_ICON_SIZE + 4.0, MENU_KIT_ICON_SIZE + 4.0);
    let row_label = |ui: &mut egui::Ui, text: &str| {
        let (rect, _) = ui.allocate_exact_size(egui::vec2(72.0, socket.y), egui::Sense::hover());
        ui.painter().text(
            rect.left_center(),
            egui::Align2::LEFT_CENTER,
            text,
            egui::FontId::proportional(11.0),
            colors::TEXT_MUTED,
        );
    };

    ui.spacing_mut().item_spacing = egui::vec2(4.0, 4.0);

    ui.horizontal(|ui| {
        row_label(ui, "WEAPON");
        let (item, name) = match class.starting_weapon() {
            EquippedWeapon::Melee(weapon) => {
                let item = weapon.item.unwrap_or(ItemType::Sword);
                (item, crate::systems::item_name(item).to_string())
            }
            EquippedWeapon::Ranged(_) => (ItemType::Bow, crate::systems::item_name(ItemType::Bow).to_string()),
        };
        let (rect, response) = ui.allocate_exact_size(socket, egui::Sense::hover());
        paint_item_socket(ui.painter(), icons, rect, item, response.hovered());
        response.on_hover_text(&name);
        ui.label(RichText::new(name).size(13.0).color(colors::TEXT_PRIMARY));
    });

    ui.horizontal(|ui| {
        row_label(ui, "PACK");
        let items = class.starting_inventory();
        if items.is_empty() {
            ui.label(RichText::new("nothing").italics().size(12.0).color(colors::TEXT_MUTED));
        }
        for stack in crate::systems::stack_items(&items) {
            let item = items[stack.first_index].kind;
            let (rect, response) = ui.allocate_exact_size(socket, egui::Sense::hover());
            paint_item_socket(ui.painter(), icons, rect, item, response.hovered());
            if stack.count > 1 {
                paint_stack_count(ui.painter(), rect, stack.count);
            }
            response.on_hover_text(crate::systems::item_name(item));
        }
    });

    ui.horizontal(|ui| {
        row_label(ui, "ABILITIES");
        let mut abilities: Vec<AbilityType> = vec![class.ability()];
        abilities.extend(class.secondary_ability().map(|(ability, _)| ability));
        abilities.extend(ClassKit::for_class(class).abilities.iter().map(|k| k.ability));
        abilities.extend(class.starting_spells());
        for ability in abilities {
            let (rect, response) = ui.allocate_exact_size(socket, egui::Sense::hover());
            let painter = ui.painter();
            style::paint_recess(
                painter,
                rect,
                if response.hovered() { colors::SOCKET_HOVER_FILL } else { colors::SOCKET_BG },
            );
            let (texture, uv) = super::ability_icon(icons, ability);
            let tint = super::hotbar::ability_icon_tint(ability);
            painter.image(
                texture,
                egui::Rect::from_center_size(rect.center(), egui::vec2(MENU_KIT_ICON_SIZE, MENU_KIT_ICON_SIZE)),
                uv,
                tint,
            );
            response.on_hover_ui(|ui| {
                ui.label(RichText::new(ability.name()).strong());
                ui.label(ability.description());
            });
        }
    });
}

/// How the run is seeded: random, today's daily seed, or a custom one
/// (numbers used as-is, words hashed).
fn seed_row(ui: &mut egui::Ui, seed_mode: &mut SeedMode, seed_input: &mut String) {
    centred_row(ui, 500.0, |ui| {
        ui.label(RichText::new("SEED").size(12.0).color(colors::TEXT_MUTED));
        ui.add_space(4.0);
        for (mode, label) in [
            (SeedMode::Random, "Random"),
            (SeedMode::Daily, "Daily"),
            (SeedMode::Custom, "Custom"),
        ] {
            if style::menu_chip(ui, label, *seed_mode == mode).clicked() {
                *seed_mode = mode;
            }
        }
        ui.add_space(8.0);
        match seed_mode {
            SeedMode::Random => {
                ui.label(RichText::new("a fresh dungeon every run").size(12.0).color(colors::TEXT_MUTED));
            }
            SeedMode::Daily => {
                ui.label(
                    RichText::new(format!(
                        "{}: the same dungeon for everyone",
                        crate::run_history::daily_date_string()
                    ))
                    .size(12.0)
                    .color(colors::TEXT_MUTED),
                );
            }
            SeedMode::Custom => {
                ui.add(
                    egui::TextEdit::singleline(seed_input)
                        .desired_width(170.0)
                        .font(egui::TextStyle::Monospace)
                        .hint_text("number or words"),
                );
            }
        }
    });
}

/// Past runs: aggregate numbers over every recorded run, then the most recent
/// few (from runs_history.jsonl), newest first. "use seed" copies that run's
/// seed into the custom seed field. Opens centred, draggable by its title
/// bar, closable via the X (which resets `open` through `.open()`).
fn past_runs_window(
    ctx: &egui::Context,
    icons: &UiIcons,
    open: &mut bool,
    seed_mode: &mut SeedMode,
    seed_input: &mut String,
    past_runs: &[RunRecord],
) {
    style::dungeon_window(
        ctx,
        icons,
        "Past Runs",
        |window| {
            window
                .open(open)
                .pivot(egui::Align2::CENTER_CENTER)
                .default_pos(ctx.screen_rect().center())
                .resizable(false)
                .collapsible(false)
        },
        |ui| {
            if past_runs.is_empty() {
                ui.label(
                    RichText::new("No completed runs yet.")
                        .size(13.0)
                        .color(colors::TEXT_MUTED),
                );
                return;
            }

            let best_floor = past_runs.iter().map(|r| r.floor_reached).max().unwrap_or(0);
            let total_kills: u32 = past_runs.iter().map(|r| r.kills).sum();
            ui.label(
                RichText::new(format!(
                    "{} runs — deepest: floor {} — {} total kills",
                    past_runs.len(),
                    best_floor + 1,
                    total_kills
                ))
                .size(13.0)
                .color(colors::DUNGEON_GOLD),
            );
            style::panel_rule(ui);

            egui::Grid::new("past_runs_grid")
                .num_columns(3)
                .spacing(egui::vec2(12.0, 6.0))
                .striped(true)
                .show(ui, |ui| {
                    for run in past_runs.iter().take(PAST_RUNS_SHOWN) {
                        ui.label(
                            RichText::new(format!("{} — floor {}", run.class, run.floor_reached + 1))
                                .size(13.0)
                                .color(colors::TEXT_PRIMARY),
                        );
                        ui.label(
                            RichText::new(format!("{} kills — {}", run.kills, run.cause_of_death))
                                .size(12.0)
                                .color(colors::TEXT_MUTED),
                        );
                        if ui
                            .small_button(RichText::new("use seed").size(12.0))
                            .on_hover_text(format!("Seed: {}", run.seed))
                            .clicked()
                        {
                            *seed_input = run.seed.to_string();
                            *seed_mode = SeedMode::Custom;
                        }
                        ui.end_row();
                    }
                });
        },
    );
}
