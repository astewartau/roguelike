//! Action effect implementations.
//!
//! This module contains the logic for applying action effects when actions complete.
//! These are called by the time system after an action's duration has elapsed.

use hecs::{Entity, World};

use crate::components::{
    Attackable, BlocksMovement, ChaseAI, Container, Door, EffectType, Equipment, EquippedWeapon,
    Health, Inventory, ItemType, LungeAnimation, Position, Projectile, ProjectileMarker,
    Sprite, Stats, StatusEffects, VisualPosition, Weapon, RangedWeapon,
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

    // Check for attackable entity at target - block movement instead of auto-attacking.
    if queries::get_attackable_at(world, target_x, target_y, Some(entity)).is_some() {
        return ActionResult::Blocked;
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

    ActionResult::Completed
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
    let has_protection = queries::has_status_effect(world, target, EffectType::Protected);

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

    damage = damage.max(1);

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

    events.push(GameEvent::DoorOpened { door, opener });

    ActionResult::Completed
}

/// Apply open chest effect
pub fn apply_open_chest(
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
    // Get shooter position
    let (start_x, start_y) = match queries::get_entity_position(world, shooter) {
        Some(p) => p,
        None => return ActionResult::Invalid,
    };

    // Can't shoot at yourself
    if start_x == target_x && start_y == target_y {
        return ActionResult::Blocked;
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
    let damage = base_damage + (agility - 10) / 2;

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
        },
        ProjectileMarker,
    ));

    events.push(GameEvent::ProjectileSpawned {
        projectile: arrow,
        source: shooter,
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
        ItemType::Bow => EquippedWeapon::Ranged(RangedWeapon::bow()),
        _ => return ActionResult::Invalid, // Not a weapon
    };

    // Check if there's currently an equipped weapon that needs to go to inventory
    let old_weapon_item = {
        if let Ok(equipment) = world.get::<&Equipment>(entity) {
            match &equipment.weapon {
                Some(EquippedWeapon::Melee(_)) => Some(ItemType::Sword),
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
            Some(EquippedWeapon::Melee(_)) => Some(ItemType::Sword),
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
            Some(EquippedWeapon::Melee(_)) => Some(ItemType::Sword),
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

/// Apply cleave attack - attacks all adjacent enemies (8 directions)
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

    // Collect all attackable entities in 8 adjacent tiles
    let mut targets: Vec<(Entity, i32, i32)> = Vec::new();
    for dx in -1..=1 {
        for dy in -1..=1 {
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
        let has_protection = queries::has_status_effect(world, *target, EffectType::Protected);

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
