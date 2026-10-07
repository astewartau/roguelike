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

mod dev_spawning;
pub mod floor_transition;
mod game_state;
pub mod initialization;
mod simulation;

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
        let audio = AudioManager::new();
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

        // Spawn campfire in starting room near wizard
        if let Some(starting_room) = &state.grid.starting_room {
            // Find a position for the campfire (offset from center)
            let (cx, cy) = starting_room.center();
            // Try to place it to the right of center, or find first available spot
            let campfire_positions = [
                (cx + 2, cy),
                (cx - 2, cy),
                (cx, cy + 2),
                (cx, cy - 2),
                (cx + 1, cy + 1),
            ];
            for (x, y) in campfire_positions {
                if state.grid.is_walkable(x, y) {
                    spawning::spawn_campfire(&mut state.world, x, y);
                    break;
                }
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

    /// Get a reference to the UI state (panics if not playing).
    pub fn ui_state(&self) -> &GameUiState {
        self.ui_state.as_ref().expect("UI state not initialized - game not started")
    }

    /// Get a mutable reference to the UI state (panics if not playing).
    pub fn ui_state_mut(&mut self) -> &mut GameUiState {
        self.ui_state.as_mut().expect("UI state not initialized - game not started")
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
            WindowEvent::KeyboardInput { event: key_event, .. } => {
                if !egui_consumed {
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
                if !egui_consumed && *button == MouseButton::Right {
                    if *btn_state == ElementState::Released {
                        if self.input.is_targeting() {
                            self.input.cancel_targeting();
                        } else {
                            self.input.pending_right_click = true;
                        }
                    }
                }
            }
            WindowEvent::MouseWheel { delta, .. } => {
                if !egui_consumed {
                    let scroll = match delta {
                        MouseScrollDelta::LineDelta(_, y) => *y * 2.0,
                        MouseScrollDelta::PixelDelta(pos) => pos.y as f32 * 0.1,
                    };
                    camera.add_zoom_impulse(scroll, self.input.mouse_pos.0, self.input.mouse_pos.1);
                }
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
        let ui_state = self.ui_state.as_mut().expect("UI state should exist when state exists");

        // Update animations
        {
            profile_scope!("animations");
            systems::update_lunge_animations(&mut state.world, dt);
            self.vfx.update(dt);
        }

        // Remove dead entities (loot rolls draw from the seeded game rng);
        // hostile deaths feed the run's kill counter.
        {
            profile_scope!("remove_dead");
            state.kills += systems::remove_dead_entities(
                &mut state.world,
                state.player_entity,
                state.current_floor,
                &mut state.rng,
                &mut self.events,
                Some(&mut state.action_scheduler),
                &mut state.spatial_cache,
                &mut state.active_ai_tracker,
            );
        }

        // Process events from remove_dead_entities
        let event_result = {
            profile_scope!("process_events");
            process_events_with_audio(
                &mut self.events,
                &mut state.world,
                &state.grid,
                &mut state.spatial_cache,
                &mut self.vfx,
                ui_state,
                state.player_entity,
                self.audio.as_ref(),
            )
        };

        // Collect skeleton spawn positions before floor transition might invalidate state
        let skeleton_spawns = event_result.skeleton_spawns.clone();
        let raised_skeletons = event_result.raised_skeletons.clone();
        let boss_minion_spawns = event_result.boss_minion_spawns.clone();

        if let Some(direction) = event_result.floor_transition {
            self.handle_floor_transition(direction, camera);
        }
        if event_result.should_interrupt_path() {
            self.input.clear_path();
        }

        // Re-borrow state after floor transition (which may have modified it)
        let state = self.state.as_mut().expect("State should still exist after floor transition");

        // Spawn skeletons from opened coffins
        if !skeleton_spawns.is_empty() {
            for (x, y) in &skeleton_spawns {
                let skeleton = spawning::enemies::SKELETON.spawn(&mut state.world, *x, *y);
                state.spatial_cache.register_entity(skeleton, (*x, *y), true, false);
                initialization::initialize_single_ai_actor(
                    &mut state.world,
                    &state.grid,
                    skeleton,
                    state.player_entity,
                    &state.game_clock,
                    &mut state.action_scheduler,
                    &mut state.active_ai_tracker,
                    &state.spatial_cache,
                    &mut self.events,
                    &mut state.rng,
                );
            }
            // Close loot UI - player must deal with skeleton first
            if let Some(ui_state) = self.ui_state.as_mut() {
                ui_state.close_chest();
            }
        }

        // Spawn friendly skeletons from completed Raise Dead channels
        for (x, y) in &raised_skeletons {
            spawn_raised_skeleton(
                &mut state.world,
                state.player_entity,
                *x,
                *y,
                &state.game_clock,
                &mut state.action_scheduler,
                &mut state.active_ai_tracker,
                &mut state.spatial_cache,
            );
        }

        // Spawn boss-summoned spiderlings (Mother Silkrot's brood)
        for (boss, (x, y)) in &boss_minion_spawns {
            spawn_boss_minion(
                &mut state.world,
                &state.grid,
                *boss,
                *x,
                *y,
                state.player_entity,
                &state.game_clock,
                &mut state.action_scheduler,
                &mut state.active_ai_tracker,
                &mut state.spatial_cache,
                &mut self.events,
                &mut state.rng,
            );
        }

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
                rand::random::<f32>() < 0.5
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

        // Update camera
        camera.update(dt, self.input.mouse_down);

        // Advance spreading fire (burnout + grass/creature ignition), paced by
        // the game-time elapsed this frame. May revert burnt grass to floor and
        // set fov_dirty, which the FOV update below then picks up.
        {
            profile_scope!("tick_fire");
            let game_dt = state.game_clock.time - clock_t0;
            systems::fire::tick_fire(
                &mut state.world,
                &mut state.grid,
                &mut state.spatial_cache,
                &mut self.events,
                game_dt,
                &mut state.fire_accumulator,
                &mut state.fov_dirty,
                &mut state.rng,
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
        debug_assert!(
            {
                let fresh =
                    crate::spatial_cache::SpatialCache::rebuild_from_world(&state.world);
                state.spatial_cache.get_blocking_positions() == fresh.get_blocking_positions()
                    && state.spatial_cache.get_vision_blocking() == fresh.get_vision_blocking()
            },
            "SpatialCache drifted from world state — some spawn/move/despawn path \
             failed to update it"
        );

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
        let Some(ref mut state) = self.state else { return };
        let ui_state = self.ui_state.as_ref().expect("UI state should exist when state exists");

        let ui_result = process_ui_actions(
            &mut state.world,
            &mut state.grid,
            state.player_entity,
            actions,
            &mut self.dev_menu,
            ui_state,
            &mut self.events,
            state.game_clock.time,
            &mut state.rng,
        );

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

    /// Get player light radius (FOV radius).
    pub fn player_light_radius(&self) -> f32 {
        crate::constants::FOV_RADIUS as f32
    }

    /// Collect light sources for rendering, sorted by distance to player.
    pub fn light_sources(&self) -> Vec<(f32, f32, f32, f32)> {
        let Some(ref state) = self.state else {
            return Vec::new();
        };

        // Get player position for sorting
        let player_pos = state.world
            .get::<&crate::components::VisualPosition>(state.player_entity)
            .map(|vp| (vp.x, vp.y))
            .unwrap_or((0.0, 0.0));

        let mut sources: Vec<_> = state.world
            .query::<(&crate::components::Position, &crate::components::LightSource)>()
            .iter()
            .map(|(_, (pos, light))| (pos.x as f32 + 0.5, pos.y as f32 + 0.5, light.radius, light.intensity))
            .collect();

        // Sort by distance to player so nearby lights are prioritized (MAX_LIGHTS limit)
        sources.sort_by(|a, b| {
            let dist_a = (a.0 - player_pos.0).powi(2) + (a.1 - player_pos.1).powi(2);
            let dist_b = (b.0 - player_pos.0).powi(2) + (b.1 - player_pos.1).powi(2);
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
                let mut actions = crate::ui::UiActions::default();
                actions.start_game = start_result;
                actions
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
                    &state.world,
                    state.player_entity,
                    &state.grid,
                    ui_state,
                    &mut self.dev_menu,
                    camera,
                    tileset,
                    ui_icons,
                    &self.vfx.effects,
                    self.vfx.resting_bubble.as_ref(),
                    &life_drain_beams,
                    &taming_beams,
                    self.input.targeting_mode.as_ref(),
                    self.input.ability_targeting_mode.as_ref(),
                    self.input.mouse_pos,
                    state.game_clock.time,
                )
            }
        }
    }

    // --- Private methods ---

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

        let mut result = InputResult::default();

        // UI toggles
        result.toggle_fullscreen = frame.toggle_fullscreen;

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
            match crate::game::handle_enter_key_container(
                &mut state.world,
                state.player_entity,
                ui_state.open_chest,
                &mut self.events,
            ) {
                crate::game::ContainerAction::TookAll(_) => {
                    ui_state.close_chest();
                    // Clean up empty ground item piles
                    systems::cleanup_empty_ground_piles(&mut state.world);
                }
                crate::game::ContainerAction::Opened(_) => {
                    let _ = process_events_with_audio(
                        &mut self.events,
                        &mut state.world,
                        &state.grid,
                        &mut state.spatial_cache,
                        &mut self.vfx,
                        ui_state,
                        state.player_entity,
                        self.audio.as_ref(),
                    );
                }
                crate::game::ContainerAction::None => {}
            }
        }

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
                &mut state.world,
                &state.grid,
                state.player_entity,
                intent,
                &mut state.game_clock,
                &mut state.action_scheduler,
                &mut state.active_ai_tracker,
                &mut state.spatial_cache,
                &mut self.events,
                &mut self.vfx,
                ui_state,
                self.audio.as_ref(),
                &mut state.rng,
            );

            // Collect skeleton spawn positions before floor transition might invalidate state
            let skeleton_spawns = turn_result.skeleton_spawns.clone();
            let raised_skeletons = turn_result.raised_skeletons.clone();
            let boss_minion_spawns = turn_result.boss_minion_spawns.clone();

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

            // Spawn skeletons from opened coffins
            if !skeleton_spawns.is_empty() {
                let state = self.state.as_mut().expect("State should exist");
                for (x, y) in &skeleton_spawns {
                    let skeleton = spawning::enemies::SKELETON.spawn(&mut state.world, *x, *y);
                    state.spatial_cache.register_entity(skeleton, (*x, *y), true, false);
                    initialization::initialize_single_ai_actor(
                        &mut state.world,
                        &state.grid,
                        skeleton,
                        state.player_entity,
                        &state.game_clock,
                        &mut state.action_scheduler,
                        &mut state.active_ai_tracker,
                        &state.spatial_cache,
                        &mut self.events,
                        &mut state.rng,
                    );
                }
                // Close loot UI - player must deal with skeleton first
                if let Some(ui_state) = self.ui_state.as_mut() {
                    ui_state.close_chest();
                }
            }

            // Spawn friendly skeletons from completed Raise Dead channels
            if !raised_skeletons.is_empty() {
                let state = self.state.as_mut().expect("State should exist");
                for (x, y) in &raised_skeletons {
                    spawn_raised_skeleton(
                        &mut state.world,
                        state.player_entity,
                        *x,
                        *y,
                        &state.game_clock,
                        &mut state.action_scheduler,
                        &mut state.active_ai_tracker,
                        &mut state.spatial_cache,
                    );
                }
            }

            // Spawn boss-summoned spiderlings
            if !boss_minion_spawns.is_empty() {
                let state = self.state.as_mut().expect("State should exist");
                for (boss, (x, y)) in &boss_minion_spawns {
                    spawn_boss_minion(
                        &mut state.world,
                        &state.grid,
                        *boss,
                        *x,
                        *y,
                        state.player_entity,
                        &state.game_clock,
                        &mut state.action_scheduler,
                        &mut state.active_ai_tracker,
                        &mut state.spatial_cache,
                        &mut self.events,
                        &mut state.rng,
                    );
                }
            }
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

        let needs_vfx = dev_spawning::spawn_at_cursor(
            tool,
            self.input.mouse_pos,
            camera,
            &mut state.world,
            &mut state.grid,
            state.player_entity,
            &state.game_clock,
            &mut state.action_scheduler,
            &mut state.active_ai_tracker,
            &state.spatial_cache,
            &mut self.events,
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

        match route {
            Route::Class => self.try_use_class_ability(),
            Route::Secondary => self.try_use_secondary_ability(),
            Route::Ranger(index) => self.try_use_ranger_ability(index),
            Route::Learned => self.try_use_learned_ability(ability_type),
        }
    }

    /// Activate a learned spell (studied scroll) or Raise Dead.
    fn try_use_learned_ability(&mut self, ability_type: AbilityType) {
        let Some(ref mut state) = self.state else {
            return;
        };
        let Some(ref mut ui_state) = self.ui_state else {
            return;
        };

        activate_learned_ability(
            &mut state.world,
            &state.grid,
            state.player_entity,
            ability_type,
            &mut state.game_clock,
            &mut state.action_scheduler,
            &mut state.active_ai_tracker,
            &mut state.spatial_cache,
            &mut self.events,
            &mut self.vfx,
            ui_state,
            &mut self.input,
            &mut state.rng,
        );
    }

    fn try_use_class_ability(&mut self) {
        let Some(ref mut state) = self.state else {
            return;
        };
        let Some(ref mut ui_state) = self.ui_state else {
            return;
        };

        activate_class_ability(
            &mut state.world,
            &state.grid,
            state.player_entity,
            &mut state.game_clock,
            &mut state.action_scheduler,
            &mut state.active_ai_tracker,
            &mut state.spatial_cache,
            &mut self.events,
            &mut self.vfx,
            ui_state,
            &mut self.input,
            &mut state.rng,
        );
    }

    fn try_use_secondary_ability(&mut self) {
        let Some(ref mut state) = self.state else {
            return;
        };
        let Some(ref mut ui_state) = self.ui_state else {
            return;
        };

        activate_secondary_ability(
            &mut state.world,
            &state.grid,
            state.player_entity,
            &mut state.game_clock,
            &mut state.action_scheduler,
            &mut state.active_ai_tracker,
            &mut state.spatial_cache,
            &mut self.events,
            &mut self.vfx,
            ui_state,
            &mut state.rng,
        );
    }

    fn try_use_ranger_ability(&mut self, ability_index: usize) {
        let Some(ref mut state) = self.state else {
            return;
        };
        let Some(ref mut ui_state) = self.ui_state else {
            return;
        };

        activate_ranger_ability(
            &mut state.world,
            &state.grid,
            state.player_entity,
            ability_index,
            &mut state.game_clock,
            &mut state.action_scheduler,
            &mut state.active_ai_tracker,
            &mut state.spatial_cache,
            &mut self.events,
            &mut self.vfx,
            ui_state,
            &mut self.input,
            &mut state.rng,
        );
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

    /// Advance the rest fast-forward by one frame's worth of game-time, stepping
    /// the simulation in small Wait increments so motion stays animated. Stops
    /// when the player is fully healed or an enemy becomes alerted.
    fn update_rest(&mut self, dt: f32) {
        if !self.resting {
            return;
        }

        // How many discrete Wait-steps to run this frame, paced by real time.
        self.rest_accumulator += dt * REST_TIME_SCALE;
        let mut steps =
            (self.rest_accumulator / crate::constants::ACTION_WAIT_DURATION) as i32;
        if steps <= 0 {
            return;
        }
        steps = steps.min(REST_MAX_STEPS_PER_FRAME);
        self.rest_accumulator -= steps as f32 * crate::constants::ACTION_WAIT_DURATION;

        for _ in 0..steps {
            let Some(ref mut state) = self.state else {
                self.resting = false;
                return;
            };
            let Some(ref mut ui_state) = self.ui_state else {
                self.resting = false;
                return;
            };

            // Stop conditions checked before each step.
            if simulation::player_at_full_health(&state.world, state.player_entity) {
                self.stop_rest("You finish resting, fully recovered.");
                return;
            }
            if simulation::any_enemy_alerted(&state.world, state.player_entity) {
                self.stop_rest("Your rest is interrupted!");
                return;
            }
            // Hunger dropped below the threshold mid-rest: healing has
            // stopped, so don't spin (and starve) forever.
            if player_too_hungry_to_recover(&state.world, state.player_entity) {
                self.stop_rest("You are too hungry to keep resting.");
                return;
            }

            let result = execute_player_intent(
                &mut state.world,
                &state.grid,
                state.player_entity,
                crate::systems::player_input::PlayerIntent::Wait,
                &mut state.game_clock,
                &mut state.action_scheduler,
                &mut state.active_ai_tracker,
                &mut state.spatial_cache,
                &mut self.events,
                &mut self.vfx,
                ui_state,
                self.audio.as_ref(),
                &mut state.rng,
            );
            state.fov_dirty = true;

            // A Raise Dead channel can tick over during rest's auto-Waits;
            // don't drop the skeleton on the floor.
            for (x, y) in &result.raised_skeletons {
                spawn_raised_skeleton(
                    &mut state.world,
                    state.player_entity,
                    *x,
                    *y,
                    &state.game_clock,
                    &mut state.action_scheduler,
                    &mut state.active_ai_tracker,
                    &mut state.spatial_cache,
                );
            }

            if result.turn_result != simulation::TurnResult::Started
                || result.enemy_spotted_player
                || result.player_took_damage
            {
                self.stop_rest("Your rest is interrupted!");
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

    /// Advance the sleep fast-forward by one frame's worth of game-time,
    /// stepping the simulation in small Wait increments (same pacing as
    /// rest). Fatigue itself recovers in the survival tick, which is paced by
    /// the game-time these steps generate. Stops when fatigue reaches zero or
    /// the sleeper is interrupted: by taking any damage (attacks, burning —
    /// detected as an HP drop), or by an enemy waking/spotting them.
    fn update_sleep(&mut self, dt: f32) {
        if !self.sleeping {
            return;
        }

        // How many discrete Wait-steps to run this frame, paced by real time.
        self.sleep_accumulator += dt * REST_TIME_SCALE;
        let mut steps =
            (self.sleep_accumulator / crate::constants::ACTION_WAIT_DURATION) as i32;
        if steps <= 0 {
            return;
        }
        steps = steps.min(REST_MAX_STEPS_PER_FRAME);
        self.sleep_accumulator -= steps as f32 * crate::constants::ACTION_WAIT_DURATION;

        for _ in 0..steps {
            let Some(ref mut state) = self.state else {
                self.sleeping = false;
                return;
            };
            let Some(ref mut ui_state) = self.ui_state else {
                self.sleeping = false;
                return;
            };

            // Fully recovered?
            let fatigue = state
                .world
                .get::<&crate::components::Fatigue>(state.player_entity)
                .map(|f| f.value)
                .unwrap_or(0.0);
            if fatigue <= 0.0 {
                self.stop_sleep("You wake up feeling refreshed.");
                return;
            }
            if simulation::any_enemy_alerted(&state.world, state.player_entity) {
                self.stop_sleep("You are jolted awake — something has noticed you!");
                return;
            }

            // Exhaustion can leave the player at 0 energy (no regen while
            // awake); Wait needs the actor able to act, so wait for a point
            // first (regen works while asleep).
            let energy = state
                .world
                .get::<&Actor>(state.player_entity)
                .map(|a| a.energy)
                .unwrap_or(0);
            if energy <= 0 {
                let got = simulation::wait_for_energy(
                    &mut state.world,
                    &state.grid,
                    state.player_entity,
                    1,
                    &mut state.game_clock,
                    &mut state.action_scheduler,
                    &mut state.active_ai_tracker,
                    &mut state.spatial_cache,
                    &mut self.events,
                    &mut state.rng,
                );
                if !got {
                    self.stop_sleep("You wake up.");
                    return;
                }
            }

            // Any HP drop while asleep wakes the player (attacks, burning,
            // traps — starvation is handled separately in the survival tick).
            let hp_before = state
                .world
                .get::<&Health>(state.player_entity)
                .map(|h| h.current)
                .unwrap_or(0);

            let result = execute_player_intent(
                &mut state.world,
                &state.grid,
                state.player_entity,
                crate::systems::player_input::PlayerIntent::Wait,
                &mut state.game_clock,
                &mut state.action_scheduler,
                &mut state.active_ai_tracker,
                &mut state.spatial_cache,
                &mut self.events,
                &mut self.vfx,
                ui_state,
                self.audio.as_ref(),
                &mut state.rng,
            );
            state.fov_dirty = true;

            // As with rest: a Raise Dead channel completing during sleep's
            // auto-Waits still spawns the skeleton.
            for (x, y) in &result.raised_skeletons {
                spawn_raised_skeleton(
                    &mut state.world,
                    state.player_entity,
                    *x,
                    *y,
                    &state.game_clock,
                    &mut state.action_scheduler,
                    &mut state.active_ai_tracker,
                    &mut state.spatial_cache,
                );
            }

            let hp_after = self
                .state
                .as_ref()
                .and_then(|s| s.world.get::<&Health>(s.player_entity).ok().map(|h| h.current))
                .unwrap_or(0);

            if result.turn_result != simulation::TurnResult::Started
                || result.enemy_spotted_player
                || result.player_took_damage
                || hp_after < hp_before
            {
                self.stop_sleep("You are rudely awakened!");
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

        // Take ownership of grid for transition
        let current_grid = std::mem::replace(
            &mut state.grid,
            crate::grid::Grid::new(1, 1),
        );

        let result = handle_floor_transition(
            &mut state.world,
            current_grid,
            &mut state.floors,
            state.current_floor,
            state.seed,
            direction,
            state.player_entity,
            &state.game_clock,
            &mut state.action_scheduler,
            &mut state.active_ai_tracker,
            &mut state.spatial_cache,
            &mut self.events,
        );

        state.grid = result.new_grid;
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
#[allow(clippy::too_many_arguments)]
fn spawn_raised_skeleton(
    world: &mut hecs::World,
    owner: Entity,
    x: i32,
    y: i32,
    clock: &crate::time_system::GameClock,
    scheduler: &mut crate::time_system::ActionScheduler,
    active_ai_tracker: &mut crate::active_ai_tracker::ActiveAITracker,
    spatial_cache: &mut crate::spatial_cache::SpatialCache,
) {
    let skeleton = spawning::enemies::SKELETON.spawn(world, x, y);

    // Convert the hostile stat block into an ally (same path as taming).
    let _ = world.remove_one::<crate::components::ChaseAI>(skeleton);
    let _ = world.remove_one::<crate::components::BlocksMovement>(skeleton);
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

    // Companions don't block movement or vision.
    spatial_cache.register_entity(skeleton, (x, y), false, false);

    // Track and schedule so the companion AI starts acting.
    active_ai_tracker.register_entity(skeleton);
    active_ai_tracker.mark_active(skeleton);
    scheduler.schedule(skeleton, clock.time + 0.1);
}

/// Spawn a boss-summoned Lesser Giant Spider at (x, y): hostile, already
/// alerted to the player, and counted against the boss's minion cap.
#[allow(clippy::too_many_arguments)]
fn spawn_boss_minion(
    world: &mut hecs::World,
    grid: &crate::grid::Grid,
    boss: Entity,
    x: i32,
    y: i32,
    player_entity: Entity,
    clock: &crate::time_system::GameClock,
    scheduler: &mut crate::time_system::ActionScheduler,
    active_ai_tracker: &mut crate::active_ai_tracker::ActiveAITracker,
    spatial_cache: &mut crate::spatial_cache::SpatialCache,
    events: &mut EventQueue,
    rng: &mut impl Rng,
) {
    let spider = spawning::enemies::LESSER_GIANT_SPIDER.spawn(world, x, y);
    let _ = world.insert_one(spider, crate::components::BossMinion { boss });

    // Summoned mid-fight: wide awake and already hunting the summoner's prey.
    let _ = world.remove_one::<crate::components::Asleep>(spider);
    let player_pos = world
        .get::<&crate::components::Position>(player_entity)
        .map(|p| (p.x, p.y))
        .ok();
    if let Ok(mut ai) = world.get::<&mut crate::components::ChaseAI>(spider) {
        ai.state = crate::components::AIState::Chasing;
        ai.add_threat(player_entity, crate::constants::WAKE_THREAT);
        if let Some(pos) = player_pos {
            ai.update_target_pos(player_entity, pos);
        }
    }

    spatial_cache.register_entity(spider, (x, y), true, false);
    initialization::initialize_single_ai_actor(
        world, grid, spider, player_entity, clock, scheduler,
        active_ai_tracker, spatial_cache, events, rng,
    );
}

/// Try to activate the player's class ability.
/// Returns true if the ability was successfully activated.
fn activate_class_ability(
    world: &mut hecs::World,
    grid: &crate::grid::Grid,
    player: Entity,
    game_clock: &mut crate::time_system::GameClock,
    action_scheduler: &mut crate::time_system::ActionScheduler,
    active_ai_tracker: &mut crate::active_ai_tracker::ActiveAITracker,
    spatial_cache: &mut crate::spatial_cache::SpatialCache,
    events: &mut EventQueue,
    vfx: &mut VfxManager,
    ui_state: &mut GameUiState,
    input_state: &mut InputState,
    rng: &mut impl Rng,
) -> bool {
    // Check if player is idle
    let is_idle = world
        .get::<&Actor>(player)
        .map(|a| a.current_action.is_none())
        .unwrap_or(false);

    if !is_idle {
        return false;
    }

    // Get ability info
    let ability_info = world
        .get::<&ClassAbility>(player)
        .ok()
        .map(|a| (a.ability_type, a.is_ready(), a.ability_type.energy_cost()));

    let Some((ability_type, is_ready, energy_cost)) = ability_info else {
        return false;
    };

    if !is_ready {
        return false;
    }

    // Check if player can ever afford this (max_energy >= cost)
    let can_afford = world
        .get::<&Actor>(player)
        .map(|a| a.max_energy >= energy_cost)
        .unwrap_or(false);

    if !can_afford {
        return false;
    }

    // Tame and LifeDrain abilities enter targeting mode instead of executing immediately
    if ability_type == AbilityType::Tame {
        input_state.ability_targeting_mode = Some(input::AbilityTargetingMode {
            ability_type: AbilityType::Tame,
            max_range: crate::constants::TAME_RANGE,
        });
        return true;
    }

    if ability_type == AbilityType::LifeDrain {
        input_state.ability_targeting_mode = Some(input::AbilityTargetingMode {
            ability_type: AbilityType::LifeDrain,
            max_range: crate::constants::LIFE_DRAIN_RANGE,
        });
        return true;
    }

    // Wait for enough energy (this advances time, enemies may act)
    let got_energy = simulation::wait_for_energy(
        world,
        grid,
        player,
        energy_cost,
        game_clock,
        action_scheduler,
        active_ai_tracker,
        spatial_cache,
        events,
        rng,
    );

    if !got_energy {
        // Player died or something went wrong during wait
        let _ = process_events(events, world, grid, spatial_cache, vfx, ui_state, player);
        return false;
    }

    // Determine action type based on ability
    let action_type = match ability_type {
        AbilityType::Cleave => ActionType::Cleave,
        AbilityType::Sprint => ActionType::ActivateSprint,
        AbilityType::Tame => unreachable!("Tame ability handled above with targeting mode"),
        AbilityType::LifeDrain => unreachable!("LifeDrain ability handled above with targeting mode"),
        AbilityType::Barkskin => return false, // Barkskin is a secondary ability, not primary
        AbilityType::Fear => return false,     // Fear is a secondary ability, not primary
        AbilityType::Stun => return false,     // Stun is a secondary ability, not primary
        // Ranger abilities are handled via RangerAbilities component and number keys
        AbilityType::Disengage | AbilityType::Tumble | AbilityType::SnareTrap | AbilityType::CripplingShot => return false,
        AbilityType::Rest => return false, // Rest is handled in try_use_ability, never routed here
        AbilityType::Sleep => return false, // Sleep is handled in try_use_ability, never routed here
        // Learned spells + Raise Dead route through activate_learned_ability
        AbilityType::LearnedBlink
        | AbilityType::LearnedFireball
        | AbilityType::LearnedFear
        | AbilityType::LearnedSlow
        | AbilityType::LearnedProtection
        | AbilityType::LearnedSpeed
        | AbilityType::LearnedInvisibility
        | AbilityType::RaiseDead => return false,
    };

    // Start the action
    let start_result = time_system::start_action(
        world,
        player,
        action_type,
        game_clock,
        action_scheduler,
    );

    if start_result.is_ok() {
        // Start cooldown
        if let Ok(mut ability) = world.get::<&mut ClassAbility>(player) {
            ability.start_cooldown();
        }

        // Advance time and process events
        simulation::advance_until_player_ready(
            world,
            grid,
            player,
            game_clock,
            action_scheduler,
            active_ai_tracker,
            spatial_cache,
            events,
            rng,
        );
    }

    let _ = process_events(events, world, grid, spatial_cache, vfx, ui_state, player);

    start_result.is_ok()
}

/// Activate Druid's secondary ability (Barkskin) when E is pressed.
fn activate_secondary_ability(
    world: &mut hecs::World,
    grid: &crate::grid::Grid,
    player: Entity,
    game_clock: &mut crate::time_system::GameClock,
    action_scheduler: &mut crate::time_system::ActionScheduler,
    active_ai_tracker: &mut crate::active_ai_tracker::ActiveAITracker,
    spatial_cache: &mut crate::spatial_cache::SpatialCache,
    events: &mut EventQueue,
    vfx: &mut VfxManager,
    ui_state: &mut GameUiState,
    rng: &mut impl Rng,
) -> bool {
    use crate::components::SecondaryAbility;

    // Check if player is idle
    let is_idle = world
        .get::<&Actor>(player)
        .map(|a| a.current_action.is_none())
        .unwrap_or(false);

    if !is_idle {
        return false;
    }

    // Get secondary ability info (only Druid has this)
    let ability_info = world
        .get::<&SecondaryAbility>(player)
        .ok()
        .map(|a| (a.ability_type, a.is_ready(), a.ability_type.energy_cost()));

    let Some((ability_type, is_ready, energy_cost)) = ability_info else {
        return false;
    };

    if !is_ready {
        return false;
    }

    // Check if player can ever afford this (max_energy >= cost)
    let can_afford = world
        .get::<&Actor>(player)
        .map(|a| a.max_energy >= energy_cost)
        .unwrap_or(false);

    if !can_afford {
        return false;
    }

    // Wait for enough energy (this advances time, enemies may act)
    let got_energy = simulation::wait_for_energy(
        world,
        grid,
        player,
        energy_cost,
        game_clock,
        action_scheduler,
        active_ai_tracker,
        spatial_cache,
        events,
        rng,
    );

    if !got_energy {
        // Player died or something went wrong during wait
        let _ = process_events(events, world, grid, spatial_cache, vfx, ui_state, player);
        return false;
    }

    // Determine action type based on ability
    let action_type = match ability_type {
        AbilityType::Barkskin => ActionType::ActivateBarkskin,
        AbilityType::Fear => ActionType::ActivateFear,
        AbilityType::Stun => ActionType::ActivateStun,
        _ => return false, // Only Barkskin, Fear and Stun are secondary abilities
    };

    // Start the action
    let start_result = time_system::start_action(
        world,
        player,
        action_type,
        game_clock,
        action_scheduler,
    );

    if start_result.is_ok() {
        // Advance time and process events
        simulation::advance_until_player_ready(
            world,
            grid,
            player,
            game_clock,
            action_scheduler,
            active_ai_tracker,
            spatial_cache,
            events,
            rng,
        );
    }

    let _ = process_events(events, world, grid, spatial_cache, vfx, ui_state, player);

    start_result.is_ok()
}

/// Activate a learned spell (studied from a scroll) or Raise Dead.
///
/// Targeted spells (Blink / Fireball / Raise Dead) enter ability-targeting
/// mode; the click then flows through the normal intent path. Untargeted
/// spells start a `CastLearnedSpell` action immediately. Cooldowns start in
/// `apply_cast_learned_spell` / `apply_start_raise_dead` on success.
#[allow(clippy::too_many_arguments)]
fn activate_learned_ability(
    world: &mut hecs::World,
    grid: &crate::grid::Grid,
    player: Entity,
    ability_type: AbilityType,
    game_clock: &mut crate::time_system::GameClock,
    action_scheduler: &mut crate::time_system::ActionScheduler,
    active_ai_tracker: &mut crate::active_ai_tracker::ActiveAITracker,
    spatial_cache: &mut crate::spatial_cache::SpatialCache,
    events: &mut EventQueue,
    vfx: &mut VfxManager,
    ui_state: &mut GameUiState,
    input_state: &mut InputState,
    rng: &mut impl Rng,
) -> bool {
    use crate::components::LearnedAbilities;

    // Check if player is idle
    let is_idle = world
        .get::<&Actor>(player)
        .map(|a| a.current_action.is_none())
        .unwrap_or(false);
    if !is_idle {
        return false;
    }

    // Look up the spell entry (must be known and off cooldown)
    let is_ready = world
        .get::<&LearnedAbilities>(player)
        .ok()
        .and_then(|la| la.get(ability_type).map(|s| s.cooldown_remaining <= 0.0));
    let Some(is_ready) = is_ready else {
        return false;
    };
    if !is_ready {
        return false;
    }

    // Check if player can ever afford this (max_energy >= cost)
    let energy_cost = ability_type.energy_cost();
    let can_afford = world
        .get::<&Actor>(player)
        .map(|a| a.max_energy >= energy_cost)
        .unwrap_or(false);
    if !can_afford {
        return false;
    }

    // Targeted spells enter targeting mode instead of executing immediately.
    match ability_type {
        AbilityType::RaiseDead => {
            // Enforce the INT-scaled control cap up front, with feedback.
            let int = crate::queries::effective_stats(world, player).intelligence;
            let cap = crate::constants::raise_dead_cap(int);
            let active = systems::actions::raised_undead_count(world);
            if active >= cap {
                ui_state.message_log.system(format!(
                    "You cannot control more than {} raised skeleton{} (1 + 1 per {} INT above 10).",
                    cap,
                    if cap == 1 { "" } else { "s" },
                    crate::constants::RAISE_DEAD_INT_PER_EXTRA,
                ));
                return false;
            }
            input_state.ability_targeting_mode = Some(input::AbilityTargetingMode {
                ability_type: AbilityType::RaiseDead,
                max_range: crate::constants::RAISE_DEAD_RANGE,
            });
            return true;
        }
        AbilityType::LearnedBlink => {
            input_state.ability_targeting_mode = Some(input::AbilityTargetingMode {
                ability_type: AbilityType::LearnedBlink,
                max_range: systems::actions::scaled_blink_range(world, player),
            });
            return true;
        }
        AbilityType::LearnedFireball => {
            input_state.ability_targeting_mode = Some(input::AbilityTargetingMode {
                ability_type: AbilityType::LearnedFireball,
                max_range: crate::constants::FIREBALL_RANGE,
            });
            return true;
        }
        _ => {}
    }

    // Untargeted learned cast: wait for energy, then start the action.
    let got_energy = simulation::wait_for_energy(
        world,
        grid,
        player,
        energy_cost,
        game_clock,
        action_scheduler,
        active_ai_tracker,
        spatial_cache,
        events,
        rng,
    );
    if !got_energy {
        let _ = process_events(events, world, grid, spatial_cache, vfx, ui_state, player);
        return false;
    }

    let start_result = time_system::start_action(
        world,
        player,
        ActionType::CastLearnedSpell {
            ability: ability_type,
            target_x: 0,
            target_y: 0,
        },
        game_clock,
        action_scheduler,
    );

    if start_result.is_ok() {
        simulation::advance_until_player_ready(
            world,
            grid,
            player,
            game_clock,
            action_scheduler,
            active_ai_tracker,
            spatial_cache,
            events,
            rng,
        );
    }

    let _ = process_events(events, world, grid, spatial_cache, vfx, ui_state, player);

    start_result.is_ok()
}

/// Activate a Ranger ability by index (0-3 for keys 1-4).
fn activate_ranger_ability(
    world: &mut hecs::World,
    grid: &crate::grid::Grid,
    player: Entity,
    ability_index: usize,
    game_clock: &mut crate::time_system::GameClock,
    action_scheduler: &mut crate::time_system::ActionScheduler,
    active_ai_tracker: &mut crate::active_ai_tracker::ActiveAITracker,
    spatial_cache: &mut crate::spatial_cache::SpatialCache,
    events: &mut EventQueue,
    vfx: &mut VfxManager,
    ui_state: &mut GameUiState,
    input_state: &mut InputState,
    rng: &mut impl Rng,
) -> bool {
    use crate::components::RangerAbilities;
    use crate::constants::*;

    // Check if player is idle
    let is_idle = world
        .get::<&Actor>(player)
        .map(|a| a.current_action.is_none())
        .unwrap_or(false);

    if !is_idle {
        return false;
    }

    // Get Ranger abilities component
    let ability_info = world
        .get::<&RangerAbilities>(player)
        .ok()
        .and_then(|ra| ra.get(ability_index).cloned());

    let Some((ability_type, cooldown_remaining, _)) = ability_info else {
        return false; // Not a Ranger or invalid index
    };

    // Check if ability is ready
    if cooldown_remaining > 0.0 {
        return false;
    }

    // Get energy cost
    let energy_cost = ability_type.energy_cost();

    // Check if player can afford this
    let can_afford = world
        .get::<&Actor>(player)
        .map(|a| a.max_energy >= energy_cost)
        .unwrap_or(false);

    if !can_afford {
        return false;
    }

    // Handle ability based on type
    match ability_type {
        AbilityType::Disengage => {
            // Disengage is immediate - no targeting needed
            let got_energy = simulation::wait_for_energy(
                world, grid, player, energy_cost, game_clock, action_scheduler,
                active_ai_tracker, spatial_cache, events, rng,
            );

            if !got_energy {
                let _ = process_events(events, world, grid, spatial_cache, vfx, ui_state, player);
                return false;
            }

            // Start the action
            let start_result = time_system::start_action(
                world, player, ActionType::Disengage, game_clock, action_scheduler,
            );

            if start_result.is_ok() {
                // Start cooldown
                if let Ok(mut ra) = world.get::<&mut RangerAbilities>(player) {
                    ra.start_cooldown(ability_index);
                }

                simulation::advance_until_player_ready(
                    world, grid, player, game_clock, action_scheduler,
                    active_ai_tracker, spatial_cache, events, rng,
                );
            }

            let _ = process_events(events, world, grid, spatial_cache, vfx, ui_state, player);
            start_result.is_ok()
        }
        AbilityType::Tumble => {
            // Enter targeting mode
            input_state.ability_targeting_mode = Some(input::AbilityTargetingMode {
                ability_type: AbilityType::Tumble,
                max_range: TUMBLE_DISTANCE,
            });
            true
        }
        AbilityType::SnareTrap => {
            // Enter targeting mode (adjacent only)
            input_state.ability_targeting_mode = Some(input::AbilityTargetingMode {
                ability_type: AbilityType::SnareTrap,
                max_range: SNARE_TRAP_RANGE,
            });
            true
        }
        AbilityType::CripplingShot => {
            // Enter targeting mode (bow range)
            input_state.ability_targeting_mode = Some(input::AbilityTargetingMode {
                ability_type: AbilityType::CripplingShot,
                max_range: BOW_RANGE,
            });
            true
        }
        _ => false, // Not a Ranger ability
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

    /// A raised skeleton is a true companion: standard skeleton stat block
    /// wired into the tamed-ally infrastructure (CompanionAI + TamedBy +
    /// RaisedUndead), not a hostile — and it doesn't block its owner's path.
    #[test]
    fn test_spawn_raised_skeleton_is_companion_not_hostile() {
        let mut world = hecs::World::new();
        let owner = world.spawn((crate::components::Position::new(1, 1),));

        let clock = crate::time_system::GameClock::new();
        let mut scheduler = crate::time_system::ActionScheduler::new();
        let mut tracker = crate::active_ai_tracker::ActiveAITracker::new();
        let mut cache = crate::spatial_cache::SpatialCache::rebuild_from_world(&world);

        spawn_raised_skeleton(
            &mut world, owner, 2, 1, &clock, &mut scheduler, &mut tracker, &mut cache,
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
        assert!(world.get::<&crate::components::BlocksMovement>(skeleton).is_err());
        assert!(world.get::<&crate::components::Asleep>(skeleton).is_err());
        // It keeps the skeleton stat block (alive, counted against the cap).
        assert_eq!(
            crate::systems::actions::raised_undead_count(&world),
            1,
            "the new companion counts against the raise cap"
        );
    }
}
