//! Continuous event-driven time system.
//!
//! Manages game time progression through an event-driven loop where time
//! jumps forward to the next action completion rather than ticking.

use crate::components::{
    ActionInProgress, ActionType, Actor, EffectType, Health, RangedCooldown, StatusEffects,
};
use crate::constants::*;
use crate::events::{EventQueue, GameEvent};
use crate::grid::Grid;
use crate::systems::action_dispatch;
use crate::systems::actions::{self, ActionResult};
use crate::systems::effects;
use hecs::{Entity, World};
use std::cmp::Ordering;
use std::collections::BinaryHeap;

// =============================================================================
// GAME CLOCK
// =============================================================================

/// Global game time clock (in seconds)
#[derive(Debug, Clone)]
pub struct GameClock {
    /// Current game time in seconds (simulation time, not real time)
    pub time: f32,
}

impl GameClock {
    pub fn new() -> Self {
        Self { time: 0.0 }
    }

    /// Advance time to the given timestamp
    pub fn advance_to(&mut self, time: f32) {
        debug_assert!(
            time >= self.time,
            "Cannot go backwards in time: {} -> {}",
            self.time,
            time
        );
        self.time = time;
    }
}

impl Default for GameClock {
    fn default() -> Self {
        Self::new()
    }
}

// =============================================================================
// ACTION SCHEDULER
// =============================================================================

/// A scheduled action completion event
#[derive(Debug, Clone, Copy)]
struct ScheduledCompletion {
    entity: Entity,
    completion_time: f32,
}

impl PartialEq for ScheduledCompletion {
    fn eq(&self, other: &Self) -> bool {
        self.completion_time == other.completion_time && self.entity == other.entity
    }
}

impl Eq for ScheduledCompletion {}

impl PartialOrd for ScheduledCompletion {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for ScheduledCompletion {
    fn cmp(&self, other: &Self) -> Ordering {
        // Reverse ordering for min-heap behavior (earliest time first)
        other
            .completion_time
            .partial_cmp(&self.completion_time)
            .unwrap_or(Ordering::Equal)
    }
}

/// Manages the event-driven time loop
#[derive(Debug, Clone)]
pub struct ActionScheduler {
    /// Entities with pending action completions, ordered by completion time (min-heap)
    pending_completions: BinaryHeap<ScheduledCompletion>,
}

impl ActionScheduler {
    pub fn new() -> Self {
        Self {
            pending_completions: BinaryHeap::new(),
        }
    }

    /// Schedule an action completion for an entity
    pub fn schedule(&mut self, entity: Entity, completion_time: f32) {
        self.pending_completions.push(ScheduledCompletion {
            entity,
            completion_time,
        });
    }

    /// Get the next completion (earliest), if any
    #[allow(dead_code)] // Public API for debugging/inspection
    pub fn peek_next(&self) -> Option<(Entity, f32)> {
        self.pending_completions
            .peek()
            .map(|sc| (sc.entity, sc.completion_time))
    }

    /// Pop the next completion (earliest)
    pub fn pop_next(&mut self) -> Option<(Entity, f32)> {
        self.pending_completions
            .pop()
            .map(|sc| (sc.entity, sc.completion_time))
    }

    /// Remove all completions for a specific entity (e.g., on death)
    pub fn cancel_for_entity(&mut self, entity: Entity) {
        // Rebuild the heap without the cancelled entity
        let remaining: Vec<_> = self
            .pending_completions
            .drain()
            .filter(|sc| sc.entity != entity)
            .collect();
        self.pending_completions = remaining.into_iter().collect();
    }

    /// Check if there are any pending completions
    #[allow(dead_code)] // Public API for debugging/inspection
    pub fn is_empty(&self) -> bool {
        self.pending_completions.is_empty()
    }
}

impl Default for ActionScheduler {
    fn default() -> Self {
        Self::new()
    }
}

// =============================================================================
// ACTION STARTING
// =============================================================================

/// Start an action for an entity. Returns Ok(()) if successful.
pub fn start_action(
    world: &mut World,
    entity: Entity,
    action_type: ActionType,
    clock: &GameClock,
    scheduler: &mut ActionScheduler,
) -> Result<(), &'static str> {
    start_action_with_events(world, entity, action_type, clock, scheduler, None)
}

/// Start an action for an entity with optional event emission. Returns Ok(()) if successful.
pub fn start_action_with_events(
    world: &mut World,
    entity: Entity,
    action_type: ActionType,
    clock: &GameClock,
    scheduler: &mut ActionScheduler,
    events: Option<&mut EventQueue>,
) -> Result<(), &'static str> {
    // Check for speed-modifying effects before borrowing Actor
    let has_speed_boost = effects::entity_has_effect(world, entity, EffectType::SpeedBoost);
    let has_slow = effects::entity_has_effect(world, entity, EffectType::Slowed);

    // Get actor component
    let mut actor = world
        .get::<&mut Actor>(entity)
        .map_err(|_| "Entity has no Actor component")?;

    let energy_cost = action_type.energy_cost();

    if actor.current_action.is_some() {
        return Err("Entity is busy with another action");
    }

    if actor.energy < energy_cost {
        return Err("Entity doesn't have enough energy");
    }

    // Spend energy to start (per-action cost)
    actor.energy -= energy_cost;
    let remaining = actor.energy;

    // Calculate effective speed (base speed modified by effects)
    let effective_speed = if has_speed_boost {
        actor.speed * SPEED_BOOST_MULTIPLIER
    } else if has_slow {
        actor.speed * SLOW_MULTIPLIER
    } else {
        actor.speed
    };

    // Calculate completion time
    let duration = action_dispatch::calculate_action_duration(&action_type, effective_speed);
    let completion_time = clock.time + duration;

    // Record action in progress
    actor.current_action = Some(ActionInProgress {
        action_type,
        start_time: clock.time,
        completion_time,
    });

    // Schedule completion
    scheduler.schedule(entity, completion_time);

    // Emit energy spent event
    if let Some(events) = events {
        events.push(GameEvent::EnergySpent {
            entity,
            amount: energy_cost,
            remaining,
        });
    }

    Ok(())
}

// =============================================================================
// ACTION COMPLETION
// =============================================================================

/// Complete an action for an entity, applying its effects
pub fn complete_action(
    world: &mut World,
    grid: &Grid,
    entity: Entity,
    events: &mut EventQueue,
    current_time: f32,
) -> ActionResult {
    // Get the action to complete
    let action = {
        let Ok(actor) = world.get::<&Actor>(entity) else {
            return ActionResult::Invalid;
        };
        match actor.current_action {
            Some(action) => action,
            None => return ActionResult::Invalid,
        }
    };

    // Apply action effects
    let result = apply_action_effects(world, grid, entity, &action.action_type, events, current_time);

    // Clear action (energy regen is now time-based, not action-based)
    if let Ok(mut actor) = world.get::<&mut Actor>(entity) {
        actor.current_action = None;
    }

    result
}

/// Apply the effects of a completed action.
/// Dispatches to the appropriate action implementation in systems::actions.
fn apply_action_effects(
    world: &mut World,
    grid: &Grid,
    entity: Entity,
    action_type: &ActionType,
    events: &mut EventQueue,
    current_time: f32,
) -> ActionResult {
    match action_type {
        ActionType::Move { dx, dy, .. } => actions::apply_move(world, grid, entity, *dx, *dy, events),
        ActionType::Attack { target } => actions::apply_attack(world, entity, *target, events),
        ActionType::AttackDirection { dx, dy } => {
            actions::apply_attack_direction(world, entity, *dx, *dy, events)
        }
        ActionType::OpenDoor { door } => actions::apply_open_door(world, entity, *door, events),
        ActionType::OpenChest { chest } => actions::apply_open_chest(world, entity, *chest, events),
        ActionType::Wait => {
            actions::apply_wait(world, entity, events)
        }
        ActionType::ShootBow { target_x, target_y } => {
            actions::apply_shoot_bow(world, grid, entity, *target_x, *target_y, events, current_time)
        }
        ActionType::UseStairs { x, y, direction } => {
            actions::apply_use_stairs(world, entity, *x, *y, *direction, events)
        }
        ActionType::TalkTo { npc } => actions::apply_talk_to(entity, *npc, events),
        ActionType::ThrowPotion { potion_type, target_x, target_y } => {
            actions::apply_throw_potion(world, grid, entity, *potion_type, *target_x, *target_y, events, current_time)
        }
        ActionType::Blink { target_x, target_y } => {
            actions::apply_blink(world, grid, entity, *target_x, *target_y, events)
        }
        ActionType::CastFireball { target_x, target_y } => {
            actions::apply_fireball(world, entity, *target_x, *target_y, events)
        }
        ActionType::EquipWeapon { item_index } => {
            actions::apply_equip_weapon(world, entity, *item_index)
        }
        ActionType::UnequipWeapon => {
            actions::apply_unequip_weapon(world, entity)
        }
        ActionType::DropItem { item_index } => {
            actions::apply_drop_item(world, entity, *item_index, events)
        }
        ActionType::DropEquippedWeapon => {
            actions::apply_drop_equipped_weapon(world, entity, events)
        }
        ActionType::Cleave => {
            actions::apply_cleave(world, entity, events)
        }
        ActionType::ActivateSprint => {
            actions::apply_activate_sprint(world, entity)
        }
        ActionType::StartTaming { target } => {
            actions::apply_start_taming(world, entity, *target, events)
        }
        ActionType::ActivateBarkskin => {
            actions::apply_activate_barkskin(world, entity, events)
        }
        ActionType::PlaceFireTrap { target_x, target_y } => {
            actions::apply_place_fire_trap(world, entity, *target_x, *target_y, events)
        }
    }
}

// =============================================================================
// TIME-BASED REGENERATION
// =============================================================================

/// Process time-based health regeneration
pub fn tick_health_regen(world: &mut World, current_time: f32, events: Option<&mut EventQueue>) {
    use std::collections::HashSet;

    // First pass: collect entities with Regenerating effect (boosted regen)
    let regenerating: HashSet<Entity> = world
        .query::<(&Health, &StatusEffects)>()
        .iter()
        .filter_map(|(id, (_, status_effects))| {
            if effects::has_effect(status_effects, EffectType::Regenerating) {
                Some(id)
            } else {
                None
            }
        })
        .collect();

    // Collect regen info first to avoid borrow issues
    let mut regen_events: Vec<(Entity, i32)> = Vec::new();

    for (id, health) in world.query_mut::<&mut Health>() {
        // Check if this entity has boosted regen from Regenerating effect
        let has_regen_boost = regenerating.contains(&id);

        // Determine regen parameters (boosted if Regenerating effect active)
        let (regen_amount, regen_interval) = if has_regen_boost {
            (REGENERATION_BOOST_AMOUNT, REGENERATION_BOOST_INTERVAL)
        } else {
            (health.regen_amount, health.regen_interval)
        };

        // Skip if no regen, already full, or dead
        if regen_interval <= 0.0 || health.current >= health.max || health.current <= 0 {
            continue;
        }

        // Calculate how many regen events have occurred
        let time_since_last = current_time - health.last_regen_time;
        if time_since_last >= regen_interval {
            let regen_ticks = (time_since_last / regen_interval) as i32;
            let amount = (regen_amount * regen_ticks).min(health.max - health.current);
            health.current += amount;
            // Update last regen time, accounting for partial intervals
            health.last_regen_time = current_time - (time_since_last % regen_interval);

            if amount > 0 {
                regen_events.push((id, amount));
            }
        }
    }

    // Emit events
    if let Some(events) = events {
        for (entity, amount) in regen_events {
            events.push(GameEvent::HealthRegenerated { entity, amount });
        }
    }
}

/// Process time-based energy regeneration for all actors
pub fn tick_energy_regen(world: &mut World, current_time: f32, events: Option<&mut EventQueue>) {
    use std::collections::HashSet;

    // First pass: collect entities with speed boost (separate query to avoid borrow issues)
    let speed_boosted: HashSet<Entity> = world
        .query::<(&Actor, &StatusEffects)>()
        .iter()
        .filter_map(|(id, (_, status_effects))| {
            if effects::has_effect(status_effects, EffectType::SpeedBoost) {
                Some(id)
            } else {
                None
            }
        })
        .collect();

    // Second pass: process energy regen
    let mut regen_events: Vec<(Entity, i32)> = Vec::new();

    for (id, actor) in world.query_mut::<&mut Actor>() {
        // Skip if no regen interval set, or already at max
        if actor.energy_regen_interval <= 0.0 || actor.energy >= actor.max_energy {
            continue;
        }

        // Apply speed boost multiplier to regen interval (faster regen = shorter interval)
        let effective_regen_interval = if speed_boosted.contains(&id) {
            actor.energy_regen_interval / SPEED_BOOST_MULTIPLIER
        } else {
            actor.energy_regen_interval
        };

        // Calculate how many regen events have occurred
        let time_since_last = current_time - actor.last_energy_regen_time;
        if time_since_last >= effective_regen_interval {
            let regen_ticks = (time_since_last / effective_regen_interval) as i32;
            let old_energy = actor.energy;
            actor.energy = (actor.energy + regen_ticks).min(actor.max_energy);
            let amount = actor.energy - old_energy;
            // Update last regen time, accounting for partial intervals
            actor.last_energy_regen_time =
                current_time - (time_since_last % effective_regen_interval);

            if amount > 0 {
                regen_events.push((id, amount));
            }
        }
    }

    // Emit events
    if let Some(events) = events {
        for (entity, amount) in regen_events {
            events.push(GameEvent::EnergyRegenerated { entity, amount });
        }
    }
}

/// Process status effect duration ticks, removing expired effects
pub fn tick_status_effects(world: &mut World, elapsed: f32) {
    if elapsed <= 0.0 {
        return;
    }

    for (_, effects) in world.query_mut::<&mut StatusEffects>() {
        effects.effects.retain_mut(|effect| {
            effect.remaining_duration -= elapsed;
            effect.remaining_duration > 0.0
        });
    }
}

/// Process ability cooldown ticks
pub fn tick_ability_cooldowns(world: &mut World, elapsed: f32) {
    use crate::components::{ClassAbility, SecondaryAbility};

    if elapsed <= 0.0 {
        return;
    }

    for (_, ability) in world.query_mut::<&mut ClassAbility>() {
        if ability.cooldown_remaining > 0.0 {
            ability.cooldown_remaining = (ability.cooldown_remaining - elapsed).max(0.0);
        }
    }

    // Also tick secondary abilities (Druid's Barkskin)
    for (_, ability) in world.query_mut::<&mut SecondaryAbility>() {
        if ability.cooldown_remaining > 0.0 {
            ability.cooldown_remaining = (ability.cooldown_remaining - elapsed).max(0.0);
        }
    }
}

/// Process ranged attack cooldown ticks (for enemies with bows)
pub fn tick_ranged_cooldowns(world: &mut World, elapsed: f32) {
    if elapsed <= 0.0 {
        return;
    }

    for (_, cooldown) in world.query_mut::<&mut RangedCooldown>() {
        if cooldown.remaining > 0.0 {
            cooldown.remaining = (cooldown.remaining - elapsed).max(0.0);
        }
    }
}

/// Process burn damage for entities with the Burning effect
pub fn tick_burn_damage(world: &mut World, current_time: f32, events: &mut EventQueue) {
    use crate::components::Position;

    // Collect entities that need to take burn damage
    let mut burn_events: Vec<(Entity, (f32, f32), i32)> = Vec::new();
    let mut deaths: Vec<Entity> = Vec::new();

    // First pass: find burning entities and check if they should take damage
    for (entity, (health, effects, pos)) in
        world.query_mut::<(&mut Health, &mut StatusEffects, &Position)>()
    {
        // Find the burning effect
        if let Some(burn_effect) = effects
            .effects
            .iter_mut()
            .find(|e| e.effect_type == EffectType::Burning)
        {
            let time_since_last = current_time - burn_effect.last_damage_tick;
            if time_since_last >= BURNING_DAMAGE_INTERVAL {
                // Deal damage
                let damage = BURNING_DAMAGE_PER_SECOND;
                health.current = (health.current - damage).max(0);
                burn_effect.last_damage_tick = current_time;

                burn_events.push((entity, (pos.x as f32 + 0.5, pos.y as f32 + 0.5), damage));

                if health.current <= 0 {
                    deaths.push(entity);
                }
            }
        }
    }

    // Emit burn damage events
    for (entity, position, damage) in burn_events {
        events.push(GameEvent::BurnDamage {
            entity,
            position,
            damage,
        });
    }

    // Handle deaths from burning
    for entity in deaths {
        if let Ok(pos) = world.get::<&Position>(entity) {
            events.push(GameEvent::EntityDied {
                entity,
                position: (pos.x as f32 + 0.5, pos.y as f32 + 0.5),
            });
        }
    }
}

