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
    pub skeleton_spawns: Vec<(i32, i32)>,
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
    pub skeleton_spawns: Vec<(i32, i32)>,
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
            skeleton_spawns: Vec::new(),
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
                skeleton_spawns: Vec::new(),
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
            skeleton_spawns: Vec::new(),
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
        skeleton_spawns: event_result.skeleton_spawns,
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
            skeleton_spawns: Vec::new(),
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
            skeleton_spawns: Vec::new(),
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
        skeleton_spawns: event_result.skeleton_spawns,
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
pub fn advance_until_player_ready(
    world: &mut World,
    grid: &Grid,
    player_entity: Entity,
    clock: &mut GameClock,
    scheduler: &mut ActionScheduler,
    events: &mut EventQueue,
    rng: &mut impl Rng,
) {
    puffin::profile_function!();

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
        time_system::tick_burn_damage(world, clock.time, events);
        time_system::tick_status_effects(world, elapsed);
        time_system::tick_ability_cooldowns(world, elapsed);
        time_system::tick_ranged_cooldowns(world, elapsed);

        time_system::complete_action(world, grid, next_entity, events, clock.time);

        if next_entity != player_entity {
            systems::ai::decide_action(world, grid, next_entity, player_entity, clock, scheduler, events, rng);
        }
    }
}

/// Advance game time until the player has at least the required energy.
/// This allows enemies to act while the player "waits" for energy.
/// Returns true if player now has enough energy, false if player died or error.
pub fn wait_for_energy(
    world: &mut World,
    grid: &Grid,
    player_entity: Entity,
    required_energy: i32,
    clock: &mut GameClock,
    scheduler: &mut ActionScheduler,
    events: &mut EventQueue,
    rng: &mut impl Rng,
) -> bool {
    loop {
        // Check if player has enough energy
        let (current_energy, max_energy, regen_interval, last_regen_time) = {
            let Ok(actor) = world.get::<&Actor>(player_entity) else {
                return false; // Player doesn't exist
            };
            (actor.energy, actor.max_energy, actor.energy_regen_interval, actor.last_energy_regen_time)
        };

        // Can never afford this action
        if max_energy < required_energy {
            return false;
        }

        // Already have enough
        if current_energy >= required_energy {
            return true;
        }

        // Calculate when we'll have enough energy
        let energy_needed = required_energy - current_energy;
        let time_to_wait = energy_needed as f32 * regen_interval;
        let target_time = (last_regen_time + regen_interval).max(clock.time) + (energy_needed - 1) as f32 * regen_interval;

        // Schedule player to "wake up" at that time so the scheduler has something to process
        scheduler.schedule(player_entity, target_time);

        // Process any pending actions until we reach target time or player has energy
        while clock.time < target_time {
            let Some((next_entity, completion_time)) = scheduler.pop_next() else {
                // Nothing scheduled, just advance time
                clock.advance_to(target_time);
                break;
            };

            // If this is the player's wakeup, we might be done
            if next_entity == player_entity && completion_time >= target_time {
                clock.advance_to(completion_time);
                let elapsed = completion_time - clock.time + time_to_wait;
                time_system::tick_energy_regen(world, clock.time, Some(events));
                time_system::tick_ability_cooldowns(world, elapsed);
                time_system::tick_ranged_cooldowns(world, elapsed);
                break;
            }

            let previous_time = clock.time;
            clock.advance_to(completion_time);
            let elapsed = clock.time - previous_time;

            update_projectiles_at_time(world, grid, clock.time, events);
            time_system::tick_health_regen(world, clock.time, Some(events));
            time_system::tick_energy_regen(world, clock.time, Some(events));
            time_system::tick_burn_damage(world, clock.time, events);
            time_system::tick_status_effects(world, elapsed);
            time_system::tick_ability_cooldowns(world, elapsed);
            time_system::tick_ranged_cooldowns(world, elapsed);

            // Complete the action
            time_system::complete_action(world, grid, next_entity, events, clock.time);

            // Let AI decide next action
            if next_entity != player_entity {
                systems::ai::decide_action(world, grid, next_entity, player_entity, clock, scheduler, events, rng);
            }

            // Check if player died
            if world.get::<&Actor>(player_entity).is_err() {
                return false;
            }
        }

        // Final regen tick to ensure energy is updated
        time_system::tick_energy_regen(world, clock.time, Some(events));
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
        skeleton_spawns: Vec::new(),
    };

    for event in events.drain() {
        vfx.handle_event(&event, grid);
        ui_state.handle_event(&event);

        match &event {
            GameEvent::DoorOpened { door, .. } => {
                systems::handle_door_opened(world, *door);
            }
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
            GameEvent::CoffinSkeletonSpawn { position } => {
                result.skeleton_spawns.push(*position);
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
    pub close_shop: bool,
}

impl Default for UiActionResult {
    fn default() -> Self {
        Self {
            enter_targeting: None,
            close_inventory: false,
            close_context_menu: false,
            close_chest: false,
            close_dialogue: false,
            close_shop: false,
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

    // Chest interactions (also works for ground item piles)
    if let Some(chest_id) = ui_state.open_chest {
        if actions.chest_take_all || actions.close_chest {
            if actions.chest_take_all {
                systems::take_all_from_container(world, player_entity, chest_id, Some(events));
                // Clean up empty ground item piles
                systems::cleanup_empty_ground_piles(world);
            }
            result.close_chest = true;
        } else if actions.chest_take_gold {
            systems::take_gold_from_container(world, player_entity, chest_id, Some(events));
        } else if let Some(item_index) = actions.chest_item_to_take {
            systems::take_item_from_container(world, player_entity, chest_id, item_index, Some(events));
            // Clean up empty ground item piles after taking items
            systems::cleanup_empty_ground_piles(world);
        }
    }

    // Dialogue interactions
    if let Some(npc_id) = ui_state.talking_to {
        if let Some(option_index) = actions.dialogue_option_selected {
            // Check if the selected option has a special action before advancing
            let action = get_dialogue_action(world, npc_id, option_index);

            // Advance the dialogue
            if crate::game::advance_dialogue(world, npc_id, option_index) {
                result.close_dialogue = true;
            }

            // Handle special actions after dialogue closes/advances
            if let Some(crate::components::DialogueAction::OpenShop) = action {
                // Open shop if NPC is a vendor
                if world.get::<&crate::components::Vendor>(npc_id).is_ok() {
                    events.push(crate::events::GameEvent::ShopOpened {
                        vendor: npc_id,
                        player: player_entity,
                    });
                    result.close_dialogue = true;
                }
            }
        }
    }

    // Shop interactions
    if let Some(vendor_id) = ui_state.shopping_at {
        if let Some(item_idx) = actions.buy_item {
            buy_item_from_vendor(world, player_entity, vendor_id, item_idx, events);
        }
        if let Some(item_idx) = actions.sell_item {
            sell_item_to_vendor(world, player_entity, vendor_id, item_idx, events);
        }
        if actions.close_shop {
            result.close_shop = true;
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

    // Drop item from inventory
    if let Some(item_index) = actions.item_to_drop {
        systems::actions::apply_drop_item(world, player_entity, item_index, events);
        result.close_context_menu = true;
    }

    // Unequip weapon (put back in inventory)
    if actions.unequip_weapon {
        systems::actions::apply_unequip_weapon(world, player_entity);
    }

    // Drop equipped weapon
    if actions.drop_equipped_weapon {
        systems::actions::apply_drop_equipped_weapon(world, player_entity, events);
    }

    result
}

// =============================================================================
// SHOP HELPER FUNCTIONS
// =============================================================================

/// Get the dialogue action for the selected option at the current dialogue node.
fn get_dialogue_action(
    world: &World,
    npc_id: hecs::Entity,
    option_index: usize,
) -> Option<crate::components::DialogueAction> {
    let dialogue = world.get::<&crate::components::Dialogue>(npc_id).ok()?;
    let node = dialogue.nodes.get(dialogue.current_node)?;
    let option = node.options.get(option_index)?;
    Some(option.action)
}

/// Buy an item from a vendor.
fn buy_item_from_vendor(
    world: &mut World,
    player_entity: hecs::Entity,
    vendor_id: hecs::Entity,
    item_idx: usize,
    events: &mut crate::events::EventQueue,
) {
    use crate::components::{Inventory, Vendor};
    use crate::events::GameEvent;
    use crate::systems::item_defs::get_price;
    use crate::systems::items::item_weight;

    // Get item info from vendor
    let (item_type, price) = {
        let Ok(vendor) = world.get::<&Vendor>(vendor_id) else { return };
        let Some((item, stock)) = vendor.inventory.get(item_idx) else { return };
        if *stock == 0 { return; }
        (*item, get_price(*item))
    };

    // Check player can afford it
    {
        let Ok(player_inv) = world.get::<&Inventory>(player_entity) else { return };
        if player_inv.gold < price { return; }
    }

    // Transfer gold from player to vendor
    if let Ok(mut player_inv) = world.get::<&mut Inventory>(player_entity) {
        player_inv.gold -= price;
        player_inv.items.push(item_type);
        player_inv.current_weight_kg += item_weight(item_type);
    }

    if let Ok(mut vendor) = world.get::<&mut Vendor>(vendor_id) {
        vendor.gold += price;
        // Decrease stock
        if let Some((_, stock)) = vendor.inventory.get_mut(item_idx) {
            *stock = stock.saturating_sub(1);
        }
    }

    events.push(GameEvent::ItemPurchased {
        vendor: vendor_id,
        item: item_type,
        price,
    });
}

/// Sell an item to a vendor.
fn sell_item_to_vendor(
    world: &mut World,
    player_entity: hecs::Entity,
    vendor_id: hecs::Entity,
    item_idx: usize,
    events: &mut crate::events::EventQueue,
) {
    use crate::components::{Inventory, Vendor};
    use crate::events::GameEvent;
    use crate::systems::item_defs::get_sell_price;
    use crate::systems::items::item_weight;

    // Get item info from player
    let (item_type, sell_price) = {
        let Ok(player_inv) = world.get::<&Inventory>(player_entity) else { return };
        let Some(&item) = player_inv.items.get(item_idx) else { return };
        (item, get_sell_price(item))
    };

    // Check vendor can afford it
    {
        let Ok(vendor) = world.get::<&Vendor>(vendor_id) else { return };
        if vendor.gold < sell_price { return; }
    }

    // Remove item from player, add gold
    if let Ok(mut player_inv) = world.get::<&mut Inventory>(player_entity) {
        player_inv.items.remove(item_idx);
        player_inv.current_weight_kg -= item_weight(item_type);
        player_inv.gold += sell_price;
    }

    // Transfer gold from vendor
    if let Ok(mut vendor) = world.get::<&mut Vendor>(vendor_id) {
        vendor.gold -= sell_price;
    }

    events.push(GameEvent::ItemSold {
        vendor: vendor_id,
        item: item_type,
        value: sell_price,
    });
}
