//! AI decision-making and behavior systems.
//!
//! This module handles AI state machines, perception, threat-based targeting,
//! and action selection for non-player entities.
//!
//! Enemies use a WoW-style threat table to decide who to chase. Companions
//! operate in defensive mode — they only engage enemies that have attacked
//! the player or that the player has attacked.

use std::collections::HashSet;

use hecs::{Entity, World};
use rand::Rng;

use crate::active_ai_tracker::ActiveAITracker;
use crate::components::{ActionType, Actor, AIState, ChaseAI, CompanionAI, Door, EffectType, Equipment, Health, Position, RangedCooldown, TamedBy};
use crate::constants::*;
use crate::events::{EventQueue, GameEvent};
use crate::grid::Grid;
use crate::pathfinding::{self, BresenhamLineIter};
use crate::queries;
use crate::spatial_cache::SpatialCache;
use crate::systems::action_dispatch;
use crate::time_system::{self, ActionScheduler, GameClock};

// =============================================================================
// THREAT GENERATION
// =============================================================================

/// Generate threat on an enemy from a damage source.
/// Call this whenever an entity deals damage to an entity that has ChaseAI.
pub fn generate_threat(world: &mut World, enemy: Entity, threat_source: Entity, amount: f32) {
    if let Ok(mut ai) = world.get::<&mut ChaseAI>(enemy) {
        ai.add_threat(threat_source, amount);
    }
}

/// Generate threat on a companion from a damage source.
/// Call this whenever an entity deals damage to a companion.
pub fn generate_companion_threat(world: &mut World, companion: Entity, threat_source: Entity, amount: f32) {
    if let Ok(mut ai) = world.get::<&mut CompanionAI>(companion) {
        ai.add_threat(threat_source, amount);
    }
}

// =============================================================================
// THREAT DECAY
// =============================================================================

/// Decay threat over time with visibility-aware rates.
/// Visible targets decay slowly, non-visible targets decay fast.
pub fn tick_threat_decay(world: &mut World, grid: &Grid, spatial_cache: &SpatialCache, elapsed: f32) {
    if elapsed <= 0.0 {
        return;
    }

    // Collect entity positions for visibility checks
    let positions: Vec<(Entity, (i32, i32))> = world
        .query::<&Position>()
        .iter()
        .map(|(e, p)| (e, (p.x, p.y)))
        .collect();

    let pos_lookup = |entity: Entity| -> Option<(i32, i32)> {
        positions.iter().find(|(e, _)| *e == entity).map(|(_, p)| *p)
    };

    // Decay enemy threat tables
    for (_, (pos, ai)) in world.query_mut::<(&Position, &mut ChaseAI)>() {
        let entity_pos = (pos.x, pos.y);
        for entry in ai.threat_table.iter_mut() {
            let target_visible = pos_lookup(entry.entity)
                .map(|tp| is_within_sight(entity_pos, tp, ai.sight_radius)
                    && has_line_of_sight(grid, spatial_cache.get_vision_blocking(), entity_pos.0, entity_pos.1, tp.0, tp.1))
                .unwrap_or(false);
            let decay_rate = if target_visible { THREAT_DECAY_VISIBLE } else { THREAT_DECAY_HIDDEN };
            entry.threat = (entry.threat - decay_rate * elapsed).max(THREAT_MINIMUM);
            if target_visible {
                entry.time_at_minimum = 0.0;
            } else if entry.threat <= THREAT_MINIMUM {
                entry.time_at_minimum += elapsed;
            }
        }
        ai.threat_table.retain(|e| e.time_at_minimum < THREAT_MEMORY_DURATION);
    }

    // Decay companion threat tables
    for (_, (pos, ai)) in world.query_mut::<(&Position, &mut CompanionAI)>() {
        let entity_pos = (pos.x, pos.y);
        for entry in ai.threat_table.iter_mut() {
            let target_visible = pos_lookup(entry.entity)
                .map(|tp| is_within_sight(entity_pos, tp, 8) // Companions use fixed sight radius
                    && has_line_of_sight(grid, spatial_cache.get_vision_blocking(), entity_pos.0, entity_pos.1, tp.0, tp.1))
                .unwrap_or(false);
            let decay_rate = if target_visible { THREAT_DECAY_VISIBLE } else { THREAT_DECAY_HIDDEN };
            entry.threat = (entry.threat - decay_rate * elapsed).max(THREAT_MINIMUM);
            if target_visible {
                entry.time_at_minimum = 0.0;
            } else if entry.threat <= THREAT_MINIMUM {
                entry.time_at_minimum += elapsed;
            }
        }
        ai.threat_table.retain(|e| e.time_at_minimum < THREAT_MEMORY_DURATION);
    }
}

/// Simple distance check for sight (Chebyshev distance).
fn is_within_sight(from: (i32, i32), to: (i32, i32), radius: i32) -> bool {
    let dx = (from.0 - to.0).abs();
    let dy = (from.1 - to.1).abs();
    dx.max(dy) <= radius
}

// =============================================================================
// AI DECISION ENTRY POINT
// =============================================================================

/// Have an AI entity decide and start its next action.
pub fn decide_action(
    world: &mut World,
    grid: &Grid,
    entity: Entity,
    player_entity: Entity,
    clock: &GameClock,
    scheduler: &mut ActionScheduler,
    active_tracker: &mut ActiveAITracker,
    spatial_cache: &SpatialCache,
    events: &mut EventQueue,
    rng: &mut impl Rng,
) {
    puffin::profile_function!();

    // FIRST: Distance check - cheapest operation, do this before anything else
    let entity_pos = world.get::<&Position>(entity).ok().map(|p| (p.x, p.y));
    let player_pos = world.get::<&Position>(player_entity).ok().map(|p| (p.x, p.y));

    if let (Some(epos), Some(ppos)) = (entity_pos, player_pos) {
        let distance = (epos.0 - ppos.0).abs() + (epos.1 - ppos.1).abs();
        if distance > AI_ACTIVE_RADIUS {
            active_tracker.mark_dormant(entity);
            return;
        }
    } else {
        return;
    }

    // Check if entity has Actor component
    let (can_act, energy_regen_interval, last_regen_time) = match world.get::<&Actor>(entity) {
        Ok(a) => (a.can_act(), a.energy_regen_interval, a.last_energy_regen_time),
        Err(_) => return,
    };

    // If can't act (usually out of energy), schedule to wake up when energy regens
    if !can_act {
        if energy_regen_interval > 0.0 {
            let next_regen_time = last_regen_time + energy_regen_interval;
            if next_regen_time > clock.time {
                scheduler.schedule(entity, next_regen_time);
            } else {
                scheduler.schedule(entity, clock.time + energy_regen_interval);
            }
        }
        return;
    }

    // Check if entity has AI (ChaseAI for enemies, CompanionAI for tamed animals)
    let has_chase_ai = world.get::<&ChaseAI>(entity).is_ok();
    let companion_ai = world.get::<&CompanionAI>(entity).ok().map(|ai| (ai.owner, ai.follow_distance));

    if !has_chase_ai && companion_ai.is_none() {
        return;
    }

    // Determine AI action based on AI type
    let action_type = if let Some((owner, follow_distance)) = companion_ai {
        determine_companion_action(world, grid, entity, owner, follow_distance, spatial_cache, rng)
    } else {
        determine_action(world, grid, entity, player_entity, spatial_cache, events, rng)
    };

    let _ = time_system::start_action_with_events(world, entity, action_type, clock, scheduler, Some(events));
}

// =============================================================================
// ENEMY AI (THREAT-BASED)
// =============================================================================

/// Determine what action an enemy AI entity should take based on threat tables.
fn determine_action(
    world: &mut World,
    grid: &Grid,
    entity: Entity,
    player_entity: Entity,
    spatial_cache: &SpatialCache,
    events: &mut EventQueue,
    rng: &mut impl Rng,
) -> ActionType {
    // Get entity position
    let entity_pos = match world.get::<&Position>(entity) {
        Ok(p) => (p.x, p.y),
        Err(_) => return ActionType::Wait,
    };

    // Get blocking positions from cache
    let blocking_positions = spatial_cache.get_blocking_positions();

    // Check for status effects that override normal AI behavior
    let is_confused = queries::has_status_effect(world, entity, EffectType::Confused);
    let is_feared = queries::has_status_effect(world, entity, EffectType::Feared);
    let is_rooted = queries::has_status_effect(world, entity, EffectType::Rooted);

    // Confused: move randomly, ignore everything
    if is_confused && !is_rooted {
        let (dx, dy) = random_wander(grid, entity_pos, blocking_positions, rng);
        if dx == 0 && dy == 0 {
            return ActionType::Wait;
        }
        return action_dispatch::determine_action_type(world, grid, entity, dx, dy);
    }

    // Get AI state and parameters
    let (sight_radius, current_state, ranged_min, ranged_max) =
        match world.get::<&ChaseAI>(entity) {
            Ok(ai) => (ai.sight_radius, ai.state, ai.ranged_min, ai.ranged_max),
            Err(_) => return ActionType::Wait,
        };

    // Feared: flee from highest-threat source
    if is_feared && !is_rooted {
        let flee_from = world.get::<&ChaseAI>(entity).ok()
            .and_then(|ai| ai.highest_threat().map(|e| e.entity))
            .and_then(|e| queries::get_entity_position(world, e))
            .unwrap_or_else(|| queries::get_entity_position(world, player_entity).unwrap_or(entity_pos));
        let (dx, dy) = flee_from_target(grid, entity_pos, flee_from, blocking_positions, rng);
        if dx == 0 && dy == 0 {
            return ActionType::Wait;
        }
        return action_dispatch::determine_action_type(world, grid, entity, dx, dy);
    }

    // Check if entity has a bow equipped (for ranged attacks)
    let has_ranged_weapon = world
        .get::<&Equipment>(entity)
        .map(|e| e.has_bow())
        .unwrap_or(false);

    // Build potential targets: player + all living companions
    let mut potential_targets: Vec<(Entity, (i32, i32))> = Vec::new();
    if let Some(ppos) = queries::get_entity_position(world, player_entity) {
        potential_targets.push((player_entity, ppos));
    }
    for (comp_id, (comp_pos, _, health)) in world.query::<(&Position, &CompanionAI, &Health)>().iter() {
        if health.current > 0 {
            potential_targets.push((comp_id, (comp_pos.x, comp_pos.y)));
        }
    }

    // Visibility scan: check which targets we can see, add passive threat, update positions
    let mut visible_targets: HashSet<Entity> = HashSet::new();
    for &(target_entity, target_pos) in &potential_targets {
        let can_see = can_see_target(world, grid, entity_pos, target_pos, sight_radius, Some(target_entity), spatial_cache);
        if can_see {
            visible_targets.insert(target_entity);
            // Passive visibility threat (enemies notice things walking around)
            if let Ok(mut ai) = world.get::<&mut ChaseAI>(entity) {
                ai.add_threat(target_entity, THREAT_PASSIVE_VISIBILITY);
                ai.update_target_pos(target_entity, target_pos);
            }
        }
    }

    // Pick best target: highest threat that is visible or has a last_known_pos
    let best_target: Option<(Entity, (i32, i32), bool)> = {
        let ai = world.get::<&ChaseAI>(entity).ok();
        ai.and_then(|ai| {
            // Sort threat table by descending threat
            let mut entries: Vec<_> = ai.threat_table.iter().collect();
            entries.sort_by(|a, b| b.threat.partial_cmp(&a.threat).unwrap_or(std::cmp::Ordering::Equal));

            for entry in entries {
                // Check if target is alive
                let alive = world.get::<&Health>(entry.entity)
                    .map(|h| h.current > 0)
                    .unwrap_or(false);
                if !alive {
                    continue;
                }

                let is_visible = visible_targets.contains(&entry.entity);
                if is_visible {
                    let pos = queries::get_entity_position(world, entry.entity).unwrap();
                    return Some((entry.entity, pos, true));
                } else if let Some(last_known) = entry.last_known_pos {
                    return Some((entry.entity, last_known, false));
                }
            }
            None
        })
    };

    // Update current_target and state machine
    let (chase_target_entity, chase_pos, target_visible) = match best_target {
        Some((e, pos, vis)) => (Some(e), Some(pos), vis),
        None => (None, None, false),
    };

    // Get the last_known_pos for the current target (for state machine)
    let last_known = chase_target_entity.and_then(|te| {
        world.get::<&ChaseAI>(entity).ok().and_then(|ai| ai.last_known_pos_for(te))
    });

    let (new_state, move_target, new_last_known) = update_state_machine(
        current_state,
        entity_pos,
        chase_pos,
        last_known,
        target_visible,
    );

    // Emit state change event if state changed
    if new_state != current_state {
        events.push(GameEvent::AIStateChanged {
            entity,
            new_state,
        });
    }

    // Update AI state
    if let Ok(mut ai) = world.get::<&mut ChaseAI>(entity) {
        ai.state = new_state;
        ai.current_target = chase_target_entity;
        // Update last_known_pos for the current target
        if let Some(te) = chase_target_entity {
            if let Some(lk) = new_last_known {
                ai.update_target_pos(te, lk);
            }
        }
    }

    // If rooted, can only attack adjacent targets - cannot move
    if is_rooted {
        // Check all threat targets for adjacency
        for &(target_entity, target_pos) in &potential_targets {
            let dx = target_pos.0 - entity_pos.0;
            let dy = target_pos.1 - entity_pos.1;
            if dx.abs() <= 1 && dy.abs() <= 1 && (dx != 0 || dy != 0) {
                return action_dispatch::determine_action_type(world, grid, entity, dx, dy);
            }
        }
        return ActionType::Wait;
    }

    // Check for ranged attack against best visible target in range
    if has_ranged_weapon && ranged_max > 0 {
        let ranged_ready = world
            .get::<&RangedCooldown>(entity)
            .map(|cd| cd.remaining <= 0.0)
            .unwrap_or(true);

        if ranged_ready {
            // Try to shoot highest-threat visible target in range
            if let Ok(ai) = world.get::<&ChaseAI>(entity) {
                let mut entries: Vec<_> = ai.threat_table.iter().collect();
                entries.sort_by(|a, b| b.threat.partial_cmp(&a.threat).unwrap_or(std::cmp::Ordering::Equal));

                for entry in entries {
                    if !visible_targets.contains(&entry.entity) {
                        continue;
                    }
                    if let Some(tp) = queries::get_entity_position(world, entry.entity) {
                        let distance = (entity_pos.0 - tp.0).abs().max((entity_pos.1 - tp.1).abs());
                        if distance >= ranged_min && distance <= ranged_max {
                            if has_clear_shot(entity_pos, tp, blocking_positions) {
                                return ActionType::ShootBow { target_x: tp.0, target_y: tp.1 };
                            }
                        }
                    }
                }
            }
        }
    }

    // Determine movement direction
    let (dx, dy) = if let Some(target_pos) = move_target {
        let pathfinding_blocked = ai_pathfinding_blocked(world, spatial_cache);
        pathfinding::next_step_toward(grid, entity_pos, target_pos, &pathfinding_blocked)
            .map(|(nx, ny)| (nx - entity_pos.0, ny - entity_pos.1))
            .unwrap_or((0, 0))
    } else {
        // Idle wandering
        random_wander(grid, entity_pos, blocking_positions, rng)
    };

    if dx == 0 && dy == 0 {
        return ActionType::Wait;
    }

    let action = action_dispatch::determine_action_type(world, grid, entity, dx, dy);

    // Don't attack fellow enemies - if we pathfound through one, just wait
    if let ActionType::Attack { target } = action {
        if world.get::<&ChaseAI>(target).is_ok() {
            return ActionType::Wait;
        }
    }

    action
}

// =============================================================================
// COMPANION AI (DEFENSIVE MODE)
// =============================================================================

/// Determine what action a companion (tamed animal) should take.
/// Defensive mode: only engages enemies that have attacked the player, that the
/// player has attacked, or that have attacked the companion directly.
fn determine_companion_action(
    world: &World,
    grid: &Grid,
    entity: Entity,
    owner: Entity,
    follow_distance: i32,
    spatial_cache: &SpatialCache,
    rng: &mut impl Rng,
) -> ActionType {
    let _ = rng;

    // Get companion position
    let companion_pos = match world.get::<&Position>(entity) {
        Ok(p) => (p.x, p.y),
        Err(_) => return ActionType::Wait,
    };

    // Get owner position
    let owner_pos = match world.get::<&Position>(owner) {
        Ok(p) => (p.x, p.y),
        Err(_) => return ActionType::Wait,
    };

    let blocking = ai_pathfinding_blocked(world, spatial_cache);

    // Priority 1: Fight highest-threat entry in our own threat table
    // (enemies that attacked us or our owner)
    if let Ok(ai) = world.get::<&CompanionAI>(entity) {
        let mut entries: Vec<_> = ai.threat_table.iter().collect();
        entries.sort_by(|a, b| b.threat.partial_cmp(&a.threat).unwrap_or(std::cmp::Ordering::Equal));

        for entry in entries {
            // Skip sibling companions
            let is_sibling = world.get::<&TamedBy>(entry.entity)
                .map(|t| t.owner == owner)
                .unwrap_or(false);
            if is_sibling {
                continue;
            }

            // Check if target is still alive
            let alive = world.get::<&Health>(entry.entity)
                .map(|h| h.current > 0)
                .unwrap_or(false);
            if !alive {
                continue;
            }

            if let Some(target_pos) = queries::get_entity_position(world, entry.entity) {
                return pursue_target(world, grid, entity, companion_pos, entry.entity, target_pos, &blocking);
            }
        }
    }

    // Priority 2: Assist with enemies that have the player in their threat table
    // (enemies currently in combat with the player)
    let mut best_enemy: Option<(Entity, i32, (i32, i32))> = None;
    for (enemy_id, (enemy_pos, ai, health)) in world.query::<(&Position, &ChaseAI, &Health)>().iter() {
        if health.current <= 0 {
            continue;
        }
        // Check if this enemy has threat on our owner
        let has_owner_threat = ai.threat_table.iter().any(|e| e.entity == owner);
        if !has_owner_threat {
            continue;
        }
        let dist = (enemy_pos.x - companion_pos.0).abs() + (enemy_pos.y - companion_pos.1).abs();
        if best_enemy.is_none() || dist < best_enemy.unwrap().1 {
            best_enemy = Some((enemy_id, dist, (enemy_pos.x, enemy_pos.y)));
        }
    }
    if let Some((enemy, _, enemy_pos)) = best_enemy {
        return pursue_target(world, grid, entity, companion_pos, enemy, enemy_pos, &blocking);
    }

    // Priority 3: Follow owner if too far
    let dist_to_owner = (companion_pos.0 - owner_pos.0)
        .abs()
        .max((companion_pos.1 - owner_pos.1).abs());

    if dist_to_owner > follow_distance {
        if let Some((nx, ny)) = pathfinding::next_step_toward(grid, companion_pos, owner_pos, &blocking) {
            let dx = nx - companion_pos.0;
            let dy = ny - companion_pos.1;
            if dx != 0 || dy != 0 {
                return action_dispatch::determine_action_type(world, grid, entity, dx, dy);
            }
        }
    }

    ActionType::Wait
}

/// Helper: move toward or attack a target entity.
fn pursue_target(
    world: &World,
    grid: &Grid,
    entity: Entity,
    entity_pos: (i32, i32),
    target: Entity,
    target_pos: (i32, i32),
    blocked: &HashSet<(i32, i32)>,
) -> ActionType {
    let dx = target_pos.0 - entity_pos.0;
    let dy = target_pos.1 - entity_pos.1;

    // Adjacent? Attack!
    if dx.abs() <= 1 && dy.abs() <= 1 && (dx != 0 || dy != 0) {
        return ActionType::Attack { target };
    }

    // Pathfind toward target
    if let Some((nx, ny)) = pathfinding::next_step_toward(grid, entity_pos, target_pos, blocked) {
        let move_dx = nx - entity_pos.0;
        let move_dy = ny - entity_pos.1;
        if move_dx != 0 || move_dy != 0 {
            return action_dispatch::determine_action_type(world, grid, entity, move_dx, move_dy);
        }
    }

    ActionType::Wait
}

// =============================================================================
// PERCEPTION
// =============================================================================

/// Check if there's a clear line of sight for a projectile (no blocking entities)
fn has_clear_shot(from: (i32, i32), to: (i32, i32), blocking: &HashSet<(i32, i32)>) -> bool {
    for (x, y) in BresenhamLineIter::new(from.0, from.1, to.0, to.1) {
        if (x, y) == from || (x, y) == to {
            continue;
        }
        if blocking.contains(&(x, y)) {
            return false;
        }
    }
    true
}

/// Check if an entity can see a target position.
fn can_see_target(
    world: &World,
    grid: &Grid,
    from: (i32, i32),
    target: (i32, i32),
    sight_radius: i32,
    target_entity: Option<Entity>,
    spatial_cache: &SpatialCache,
) -> bool {
    // Check if target entity is invisible
    if let Some(entity) = target_entity {
        if queries::has_status_effect(world, entity, EffectType::Invisible) {
            return false;
        }
    }

    if !is_within_sight(from, target, sight_radius) {
        return false;
    }

    let vision_blocking = spatial_cache.get_vision_blocking();
    has_line_of_sight(grid, vision_blocking, from.0, from.1, target.0, target.1)
}

/// Check if there's a clear line of sight between two points.
fn has_line_of_sight(
    grid: &Grid,
    blocking_entities: &HashSet<(i32, i32)>,
    x0: i32,
    y0: i32,
    x1: i32,
    y1: i32,
) -> bool {
    let dx = (x1 - x0).abs();
    let dy = (y1 - y0).abs();
    let sx = if x0 < x1 { 1 } else { -1 };
    let sy = if y0 < y1 { 1 } else { -1 };
    let mut err = dx - dy;

    let mut x = x0;
    let mut y = y0;

    while x != x1 || y != y1 {
        let e2 = 2 * err;
        if e2 > -dy {
            err -= dy;
            x += sx;
        }
        if e2 < dx {
            err += dx;
            y += sy;
        }

        if x == x1 && y == y1 {
            break;
        }

        if let Some(tile) = grid.get(x, y) {
            if tile.tile_type.blocks_vision() {
                return false;
            }
        }

        if blocking_entities.contains(&(x, y)) {
            return false;
        }
    }

    true
}

// =============================================================================
// STATE MACHINE
// =============================================================================

/// Update the AI state machine based on perception (target-agnostic).
fn update_state_machine(
    current_state: AIState,
    entity_pos: (i32, i32),
    target_pos: Option<(i32, i32)>,
    last_known: Option<(i32, i32)>,
    can_see_target: bool,
) -> (AIState, Option<(i32, i32)>, Option<(i32, i32)>) {
    // No target at all
    if target_pos.is_none() && !can_see_target {
        return match current_state {
            AIState::Investigating => {
                if last_known.map(|lk| lk == entity_pos).unwrap_or(true) {
                    (AIState::Idle, None, None)
                } else {
                    (AIState::Investigating, last_known, last_known)
                }
            }
            _ => (AIState::Idle, None, last_known),
        };
    }

    match current_state {
        AIState::Idle => {
            if can_see_target {
                (AIState::Chasing, target_pos, target_pos)
            } else if target_pos.is_some() {
                // Have a last_known_pos from threat table but can't see — investigate
                (AIState::Investigating, target_pos, target_pos)
            } else {
                (AIState::Idle, None, None)
            }
        }
        AIState::Chasing => {
            if can_see_target {
                (AIState::Chasing, target_pos, target_pos)
            } else {
                (AIState::Investigating, last_known, last_known)
            }
        }
        AIState::Investigating => {
            if can_see_target {
                (AIState::Chasing, target_pos, target_pos)
            } else if last_known.map(|lk| lk == entity_pos).unwrap_or(true) {
                (AIState::Idle, None, None)
            } else {
                (AIState::Investigating, last_known, last_known)
            }
        }
    }
}

// =============================================================================
// PATHFINDING HELPERS
// =============================================================================

/// Build a blocked set for AI pathfinding that excludes traversable obstacles.
fn ai_pathfinding_blocked(
    world: &World,
    spatial_cache: &SpatialCache,
) -> HashSet<(i32, i32)> {
    let mut blocked = spatial_cache.get_blocking_positions().clone();

    for (_id, (pos, _)) in world.query::<(&Position, &ChaseAI)>().iter() {
        blocked.remove(&(pos.x, pos.y));
    }
    for (_id, (pos, _)) in world.query::<(&Position, &CompanionAI)>().iter() {
        blocked.remove(&(pos.x, pos.y));
    }
    for (_id, (pos, door)) in world.query::<(&Position, &Door)>().iter() {
        if !door.is_open {
            blocked.remove(&(pos.x, pos.y));
        }
    }

    blocked
}

/// Pick a random adjacent walkable tile for wandering.
fn random_wander(
    grid: &Grid,
    pos: (i32, i32),
    blocked: &HashSet<(i32, i32)>,
    rng: &mut impl Rng,
) -> (i32, i32) {
    let mut valid = [(0i32, 0i32); 4];
    let mut count = 0;
    for (dx, dy) in [(0, 1), (0, -1), (1, 0), (-1, 0)] {
        let target = (pos.0 + dx, pos.1 + dy);
        if grid.is_walkable(target.0, target.1) && !blocked.contains(&target) {
            valid[count] = (dx, dy);
            count += 1;
        }
    }

    if count == 0 {
        (0, 0)
    } else {
        valid[rng.gen_range(0..count)]
    }
}

/// Flee from a target position - move in the opposite direction.
fn flee_from_target(
    grid: &Grid,
    pos: (i32, i32),
    target: (i32, i32),
    blocked: &HashSet<(i32, i32)>,
    rng: &mut impl Rng,
) -> (i32, i32) {
    let flee_dx = (pos.0 - target.0).signum();
    let flee_dy = (pos.1 - target.1).signum();

    if flee_dx != 0 || flee_dy != 0 {
        let nx = pos.0 + flee_dx;
        let ny = pos.1 + flee_dy;
        if grid.is_walkable(nx, ny) && !blocked.contains(&(nx, ny)) {
            return (flee_dx, flee_dy);
        }

        if flee_dx != 0 {
            let nx = pos.0 + flee_dx;
            let ny = pos.1;
            if grid.is_walkable(nx, ny) && !blocked.contains(&(nx, ny)) {
                return (flee_dx, 0);
            }
        }

        if flee_dy != 0 {
            let nx = pos.0;
            let ny = pos.1 + flee_dy;
            if grid.is_walkable(nx, ny) && !blocked.contains(&(nx, ny)) {
                return (0, flee_dy);
            }
        }
    }

    random_wander(grid, pos, blocked, rng)
}
