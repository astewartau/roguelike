//! Game simulation - turn execution, time advancement, and event processing.

use crate::components::{ActionType, Actor, EffectType, Inventory};
use crate::constants;
use crate::events::{EventQueue, GameEvent, StairDirection};
use crate::grid::Grid;
use crate::input::TargetingMode;
use crate::queries;
use crate::systems;
use crate::systems::action_dispatch;
use crate::systems::player_input::{self, PlayerIntent};
use crate::time_system::{self, ActionScheduler, GameClock};
use crate::ui::{DevMenu, GameUiState, UiActions};
use crate::vfx::VfxManager;

use hecs::{Entity, World};
use rand::Rng;

/// Result of attempting to start a player action.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TurnResult {
    Started,
    Blocked,
    NotReady,
}

/// Full result of executing a player turn.
pub struct TurnExecutionResult {
    pub turn_result: TurnResult,
    pub floor_transition: Option<StairDirection>,
    pub player_attacked: bool,
    pub player_took_damage: bool,
    pub enemy_spotted_player: bool,
}

impl TurnExecutionResult {
    pub fn should_interrupt_path(&self) -> bool {
        self.player_attacked || self.player_took_damage || self.enemy_spotted_player
    }
}

/// Result of processing events.
pub struct EventProcessingResult {
    pub floor_transition: Option<StairDirection>,
    pub player_attacked: bool,
    pub player_took_damage: bool,
    pub enemy_spotted_player: bool,
}

impl EventProcessingResult {
    pub fn should_interrupt_path(&self) -> bool {
        self.player_attacked || self.player_took_damage || self.enemy_spotted_player
    }
}

/// Execute a player intent - unified entry point for all player actions.
pub fn execute_player_intent(
    world: &mut World,
    grid: &Grid,
    player_entity: Entity,
    intent: PlayerIntent,
    clock: &mut GameClock,
    scheduler: &mut ActionScheduler,
    events: &mut EventQueue,
    vfx: &mut VfxManager,
    ui_state: &mut GameUiState,
) -> TurnExecutionResult {
    let can_act = world
        .get::<&Actor>(player_entity)
        .map(|a| a.can_act())
        .unwrap_or(false);

    if !can_act {
        return TurnExecutionResult {
            turn_result: TurnResult::NotReady,
            floor_transition: None,
            player_attacked: false,
            player_took_damage: false,
            enemy_spotted_player: false,
        };
    }

    let action_type = match player_input::intent_to_action(world, grid, player_entity, &intent) {
        Some(action) => action,
        None => {
            return TurnExecutionResult {
                turn_result: TurnResult::Blocked,
                floor_transition: None,
                player_attacked: false,
                player_took_damage: false,
                enemy_spotted_player: false,
            };
        }
    };

    if time_system::start_action(world, player_entity, action_type, clock, scheduler).is_err() {
        return TurnExecutionResult {
            turn_result: TurnResult::Blocked,
            floor_transition: None,
            player_attacked: false,
            player_took_damage: false,
            enemy_spotted_player: false,
        };
    }

    let mut rng = rand::thread_rng();
    advance_until_player_ready(world, grid, player_entity, clock, scheduler, events, &mut rng);

    let event_result = process_events(events, world, grid, vfx, ui_state, player_entity);

    TurnExecutionResult {
        turn_result: TurnResult::Started,
        floor_transition: event_result.floor_transition,
        player_attacked: event_result.player_attacked,
        player_took_damage: event_result.player_took_damage,
        enemy_spotted_player: event_result.enemy_spotted_player,
    }
}

/// Execute a player turn based on movement input.
#[allow(dead_code)] // Public API for alternative game loop implementations
pub fn execute_player_turn(
    world: &mut World,
    grid: &Grid,
    player_entity: Entity,
    dx: i32,
    dy: i32,
    clock: &mut GameClock,
    scheduler: &mut ActionScheduler,
    events: &mut EventQueue,
    vfx: &mut VfxManager,
    ui_state: &mut GameUiState,
) -> TurnExecutionResult {
    let can_act = world
        .get::<&Actor>(player_entity)
        .map(|a| a.can_act())
        .unwrap_or(false);

    if !can_act {
        return TurnExecutionResult {
            turn_result: TurnResult::NotReady,
            floor_transition: None,
            player_attacked: false,
            player_took_damage: false,
            enemy_spotted_player: false,
        };
    }

    let action_type = action_dispatch::determine_action_type(world, grid, player_entity, dx, dy);

    if time_system::start_action(world, player_entity, action_type, clock, scheduler).is_err() {
        return TurnExecutionResult {
            turn_result: TurnResult::Blocked,
            floor_transition: None,
            player_attacked: false,
            player_took_damage: false,
            enemy_spotted_player: false,
        };
    }

    let mut rng = rand::thread_rng();
    advance_until_player_ready(world, grid, player_entity, clock, scheduler, events, &mut rng);

    let event_result = process_events(events, world, grid, vfx, ui_state, player_entity);

    TurnExecutionResult {
        turn_result: TurnResult::Started,
        floor_transition: event_result.floor_transition,
        player_attacked: event_result.player_attacked,
        player_took_damage: event_result.player_took_damage,
        enemy_spotted_player: event_result.enemy_spotted_player,
    }
}

/// Get the action type that would result from movement input.
#[allow(dead_code)] // Public API for action preview
pub fn peek_action_type(
    world: &World,
    grid: &Grid,
    player_entity: Entity,
    dx: i32,
    dy: i32,
) -> ActionType {
    action_dispatch::determine_action_type(world, grid, player_entity, dx, dy)
}

/// Advance game time until the player can act again.
fn advance_until_player_ready(
    world: &mut World,
    grid: &Grid,
    player_entity: Entity,
    clock: &mut GameClock,
    scheduler: &mut ActionScheduler,
    events: &mut EventQueue,
    rng: &mut impl Rng,
) {
    loop {
        let player_can_act = world
            .get::<&Actor>(player_entity)
            .map(|a| a.can_act())
            .unwrap_or(false);

        if player_can_act {
            update_projectiles_at_time(world, grid, clock.time, events);
            return;
        }

        if world.get::<&Actor>(player_entity).is_err() {
            return;
        }

        let Some((next_entity, completion_time)) = scheduler.pop_next() else {
            return;
        };

        let previous_time = clock.time;
        clock.advance_to(completion_time);
        let elapsed = clock.time - previous_time;

        update_projectiles_at_time(world, grid, clock.time, events);

        time_system::tick_health_regen(world, clock.time, Some(events));
        time_system::tick_energy_regen(world, clock.time, Some(events));
        time_system::tick_status_effects(world, elapsed);

        time_system::complete_action(world, grid, next_entity, events, clock.time);

        if next_entity != player_entity {
            systems::ai::decide_action(world, grid, next_entity, player_entity, clock, scheduler, events, rng);
        }
    }
}

/// Update projectiles at current time.
fn update_projectiles_at_time(
    world: &mut World,
    grid: &Grid,
    current_time: f32,
    events: &mut EventQueue,
) {
    systems::update_projectiles(world, grid, current_time, events);
}

/// Process all pending events.
pub fn process_events(
    events: &mut EventQueue,
    world: &mut World,
    grid: &Grid,
    vfx: &mut VfxManager,
    ui_state: &mut GameUiState,
    player_entity: Entity,
) -> EventProcessingResult {
    let mut result = EventProcessingResult {
        floor_transition: None,
        player_attacked: false,
        player_took_damage: false,
        enemy_spotted_player: false,
    };

    for event in events.drain() {
        vfx.handle_event(&event, grid);
        ui_state.handle_event(&event);

        match &event {
            GameEvent::ContainerOpened { container, .. } => {
                systems::handle_container_opened(world, *container);
            }
            GameEvent::FloorTransition { direction, .. } => {
                result.floor_transition = Some(*direction);
            }
            GameEvent::AttackHit { attacker, target, .. } => {
                if *attacker == player_entity {
                    result.player_attacked = true;
                }
                if *target == player_entity {
                    result.player_took_damage = true;
                }
            }
            GameEvent::AIStateChanged { entity, new_state } => {
                if *new_state == crate::components::AIState::Chasing {
                    if let Ok(pos) = world.get::<&crate::components::Position>(*entity) {
                        vfx.spawn_alert(pos.x as f32 + 0.5, pos.y as f32 + 0.5);
                    }
                    result.enemy_spotted_player = true;
                }
            }
            _ => {}
        }
    }

    result
}

/// Result of processing UI actions.
pub struct UiActionResult {
    pub enter_targeting: Option<TargetingMode>,
    pub close_inventory: bool,
    pub close_context_menu: bool,
    pub close_chest: bool,
    pub close_dialogue: bool,
}

impl Default for UiActionResult {
    fn default() -> Self {
        Self {
            enter_targeting: None,
            close_inventory: false,
            close_context_menu: false,
            close_chest: false,
            close_dialogue: false,
        }
    }
}

/// Process UI actions and execute game logic.
pub fn process_ui_actions(
    world: &mut World,
    grid: &mut Grid,
    player_entity: Entity,
    actions: &UiActions,
    dev_menu: &mut DevMenu,
    ui_state: &GameUiState,
    events: &mut EventQueue,
    game_time: f32,
) -> UiActionResult {
    let mut result = UiActionResult::default();

    // Dev menu item giving
    if let Some(item) = dev_menu.take_item_to_give() {
        systems::dev_tools::give_item_to_player(world, player_entity, item);
    }

    // Chest interactions
    if let Some(chest_id) = ui_state.open_chest {
        if actions.chest_take_all || actions.close_chest {
            if actions.chest_take_all {
                systems::take_all_from_container(world, player_entity, chest_id, Some(events));
            }
            result.close_chest = true;
        } else if actions.chest_take_gold {
            systems::take_gold_from_container(world, player_entity, chest_id, Some(events));
        } else if let Some(item_index) = actions.chest_item_to_take {
            systems::take_item_from_container(world, player_entity, chest_id, item_index, Some(events));
        }
    }

    // Dialogue interactions
    if let Some(npc_id) = ui_state.talking_to {
        if let Some(option_index) = actions.dialogue_option_selected {
            if crate::game::advance_dialogue(world, npc_id, option_index) {
                result.close_dialogue = true;
            }
        }
    }

    // Item use
    if let Some(item_index) = actions.item_to_use {
        let use_result = systems::use_item(world, player_entity, item_index);

        match use_result {
            systems::ItemUseResult::RequiresTarget { item_type, item_index } => {
                let params = systems::item_targeting_params(item_type);
                result.enter_targeting = Some(TargetingMode {
                    item_type,
                    item_index,
                    max_range: params.max_range,
                    radius: params.radius,
                });
                result.close_inventory = true;
                result.close_context_menu = true;
            }
            systems::ItemUseResult::RevealEnemies => {
                systems::reveal_enemies(world, grid, game_time);
                systems::remove_item_from_inventory(world, player_entity, item_index);
            }
            systems::ItemUseResult::RevealMap => {
                systems::reveal_entire_map(grid);
                systems::remove_item_from_inventory(world, player_entity, item_index);
            }
            systems::ItemUseResult::ApplyFearToVisible => {
                let player_pos = queries::get_entity_position(world, player_entity).unwrap_or((0, 0));
                systems::effects::apply_effect_to_visible_enemies(
                    world, grid, player_pos,
                    constants::FOV_RADIUS, EffectType::Feared, constants::FEAR_DURATION,
                );
                systems::remove_item_from_inventory(world, player_entity, item_index);
            }
            systems::ItemUseResult::ApplySlowToVisible => {
                let player_pos = queries::get_entity_position(world, player_entity).unwrap_or((0, 0));
                systems::effects::apply_effect_to_visible_enemies(
                    world, grid, player_pos,
                    constants::FOV_RADIUS, EffectType::Slowed, constants::SLOW_DURATION,
                );
                systems::remove_item_from_inventory(world, player_entity, item_index);
            }
            systems::ItemUseResult::IsWeapon { item_index, .. } => {
                systems::actions::apply_equip_weapon(world, player_entity, item_index);
            }
            _ => {}
        }
    }

    // Throw item
    if let Some(item_index) = actions.item_to_throw {
        if let Ok(inv) = world.get::<&Inventory>(player_entity) {
            if let Some(&item_type) = inv.items.get(item_index) {
                if systems::items::item_is_throwable(item_type) {
                    let params = systems::item_targeting_params(item_type);
                    result.enter_targeting = Some(TargetingMode {
                        item_type,
                        item_index,
                        max_range: params.max_range,
                        radius: params.radius,
                    });
                    result.close_inventory = true;
                    result.close_context_menu = true;
                }
            }
        }
    }

    result
}
