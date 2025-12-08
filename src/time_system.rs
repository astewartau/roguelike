//! Continuous event-driven time system.
//!
//! Manages game time progression through an event-driven loop where time
//! jumps forward to the next action completion rather than ticking.

use crate::components::{
    ActionInProgress, ActionType, Actor, Attackable, BlocksMovement, Container, Door,
    Health, Position,
};
use crate::constants::*;
use crate::events::{EventQueue, GameEvent};
use crate::grid::Grid;
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
// ACTION DURATION CALCULATION
// =============================================================================

/// Calculate the duration of an action for a given actor
pub fn calculate_action_duration(action_type: &ActionType, speed: f32) -> f32 {
    let base_duration = match action_type {
        ActionType::Move { is_diagonal, .. } => {
            let base = ACTION_WALK_DURATION;
            if *is_diagonal {
                base * DIAGONAL_MOVEMENT_MULTIPLIER
            } else {
                base
            }
        }
        ActionType::Attack { .. } => ACTION_ATTACK_DURATION,
        ActionType::OpenDoor { .. } => ACTION_DOOR_DURATION,
        ActionType::OpenChest { .. } => ACTION_CHEST_DURATION,
        ActionType::Wait => ACTION_WAIT_DURATION,
    };

    // Speed modifies duration: higher speed = shorter duration
    base_duration / speed
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

    // Calculate completion time
    let duration = calculate_action_duration(&action_type, actor.speed);
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

/// Result of completing an action
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ActionResult {
    /// Action completed successfully
    Completed,
    /// Movement was blocked
    Blocked,
    /// Entity doesn't exist or has no action
    Invalid,
}

/// Complete an action for an entity, applying its effects
pub fn complete_action(
    world: &mut World,
    grid: &Grid,
    entity: Entity,
    events: &mut EventQueue,
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
    let result = apply_action_effects(world, grid, entity, &action.action_type, events);

    // Clear action (energy regen is now time-based, not action-based)
    if let Ok(mut actor) = world.get::<&mut Actor>(entity) {
        actor.current_action = None;
    }

    result
}

/// Apply the effects of a completed action
fn apply_action_effects(
    world: &mut World,
    grid: &Grid,
    entity: Entity,
    action_type: &ActionType,
    events: &mut EventQueue,
) -> ActionResult {
    match action_type {
        ActionType::Move { dx, dy, .. } => apply_move(world, grid, entity, *dx, *dy, events),
        ActionType::Attack { target } => apply_attack(world, entity, *target, events),
        ActionType::OpenDoor { door } => apply_open_door(world, entity, *door, events),
        ActionType::OpenChest { chest } => apply_open_chest(world, entity, *chest, events),
        ActionType::Wait => ActionResult::Completed,
    }
}

/// Apply movement effect
fn apply_move(
    world: &mut World,
    grid: &Grid,
    entity: Entity,
    dx: i32,
    dy: i32,
    events: &mut EventQueue,
) -> ActionResult {
    // Get current position
    let current_pos = match world.get::<&Position>(entity) {
        Ok(p) => (p.x, p.y),
        Err(_) => return ActionResult::Invalid,
    };

    let target_x = current_pos.0 + dx;
    let target_y = current_pos.1 + dy;

    // Check tile walkability
    let tile_walkable = grid
        .get(target_x, target_y)
        .map(|t| t.tile_type.is_walkable())
        .unwrap_or(false);

    if !tile_walkable {
        return ActionResult::Blocked;
    }

    // Check for attackable entity at target (convert to attack)
    let mut enemy_to_attack: Option<Entity> = None;
    for (id, (enemy_pos, _)) in world.query::<(&Position, &Attackable)>().iter() {
        if id != entity && enemy_pos.x == target_x && enemy_pos.y == target_y {
            enemy_to_attack = Some(id);
            break;
        }
    }
    if let Some(enemy_id) = enemy_to_attack {
        return apply_attack(world, entity, enemy_id, events);
    }

    // Check for closed door at target
    let mut door_to_open: Option<Entity> = None;
    for (id, (door_pos, door)) in world.query::<(&Position, &Door)>().iter() {
        if door_pos.x == target_x && door_pos.y == target_y && !door.is_open {
            door_to_open = Some(id);
            break;
        }
    }
    if let Some(door_id) = door_to_open {
        return apply_open_door(world, entity, door_id, events);
    }

    // Check for container (chest) at target
    let mut chest_action: Option<(Entity, bool, bool)> = None; // (id, is_open, is_empty)
    for (id, (chest_pos, container, _)) in
        world.query::<(&Position, &Container, &BlocksMovement)>().iter()
    {
        if chest_pos.x == target_x && chest_pos.y == target_y {
            chest_action = Some((id, container.is_open, container.is_empty()));
            break;
        }
    }
    if let Some((chest_id, is_open, is_empty)) = chest_action {
        if !is_open || !is_empty {
            // Closed chest, or open chest with items - interact with it
            return apply_open_chest(world, entity, chest_id, events);
        }
        // Open empty chest doesn't block - continue to move
    }

    // Check for any other blocking entity
    let mut is_blocked = false;
    for (id, (blocking_pos, _)) in world.query::<(&Position, &BlocksMovement)>().iter() {
        if id != entity && blocking_pos.x == target_x && blocking_pos.y == target_y {
            is_blocked = true;
            break;
        }
    }
    if is_blocked {
        return ActionResult::Blocked;
    }

    // Execute the move
    if let Ok(mut pos) = world.get::<&mut Position>(entity) {
        let from = (pos.x, pos.y);
        pos.x = target_x;
        pos.y = target_y;
        events.push(GameEvent::EntityMoved {
            entity,
            from,
            to: (target_x, target_y),
        });
    }

    ActionResult::Completed
}

/// Apply attack effect
fn apply_attack(
    world: &mut World,
    attacker: Entity,
    target: Entity,
    events: &mut EventQueue,
) -> ActionResult {
    use crate::components::{Equipment, LungeAnimation, Stats};
    use rand::Rng;

    // Get target position for VFX
    let target_pos = match world.get::<&Position>(target) {
        Ok(p) => (p.x as f32, p.y as f32),
        Err(_) => return ActionResult::Invalid,
    };

    // Calculate damage
    let base_damage = {
        let strength = world
            .get::<&Stats>(attacker)
            .map(|s| s.strength)
            .unwrap_or(10);
        let weapon_damage = world
            .get::<&Equipment>(attacker)
            .ok()
            .and_then(|e| e.weapon.as_ref().map(|w| w.base_damage + w.damage_bonus))
            .unwrap_or(UNARMED_DAMAGE);

        weapon_damage + (strength - 10) / 2
    };

    // Apply damage variance and crit
    let mut rng = rand::thread_rng();
    let damage_mult = rng.gen_range(COMBAT_DAMAGE_MIN_MULT..=COMBAT_DAMAGE_MAX_MULT);
    let is_crit = rng.gen::<f32>() < COMBAT_CRIT_CHANCE;
    let mut damage = (base_damage as f32 * damage_mult) as i32;
    if is_crit {
        damage = (damage as f32 * COMBAT_CRIT_MULTIPLIER) as i32;
    }
    damage = damage.max(1); // Minimum 1 damage

    // Apply damage to target
    if let Ok(mut health) = world.get::<&mut Health>(target) {
        health.current -= damage;
    }

    // Add lunge animation to attacker
    let _ = world.insert_one(attacker, LungeAnimation::new(target_pos.0 + 0.5, target_pos.1 + 0.5));

    // Emit attack event
    events.push(GameEvent::AttackHit {
        attacker,
        target,
        target_pos: (target_pos.0 + 0.5, target_pos.1 + 0.5),
        damage,
    });

    ActionResult::Completed
}

/// Apply open door effect
fn apply_open_door(
    world: &mut World,
    opener: Entity,
    door: Entity,
    events: &mut EventQueue,
) -> ActionResult {
    if let Ok(mut door_comp) = world.get::<&mut Door>(door) {
        door_comp.is_open = true;
    }

    // Remove blocks movement/vision when door opens
    let _ = world.remove_one::<BlocksMovement>(door);
    let _ = world.remove_one::<crate::components::BlocksVision>(door);

    events.push(GameEvent::DoorOpened { door, opener });

    ActionResult::Completed
}

/// Apply open chest effect
fn apply_open_chest(
    world: &mut World,
    opener: Entity,
    chest: Entity,
    events: &mut EventQueue,
) -> ActionResult {
    if let Ok(mut container) = world.get::<&mut Container>(chest) {
        container.is_open = true;
    }

    events.push(GameEvent::ContainerOpened { container: chest, opener });

    ActionResult::Completed
}

// =============================================================================
// TIME-BASED REGENERATION
// =============================================================================

/// Process time-based health regeneration
pub fn tick_health_regen(world: &mut World, current_time: f32, events: Option<&mut EventQueue>) {
    // Collect regen info first to avoid borrow issues
    let mut regen_events: Vec<(Entity, i32)> = Vec::new();

    for (id, health) in world.query_mut::<&mut Health>() {
        // Skip if no regen, already full, or dead
        if health.regen_interval <= 0.0 || health.current >= health.max || health.current <= 0 {
            continue;
        }

        // Calculate how many regen events have occurred
        let time_since_last = current_time - health.last_regen_time;
        if time_since_last >= health.regen_interval {
            let regen_ticks = (time_since_last / health.regen_interval) as i32;
            let amount = (health.regen_amount * regen_ticks).min(health.max - health.current);
            health.current += amount;
            // Update last regen time, accounting for partial intervals
            health.last_regen_time = current_time - (time_since_last % health.regen_interval);

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
    // Collect regen info first to avoid borrow issues
    let mut regen_events: Vec<(Entity, i32)> = Vec::new();

    for (id, actor) in world.query_mut::<&mut Actor>() {
        // Skip if no regen interval set, or already at max
        if actor.energy_regen_interval <= 0.0 || actor.energy >= actor.max_energy {
            continue;
        }

        // Calculate how many regen events have occurred
        let time_since_last = current_time - actor.last_energy_regen_time;
        if time_since_last >= actor.energy_regen_interval {
            let regen_ticks = (time_since_last / actor.energy_regen_interval) as i32;
            let old_energy = actor.energy;
            actor.energy = (actor.energy + regen_ticks).min(actor.max_energy);
            let amount = actor.energy - old_energy;
            // Update last regen time, accounting for partial intervals
            actor.last_energy_regen_time =
                current_time - (time_since_last % actor.energy_regen_interval);

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

// =============================================================================
// ACTION TYPE DETERMINATION
// =============================================================================

/// Determine what action type results from a movement input
pub fn determine_action_type(
    world: &World,
    _grid: &Grid,
    entity: Entity,
    dx: i32,
    dy: i32,
) -> ActionType {
    let is_diagonal = dx != 0 && dy != 0;

    // Get entity position
    let pos = match world.get::<&Position>(entity) {
        Ok(p) => p,
        Err(_) => return ActionType::Wait,
    };

    let target_x = pos.x + dx;
    let target_y = pos.y + dy;

    // Check for attackable entity at target
    for (id, (enemy_pos, _)) in world.query::<(&Position, &Attackable)>().iter() {
        if id != entity && enemy_pos.x == target_x && enemy_pos.y == target_y {
            return ActionType::Attack { target: id };
        }
    }

    // Check for closed door at target
    for (id, (door_pos, door)) in world.query::<(&Position, &Door)>().iter() {
        if door_pos.x == target_x && door_pos.y == target_y && !door.is_open {
            return ActionType::OpenDoor { door: id };
        }
    }

    // Check for chest at target (closed, or open with items still inside)
    for (id, (chest_pos, container, _)) in
        world.query::<(&Position, &Container, &BlocksMovement)>().iter()
    {
        if chest_pos.x == target_x && chest_pos.y == target_y {
            // Interact if closed OR if open but still has items
            if !container.is_open || !container.is_empty() {
                return ActionType::OpenChest { chest: id };
            }
        }
    }

    // Default to move
    ActionType::Move { dx, dy, is_diagonal }
}
