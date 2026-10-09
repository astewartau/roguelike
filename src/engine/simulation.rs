//! Game simulation - turn execution, time advancement, and event processing.

use crate::components::{
    ActionType, Actor, AIState, ChaseAI, EffectType, Health, Inventory, ItemType, TamedBy,
};
use crate::constants;
use crate::events::{EventQueue, GameEvent, StairDirection};
use crate::grid::Grid;
use crate::input::TargetingMode;
use crate::queries;
use crate::systems;
use crate::systems::action_dispatch;
use crate::systems::player_input::{self, PlayerIntent};
use crate::time_system::{self};
use crate::ui::{DevMenu, UiActions};

use super::{ActorCtx, SimCtx};
use hecs::{Entity, World};
use rand::Rng;

/// Result of attempting to start a player action.
///
/// `Started` is the `Default` so that outcomes produced by pure event
/// processing (where no turn was attempted) carry a neutral value; those
/// callers never read the field.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TurnResult {
    #[default]
    Started,
    Blocked,
    NotReady,
}

/// Everything the engine needs to know after simulating a slice of game time.
///
/// Produced both by executing a player turn (`turn_result` says whether the
/// action actually started) and by draining the event queue, which only fills
/// in the event-derived fields.
#[derive(Default)]
pub struct TurnExecutionResult {
    pub turn_result: TurnResult,
    pub floor_transition: Option<StairDirection>,
    pub player_attacked: bool,
    pub player_took_damage: bool,
    pub enemy_spotted_player: bool,
    pub skeleton_spawns: Vec<(i32, i32)>,
    /// Positions where Raise Dead completed (spawn friendly skeletons here)
    pub raised_skeletons: Vec<(i32, i32)>,
    /// Boss minion summons: (boss, position) pairs to spawn hostile spiders at
    pub boss_minion_spawns: Vec<(Entity, (i32, i32))>,
}

impl TurnExecutionResult {
    pub fn should_interrupt_path(&self) -> bool {
        self.player_attacked || self.player_took_damage || self.enemy_spotted_player
    }
}

/// Execute a player intent - unified entry point for all player actions.
pub fn execute_player_intent(ctx: &mut SimCtx, intent: PlayerIntent) -> TurnExecutionResult {
    let player_entity = ctx.player;

    let can_act = ctx
        .world
        .get::<&Actor>(player_entity)
        .map(|a| a.can_act())
        .unwrap_or(false);

    if !can_act {
        return TurnExecutionResult {
            turn_result: TurnResult::NotReady,
            ..Default::default()
        };
    }

    let action_type =
        match player_input::intent_to_action(ctx.world, ctx.grid, player_entity, &intent) {
            Some(action) => action,
            None => {
                return TurnExecutionResult {
                    turn_result: TurnResult::Blocked,
                    ..Default::default()
                };
            }
        };

    if time_system::start_action(
        ctx.world,
        player_entity,
        action_type,
        ctx.clock,
        ctx.scheduler,
    )
    .is_err()
    {
        return TurnExecutionResult {
            turn_result: TurnResult::Blocked,
            ..Default::default()
        };
    }

    // Start cooldown for Ranger abilities
    if let Ok(mut ra) = ctx
        .world
        .get::<&mut crate::components::RangerAbilities>(player_entity)
    {
        let ability_index = match &action_type {
            ActionType::Tumble { .. } => Some(1), // Index 1 = Tumble
            ActionType::PlaceSnareTrap { .. } => Some(2), // Index 2 = SnareTrap
            ActionType::ShootCripplingShot { .. } => Some(3), // Index 3 = CripplingShot
            _ => None,
        };
        if let Some(index) = ability_index {
            ra.start_cooldown(index);
        }
    }

    advance_until_player_ready(&mut ctx.actors());

    let event_result = process_events(ctx);

    TurnExecutionResult {
        turn_result: TurnResult::Started,
        ..event_result
    }
}

/// Whether the player is currently at (or above) full health.
pub fn player_at_full_health(world: &World, player: Entity) -> bool {
    world
        .get::<&Health>(player)
        .map(|h| h.current >= h.max)
        .unwrap_or(true)
}

/// Whether any hostile enemy is currently aware of a target (chasing or
/// investigating). Tamed companions are excluded — they use `CompanionAI`, but
/// we guard with `TamedBy` as well so a friendly never blocks resting.
pub fn any_enemy_alerted(world: &World, _player: Entity) -> bool {
    for (id, ai) in world.query::<&ChaseAI>().iter() {
        if world.get::<&TamedBy>(id).is_ok() {
            continue;
        }
        if matches!(ai.state, AIState::Chasing | AIState::Investigating) {
            return true;
        }
    }
    false
}

/// Execute a player turn based on movement input.
#[allow(dead_code)] // Public API for alternative game loop implementations
pub fn execute_player_turn(ctx: &mut SimCtx, dx: i32, dy: i32) -> TurnExecutionResult {
    let player_entity = ctx.player;

    let can_act = ctx
        .world
        .get::<&Actor>(player_entity)
        .map(|a| a.can_act())
        .unwrap_or(false);

    if !can_act {
        return TurnExecutionResult {
            turn_result: TurnResult::NotReady,
            ..Default::default()
        };
    }

    let action_type =
        action_dispatch::determine_action_type(ctx.world, ctx.grid, player_entity, dx, dy);

    if time_system::start_action(ctx.world, player_entity, action_type, ctx.clock, ctx.scheduler)
        .is_err()
    {
        return TurnExecutionResult {
            turn_result: TurnResult::Blocked,
            ..Default::default()
        };
    }

    advance_until_player_ready(&mut ctx.actors());

    let event_result = process_events(ctx);

    TurnExecutionResult {
        turn_result: TurnResult::Started,
        ..event_result
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
pub fn advance_until_player_ready(ctx: &mut ActorCtx) {
    profile_function!();

    let player_entity = ctx.player;

    loop {
        let player_can_act = ctx
            .world
            .get::<&Actor>(player_entity)
            .map(|a| a.can_act())
            .unwrap_or(false);

        if player_can_act {
            update_projectiles_at_time(ctx, ctx.clock.time);
            return;
        }

        if ctx.world.get::<&Actor>(player_entity).is_err() {
            return;
        }

        let Some((next_entity, completion_time)) = ctx.scheduler.pop_next() else {
            // Safety check: if scheduler is empty but player can't act, recover
            if let Ok(mut actor) = ctx.world.get::<&mut Actor>(player_entity) {
                // Case 1: Player has a stuck current_action
                if actor.current_action.is_some() {
                    eprintln!("[WARNING] Scheduler empty but player has current_action - clearing to prevent soft-lock");
                    actor.current_action = None;
                    continue; // Re-check if player can act now
                }
            }
            return;
        };

        let previous_time = ctx.clock.time;
        ctx.clock.advance_to(completion_time);
        let now = ctx.clock.time;
        let elapsed = now - previous_time;

        update_projectiles_at_time(ctx, now);

        time_system::tick_health_regen(ctx.world, now, Some(ctx.events));
        time_system::tick_burn_damage(ctx.world, now, ctx.events);
        time_system::tick_status_effects(ctx.world, elapsed);
        time_system::tick_ability_cooldowns(ctx.world, elapsed);
        time_system::tick_ranged_cooldowns(ctx.world, elapsed);
        systems::ai::tick_threat_decay(ctx.world, ctx.grid, ctx.spatial, elapsed);
        systems::ai::tick_alarms(ctx.world, elapsed);
        systems::ai::tick_role_cooldowns(ctx.world, elapsed);

        time_system::complete_action(ctx, next_entity);

        // After player completes an action, check for dormant entities that should wake up
        if next_entity == player_entity {
            if let Some(player_pos) = queries::get_entity_position(ctx.world, player_entity) {
                let newly_active = ctx.tracker.update_on_player_move(ctx.world, player_pos);
                // Schedule newly awakened entities
                let wake_at = ctx.clock.time + 0.1;
                for ai_entity in newly_active {
                    ctx.scheduler.schedule(ai_entity, wake_at);
                }
            }
        } else {
            // Non-player entity: let AI decide next action
            systems::ai::decide_action(ctx, next_entity);
        }
    }
}

/// Update projectiles at current time.
fn update_projectiles_at_time(ctx: &mut ActorCtx, current_time: f32) {
    systems::update_projectiles(&mut ctx.effects(), current_time);
}

/// Display name of an entity for damage attribution ("Goblin", ...).
fn entity_display_name(world: &World, entity: Entity) -> String {
    world
        .get::<&crate::components::Name>(entity)
        .map(|n| n.0.clone())
        .unwrap_or_else(|_| "an enemy".to_string())
}

/// Remember the most recent source of player damage (best-effort cause of
/// death for the run-history record).
fn record_player_damage_source(world: &mut World, player_entity: Entity, source: String) {
    if let Ok(mut last) = world.get::<&mut crate::components::LastDamageSource>(player_entity) {
        last.0 = source;
        return;
    }
    let _ = world.insert_one(player_entity, crate::components::LastDamageSource(source));
}

/// Drain the event queue, playing each event's sound, updating vfx, the message
/// log and UI state, and collecting the follow-up work the engine has to do
/// (floor transitions, deferred spawns, path interruption).
pub fn process_events(ctx: &mut SimCtx) -> TurnExecutionResult {
    let player_entity = ctx.player;
    let mut result = TurnExecutionResult::default();

    // Collect events for audio processing
    let event_list: Vec<_> = ctx.events.drain().collect();

    // Process audio first (with player position for distance-based volume)
    if let Some(audio_manager) = ctx.audio {
        let player_pos = ctx
            .world
            .get::<&crate::components::Position>(player_entity)
            .map(|p| (p.x, p.y))
            .unwrap_or((0, 0));
        audio_manager.process_events(&event_list, player_pos);
    }

    let (world, grid, spatial_cache, vfx, ui_state) = (
        &mut *ctx.world,
        &*ctx.grid,
        &mut *ctx.spatial,
        &mut *ctx.vfx,
        &mut *ctx.ui,
    );

    // Camera shakes asked for by this batch of events, handed to the vfx
    // manager below (the camera itself is not reachable from here).
    let mut shake_requests = Vec::new();

    for event in event_list {
        vfx.handle_event(&event, grid, player_entity);
        ui_state.handle_event(&event);
        ui_state.message_log.record_event(&event, &*world);

        // Presentation reactions that need world access, driven off the same
        // events as everything else above: flash whatever just took damage,
        // and shake the view if the event was worth shaking it for.
        systems::flash_on_damage(world, &event);
        systems::camera_shake::queue_for_event(world, player_entity, &event, &mut shake_requests);

        match &event {
            GameEvent::DoorOpened { door, .. } => {
                systems::handle_door_opened(world, *door);
                // Update spatial cache so pathfinding sees the door as open
                spatial_cache.clear_blocking_flags(*door);
            }
            GameEvent::DoorClosed { door, .. } => {
                systems::handle_door_closed(world, *door);
                // Update spatial cache so pathfinding sees the door as closed
                spatial_cache.set_blocking_flags(*door, true, true);
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
                    let source = entity_display_name(world, *attacker);
                    record_player_damage_source(world, player_entity, source);
                }
            }
            GameEvent::ProjectileHit { source, target: Some(target), damage, .. }
                if *target == player_entity && *damage > 0 =>
            {
                let source = entity_display_name(world, *source);
                record_player_damage_source(world, player_entity, source);
            }
            GameEvent::BurnDamage { entity, .. } if *entity == player_entity => {
                record_player_damage_source(world, player_entity, "burning".to_string());
            }
            GameEvent::StarvationDamage { entity, .. } if *entity == player_entity => {
                record_player_damage_source(world, player_entity, "starvation".to_string());
            }
            GameEvent::FireTrapTriggered { victim, .. } if *victim == player_entity => {
                record_player_damage_source(world, player_entity, "a fire trap".to_string());
            }
            GameEvent::DungeonTrapTriggered { kind, victim, damage, .. }
                if *victim == player_entity && *damage > 0 =>
            {
                let source = match kind {
                    crate::components::DungeonTrapKind::Spike => "a spike trap",
                    crate::components::DungeonTrapKind::Fire => "a fire trap",
                    crate::components::DungeonTrapKind::Snare => "a snare trap",
                    crate::components::DungeonTrapKind::Alarm => "an alarm trap",
                };
                record_player_damage_source(world, player_entity, source.to_string());
            }
            GameEvent::AIStateChanged { entity, new_state }
                if *new_state == crate::components::AIState::Chasing =>
            {
                if let Ok(pos) = world.get::<&crate::components::Position>(*entity) {
                    vfx.spawn_alert(pos.x as f32 + 0.5, pos.y as f32 + 0.5);
                }
                result.enemy_spotted_player = true;
            }
            GameEvent::CoffinSkeletonSpawn { position } => {
                result.skeleton_spawns.push(*position);
            }
            GameEvent::SkeletonRaised { position, .. } => {
                result.raised_skeletons.push(*position);
            }
            GameEvent::BossMinionSpawn { boss, position } => {
                result.boss_minion_spawns.push((*boss, *position));
            }
            _ => {}
        }
    }

    // Park the shakes with the vfx manager; the engine tick drains them into
    // the camera. Going through the manager rather than the return value means
    // the paths that discard a `TurnExecutionResult` (ability casts, for
    // instance) still get their shake.
    for request in shake_requests {
        vfx.request_shake(request);
    }

    result
}

/// Result of processing UI actions.
#[derive(Default)]
pub struct UiActionResult {
    pub enter_targeting: Option<TargetingMode>,
    pub close_inventory: bool,
    pub close_context_menu: bool,
    pub close_chest: bool,
    pub close_dialogue: bool,
    pub close_shop: bool,
    pub close_altar: bool,
    /// A spell was just learned by studying a scroll; the engine auto-assigns
    /// it to the first free hotbar slot so it's immediately usable.
    pub learned_ability: Option<crate::components::AbilityType>,
}

/// Process UI actions and execute game logic. `ctx.rng` is the seeded game rng
/// (altar outcomes draw from it).
pub fn process_ui_actions(
    ctx: &mut SimCtx,
    actions: &UiActions,
    dev_menu: &mut DevMenu,
) -> UiActionResult {
    let player_entity = ctx.player;
    let game_time = ctx.clock.time;
    let (world, grid, spatial_cache, events, rng, ui_state) = (
        &mut *ctx.world,
        &mut *ctx.grid,
        &mut *ctx.spatial,
        &mut *ctx.events,
        &mut *ctx.rng,
        &*ctx.ui,
    );

    let mut result = UiActionResult::default();

    // Dev menu item giving
    if let Some(item) = dev_menu.take_item_to_give() {
        systems::dev_tools::give_item_to_player(world, player_entity, item);
    }

    // Chest interactions (also works for ground item piles)
    // The window covers every container on the tile (see `loot_sources`), so
    // each take names the container it came from.
    if ui_state.open_chest.is_some() {
        let sources = systems::loot_sources(world, ui_state.open_chest, ui_state.loot_tile);
        let mut took_something = false;
        if actions.chest_take_all {
            systems::take_all_from_sources(world, player_entity, &sources, Some(events));
            result.close_chest = true;
            took_something = true;
        } else if actions.close_chest {
            result.close_chest = true;
        } else if let Some(container) = actions.chest_take_gold {
            systems::take_gold_from_container(world, player_entity, container, Some(events));
            took_something = true;
        } else if let Some((container, item_index)) = actions.chest_item_to_take {
            systems::take_item_from_container(world, player_entity, container, item_index, Some(events));
            took_something = true;
        }
        if took_something {
            // Clean up empty ground item piles, and stop a container that was
            // just emptied blocking its tile.
            systems::cleanup_empty_ground_piles(world);
            systems::unblock_emptied_containers(world, spatial_cache);
            // Nothing left anywhere on the tile: the window has done its job.
            let all_empty = systems::loot_sources(world, ui_state.open_chest, ui_state.loot_tile)
                .into_iter()
                .all(|id| world.get::<&crate::components::Container>(id).map(|c| c.is_empty()).unwrap_or(true));
            if all_empty {
                result.close_chest = true;
            }
        }
    }

    // Dialogue interactions
    if let Some(npc_id) = ui_state.talking_to {
        if actions.close_dialogue {
            result.close_dialogue = true;
        } else if let Some(option_index) = actions.dialogue_option_selected {
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
            buy_item_from_vendor(world, player_entity, vendor_id, item_idx, events, rng);
        }
        if let Some(item_idx) = actions.sell_item {
            sell_item_to_vendor(world, player_entity, vendor_id, item_idx, events);
        }
        if actions.close_shop {
            result.close_shop = true;
        }
    }

    // Altar interactions: sacrifice the chosen inventory item.
    if ui_state.open_altar.is_some() {
        if let Some(item_index) = actions.altar_sacrifice {
            crate::systems::furniture::perform_altar_sacrifice(
                world,
                player_entity,
                item_index,
                events,
                rng,
            );
            result.close_altar = true;
        }
        if actions.close_altar {
            result.close_altar = true;
        }
    }

    // Item use
    if let Some(item_index) = actions.item_to_use {
        let use_result = systems::use_item(world, player_entity, item_index);

        match use_result {
            systems::ItemUseResult::RequiresTarget { item_type, item_index } => {
                let params = systems::item_targeting_params(item_type);
                // Blink range is a scroll magnitude: it scales with the
                // reader's effective INT (apply_blink checks the same range).
                let max_range = if item_type == ItemType::ScrollOfBlink {
                    systems::actions::scaled_blink_range(world, player_entity)
                } else {
                    params.max_range
                };
                result.enter_targeting = Some(TargetingMode {
                    item_type,
                    item_index,
                    max_range,
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
                // Scroll magnitude scales with the reader's effective INT.
                let duration = constants::FEAR_DURATION * queries::int_power(world, player_entity);
                systems::effects::apply_effect_to_visible_enemies(
                    world, grid, player_pos,
                    constants::FOV_RADIUS, EffectType::Feared, duration,
                );
                systems::remove_item_from_inventory(world, player_entity, item_index);
            }
            systems::ItemUseResult::ApplySlowToVisible => {
                let player_pos = queries::get_entity_position(world, player_entity).unwrap_or((0, 0));
                // Scroll magnitude scales with the reader's effective INT.
                let duration = constants::SLOW_DURATION * queries::int_power(world, player_entity);
                systems::effects::apply_effect_to_visible_enemies(
                    world, grid, player_pos,
                    constants::FOV_RADIUS, EffectType::Slowed, duration,
                );
                systems::remove_item_from_inventory(world, player_entity, item_index);
            }
            systems::ItemUseResult::IsWeapon { item_type, item_index } => {
                let result =
                    systems::actions::apply_equip_weapon(world, player_entity, item_index);
                if result == systems::actions::ActionResult::Completed {
                    events.push(GameEvent::WeaponEquipped {
                        entity: player_entity,
                        weapon_type: item_type,
                    });
                }
            }
            systems::ItemUseResult::IsArmor { item_type: _, item_index } => {
                systems::actions::apply_equip_armor(world, player_entity, item_index);
            }
            systems::ItemUseResult::Used { item_type } => {
                // Emit PotionDrunk event for potions (drinking a full water
                // flask also douses any Burning; see systems::items::use_item)
                if matches!(
                    item_type,
                    ItemType::HealthPotion
                        | ItemType::RegenerationPotion
                        | ItemType::StrengthPotion
                        | ItemType::ConfusionPotion
                        | ItemType::WaterFlaskFull
                ) {
                    events.push(GameEvent::PotionDrunk {
                        entity: player_entity,
                        potion_type: item_type,
                    });
                }
            }
            systems::ItemUseResult::FillWaterFlask { item_index } => {
                // Fill from an adjacent (or underfoot) water tile.
                if systems::items::fill_water_flask(world, grid, player_entity, item_index) {
                    events.push(GameEvent::FlaskFilled { entity: player_entity });
                } else {
                    events.push(GameEvent::FlaskFillFailed { entity: player_entity });
                }
            }
            _ => {}
        }
    }

    // Study a scroll: with enough effective INT this consumes the scroll and
    // permanently adds its spell to the player's spell list (LearnedAbilities).
    if let Some(item_index) = actions.item_to_study {
        result.learned_ability = study_scroll(world, player_entity, item_index, events);
        result.close_context_menu = true;
    }

    // Throw item
    if let Some(item_index) = actions.item_to_throw {
        if let Ok(inv) = world.get::<&Inventory>(player_entity) {
            if let Some(item) = inv.items.get(item_index) {
                let item_type = item.kind;
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

    // Unequip armor (put back in inventory)
    if let Some(slot) = actions.unequip_armor {
        systems::actions::apply_unequip_armor(world, player_entity, slot);
    }

    // Drop equipped weapon
    if actions.drop_equipped_weapon {
        systems::actions::apply_drop_equipped_weapon(world, player_entity, events);
    }

    // Select active bow ammo (Arrow / FireArrow)
    if let Some(kind) = actions.set_active_ammo {
        if kind.is_ammo() {
            let _ = world.insert_one(player_entity, crate::components::ActiveAmmo { kind });
            result.close_context_menu = true;
        }
    }

    result
}

/// Attempt to study the scroll at `item_index` in the player's inventory.
///
/// Requires the scroll to be learnable (`min_learn_int`) and the player's
/// *effective* INT (gear counts) to meet the threshold. On success the scroll
/// is consumed and the spell is added to `LearnedAbilities`; returns the
/// learned ability so the engine can hotbar it. All outcomes emit an event
/// for the message log.
fn study_scroll(
    world: &mut World,
    player_entity: Entity,
    item_index: usize,
    events: &mut EventQueue,
) -> Option<crate::components::AbilityType> {
    use crate::components::LearnedAbilities;
    use crate::systems::item_defs::{min_learn_int, scroll_learned_ability};

    let item_type = {
        let inv = world.get::<&Inventory>(player_entity).ok()?;
        inv.items.get(item_index)?.kind
    };

    let required_int = min_learn_int(item_type)?;
    let ability = scroll_learned_ability(item_type)?;

    let current_int = queries::effective_stats(world, player_entity).intelligence;
    if current_int < required_int {
        events.push(GameEvent::SpellStudyFailed {
            scroll: item_type,
            required_int,
            current_int,
        });
        return None;
    }

    // Already known? Keep the scroll.
    let already_known = world
        .get::<&LearnedAbilities>(player_entity)
        .map(|la| la.knows(ability))
        .unwrap_or(false);
    if already_known {
        events.push(GameEvent::SpellAlreadyKnown { ability });
        return None;
    }

    // Consume the scroll and commit the spell to memory.
    systems::remove_item_from_inventory(world, player_entity, item_index);
    let had_component = {
        if let Ok(mut learned) = world.get::<&mut LearnedAbilities>(player_entity) {
            learned.learn(ability);
            true
        } else {
            false
        }
    };
    if !had_component {
        let mut learned = LearnedAbilities::default();
        learned.learn(ability);
        let _ = world.insert_one(player_entity, learned);
    }

    events.push(GameEvent::SpellLearned { ability });
    Some(ability)
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
    rng: &mut impl Rng,
) {
    use crate::components::{Inventory, ItemInstance, Vendor};
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

    // Accessories are pure affix carriers — a plain ring would be pointless,
    // so shop stock is rolled on purchase (Magic tier) and sold identified:
    // the merchant vouches for the wares.
    let instance = if crate::systems::item_defs::is_accessory_kind(item_type) {
        let mut inst = crate::systems::item_defs::roll_gear_with_rarity(
            item_type,
            crate::components::Rarity::Magic,
            rng,
        );
        inst.identified = true;
        inst
    } else {
        ItemInstance::plain(item_type)
    };

    // Transfer gold from player to vendor
    if let Ok(mut player_inv) = world.get::<&mut Inventory>(player_entity) {
        player_inv.gold -= price;
        player_inv.items.push(instance);
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
        let Some(item) = player_inv.items.get(item_idx) else { return };
        (item.kind, get_sell_price(item.kind))
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::components::{
        AbilityType, Affix, Equipment, ItemInstance, LearnedAbilities, Rarity, Stats,
    };

    /// Spawn a bare player with one Scroll of Blink and the given base INT.
    fn player_with_blink_scroll(world: &mut World, int: i32) -> Entity {
        let mut inv = Inventory::new();
        inv.items.push(ItemInstance::plain(ItemType::ScrollOfBlink));
        world.spawn((inv, Stats::new(10, int, 10), LearnedAbilities::default()))
    }

    #[test]
    fn test_study_fails_below_int_threshold() {
        let mut world = World::new();
        let player = player_with_blink_scroll(&mut world, 13); // Blink needs 14
        let mut events = EventQueue::new();

        let learned = study_scroll(&mut world, player, 0, &mut events);

        assert!(learned.is_none());
        // The scroll is kept and nothing was learned.
        assert_eq!(world.get::<&Inventory>(player).unwrap().items.len(), 1);
        assert!(!world
            .get::<&LearnedAbilities>(player)
            .unwrap()
            .knows(AbilityType::LearnedBlink));
        // Feedback event carries the requirement.
        assert!(events.drain().any(|e| matches!(
            e,
            GameEvent::SpellStudyFailed { required_int: 14, current_int: 13, .. }
        )));
    }

    #[test]
    fn test_study_success_consumes_scroll_and_learns_spell() {
        let mut world = World::new();
        let player = player_with_blink_scroll(&mut world, 14);
        let mut events = EventQueue::new();

        let learned = study_scroll(&mut world, player, 0, &mut events);

        assert_eq!(learned, Some(AbilityType::LearnedBlink));
        assert!(world.get::<&Inventory>(player).unwrap().items.is_empty());
        assert!(world
            .get::<&LearnedAbilities>(player)
            .unwrap()
            .knows(AbilityType::LearnedBlink));
        assert!(events
            .drain()
            .any(|e| matches!(e, GameEvent::SpellLearned { .. })));
    }

    #[test]
    fn test_study_uses_effective_int_from_gear() {
        let mut world = World::new();
        // Base INT 12, +2 from an identified helmet affix = 14 (meets Blink).
        let player = player_with_blink_scroll(&mut world, 12);
        let mut equipment = Equipment::empty();
        equipment.head = Some(ItemInstance {
            kind: ItemType::Helmet,
            rarity: Rarity::Magic,
            affixes: vec![Affix::Intelligence(2)],
            name: None,
            identified: true,
            identify_progress: 0.0,
        });
        world.insert_one(player, equipment).expect("insert equipment");

        let mut events = EventQueue::new();
        let learned = study_scroll(&mut world, player, 0, &mut events);
        assert_eq!(learned, Some(AbilityType::LearnedBlink));
    }

    #[test]
    fn test_study_known_spell_keeps_scroll() {
        let mut world = World::new();
        let player = player_with_blink_scroll(&mut world, 18);
        world
            .get::<&mut LearnedAbilities>(player)
            .expect("learned abilities")
            .learn(AbilityType::LearnedBlink);

        let mut events = EventQueue::new();
        let learned = study_scroll(&mut world, player, 0, &mut events);

        assert!(learned.is_none());
        assert_eq!(world.get::<&Inventory>(player).unwrap().items.len(), 1);
        assert!(events
            .drain()
            .any(|e| matches!(e, GameEvent::SpellAlreadyKnown { .. })));
    }
}
