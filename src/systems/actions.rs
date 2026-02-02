//! Action effect implementations.
//!
//! This module contains the logic for applying action effects when actions complete.
//! These are called by the time system after an action's duration has elapsed.

use hecs::{Entity, World};

use crate::components::{
    Attackable, BlocksMovement, ChaseAI, ClassAbility, CompanionAI, Container, ContainerType, Door, EffectType, Equipment,
    EquippedWeapon, Health, Inventory, ItemType, LifeDrainInProgress, LungeAnimation, PlacedTrap, Player, PlayerAttackTarget, Position, Projectile,
    ProjectileMarker, RangedCooldown, SecondaryAbility, Sprite, Stats, StatusEffects, TamedBy, TamingInProgress, TrapType, VisualPosition, Weapon, RangedWeapon,
};
use crate::constants::*;
use crate::events::{EventQueue, GameEvent, StairDirection};
use crate::grid::Grid;
use crate::pathfinding::{BresenhamLineIter, step_distance};
use crate::queries;
use crate::tile::tile_ids;

use super::effects;

/// Result of applying an action's effects
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ActionResult {
    /// Action completed successfully
    Completed,
    /// Movement was blocked
    Blocked,
    /// Entity doesn't exist or has no action
    Invalid,
}

/// Apply talk to NPC effect - emits dialogue started event
pub fn apply_talk_to(player: Entity, npc: Entity, events: &mut EventQueue) -> ActionResult {
    events.push(GameEvent::DialogueStarted { npc, player });
    ActionResult::Completed
}

/// Apply movement effect
pub fn apply_move(
    world: &mut World,
    grid: &Grid,
    entity: Entity,
    dx: i32,
    dy: i32,
    events: &mut EventQueue,
) -> ActionResult {
    // Get current position
    let current_pos = match queries::get_entity_position(world, entity) {
        Some(p) => p,
        None => return ActionResult::Invalid,
    };

    let target_x = current_pos.0 + dx;
    let target_y = current_pos.1 + dy;

    // Check tile walkability
    if !grid.is_walkable(target_x, target_y) {
        return ActionResult::Blocked;
    }

    // Check for attackable entity at target - block movement unless it's our own companion
    if let Some(target_entity) = queries::get_attackable_at(world, target_x, target_y, Some(entity)) {
        // Allow walking through our own tamed companions
        let is_our_companion = world
            .get::<&crate::components::TamedBy>(target_entity)
            .map(|t| t.owner == entity)
            .unwrap_or(false);
        if !is_our_companion {
            return ActionResult::Blocked;
        }
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
    let mut chest_action: Option<(Entity, bool, bool)> = None;
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
            return apply_open_chest(world, entity, chest_id, events);
        }
    }

    // Check for any other blocking entity
    if queries::is_position_blocked(world, target_x, target_y, Some(entity)) {
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

    // Check if entity stepped into water (extinguishes fire)
    if grid.water_positions.contains(&(target_x, target_y)) {
        effects::remove_effect_from_entity(world, entity, EffectType::Burning);
    }

    // Check if entity stepped into a fire source (brazier, campfire) and catches fire
    let stepped_on_fire = world
        .query::<(&Position, &crate::components::CausesBurning)>()
        .iter()
        .any(|(_, (pos, _))| pos.x == target_x && pos.y == target_y);

    if stepped_on_fire {
        use crate::constants::BURNING_DURATION;
        effects::add_effect_to_entity(world, entity, EffectType::Burning, BURNING_DURATION);
        events.push(GameEvent::CaughtFire {
            entity,
            position: (target_x, target_y),
        });
    }

    // Check if entity stepped on a fire trap
    check_fire_trap_trigger(world, entity, target_x, target_y, events);

    // Check if entity stepped on a snare trap
    check_snare_trap_trigger(world, entity, target_x, target_y, events);

    ActionResult::Completed
}

/// Check if an entity stepping on a fire trap should trigger it.
/// Fire traps ignore their owner and the owner's tamed pets.
fn check_fire_trap_trigger(
    world: &mut World,
    victim: Entity,
    target_x: i32,
    target_y: i32,
    events: &mut EventQueue,
) {
    use crate::components::{PlacedFireTrap, TamedBy, Health};
    use crate::constants::BURNING_DURATION;

    // Find fire trap at this position
    let trap_info: Option<(hecs::Entity, Entity, i32)> = world
        .query::<(&Position, &PlacedFireTrap)>()
        .iter()
        .find_map(|(trap_id, (pos, trap))| {
            if pos.x == target_x && pos.y == target_y {
                Some((trap_id, trap.owner, trap.burst_damage))
            } else {
                None
            }
        });

    let Some((trap_entity, trap_owner, burst_damage)) = trap_info else {
        return;
    };

    // Check if victim is the owner
    if victim == trap_owner {
        return;
    }

    // Check if victim is a tamed pet of the owner
    if let Ok(tamed_by) = world.get::<&TamedBy>(victim) {
        if tamed_by.owner == trap_owner {
            return;
        }
    }

    // Trap triggered! Apply burst damage and burning effect
    if let Ok(mut health) = world.get::<&mut Health>(victim) {
        health.current -= burst_damage;
    }

    // Interrupt life drain if victim was channeling
    interrupt_life_drain_on_damage(world, victim, events);

    // Apply burning effect
    effects::add_effect_to_entity(world, victim, EffectType::Burning, BURNING_DURATION);

    // Emit events
    events.push(GameEvent::FireTrapTriggered {
        trap: trap_entity,
        victim,
        position: (target_x, target_y),
    });

    events.push(GameEvent::CaughtFire {
        entity: victim,
        position: (target_x, target_y),
    });

    // Destroy the trap after triggering
    let _ = world.despawn(trap_entity);
}

/// Check if an entity stepping on a snare trap should trigger it.
/// Snare traps ignore their owner and the owner's tamed pets.
fn check_snare_trap_trigger(
    world: &mut World,
    victim: Entity,
    target_x: i32,
    target_y: i32,
    events: &mut EventQueue,
) {
    use crate::components::{PlacedTrap, TrapType, TamedBy};

    // Find snare trap at this position
    let trap_info: Option<(hecs::Entity, Entity, f32)> = world
        .query::<(&Position, &PlacedTrap)>()
        .iter()
        .find_map(|(trap_id, (pos, trap))| {
            if pos.x == target_x && pos.y == target_y {
                if let TrapType::Snare { root_duration } = trap.trap_type {
                    return Some((trap_id, trap.owner, root_duration));
                }
            }
            None
        });

    let Some((trap_entity, trap_owner, root_duration)) = trap_info else {
        return;
    };

    // Check if victim is the owner
    if victim == trap_owner {
        return;
    }

    // Check if victim is a tamed pet of the owner
    if let Ok(tamed_by) = world.get::<&TamedBy>(victim) {
        if tamed_by.owner == trap_owner {
            return;
        }
    }

    // Trap triggered! Apply rooted effect
    effects::add_effect_to_entity(world, victim, EffectType::Rooted, root_duration);

    // Emit event
    events.push(GameEvent::SnareTrapTriggered {
        trap: trap_entity,
        victim,
        position: (target_x, target_y),
    });

    // Destroy the trap after triggering
    let _ = world.despawn(trap_entity);
}

/// Apply attack effect
pub fn apply_attack(
    world: &mut World,
    attacker: Entity,
    target: Entity,
    events: &mut EventQueue,
) -> ActionResult {
    use rand::Rng;

    // Get target position for VFX
    let target_pos = match queries::get_entity_position(world, target) {
        Some(p) => (p.0 as f32, p.1 as f32),
        None => return ActionResult::Invalid,
    };

    // Check for status effects
    let has_strength_boost = queries::has_status_effect(world, attacker, EffectType::Strengthened);
    let has_protection = queries::has_status_effect(world, target, EffectType::Protected)
        || queries::has_status_effect(world, target, EffectType::Barkskin);
    let is_invulnerable = queries::has_status_effect(world, target, EffectType::Invulnerable);

    // Calculate damage
    let base_damage = {
        let strength = world
            .get::<&Stats>(attacker)
            .map(|s| s.strength)
            .unwrap_or(10);
        let weapon_damage = world
            .get::<&Equipment>(attacker)
            .ok()
            .and_then(|e| e.get_melee().map(|w| w.base_damage + w.damage_bonus))
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

    // Apply Strengthened bonus
    if has_strength_boost {
        damage = (damage as f32 * STRENGTH_DAMAGE_MULTIPLIER) as i32;
    }

    // Apply Protected reduction
    if has_protection {
        damage = (damage as f32 * PROTECTION_DAMAGE_REDUCTION) as i32;
    }

    // Invulnerable negates all damage
    if is_invulnerable {
        damage = 0;
    } else {
        damage = damage.max(1);
    }

    // Apply damage to target
    if let Ok(mut health) = world.get::<&mut Health>(target) {
        health.current -= damage;
    }

    // Interrupt life drain if target was channeling
    interrupt_life_drain_on_damage(world, target, events);

    // Track attacker for companion retaliation (but not if attacked by owner)
    if let Ok(mut companion_ai) = world.get::<&mut CompanionAI>(target) {
        if companion_ai.owner != attacker {
            companion_ai.last_attacker = Some(attacker);
        }
    }

    // Track player's attack target for companion assistance
    if let Ok(mut player_target) = world.get::<&mut PlayerAttackTarget>(attacker) {
        player_target.target = Some(target);
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

/// Apply attack direction effect - attacks whatever is at the target tile, or whiffs
pub fn apply_attack_direction(
    world: &mut World,
    attacker: Entity,
    dx: i32,
    dy: i32,
    events: &mut EventQueue,
) -> ActionResult {
    // Get attacker position
    let attacker_pos = match queries::get_entity_position(world, attacker) {
        Some(p) => p,
        None => return ActionResult::Invalid,
    };

    let target_x = attacker_pos.0 + dx;
    let target_y = attacker_pos.1 + dy;

    // Find any Attackable entity at the target position
    if let Some(target) = queries::get_attackable_at(world, target_x, target_y, Some(attacker)) {
        apply_attack(world, attacker, target, events)
    } else {
        // No target - whiff (swing at air), but still add lunge animation
        let _ = world.insert_one(
            attacker,
            LungeAnimation::new(target_x as f32 + 0.5, target_y as f32 + 0.5),
        );
        ActionResult::Completed
    }
}

/// Apply open door effect
pub fn apply_open_door(
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

    let position = world
        .get::<&Position>(door)
        .map(|p| (p.x, p.y))
        .unwrap_or((0, 0));
    events.push(GameEvent::DoorOpened { door, opener, position });

    ActionResult::Completed
}

/// Apply open chest effect
pub fn apply_open_chest(
    world: &mut World,
    opener: Entity,
    chest: Entity,
    events: &mut EventQueue,
) -> ActionResult {
    // Check if this is a coffin that might spawn a skeleton
    let spawn_skeleton = {
        if let Ok(container) = world.get::<&Container>(chest) {
            if container.container_type == ContainerType::Coffin && !container.is_open {
                // Roll for skeleton spawn
                let roll: f32 = rand::random();
                roll < container.spawn_chance
            } else {
                false
            }
        } else {
            false
        }
    };

    // Get position for skeleton spawn if needed
    let spawn_pos = if spawn_skeleton {
        world.get::<&Position>(chest).ok().map(|p| (p.x, p.y))
    } else {
        None
    };

    // Mark container as open and get type
    let container_type = if let Ok(mut container) = world.get::<&mut Container>(chest) {
        container.is_open = true;
        Some(container.container_type)
    } else {
        None
    };

    // Get container position for audio
    let container_pos = world
        .get::<&Position>(chest)
        .map(|p| (p.x, p.y))
        .unwrap_or((0, 0));

    // If skeleton spawns, only emit the spawn event (player must deal with skeleton first)
    // Otherwise, emit ContainerOpened to show loot UI
    if let Some(position) = spawn_pos {
        // Skeleton spawning - emit spawn event but skip loot UI
        // Still emit ContainerOpened for sprite change, but skeleton takes priority
        events.push(GameEvent::ContainerOpened { container: chest, opener, container_type, position: container_pos });
        events.push(GameEvent::CoffinSkeletonSpawn { position });
    } else {
        // No skeleton - normal loot behavior
        events.push(GameEvent::ContainerOpened { container: chest, opener, container_type, position: container_pos });
    }

    ActionResult::Completed
}

/// Apply use stairs effect - moves entity to stairs and emits floor transition event
pub fn apply_use_stairs(
    world: &mut World,
    entity: Entity,
    x: i32,
    y: i32,
    direction: StairDirection,
    events: &mut EventQueue,
) -> ActionResult {
    // Get current position
    let current_pos = match queries::get_entity_position(world, entity) {
        Some(p) => p,
        None => return ActionResult::Invalid,
    };

    // Move entity to the stairs position
    if let Ok(mut pos) = world.get::<&mut Position>(entity) {
        pos.x = x;
        pos.y = y;
    }

    // Emit movement event
    events.push(GameEvent::EntityMoved {
        entity,
        from: current_pos,
        to: (x, y),
    });

    // Emit floor transition event
    events.push(GameEvent::FloorTransition {
        direction,
        from_floor: 0,
    });

    ActionResult::Completed
}

/// Apply shoot bow effect - spawns an arrow projectile
pub fn apply_shoot_bow(
    world: &mut World,
    grid: &Grid,
    shooter: Entity,
    target_x: i32,
    target_y: i32,
    events: &mut EventQueue,
    current_time: f32,
) -> ActionResult {
    use crate::constants::{RANGE_OPTIMAL_MIN, RANGE_OPTIMAL_MAX, RANGE_OPTIMAL_MULT, RANGE_CLOSE_MULT, RANGE_FAR_MULT};

    // Get shooter position
    let (start_x, start_y) = match queries::get_entity_position(world, shooter) {
        Some(p) => p,
        None => return ActionResult::Invalid,
    };

    // Can't shoot at yourself
    if start_x == target_x && start_y == target_y {
        return ActionResult::Blocked;
    }

    // Check if shooter is player (needs arrows) or enemy (unlimited ammo)
    let is_player = world.get::<&Player>(shooter).is_ok();

    // If player, check for and consume arrow from inventory
    if is_player {
        if let Ok(mut inventory) = world.get::<&mut Inventory>(shooter) {
            if let Some(idx) = inventory.items.iter().position(|i| *i == ItemType::Arrow) {
                inventory.items.remove(idx);
            } else {
                // No arrows! Can't shoot
                return ActionResult::Blocked;
            }
        } else {
            return ActionResult::Blocked;
        }
    }

    // Get bow stats
    let (base_damage, arrow_speed) = {
        let equipment = world.get::<&Equipment>(shooter).ok();
        let bow = equipment.as_ref().and_then(|e| e.get_bow());
        match bow {
            Some(bow) => (bow.base_damage, bow.arrow_speed),
            None => return ActionResult::Blocked,
        }
    };

    // Calculate damage with stats
    let agility = world.get::<&Stats>(shooter).map(|s| s.agility).unwrap_or(10);
    let base_calc_damage = base_damage + (agility - 10) / 2;

    // Apply range band modifier (only for player)
    let damage = if is_player {
        let distance = (target_x - start_x).abs().max((target_y - start_y).abs());
        let range_mult = if distance >= RANGE_OPTIMAL_MIN && distance <= RANGE_OPTIMAL_MAX {
            RANGE_OPTIMAL_MULT
        } else if distance <= 2 {
            RANGE_CLOSE_MULT
        } else {
            RANGE_FAR_MULT
        };
        (base_calc_damage as f32 * range_mult) as i32
    } else {
        base_calc_damage
    };

    // Calculate line from shooter to target using Bresenham
    let path = calculate_arrow_path(start_x, start_y, target_x, target_y, arrow_speed, grid);

    if path.is_empty() {
        return ActionResult::Blocked;
    }

    // Calculate normalized direction
    let dx = target_x - start_x;
    let dy = target_y - start_y;
    let len = ((dx * dx + dy * dy) as f32).sqrt();
    let direction = if len > 0.0 {
        (dx as f32 / len, dy as f32 / len)
    } else {
        (1.0, 0.0)
    };

    // Spawn arrow at shooter's position
    let pos = Position::new(start_x, start_y);
    let arrow = world.spawn((
        pos,
        VisualPosition::from_position(&pos),
        Sprite::from_ref(tile_ids::ARROW),
        Projectile {
            source: shooter,
            damage,
            path,
            path_index: 0,
            direction,
            spawn_time: current_time,
            finished: None,
            potion_type: None,
            on_hit_effect: None,
            hit_enemy: false,
        },
        ProjectileMarker,
    ));

    events.push(GameEvent::ProjectileSpawned {
        projectile: arrow,
        source: shooter,
    });

    // Start ranged attack cooldown for the shooter (used by enemies, not player)
    let _ = world.insert_one(shooter, RangedCooldown {
        remaining: crate::constants::RANGED_ATTACK_COOLDOWN,
    });

    ActionResult::Completed
}

/// Calculate arrow path by extending a ray from start through target until hitting a wall.
pub fn calculate_arrow_path(
    start_x: i32,
    start_y: i32,
    target_x: i32,
    target_y: i32,
    arrow_speed: f32,
    grid: &Grid,
) -> Vec<(i32, i32, f32)> {
    if start_x == target_x && start_y == target_y {
        return Vec::new();
    }

    let mut path = Vec::new();
    let mut prev = (start_x, start_y);
    let mut cumulative_time: f32 = 0.0;

    // Extend the line well past the target to hit walls
    let dx = target_x - start_x;
    let dy = target_y - start_y;
    let extended_x = start_x + dx * 50;
    let extended_y = start_y + dy * 50;

    for (x, y) in BresenhamLineIter::new(start_x, start_y, extended_x, extended_y).take(50) {
        cumulative_time += step_distance(prev, (x, y)) / arrow_speed;
        path.push((x, y, cumulative_time));

        // Stop if we hit a wall
        if !grid.is_walkable(x, y) {
            break;
        }

        prev = (x, y);
    }

    path
}

/// Calculate throw path - a direct line from start to target, stopping at the target.
/// Unlike arrows, thrown items don't continue past their target.
pub fn calculate_throw_path(
    start_x: i32,
    start_y: i32,
    target_x: i32,
    target_y: i32,
    throw_speed: f32,
) -> Vec<(i32, i32, f32)> {
    if start_x == target_x && start_y == target_y {
        return Vec::new();
    }

    let mut path = Vec::new();
    let mut prev = (start_x, start_y);
    let mut cumulative_time: f32 = 0.0;

    for (x, y) in BresenhamLineIter::new(start_x, start_y, target_x, target_y) {
        cumulative_time += step_distance(prev, (x, y)) / throw_speed;
        path.push((x, y, cumulative_time));
        prev = (x, y);
    }

    path
}

/// Apply throw potion action - throws a potion at target with splash effect
pub fn apply_throw_potion(
    world: &mut World,
    _grid: &Grid,
    thrower: Entity,
    potion_type: ItemType,
    target_x: i32,
    target_y: i32,
    events: &mut EventQueue,
    current_time: f32,
) -> ActionResult {
    // Get sprite for the potion type
    let sprite_ref = match potion_type {
        ItemType::HealthPotion => tile_ids::RED_POTION,
        ItemType::RegenerationPotion => tile_ids::GREEN_POTION,
        ItemType::StrengthPotion => tile_ids::AMBER_POTION,
        ItemType::ConfusionPotion => tile_ids::BLUE_POTION,
        _ => return ActionResult::Invalid, // Not a throwable
    };

    // Get thrower position
    let (start_x, start_y) = match queries::get_entity_position(world, thrower) {
        Some(p) => p,
        None => return ActionResult::Invalid,
    };

    // Calculate path to target (stops at target, unlike arrows which continue)
    let path = calculate_throw_path(start_x, start_y, target_x, target_y, POTION_THROW_SPEED);

    if path.is_empty() {
        return ActionResult::Blocked;
    }

    // Spawn visual projectile (splash effect applied when projectile finishes)
    let pos = Position::new(start_x, start_y);
    let direction = {
        let dx = (target_x - start_x) as f32;
        let dy = (target_y - start_y) as f32;
        let len = (dx * dx + dy * dy).sqrt().max(0.001);
        (dx / len, dy / len)
    };

    let potion_projectile = world.spawn((
        pos,
        VisualPosition::from_position(&pos),
        Sprite::from_ref(sprite_ref),
        Projectile {
            source: thrower,
            damage: 0,
            path,
            path_index: 0,
            direction,
            spawn_time: current_time,
            finished: None,
            potion_type: Some(potion_type),
            on_hit_effect: None,
            hit_enemy: false,
        },
        ProjectileMarker,
    ));

    events.push(GameEvent::ProjectileSpawned {
        projectile: potion_projectile,
        source: thrower,
    });

    // Splash effect and status application happen when the projectile lands
    // (handled in projectile system)

    ActionResult::Completed
}

/// Apply a potion's splash effect to all entities in the splash radius
pub fn apply_potion_splash(world: &mut World, potion_type: ItemType, center_x: i32, center_y: i32) {
    // Collect entities in splash radius that can be affected
    let mut affected: Vec<Entity> = Vec::new();
    for (entity, (pos, _)) in world.query::<(&Position, &StatusEffects)>().iter() {
        let dx = (pos.x - center_x).abs();
        let dy = (pos.y - center_y).abs();
        if dx <= POTION_SPLASH_RADIUS && dy <= POTION_SPLASH_RADIUS {
            affected.push(entity);
        }
    }

    // Also collect entities with Health but no StatusEffects (for healing potion)
    if potion_type == ItemType::HealthPotion {
        for (entity, (pos, _)) in world.query::<(&Position, &Health)>().iter() {
            let dx = (pos.x - center_x).abs();
            let dy = (pos.y - center_y).abs();
            if dx <= POTION_SPLASH_RADIUS && dy <= POTION_SPLASH_RADIUS {
                if !affected.contains(&entity) {
                    affected.push(entity);
                }
            }
        }
    }

    // Apply effect based on potion type
    for entity in affected {
        match potion_type {
            ItemType::HealthPotion => {
                if let Ok(mut health) = world.get::<&mut Health>(entity) {
                    health.current = (health.current + HEALTH_POTION_HEAL).min(health.max);
                }
            }
            ItemType::RegenerationPotion => {
                effects::add_effect_to_entity(world, entity, EffectType::Regenerating, REGENERATION_DURATION);
            }
            ItemType::StrengthPotion => {
                effects::add_effect_to_entity(world, entity, EffectType::Strengthened, STRENGTH_DURATION);
            }
            ItemType::ConfusionPotion => {
                // Confusion only affects enemies (entities with ChaseAI)
                if world.get::<&ChaseAI>(entity).is_ok() {
                    effects::add_effect_to_entity(world, entity, EffectType::Confused, CONFUSION_DURATION);
                }
            }
            _ => {}
        }
    }
}

/// Apply blink (teleport) action
pub fn apply_blink(
    world: &mut World,
    grid: &Grid,
    entity: Entity,
    target_x: i32,
    target_y: i32,
    events: &mut EventQueue,
) -> ActionResult {
    // Get current position
    let current_pos = match queries::get_entity_position(world, entity) {
        Some(p) => p,
        None => return ActionResult::Invalid,
    };

    // Check range
    let dist = (target_x - current_pos.0).abs().max((target_y - current_pos.1).abs());
    if dist > BLINK_RANGE {
        return ActionResult::Blocked;
    }

    // Check target is walkable
    if !grid.is_walkable(target_x, target_y) {
        return ActionResult::Blocked;
    }

    // Check no blocking entity at target
    if queries::is_position_blocked(world, target_x, target_y, Some(entity)) {
        return ActionResult::Blocked;
    }

    // Teleport: update position
    if let Ok(mut pos) = world.get::<&mut Position>(entity) {
        pos.x = target_x;
        pos.y = target_y;
    }

    // Snap visual position (instant teleport, no lerping)
    if let Ok(mut vis_pos) = world.get::<&mut VisualPosition>(entity) {
        vis_pos.x = target_x as f32;
        vis_pos.y = target_y as f32;
    }

    events.push(GameEvent::EntityMoved {
        entity,
        from: current_pos,
        to: (target_x, target_y),
    });

    ActionResult::Completed
}

/// Apply fireball action - AoE damage at target location
pub fn apply_fireball(
    world: &mut World,
    caster: Entity,
    target_x: i32,
    target_y: i32,
    events: &mut EventQueue,
) -> ActionResult {
    let caster_pos = match queries::get_entity_position(world, caster) {
        Some(p) => p,
        None => return ActionResult::Invalid,
    };

    // Check range
    let dist = (target_x - caster_pos.0).abs().max((target_y - caster_pos.1).abs());
    if dist > FIREBALL_RANGE {
        return ActionResult::Blocked;
    }

    // Emit explosion VFX event
    events.push(GameEvent::FireballExplosion {
        x: target_x,
        y: target_y,
        radius: FIREBALL_RADIUS,
    });

    // Collect all attackable entities in radius
    let mut damaged: Vec<(Entity, i32, i32)> = Vec::new();
    for (id, (pos, _)) in world.query::<(&Position, &Attackable)>().iter() {
        let dx = (pos.x - target_x).abs();
        let dy = (pos.y - target_y).abs();
        if dx <= FIREBALL_RADIUS && dy <= FIREBALL_RADIUS {
            damaged.push((id, pos.x, pos.y));
        }
    }

    // Apply damage to all
    for (entity, x, y) in damaged {
        if let Ok(mut health) = world.get::<&mut Health>(entity) {
            health.current -= FIREBALL_DAMAGE;
        }
        // Interrupt life drain if entity was channeling
        interrupt_life_drain_on_damage(world, entity, events);
        events.push(GameEvent::AttackHit {
            attacker: caster,
            target: entity,
            target_pos: (x as f32 + 0.5, y as f32 + 0.5),
            damage: FIREBALL_DAMAGE,
        });
    }

    ActionResult::Completed
}

/// Apply equip weapon action - equips a weapon from inventory
pub fn apply_equip_weapon(
    world: &mut World,
    entity: Entity,
    item_index: usize,
) -> ActionResult {
    // Get the item type from inventory
    let item_type = {
        let Ok(inventory) = world.get::<&Inventory>(entity) else {
            return ActionResult::Invalid;
        };
        if item_index >= inventory.items.len() {
            return ActionResult::Invalid;
        }
        inventory.items[item_index]
    };

    // Create the equipped weapon
    let new_weapon = match item_type {
        ItemType::Sword => EquippedWeapon::Melee(Weapon::sword()),
        ItemType::Dagger => EquippedWeapon::Melee(Weapon::dagger()),
        ItemType::Staff => EquippedWeapon::Melee(Weapon::staff()),
        ItemType::Bow => EquippedWeapon::Ranged(RangedWeapon::bow()),
        _ => return ActionResult::Invalid, // Not a weapon
    };

    // Check if there's currently an equipped weapon that needs to go to inventory
    let old_weapon_item = {
        if let Ok(equipment) = world.get::<&Equipment>(entity) {
            match &equipment.weapon {
                Some(EquippedWeapon::Melee(weapon)) => match weapon.name.as_str() {
                    "Dagger" => Some(ItemType::Dagger),
                    "Staff" => Some(ItemType::Staff),
                    _ => Some(ItemType::Sword),
                },
                Some(EquippedWeapon::Ranged(_)) => Some(ItemType::Bow),
                None => None,
            }
        } else {
            None
        }
    };

    // Remove the item we're equipping from inventory
    crate::systems::items::remove_item_from_inventory(world, entity, item_index);

    // Add the old weapon to inventory if there was one
    if let Some(old_item) = old_weapon_item {
        crate::systems::inventory::add_item_to_inventory(world, entity, old_item);
    }

    // Equip the new weapon
    if let Ok(mut equipment) = world.get::<&mut Equipment>(entity) {
        equipment.weapon = Some(new_weapon);
    }

    ActionResult::Completed
}

/// Apply unequip weapon action - moves current weapon to inventory
pub fn apply_unequip_weapon(
    world: &mut World,
    entity: Entity,
) -> ActionResult {
    // Get the currently equipped weapon
    let weapon_item = {
        let Ok(equipment) = world.get::<&Equipment>(entity) else {
            return ActionResult::Invalid;
        };
        match &equipment.weapon {
            Some(EquippedWeapon::Melee(weapon)) => match weapon.name.as_str() {
                "Dagger" => Some(ItemType::Dagger),
                "Staff" => Some(ItemType::Staff),
                _ => Some(ItemType::Sword),
            },
            Some(EquippedWeapon::Ranged(_)) => Some(ItemType::Bow),
            None => return ActionResult::Invalid, // Nothing to unequip
        }
    };

    // Add the weapon to inventory
    if let Some(item) = weapon_item {
        crate::systems::inventory::add_item_to_inventory(world, entity, item);
    }

    // Remove weapon from equipment
    if let Ok(mut equipment) = world.get::<&mut Equipment>(entity) {
        equipment.weapon = None;
    }

    ActionResult::Completed
}

/// Apply drop item action - removes item from inventory and spawns on ground
pub fn apply_drop_item(
    world: &mut World,
    entity: Entity,
    item_index: usize,
    events: &mut EventQueue,
) -> ActionResult {
    // Get entity position
    let (x, y) = match queries::get_entity_position(world, entity) {
        Some(p) => p,
        None => return ActionResult::Invalid,
    };

    // Get the item type from inventory
    let item_type = {
        let Ok(inventory) = world.get::<&Inventory>(entity) else {
            return ActionResult::Invalid;
        };
        if item_index >= inventory.items.len() {
            return ActionResult::Invalid;
        }
        inventory.items[item_index]
    };

    // Remove from inventory
    crate::systems::items::remove_item_from_inventory(world, entity, item_index);

    // Spawn on ground
    crate::systems::inventory::spawn_ground_item(world, x, y, item_type);

    // Emit event
    events.push(GameEvent::ItemDropped {
        entity,
        item: item_type,
        position: (x, y),
    });

    ActionResult::Completed
}

/// Apply drop equipped weapon action - unequips and drops weapon on ground
pub fn apply_drop_equipped_weapon(
    world: &mut World,
    entity: Entity,
    events: &mut EventQueue,
) -> ActionResult {
    // Get entity position
    let (x, y) = match queries::get_entity_position(world, entity) {
        Some(p) => p,
        None => return ActionResult::Invalid,
    };

    // Get the currently equipped weapon
    let weapon_item = {
        let Ok(equipment) = world.get::<&Equipment>(entity) else {
            return ActionResult::Invalid;
        };
        match &equipment.weapon {
            Some(EquippedWeapon::Melee(weapon)) => match weapon.name.as_str() {
                "Dagger" => Some(ItemType::Dagger),
                "Staff" => Some(ItemType::Staff),
                _ => Some(ItemType::Sword),
            },
            Some(EquippedWeapon::Ranged(_)) => Some(ItemType::Bow),
            None => return ActionResult::Invalid, // Nothing to drop
        }
    };

    let item_type = weapon_item.unwrap();

    // Remove weapon from equipment
    if let Ok(mut equipment) = world.get::<&mut Equipment>(entity) {
        equipment.weapon = None;
    }

    // Spawn on ground
    crate::systems::inventory::spawn_ground_item(world, x, y, item_type);

    // Emit event
    events.push(GameEvent::ItemDropped {
        entity,
        item: item_type,
        position: (x, y),
    });

    ActionResult::Completed
}

// =============================================================================
// CLASS ABILITY ACTIONS
// =============================================================================

/// Apply cleave attack - attacks all enemies within radius 2 (24 tiles)
pub fn apply_cleave(
    world: &mut World,
    attacker: Entity,
    events: &mut EventQueue,
) -> ActionResult {
    use rand::Rng;

    // Get attacker position
    let attacker_pos = match queries::get_entity_position(world, attacker) {
        Some(p) => p,
        None => return ActionResult::Invalid,
    };

    // Emit cleave event for VFX
    events.push(GameEvent::CleavePerformed {
        center: attacker_pos,
    });

    // Get attacker stats for damage calculation
    let strength = world
        .get::<&Stats>(attacker)
        .map(|s| s.strength)
        .unwrap_or(10);
    let weapon_damage = world
        .get::<&Equipment>(attacker)
        .ok()
        .and_then(|e| e.get_melee().map(|w| w.base_damage + w.damage_bonus))
        .unwrap_or(UNARMED_DAMAGE);
    let base_damage = weapon_damage + (strength - 10) / 2;

    // Check for status effects on attacker
    let has_strength_boost = queries::has_status_effect(world, attacker, EffectType::Strengthened);

    // Collect all attackable entities within radius 2 (5x5 area minus center = 24 tiles)
    let mut targets: Vec<(Entity, i32, i32)> = Vec::new();
    for dx in -2..=2 {
        for dy in -2..=2 {
            if dx == 0 && dy == 0 {
                continue; // Skip self
            }
            let tx = attacker_pos.0 + dx;
            let ty = attacker_pos.1 + dy;
            if let Some(target) = queries::get_attackable_at(world, tx, ty, Some(attacker)) {
                targets.push((target, tx, ty));
            }
        }
    }

    // Apply damage to each target
    let mut rng = rand::thread_rng();
    for (target, tx, ty) in &targets {
        // Check for protection on target
        let has_protection = queries::has_status_effect(world, *target, EffectType::Protected)
            || queries::has_status_effect(world, *target, EffectType::Barkskin);

        // Apply damage variance and crit
        let damage_mult = rng.gen_range(COMBAT_DAMAGE_MIN_MULT..=COMBAT_DAMAGE_MAX_MULT);
        let is_crit = rng.gen::<f32>() < COMBAT_CRIT_CHANCE;
        let mut damage = (base_damage as f32 * damage_mult) as i32;
        if is_crit {
            damage = (damage as f32 * COMBAT_CRIT_MULTIPLIER) as i32;
        }

        // Apply Strengthened bonus
        if has_strength_boost {
            damage = (damage as f32 * STRENGTH_DAMAGE_MULTIPLIER) as i32;
        }

        // Apply Protected reduction
        if has_protection {
            damage = (damage as f32 * PROTECTION_DAMAGE_REDUCTION) as i32;
        }

        damage = damage.max(1);

        // Apply damage to target
        if let Ok(mut health) = world.get::<&mut Health>(*target) {
            health.current -= damage;
        }

        // Interrupt life drain if target was channeling
        interrupt_life_drain_on_damage(world, *target, events);

        // Emit attack event for VFX
        events.push(GameEvent::AttackHit {
            attacker,
            target: *target,
            target_pos: (*tx as f32 + 0.5, *ty as f32 + 0.5),
            damage,
        });
    }

    // Add a small lunge animation (to center, since we're hitting all around)
    // Just do a small pulse effect by lunging to self
    let _ = world.insert_one(
        attacker,
        LungeAnimation::new(attacker_pos.0 as f32 + 0.5, attacker_pos.1 as f32 + 0.5),
    );

    ActionResult::Completed
}

/// Apply sprint activation - applies speed boost effect to entity
pub fn apply_activate_sprint(
    world: &mut World,
    entity: Entity,
) -> ActionResult {
    use crate::constants::SPRINT_DURATION;
    use crate::systems::effects::add_effect_to_entity;

    add_effect_to_entity(world, entity, EffectType::SpeedBoost, SPRINT_DURATION);

    ActionResult::Completed
}

/// Apply barkskin activation - applies damage reduction effect to entity
pub fn apply_activate_barkskin(
    world: &mut World,
    entity: Entity,
    events: &mut EventQueue,
) -> ActionResult {
    use crate::constants::BARKSKIN_DURATION;
    use crate::systems::effects::add_effect_to_entity;
    use crate::components::SecondaryAbility;

    add_effect_to_entity(world, entity, EffectType::Barkskin, BARKSKIN_DURATION);

    // Start the ability cooldown
    if let Ok(mut ability) = world.get::<&mut SecondaryAbility>(entity) {
        ability.start_cooldown();
    }

    // Emit event for VFX
    events.push(GameEvent::BarkskinActivated { entity });

    ActionResult::Completed
}

/// Start life drain channeling (Necromancer ability)
pub fn apply_start_life_drain(
    world: &mut World,
    caster: Entity,
    target: Entity,
    events: &mut EventQueue,
) -> ActionResult {
    // Verify target still exists and is alive
    let target_alive = world.get::<&Health>(target).map(|h| h.current > 0).unwrap_or(false);
    if !target_alive {
        return ActionResult::Invalid;
    }

    // Check range
    let caster_pos = queries::get_entity_position(world, caster);
    let target_pos = queries::get_entity_position(world, target);
    match (caster_pos, target_pos) {
        (Some((cx, cy)), Some((tx, ty))) => {
            let dist = (cx - tx).abs().max((cy - ty).abs());
            if dist > LIFE_DRAIN_RANGE {
                return ActionResult::Invalid;
            }
        }
        _ => return ActionResult::Invalid,
    }

    // Add LifeDrainInProgress component to start channeling
    let _ = world.insert_one(caster, LifeDrainInProgress {
        target,
        tick_timer: 0.0, // Tick immediately on first wait
    });

    // Emit event to show VFX
    events.push(GameEvent::LifeDrainStarted { caster, target });

    ActionResult::Completed
}

/// Apply fear activation - causes nearby enemies to flee
pub fn apply_activate_fear(
    world: &mut World,
    entity: Entity,
    events: &mut EventQueue,
) -> ActionResult {
    use crate::constants::FEAR_ABILITY_DURATION;

    // Get entity position
    let entity_pos = crate::queries::get_entity_position(world, entity);
    if entity_pos.is_none() {
        return ActionResult::Invalid;
    }
    let (ex, ey) = entity_pos.unwrap();

    // Apply fear to all visible enemies in range
    let targets: Vec<hecs::Entity> = world
        .query::<(&Position, &Health)>()
        .without::<&Player>()
        .without::<&TamedBy>()
        .iter()
        .filter(|(_, (pos, health))| {
            let dx = (pos.x - ex).abs();
            let dy = (pos.y - ey).abs();
            dx <= crate::constants::FEAR_ABILITY_RADIUS
                && dy <= crate::constants::FEAR_ABILITY_RADIUS
                && health.current > 0
        })
        .map(|(e, _)| e)
        .collect();

    // Apply fear effect to each target
    for target in targets {
        crate::systems::effects::add_effect_to_entity(world, target, EffectType::Feared, FEAR_ABILITY_DURATION);
    }

    // Start the ability cooldown
    if let Ok(mut ability) = world.get::<&mut SecondaryAbility>(entity) {
        ability.start_cooldown();
    }

    // Emit event for VFX
    events.push(GameEvent::FearActivated {
        entity,
        position: (ex, ey),
    });

    ActionResult::Completed
}

/// Apply wait action - handles taming and life drain progress if applicable
pub fn apply_wait(
    world: &mut World,
    entity: Entity,
    events: &mut EventQueue,
) -> ActionResult {
    // Check if entity is taming something
    let taming_info = world.get::<&TamingInProgress>(entity)
        .ok()
        .map(|t| (t.target, t.progress, t.required));

    if let Some((target, progress, required)) = taming_info {
        // Check if target still exists and is in range
        let entity_pos = world.get::<&Position>(entity).ok().map(|p| (p.x, p.y));
        let target_pos = world.get::<&Position>(target).ok().map(|p| (p.x, p.y));

        match (entity_pos, target_pos) {
            (Some((ex, ey)), Some((tx, ty))) => {
                let dist = (ex - tx).abs().max((ey - ty).abs());
                if dist <= TAME_RANGE {
                    // Add progress (wait duration is 0.5s)
                    let new_progress = progress + ACTION_WAIT_DURATION;

                    if new_progress >= required {
                        // Taming complete!
                        complete_taming(world, entity, target, events);
                    } else {
                        // Update progress
                        if let Ok(mut taming) = world.get::<&mut TamingInProgress>(entity) {
                            taming.progress = new_progress;
                        }
                        events.push(GameEvent::TamingProgress {
                            tamer: entity,
                            target,
                            progress: new_progress,
                            required,
                        });
                    }
                } else {
                    // Too far away - taming failed
                    let _ = world.remove_one::<TamingInProgress>(entity);
                    events.push(GameEvent::TamingFailed { tamer: entity, target });
                }
            }
            _ => {
                // Target no longer exists - remove taming state
                let _ = world.remove_one::<TamingInProgress>(entity);
            }
        }
    }

    // Check if entity is channeling life drain
    let drain_info = world.get::<&LifeDrainInProgress>(entity)
        .ok()
        .map(|d| (d.target, d.tick_timer));

    if let Some((target, tick_timer)) = drain_info {
        tick_life_drain(world, entity, target, tick_timer, events);
    }

    ActionResult::Completed
}

/// Tick life drain channeling - applies damage and healing
fn tick_life_drain(
    world: &mut World,
    caster: Entity,
    target: Entity,
    tick_timer: f32,
    events: &mut EventQueue,
) {
    // Check if target still exists and is alive
    let target_alive = world.get::<&Health>(target).map(|h| h.current > 0).unwrap_or(false);
    if !target_alive {
        // Target died - end drain and start cooldown
        end_life_drain(world, caster, target, events, false);
        return;
    }

    // Check range and get positions for VFX
    let caster_pos = queries::get_entity_position(world, caster);
    let target_pos = queries::get_entity_position(world, target);
    let (cx, cy, tx, ty) = match (caster_pos, target_pos) {
        (Some((cx, cy)), Some((tx, ty))) => {
            let dist = (cx - tx).abs().max((cy - ty).abs());
            if dist > LIFE_DRAIN_RANGE {
                // Out of range - end drain and start cooldown
                end_life_drain(world, caster, target, events, false);
                return;
            }
            (cx, cy, tx, ty)
        }
        _ => {
            // Invalid positions - end drain
            end_life_drain(world, caster, target, events, false);
            return;
        }
    };

    // Update tick timer
    let new_timer = tick_timer + ACTION_WAIT_DURATION;
    if new_timer >= LIFE_DRAIN_TICK_INTERVAL {
        // Time to tick! Apply damage and healing
        let intelligence = world
            .get::<&Stats>(caster)
            .map(|s| s.intelligence)
            .unwrap_or(10);

        // Calculate damage: base + INT bonus
        let damage = LIFE_DRAIN_DAMAGE_PER_TICK + (intelligence - 10) / 3;

        // Apply damage to target
        let target_died = if let Ok(mut health) = world.get::<&mut Health>(target) {
            health.current -= damage;
            health.current <= 0
        } else {
            false
        };

        // Heal caster (percentage of damage dealt)
        let heal_amount = (damage as f32 * LIFE_DRAIN_HEAL_PERCENT) as i32;
        if let Ok(mut health) = world.get::<&mut Health>(caster) {
            health.current = (health.current + heal_amount).min(health.max);
        }

        // Emit tick event with positions for damage numbers
        events.push(GameEvent::LifeDrainTick {
            caster,
            target,
            caster_pos: (cx as f32 + 0.5, cy as f32 + 0.5),
            target_pos: (tx as f32 + 0.5, ty as f32 + 0.5),
            damage,
            healed: heal_amount,
        });

        // Reset timer (or end if target died)
        if target_died {
            end_life_drain(world, caster, target, events, false);
        } else {
            // Reset tick timer
            if let Ok(mut drain) = world.get::<&mut LifeDrainInProgress>(caster) {
                drain.tick_timer = 0.0;
            }
        }
    } else {
        // Just update timer
        if let Ok(mut drain) = world.get::<&mut LifeDrainInProgress>(caster) {
            drain.tick_timer = new_timer;
        }
    }
}

/// Interrupt life drain if the caster takes damage
/// Call this when an entity takes damage to check if they should stop channeling
pub fn interrupt_life_drain_on_damage(
    world: &mut World,
    entity: Entity,
    events: &mut EventQueue,
) {
    // Check if this entity is channeling life drain (extract target to avoid borrow conflict)
    let target = world.get::<&LifeDrainInProgress>(entity).ok().map(|d| d.target);
    if let Some(target) = target {
        end_life_drain(world, entity, target, events, true);
    }
}

/// End life drain channeling and start cooldown
fn end_life_drain(
    world: &mut World,
    caster: Entity,
    target: Entity,
    events: &mut EventQueue,
    was_interrupted: bool,
) {
    // Remove the channeling component
    let _ = world.remove_one::<LifeDrainInProgress>(caster);

    // Start cooldown
    if let Ok(mut ability) = world.get::<&mut ClassAbility>(caster) {
        ability.start_cooldown();
    }

    // Emit appropriate event
    if was_interrupted {
        events.push(GameEvent::LifeDrainInterrupted { caster, target });
    } else {
        events.push(GameEvent::LifeDrainEnded { caster, target });
    }
}

/// Complete taming - convert enemy to companion
fn complete_taming(
    world: &mut World,
    tamer: Entity,
    target: Entity,
    events: &mut EventQueue,
) {
    use crate::components::{CompanionAI, TamedBy};

    // Remove hostile AI (but keep Attackable so enemies can still attack the companion)
    let _ = world.remove_one::<ChaseAI>(target);

    // Remove BlocksMovement so player can walk through their companion
    let _ = world.remove_one::<BlocksMovement>(target);

    // Add companion components
    let _ = world.insert_one(target, TamedBy { owner: tamer });
    let _ = world.insert_one(target, CompanionAI {
        owner: tamer,
        follow_distance: 2,
        last_attacker: None,
    });

    // Remove taming state from player
    let _ = world.remove_one::<TamingInProgress>(tamer);

    // Emit event
    events.push(GameEvent::TamingCompleted { tamer, target });
}

/// Start taming an animal (Druid ability)
pub fn apply_start_taming(
    world: &mut World,
    tamer: Entity,
    target: Entity,
    events: &mut EventQueue,
) -> ActionResult {
    // Verify target still exists and is tameable
    if world.get::<&crate::components::Tameable>(target).is_err() {
        return ActionResult::Invalid;
    }

    // Add TamingInProgress component to the tamer
    let _ = world.insert_one(tamer, TamingInProgress {
        target,
        progress: 0.0,
        required: TAME_DURATION,
    });

    // Start the ability cooldown
    if let Ok(mut ability) = world.get::<&mut ClassAbility>(tamer) {
        ability.start_cooldown();
    }

    // Emit event to show message and VFX
    events.push(GameEvent::TamingStarted { tamer, target });

    ActionResult::Completed
}

// =============================================================================
// TRAP ACTIONS
// =============================================================================

/// Place a fire trap at the target location
pub fn apply_place_fire_trap(
    world: &mut World,
    placer: Entity,
    target_x: i32,
    target_y: i32,
    events: &mut EventQueue,
) -> ActionResult {
    use crate::components::PlacedFireTrap;
    use crate::constants::FIRE_TRAP_BURST_DAMAGE;

    // Spawn the fire trap entity with pressure plate sprite
    // Fire animation is rendered as overlay in rendering.rs (like burning entities)
    let pos = Position::new(target_x, target_y);
    let trap = world.spawn((
        pos,
        VisualPosition::from_position(&pos),
        // Base sprite (pressure plate)
        Sprite::from_ref(tile_ids::PRESSURE_PLATE),
        // Trap data - tracks owner and damage
        PlacedFireTrap {
            owner: placer,
            burst_damage: FIRE_TRAP_BURST_DAMAGE,
        },
    ));

    events.push(GameEvent::FireTrapPlaced {
        trap,
        placer,
        position: (target_x, target_y),
    });

    ActionResult::Completed
}

// =============================================================================
// RANGER ABILITIES
// =============================================================================

/// Ranger ability: Disengage - leap away from the nearest enemy
pub fn apply_disengage(
    world: &mut World,
    grid: &Grid,
    entity: Entity,
    _events: &mut EventQueue,
) -> ActionResult {
    use crate::fov::FOV;
    use crate::constants::{DISENGAGE_DISTANCE, FOV_RADIUS};

    // Get entity position
    let pos = match world.get::<&Position>(entity) {
        Ok(p) => (p.x, p.y),
        Err(_) => return ActionResult::Blocked,
    };

    // Find visible enemies
    let visible_tiles = FOV::calculate(grid, pos.0, pos.1, FOV_RADIUS, None::<fn(i32, i32) -> bool>);
    let visible_set: std::collections::HashSet<(i32, i32)> = visible_tiles.into_iter().collect();

    // Find nearest enemy
    let mut nearest_enemy: Option<(i32, i32, i32)> = None; // (x, y, distance_squared)
    for (_, (enemy_pos, _)) in world.query::<(&Position, &Attackable)>().iter() {
        if (enemy_pos.x, enemy_pos.y) == pos {
            continue; // Skip self
        }
        if !visible_set.contains(&(enemy_pos.x, enemy_pos.y)) {
            continue;
        }
        let dx = enemy_pos.x - pos.0;
        let dy = enemy_pos.y - pos.1;
        let dist_sq = dx * dx + dy * dy;
        if nearest_enemy.is_none() || dist_sq < nearest_enemy.unwrap().2 {
            nearest_enemy = Some((enemy_pos.x, enemy_pos.y, dist_sq));
        }
    }

    // If no enemy found, just stay in place
    let (enemy_x, enemy_y) = match nearest_enemy {
        Some((x, y, _)) => (x, y),
        None => return ActionResult::Completed, // No enemies, ability still goes on cooldown
    };

    // Calculate direction away from enemy
    let dx = pos.0 - enemy_x;
    let dy = pos.1 - enemy_y;
    let len = ((dx * dx + dy * dy) as f32).sqrt().max(0.001);
    let dir_x = (dx as f32 / len).round() as i32;
    let dir_y = (dy as f32 / len).round() as i32;

    // Try to find a valid landing spot
    let try_offsets = [
        (dir_x, dir_y),
        (dir_y, -dir_x),  // Perpendicular
        (-dir_y, dir_x),  // Other perpendicular
    ];

    for (off_x, off_y) in try_offsets {
        if off_x == 0 && off_y == 0 {
            continue;
        }
        let target_x = pos.0 + off_x * DISENGAGE_DISTANCE;
        let target_y = pos.1 + off_y * DISENGAGE_DISTANCE;

        // Check if target is walkable and unoccupied
        if grid.get(target_x, target_y).map(|t| t.tile_type.is_walkable()).unwrap_or(false) {
            let blocked = world.query::<(&Position, &BlocksMovement)>()
                .iter()
                .any(|(_, (p, _))| p.x == target_x && p.y == target_y);

            if !blocked {
                // Teleport to target
                if let Ok(mut pos) = world.get::<&mut Position>(entity) {
                    pos.x = target_x;
                    pos.y = target_y;
                }
                if let Ok(mut vpos) = world.get::<&mut VisualPosition>(entity) {
                    vpos.x = target_x as f32;
                    vpos.y = target_y as f32;
                }
                return ActionResult::Completed;
            }
        }
    }

    // All spots blocked, ability still goes on cooldown but no movement
    ActionResult::Completed
}

/// Ranger ability: Tumble - roll to target position with brief invulnerability
pub fn apply_tumble(
    world: &mut World,
    grid: &Grid,
    entity: Entity,
    target_x: i32,
    target_y: i32,
    _events: &mut EventQueue,
) -> ActionResult {
    use crate::constants::TUMBLE_INVULN_DURATION;

    // Get entity position
    let _pos = match world.get::<&Position>(entity) {
        Ok(p) => (p.x, p.y),
        Err(_) => return ActionResult::Blocked,
    };

    // Check target is walkable terrain (ignore blocking entities - we roll through)
    if !grid.get(target_x, target_y).map(|t| t.tile_type.is_walkable()).unwrap_or(false) {
        return ActionResult::Blocked;
    }

    // Teleport to target
    if let Ok(mut p) = world.get::<&mut Position>(entity) {
        p.x = target_x;
        p.y = target_y;
    }
    if let Ok(mut vpos) = world.get::<&mut VisualPosition>(entity) {
        vpos.x = target_x as f32;
        vpos.y = target_y as f32;
    }

    // Apply invulnerability effect
    effects::add_effect_to_entity(world, entity, EffectType::Invulnerable, TUMBLE_INVULN_DURATION);

    ActionResult::Completed
}

/// Ranger ability: Place a snare trap that roots enemies
pub fn apply_place_snare_trap(
    world: &mut World,
    placer: Entity,
    target_x: i32,
    target_y: i32,
    events: &mut EventQueue,
) -> ActionResult {
    use crate::constants::SNARE_TRAP_ROOT_DURATION;

    // Spawn the trap entity
    let pos = Position::new(target_x, target_y);
    let trap = world.spawn((
        pos,
        VisualPosition::from_position(&pos),
        Sprite::from_ref(tile_ids::PRESSURE_PLATE),
        PlacedTrap {
            owner: placer,
            trap_type: TrapType::Snare { root_duration: SNARE_TRAP_ROOT_DURATION },
        },
    ));

    // TODO: Add SnareTrapPlaced event when we create it
    let _ = trap;

    ActionResult::Completed
}

/// Ranger ability: Shoot a crippling arrow that slows the target
pub fn apply_shoot_crippling_shot(
    world: &mut World,
    grid: &Grid,
    shooter: Entity,
    target_x: i32,
    target_y: i32,
    events: &mut EventQueue,
    current_time: f32,
) -> ActionResult {
    use crate::constants::CRIPPLING_SHOT_SLOW_DURATION;

    // Get shooter position and stats
    let (start_x, start_y) = match world.get::<&Position>(shooter) {
        Ok(pos) => (pos.x, pos.y),
        Err(_) => return ActionResult::Blocked,
    };

    // Check for and consume arrow from inventory
    if let Ok(mut inventory) = world.get::<&mut Inventory>(shooter) {
        if let Some(idx) = inventory.items.iter().position(|i| *i == ItemType::Arrow) {
            inventory.items.remove(idx);
        } else {
            // No arrows! Can't shoot
            return ActionResult::Blocked;
        }
    } else {
        return ActionResult::Blocked;
    }

    // Get bow stats
    let (base_damage, arrow_speed) = if let Ok(equip) = world.get::<&Equipment>(shooter) {
        match &equip.weapon {
            Some(EquippedWeapon::Ranged(bow)) => (bow.base_damage, bow.arrow_speed),
            _ => return ActionResult::Blocked, // No bow equipped
        }
    } else {
        return ActionResult::Blocked;
    };

    // Calculate damage with stats (same as regular bow shot)
    let agility = world.get::<&Stats>(shooter).map(|s| s.agility).unwrap_or(10);
    let damage = base_damage + (agility - 10) / 2;

    // Calculate arrow path using Bresenham
    let path = calculate_arrow_path(start_x, start_y, target_x, target_y, arrow_speed, grid);

    // Calculate direction for sprite rotation
    let direction = {
        let dx = (target_x - start_x) as f32;
        let dy = (target_y - start_y) as f32;
        let len = (dx * dx + dy * dy).sqrt().max(0.001);
        (dx / len, dy / len)
    };

    // Spawn arrow projectile with on_hit_effect
    let pos = Position::new(start_x, start_y);
    let arrow = world.spawn((
        pos,
        VisualPosition::from_position(&pos),
        Sprite::from_ref(tile_ids::ARROW),
        Projectile {
            source: shooter,
            damage,
            path,
            path_index: 0,
            direction,
            spawn_time: current_time,
            finished: None,
            potion_type: None,
            on_hit_effect: Some((EffectType::Slowed, CRIPPLING_SHOT_SLOW_DURATION)),
            hit_enemy: false,
        },
        ProjectileMarker,
    ));

    events.push(GameEvent::ProjectileSpawned {
        projectile: arrow,
        source: shooter,
    });

    ActionResult::Completed
}
