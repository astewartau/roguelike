//! Start screen UI component.
//!
//! Displays class selection and game start interface.

use super::icons::UiIcons;
use super::style;
use crate::components::PlayerClass;
use crate::multi_tileset::MultiTileset;
use crate::run_history::{RunRecord, SeedMode, PAST_RUNS_SHOWN};
use egui_glow::EguiGlow;
use winit::window::Window;

/// Run the start screen UI for class selection.
///
/// `seed_mode` picks how the run is seeded (random / daily / custom);
/// `seed_input` is the custom seed text (numbers parse directly, other text
/// is hashed). `past_runs` (all recorded runs, newest first) backs the Stats
/// panel, toggled by `stats_open`, whose "use seed" buttons copy a past seed
/// into the custom field.
///
/// Returns Some(PlayerClass) if the player clicked Start, None otherwise.
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
) -> Option<PlayerClass> {
    let mut start_clicked = None;

    egui_glow.run(window, |ctx| {
        // Keyboard navigation: arrows/WASD change class, Enter/Space starts.
        // Default-select the first class so the menu always has a highlight and
        // Enter works straight away. Suppressed while a widget (the seed text
        // field) has keyboard focus, so typing a seed doesn't flip classes.
        let classes = PlayerClass::ALL;
        if selected_class.is_none() {
            *selected_class = Some(classes[0]);
        }
        if !ctx.wants_keyboard_input() {
            ctx.input(|i| {
                use egui::Key;
                let cur = selected_class
                    .and_then(|c| classes.iter().position(|&x| x == c))
                    .unwrap_or(0);
                let next = i.key_pressed(Key::ArrowRight)
                    || i.key_pressed(Key::D)
                    || i.key_pressed(Key::ArrowDown)
                    || i.key_pressed(Key::S);
                let prev = i.key_pressed(Key::ArrowLeft)
                    || i.key_pressed(Key::A)
                    || i.key_pressed(Key::ArrowUp)
                    || i.key_pressed(Key::W);
                if next {
                    *selected_class = Some(classes[(cur + 1) % classes.len()]);
                } else if prev {
                    *selected_class = Some(classes[(cur + classes.len() - 1) % classes.len()]);
                }
                if i.key_pressed(Key::Enter) || i.key_pressed(Key::Space) {
                    start_clicked = *selected_class;
                }
            });
        }

        // Center the window
        egui::CentralPanel::default()
            .frame(egui::Frame::none().fill(egui::Color32::from_rgb(20, 20, 30)))
            .show(ctx, |ui| {
                ui.vertical_centered(|ui| {
                    ui.add_space(100.0);

                    // Title
                    ui.heading(
                        egui::RichText::new("Roguelike")
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
                        // Calculate total width: 120px per class + 20px spacing between
                        let class_count = PlayerClass::ALL.len() as f32;
                        let total_width = class_count * 120.0 + (class_count - 1.0) * 20.0;
                        ui.add_space((ui.available_width() - total_width) / 2.0);

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
                            painter.image(texture_id, sprite_rect, uv_rect, egui::Color32::WHITE);

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
                        egui::RichText::new("Start Game").size(24.0).color(
                            if start_enabled {
                                egui::Color32::WHITE
                            } else {
                                egui::Color32::GRAY
                            },
                        ),
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
                            PlayerClass::Druid => "Staff",
                            PlayerClass::Necromancer => "Staff",
                        };
                        let inventory = match class {
                            PlayerClass::Fighter => "(empty)",
                            PlayerClass::Ranger => "Dagger",
                            PlayerClass::Druid => "(empty)",
                            PlayerClass::Necromancer => "Health Potion",
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

                    ui.add_space(24.0);

                    // Seed picker: random every run, today's daily seed, or a
                    // custom entry (numbers used as-is, words hashed).
                    ui.label(
                        egui::RichText::new("Seed")
                            .size(14.0)
                            .color(egui::Color32::LIGHT_GRAY),
                    );
                    ui.add_space(4.0);
                    ui.horizontal(|ui| {
                        let total_width = 3.0 * 80.0 + 2.0 * ui.spacing().item_spacing.x;
                        ui.add_space((ui.available_width() - total_width) / 2.0);
                        for (mode, label) in [
                            (SeedMode::Random, "Random"),
                            (SeedMode::Daily, "Daily"),
                            (SeedMode::Custom, "Custom"),
                        ] {
                            let selected = *seed_mode == mode;
                            let text = egui::RichText::new(label).size(14.0).color(
                                if selected {
                                    style::colors::DUNGEON_GOLD
                                } else {
                                    egui::Color32::LIGHT_GRAY
                                },
                            );
                            let button = egui::SelectableLabel::new(selected, text);
                            if ui.add_sized(egui::vec2(80.0, 22.0), button).clicked() {
                                *seed_mode = mode;
                            }
                        }
                    });
                    ui.add_space(6.0);
                    match seed_mode {
                        SeedMode::Random => {
                            ui.label(
                                egui::RichText::new("A fresh dungeon every run")
                                    .size(12.0)
                                    .color(egui::Color32::GRAY),
                            );
                        }
                        SeedMode::Daily => {
                            ui.label(
                                egui::RichText::new(format!(
                                    "{} — same dungeon for everyone today",
                                    crate::run_history::daily_date_string()
                                ))
                                .size(12.0)
                                .color(egui::Color32::GRAY),
                            );
                        }
                        SeedMode::Custom => {
                            ui.add(
                                egui::TextEdit::singleline(seed_input)
                                    .desired_width(240.0)
                                    .horizontal_align(egui::Align::Center)
                                    .font(egui::TextStyle::Monospace)
                                    .hint_text("number or words"),
                            );
                        }
                    }

                    ui.add_space(16.0);

                    // Stats panel toggle (past runs live behind it).
                    if ui
                        .add(egui::Button::new(
                            egui::RichText::new(if *stats_open {
                                "Hide Stats"
                            } else {
                                "Stats"
                            })
                            .size(14.0),
                        ))
                        .clicked()
                    {
                        *stats_open = !*stats_open;
                    }
                });
            });

        // Stats panel: aggregate numbers over all recorded runs plus the most
        // recent few (from runs_history.jsonl), newest first. "use seed"
        // copies that run's seed into the custom seed field. Opens centered,
        // draggable by its title bar, closable via the X (which resets
        // stats_open through .open()).
        if *stats_open {
            style::dungeon_window(
                ctx,
                icons,
                "Stats",
                |window| {
                    window
                        .open(stats_open)
                        .pivot(egui::Align2::CENTER_CENTER)
                        .default_pos(ctx.screen_rect().center())
                        .resizable(false)
                        .collapsible(false)
                },
                |ui| {
                    if past_runs.is_empty() {
                        ui.label(
                            egui::RichText::new("No completed runs yet.")
                                .size(13.0)
                                .color(egui::Color32::LIGHT_GRAY),
                        );
                        return;
                    }

                    let best_floor = past_runs.iter().map(|r| r.floor_reached).max().unwrap_or(0);
                    let total_kills: u32 = past_runs.iter().map(|r| r.kills).sum();
                    ui.label(
                        egui::RichText::new(format!(
                            "{} runs — best: floor {} — {} total kills",
                            past_runs.len(),
                            best_floor + 1,
                            total_kills
                        ))
                        .size(13.0)
                        .color(style::colors::DUNGEON_GOLD),
                    );
                    ui.separator();

                    egui::Grid::new("past_runs_grid")
                        .num_columns(3)
                        .spacing(egui::vec2(10.0, 4.0))
                        .show(ui, |ui| {
                            for run in past_runs.iter().take(PAST_RUNS_SHOWN) {
                                ui.label(
                                    egui::RichText::new(format!(
                                        "{} — floor {}",
                                        run.class,
                                        run.floor_reached + 1
                                    ))
                                    .size(13.0)
                                    .color(egui::Color32::WHITE),
                                );
                                ui.label(
                                    egui::RichText::new(format!(
                                        "{} kills — {}",
                                        run.kills, run.cause_of_death
                                    ))
                                    .size(12.0)
                                    .color(egui::Color32::LIGHT_GRAY),
                                );
                                if ui
                                    .small_button(
                                        egui::RichText::new("use seed").size(12.0),
                                    )
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
    });

    start_clicked
}
