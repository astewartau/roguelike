//! Game engine - owns all game state and provides a clean API to the application shell.
//!
//! The engine handles:
//! - Game state (world, grid, floors, time system)
//! - Input processing
//! - Simulation advancement
//! - Event processing
//!
//! The application shell (main.rs) only handles:
//! - Window creation and event loop
//! - Forwarding events to the engine
//! - Rendering what the engine returns

mod context;
mod dev_spawning;
pub mod floor_transition;
mod game_state;
pub mod initialization;
mod simulation;

pub use context::{ActorCtx, EffectCtx, SimCtx};
pub use floor_transition::{can_transition_floor, handle_floor_transition};
pub use game_state::GameState;
pub use initialization::initialize_single_ai_actor;
pub use simulation::*;


use rand::Rng;

use crate::audio::AudioManager;
use crate::components::{
    AbilityType, ActionType, Actor, ClassAbility, Health, PlayerClass, RangerAbilities,
    SecondaryAbility,
};

use crate::camera::Camera;
use crate::events::EventQueue;
use crate::input::{self, InputState, TargetingMode};
use crate::spawning;
use crate::systems;
use crate::time_system;
use crate::ui::{DevMenu, GameUiState, UiActions};
use crate::vfx::{FireEffect, VfxManager, VisualEffect};

use hecs::Entity;
use winit::event::{ElementState, MouseButton, MouseScrollDelta, WindowEvent};
use winit::keyboard::{KeyCode, PhysicalKey};

/// Actions the engine wants the window to perform
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WindowAction {
    Exit,
    ToggleFullscreen,
}

/// Game mode - whether we're on the start screen or playing
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GameMode {
    /// Class selection screen
    StartScreen,
    /// Playing the game
    Playing,
    /// Paused - showing the pause menu over a frozen dungeon
    Paused,
    /// Player has died - showing the retry screen (state is kept so the
    /// frozen dungeon stays visible behind the overlay)
    GameOver,
}

/// Result of a game tick - contains everything needed for rendering
pub struct TickResult {
    /// Entities to render
    pub entities: Vec<crate::systems::RenderEntity>,
    /// Window action to perform (if any)
    pub window_action: Option<WindowAction>,
}

/// Game-seconds of simulation fast-forwarded per real second while resting.
/// Higher = the world (and natural regen) races ahead faster; very high values
/// approach an instant jump with little visible motion. At natural regen of
/// ~0.1 HP/game-sec, 48 gives ~4.8 HP/real-sec (a full 50 HP heal in ~10s).
const REST_TIME_SCALE: f32 = 48.0;
/// Cap on Wait-steps simulated in a single frame, so a stutter can't run away.
const REST_MAX_STEPS_PER_FRAME: i32 = 64;

/// The game engine - owns all game state and simulation logic.
pub struct GameEngine {
    /// Current game mode (start screen or playing)
    pub game_mode: GameMode,

    /// Selected player class (for start screen)
    pub selected_class: Option<PlayerClass>,

    /// Core game state (world, grid, floors, time) - None on start screen
    pub state: Option<GameState>,

    /// Visual effects manager
    pub vfx: VfxManager,

    /// Event queue for game events
    pub events: EventQueue,

    /// Input state tracking
    pub input: InputState,

    /// UI state (inventory open, chest open, etc.) - needs player entity, created with state
    ui_state: Option<GameUiState>,

    /// Developer menu state
    pub dev_menu: DevMenu,

    /// Accumulated real time (for animations)
    pub real_time: f32,

    /// Clock driving light flicker, advanced by real frame time but only while
    /// actually playing - so torches gutter smoothly in play and stand still
    /// behind the pause and game-over overlays.
    ///
    /// Deliberately *not* the game clock. `GameClock` is event-driven: it
    /// jumps straight to the next action completion and stands completely
    /// still while waiting on player input, which in a turn-based game is most
    /// of the wall clock. Sampling a sine off it would strobe - frozen while
    /// you think, snapping to a new brightness the instant you move - and
    /// during rest/sleep fast-forward it races ahead by whole seconds per
    /// frame, which aliases the flicker into noise. Flicker is cosmetic
    /// animation, so it rides real time like the fire sprite it sits under,
    /// the camera shake and the hit flashes.
    light_flicker_time: f32,

    /// Highlighted option index in the pause menu (keyboard navigation)
    pause_selected: usize,

    /// Audio manager for sound effects
    pub audio: Option<AudioManager>,

    /// True while the player is resting (time auto-fast-forwards each frame).
    resting: bool,

    /// Accumulates fast-forwarded game-time toward the next rest step.
    rest_accumulator: f32,

    /// True while the player is sleeping (time auto-fast-forwards each frame,
    /// fatigue recovers, and the player counts as unaware/sneak-attackable).
    sleeping: bool,

    /// Accumulates fast-forwarded game-time toward the next sleep step.
    sleep_accumulator: f32,

    /// How the start screen picks the next run's seed (random/daily/custom).
    pub seed_mode: crate::run_history::SeedMode,

    /// Seed text for SeedMode::Custom (numbers parse directly, words hash).
    pub seed_input: String,

    /// Whether the start screen's stats panel (past runs) is open.
    pub stats_open: bool,

    /// All recorded runs from `runs_history.jsonl`, newest first. The start
    /// screen aggregates stats over the lot and lists the most recent few.
    pub past_runs: Vec<crate::run_history::RunRecord>,
}

impl GameEngine {
    /// Create a new game engine on the start screen.
    pub fn new() -> Self {
        // Tests build many engines in parallel; opening a real output stream
        // for each one churns the system sound server for no benefit.
        let audio = if cfg!(test) { None } else { AudioManager::new() };
        if audio.is_none() {
            eprintln!("Warning: Could not initialize audio system");
        }

        Self {
            game_mode: GameMode::StartScreen,
            selected_class: Some(PlayerClass::Fighter), // Default selection
            state: None,
            vfx: VfxManager::new(),
            events: EventQueue::new(),
            input: InputState::new(),
            ui_state: None,
            dev_menu: DevMenu::new(),
            real_time: 0.0,
            light_flicker_time: 0.0,
            pause_selected: 0,
            audio,
            resting: false,
            rest_accumulator: 0.0,
            sleeping: false,
            sleep_accumulator: 0.0,
            seed_mode: crate::run_history::SeedMode::default(),
            seed_input: String::new(),
            stats_open: false,
            past_runs: crate::run_history::load_recent(usize::MAX),
        }
    }

    /// The seed the next run will use, per the start screen's seed mode:
    /// fresh entropy, today's daily seed, or the custom text (numeric or
    /// hashed; empty custom text falls back to a random seed).
    pub fn next_run_seed(&self) -> u64 {
        use crate::run_history::SeedMode;
        match self.seed_mode {
            SeedMode::Random => crate::run_history::random_seed(),
            SeedMode::Daily => crate::run_history::daily_seed(),
            SeedMode::Custom => crate::run_history::seed_from_text(&self.seed_input)
                .unwrap_or_else(crate::run_history::random_seed),
        }
    }

    /// The seed of the current (or just-ended) run, if any.
    pub fn current_run_seed(&self) -> Option<u64> {
        self.state.as_ref().map(|s| s.seed)
    }

    /// Start the game with the selected class and run seed.
    pub fn start_game(&mut self, class: PlayerClass, seed: u64, camera: &mut Camera) {
        // A run abandoned mid-play (retry from the pause menu) still gets a
        // history line; no-op if there is no live run or it was already saved.
        self.finalize_run_record();

        let mut state = GameState::new(class, seed);

        // Initialize AI actors
        state.initialize_ai(&mut self.events);

        // Spawn campfire in starting room near wizard.
        //
        // The candidates below are centre-offsets, and in a small starting room
        // an offset of 2 lands on the room edge — frequently right in the
        // doorway. A campfire carries CausesBurning but not BlocksMovement, so
        // one sitting there is not an obstacle you route around, it is a tile
        // that sets you alight on the way out. Skip any candidate in a
        // doorway's approach, and fall back to scanning the room interior
        // rather than silently leaving the starting room dark.
        if let Some(starting_room) = state.grid.starting_room {
            let player_start = state.player_start_position().map(|(x, y)| (x as i32, y as i32));
            if let Some((x, y)) = pick_campfire_spot(&state.grid, &starting_room, player_start) {
                spawning::spawn_campfire(&mut state.world, x, y);
            }
        }

        // Set up camera to track player (center of tile, not corner)
        if let Some((x, y)) = state.player_start_position() {
            camera.set_tracking_target(glam::Vec2::new(x + 0.5, y + 0.5));
        }

        let mut ui_state = GameUiState::new(state.player_entity);

        // Auto-fill the main hotbar with the player's abilities (class, then
        // secondary, then ranger, in order) so they're usable right away.
        {
            let mut entries: Vec<AbilityType> = Vec::new();
            if let Ok(a) = state.world.get::<&ClassAbility>(state.player_entity) {
                entries.push(a.ability_type);
            }
            if let Ok(a) = state.world.get::<&SecondaryAbility>(state.player_entity) {
                entries.push(a.ability_type);
            }
            if let Ok(ra) = state.world.get::<&RangerAbilities>(state.player_entity) {
                for (at, _, _) in ra.abilities.iter() {
                    entries.push(*at);
                }
            }
            // Spell-list abilities (the Necromancer starts with Raise Dead).
            if let Ok(la) = state
                .world
                .get::<&crate::components::LearnedAbilities>(state.player_entity)
            {
                for spell in la.spells.iter() {
                    entries.push(spell.ability);
                }
            }
            for (slot, ability) in ui_state.hotbar_main.iter_mut().zip(entries) {
                *slot = Some(crate::ui::HotbarEntry::Ability(ability));
            }

            // Everyone gets Rest, bound to the R slot of the Q/E/R hotbar,
            // and Sleep on the E slot.
            ui_state.hotbar_qer[1] =
                Some(crate::ui::HotbarEntry::Ability(AbilityType::Sleep));
            ui_state.hotbar_qer[2] =
                Some(crate::ui::HotbarEntry::Ability(AbilityType::Rest));
        }

        self.state = Some(state);
        self.ui_state = Some(ui_state);
        self.game_mode = GameMode::Playing;
        self.resting = false;
        self.rest_accumulator = 0.0;
        self.sleeping = false;
        self.sleep_accumulator = 0.0;
    }

    /// Check if we're currently playing (not on start screen).
    pub fn is_playing(&self) -> bool {
        self.game_mode == GameMode::Playing
    }

    /// Tear down the current run and return to the class selection screen.
    pub fn return_to_start_screen(&mut self) {
        // Record a retreat-to-menu as an abandoned run (no-op if the death
        // was already recorded when the game-over screen appeared).
        self.finalize_run_record();

        self.state = None;
        self.ui_state = None;
        self.input = InputState::new();
        self.vfx = VfxManager::new();
        self.events = EventQueue::new();
        self.game_mode = GameMode::StartScreen;
        self.resting = false;
        self.rest_accumulator = 0.0;
        self.sleeping = false;
        self.sleep_accumulator = 0.0;

        // Refresh the history panel; the seed mode and any custom seed text
        // are kept so a typed seed survives a retreat to the menu.
        self.past_runs = crate::run_history::load_recent(usize::MAX);
    }

    /// Append the current run to `runs_history.jsonl` if it hasn't been
    /// recorded yet. Called on death, on retreat-to-menu, and before a retry
    /// replaces the state. Safe to call repeatedly.
    fn finalize_run_record(&mut self) {
        let Some(state) = self.state.as_mut() else {
            return;
        };
        if state.run_recorded {
            return;
        }
        state.run_recorded = true;

        let dead = state
            .world
            .get::<&Health>(state.player_entity)
            .map(|h| h.is_dead())
            .unwrap_or(false);
        let cause_of_death = if dead {
            state
                .world
                .get::<&crate::components::LastDamageSource>(state.player_entity)
                .map(|s| s.0.clone())
                .unwrap_or_else(|_| "unknown".to_string())
        } else {
            "abandoned".to_string()
        };

        let record = crate::run_history::RunRecord {
            timestamp: crate::run_history::unix_now_secs(),
            seed: state.seed,
            class: state.player_class.name().to_string(),
            floor_reached: state.current_floor,
            game_time_survived_secs: state.game_clock.time,
            kills: state.kills,
            cause_of_death,
        };
        crate::run_history::append_run(&record);
        // Keep the in-memory panel current without re-reading the file.
        self.past_runs.insert(0, record);
        self.past_runs
            .truncate(crate::run_history::PAST_RUNS_SHOWN);
    }

    /// Handle the Escape key. Escape never quits the game directly; instead it
    /// backs out of whatever is open (targeting, dev menu, UI windows), and if
    /// nothing is open it opens the pause menu. From the pause menu it resumes;
    /// from the start screen it quits.
    fn handle_escape(&mut self) -> Option<WindowAction> {
        match self.game_mode {
            GameMode::Paused => {
                // Resume play; drop any keys held during the menu.
                self.game_mode = GameMode::Playing;
                self.input.keys_pressed.clear();
                None
            }
            GameMode::Playing => {
                // 1. Cancel an in-progress targeting prompt.
                if self.input.is_targeting() {
                    self.input.cancel_targeting();
                    return None;
                }
                // 2. Close the dev menu.
                if self.dev_menu.visible {
                    self.dev_menu.visible = false;
                    return None;
                }
                // 3. Close any open game UI window (inventory, dialogue, shop, loot).
                if let Some(ui) = self.ui_state.as_mut() {
                    if ui.close_open_menus() {
                        return None;
                    }
                }
                // 4. Nothing open — open the pause menu.
                self.game_mode = GameMode::Paused;
                self.pause_selected = 0;
                self.input.keys_pressed.clear();
                None
            }
            // From the start screen, Escape quits.
            GameMode::StartScreen => Some(WindowAction::Exit),
            // The game-over screen has its own buttons; ignore Escape.
            GameMode::GameOver => None,
        }
    }

    /// Handle a window event.
    /// Returns a WindowAction if the engine wants the window to do something.
    pub fn handle_event(
        &mut self,
        event: &WindowEvent,
        camera: &mut Camera,
        egui_consumed: bool,
    ) -> Option<WindowAction> {
        match event {
            WindowEvent::KeyboardInput { event: key_event, .. } if !egui_consumed => {
                if let PhysicalKey::Code(key) = key_event.physical_key {
                    match key_event.state {
                        ElementState::Pressed => {
                            if key == KeyCode::Escape {
                                return self.handle_escape();
                            }
                            if key == KeyCode::Backquote {
                                self.dev_menu.toggle();
                            }
                            self.input.keys_pressed.insert(key);
                        }
                        ElementState::Released => {
                            self.input.keys_pressed.remove(&key);
                        }
                    }
                }
            }
            WindowEvent::CursorMoved { position, .. } => {
                self.input.last_mouse_pos = self.input.mouse_pos;
                self.input.mouse_pos = (position.x as f32, position.y as f32);
            }
            WindowEvent::MouseInput { state: btn_state, button, .. } => {
                if !egui_consumed && *button == MouseButton::Left {
                    let was_down = self.input.mouse_down;
                    self.input.mouse_down = *btn_state == ElementState::Pressed;

                    if *btn_state == ElementState::Pressed {
                        self.input.mouse_down_pos = self.input.mouse_pos;
                        camera.start_pan(self.input.mouse_pos.0, self.input.mouse_pos.1);
                    } else if *btn_state == ElementState::Released {
                        camera.release_pan();
                        let dx = self.input.mouse_pos.0 - self.input.mouse_down_pos.0;
                        let dy = self.input.mouse_pos.1 - self.input.mouse_down_pos.1;
                        let was_drag = was_down
                            && (dx.abs() > crate::constants::CLICK_DRAG_THRESHOLD
                                || dy.abs() > crate::constants::CLICK_DRAG_THRESHOLD);

                        if !was_drag && self.is_playing() {
                            if self.input.is_targeting() {
                                self.input.pending_left_click = true;
                            } else if self.dev_menu.has_active_tool() {
                                self.handle_dev_spawn(camera);
                            } else if let Some(ref state) = self.state {
                                input::handle_click_to_move(
                                    &mut self.input,
                                    camera,
                                    &state.world,
                                    &state.grid,
                                    state.player_entity,
                                );
                            }
                        }
                    }
                }
                if !egui_consumed && *button == MouseButton::Right
                    && *btn_state == ElementState::Released {
                        if self.input.is_targeting() {
                            self.input.cancel_targeting();
                        } else {
                            self.input.pending_right_click = true;
                        }
                    }
            }
            WindowEvent::MouseWheel { delta, .. } if !egui_consumed => {
                let scroll = match delta {
                    MouseScrollDelta::LineDelta(_, y) => *y * 2.0,
                    MouseScrollDelta::PixelDelta(pos) => pos.y as f32 * 0.1,
                };
                camera.add_zoom_impulse(scroll, self.input.mouse_pos.0, self.input.mouse_pos.1);
            }
            _ => {}
        }
        None
    }

    /// Process a frame tick - advances simulation, returns render data.
    /// Returns empty results if not in playing mode.
    pub fn tick(&mut self, dt: f32, camera: &mut Camera) -> TickResult {
        profile_function!();

        // Accumulate real time for animations
        self.real_time += dt;

        // Light flicker only advances while the dungeon is live, so the lit
        // scene behind a menu is as frozen as the simulation. See the field's
        // doc comment for why this is real time and not the game clock.
        if self.game_mode == GameMode::Playing {
            self.light_flicker_time += dt;
        }

        // Transition to the game over screen once the player dies. State is kept
        // alive so the dungeon keeps rendering (frozen) behind the retry overlay.
        if self.game_mode == GameMode::Playing {
            if let Some(state) = &self.state {
                let player_dead = state
                    .world
                    .get::<&Health>(state.player_entity)
                    .map(|h| h.is_dead())
                    .unwrap_or(false);
                if player_dead {
                    self.game_mode = GameMode::GameOver;
                    // Write this run's history line as soon as death is
                    // detected (the game-over screen shows the same stats).
                    self.finalize_run_record();
                }
            }
        }

        // Only run game simulation when playing
        if self.state.is_none() {
            camera.update(dt, self.input.mouse_down);
            return TickResult {
                entities: vec![],
                window_action: None,
            };
        }

        // Game-clock time before this frame's simulation, so fire can advance in
        // game-time (and freeze when paused/idle).
        let clock_t0 = self.state.as_ref().map(|s| s.game_clock.time).unwrap_or(0.0);

        // Handle input first (needs full &mut self access). Skip entirely when
        // not actively playing (e.g. paused) so the turn-based simulation stays
        // frozen behind the menu.
        let input_result = if self.game_mode == GameMode::Playing {
            profile_scope!("process_input");
            self.process_input(camera)
        } else {
            InputResult::default()
        };
        let mut window_action = None;
        if input_result.toggle_fullscreen {
            window_action = Some(WindowAction::ToggleFullscreen);
        }

        // Fast-forward the simulation while resting (a few Wait-steps per frame).
        {
            profile_scope!("update_rest");
            self.update_rest(dt);
        }

        // Fast-forward the simulation while sleeping (like rest, but recovers
        // fatigue and wakes on damage / nearby enemies).
        {
            profile_scope!("update_sleep");
            self.update_sleep(dt);
        }

        // Now extract state references for the rest
        let state = self.state.as_mut().expect("State checked above");
        let _ui_state = self.ui_state.as_mut().expect("UI state should exist when state exists");

        // Update animations
        {
            profile_scope!("animations");
            systems::update_lunge_animations(&mut state.world, dt);
            systems::update_hit_flashes(&mut state.world, dt);
            self.vfx.update(dt);
        }

        // Remove dead entities (loot rolls draw from the seeded game rng);
        // hostile deaths feed the run's kill counter.
        {
            profile_scope!("remove_dead");
            let floor = state.current_floor;
            state.kills +=
                systems::remove_dead_entities(&mut state.actor_ctx(&mut self.events), floor);
        }

        // Process events from remove_dead_entities
        let event_result = {
            profile_scope!("process_events");
            process_events(&mut self.sim_ctx().expect("State checked above"))
        };

        if let Some(direction) = event_result.floor_transition {
            self.handle_floor_transition(direction, camera);
        }
        if event_result.should_interrupt_path() {
            self.input.clear_path();
        }

        self.apply_deferred_spawns(&event_result);

        // Re-borrow state after the floor transition (which may have replaced it)
        let state = self.state.as_mut().expect("State should still exist after floor transition");

        // Visual lerping
        {
            profile_scope!("visual_lerp");
            systems::visual_lerp(&mut state.world, dt);
            systems::lerp_projectiles_realtime(
                &mut state.world,
                dt,
                crate::constants::ARROW_SPEED,
            );
        }

        // Projectile cleanup
        let (finished, arrow_recovery_info) = systems::cleanup_finished_projectiles(&state.world);
        systems::despawn_projectiles(&mut state.world, finished);

        // Spawn recoverable arrows on the ground
        // Missed arrows are always recovered; arrows that hit have 50% chance
        for ((x, y), hit_enemy) in arrow_recovery_info {
            let should_recover = if hit_enemy {
                state.rng.gen::<f32>() < crate::constants::ARROW_RECOVERY_CHANCE_ON_HIT
            } else {
                true // Missed arrows always recoverable
            };
            if should_recover {
                systems::inventory::spawn_ground_item(&mut state.world, x, y, crate::components::ItemInstance::plain(crate::components::ItemType::Arrow));
            }
        }

        // Update camera tracking (center of tile, not corner)
        if let Ok(vis_pos) = state.world.get::<&crate::components::VisualPosition>(state.player_entity) {
            camera.set_tracking_target(glam::Vec2::new(vis_pos.x + 0.5, vis_pos.y + 0.5));
        }

        // Hand over the shakes this frame's events asked for. The camera
        // clamps and decays them; nothing here knows what a shake looks like.
        for request in self.vfx.take_shake_requests() {
            camera.apply_shake_request(&request);
        }

        // Update camera.
        //
        // `dt` is REAL frame time, and camera shake deliberately rides on it
        // rather than on the game-time accumulator the rest of this tick uses.
        // CLAUDE.md's "never tick game state off real time" rule does not
        // apply, because a shake is presentation and not state: nothing about
        // the simulation can be reached from it. Pacing it by game time would
        // freeze it mid-rattle every time the clock stopped to wait for input
        // — which is exactly when the player is looking at it — so please do
        // not "fix" this to game time. Hit flashes ride on `dt` for the same
        // reason; see `systems::animation::update_hit_flashes`.
        camera.update(dt, self.input.mouse_down);

        // Advance spreading fire (burnout + grass/creature ignition), paced by
        // the game-time elapsed this frame. May revert burnt grass to floor and
        // set fov_dirty, which the FOV update below then picks up.
        {
            profile_scope!("tick_fire");
            let game_dt = state.game_clock.time - clock_t0;
            systems::fire::tick_fire(
                &mut EffectCtx {
                    world: &mut state.world,
                    grid: &mut state.grid,
                    spatial: &mut state.spatial_cache,
                    events: &mut self.events,
                    rng: &mut state.rng,
                },
                game_dt,
                &mut state.fire_accumulator,
                &mut state.fov_dirty,
            );

            // Passive identification of carried/equipped items, paced by the
            // same game-time delta (frozen while paused/idle).
            systems::identify::tick_identification(
                &mut state.world,
                state.player_entity,
                game_dt,
                &mut state.identify_accumulator,
                &mut self.events,
            );

            // Survival clock: hunger drains and fatigue grows in game-time
            // (so both race ahead during rest/sleep fast-forward).
            let survival = systems::survival::tick_survival(
                &mut state.world,
                state.player_entity,
                game_dt,
                &mut state.survival_accumulator,
                systems::survival::SurvivalContext {
                    resting: self.resting,
                    sleeping: self.sleeping,
                },
                &mut self.events,
            );

            // Starvation damage interrupts rest and wakes a sleeper (inlined
            // rather than via stop_rest/stop_sleep: `state` holds a field
            // borrow of self here, so re-borrow ui_state directly).
            if survival.starvation_damage {
                if self.sleeping {
                    self.sleeping = false;
                    self.sleep_accumulator = 0.0;
                    let _ = state
                        .world
                        .remove_one::<crate::components::Asleep>(state.player_entity);
                    self.vfx.clear_resting_bubble();
                    if let Some(ui) = self.ui_state.as_mut() {
                        ui.message_log
                            .system("You are jolted awake by gnawing hunger!");
                    }
                } else if self.resting {
                    self.resting = false;
                    self.rest_accumulator = 0.0;
                    self.vfx.clear_resting_bubble();
                    if let Some(ui) = self.ui_state.as_mut() {
                        ui.message_log
                            .system("Your rest is broken by gnawing hunger!");
                    }
                }
            }
        }

        // Update visibility based on LOS and illumination (only when game state changed)
        if state.fov_dirty {
            {
                profile_scope!("fov_update");
                systems::update_fov(
                    &state.world,
                    &mut state.grid,
                    state.player_entity,
                    crate::constants::FOV_RADIUS,
                    state.game_clock.time,
                );
            }

            // Calculate per-tile illumination (must be after FOV update)
            {
                profile_scope!("illumination");
                systems::calculate_illumination(
                    &state.world,
                    &mut state.grid,
                    state.player_entity,
                    crate::constants::FOV_RADIUS,
                );
            }

            state.fov_dirty = false;
        }

        // The spatial cache is now the single source of truth for "is this tile
        // blocked" — both AI pathfinding and player/entity movement read it. It
        // is maintained incrementally, so any spawn/move/despawn path that
        // forgets to update it silently reintroduces phantom blockers. Verify
        // it against a fresh rebuild once per tick; compiled out in release.
        //
        // The check compares per-tile blocker *counts*, so a tile two entities
        // share has to be accounted for twice — an opened coffin and the
        // skeleton standing on it, say.
        #[cfg(debug_assertions)]
        state
            .spatial_cache
            .assert_coherent_with_world(&state.world, "engine tick");

        // First-sighting boss announcements ("... glares at you!"). Cheap:
        // at most one boss per floor; the event is picked up by the message
        // log on the next event-processing pass.
        systems::ai::announce_boss_sightings(&mut state.world, &state.grid, &mut self.events);

        // Collect renderables
        let entities = {
            profile_scope!("collect_renderables");
            systems::collect_renderables(
                &state.world,
                &state.grid,
                state.player_entity,
                self.real_time,
            )
        };

        TickResult {
            entities,
            window_action,
        }
    }

    /// Process UI actions from the UI layer.
    /// Does nothing if not playing.
    pub fn process_ui_actions(&mut self, actions: &UiActions) {
        let ui_result = {
            let Some((mut ctx, dev_menu)) = self.sim_ctx_and_dev_menu() else { return };
            process_ui_actions(&mut ctx, actions, dev_menu)
        };

        // Handle ability activation from a hotbar slot
        if let Some(ability_type) = actions.ability_to_use {
            self.try_use_ability(ability_type);
        }

        let ui_state = self.ui_state.as_mut().expect("UI state should exist");
        // Apply UI state changes
        if let Some(targeting) = ui_result.enter_targeting {
            self.input.targeting_mode = Some(targeting);
        }
        // A freshly studied spell goes straight onto the first free hotbar
        // slot (main bar first, then shift bar) so it's immediately usable.
        if let Some(ability) = ui_result.learned_ability {
            let entry = Some(crate::ui::HotbarEntry::Ability(ability));
            if let Some(slot) = ui_state
                .hotbar_main
                .iter_mut()
                .chain(ui_state.hotbar_shift.iter_mut())
                .find(|s| s.is_none())
            {
                *slot = entry;
            }
        }
        if ui_result.close_inventory {
            ui_state.show_inventory = false;
        }
        if ui_result.close_context_menu {
            ui_state.close_context_menu();
        }
        if ui_result.close_chest {
            ui_state.close_chest();
        }
        if ui_result.close_dialogue {
            ui_state.close_dialogue();
        }
        if ui_result.close_shop {
            ui_state.close_shop();
        }
        if ui_result.close_altar {
            ui_state.close_altar();
        }
    }

    /// Get the grid for rendering (returns None if not playing).
    pub fn grid(&self) -> Option<&crate::grid::Grid> {
        self.state.as_ref().map(|s| &s.grid)
    }

    /// Get VFX effects for rendering.
    pub fn vfx_effects(&self) -> &[VisualEffect] {
        &self.vfx.effects
    }

    /// Get fire effects for rendering.
    pub fn fires(&self) -> &[FireEffect] {
        &self.vfx.fires
    }

    /// Get the current game time (0.0 if not playing).
    #[allow(dead_code)] // Public API for external callers
    pub fn game_time(&self) -> f32 {
        self.state.as_ref().map(|s| s.game_clock.time).unwrap_or(0.0)
    }

    /// Get the targeting mode if active.
    #[allow(dead_code)] // Public API for external callers
    pub fn targeting_mode(&self) -> Option<&TargetingMode> {
        self.input.targeting_mode.as_ref()
    }

    /// Get mouse position.
    #[allow(dead_code)] // Public API for external callers
    pub fn mouse_pos(&self) -> (f32, f32) {
        self.input.mouse_pos
    }

    /// Get the player entity (returns None if not playing).
    #[allow(dead_code)] // Public API for external callers
    pub fn player_entity(&self) -> Option<Entity> {
        self.state.as_ref().map(|s| s.player_entity)
    }

    /// Get a reference to the ECS world (returns None if not playing).
    #[allow(dead_code)] // Public API for external callers
    pub fn world(&self) -> Option<&hecs::World> {
        self.state.as_ref().map(|s| &s.world)
    }

    /// Should show grid lines?
    pub fn show_grid_lines(&self) -> bool {
        self.ui_state.as_ref().map(|u| u.show_grid_lines).unwrap_or(false)
    }

    /// Get player position for lighting (uses visual position for smooth lighting).
    pub fn player_visual_pos(&self) -> (f32, f32) {
        self.state.as_ref().and_then(|s| {
            s.world.get::<&crate::components::VisualPosition>(s.player_entity).ok()
                .map(|vp| (vp.x + 0.5, vp.y + 0.5))  // Center of tile
        }).unwrap_or((0.0, 0.0))
    }

    /// Get player light radius (FOV radius), breathing very slightly so the
    /// edge of the visible area does not read as a circle drawn on the floor.
    ///
    /// This feeds the tile shader's brightness falloff only - the actual FOV
    /// used for visibility and gameplay comes from
    /// [`crate::constants::FOV_RADIUS`] directly, so the flicker here cannot
    /// change what the player can see.
    pub fn player_light_radius(&self) -> f32 {
        let flicker = crate::render::flicker_offset(
            self.light_flicker_time,
            crate::constants::LIGHT_PLAYER_FLICKER_PHASE,
            crate::constants::LIGHT_FLICKER_SCALE_PLAYER,
        );
        crate::constants::FOV_RADIUS as f32
            * (1.0 + flicker * crate::constants::LIGHT_FLICKER_RADIUS_AMPLITUDE)
    }

    /// Tint of the player's own light.
    pub fn player_light_color(&self) -> (f32, f32, f32) {
        crate::constants::LIGHT_COLOR_PLAYER
    }

    /// Collect light sources for rendering, sorted by distance to player.
    ///
    /// Each light arrives already coloured and already flickered: the two-sine
    /// modulation is evaluated here, once per light per frame, rather than per
    /// fragment in the shader. See [`crate::render::SceneLight`] for why.
    pub fn light_sources(&self) -> Vec<crate::render::SceneLight> {
        use crate::constants::*;

        let Some(ref state) = self.state else {
            return Vec::new();
        };

        // Get player position for sorting
        let player_pos = state.world
            .get::<&crate::components::VisualPosition>(state.player_entity)
            .map(|vp| (vp.x, vp.y))
            .unwrap_or((0.0, 0.0));

        let time = self.light_flicker_time;
        let mut sources: Vec<crate::render::SceneLight> = state.world
            .query::<(&crate::components::Position, &crate::components::LightSource)>()
            .iter()
            .map(|(_, (pos, light))| {
                let flicker = crate::render::flicker_offset(time, light.phase, light.flicker);
                crate::render::SceneLight {
                    pos: (pos.x as f32 + 0.5, pos.y as f32 + 0.5),
                    // Clamped at zero so a hand-authored amplitude over 1.0
                    // cannot invert a light into a pool of darkness.
                    radius: (light.radius * (1.0 + flicker * LIGHT_FLICKER_RADIUS_AMPLITUDE))
                        .max(0.0),
                    intensity: (light.intensity
                        * (1.0 + flicker * LIGHT_FLICKER_INTENSITY_AMPLITUDE))
                        .max(0.0),
                    color: light.color_or_default(),
                }
            })
            .collect();

        // Sort by distance to player so nearby lights are prioritized
        // (MAX_SCENE_LIGHTS limit)
        sources.sort_by(|a, b| {
            let dist_a = (a.pos.0 - player_pos.0).powi(2) + (a.pos.1 - player_pos.1).powi(2);
            let dist_b = (b.pos.0 - player_pos.0).powi(2) + (b.pos.1 - player_pos.1).powi(2);
            dist_a.partial_cmp(&dist_b).unwrap_or(std::cmp::Ordering::Equal)
        });

        sources
    }

    /// Run the UI and return actions. This handles the borrowing internally.
    /// When on start screen, shows class selection. Returns start_game action if player clicks Start.
    pub fn run_ui(
        &mut self,
        egui_glow: &mut egui_glow::EguiGlow,
        window: &winit::window::Window,
        camera: &crate::camera::Camera,
        tileset: &crate::multi_tileset::MultiTileset,
        ui_icons: &crate::ui::UiIcons,
    ) -> crate::ui::UiActions {
        match self.game_mode {
            GameMode::StartScreen => {
                // Show class selection screen (with seed picker + stats panel)
                let start_result = crate::ui::run_start_screen(
                    egui_glow,
                    window,
                    tileset,
                    ui_icons,
                    &mut self.selected_class,
                    &mut self.seed_mode,
                    &mut self.seed_input,
                    &mut self.stats_open,
                    &self.past_runs,
                );

                // Return start_game action if player clicked Start
                crate::ui::UiActions {
                    start_game: start_result,
                    ..Default::default()
                }
            }
            GameMode::GameOver => {
                let stats = self
                    .state
                    .as_ref()
                    .map(|s| crate::ui::GameOverStats {
                        time_survived: s.game_clock.time,
                        floor: s.current_floor,
                        seed: s.seed,
                        kills: s.kills,
                        cause_of_death: s
                            .world
                            .get::<&crate::components::LastDamageSource>(s.player_entity)
                            .map(|src| src.0.clone())
                            .unwrap_or_else(|_| "unknown".to_string()),
                    })
                    .unwrap_or_default();

                let choice = crate::ui::run_game_over_screen(egui_glow, window, &stats);

                let mut actions = crate::ui::UiActions::default();
                match choice {
                    crate::ui::GameOverChoice::Retry => actions.retry_game = true,
                    crate::ui::GameOverChoice::MainMenu => actions.return_to_menu = true,
                    crate::ui::GameOverChoice::None => {}
                }
                actions
            }
            GameMode::Paused => {
                let choice =
                    crate::ui::run_pause_screen(egui_glow, window, &mut self.pause_selected);

                let mut actions = crate::ui::UiActions::default();
                match choice {
                    crate::ui::PauseChoice::Resume => {
                        self.game_mode = GameMode::Playing;
                        self.input.keys_pressed.clear();
                    }
                    crate::ui::PauseChoice::Retry => actions.retry_game = true,
                    crate::ui::PauseChoice::MainMenu => actions.return_to_menu = true,
                    crate::ui::PauseChoice::Exit => actions.exit_game = true,
                    crate::ui::PauseChoice::None => {}
                }
                actions
            }
            GameMode::Playing => {
                let state = self.state.as_ref().expect("State should exist when playing");
                let ui_state = self.ui_state.as_mut().expect("UI state should exist when playing");

                // Extract life drain beam data for rendering
                let life_drain_beams = crate::ui::get_life_drain_beam_data(
                    &state.world,
                    &self.vfx.life_drain_beams,
                );

                // Extract taming channel data for rendering
                let taming_beams = crate::ui::get_taming_beam_data(
                    &state.world,
                    &self.vfx.taming_beams,
                );

                crate::ui::run_ui(
                    egui_glow,
                    window,
                    ui_state,
                    &mut self.dev_menu,
                    crate::ui::UiFrame {
                        game: crate::ui::UiWorld {
                            world: &state.world,
                            player_entity: state.player_entity,
                            grid: &state.grid,
                        },
                        resources: crate::ui::UiResources { camera, tileset, icons: ui_icons },
                        overlays: crate::ui::UiOverlays {
                            vfx_effects: &self.vfx.effects,
                            resting_bubble: self.vfx.resting_bubble.as_ref(),
                            life_drain_beams: &life_drain_beams,
                            taming_beams: &taming_beams,
                            targeting_mode: self.input.targeting_mode.as_ref(),
                            ability_targeting_mode: self.input.ability_targeting_mode.as_ref(),
                        },
                        mouse_pos: self.input.mouse_pos,
                        game_time: state.game_clock.time,
                    },
                )
            }
        }
    }

    // --- Private methods ---

    /// Bundle the engine- and state-owned simulation fields into a [`SimCtx`].
    ///
    /// `None` on the start screen, where there is no `GameState`. The returned
    /// context borrows all of `self`, so build it in the narrowest scope that
    /// covers the simulation call and let it drop before touching `self` again.
    fn sim_ctx(&mut self) -> Option<SimCtx<'_>> {
        self.sim_ctx_and_dev_menu().map(|(ctx, _)| ctx)
    }

    /// Apply the spawns an event pass deferred to the engine: skeletons from
    /// opened coffins, companions from completed Raise Dead channels, and
    /// boss-summoned minions. Deferred because spawning needs `&mut` access to
    /// state the event loop is still reading.
    fn apply_deferred_spawns(&mut self, result: &TurnExecutionResult) {
        if result.skeleton_spawns.is_empty()
            && result.raised_skeletons.is_empty()
            && result.boss_minion_spawns.is_empty()
        {
            return;
        }
        let Some(mut ctx) = self.sim_ctx() else { return };

        // Skeletons from opened coffins
        for &(x, y) in &result.skeleton_spawns {
            let skeleton = spawning::enemies::SKELETON.spawn(ctx.world, x, y, ctx.rng);
            ctx.spatial.register_entity(skeleton, (x, y), true, false);
            initialization::initialize_single_ai_actor(&mut ctx.actors(), skeleton);
        }
        if !result.skeleton_spawns.is_empty() {
            // Close loot UI - player must deal with skeleton first
            ctx.ui.close_chest();
        }

        // Friendly skeletons from completed Raise Dead channels
        for &(x, y) in &result.raised_skeletons {
            spawn_raised_skeleton(&mut ctx.actors(), x, y);
        }

        // Boss-summoned spiderlings (Mother Silkrot's brood)
        for &(boss, (x, y)) in &result.boss_minion_spawns {
            spawn_boss_minion(&mut ctx.actors(), boss, x, y);
        }
    }

    /// [`Self::sim_ctx`] paired with the dev menu. The dev menu is engine-owned
    /// but isn't simulation state, so it stays out of `SimCtx`; this hands out
    /// both halves of the split borrow at once.
    fn sim_ctx_and_dev_menu(&mut self) -> Option<(SimCtx<'_>, &mut DevMenu)> {
        let state = self.state.as_mut()?;
        let ui = self.ui_state.as_mut()?;
        let ctx = SimCtx {
            world: &mut state.world,
            grid: &mut state.grid,
            player: state.player_entity,
            clock: &mut state.game_clock,
            scheduler: &mut state.action_scheduler,
            tracker: &mut state.active_ai_tracker,
            spatial: &mut state.spatial_cache,
            events: &mut self.events,
            rng: &mut state.rng,
            vfx: &mut self.vfx,
            ui,
            input: &mut self.input,
            audio: self.audio.as_ref(),
        };
        Some((ctx, &mut self.dev_menu))
    }

    /// Process input. Only called when playing (state must exist).
    fn process_input(&mut self, camera: &mut Camera) -> InputResult {
        let state = self.state.as_mut().expect("process_input called without state");
        let ui_state = self.ui_state.as_mut().expect("process_input called without ui_state");

        let frame = input::process_frame(
            &mut self.input,
            &state.world,
            &state.grid,
            camera,
            state.player_entity,
        );

        // UI toggles
        let result = InputResult {
            toggle_fullscreen: frame.toggle_fullscreen,
        };

        // While a dialogue is open, the dialogue window owns keyboard input
        // (navigated/confirmed in the egui layer). Swallow movement and other
        // game input so arrow/WASD keys don't move the player — which would also
        // auto-close the conversation. `frame` already consumed the key presses.
        if ui_state.talking_to.is_some() {
            return result;
        }

        // While sleeping, the simulation auto-advances (see update_sleep);
        // swallow normal input. Any movement, interaction, or fresh click
        // wakes the player up (as does activating Sleep again, via try_sleep).
        if self.sleeping {
            let interacted = frame.player_intent.is_some()
                || frame.enter_pressed
                || frame.toggle_inventory
                || frame.toggle_grid_lines
                || !self.input.player_path.is_empty();
            if interacted {
                self.input.clear_path();
                self.sleeping = false;
                self.sleep_accumulator = 0.0;
                let _ = state
                    .world
                    .remove_one::<crate::components::Asleep>(state.player_entity);
                self.vfx.clear_resting_bubble();
                ui_state.message_log.system("You wake up.");
            }
            return result;
        }

        // While resting, the simulation auto-advances (see update_rest); swallow
        // normal input. Any movement, interaction, or fresh click cancels rest.
        // (Activating Sleep from the hotbar supersedes rest via try_sleep.)
        if self.resting {
            let interacted = frame.player_intent.is_some()
                || frame.enter_pressed
                || frame.toggle_inventory
                || frame.toggle_grid_lines
                || !self.input.player_path.is_empty();
            if interacted {
                self.input.clear_path();
                self.resting = false;
                self.rest_accumulator = 0.0;
                self.vfx.clear_resting_bubble();
                ui_state.message_log.system("You stop resting.");
            }
            return result;
        }

        if frame.toggle_inventory {
            ui_state.toggle_inventory();
        }
        if frame.toggle_grid_lines {
            ui_state.toggle_grid_lines();
        }
        if frame.toggle_sneak {
            let player = state.player_entity;
            if state.world.get::<&crate::components::Sneaking>(player).is_ok() {
                let _ = state.world.remove_one::<crate::components::Sneaking>(player);
                ui_state.message_log.system("You stop sneaking.");
            } else {
                let _ = state.world.insert_one(player, crate::components::Sneaking);
                ui_state.message_log.system("You move into a crouch and start sneaking.");
            }
        }

        // Enter key: container interaction (chests, bones, ground items)
        if frame.enter_pressed {
            let container_action = crate::game::handle_enter_key_container(
                &mut state.world,
                state.player_entity,
                ui_state.open_chest,
                &mut self.events,
            );
            match container_action {
                crate::game::ContainerAction::TookAll(_) => {
                    ui_state.close_chest();
                    // Clean up empty ground item piles, and stop the container
                    // that was just emptied blocking its tile.
                    systems::cleanup_empty_ground_piles(&mut state.world);
                    systems::unblock_emptied_containers(
                        &mut state.world,
                        &mut state.spatial_cache,
                    );
                }
                crate::game::ContainerAction::Opened(_) => {
                    // Takes the whole of `self`, so the borrows above must end
                    // here; they are re-taken below.
                    let _ = process_events(
                        &mut self.sim_ctx().expect("process_input called with state"),
                    );
                }
                crate::game::ContainerAction::None => {}
            }
        }

        let state = self.state.as_mut().expect("process_input called without state");
        let ui_state = self.ui_state.as_mut().expect("process_input called without ui_state");

        // Player dead - just handle drag
        if frame.player_dead {
            input::process_mouse_drag(&mut self.input, camera, ui_state.show_inventory);
            return result;
        }

        // Abilities are activated from the hotbars (see ability_to_use in
        // process_ui_actions), not from dedicated keybinds anymore.

        // Execute player intent
        if let Some(intent) = frame.player_intent {
            if let Some(item_index) = frame.item_to_remove {
                systems::remove_item_from_inventory(
                    &mut state.world,
                    state.player_entity,
                    item_index,
                );
            }

            let turn_result = execute_player_intent(
                &mut self.sim_ctx().expect("process_input called with state"),
                intent,
            );

            let state = self.state.as_mut().expect("State should exist");

            match turn_result.turn_result {
                TurnResult::Started => {
                    // Mark FOV for recalculation since game state changed
                    state.fov_dirty = true;

                    if !frame.from_keyboard {
                        self.input.consume_step();
                        if self.input.has_arrived() {
                            self.input.clear_destination();
                            if let Some(container_id) = systems::find_container_at_player(
                                &state.world,
                                state.player_entity,
                            ) {
                                let container_type = state.world
                                    .get::<&crate::components::Container>(container_id)
                                    .ok()
                                    .map(|c| c.container_type);
                                let position = state.world
                                    .get::<&crate::components::Position>(container_id)
                                    .map(|p| (p.x, p.y))
                                    .unwrap_or((0, 0));
                                self.events.push(crate::events::GameEvent::ContainerOpened {
                                    container: container_id,
                                    opener: state.player_entity,
                                    container_type,
                                    position,
                                });
                            }
                        }
                    }
                    if let Some(direction) = turn_result.floor_transition {
                        self.handle_floor_transition(direction, camera);
                    }
                }
                TurnResult::Blocked | TurnResult::NotReady => {
                    self.input.clear_path();
                }
            }

            if turn_result.should_interrupt_path() {
                self.input.clear_path();
            }

            self.apply_deferred_spawns(&turn_result);
        }

        // Mouse drag for camera
        let show_inv = self.ui_state.as_ref().map(|u| u.show_inventory).unwrap_or(false);
        input::process_mouse_drag(&mut self.input, camera, show_inv);

        result
    }

    fn handle_dev_spawn(&mut self, camera: &Camera) {
        let Some(tool) = self.dev_menu.selected_tool else {
            return;
        };
        let Some(ref mut state) = self.state else {
            return;
        };

        let mouse_pos = self.input.mouse_pos;
        let needs_vfx = dev_spawning::spawn_at_cursor(
            &mut state.actor_ctx(&mut self.events),
            tool,
            mouse_pos,
            camera,
        );

        if needs_vfx {
            let world_pos = camera.screen_to_world(self.input.mouse_pos.0, self.input.mouse_pos.1);
            let tile_x = world_pos.x.round() as i32;
            let tile_y = world_pos.y.round() as i32;
            dev_spawning::spawn_vfx_for_tool(tool, tile_x, tile_y, &mut self.vfx);
        }
    }

    /// Try to use the player's class ability (called from UI button)
    /// Activate an ability by type, routing to whichever component holds it
    /// (class ability, secondary ability, or a ranger ability slot).
    fn try_use_ability(&mut self, ability_type: AbilityType) {
        // Rest and Sleep are universal abilities not tied to a class component.
        if ability_type == AbilityType::Rest {
            self.try_rest();
            return;
        }
        if ability_type == AbilityType::Sleep {
            self.try_sleep();
            return;
        }

        enum Route {
            Class,
            Secondary,
            Ranger(usize),
            Learned,
        }

        let route = {
            let Some(ref state) = self.state else {
                return;
            };
            let world = &state.world;
            let player = state.player_entity;

            if world
                .get::<&ClassAbility>(player)
                .map(|a| a.ability_type == ability_type)
                .unwrap_or(false)
            {
                Route::Class
            } else if world
                .get::<&SecondaryAbility>(player)
                .map(|a| a.ability_type == ability_type)
                .unwrap_or(false)
            {
                Route::Secondary
            } else if let Some(index) = world
                .get::<&RangerAbilities>(player)
                .ok()
                .and_then(|ra| ra.abilities.iter().position(|(at, _, _)| *at == ability_type))
            {
                Route::Ranger(index)
            } else if world
                .get::<&crate::components::LearnedAbilities>(player)
                .map(|la| la.knows(ability_type))
                .unwrap_or(false)
            {
                Route::Learned
            } else {
                return;
            }
        };

        self.try_use_slot(match route {
            Route::Class => AbilitySlot::Class,
            Route::Secondary => AbilitySlot::Secondary,
            Route::Ranger(index) => AbilitySlot::Ranger(index),
            Route::Learned => AbilitySlot::Learned(ability_type),
        });
    }

    /// Activate the ability in one of the player's four ability slots.
    fn try_use_slot(&mut self, slot: AbilitySlot) {
        let Some(mut ctx) = self.sim_ctx() else { return };
        activate_ability(&mut ctx, slot);
    }

    /// Toggle resting. Pressing Rest while already resting cancels it; otherwise
    /// it begins resting if it's safe and the player isn't already full.
    fn try_rest(&mut self) {
        if self.resting {
            self.stop_rest("You stop resting.");
            return;
        }

        let Some(ref mut state) = self.state else {
            return;
        };
        let Some(ref mut ui_state) = self.ui_state else {
            return;
        };

        // Only start resting while idle (not mid-action).
        let is_idle = state
            .world
            .get::<&Actor>(state.player_entity)
            .map(|a| a.current_action.is_none())
            .unwrap_or(false);
        if !is_idle {
            return;
        }

        if simulation::player_at_full_health(&state.world, state.player_entity) {
            ui_state.message_log.system("You are already at full health.");
            return;
        }
        if simulation::any_enemy_alerted(&state.world, state.player_entity) {
            ui_state.message_log.system("You can't rest with enemies nearby.");
            return;
        }
        // Hungry stops natural regen, so resting can't heal (a Regeneration
        // potion still works and is checked separately in update_rest).
        if player_too_hungry_to_recover(&state.world, state.player_entity) {
            ui_state
                .message_log
                .system("You are too hungry to recover — find something to eat.");
            return;
        }

        // Begin resting and show the Zzz bubble. Healing happens purely from the
        // game's normal time-based regen as the Wait steps advance the clock.
        self.resting = true;
        self.rest_accumulator = 0.0;
        if let Ok(pos) = state.world.get::<&crate::components::Position>(state.player_entity) {
            self.vfx.set_resting_bubble(pos.x as f32 + 0.5, pos.y as f32 + 0.5);
        }
        ui_state.message_log.system("You settle down to rest...");
    }

    /// End resting, clear its boost and bubble, and log `message`.
    fn stop_rest(&mut self, message: &str) {
        if !self.resting {
            return;
        }
        self.resting = false;
        self.rest_accumulator = 0.0;
        self.vfx.clear_resting_bubble();
        if let Some(ref mut ui_state) = self.ui_state {
            ui_state.message_log.system(message);
        }
    }

    /// Toggle sleeping. Activating Sleep while already asleep wakes up;
    /// otherwise it begins sleeping if it's safe and the player is tired at
    /// all. While asleep the player carries the `Asleep` marker: enemies get
    /// the sneak-attack multiplier on them and notice them far more easily.
    fn try_sleep(&mut self) {
        if self.sleeping {
            self.stop_sleep("You wake up.");
            return;
        }

        // Decide whether sleep can start while the state borrow is confined to
        // this block: stop_rest below takes &mut self.
        let refusal: Option<&'static str> = {
            let Some(ref state) = self.state else {
                return;
            };
            if self.ui_state.is_none() {
                return;
            }

            // Only start sleeping while idle (not mid-action).
            let is_idle = state
                .world
                .get::<&Actor>(state.player_entity)
                .map(|a| a.current_action.is_none())
                .unwrap_or(false);
            if !is_idle {
                return;
            }

            let fatigue = state
                .world
                .get::<&crate::components::Fatigue>(state.player_entity)
                .map(|f| f.value)
                .unwrap_or(0.0);
            if fatigue <= 0.0 {
                Some("You don't feel tired.")
            } else if simulation::any_enemy_alerted(&state.world, state.player_entity) {
                Some("You can't sleep with enemies nearby.")
            } else {
                None
            }
        };

        if let Some(message) = refusal {
            if let Some(ref mut ui_state) = self.ui_state {
                ui_state.message_log.system(message);
            }
            return;
        }

        // Sleep supersedes rest. Go through stop_rest rather than clearing the
        // flags here: the direct assignment skipped the "You stop resting." log
        // line and the clear_resting_bubble() call, and would skip anything
        // either path grows later. It no-ops when not resting.
        self.stop_rest("You stop resting.");

        let Some(ref mut state) = self.state else {
            return;
        };
        let Some(ref mut ui_state) = self.ui_state else {
            return;
        };

        // You can't stay crouched while unconscious.
        let _ = state
            .world
            .remove_one::<crate::components::Sneaking>(state.player_entity);

        // Begin sleeping: the Asleep marker makes the player sneak-attackable
        // and easy to notice (see combat::apply_damage and systems::ai).
        self.sleeping = true;
        self.sleep_accumulator = 0.0;
        let _ = state
            .world
            .insert_one(state.player_entity, crate::components::Asleep);
        if let Ok(pos) = state.world.get::<&crate::components::Position>(state.player_entity) {
            self.vfx.set_resting_bubble(pos.x as f32 + 0.5, pos.y as f32 + 0.5);
        }
        ui_state.message_log.system("You lie down and drift off to sleep...");
    }

    /// End sleeping, remove the Asleep marker and bubble, and log `message`.
    fn stop_sleep(&mut self, message: &str) {
        if !self.sleeping {
            return;
        }
        self.sleeping = false;
        self.sleep_accumulator = 0.0;
        if let Some(ref mut state) = self.state {
            let _ = state
                .world
                .remove_one::<crate::components::Asleep>(state.player_entity);
        }
        self.vfx.clear_resting_bubble();
        if let Some(ref mut ui_state) = self.ui_state {
            ui_state.message_log.system(message);
        }
    }


    /// Advance the rest fast-forward by one frame's worth of game-time. Stops
    /// when the player is fully healed, an enemy becomes alerted, or hunger has
    /// shut natural regen off (so resting could never finish).
    fn update_rest(&mut self, dt: f32) {
        self.fast_forward(TimeSkip::Rest, dt, |world, player| {
            if simulation::player_at_full_health(world, player) {
                return Some("You finish resting, fully recovered.");
            }
            if simulation::any_enemy_alerted(world, player) {
                return Some("Your rest is interrupted!");
            }
            // Hunger dropped below the threshold mid-rest: healing has
            // stopped, so don't spin (and starve) forever.
            if player_too_hungry_to_recover(world, player) {
                return Some("You are too hungry to keep resting.");
            }
            None
        });
    }

    /// Advance the sleep fast-forward by one frame's worth of game-time.
    /// Fatigue itself recovers in the survival tick, which is paced by the
    /// game-time these steps generate. Stops when fatigue reaches zero or an
    /// enemy wakes/spots the sleeper; `fast_forward` adds sleep's wake-on-damage
    /// check.
    fn update_sleep(&mut self, dt: f32) {
        self.fast_forward(TimeSkip::Sleep, dt, |world, player| {
            // Fully recovered?
            let fatigue = world
                .get::<&crate::components::Fatigue>(player)
                .map(|f| f.value)
                .unwrap_or(0.0);
            if fatigue <= 0.0 {
                return Some("You wake up feeling refreshed.");
            }
            if simulation::any_enemy_alerted(world, player) {
                return Some("You are jolted awake — something has noticed you!");
            }
            None
        });
    }

    /// Fast-forward the simulation one frame's worth of game-time, in small
    /// Wait increments so motion stays animated. Shared by rest and sleep:
    /// same real-time pacing against `REST_TIME_SCALE`, same Wait-step loop,
    /// same Raise-Dead spawn handling, same bubble repin.
    ///
    /// `stop_reason` is checked before each step and returns the message to end
    /// on, or `None` to keep going. Sleep additionally waits for a point of
    /// energy before each step and wakes on any HP drop.
    fn fast_forward(
        &mut self,
        mode: TimeSkip,
        dt: f32,
        stop_reason: impl Fn(&hecs::World, Entity) -> Option<&'static str>,
    ) {
        if !mode.active(self) {
            return;
        }

        // How many discrete Wait-steps to run this frame, paced by real time.
        let accumulator = mode.accumulator_mut(self);
        *accumulator += dt * REST_TIME_SCALE;
        let mut steps = (*accumulator / crate::constants::ACTION_WAIT_DURATION) as i32;
        if steps <= 0 {
            return;
        }
        steps = steps.min(REST_MAX_STEPS_PER_FRAME);
        *accumulator -= steps as f32 * crate::constants::ACTION_WAIT_DURATION;

        for _ in 0..steps {
            let Some(ref state) = self.state else {
                mode.clear_flag(self);
                return;
            };

            // Stop conditions checked before each step.
            if let Some(message) = stop_reason(&state.world, state.player_entity) {
                mode.stop(self, message);
                return;
            }

            // Sleep only: exhaustion can leave the player at 0 energy (no regen
            // while awake); Wait needs the actor able to act, so wait for a
            // point first (regen works while asleep).
            if mode == TimeSkip::Sleep {
                let energy = self
                    .state
                    .as_ref()
                    .and_then(|s| s.world.get::<&Actor>(s.player_entity).ok().map(|a| a.energy))
                    .unwrap_or(0);
                if energy <= 0 {
                    let got = {
                        let Some(mut ctx) = self.sim_ctx() else {
                            mode.clear_flag(self);
                            return;
                        };
                        simulation::wait_for_energy(&mut ctx.actors(), 1)
                    };
                    if !got {
                        mode.stop(self, "You wake up.");
                        return;
                    }
                }
            }

            // Sleep only: any HP drop while asleep wakes the player (attacks,
            // burning, traps — starvation is handled separately in the survival
            // tick).
            let hp_before = if mode == TimeSkip::Sleep {
                self.player_hp()
            } else {
                0
            };

            let result = {
                let Some(mut ctx) = self.sim_ctx() else {
                    mode.clear_flag(self);
                    return;
                };
                let result = execute_player_intent(
                    &mut ctx,
                    crate::systems::player_input::PlayerIntent::Wait,
                );

                // A Raise Dead channel can tick over during the auto-Waits;
                // don't drop the skeleton on the floor.
                for &(x, y) in &result.raised_skeletons {
                    spawn_raised_skeleton(&mut ctx.actors(), x, y);
                }
                result
            };
            self.state.as_mut().expect("state checked above").fov_dirty = true;

            let hp_dropped = mode == TimeSkip::Sleep && self.player_hp() < hp_before;

            if result.turn_result != simulation::TurnResult::Started
                || result.enemy_spotted_player
                || result.player_took_damage
                || hp_dropped
            {
                mode.stop(self, mode.interrupt_message());
                return;
            }
        }

        // Keep the bubble pinned above the (stationary) player.
        if let Some(ref state) = self.state {
            if let Ok(pos) = state.world.get::<&crate::components::Position>(state.player_entity) {
                self.vfx.set_resting_bubble(pos.x as f32 + 0.5, pos.y as f32 + 0.5);
            }
        }
    }

    /// The player's current HP, or 0 if there is no live player.
    fn player_hp(&self) -> i32 {
        self.state
            .as_ref()
            .and_then(|s| s.world.get::<&Health>(s.player_entity).ok().map(|h| h.current))
            .unwrap_or(0)
    }

    fn handle_floor_transition(
        &mut self,
        direction: crate::events::StairDirection,
        camera: &mut Camera,
    ) {
        let Some(ref mut state) = self.state else {
            return;
        };

        if !can_transition_floor(state.current_floor, direction) {
            return;
        }

        // `handle_floor_transition` swaps the new grid into `state.grid` itself.
        let (current_floor, seed) = (state.current_floor, state.seed);
        let mut floors = std::mem::take(&mut state.floors);
        let result = handle_floor_transition(
            &mut state.actor_ctx(&mut self.events),
            &mut floors,
            current_floor,
            seed,
            direction,
        );
        state.floors = floors;

        state.current_floor = result.new_floor;
        state.fov_dirty = true; // New floor needs FOV calculation

        self.input.clear_path();

        camera.set_tracking_target(glam::Vec2::new(
            result.player_visual_pos.0 + 0.5,
            result.player_visual_pos.1 + 0.5,
        ));
    }
}

/// True when the player is Hungry (or worse) and has no Regenerating effect:
/// natural HP regen is stopped, so resting cannot heal them.
fn player_too_hungry_to_recover(world: &hecs::World, player: Entity) -> bool {
    let hungry = world
        .get::<&crate::components::Hunger>(player)
        .map(|h| h.is_hungry())
        .unwrap_or(false);
    hungry
        && !crate::systems::effects::entity_has_effect(
            world,
            player,
            crate::components::EffectType::Regenerating,
        )
}

/// Spawn a raised skeleton companion at a position (Raise Dead completion).
///
/// Uses the standard Skeleton stat block but wired into the tamed-companion
/// infrastructure: no hostile AI, walkable (no BlocksMovement), `TamedBy` +
/// `CompanionAI` for defensive follow behavior, plus the `RaisedUndead`
/// marker that counts against the caster's INT-scaled control cap.
/// Pick where the starting-room campfire goes, or `None` if the room has
/// nowhere safe for it.
///
/// Prefers a ring of centre-offsets (so the fire reads as placed, not dumped in
/// a corner) and falls back to scanning the room interior. Every candidate must
/// be walkable, outside any doorway's approach, and not the player's own tile.
fn pick_campfire_spot(
    grid: &crate::grid::Grid,
    room: &crate::dungeon_gen::Rect,
    player_start: Option<(i32, i32)>,
) -> Option<(i32, i32)> {
    let usable = |x: i32, y: i32| {
        grid.is_walkable(x, y) && !grid.blocks_a_doorway(x, y) && player_start != Some((x, y))
    };

    let (cx, cy) = room.center();
    let preferred = [
        (cx + 2, cy),
        (cx - 2, cy),
        (cx, cy + 2),
        (cx, cy - 2),
        (cx + 1, cy + 1),
    ];

    preferred
        .into_iter()
        .find(|&(x, y)| usable(x, y))
        .or_else(|| {
            // Nothing in the preferred ring works (small room, or doors on
            // several sides). Take any interior tile that does, rather than
            // leaving the starting room unlit.
            (1..room.height - 1)
                .flat_map(|dy| (1..room.width - 1).map(move |dx| (dx, dy)))
                .map(|(dx, dy)| (room.x + dx, room.y + dy))
                .find(|&(x, y)| usable(x, y))
        })
}

fn spawn_raised_skeleton(ctx: &mut ActorCtx, x: i32, y: i32) {
    let ActorCtx { world, player: owner, clock, scheduler, tracker: active_ai_tracker, spatial: spatial_cache, rng, .. } = ctx;
    let (world, owner) = (&mut **world, *owner);

    // The sleep roll is discarded right below (companions are never asleep) but
    // still draws, so it stays on the run's seeded stream like every other spawn.
    let skeleton = spawning::enemies::SKELETON.spawn(world, x, y, rng);

    // Convert the hostile stat block into an ally. Unlike a tamed animal, a
    // raised skeleton KEEPS BlocksMovement: it is a construct you put between
    // yourself and something else, so it has to body-block enemies. Its owner
    // can still walk through it (see `apply_move`'s companion exception).
    let _ = world.remove_one::<crate::components::ChaseAI>(skeleton);
    let _ = world.remove_one::<crate::components::Asleep>(skeleton);
    let _ = world.insert(
        skeleton,
        (
            crate::components::TamedBy { owner },
            crate::components::CompanionAI {
                owner,
                follow_distance: 2,
                threat_table: Vec::new(),
            },
            crate::components::RaisedUndead,
            // Sickly green tint so allied bones read differently from the
            // hostile skeletons they share a sprite with.
            crate::components::SpriteTint { r: 0.65, g: 1.0, b: 0.75 },
        ),
    );

    // Blocks movement (so it screens for its owner), never vision.
    spatial_cache.register_entity(skeleton, (x, y), true, false);

    // Track and schedule so the companion AI starts acting.
    active_ai_tracker.register_entity(skeleton);
    active_ai_tracker.mark_active(skeleton);
    scheduler.schedule(skeleton, clock.time + 0.1);
}

/// Spawn a boss-summoned Lesser Giant Spider at (x, y): hostile, already
/// alerted to the player, and counted against the boss's minion cap.
fn spawn_boss_minion(ctx: &mut ActorCtx, boss: Entity, x: i32, y: i32) {
    let player_entity = ctx.player;
    let spider = spawning::enemies::LESSER_GIANT_SPIDER.spawn(ctx.world, x, y, ctx.rng);
    let _ = ctx
        .world
        .insert_one(spider, crate::components::BossMinion { boss });

    // Summoned mid-fight: wide awake and already hunting the summoner's prey.
    let _ = ctx.world.remove_one::<crate::components::Asleep>(spider);
    let player_pos = ctx
        .world
        .get::<&crate::components::Position>(player_entity)
        .map(|p| (p.x, p.y))
        .ok();
    if let Ok(mut ai) = ctx
        .world
        .get::<&mut crate::components::ChaseAI>(spider)
    {
        ai.state = crate::components::AIState::Chasing;
        ai.add_threat(player_entity, crate::constants::WAKE_THREAT);
        if let Some(pos) = player_pos {
            ai.update_target_pos(player_entity, pos);
        }
    }

    ctx.spatial.register_entity(spider, (x, y), true, false);
    initialization::initialize_single_ai_actor(ctx, spider);
}

/// Which slot an ability activation came from.
///
/// The four slots resolve their ability and their cooldown differently, but the
/// activation spine around them - idle check, readiness, affordability,
/// targeting, wait for energy, start the action, advance time, process events -
/// is the same. [`activate_ability`] holds that spine; the slot supplies the
/// differences.
#[derive(Clone, Copy)]
enum AbilitySlot {
    /// The class ability (Cleave / Sprint / Tame / Life Drain).
    Class,
    /// The Druid/Necromancer-style second ability (Barkskin / Fear / Stun).
    Secondary,
    /// A spell studied from a scroll, plus the Necromancer's Raise Dead.
    Learned(AbilityType),
    /// One of the Ranger's four indexed abilities.
    Ranger(usize),
}

/// What the targeting check decided about an ability that is about to activate.
enum Targeting {
    /// No target needed - act now.
    Immediate,
    /// Enter targeting mode at this range; the click then flows through the
    /// normal intent path, and no game time passes here.
    Enter(i32),
    /// Cannot be used right now; feedback has already been logged.
    Refused,
}

impl AbilitySlot {
    /// The ability this slot holds and its energy cost, or `None` if the slot is
    /// empty, holds something else, or is still on cooldown.
    fn ready_ability(self, world: &hecs::World, player: Entity) -> Option<(AbilityType, i32)> {
        match self {
            AbilitySlot::Class => {
                let a = world.get::<&ClassAbility>(player).ok()?;
                a.is_ready()
                    .then(|| (a.ability_type, a.ability_type.energy_cost()))
            }
            AbilitySlot::Secondary => {
                let a = world.get::<&SecondaryAbility>(player).ok()?;
                a.is_ready()
                    .then(|| (a.ability_type, a.ability_type.energy_cost()))
            }
            AbilitySlot::Learned(ability_type) => {
                let la = world
                    .get::<&crate::components::LearnedAbilities>(player)
                    .ok()?;
                let spell = la.get(ability_type)?;
                (spell.cooldown_remaining <= 0.0)
                    .then(|| (ability_type, ability_type.energy_cost()))
            }
            AbilitySlot::Ranger(index) => {
                let ra = world.get::<&RangerAbilities>(player).ok()?;
                let &(ability_type, cooldown_remaining, _) = ra.get(index)?;
                (cooldown_remaining <= 0.0)
                    .then(|| (ability_type, ability_type.energy_cost()))
            }
        }
    }

    /// Whether this slot's `ability_type` needs a target picked first.
    ///
    /// Matched on the (slot, ability) pair rather than the ability alone, so a
    /// slot can only ever target the abilities it actually grants.
    fn targeting(self, ctx: &mut SimCtx, ability_type: AbilityType) -> Targeting {
        use crate::constants::*;
        let player = ctx.player;

        match (self, ability_type) {
            (AbilitySlot::Class, AbilityType::Tame) => Targeting::Enter(TAME_RANGE),
            (AbilitySlot::Class, AbilityType::LifeDrain) => Targeting::Enter(LIFE_DRAIN_RANGE),
            (AbilitySlot::Learned(_), AbilityType::RaiseDead) => {
                // Enforce the INT-scaled control cap up front, with feedback.
                let int = crate::queries::effective_stats(ctx.world, player).intelligence;
                let cap = raise_dead_cap(int);
                let active = systems::actions::raised_undead_count(ctx.world);
                if active >= cap {
                    ctx.ui.message_log.system(format!(
                        "You cannot control more than {} raised skeleton{} (1 + 1 per {} INT above 10).",
                        cap,
                        if cap == 1 { "" } else { "s" },
                        RAISE_DEAD_INT_PER_EXTRA,
                    ));
                    return Targeting::Refused;
                }
                Targeting::Enter(RAISE_DEAD_RANGE)
            }
            // Blink range is a spell magnitude: it scales with the caster's INT.
            (AbilitySlot::Learned(_), AbilityType::LearnedBlink) => {
                Targeting::Enter(systems::actions::scaled_blink_range(ctx.world, player))
            }
            (AbilitySlot::Learned(_), AbilityType::LearnedFireball) => {
                Targeting::Enter(FIREBALL_RANGE)
            }
            (AbilitySlot::Ranger(_), AbilityType::Tumble) => Targeting::Enter(TUMBLE_DISTANCE),
            (AbilitySlot::Ranger(_), AbilityType::SnareTrap) => Targeting::Enter(SNARE_TRAP_RANGE),
            (AbilitySlot::Ranger(_), AbilityType::CripplingShot) => Targeting::Enter(BOW_RANGE),
            _ => Targeting::Immediate,
        }
    }

    /// The action an untargeted activation of `ability_type` starts, or `None`
    /// if this slot can't use that ability (unreachable for every ability the
    /// slots actually grant - see `PlayerClass::ability`, the `SecondaryAbility`
    /// inserts in `initialization`, and `RangerAbilities::new`).
    fn action_for(self, ability_type: AbilityType) -> Option<ActionType> {
        match (self, ability_type) {
            (AbilitySlot::Class, AbilityType::Cleave) => Some(ActionType::Cleave),
            (AbilitySlot::Class, AbilityType::Sprint) => Some(ActionType::ActivateSprint),
            (AbilitySlot::Secondary, AbilityType::Barkskin) => {
                Some(ActionType::ActivateBarkskin)
            }
            (AbilitySlot::Secondary, AbilityType::Fear) => Some(ActionType::ActivateFear),
            (AbilitySlot::Secondary, AbilityType::Stun) => Some(ActionType::ActivateStun),
            (AbilitySlot::Ranger(_), AbilityType::Disengage) => Some(ActionType::Disengage),
            (AbilitySlot::Learned(_), _) => Some(ActionType::CastLearnedSpell {
                ability: ability_type,
                target_x: 0,
                target_y: 0,
            }),
            _ => None,
        }
    }

    /// Put this slot on cooldown after its action started.
    ///
    /// `Secondary` and `Learned` are no-ops here because both start their
    /// cooldown inside the action they fire instead: `apply_activate_barkskin`
    /// / `apply_activate_fear` / `apply_activate_stun` for the secondary slot,
    /// `apply_cast_learned_spell` / `apply_start_raise_dead` for spells.
    fn start_cooldown(self, world: &mut hecs::World, player: Entity) {
        match self {
            AbilitySlot::Class => {
                if let Ok(mut ability) = world.get::<&mut ClassAbility>(player) {
                    ability.start_cooldown();
                }
            }
            AbilitySlot::Ranger(index) => {
                if let Ok(mut ra) = world.get::<&mut RangerAbilities>(player) {
                    ra.start_cooldown(index);
                }
            }
            AbilitySlot::Secondary | AbilitySlot::Learned(_) => {}
        }
    }
}

/// Activate the ability in `slot`. Returns true if it was activated (which
/// includes entering targeting mode - the ability is then spent on the click).
fn activate_ability(ctx: &mut SimCtx, slot: AbilitySlot) -> bool {
    let player = ctx.player;

    // Only from idle, never mid-action.
    let is_idle = ctx
        .world
        .get::<&Actor>(player)
        .map(|a| a.current_action.is_none())
        .unwrap_or(false);
    if !is_idle {
        return false;
    }

    // The slot must hold a known ability that is off cooldown...
    let Some((ability_type, energy_cost)) = slot.ready_ability(ctx.world, player) else {
        return false;
    };

    // ...and the player must be able to afford it at all (max_energy >= cost).
    let can_afford = ctx
        .world
        .get::<&Actor>(player)
        .map(|a| a.max_energy >= energy_cost)
        .unwrap_or(false);
    if !can_afford {
        return false;
    }

    // Targeted abilities enter targeting mode and spend no time.
    match slot.targeting(ctx, ability_type) {
        Targeting::Refused => return false,
        Targeting::Enter(max_range) => {
            ctx.input.ability_targeting_mode = Some(input::AbilityTargetingMode {
                ability_type,
                max_range,
            });
            return true;
        }
        Targeting::Immediate => {}
    }

    let Some(action_type) = slot.action_for(ability_type) else {
        return false;
    };

    // Wait for enough energy (this advances time, enemies may act).
    if !simulation::wait_for_energy(&mut ctx.actors(), energy_cost) {
        // Player died or something went wrong during wait
        let _ = process_events(ctx);
        return false;
    }

    let start_result =
        time_system::start_action(ctx.world, player, action_type, ctx.clock, ctx.scheduler);

    if start_result.is_ok() {
        slot.start_cooldown(ctx.world, player);
        // Advance time and process events
        simulation::advance_until_player_ready(&mut ctx.actors());
    }

    let _ = process_events(ctx);

    start_result.is_ok()
}

/// The two time-skip modes [`GameEngine::fast_forward`] drives. They share
/// their pacing, Wait-step loop, Raise-Dead spawn handling and bubble repin;
/// they differ in their stop conditions and in sleep's extra bookkeeping
/// (fatigue recovery, the `Asleep` marker, waiting for energy, waking on
/// damage).
#[derive(Clone, Copy, PartialEq, Eq)]
enum TimeSkip {
    Rest,
    Sleep,
}

impl TimeSkip {
    /// Whether this mode is currently running.
    fn active(self, engine: &GameEngine) -> bool {
        match self {
            TimeSkip::Rest => engine.resting,
            TimeSkip::Sleep => engine.sleeping,
        }
    }

    /// This mode's accumulator of fast-forwarded game-time.
    fn accumulator_mut(self, engine: &mut GameEngine) -> &mut f32 {
        match self {
            TimeSkip::Rest => &mut engine.rest_accumulator,
            TimeSkip::Sleep => &mut engine.sleep_accumulator,
        }
    }

    /// End the skip cleanly, logging `message`.
    fn stop(self, engine: &mut GameEngine, message: &str) {
        match self {
            TimeSkip::Rest => engine.stop_rest(message),
            TimeSkip::Sleep => engine.stop_sleep(message),
        }
    }

    /// Drop out of the skip without the usual teardown. Only for the
    /// "state vanished mid-frame" paths, which have nothing left to tear down.
    fn clear_flag(self, engine: &mut GameEngine) {
        match self {
            TimeSkip::Rest => engine.resting = false,
            TimeSkip::Sleep => engine.sleeping = false,
        }
    }

    /// What to log when a step is cut short (a blocked Wait, an enemy spotting
    /// the player, or damage taken).
    fn interrupt_message(self) -> &'static str {
        match self {
            TimeSkip::Rest => "Your rest is interrupted!",
            TimeSkip::Sleep => "You are rudely awakened!",
        }
    }
}

/// Result of input processing (internal)
#[derive(Default)]
struct InputResult {
    toggle_fullscreen: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Start a real run so the rest/sleep paths have live state and ui_state.
    fn engine_with_run() -> GameEngine {
        let mut engine = GameEngine::new();
        let mut camera = crate::camera::Camera::new(800.0, 600.0);
        engine.start_game(PlayerClass::Fighter, 1234, &mut camera);
        engine
    }

    #[test]
    fn test_sleeping_while_resting_goes_through_stop_rest() {
        // try_sleep used to clear self.resting / self.rest_accumulator directly
        // instead of calling stop_rest, which skipped the "You stop resting."
        // log line and the clear_resting_bubble() call. The bubble was masked
        // because sleep immediately sets its own, so the missing message was
        // the only visible symptom — but the bypass breaks the moment either
        // path grows.
        let mut engine = engine_with_run();
        let state = engine.state.as_mut().expect("run started");
        let player = state.player_entity;

        // Tired enough to sleep, and no enemy alerted (fresh run, so quiet).
        if let Ok(mut fatigue) = state.world.get::<&mut crate::components::Fatigue>(player) {
            fatigue.value = 50.0;
        }

        // Enter the resting state the way toggle_rest would.
        engine.resting = true;
        engine.rest_accumulator = 3.5;
        if let Some(ui) = engine.ui_state.as_mut() {
            ui.message_log.system("You settle down to rest.");
        }

        engine.try_sleep();

        assert!(engine.sleeping, "sleep should have started");
        assert!(!engine.resting, "resting must be cleared");
        assert_eq!(
            engine.rest_accumulator, 0.0,
            "rest accumulator must be reset"
        );

        let lines = engine
            .ui_state
            .as_ref()
            .expect("ui state")
            .message_log
            .lines();
        assert!(
            lines.iter().any(|l| l == "You stop resting."),
            "stop_rest's log line must not be skipped; got {lines:?}"
        );
        assert!(
            lines
                .iter()
                .any(|l| l == "You lie down and drift off to sleep..."),
            "sleep should still announce itself; got {lines:?}"
        );
        assert!(
            engine.vfx.resting_bubble.is_some(),
            "sleep sets its own bubble after stop_rest clears the rest one"
        );
    }

    #[test]
    fn test_sleeping_when_not_resting_logs_no_stop_rest_line() {
        // stop_rest no-ops when not resting, so going straight to sleep must
        // not produce a spurious "You stop resting." line.
        let mut engine = engine_with_run();
        let state = engine.state.as_mut().expect("run started");
        let player = state.player_entity;
        if let Ok(mut fatigue) = state.world.get::<&mut crate::components::Fatigue>(player) {
            fatigue.value = 50.0;
        }

        assert!(!engine.resting);
        engine.try_sleep();

        assert!(engine.sleeping, "sleep should have started");
        let lines = engine
            .ui_state
            .as_ref()
            .expect("ui state")
            .message_log
            .lines();
        assert!(
            !lines.iter().any(|l| l == "You stop resting."),
            "no stop-resting line when the player was not resting; got {lines:?}"
        );
    }

    #[test]
    fn test_refused_sleep_leaves_resting_untouched() {
        // A refused sleep (not tired) must not disturb an in-progress rest:
        // the refusal check happens before stop_rest.
        let mut engine = engine_with_run();
        let state = engine.state.as_mut().expect("run started");
        let player = state.player_entity;
        if let Ok(mut fatigue) = state.world.get::<&mut crate::components::Fatigue>(player) {
            fatigue.value = 0.0;
        }

        engine.resting = true;
        engine.rest_accumulator = 2.0;

        engine.try_sleep();

        assert!(!engine.sleeping, "sleep must be refused when not tired");
        assert!(engine.resting, "a refused sleep must not cancel the rest");
        assert_eq!(engine.rest_accumulator, 2.0, "accumulator preserved");
        let lines = engine
            .ui_state
            .as_ref()
            .expect("ui state")
            .message_log
            .lines();
        assert!(
            lines.iter().any(|l| l == "You don't feel tired."),
            "refusal should be reported; got {lines:?}"
        );
    }

    /// One determinism scenario.
    struct Scenario {
        /// Player turns to run.
        turns: usize,
        /// Give the player a huge HP pool so the whole script runs instead of
        /// ending early in a death.
        tough_player: bool,
        /// Descend at this turn, exercising floor save/load and AI re-init.
        descend_at: Option<usize>,
    }

    /// Golden-value determinism test: the player under pressure.
    ///
    /// No HP boost, so the swarm eventually kills them. Deaths and the AI
    /// decisions leading up to them amplify tiny divergences, which makes this
    /// the scenario that catches unordered iteration over the active-AI
    /// `HashSet` (see `initialize_ai_actors`).
    ///
    /// **Expected to fail when you change game balance.** It asserts
    /// reproducibility, not correctness: when an intentional change moves these
    /// numbers, check the diff moved the way you meant, then re-record.
    ///
    /// Adding or removing a component on a live entity counts as such a
    /// change, even a purely cosmetic one. It moves the entity between hecs
    /// archetypes, which reorders every query that touches it, which reorders
    /// the draws systems make from the seeded rng. The `HitFlash` flash-on-hit
    /// component last moved these values for exactly that reason; the runs
    /// stayed reproducible, they just took a different path.
    #[test]
    fn test_fixed_seed_replays_identically_under_pressure() {
        const EXPECTED: &str = "t=355.0580 floor=0 kills=13 hp=-2/50 pos=11,10 hunger=83.3344 fatigue=10.0000 n=92 roster=0ed322dd";
        assert_eq!(
            run_fixed_script(&Scenario {
                turns: 400,
                tough_player: false,
                descend_at: None,
            }),
            EXPECTED
        );
    }

    /// Golden-value determinism test: a long run across a floor boundary.
    ///
    /// The player survives the whole script, so this covers far more simulation
    /// than the scenario above - sustained combat, kills and loot, the survival
    /// clock, fire, and a descent through floor save/load and the per-floor rng.
    ///
    /// Same caveat: expected to fail on intentional balance changes.
    #[test]
    fn test_fixed_seed_replays_identically_across_a_floor() {
        const EXPECTED: &str = "t=263.9901 floor=1 kills=16 hp=99869/100000 pos=10,13 hunger=87.5008 fatigue=7.5000 n=95 roster=d4bbc415";
        assert_eq!(
            run_fixed_script(&Scenario {
                turns: 300,
                tough_player: true,
                descend_at: Some(150),
            }),
            EXPECTED
        );
    }

    /// Drive a fixed script of player turns and return a digest of the world.
    /// No tile may finish floor construction with two movement blockers on it.
    ///
    /// This is the invariant `TileOccupancy` exists to hold, checked here on
    /// real generated floors rather than a hand-built world. Two blockers on a
    /// tile is a tile neither occupant can be pushed off and a monster standing
    /// inside the scenery.
    ///
    /// Nothing in the grid's position lists used to guarantee it. `spawn_all`
    /// picked from walkable *terrain*, so the floor roster dropped enemies onto
    /// the chests, coffins, barrels, doorways and furniture the prop passes had
    /// just spawned; and generation could roll a chest and a barrel onto the
    /// same Storage-room tile. Measured over these 200 floors before the fix:
    /// 698 stacked tiles on 196 of them, 652 of those an enemy on a prop or in
    /// a doorway (doorways alone about 260) and 46 a chest on a barrel.
    ///
    /// Both construction paths also assert this themselves via
    /// `tile_occupancy::assert_one_blocker_per_tile`, so a regression trips in
    /// any debug run rather than waiting for this test; this pins it across far
    /// more floors than a normal session touches.
    #[test]
    fn floor_construction_never_stacks_two_blockers_on_a_tile() {
        use crate::components::{BlocksMovement, Position};
        use std::collections::HashMap;

        let mut blockers_checked = 0usize;

        for seed in 0..50u64 {
            let mut camera = crate::camera::Camera::new(800.0, 600.0);
            let mut engine = GameEngine::new();
            engine.start_game(PlayerClass::Fighter, seed, &mut camera);

            // Floor 0 plus three descents, so cavern fauna and boss floors are
            // covered as well as the plain roster.
            for depth in 0..4 {
                if depth > 0 {
                    engine.handle_floor_transition(
                        crate::events::StairDirection::Down,
                        &mut camera,
                    );
                }
                let state = engine.state.as_ref().expect("run started");

                let mut per_tile: HashMap<(i32, i32), usize> = HashMap::new();
                for (_, (pos, _)) in state.world.query::<(&Position, &BlocksMovement)>().iter() {
                    blockers_checked += 1;
                    *per_tile.entry((pos.x, pos.y)).or_default() += 1;
                }

                let mut stacked: Vec<((i32, i32), usize)> =
                    per_tile.into_iter().filter(|&(_, n)| n > 1).collect();
                stacked.sort_unstable();
                assert!(
                    stacked.is_empty(),
                    "seed {seed} floor {depth}: {} tile(s) hold more than one blocker\n  \
                     (tile, blockers): {stacked:?}",
                    stacked.len()
                );
            }
        }

        assert!(
            blockers_checked > 10_000,
            "only {blockers_checked} blockers seen — the scan is not covering the floors"
        );
    }

    /// Crypt is a required room theme on every floor, so every floor rolls
    /// coffin positions — but `spawn_floor_entities` never spawned them, so
    /// only floor 0 (built by `init_world`) actually had coffins. Below that,
    /// crypts were decorated rooms with nothing in them.
    #[test]
    fn coffins_spawn_on_floors_below_the_first() {
        use crate::components::{Container, ContainerType};

        let mut deep_floors_with_coffins = 0;
        let mut deep_floors_rolling_coffins = 0;

        for seed in 0..25u64 {
            let mut camera = crate::camera::Camera::new(800.0, 600.0);
            let mut engine = GameEngine::new();
            engine.start_game(PlayerClass::Fighter, seed, &mut camera);

            for _ in 1..4 {
                engine.handle_floor_transition(crate::events::StairDirection::Down, &mut camera);
                let state = engine.state.as_ref().expect("run started");

                // Only floors whose generation actually rolled coffin spots can
                // be expected to have any.
                if state.grid.coffin_positions.is_empty() {
                    continue;
                }
                deep_floors_rolling_coffins += 1;

                let coffins = state
                    .world
                    .query::<&Container>()
                    .iter()
                    .filter(|(_, c)| c.container_type == ContainerType::Coffin)
                    .count();
                if coffins > 0 {
                    deep_floors_with_coffins += 1;
                }
            }
        }

        assert!(
            deep_floors_rolling_coffins > 0,
            "no floor below the first rolled any coffin positions — this test \
             cannot say anything"
        );
        assert_eq!(
            deep_floors_with_coffins, deep_floors_rolling_coffins,
            "every floor that rolled coffin positions should have spawned coffins"
        );
    }

    /// Leaving a floor and coming back must restore containers as what they
    /// were.
    ///
    /// `save_floor` used to flatten every non-corpse `Container` into a
    /// `Chest`, dropping `container_type` and `spawn_chance`. A revisited crypt
    /// came back full of chests that could never release their skeletons, a
    /// storage room came back with chest-sprited barrels, and a pile of dropped
    /// loot came back as a blocking chest.
    #[test]
    fn container_kinds_survive_a_floor_round_trip() {
        use crate::components::{Container, ContainerType, GroundItemPile, ItemInstance, ItemType, Position};

        /// Counts per container kind, in a fixed order so two censuses compare
        /// directly. `ContainerType` is not `Hash`, and this is not a reason to
        /// make it so.
        const KINDS: [ContainerType; 5] = [
            ContainerType::Chest,
            ContainerType::Coffin,
            ContainerType::Barrel,
            ContainerType::Corpse,
            ContainerType::GroundPile,
        ];
        fn census(world: &hecs::World) -> [usize; 5] {
            let mut counts = [0usize; 5];
            for (_, container) in world.query::<&Container>().iter() {
                if let Some(i) = KINDS.iter().position(|k| *k == container.container_type) {
                    counts[i] += 1;
                }
            }
            counts
        }
        let kind_index = |kind: ContainerType| {
            KINDS.iter().position(|k| *k == kind).expect("known kind")
        };

        let mut checked_coffins = 0;
        let mut checked_piles = 0;

        for seed in 0..25u64 {
            let mut camera = crate::camera::Camera::new(800.0, 600.0);
            let mut engine = GameEngine::new();
            engine.start_game(PlayerClass::Fighter, seed, &mut camera);

            // Drop a pile of loot on the player's tile, so the walkable
            // container kind is covered too.
            {
                let state = engine.state.as_mut().expect("run started");
                let at = state
                    .world
                    .get::<&Position>(state.player_entity)
                    .map(|p| (p.x, p.y))
                    .expect("player has a position");
                let pos = Position::new(at.0, at.1);
                state.world.spawn((
                    pos,
                    crate::components::VisualPosition::from_position(&pos),
                    crate::components::Sprite::from_ref(crate::tile::tile_ids::COINS),
                    Container::ground_pile(vec![ItemInstance::plain(ItemType::Apple)]),
                    GroundItemPile,
                ));
            }

            let before = census(&engine.state.as_ref().expect("run").world);

            // Down and back up: floor 0 is saved on the way down and reloaded
            // on the way back.
            engine.handle_floor_transition(crate::events::StairDirection::Down, &mut camera);
            engine.handle_floor_transition(crate::events::StairDirection::Up, &mut camera);

            let state = engine.state.as_ref().expect("run started");
            assert_eq!(state.current_floor, 0, "seed {seed}: should be back on floor 0");
            let after = census(&state.world);

            assert_eq!(
                before, after,
                "seed {seed}: container kinds changed across a floor round trip \
                 (counts in order {KINDS:?})"
            );

            checked_coffins += before[kind_index(ContainerType::Coffin)];
            checked_piles += before[kind_index(ContainerType::GroundPile)];

            // The restored pile must still be walkable and still be findable as
            // a pile, not a blocking chest.
            let piles = state
                .world
                .query::<(&Container, &GroundItemPile)>()
                .iter()
                .count();
            assert!(piles > 0, "seed {seed}: the dropped pile came back as something else");
            for (id, (container, _)) in state.world.query::<(&Container, &GroundItemPile)>().iter() {
                assert_eq!(container.container_type, ContainerType::GroundPile);
                assert!(
                    state.world.get::<&crate::components::BlocksMovement>(id).is_err(),
                    "seed {seed}: a dropped pile must not block movement"
                );
            }
        }

        assert!(
            checked_coffins > 0,
            "no coffins were round-tripped — the coffin half of this is untested"
        );
        assert!(checked_piles > 0, "no ground piles were round-tripped");
    }


    /// Whether a container blocks its tile must not depend on whether the
    /// player left the floor and came back.
    ///
    /// Live play never dropped `BlocksMovement` from a looted container, but
    /// `load_floor` restored an open-and-empty one as walkable. So a looted
    /// chest was an obstacle while you stayed on the floor and scenery once you
    /// took the stairs down and back. Both sides now decide from
    /// `Container::is_looted`.
    #[test]
    fn whether_a_looted_container_blocks_survives_a_floor_round_trip() {
        use crate::components::{BlocksMovement, Container, Position};

        let mut looted_containers = 0;

        for seed in 0..25u64 {
            let mut camera = crate::camera::Camera::new(800.0, 600.0);
            let mut engine = GameEngine::new();
            engine.start_game(PlayerClass::Fighter, seed, &mut camera);

            // Loot roughly half of this floor's containers dry, then let the
            // live sweep react, exactly as a looting turn would.
            {
                let state = engine.state.as_mut().expect("run started");
                let ids: Vec<hecs::Entity> = state
                    .world
                    .query::<&Container>()
                    .iter()
                    .map(|(id, _)| id)
                    .collect();
                for (n, id) in ids.iter().enumerate() {
                    if n % 2 == 1 {
                        continue;
                    }
                    if let Ok(mut container) = state.world.get::<&mut Container>(*id) {
                        container.is_open = true;
                        container.items.clear();
                        container.gold = 0;
                    }
                }
                systems::unblock_emptied_containers(
                    &mut state.world,
                    &mut state.spatial_cache,
                );
                state
                    .spatial_cache
                    .assert_coherent_with_world(&state.world, "after looting");
            }

            // Record, per tile, whether a container there blocks.
            let census = |world: &hecs::World| -> Vec<((i32, i32), bool, bool)> {
                let mut rows: Vec<((i32, i32), bool, bool)> = world
                    .query::<(&Position, &Container)>()
                    .iter()
                    .map(|(id, (pos, container))| {
                        (
                            (pos.x, pos.y),
                            container.is_looted(),
                            world.get::<&BlocksMovement>(id).is_ok(),
                        )
                    })
                    .collect();
                rows.sort_unstable();
                rows
            };

            let before = census(&engine.state.as_ref().expect("run").world);
            looted_containers += before.iter().filter(|(_, looted, _)| *looted).count();

            engine.handle_floor_transition(crate::events::StairDirection::Down, &mut camera);
            engine.handle_floor_transition(crate::events::StairDirection::Up, &mut camera);

            let state = engine.state.as_ref().expect("run started");
            assert_eq!(state.current_floor, 0, "seed {seed}: should be back on floor 0");
            assert_eq!(
                before,
                census(&state.world),
                "seed {seed}: a container's blocking changed across a floor round trip \
                 (tile, looted, blocks)"
            );

            // And a looted container must be walkable on both sides, not just
            // consistently wrong.
            for &(tile, looted, blocks) in &before {
                if looted {
                    assert!(!blocks, "seed {seed}: looted container at {tile:?} still blocks");
                }
            }
        }

        assert!(
            looted_containers > 0,
            "nothing was looted — this test cannot say anything"
        );
    }


    fn run_fixed_script(scenario: &Scenario) -> String {
        use crate::systems::player_input::PlayerIntent;

        let mut camera = crate::camera::Camera::new(800.0, 600.0);
        let mut engine = GameEngine::new();
        engine.start_game(PlayerClass::Fighter, 1234, &mut camera);

        // Wake every hostile onto the player so AI, pathfinding, combat and the
        // spatial cache all actually run during the turns below.
        {
            let state = engine.state.as_mut().expect("run started");
            let player = state.player_entity;
            if scenario.tough_player {
                if let Ok(mut health) = state.world.get::<&mut Health>(player) {
                    health.max = 100_000;
                    health.current = 100_000;
                }
            }
            let player_pos = state
                .world
                .get::<&crate::components::Position>(player)
                .map(|p| (p.x, p.y))
                .expect("player has a position");
            let mut hostiles: Vec<Entity> = state
                .world
                .query::<&crate::components::ChaseAI>()
                .iter()
                .map(|(id, _)| id)
                .collect();
            hostiles.sort_unstable();
            for &id in &hostiles {
                let _ = state.world.remove_one::<crate::components::Asleep>(id);
                if let Ok(mut ai) = state.world.get::<&mut crate::components::ChaseAI>(id) {
                    ai.state = crate::components::AIState::Chasing;
                    ai.add_threat(player, crate::constants::WAKE_THREAT);
                    ai.update_target_pos(player, player_pos);
                }
            }

            // Drag a handful of them into melee range. Without this the scenario
            // depends on where the dungeon generator happened to put enemies
            // relative to the player's spawn - a cavern-generation change once
            // left the player untouched for 400 turns, which quietly gutted what
            // these tests covered. Assignment is in entity-id and then tile
            // order, so it stays deterministic.
            let mut ring: Vec<(i32, i32)> = (-2..=2)
                .flat_map(|dy| (-2..=2).map(move |dx| (dx, dy)))
                .filter(|&(dx, dy)| (dx, dy) != (0, 0))
                .map(|(dx, dy)| (player_pos.0 + dx, player_pos.1 + dy))
                .filter(|&(x, y)| state.grid.is_walkable(x, y))
                .collect();
            ring.sort_unstable();
            for (&id, &(x, y)) in hostiles.iter().zip(ring.iter()) {
                if let Ok(mut pos) = state.world.get::<&mut crate::components::Position>(id) {
                    pos.x = x;
                    pos.y = y;
                }
                if let Ok(mut vis) = state
                    .world
                    .get::<&mut crate::components::VisualPosition>(id)
                {
                    vis.x = x as f32;
                    vis.y = y as f32;
                }
            }

            // Those moves bypassed the incremental cache updates, so rebuild it
            // wholesale rather than leaving phantom blockers behind.
            state.spatial_cache.rebuild_in_place(&state.world);
            state
                .active_ai_tracker
                .initialize_from_world(&state.world, player_pos);
        }

        let script = [
            PlayerIntent::Move { dx: 1, dy: 0 },
            PlayerIntent::Move { dx: 0, dy: 1 },
            PlayerIntent::Wait,
            PlayerIntent::Move { dx: 1, dy: 1 },
            PlayerIntent::AttackDirection { dx: 1, dy: 0 },
            PlayerIntent::Move { dx: 0, dy: -1 },
            PlayerIntent::Move { dx: -1, dy: 0 },
            PlayerIntent::Wait,
        ];

        for turn in 0..scenario.turns {
            if Some(turn) == scenario.descend_at {
                engine.handle_floor_transition(crate::events::StairDirection::Down, &mut camera);
            }

            let result = {
                let Some(mut ctx) = engine.sim_ctx() else { break };
                execute_player_intent(&mut ctx, script[turn % script.len()].clone())
            };
            engine.apply_deferred_spawns(&result);

            // Fire, identification and the survival clock are paced by the
            // game-time the turns generate; drive them the way `tick` does.
            let state = engine.state.as_mut().expect("run started");
            crate::systems::fire::tick_fire(
                &mut EffectCtx {
                    world: &mut state.world,
                    grid: &mut state.grid,
                    spatial: &mut state.spatial_cache,
                    events: &mut engine.events,
                    rng: &mut state.rng,
                },
                crate::constants::ACTION_WAIT_DURATION,
                &mut state.fire_accumulator,
                &mut state.fov_dirty,
            );
            let _ = crate::systems::survival::tick_survival(
                &mut state.world,
                state.player_entity,
                crate::constants::ACTION_WAIT_DURATION,
                &mut state.survival_accumulator,
                crate::systems::survival::SurvivalContext {
                    resting: false,
                    sleeping: false,
                },
                &mut engine.events,
            );
            let floor = state.current_floor;
            let kills =
                systems::remove_dead_entities(&mut state.actor_ctx(&mut engine.events), floor);
            state.kills += kills;
        }

        world_digest(&engine)
    }

    /// A compact, stable summary of the whole world. Named scalars stay readable
    /// in a failure message; the roster of every positioned entity is folded
    /// into one FNV-1a hash so the expected value stays a single line.
    fn world_digest(engine: &GameEngine) -> String {
        let state = engine.state.as_ref().expect("run started");

        let mut rows: Vec<String> = state
            .world
            .query::<&crate::components::Position>()
            .iter()
            .map(|(id, pos)| {
                let name = state
                    .world
                    .get::<&crate::components::Name>(id)
                    .map(|n| n.0.clone())
                    .unwrap_or_else(|_| "?".into());
                let hp = state
                    .world
                    .get::<&Health>(id)
                    .map(|h| format!("{}/{}", h.current, h.max))
                    .unwrap_or_else(|_| "-".into());
                let ai = state
                    .world
                    .get::<&crate::components::ChaseAI>(id)
                    .map(|a| format!("{:?}", a.state))
                    .unwrap_or_else(|_| "-".into());
                format!("{name}@{},{} {hp} {ai}", pos.x, pos.y)
            })
            .collect();
        rows.sort();

        let mut hash: u32 = 0x811c_9dc5;
        for byte in rows.join(";").bytes() {
            hash ^= byte as u32;
            hash = hash.wrapping_mul(0x0100_0193);
        }

        let player = state.player_entity;
        let hp = state
            .world
            .get::<&Health>(player)
            .map(|h| format!("{}/{}", h.current, h.max))
            .unwrap_or_else(|_| "dead".into());
        let pos = state
            .world
            .get::<&crate::components::Position>(player)
            .map(|p| format!("{},{}", p.x, p.y))
            .unwrap_or_else(|_| "-".into());
        let hunger = state
            .world
            .get::<&crate::components::Hunger>(player)
            .map(|h| format!("{:.4}", h.value))
            .unwrap_or_else(|_| "-".into());
        let fatigue = state
            .world
            .get::<&crate::components::Fatigue>(player)
            .map(|f| format!("{:.4}", f.value))
            .unwrap_or_else(|_| "-".into());

        format!(
            "t={:.4} floor={} kills={} hp={hp} pos={pos} hunger={hunger} fatigue={fatigue} n={} roster={hash:08x}",
            state.game_clock.time,
            state.current_floor,
            state.kills,
            rows.len(),
        )
    }

    /// Mark one hostile as actively chasing, which both rest and sleep treat
    /// as a reason to stop.
    fn alert_an_enemy(engine: &mut GameEngine) {
        let state = engine.state.as_mut().expect("run started");
        let hostile = state
            .world
            .query::<&crate::components::ChaseAI>()
            .iter()
            .map(|(id, _)| id)
            .find(|id| state.world.get::<&crate::components::TamedBy>(*id).is_err())
            .expect("a fresh floor has hostiles");
        if let Ok(mut ai) = state.world.get::<&mut crate::components::ChaseAI>(hostile) {
            ai.state = crate::components::AIState::Chasing;
        }
    }

    fn hurt_player(engine: &mut GameEngine, to: i32) {
        let state = engine.state.as_mut().expect("run started");
        let player = state.player_entity;
        if let Ok(mut health) = state.world.get::<&mut Health>(player) {
            health.current = to;
        }
    }

    fn log_lines(engine: &GameEngine) -> Vec<String> {
        engine
            .ui_state
            .as_ref()
            .expect("ui state")
            .message_log
            .lines()
    }

    /// One frame's dt large enough to generate at least one Wait-step, so the
    /// fast-forward loop actually reaches its stop checks.
    const ONE_STEP_DT: f32 = crate::constants::ACTION_WAIT_DURATION / REST_TIME_SCALE + 0.001;

    #[test]
    fn test_rest_stops_at_full_health() {
        let mut engine = engine_with_run();
        engine.resting = true;

        engine.update_rest(ONE_STEP_DT);

        assert!(!engine.resting, "a full-health player should stop resting");
        let lines = log_lines(&engine);
        assert!(
            lines.iter().any(|l| l == "You finish resting, fully recovered."),
            "got {lines:?}"
        );
    }

    #[test]
    fn test_rest_stops_when_too_hungry() {
        let mut engine = engine_with_run();
        hurt_player(&mut engine, 1); // not full health, so that check doesn't fire first
        {
            let state = engine.state.as_mut().expect("run started");
            let player = state.player_entity;
            let mut hunger = state
                .world
                .get::<&mut crate::components::Hunger>(player)
                .expect("player has a hunger meter");
            hunger.value = crate::constants::HUNGER_HUNGRY_THRESHOLD - 1.0;
        }
        engine.resting = true;

        engine.update_rest(ONE_STEP_DT);

        assert!(!engine.resting, "hunger stops natural regen, so rest must end");
        let lines = log_lines(&engine);
        assert!(
            lines.iter().any(|l| l == "You are too hungry to keep resting."),
            "got {lines:?}"
        );
    }

    #[test]
    fn test_rest_stops_when_an_enemy_is_alerted() {
        let mut engine = engine_with_run();
        hurt_player(&mut engine, 1);
        alert_an_enemy(&mut engine);
        engine.resting = true;

        engine.update_rest(ONE_STEP_DT);

        assert!(!engine.resting, "an alerted enemy must interrupt rest");
        let lines = log_lines(&engine);
        assert!(
            lines.iter().any(|l| l == "Your rest is interrupted!"),
            "got {lines:?}"
        );
    }

    #[test]
    fn test_sleep_stops_once_fatigue_is_recovered() {
        let mut engine = engine_with_run();
        engine.sleeping = true; // fresh run starts at zero fatigue

        engine.update_sleep(ONE_STEP_DT);

        assert!(!engine.sleeping, "zero fatigue means sleep is done");
        let lines = log_lines(&engine);
        assert!(
            lines.iter().any(|l| l == "You wake up feeling refreshed."),
            "got {lines:?}"
        );
    }

    #[test]
    fn test_sleep_stops_when_an_enemy_is_alerted() {
        let mut engine = engine_with_run();
        {
            let state = engine.state.as_mut().expect("run started");
            let player = state.player_entity;
            // Still tired, so the fatigue check doesn't fire first.
            if let Ok(mut fatigue) = state.world.get::<&mut crate::components::Fatigue>(player) {
                fatigue.value = 50.0;
            }
        }
        alert_an_enemy(&mut engine);
        engine.sleeping = true;

        engine.update_sleep(ONE_STEP_DT);

        assert!(!engine.sleeping, "an alerted enemy must wake the sleeper");
        let lines = log_lines(&engine);
        assert!(
            lines
                .iter()
                .any(|l| l == "You are jolted awake — something has noticed you!"),
            "got {lines:?}"
        );
    }

    /// Sleep's distinct wake condition: any HP drop. Burning damage lands
    /// during the Wait step's time advancement and raises no AttackHit event,
    /// so only the HP comparison can catch it.
    #[test]
    fn test_sleep_wakes_on_hp_drop_from_burning() {
        let mut engine = engine_with_run();
        {
            let state = engine.state.as_mut().expect("run started");
            let player = state.player_entity;
            // Still tired, so the fatigue check doesn't fire first.
            if let Ok(mut fatigue) = state.world.get::<&mut crate::components::Fatigue>(player) {
                fatigue.value = 50.0;
            }
            crate::systems::effects::add_effect_to_entity(
                &mut state.world,
                player,
                crate::components::EffectType::Burning,
                60.0,
            );
        }
        engine.sleeping = true;

        // Enough fast-forward for at least one burn tick to land.
        engine.update_sleep(ONE_STEP_DT * 8.0);

        assert!(!engine.sleeping, "burning damage must wake the sleeper");
        let lines = log_lines(&engine);
        assert!(
            lines.iter().any(|l| l == "You are rudely awakened!"),
            "got {lines:?}"
        );
    }

    /// The mirror of the stop-condition tests: with nothing to stop for, the
    /// fast-forward keeps running (so the tests above are not just asserting
    /// that the loop never starts).
    #[test]
    fn test_rest_continues_while_hurt_and_unthreatened() {
        let mut engine = engine_with_run();
        hurt_player(&mut engine, 1);
        engine.resting = true;

        engine.update_rest(ONE_STEP_DT);

        assert!(engine.resting, "nothing should have stopped the rest");
        assert!(
            engine.vfx.resting_bubble.is_some(),
            "the bubble stays pinned while resting continues"
        );
    }

    /// A raised skeleton is a true companion: standard skeleton stat block
    /// wired into the tamed-ally infrastructure (CompanionAI + TamedBy +
    /// RaisedUndead), not a hostile — and it doesn't block its owner's path.
    #[test]
    fn test_spawn_raised_skeleton_is_companion_not_hostile() {
        let mut world = hecs::World::new();
        let owner = world.spawn((crate::components::Position::new(1, 1),));

        let mut clock = crate::time_system::GameClock::new();
        let mut scheduler = crate::time_system::ActionScheduler::new();
        let mut tracker = crate::active_ai_tracker::ActiveAITracker::new();
        let mut cache = crate::spatial_cache::SpatialCache::rebuild_from_world(&world);
        let mut events = EventQueue::new();
        let mut rng = {
            use rand::SeedableRng;
            rand::rngs::StdRng::seed_from_u64(1)
        };
        let mut grid = crate::grid::Grid::new_floor(20, 20, 0, &mut rng);

        spawn_raised_skeleton(
            &mut ActorCtx {
                world: &mut world,
                grid: &mut grid,
                player: owner,
                clock: &mut clock,
                scheduler: &mut scheduler,
                tracker: &mut tracker,
                spatial: &mut cache,
                events: &mut events,
                rng: &mut rng,
            },
            2,
            1,
        );

        let (skeleton, _) = world
            .query::<&crate::components::RaisedUndead>()
            .iter()
            .next()
            .map(|(id, r)| (id, *r))
            .expect("a raised skeleton exists");

        // Ally wiring...
        let companion_owner = world
            .get::<&crate::components::CompanionAI>(skeleton)
            .map(|c| c.owner)
            .expect("companion AI attached");
        assert_eq!(companion_owner, owner);
        assert!(world.get::<&crate::components::TamedBy>(skeleton).is_ok());
        // ... and no hostile leftovers.
        assert!(world.get::<&crate::components::ChaseAI>(skeleton).is_err());
        assert!(world.get::<&crate::components::Asleep>(skeleton).is_err());
        // Unlike a tamed animal, a raised skeleton KEEPS BlocksMovement: it is
        // a screen you put between yourself and something else. Its owner walks
        // through it anyway (see apply_move's companion exception).
        assert!(
            world.get::<&crate::components::BlocksMovement>(skeleton).is_ok(),
            "a raised skeleton body-blocks for its owner"
        );
        assert!(
            cache.is_blocked((2, 1)),
            "and the SpatialCache agrees, so enemies path around it"
        );
        assert!(
            !cache.blocks_vision((2, 1)),
            "but it never blocks line of sight"
        );
        cache.assert_coherent_with_world(&world, "after raising a skeleton");
        // It keeps the skeleton stat block (alive, counted against the cap).
        assert_eq!(
            crate::systems::actions::raised_undead_count(&world),
            1,
            "the new companion counts against the raise cap"
        );
    }

    /// The starting-room campfire used to be placed purely on centre-offsets
    /// with only a walkability check. In a small starting room an offset of 2
    /// lands on the edge column, one tile from the door — and a campfire is
    /// CausesBurning without BlocksMovement, so it reads as scenery and then
    /// sets you alight on the way out. There is no walk-around in a one-tile
    /// doorway.
    #[test]
    fn the_starting_campfire_never_sits_in_a_doorway() {
        use rand::SeedableRng;

        let mut checked = 0;
        let mut would_have_offended = 0;

        for seed in 0..80u64 {
            let mut rng = rand::rngs::StdRng::seed_from_u64(seed);
            let grid = crate::grid::Grid::new_floor(50, 50, 0, &mut rng);
            let Some(room) = grid.starting_room else { continue };

            // What the old code would have chosen: first walkable centre-offset.
            let (cx, cy) = room.center();
            let old_choice = [
                (cx + 2, cy),
                (cx - 2, cy),
                (cx, cy + 2),
                (cx, cy - 2),
                (cx + 1, cy + 1),
            ]
            .into_iter()
            .find(|&(x, y)| grid.is_walkable(x, y));
            if let Some((x, y)) = old_choice {
                if grid.blocks_a_doorway(x, y) {
                    would_have_offended += 1;
                }
            }

            if let Some((x, y)) = pick_campfire_spot(&grid, &room, None) {
                checked += 1;
                assert!(
                    !grid.blocks_a_doorway(x, y),
                    "seed {seed}: campfire at {:?} is in a doorway approach",
                    (x, y)
                );
                assert!(grid.is_walkable(x, y), "seed {seed}: campfire on an unwalkable tile");
            }
        }

        assert!(checked > 40, "expected most seeds to place a campfire, got {checked}");
        // Proves the filter is load-bearing rather than vacuous.
        assert!(
            would_have_offended > 0,
            "the old centre-offset placement never hit a doorway in {checked} floors, \
             so this test would not have caught the reported bug"
        );
    }

    /// The campfire must not land on the player's own tile either.
    #[test]
    fn the_starting_campfire_avoids_the_player_tile() {
        use rand::SeedableRng;
        for seed in 0..40u64 {
            let mut rng = rand::rngs::StdRng::seed_from_u64(seed);
            let grid = crate::grid::Grid::new_floor(50, 50, 0, &mut rng);
            let Some(room) = grid.starting_room else { continue };
            let start = room.center();
            if let Some(spot) = pick_campfire_spot(&grid, &room, Some(start)) {
                assert_ne!(spot, start, "seed {seed}: campfire spawned on the player");
            }
        }
    }

}
