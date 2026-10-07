//! AI decision-making and behavior systems.
//!
//! This module handles AI state machines, perception, threat-based targeting,
//! and action selection for non-player entities.
//!
//! Enemies use a WoW-style threat table to decide who to chase. Companions
//! operate in defensive mode — they only engage enemies that have attacked
//! the player or that the player has attacked.

use std::collections::{HashMap, HashSet};

use hecs::{Entity, World};
use rand::Rng;

use crate::active_ai_tracker::ActiveAITracker;
use crate::components::{ActionType, Actor, AIState, AlarmInProgress, Asleep, Boss, BossAbility, BossMinion, CanOpenDoors, CausesBurning, ChaseAI, CompanionAI, ContainerType, Door, EffectType, Equipment, Health, Name, PlacedFireTrap, Player, Position, RangedCooldown, Sneaking, Stats, SupportAI, TamedBy, TamingInProgress, WebSpinner};
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

    // Entity positions for visibility checks. A HashMap, not a Vec: this is
    // probed once per threat-table entry per entity per tick, and deep floors
    // carry ~47 enemies with several entries each — a linear scan made the
    // whole pass quadratic in entity count.
    let positions: HashMap<Entity, (i32, i32)> = world
        .query::<&Position>()
        .iter()
        .map(|(e, p)| (e, (p.x, p.y)))
        .collect();

    let pos_lookup = |entity: Entity| -> Option<(i32, i32)> { positions.get(&entity).copied() };

    // Decay enemy threat tables
    for (_, (pos, ai)) in world.query_mut::<(&Position, &mut ChaseAI)>() {
        let entity_pos = (pos.x, pos.y);
        for entry in ai.threat_table.iter_mut() {
            let target_visible = pos_lookup(entry.entity)
                .map(|tp| {
                    // A target concealed in tall grass is only "visible" up close,
                    // so threat decays fast once they break contact into cover.
                    let eff = if is_concealed(grid, tp) {
                        ai.sight_radius.min(CONCEAL_SIGHT_RADIUS)
                    } else {
                        ai.sight_radius
                    };
                    is_within_sight(entity_pos, tp, eff)
                        && has_line_of_sight(grid, spatial_cache.get_vision_blocking(), entity_pos.0, entity_pos.1, tp.0, tp.1)
                })
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
// AWARENESS: WAKE, NOISE, SHOUT
// =============================================================================

/// Find the player entity (single Player marker).
fn find_player(world: &World) -> Option<Entity> {
    world.query::<&Player>().iter().next().map(|(id, _)| id)
}

/// True if any unaware enemy is within `radius` (Chebyshev) of `center`.
fn unaware_ally_near(world: &World, center: (i32, i32), radius: i32, exclude: Entity) -> bool {
    world.query::<(&Position, &ChaseAI)>().iter().any(|(id, (pos, ai))| {
        id != exclude
            && ai.state == AIState::Unaware
            && (pos.x - center.0).abs().max((pos.y - center.1).abs()) <= radius
    })
}

/// Wake a single unaware enemy, sending it to investigate `toward`.
fn wake_one(world: &mut World, entity: Entity, player: Entity, toward: (i32, i32)) {
    let _ = world.remove_one::<Asleep>(entity);
    if let Ok(mut ai) = world.get::<&mut ChaseAI>(entity) {
        if ai.state == AIState::Unaware {
            ai.state = AIState::Investigating;
        }
        ai.add_threat(player, WAKE_THREAT);
        ai.update_target_pos(player, toward);
    }
}

/// Wake every unaware enemy within `radius` of `center` (noise / shout). Woken
/// enemies head toward `center` to investigate the disturbance.
pub fn wake_enemies_in_radius(world: &mut World, center: (i32, i32), radius: i32) {
    let Some(player) = find_player(world) else { return };
    let to_wake: Vec<Entity> = world
        .query::<(&Position, &ChaseAI)>()
        .iter()
        .filter(|(_, (pos, ai))| {
            ai.state == AIState::Unaware
                && (pos.x - center.0).abs().max((pos.y - center.1).abs()) <= radius
        })
        .map(|(id, _)| id)
        .collect();
    for e in to_wake {
        wake_one(world, e, player, center);
    }
}

/// Advance alarm-shout channels and shout cooldowns each frame. When a shout
/// finishes it wakes nearby unaware allies; an interrupted shout (component
/// removed on damage) simply never completes.
pub fn tick_alarms(world: &mut World, elapsed: f32) {
    // Decrement shout cooldowns.
    for (_, ai) in world.query_mut::<&mut ChaseAI>() {
        if ai.shout_cooldown > 0.0 {
            ai.shout_cooldown = (ai.shout_cooldown - elapsed).max(0.0);
        }
    }
    // Advance active shouts; collect completed ones.
    let mut completed: Vec<(Entity, (i32, i32))> = Vec::new();
    for (id, (pos, alarm)) in world.query_mut::<(&Position, &mut AlarmInProgress)>() {
        alarm.remaining -= elapsed;
        if alarm.remaining <= 0.0 {
            completed.push((id, (pos.x, pos.y)));
        }
    }
    for (id, pos) in completed {
        let _ = world.remove_one::<AlarmInProgress>(id);
        wake_enemies_in_radius(world, pos, SHOUT_WAKE_RADIUS);
    }
}

/// Cancel an entity's in-progress alarm shout (e.g. when it takes damage).
pub fn interrupt_shout_on_damage(world: &mut World, entity: Entity) {
    let _ = world.remove_one::<AlarmInProgress>(entity);
}

/// Advance the role-specific ability cooldowns (shaman support casts, spider
/// web-laying, boss unique abilities). Ticked in game-time alongside
/// `tick_alarms`.
pub fn tick_role_cooldowns(world: &mut World, elapsed: f32) {
    if elapsed <= 0.0 {
        return;
    }
    for (_, support) in world.query_mut::<&mut SupportAI>() {
        support.cooldown = (support.cooldown - elapsed).max(0.0);
    }
    for (_, spinner) in world.query_mut::<&mut WebSpinner>() {
        spinner.cooldown = (spinner.cooldown - elapsed).max(0.0);
    }
    for (_, boss) in world.query_mut::<&mut Boss>() {
        boss.cooldown = (boss.cooldown - elapsed).max(0.0);
    }
}

/// Announce any boss the player can now see for the first time
/// ("Gnash, Orc Warlord glares at you!"). Called from the engine tick after
/// the FOV update; cheap (there is at most one boss per floor).
pub fn announce_boss_sightings(world: &mut World, grid: &Grid, events: &mut EventQueue) {
    let mut sighted: Vec<(Entity, String)> = Vec::new();
    for (id, (pos, boss)) in world.query::<(&Position, &Boss)>().iter() {
        if boss.announced {
            continue;
        }
        let visible = grid.get(pos.x, pos.y).map(|t| t.visible).unwrap_or(false);
        if visible {
            let name = world
                .get::<&Name>(id)
                .map(|n| n.0.clone())
                .unwrap_or_else(|_| "The boss".to_string());
            sighted.push((id, name));
        }
    }
    for (id, name) in sighted {
        if let Ok(mut boss) = world.get::<&mut Boss>(id) {
            boss.announced = true;
        }
        events.push(GameEvent::BossSighted { boss: id, name });
    }
}

/// Wake an enemy that was attacked while unaware, sending it after the player.
/// No-op for already-aware enemies and non-enemies.
pub fn wake_on_attacked(world: &mut World, entity: Entity) {
    let is_unaware = world
        .get::<&ChaseAI>(entity)
        .map(|ai| ai.state == AIState::Unaware)
        .unwrap_or(false);
    if !is_unaware {
        return;
    }
    let Some(player) = find_player(world) else { return };
    let toward = queries::get_entity_position(world, player).unwrap_or_else(|| {
        queries::get_entity_position(world, entity).unwrap_or((0, 0))
    });
    wake_one(world, entity, player, toward);
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
    profile_function!();

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
    let is_stunned = queries::has_status_effect(world, entity, EffectType::Stunned);
    let is_confused = queries::has_status_effect(world, entity, EffectType::Confused);
    let is_feared = queries::has_status_effect(world, entity, EffectType::Feared);
    let is_rooted = queries::has_status_effect(world, entity, EffectType::Rooted);

    // Stunned: cannot act at all - skip the turn entirely.
    if is_stunned {
        return ActionType::Wait;
    }

    // Confused: move randomly, ignore everything (including fire hazards)
    if is_confused && !is_rooted {
        let (dx, dy) = random_wander(grid, entity_pos, blocking_positions, &HashSet::new(), rng);
        if dx == 0 && dy == 0 {
            return ActionType::Wait;
        }
        return action_dispatch::determine_action_type(world, grid, entity, dx, dy);
    }

    // Get AI state and parameters
    let (sight_radius, mut current_state, ranged_min, ranged_max) =
        match world.get::<&ChaseAI>(entity) {
            Ok(ai) => (ai.sight_radius, ai.state, ai.ranged_min, ai.ranged_max),
            Err(_) => return ActionType::Wait,
        };

    // Is the player sneaking? (reduces detection by unaware/unalerted enemies)
    let player_sneaking = world.get::<&Sneaking>(player_entity).is_ok();
    // A sleeping player (Asleep marker from the Sleep action) isn't hiding —
    // they're far easier to notice. A Tired player is sloppier too.
    let player_asleep = world.get::<&Asleep>(player_entity).is_ok();
    let player_tired = world
        .get::<&crate::components::Fatigue>(player_entity)
        .map(|f| f.is_tired())
        .unwrap_or(false);

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

    // Busy raising an alarm shout — keep channeling (advanced by tick_alarms).
    if world.get::<&AlarmInProgress>(entity).is_ok() {
        return ActionType::Wait;
    }

    // Spiders lay a web on their own tile every few seconds while active
    // (chasing or idling — never while asleep). A free side effect of the
    // decision, not an action.
    let spinner_state = world
        .get::<&WebSpinner>(entity)
        .ok()
        .map(|w| (w.cooldown <= 0.0, w.interval));
    if let Some((ready, interval)) = spinner_state {
        if ready
            && world.get::<&Asleep>(entity).is_err()
            && crate::systems::webs::try_lay_web(world, entity, entity_pos.0, entity_pos.1)
        {
            if let Ok(mut spinner) = world.get::<&mut WebSpinner>(entity) {
                spinner.cooldown = interval;
            }
        }
    }

    // Unaware (asleep or idly patrolling): not yet aware of the player. Build an
    // alertness meter when the player is in sight — faster the closer they are,
    // slower the higher the player's agility and the deeper the sleep. Crossing
    // the threshold wakes the enemy (gradual, not instant). Out of sight, the
    // meter decays back down.
    if current_state == AIState::Unaware {
        let asleep = world.get::<&Asleep>(entity).is_ok();
        let seen = queries::get_entity_position(world, player_entity).and_then(|p| {
            if can_see_target(world, grid, entity_pos, p, sight_radius, Some(player_entity), spatial_cache) {
                Some((entity_pos.0 - p.0).abs().max((entity_pos.1 - p.1).abs()))
            } else {
                None
            }
        });

        let mut woke = false;
        if let Some(dist) = seen {
            // Proximity factor: 1.0 point-blank, ~0 at the edge of sight.
            let proximity = ((sight_radius - dist).max(0) as f32) / (sight_radius.max(1) as f32);
            let agility = world.get::<&Stats>(player_entity).map(|s| s.agility).unwrap_or(10);
            let stealth = 1.0 + ((agility - 10).max(0) as f32) * AGILITY_STEALTH_FACTOR;
            let mut gain = ALERTNESS_BASE_GAIN * proximity / stealth;
            if asleep {
                gain *= ASLEEP_ALERT_MULT;
            }
            // Tired players are noticed faster (+25% alertness gain).
            if player_tired {
                gain *= TIRED_ALERTNESS_MULT;
            }
            if player_asleep {
                // Lying unconscious in the open: much easier to spot.
                gain *= SLEEPING_PLAYER_ALERT_MULT;
            } else if player_sneaking {
                gain *= SNEAK_ALERTNESS_MULT;
            }
            let new_alert = world.get::<&ChaseAI>(entity).map(|a| a.alertness + gain).unwrap_or(0.0);
            if new_alert >= ALERTNESS_WAKE_THRESHOLD {
                woke = true;
            } else if let Ok(mut a) = world.get::<&mut ChaseAI>(entity) {
                a.alertness = new_alert;
            }
        } else if let Ok(mut a) = world.get::<&mut ChaseAI>(entity) {
            a.alertness = (a.alertness - ALERTNESS_DECAY).max(0.0);
        }

        if woke {
            // Fully roused — commit to the chase (seed threat so a sneaking
            // player can't immediately slip back out of the reduced-sight scan).
            let _ = world.remove_one::<Asleep>(entity);
            let ppos = queries::get_entity_position(world, player_entity);
            if let Ok(mut ai) = world.get::<&mut ChaseAI>(entity) {
                ai.state = AIState::Chasing;
                ai.alertness = 0.0;
                ai.add_threat(player_entity, WAKE_THREAT);
                if let Some(p) = ppos {
                    ai.update_target_pos(player_entity, p);
                }
            }
            current_state = AIState::Chasing;
        } else if asleep {
            return ActionType::Wait; // still sleeping (perhaps stirring)
        } else {
            // Awake but unaware: patrol/wander.
            let fire = fire_positions(world);
            let (dx, dy) = random_wander(grid, entity_pos, blocking_positions, &fire, rng);
            if dx == 0 && dy == 0 {
                return ActionType::Wait;
            }
            return action_dispatch::determine_action_type(world, grid, entity, dx, dy);
        }
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
        // A sneaking player is harder for a not-yet-alerted (Idle) enemy to spot.
        let eff_sight = if target_entity == player_entity
            && player_sneaking
            && current_state == AIState::Idle
        {
            (((sight_radius as f32) * SNEAK_SIGHT_MULT).round() as i32).max(1)
        } else {
            sight_radius
        };
        let can_see = can_see_target(world, grid, entity_pos, target_pos, eff_sight, Some(target_entity), spatial_cache);
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

    // Give up: reached the target's last-known position without reacquiring sight
    // (Investigating -> Idle). Forget the target entirely so we resume wandering
    // instead of re-pathing to the same stale spot forever.
    let gave_up = current_state == AIState::Investigating && new_state == AIState::Idle;

    // Update AI state
    if let Ok(mut ai) = world.get::<&mut ChaseAI>(entity) {
        ai.state = new_state;
        ai.current_target = chase_target_entity;
        if gave_up {
            if let Some(te) = chase_target_entity {
                ai.remove_target(te);
            }
        } else if let Some(te) = chase_target_entity {
            // Update last_known_pos for the current target
            if let Some(lk) = new_last_known {
                ai.update_target_pos(te, lk);
            }
        }
    }

    // Raise an alarm shout: a smart, aware enemy with a sleeping ally nearby and
    // no shout on cooldown begins a channeled shout (interruptible). It wakes
    // nearby unaware allies on completion (see tick_alarms).
    //
    // Support casters (Goblin Shaman) have a distinct alert behavior: they
    // shout the moment they spot the player, whether or not anyone nearby is
    // still asleep.
    let is_aware = matches!(new_state, AIState::Chasing | AIState::Investigating);
    let is_support = world.get::<&SupportAI>(entity).is_ok();
    let just_spotted = new_state == AIState::Chasing && current_state != AIState::Chasing;
    if is_aware && !is_rooted && world.get::<&CanOpenDoors>(entity).is_ok() {
        let ready = world.get::<&ChaseAI>(entity).map(|ai| ai.shout_cooldown <= 0.0).unwrap_or(false);
        let wants_shout = unaware_ally_near(world, entity_pos, SHOUT_WAKE_RADIUS, entity)
            || (is_support && just_spotted);
        if ready && wants_shout {
            let _ = world.insert_one(entity, AlarmInProgress { remaining: SHOUT_DURATION });
            if let Ok(mut ai) = world.get::<&mut ChaseAI>(entity) {
                ai.shout_cooldown = SHOUT_COOLDOWN;
            }
            events.push(GameEvent::EnemyShout { entity, position: entity_pos });
            return ActionType::Wait;
        }
    }

    // Boss unique abilities (ground slam / summon spiders / raise dead),
    // each on its own cooldown.
    if !is_rooted {
        if let Some(action) = try_boss_ability(
            world, grid, entity, entity_pos, new_state,
            &potential_targets, &visible_targets, spatial_cache, events,
        ) {
            return action;
        }
    }

    // Shaman support cast: heal the most wounded visible ally in range, or
    // haste one that is attacking the player.
    if is_support && is_aware {
        if let Some(action) = try_support_cast(
            world, grid, spatial_cache, entity, entity_pos, player_entity, events,
        ) {
            return action;
        }
    }

    // If rooted, can only attack adjacent targets - cannot move
    if is_rooted {
        // Check all threat targets for adjacency
        for &(_target_entity, target_pos) in &potential_targets {
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
            // Shoot the highest-threat target in range. If it's visible, aim at
            // it directly; if we've lost sight of it (e.g. it slipped into grass)
            // but we're still aware, fire a shot at the tile we last saw it on —
            // so concealment only saves you if you keep moving.
            if let Ok(ai) = world.get::<&ChaseAI>(entity) {
                let mut entries: Vec<_> = ai.threat_table.iter().collect();
                entries.sort_by(|a, b| b.threat.partial_cmp(&a.threat).unwrap_or(std::cmp::Ordering::Equal));

                for entry in entries {
                    let aim = if visible_targets.contains(&entry.entity) {
                        queries::get_entity_position(world, entry.entity)
                    } else if is_aware {
                        entry.last_known_pos
                    } else {
                        None
                    };
                    if let Some(tp) = aim {
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
        // Support casters kite: keep a 3-5 tile cushion from the target they
        // can see — back off when crowded, close in when allies drift out of
        // support range, hold position in the sweet spot.
        if is_support && target_visible {
            let dist = (entity_pos.0 - target_pos.0)
                .abs()
                .max((entity_pos.1 - target_pos.1).abs());
            if dist < SHAMAN_KITE_MIN {
                let (fdx, fdy) = flee_from_target(grid, entity_pos, target_pos, blocking_positions, rng);
                if fdx == 0 && fdy == 0 {
                    return ActionType::Wait;
                }
                return action_dispatch::determine_action_type(world, grid, entity, fdx, fdy);
            } else if dist <= SHAMAN_KITE_MAX {
                return ActionType::Wait;
            }
            // dist > SHAMAN_KITE_MAX: fall through to normal approach.
        }
        let can_open = world.get::<&CanOpenDoors>(entity).is_ok();
        let pathfinding_blocked = ai_pathfinding_blocked(world, spatial_cache, can_open);
        pathfinding::next_step_toward(grid, entity_pos, target_pos, &pathfinding_blocked)
            .map(|(nx, ny)| (nx - entity_pos.0, ny - entity_pos.1))
            .unwrap_or((0, 0))
    } else {
        // Idle wandering — avoid wandering into fire hazards.
        let fire = fire_positions(world);
        random_wander(grid, entity_pos, blocking_positions, &fire, rng)
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
// BOSS ABILITIES
// =============================================================================

/// Fire the boss's unique ability if it is ready and conditions are met.
/// Returns Some(Wait) when the ability was used this turn (the cast IS the
/// turn), None to fall through to normal behavior.
#[allow(clippy::too_many_arguments)]
fn try_boss_ability(
    world: &mut World,
    grid: &Grid,
    entity: Entity,
    entity_pos: (i32, i32),
    state: AIState,
    potential_targets: &[(Entity, (i32, i32))],
    visible_targets: &HashSet<Entity>,
    spatial_cache: &SpatialCache,
    events: &mut EventQueue,
) -> Option<ActionType> {
    let (ability, ready) = match world.get::<&Boss>(entity) {
        Ok(boss) => (boss.ability, boss.cooldown <= 0.0),
        Err(_) => return None,
    };
    if !ready {
        return None;
    }
    let aware = matches!(state, AIState::Chasing | AIState::Investigating);

    let cheb = |a: (i32, i32), b: (i32, i32)| (a.0 - b.0).abs().max((a.1 - b.1).abs());

    match ability {
        BossAbility::GroundSlam => {
            // Needs a visible player-side target within the shockwave.
            let in_range = potential_targets.iter().any(|&(t, tp)| {
                visible_targets.contains(&t) && cheb(entity_pos, tp) <= BOSS_SLAM_RADIUS
            });
            if !in_range {
                return None;
            }
            events.push(GameEvent::BossAbilityUsed { boss: entity, ability, position: entity_pos });

            // Damage + stun everything player-side caught in the radius
            // (fellow enemies are spared — the shockwave is aimed).
            let victims: Vec<(Entity, (i32, i32))> = potential_targets
                .iter()
                .filter(|&&(_, tp)| cheb(entity_pos, tp) <= BOSS_SLAM_RADIUS)
                .copied()
                .collect();
            for (victim, vpos) in victims {
                let damage = crate::systems::combat::apply_damage(world, victim, BOSS_SLAM_DAMAGE);
                crate::systems::effects::add_effect_to_entity(
                    world, victim, EffectType::Stunned, BOSS_SLAM_STUN_DURATION,
                );
                events.push(GameEvent::AttackHit {
                    attacker: entity,
                    target: victim,
                    target_pos: (vpos.0 as f32 + 0.5, vpos.1 as f32 + 0.5),
                    damage,
                    kind: crate::events::DamageKind::Slam,
                    crit: false,
                });
            }
            if let Ok(mut boss) = world.get::<&mut Boss>(entity) {
                boss.cooldown = BOSS_SLAM_COOLDOWN;
            }
            Some(ActionType::Wait)
        }
        BossAbility::SummonSpiders => {
            if !aware {
                return None;
            }
            // Cap living minions; don't consume the cooldown while capped so
            // the next brood follows promptly once one falls.
            let alive = world
                .query::<(&BossMinion, &Health)>()
                .iter()
                .filter(|(_, (minion, health))| minion.boss == entity && health.current > 0)
                .count();
            if alive >= BOSS_SPIDER_MINION_CAP {
                return None;
            }
            let want = BOSS_SPIDER_SPAWN_COUNT.min(BOSS_SPIDER_MINION_CAP - alive);

            // Free adjacent tiles for the brood to skitter out of.
            let blocking = spatial_cache.get_blocking_positions();
            let spots: Vec<(i32, i32)> = [
                (-1, 0), (1, 0), (0, -1), (0, 1),
                (-1, -1), (-1, 1), (1, -1), (1, 1),
            ]
            .iter()
            .map(|(dx, dy)| (entity_pos.0 + dx, entity_pos.1 + dy))
            .filter(|&(x, y)| grid.is_walkable(x, y) && !blocking.contains(&(x, y)))
            .take(want)
            .collect();
            if spots.is_empty() {
                return None;
            }

            events.push(GameEvent::BossAbilityUsed { boss: entity, ability, position: entity_pos });
            for spot in spots {
                events.push(GameEvent::BossMinionSpawn { boss: entity, position: spot });
            }
            if let Ok(mut boss) = world.get::<&mut Boss>(entity) {
                boss.cooldown = BOSS_SPIDER_SPAWN_COOLDOWN;
            }
            Some(ActionType::Wait)
        }
        BossAbility::RaiseDead => {
            if !aware {
                return None;
            }
            // Nearest bones pile (corpse container) in range is consumed and
            // rises as a hostile skeleton (spawned by the engine, which owns
            // the scheduler — same path as coffin skeletons).
            let bones: Option<(Entity, (i32, i32))> = world
                .query::<(&Position, &crate::components::Container)>()
                .iter()
                .filter(|(_, (pos, container))| {
                    container.container_type == ContainerType::Corpse
                        && cheb(entity_pos, (pos.x, pos.y)) <= BOSS_RAISE_RANGE
                })
                .min_by_key(|(_, (pos, _))| cheb(entity_pos, (pos.x, pos.y)))
                .map(|(id, (pos, _))| (id, (pos.x, pos.y)));
            let (bones_id, bones_pos) = bones?;

            let _ = world.despawn(bones_id);
            events.push(GameEvent::BossAbilityUsed { boss: entity, ability, position: entity_pos });
            events.push(GameEvent::CoffinSkeletonSpawn { position: bones_pos });
            if let Ok(mut boss) = world.get::<&mut Boss>(entity) {
                boss.cooldown = BOSS_RAISE_COOLDOWN;
            }
            Some(ActionType::Wait)
        }
    }
}

// =============================================================================
// SUPPORT CASTER (GOBLIN SHAMAN)
// =============================================================================

/// Pick the heal target from (entity, current_hp, max_hp) candidates: the
/// living, damaged one at the lowest health fraction.
pub fn select_heal_target(candidates: &[(Entity, i32, i32)]) -> Option<Entity> {
    candidates
        .iter()
        .filter(|&&(_, current, max)| current > 0 && current < max)
        .min_by(|a, b| {
            let fa = a.1 as f32 / a.2.max(1) as f32;
            let fb = b.1 as f32 / b.2.max(1) as f32;
            fa.partial_cmp(&fb).unwrap_or(std::cmp::Ordering::Equal)
        })
        .map(|&(entity, _, _)| entity)
}

/// Cast a support spell if the cooldown is ready: heal the most wounded
/// visible ally within range, or failing that haste an ally that is engaged
/// with the player. Returns Some(Wait) when a cast happened.
fn try_support_cast(
    world: &mut World,
    grid: &Grid,
    spatial_cache: &SpatialCache,
    entity: Entity,
    entity_pos: (i32, i32),
    player_entity: Entity,
    events: &mut EventQueue,
) -> Option<ActionType> {
    let ready = world
        .get::<&SupportAI>(entity)
        .map(|s| s.cooldown <= 0.0)
        .unwrap_or(false);
    if !ready {
        return None;
    }

    // Visible living allies within support range.
    let vision_blocking = spatial_cache.get_vision_blocking();
    let allies: Vec<(Entity, (i32, i32), i32, i32)> = world
        .query::<(&Position, &ChaseAI, &Health)>()
        .iter()
        .filter(|&(id, (pos, _, health))| {
            id != entity
                && health.current > 0
                && (pos.x - entity_pos.0).abs().max((pos.y - entity_pos.1).abs())
                    <= SHAMAN_SUPPORT_RANGE
                && has_line_of_sight(grid, vision_blocking, entity_pos.0, entity_pos.1, pos.x, pos.y)
        })
        .map(|(id, (pos, _, health))| (id, (pos.x, pos.y), health.current, health.max))
        .collect();
    if allies.is_empty() {
        return None;
    }

    // Priority 1: mend the most wounded ally.
    let heal_candidates: Vec<(Entity, i32, i32)> =
        allies.iter().map(|&(id, _, cur, max)| (id, cur, max)).collect();
    if let Some(target) = select_heal_target(&heal_candidates) {
        let mut healed = 0;
        if let Ok(mut health) = world.get::<&mut Health>(target) {
            let before = health.current;
            health.current = (health.current + SHAMAN_HEAL_AMOUNT).min(health.max);
            healed = health.current - before;
        }
        if healed > 0 {
            let target_pos = allies
                .iter()
                .find(|&&(id, ..)| id == target)
                .map(|&(_, pos, ..)| pos)
                .unwrap_or(entity_pos);
            // Green vfx + message only when the player can see it happen.
            let seen = grid.get(target_pos.0, target_pos.1).map(|t| t.visible).unwrap_or(false);
            if seen {
                events.push(GameEvent::EnemyHealed {
                    healer: entity,
                    target,
                    amount: healed,
                    position: target_pos,
                });
            }
            if let Ok(mut support) = world.get::<&mut SupportAI>(entity) {
                support.cooldown = SHAMAN_SUPPORT_COOLDOWN;
            }
            return Some(ActionType::Wait);
        }
    }

    // Priority 2: haste an unhastened ally that is attacking the player.
    let haste_target: Option<(Entity, (i32, i32))> = allies
        .iter()
        .filter(|&&(id, ..)| {
            !queries::has_status_effect(world, id, EffectType::SpeedBoost)
                && world
                    .get::<&ChaseAI>(id)
                    .map(|ai| ai.threat_table.iter().any(|e| e.entity == player_entity))
                    .unwrap_or(false)
        })
        .map(|&(id, pos, ..)| (id, pos))
        .next();
    if let Some((target, target_pos)) = haste_target {
        crate::systems::effects::add_effect_to_entity(
            world, target, EffectType::SpeedBoost, SHAMAN_HASTE_DURATION,
        );
        let seen = grid.get(target_pos.0, target_pos.1).map(|t| t.visible).unwrap_or(false);
        if seen {
            events.push(GameEvent::EnemyHasted { healer: entity, target, position: target_pos });
        }
        if let Ok(mut support) = world.get::<&mut SupportAI>(entity) {
            support.cooldown = SHAMAN_SUPPORT_COOLDOWN;
        }
        return Some(ActionType::Wait);
    }

    None
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

    // Companions follow their owner and can route through doors like the player.
    let blocking = ai_pathfinding_blocked(world, spatial_cache, true);

    // Don't attack an animal the owner is currently taming — let the channel finish.
    let taming_target = world.get::<&TamingInProgress>(owner).ok().map(|t| t.target);

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

            // Skip the animal the owner is currently taming
            if taming_target == Some(entry.entity) {
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
        // Skip the animal the owner is currently taming
        if taming_target == Some(enemy_id) {
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

    // Concealment: a target standing in tall grass can only be noticed from close
    // range — cap the effective detection radius.
    let effective_radius = if is_concealed(grid, target) {
        sight_radius.min(CONCEAL_SIGHT_RADIUS)
    } else {
        sight_radius
    };

    if !is_within_sight(from, target, effective_radius) {
        return false;
    }

    let vision_blocking = spatial_cache.get_vision_blocking();
    has_line_of_sight(grid, vision_blocking, from.0, from.1, target.0, target.1)
}

/// True if the tile at `pos` conceals a target standing on it (tall grass).
fn is_concealed(grid: &Grid, pos: (i32, i32)) -> bool {
    grid.get(pos.0, pos.1)
        .map(|t| matches!(t.tile_type, crate::tile::TileType::TallGrass))
        .unwrap_or(false)
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
        // Unaware is resolved before the state machine in determine_action; treat
        // it like Idle here for exhaustiveness.
        AIState::Unaware | AIState::Idle => {
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
/// Build the set of impassable tiles for AI pathfinding. Other actors are walked
/// through (they move out of the way). Closed doors are treated as passable only
/// if `can_open_doors` — dumb enemies must route around them.
fn ai_pathfinding_blocked(
    world: &World,
    spatial_cache: &SpatialCache,
    can_open_doors: bool,
) -> HashSet<(i32, i32)> {
    let mut blocked = spatial_cache.get_blocking_positions().clone();

    for (_id, (pos, _)) in world.query::<(&Position, &ChaseAI)>().iter() {
        blocked.remove(&(pos.x, pos.y));
    }
    for (_id, (pos, _)) in world.query::<(&Position, &CompanionAI)>().iter() {
        blocked.remove(&(pos.x, pos.y));
    }
    if can_open_doors {
        for (_id, (pos, door)) in world.query::<(&Position, &Door)>().iter() {
            if !door.is_open {
                blocked.remove(&(pos.x, pos.y));
            }
        }
    }

    blocked
}

/// Collect tile positions that cause burning (campfires, braziers, fire traps).
/// Used so idle AI doesn't casually wander into a fire.
fn fire_positions(world: &World) -> HashSet<(i32, i32)> {
    let mut fire = HashSet::new();
    for (_id, (pos, _)) in world.query::<(&Position, &CausesBurning)>().iter() {
        fire.insert((pos.x, pos.y));
    }
    for (_id, (pos, _)) in world.query::<(&Position, &PlacedFireTrap)>().iter() {
        fire.insert((pos.x, pos.y));
    }
    fire
}

/// Pick a random adjacent walkable tile for wandering.
/// Tiles in `fire` are avoided so idle AI doesn't walk into hazards.
fn random_wander(
    grid: &Grid,
    pos: (i32, i32),
    blocked: &HashSet<(i32, i32)>,
    fire: &HashSet<(i32, i32)>,
    rng: &mut impl Rng,
) -> (i32, i32) {
    let mut valid = [(0i32, 0i32); 4];
    let mut count = 0;
    for (dx, dy) in [(0, 1), (0, -1), (1, 0), (-1, 0)] {
        let target = (pos.0 + dx, pos.1 + dy);
        if grid.is_walkable(target.0, target.1)
            && !blocked.contains(&target)
            && !fire.contains(&target)
        {
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

    // Panicked flee fallback — desperation overrides fire avoidance.
    random_wander(grid, pos, blocked, &HashSet::new(), rng)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// An open floor grid with a wall column at `wall_x` (0 = none), so a test
    /// can put a target out of line of sight.
    fn open_grid(width: usize, height: usize, wall_x: Option<usize>) -> Grid {
        use crate::tile::{Tile, TileType};
        let mut tiles = Vec::with_capacity(width * height);
        for _y in 0..height {
            for x in 0..width {
                let wall = wall_x == Some(x);
                tiles.push(Tile::new(if wall { TileType::Wall } else { TileType::Floor }));
            }
        }
        Grid {
            width,
            height,
            tiles,
            chest_positions: vec![],
            door_positions: vec![],
            brazier_positions: vec![],
            decals: vec![],
            stairs_up_pos: None,
            stairs_down_pos: None,
            starting_room: None,
            illumination: vec![0.0; width * height],
            themed_rooms: vec![],
            water_positions: vec![],
            coffin_positions: vec![],
            barrel_positions: vec![],
            shop_position: None,
            shop_decor_positions: vec![],
            trap_positions: vec![],
            furniture_positions: vec![],
            secret_room: None,
            secret_door_pos: None,
            stalagmite_positions: Vec::new(),
            mushroom_positions: Vec::new(),
            crystal_positions: Vec::new(),
        }
    }

    #[test]
    fn test_threat_decay_resolves_each_target_position_independently() {
        // tick_threat_decay looks up every threat target's position. That lookup
        // used a linear scan over a Vec of all entity positions, making the pass
        // quadratic in entity count; it is a HashMap now. This test pins the
        // behaviour the lookup has to preserve: each enemy's own threat entries
        // decay at the visible or hidden rate according to *that* target's
        // position, not whichever entity the scan happened to reach first.
        let grid = open_grid(24, 5, None);

        let mut world = World::new();

        // Two visible targets at distinct positions and one far-away target.
        let near_a = world.spawn((Position::new(2, 2),));
        let near_b = world.spawn((Position::new(4, 2),));
        let far = world.spawn((Position::new(22, 2),));

        // Several enemies, each tracking all three targets, so a mixed-up
        // lookup would show as a wrong decay rate on some entry.
        let mut enemies = Vec::new();
        for x in [1, 3, 5, 6] {
            let mut ai = ChaseAI::new(8);
            ai.add_threat(near_a, 50.0);
            ai.add_threat(near_b, 50.0);
            ai.add_threat(far, 50.0);
            enemies.push(world.spawn((Position::new(x, 2), ai)));
        }

        let cache = SpatialCache::rebuild_from_world(&world);
        let elapsed = 1.0;
        tick_threat_decay(&mut world, &grid, &cache, elapsed);

        for enemy in enemies {
            let ai = world.get::<&ChaseAI>(enemy).expect("enemy keeps its AI");
            let threat_of = |target: Entity| {
                ai.threat_table
                    .iter()
                    .find(|e| e.entity == target)
                    .map(|e| e.threat)
                    .expect("entry retained")
            };

            // The two in-sight targets decay at the slow rate...
            let expected_visible = 50.0 - THREAT_DECAY_VISIBLE * elapsed;
            assert!(
                (threat_of(near_a) - expected_visible).abs() < 1e-5,
                "visible target near_a should decay slowly, got {}",
                threat_of(near_a)
            );
            assert!(
                (threat_of(near_b) - expected_visible).abs() < 1e-5,
                "visible target near_b should decay slowly, got {}",
                threat_of(near_b)
            );

            // ...and the out-of-sight one at the fast rate.
            let expected_hidden = 50.0 - THREAT_DECAY_HIDDEN * elapsed;
            assert!(
                (threat_of(far) - expected_hidden).abs() < 1e-5,
                "far target should decay fast, got {}",
                threat_of(far)
            );
        }
    }

    #[test]
    fn test_threat_decay_handles_target_with_no_position() {
        // A target whose Position is gone (despawned corpse) must simply be
        // treated as not visible rather than panicking or matching some other
        // entity — the HashMap lookup returns None exactly like the scan did.
        let grid = open_grid(12, 5, None);
        let mut world = World::new();

        let ghost = world.spawn((Position::new(3, 2),));
        let mut ai = ChaseAI::new(8);
        ai.add_threat(ghost, 20.0);
        let enemy = world.spawn((Position::new(2, 2), ai));

        let cache = SpatialCache::rebuild_from_world(&world);
        let _ = world.despawn(ghost);

        tick_threat_decay(&mut world, &grid, &cache, 1.0);

        let ai = world.get::<&ChaseAI>(enemy).unwrap();
        let entry = ai.threat_table.iter().find(|e| e.entity == ghost).unwrap();
        assert!(
            (entry.threat - (20.0 - THREAT_DECAY_HIDDEN)).abs() < 1e-5,
            "a target with no position decays at the hidden rate, got {}",
            entry.threat
        );
        assert!(
            entry.time_at_minimum >= 0.0,
            "memory timer should be tracked, not skipped"
        );
    }

    #[test]
    fn test_select_heal_target_prefers_lowest_health_fraction() {
        let mut world = World::new();
        let a = world.spawn(());
        let b = world.spawn(());
        let c = world.spawn(());

        // b is at 25%, a at 50%: heal b even though a is missing more raw HP.
        let candidates = vec![(a, 40, 80), (b, 5, 20), (c, 30, 30)];
        assert_eq!(select_heal_target(&candidates), Some(b));

        // Fully healthy or dead allies are never heal targets.
        let candidates = vec![(a, 80, 80), (b, 0, 20)];
        assert_eq!(select_heal_target(&candidates), None);

        // Empty input degrades gracefully.
        assert_eq!(select_heal_target(&[]), None);
    }
}
