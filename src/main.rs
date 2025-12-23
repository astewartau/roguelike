#![allow(dead_code)]

mod app;
mod camera;
mod components;
mod constants;
mod dungeon_gen;
mod events;
mod fov;
mod game;
mod game_loop;
mod grid;
mod input;
mod pathfinding;
mod queries;
mod renderer;
mod spawning;
mod systems;
mod tile;
mod tileset;
mod time_system;
mod ui;
mod vfx;

use camera::Camera;
use constants::*;
use grid::Grid;
use hecs::World;
use renderer::Renderer;
use tileset::Tileset;
use std::sync::Arc;
use std::time::Instant;

use glutin::prelude::*;
use glutin::surface::WindowSurface;
use winit::application::ApplicationHandler;
use winit::event::{ElementState, MouseButton, MouseScrollDelta, WindowEvent};
use winit::event_loop::{ActiveEventLoop, EventLoop};
use winit::keyboard::{KeyCode, PhysicalKey};
use winit::window::{Window, WindowId};

use egui_glow::EguiGlow;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let event_loop = EventLoop::new()?;
    let mut app = App::new();
    event_loop.run_app(&mut app)?;
    Ok(())
}

struct App {
    state: Option<AppState>,
}

struct AppState {
    // Window and GL
    window: Window,
    gl_surface: glutin::surface::Surface<WindowSurface>,
    gl_context: glutin::context::PossiblyCurrentContext,
    gl: Arc<glow::Context>,
    egui_glow: EguiGlow,

    // Rendering
    camera: Camera,
    renderer: Renderer,
    tileset: Tileset,
    ui_icons: ui::UiIcons,

    // Game state
    grid: Grid,
    world: World,
    player_entity: hecs::Entity,
    vfx: vfx::VfxManager,
    events: events::EventQueue,

    // Multi-floor dungeon state
    current_floor: u32,
    floors: std::collections::HashMap<u32, game::SavedFloor>,

    // Time system
    game_clock: time_system::GameClock,
    action_scheduler: time_system::ActionScheduler,

    // UI state
    ui_state: ui::GameUiState,
    dev_menu: ui::DevMenu,

    // Input state
    input: input::InputState,

    // Timing
    last_frame_time: Instant,
}

impl App {
    fn new() -> Self {
        Self { state: None }
    }
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.state.is_some() {
            return;
        }

        // Create window and GL context
        let app::WindowContext {
            window,
            gl_surface,
            gl_context,
            gl,
            mut egui_glow,
        } = app::create_window(event_loop);

        // Initialize game state
        let size = window.inner_size();
        let mut camera = Camera::new(size.width as f32, size.height as f32);
        let grid = Grid::new(DUNGEON_DEFAULT_WIDTH, DUNGEON_DEFAULT_HEIGHT);
        let renderer = Renderer::new(gl.clone()).expect("Failed to create renderer");

        // Load tileset
        let tileset = Tileset::load(gl.clone(), std::path::Path::new("assets/minirogue-all.tsj"))
            .expect("Failed to load tileset");

        // Register tileset texture with egui and create UI icons
        let tileset_egui_id = egui_glow.painter.register_native_texture(tileset.texture);
        let ui_icons = ui::UiIcons::new(&tileset, tileset_egui_id);

        // Initialize game world
        let (mut world, player_entity, player_start) = game::init_world(&grid);
        game::setup_camera(&mut camera, &player_start);

        // Initialize time system
        let game_clock = time_system::GameClock::new();
        let mut action_scheduler = time_system::ActionScheduler::new();
        let mut event_queue = events::EventQueue::new();

        // Initialize AI entities with their first actions
        let mut rng = rand::thread_rng();
        game::initialize_ai_actors(
            &mut world,
            &grid,
            player_entity,
            &game_clock,
            &mut action_scheduler,
            &mut event_queue,
            &mut rng,
        );

        self.state = Some(AppState {
            window,
            gl_surface,
            gl_context,
            gl,
            egui_glow,
            camera,
            renderer,
            tileset,
            ui_icons,
            grid,
            world,
            player_entity,
            vfx: vfx::VfxManager::new(),
            events: event_queue,
            current_floor: 0,
            floors: std::collections::HashMap::new(),
            game_clock,
            action_scheduler,
            ui_state: ui::GameUiState::new(player_entity),
            dev_menu: ui::DevMenu::new(),
            input: input::InputState::new(),
            last_frame_time: Instant::now(),
        });
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        let state = match &mut self.state {
            Some(s) => s,
            None => return,
        };

        // Let egui handle the event first
        let egui_consumed = state.egui_glow.on_window_event(&state.window, &event);

        match event {
            WindowEvent::CloseRequested => {
                event_loop.exit();
            }
            WindowEvent::Resized(size) => {
                app::resize_surface(&state.gl_surface, &state.gl_context, size.width, size.height);
                state.camera.viewport_width = size.width as f32;
                state.camera.viewport_height = size.height as f32;
            }
            WindowEvent::KeyboardInput { event, .. } => {
                if !egui_consumed.consumed {
                    if let PhysicalKey::Code(key) = event.physical_key {
                        match event.state {
                            ElementState::Pressed => {
                                if key == KeyCode::Escape {
                                    // If in targeting mode, cancel it instead of exiting
                                    if state.input.is_targeting() {
                                        state.input.cancel_targeting();
                                    } else {
                                        event_loop.exit();
                                    }
                                }
                                if key == KeyCode::Backquote {
                                    state.dev_menu.toggle();
                                }
                                state.input.keys_pressed.insert(key);
                            }
                            ElementState::Released => {
                                state.input.keys_pressed.remove(&key);
                            }
                        }
                    }
                }
            }
            WindowEvent::CursorMoved { position, .. } => {
                state.input.last_mouse_pos = state.input.mouse_pos;
                state.input.mouse_pos = (position.x as f32, position.y as f32);
            }
            WindowEvent::MouseInput { state: btn_state, button, .. } => {
                if !egui_consumed.consumed && button == MouseButton::Left {
                    let was_down = state.input.mouse_down;
                    state.input.mouse_down = btn_state == ElementState::Pressed;

                    if btn_state == ElementState::Released {
                        // Check if this was a click (not a drag)
                        let dx = state.input.mouse_pos.0 - state.input.last_mouse_pos.0;
                        let dy = state.input.mouse_pos.1 - state.input.last_mouse_pos.1;
                        let was_drag = was_down
                            && (dx.abs() > CLICK_DRAG_THRESHOLD || dy.abs() > CLICK_DRAG_THRESHOLD);

                        if !was_drag {
                            // Check if in targeting mode first
                            if state.input.is_targeting() {
                                state.handle_targeting_click();
                            } else if state.dev_menu.has_active_tool() {
                                // Check if a dev tool is active
                                state.handle_dev_spawn();
                            } else {
                                input::handle_click_to_move(
                                    &mut state.input,
                                    &state.camera,
                                    &state.world,
                                    &state.grid,
                                    state.player_entity,
                                );
                            }
                        }

                        state.camera.release_pan();
                    }
                }
                // Handle right-click for shooting bow or canceling targeting
                if !egui_consumed.consumed && button == MouseButton::Right {
                    if btn_state == ElementState::Released {
                        if state.input.is_targeting() {
                            // Right-click cancels targeting mode
                            state.input.cancel_targeting();
                        } else {
                            state.handle_right_click_shoot();
                        }
                    }
                }
            }
            WindowEvent::MouseWheel { delta, .. } => {
                if !egui_consumed.consumed {
                    let scroll = match delta {
                        MouseScrollDelta::LineDelta(_, y) => y * 2.0,
                        MouseScrollDelta::PixelDelta(pos) => pos.y as f32 * 0.1,
                    };
                    state.camera.add_zoom_impulse(
                        scroll,
                        state.input.mouse_pos.0,
                        state.input.mouse_pos.1,
                    );
                }
            }
            WindowEvent::RedrawRequested => {
                state.update_and_render();
                state.window.request_redraw();
            }
            _ => {}
        }
    }

    fn about_to_wait(&mut self, _event_loop: &ActiveEventLoop) {
        if let Some(state) = &self.state {
            state.window.request_redraw();
        }
    }
}

impl AppState {
    fn update_and_render(&mut self) {
        let current_time = Instant::now();
        let raw_dt = (current_time - self.last_frame_time).as_secs_f32();
        self.last_frame_time = current_time;

        // Cap dt to prevent animation snapping after long frames (e.g., lots of AI processing)
        let dt = raw_dt.min(MAX_ANIMATION_DT);

        // Handle input
        self.handle_input();

        // Update animations
        systems::update_lunge_animations(&mut self.world, dt);
        self.vfx.update(dt);

        // Remove dead entities (turn into lootable bones, grant XP)
        let mut rng = rand::thread_rng();
        systems::remove_dead_entities(
            &mut self.world,
            self.player_entity,
            &mut rng,
            &mut self.events,
            Some(&mut self.action_scheduler),
        );
        // Process any events from remove_dead_entities (death VFX, etc.)
        let event_result = game_loop::process_events(&mut self.events, &mut self.world, &self.grid, &mut self.vfx, &mut self.ui_state, self.player_entity);
        if let Some(direction) = event_result.floor_transition {
            self.handle_floor_transition(direction);
        }
        // Stop pursuit if player attacked or took damage
        if event_result.player_attacked || event_result.player_took_damage || event_result.enemy_spotted_player {
            self.input.clear_path();
        }

        // Lerp all visual positions toward logical positions
        systems::visual_lerp(&mut self.world, dt);

        // Lerp projectiles at their own speed (faster than normal entities)
        systems::lerp_projectiles_realtime(&mut self.world, dt, ARROW_SPEED);

        // Clean up finished projectiles whose visuals have caught up
        let finished = systems::cleanup_finished_projectiles(&self.world);
        systems::despawn_projectiles(&mut self.world, finished);

        // Update camera to follow player's visual position
        if let Ok(vis_pos) = self.world.get::<&components::VisualPosition>(self.player_entity) {
            self.camera
                .set_tracking_target(glam::Vec2::new(vis_pos.x, vis_pos.y));
        }

        // Update camera (pass mouse_down so momentum doesn't apply while dragging)
        self.camera.update(dt, self.input.mouse_down);

        // Update FOV
        systems::update_fov(&self.world, &mut self.grid, self.player_entity, FOV_RADIUS);

        // Collect entities for rendering
        let entities_to_render =
            systems::collect_renderables(&self.world, &self.grid, self.player_entity);

        // Run UI
        let ui_actions = self.run_ui();

        // Handle UI actions
        self.process_ui_actions(ui_actions);

        // Render
        unsafe {
            use glow::HasContext;
            self.gl.clear_color(0.1, 0.1, 0.1, 1.0);
            self.gl.clear(glow::COLOR_BUFFER_BIT);
        }

        self.renderer
            .render(&self.camera, &self.grid, &self.tileset, self.ui_state.show_grid_lines)
            .unwrap();
        self.renderer
            .render_decals(&self.camera, &self.grid, &self.tileset)
            .unwrap();
        self.renderer
            .render_entities(&self.camera, &entities_to_render, &self.tileset)
            .unwrap();
        self.renderer.render_vfx(&self.camera, &self.vfx.effects);
        self.renderer.render_fire(&self.camera, &self.vfx.fires);

        // Render egui
        self.egui_glow.paint(&self.window);

        // Swap buffers
        self.gl_surface.swap_buffers(&self.gl_context).unwrap();
    }

    fn run_ui(&mut self) -> ui::UiActions {
        let mut actions = ui::UiActions::default();

        // Get status bar data
        let status_data = ui::get_status_bar_data(&self.world, self.player_entity);

        // Get loot window data if chest is open
        let loot_data = ui::get_loot_window_data(
            &self.world,
            self.ui_state.open_chest,
            self.camera.viewport_width,
            self.camera.viewport_height,
        );

        // Get dialogue window data if talking to an NPC
        let dialogue_data = ui::get_dialogue_window_data(
            &self.world,
            self.ui_state.talking_to,
            self.camera.viewport_width,
            self.camera.viewport_height,
        );

        let icons = &self.ui_icons;
        let show_inventory = self.ui_state.show_inventory;
        let world = &self.world;
        let player_entity = self.player_entity;
        let viewport_width = self.camera.viewport_width;
        let viewport_height = self.camera.viewport_height;
        let vfx_effects = &self.vfx.effects;
        let camera = &self.camera;
        let tileset = &self.tileset;
        let dev_menu = &mut self.dev_menu;

        // Prepare player buff aura data
        let buff_aura_data = {
            let player_vis_pos = self.world.get::<&components::VisualPosition>(self.player_entity)
                .ok()
                .map(|p| (p.x, p.y));
            let has_regen = self.world.get::<&components::StatusEffects>(self.player_entity)
                .map(|e| e.has_effect(components::EffectType::Regenerating))
                .unwrap_or(false);
            let has_protected = self.world.get::<&components::StatusEffects>(self.player_entity)
                .map(|e| e.has_effect(components::EffectType::Protected))
                .unwrap_or(false);

            player_vis_pos.map(|(x, y)| ui::PlayerBuffAuraData {
                player_x: x,
                player_y: y,
                has_regen,
                has_protected,
                time: self.game_clock.time,
            })
        };

        // Prepare targeting overlay data if in targeting mode
        let targeting_data = if let Some(ref targeting) = self.input.targeting_mode {
            // Get player position
            let player_pos = self.world.get::<&components::Position>(self.player_entity)
                .ok()
                .map(|p| (p.x, p.y))
                .unwrap_or((0, 0));

            // Get cursor tile from mouse position
            let world_pos = self.camera.screen_to_world(
                self.input.mouse_pos.0,
                self.input.mouse_pos.1,
            );
            let cursor_x = world_pos.x.floor() as i32;
            let cursor_y = world_pos.y.floor() as i32;

            Some(ui::TargetingOverlayData {
                player_x: player_pos.0,
                player_y: player_pos.1,
                cursor_x,
                cursor_y,
                max_range: targeting.max_range,
                radius: targeting.radius,
                is_blink: matches!(targeting.item_type, components::ItemType::ScrollOfBlink),
            })
        } else {
            None
        };

        // Collect enemy status effect data for visible enemies
        let enemy_status_data: Vec<ui::EnemyStatusData> = {
            use components::{ChaseAI, StatusEffects, VisualPosition, EffectType};
            self.world
                .query::<(&VisualPosition, &ChaseAI, &StatusEffects)>()
                .iter()
                .filter(|(_, (pos, _, _))| {
                    // Only show for visible tiles
                    self.grid.get(pos.x as i32, pos.y as i32)
                        .map(|t| t.visible)
                        .unwrap_or(false)
                })
                .map(|(_, (pos, _, effects))| ui::EnemyStatusData {
                    x: pos.x,
                    y: pos.y,
                    is_feared: effects.has_effect(EffectType::Feared),
                    is_slowed: effects.has_effect(EffectType::Slowed),
                    is_confused: effects.has_effect(EffectType::Confused),
                })
                .collect()
        };
        let game_time = self.game_clock.time;

        self.egui_glow.run(&self.window, |ctx| {
            // Player buff auras (draw first so they're behind everything)
            ui::draw_player_buff_auras(ctx, camera, buff_aura_data.as_ref());

            // Targeting overlay (draw first so it's behind other UI)
            if let Some(ref data) = targeting_data {
                ui::draw_targeting_overlay(ctx, camera, data);
            }

            // Status bar (always visible)
            ui::draw_status_bar(ctx, &status_data, icons);

            // Floating damage numbers
            ui::draw_damage_numbers(ctx, vfx_effects, camera);

            // Alert indicators (enemy spotted player)
            ui::draw_alert_indicators(ctx, vfx_effects, camera);

            // Enemy status effect indicators (fear, slow, confusion)
            ui::draw_enemy_status_indicators(ctx, camera, &enemy_status_data, game_time);

            // Explosion effects (fireball)
            ui::draw_explosions(ctx, vfx_effects, camera);

            // Developer menu
            ui::draw_dev_menu(ctx, dev_menu, icons.tileset_texture_id, tileset);

            // Loot window (if chest is open)
            if let Some(ref data) = loot_data {
                ui::draw_loot_window(
                    ctx,
                    data,
                    icons.tileset_texture_id,
                    icons.coins_uv,
                    icons.potion_uv,
                    icons.scroll_uv,
                    &mut actions,
                );
            }

            // Dialogue window (if talking to NPC)
            if let Some(ref data) = dialogue_data {
                ui::draw_dialogue_window(ctx, data, &mut actions);
            }

            // Inventory window (if toggled)
            if show_inventory {
                let inv_data = ui::InventoryWindowData {
                    viewport_width,
                    viewport_height,
                };
                ui::draw_inventory_window(
                    ctx,
                    world,
                    player_entity,
                    &inv_data,
                    icons.tileset_texture_id,
                    icons.sword_uv,
                    icons.bow_uv,
                    icons.coins_uv,
                    icons.potion_uv,
                    icons.scroll_uv,
                    &mut actions,
                );
            }
        });

        actions
    }

    fn process_ui_actions(&mut self, actions: ui::UiActions) {
        // Handle dev menu item giving
        if let Some(item) = self.dev_menu.take_item_to_give() {
            // Add item to player's inventory (dev tool - no weight limit)
            if let Ok(mut inv) = self.world.get::<&mut components::Inventory>(self.player_entity) {
                let weight = systems::item_weight(item);
                inv.items.push(item);
                inv.current_weight_kg += weight;
            }
        }

        // Handle chest/loot interactions
        if let Some(chest_id) = self.ui_state.open_chest {
            if actions.chest_take_all || actions.close_chest {
                if actions.chest_take_all {
                    systems::take_all_from_container(&mut self.world, self.player_entity, chest_id, Some(&mut self.events));
                }
                self.ui_state.close_chest();
            } else if actions.chest_take_gold {
                systems::take_gold_from_container(&mut self.world, self.player_entity, chest_id, Some(&mut self.events));
            } else if let Some(item_index) = actions.chest_item_to_take {
                systems::take_item_from_container(
                    &mut self.world,
                    self.player_entity,
                    chest_id,
                    item_index,
                    Some(&mut self.events),
                );
            }
        }

        // Handle dialogue interactions
        if let Some(npc_id) = self.ui_state.talking_to {
            if let Some(option_index) = actions.dialogue_option_selected {
                // Get the dialogue component and advance it
                if let Ok(mut dialogue) = self.world.get::<&mut components::Dialogue>(npc_id) {
                    let continues = dialogue.select_option(option_index);
                    if !continues {
                        // Dialogue ended - close window and reset for next conversation
                        dialogue.reset();
                        self.ui_state.close_dialogue();
                    }
                } else {
                    // NPC no longer has dialogue component - close window
                    self.ui_state.close_dialogue();
                }
            }
        }

        // Use item if clicked
        if let Some(item_index) = actions.item_to_use {
            let result = systems::use_item(&mut self.world, self.player_entity, item_index);

            // Handle special item use results
            match result {
                systems::ItemUseResult::RequiresTarget { item_type, item_index } => {
                    // Enter targeting mode
                    use crate::components::ItemType;
                    let (max_range, radius) = match item_type {
                        ItemType::ScrollOfBlink => (constants::BLINK_RANGE, 0),
                        ItemType::ScrollOfFireball => (constants::FIREBALL_RANGE, constants::FIREBALL_RADIUS),
                        _ => (8, 0), // Default fallback
                    };
                    self.input.enter_targeting_mode(item_type, item_index, max_range, radius);
                }
                systems::ItemUseResult::RevealEnemies => {
                    systems::reveal_enemies(&self.world, &mut self.grid);
                    systems::remove_item_from_inventory(&mut self.world, self.player_entity, item_index);
                }
                systems::ItemUseResult::RevealMap => {
                    systems::reveal_entire_map(&mut self.grid);
                    systems::remove_item_from_inventory(&mut self.world, self.player_entity, item_index);
                }
                systems::ItemUseResult::ApplyFearToVisible => {
                    self.apply_effect_to_visible_enemies(components::EffectType::Feared, constants::FEAR_DURATION);
                    systems::remove_item_from_inventory(&mut self.world, self.player_entity, item_index);
                }
                systems::ItemUseResult::ApplySlowToVisible => {
                    self.apply_effect_to_visible_enemies(components::EffectType::Slowed, constants::SLOW_DURATION);
                    systems::remove_item_from_inventory(&mut self.world, self.player_entity, item_index);
                }
                systems::ItemUseResult::IsThrowable { item_type, item_index } => {
                    // Equip the throwable in the ranged slot
                    let tile_id = systems::item_tile_id(item_type);
                    if let Ok(mut equipment) = self.world.get::<&mut components::Equipment>(self.player_entity) {
                        equipment.ranged = Some(components::RangedSlot::Throwable { item_type, tile_id });
                    }
                    systems::remove_item_from_inventory(&mut self.world, self.player_entity, item_index);
                }
                _ => {}
            }
        }
    }

    fn handle_input(&mut self) {
        // Process keyboard input (pure input handling - no game logic)
        let result = input::process_keyboard(&mut self.input);

        // Handle toggle actions
        if result.toggle_fullscreen {
            use winit::window::Fullscreen;
            let fullscreen = if self.window.fullscreen().is_some() {
                None
            } else {
                Some(Fullscreen::Borderless(None))
            };
            self.window.set_fullscreen(fullscreen);
        }

        if result.toggle_inventory {
            self.ui_state.toggle_inventory();
        }

        if result.toggle_grid_lines {
            self.ui_state.toggle_grid_lines();
        }

        // Enter key: Take All if chest open, otherwise open chest at player position
        if result.enter_pressed {
            if let Some(chest_id) = self.ui_state.open_chest {
                systems::take_all_from_container(&mut self.world, self.player_entity, chest_id, Some(&mut self.events));
                self.ui_state.close_chest();
            } else if let Some(container_id) =
                systems::find_container_at_player(&self.world, self.player_entity)
            {
                self.events.push(crate::events::GameEvent::ContainerOpened {
                    container: container_id,
                    opener: self.player_entity,
                });
                // Process immediately so UI updates this frame
                let _ = game_loop::process_events(&mut self.events, &mut self.world, &self.grid, &mut self.vfx, &mut self.ui_state, self.player_entity);
            }
        }

        // Check if player is dead - no movement allowed
        let is_dead = self
            .world
            .get::<&components::Health>(self.player_entity)
            .map(|h| h.is_dead())
            .unwrap_or(true);

        if is_dead {
            self.input.clear_path();
            input::process_mouse_drag(&mut self.input, &mut self.camera, self.ui_state.show_inventory);
            return;
        }

        // Handle attack direction (Shift+movement) - takes priority over regular movement
        if let Some((dx, dy)) = result.attack_direction {
            self.input.clear_path();
            let intent = systems::player_input::PlayerIntent::AttackDirection { dx, dy };
            let _turn_result = game_loop::execute_player_intent(
                &mut self.world,
                &self.grid,
                self.player_entity,
                intent,
                &mut self.game_clock,
                &mut self.action_scheduler,
                &mut self.events,
                &mut self.vfx,
                &mut self.ui_state,
            );
            // Process mouse drag and return early
            input::process_mouse_drag(&mut self.input, &mut self.camera, self.ui_state.show_inventory);
            return;
        }

        // Determine movement intent (keyboard takes priority over click-to-move)
        let (movement_intent, from_keyboard) = if let Some((dx, dy)) = result.movement {
            // Keyboard movement cancels click-to-move path
            self.input.clear_path();
            (Some((dx, dy)), true)
        } else {
            // Update pursuit path if chasing an enemy (recalculates path to moving target)
            input::update_pursuit(&mut self.input, &self.world, &self.grid, self.player_entity);
            // Try click-to-move path
            (input::get_path_movement(&self.input, &self.world, self.player_entity), false)
        };

        // Execute movement if we have an intent
        if let Some((dx, dy)) = movement_intent {
            // Validate target tile is walkable
            let player_pos = self
                .world
                .get::<&components::Position>(self.player_entity)
                .ok()
                .map(|p| (p.x, p.y));

            let tile_walkable = player_pos
                .and_then(|(px, py)| self.grid.get(px + dx, py + dy))
                .map(|t| t.tile_type.is_walkable())
                .unwrap_or(false);

            if tile_walkable {
                // Execute the turn via game_loop (handles time advancement, AI, events, UI state)
                let turn_result = game_loop::execute_player_turn(
                    &mut self.world,
                    &self.grid,
                    self.player_entity,
                    dx,
                    dy,
                    &mut self.game_clock,
                    &mut self.action_scheduler,
                    &mut self.events,
                    &mut self.vfx,
                    &mut self.ui_state,
                );

                // Handle path consumption based on result
                match turn_result.turn_result {
                    game_loop::TurnResult::Started => {
                        // For path-following, consume the step we just took
                        if !from_keyboard {
                            self.input.consume_step();

                            // Check if we've arrived at destination - auto-loot any container (bones)
                            if self.input.has_arrived() {
                                self.input.clear_destination();
                                if let Some(container_id) =
                                    systems::find_container_at_player(&self.world, self.player_entity)
                                {
                                    self.events.push(crate::events::GameEvent::ContainerOpened {
                                        container: container_id,
                                        opener: self.player_entity,
                                    });
                                }
                            }
                        }
                        // Handle floor transition if player used stairs
                        if let Some(direction) = turn_result.floor_transition {
                            self.handle_floor_transition(direction);
                        }
                    }
                    game_loop::TurnResult::Blocked | game_loop::TurnResult::NotReady => {
                        // Clear path on blocked movement
                        self.input.clear_path();
                    }
                }

                // Stop pursuit if player attacked, took damage, or enemy spotted them
                if turn_result.player_attacked || turn_result.player_took_damage || turn_result.enemy_spotted_player {
                    self.input.clear_path();
                }
            }
        }

        // Process mouse drag for camera panning
        input::process_mouse_drag(&mut self.input, &mut self.camera, self.ui_state.show_inventory);
    }

    fn handle_dev_spawn(&mut self) {
        let Some(tool) = self.dev_menu.selected_tool else {
            return;
        };

        // Convert mouse position to world coordinates
        let world_pos = self.camera.screen_to_world(
            self.input.mouse_pos.0,
            self.input.mouse_pos.1,
        );

        // Round to get tile coordinates
        let tile_x = world_pos.x.round() as i32;
        let tile_y = world_pos.y.round() as i32;

        // Execute the spawn via the dev_tools system
        let result = systems::dev_tools::execute_dev_spawn(
            &mut self.world,
            &mut self.grid,
            tool,
            tile_x,
            tile_y,
            self.player_entity,
            &self.game_clock,
            &mut self.action_scheduler,
            &mut self.events,
        );

        // Handle VFX requests (not ECS, stays here)
        if matches!(result, systems::dev_tools::DevSpawnResult::VfxRequested) {
            if matches!(tool, ui::DevTool::SpawnFire) {
                self.vfx.spawn_fire(tile_x as f32 + 0.5, tile_y as f32 + 0.5);
            }
        }
    }

    /// Apply an effect to all enemies visible to the player
    fn apply_effect_to_visible_enemies(&mut self, effect: components::EffectType, duration: f32) {
        // Get player position
        let player_pos = queries::get_entity_position(&self.world, self.player_entity)
            .unwrap_or((0, 0));

        systems::effects::apply_effect_to_visible_enemies(
            &mut self.world,
            &self.grid,
            player_pos,
            constants::FOV_RADIUS,
            effect,
            duration,
        );
    }

    fn handle_right_click_shoot(&mut self) {
        // Check if player has ranged weapon equipped
        if !systems::player_input::has_ranged_equipped(&self.world, self.player_entity) {
            return;
        }

        // Get target tile from mouse position
        let (target_x, target_y) = input::get_shoot_target(&self.input, &self.camera);

        // Clear any click-to-move path
        self.input.clear_path();

        // Execute via unified intent system
        let intent = systems::player_input::PlayerIntent::ShootRanged { target_x, target_y };
        let result = game_loop::execute_player_intent(
            &mut self.world,
            &self.grid,
            self.player_entity,
            intent,
            &mut self.game_clock,
            &mut self.action_scheduler,
            &mut self.events,
            &mut self.vfx,
            &mut self.ui_state,
        );

        // Stop pursuit if player attacked, took damage, or enemy spotted them
        if result.player_attacked || result.player_took_damage || result.enemy_spotted_player {
            self.input.clear_path();
        }
    }

    /// Handle a click while in targeting mode (for Blink, Fireball, etc.)
    fn handle_targeting_click(&mut self) {
        // Get targeting info before we potentially consume it
        let targeting = match self.input.targeting_mode.clone() {
            Some(t) => t,
            None => return,
        };

        // Get target tile from mouse position
        let world_pos = self.camera.screen_to_world(
            self.input.mouse_pos.0,
            self.input.mouse_pos.1,
        );
        let target_x = world_pos.x.floor() as i32;
        let target_y = world_pos.y.floor() as i32;

        // Get player position for validation
        let player_pos = match self.world.get::<&components::Position>(self.player_entity) {
            Ok(p) => (p.x, p.y),
            Err(_) => {
                self.input.cancel_targeting();
                return;
            }
        };

        // Validate targeting using player_input system
        let validation = systems::player_input::validate_targeting(
            &self.world,
            &self.grid,
            player_pos,
            target_x,
            target_y,
            &targeting,
        );

        if validation != systems::player_input::TargetingValidation::Valid {
            // Invalid target - ignore click (don't cancel targeting, let player try again)
            return;
        }

        // Remove the item from inventory before executing
        systems::remove_item_from_inventory(&mut self.world, self.player_entity, targeting.item_index);

        // Exit targeting mode before starting action
        self.input.cancel_targeting();

        // Execute via unified intent system
        let intent = systems::player_input::PlayerIntent::UseTargetedAbility {
            item_type: targeting.item_type,
            item_index: targeting.item_index,
            target_x,
            target_y,
        };
        let result = game_loop::execute_player_intent(
            &mut self.world,
            &self.grid,
            self.player_entity,
            intent,
            &mut self.game_clock,
            &mut self.action_scheduler,
            &mut self.events,
            &mut self.vfx,
            &mut self.ui_state,
        );

        // Stop pursuit if player attacked, took damage, or enemy spotted them
        if result.player_attacked || result.player_took_damage || result.enemy_spotted_player {
            self.input.clear_path();
        }
    }

    /// Handle a floor transition (going up or down stairs)
    fn handle_floor_transition(&mut self, direction: events::StairDirection) {
        use events::StairDirection;

        let target_floor = match direction {
            StairDirection::Down => self.current_floor + 1,
            StairDirection::Up => {
                if self.current_floor == 0 {
                    return; // Can't go up from floor 0
                }
                self.current_floor - 1
            }
        };

        // Save current floor
        let old_grid = std::mem::replace(
            &mut self.grid,
            Grid::new_floor(DUNGEON_DEFAULT_WIDTH, DUNGEON_DEFAULT_HEIGHT, target_floor),
        );
        let saved_floor = game::save_floor(&self.world, old_grid, self.player_entity);
        self.floors.insert(self.current_floor, saved_floor);

        // Clear current floor entities
        game::clear_floor_entities(&mut self.world, self.player_entity, &mut self.action_scheduler);

        // Load or generate target floor
        if let Some(saved) = self.floors.remove(&target_floor) {
            // Determine spawn position (near opposite stairs)
            let spawn_pos = match direction {
                StairDirection::Down => {
                    // Coming from above, spawn at stairs up
                    saved.grid.stairs_up_pos.unwrap_or((1, 1))
                }
                StairDirection::Up => {
                    // Coming from below, spawn at stairs down
                    saved.grid.stairs_down_pos.unwrap_or((1, 1))
                }
            };

            // Load the saved floor
            self.grid = saved.grid;
            game::load_floor(
                &mut self.world,
                &self.grid,
                &saved.entities,
                self.player_entity,
                spawn_pos,
                &self.game_clock,
                &mut self.action_scheduler,
                &mut self.events,
            );
        } else {
            // Generate new floor
            self.grid = Grid::new_floor(DUNGEON_DEFAULT_WIDTH, DUNGEON_DEFAULT_HEIGHT, target_floor);

            // Determine spawn position
            let spawn_pos = match direction {
                StairDirection::Down => {
                    // Coming from above, spawn at stairs up
                    self.grid.stairs_up_pos.unwrap_or((1, 1))
                }
                StairDirection::Up => {
                    // Coming from below, spawn at stairs down
                    self.grid.stairs_down_pos.unwrap_or((1, 1))
                }
            };

            game::spawn_floor_entities(
                &mut self.world,
                &self.grid,
                self.player_entity,
                spawn_pos,
                &self.game_clock,
                &mut self.action_scheduler,
                &mut self.events,
            );
        }

        self.current_floor = target_floor;

        // Clear click-to-move path
        self.input.clear_path();

        // Update camera to player's new position
        if let Ok(vis_pos) = self.world.get::<&components::VisualPosition>(self.player_entity) {
            self.camera.set_tracking_target(glam::Vec2::new(vis_pos.x, vis_pos.y));
        }
    }
}
