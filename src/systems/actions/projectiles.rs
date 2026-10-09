//! Bow shots, thrown potions, and projectile path calculation.

use hecs::{Entity, World};

use crate::components::{
    ActiveAmmo, ChaseAI, EffectType, Equipment, Health, Inventory, ItemType, Player, Position,
    Projectile, ProjectileMarker, RangedCooldown, Sprite, SpriteTint, StatusEffects,
    VisualPosition,
};
use crate::constants::*;
use crate::events::{EventQueue, GameEvent};
use crate::grid::Grid;
use crate::pathfinding::{step_distance, BresenhamLineIter};
use crate::queries;
use crate::systems::effects;
use crate::tile::tile_ids;

use super::ActionResult;

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

    // If player, check for and consume ammo from inventory. The preferred ammo
    // (toggled via the inventory context menu) is loaded first; if the shooter
    // is out of that kind, fall back to whatever other ammo is carried.
    let mut is_fire_arrow = false;
    if is_player {
        let preferred = world
            .get::<&ActiveAmmo>(shooter)
            .map(|a| a.kind)
            .unwrap_or(ItemType::Arrow);
        let fallback = if preferred == ItemType::FireArrow {
            ItemType::Arrow
        } else {
            ItemType::FireArrow
        };

        if let Ok(mut inventory) = world.get::<&mut Inventory>(shooter) {
            let slot = inventory
                .items
                .iter()
                .position(|i| i.kind == preferred)
                .or_else(|| inventory.items.iter().position(|i| i.kind == fallback));
            match slot {
                Some(idx) => {
                    is_fire_arrow = inventory.items[idx].kind == ItemType::FireArrow;
                    inventory.items.remove(idx);
                }
                None => return ActionResult::Blocked, // No ammo! Can't shoot
            }
        } else {
            return ActionResult::Blocked;
        }
    }

    // Get bow stats (including any weapon affix damage from the equipped bow)
    let (base_damage, arrow_speed, affix_damage) = {
        let equipment = world.get::<&Equipment>(shooter).ok();
        let bow = equipment.as_ref().and_then(|e| e.get_bow());
        match bow {
            Some(bow) => (
                bow.base_damage,
                bow.arrow_speed,
                equipment.as_ref().map_or(0, |e| e.affix_damage_bonus()),
            ),
            None => return ActionResult::Blocked,
        }
    };

    // Calculate damage with stats (effective stats include stat affixes)
    let agility = queries::effective_stats(world, shooter).agility;
    let base_calc_damage = base_damage + affix_damage + (agility - 10) / 2;

    // Apply range band modifier (only for player)
    let damage = if is_player {
        let distance = (target_x - start_x).abs().max((target_y - start_y).abs());
        let range_mult = if (RANGE_OPTIMAL_MIN..=RANGE_OPTIMAL_MAX).contains(&distance) {
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

    // Conditional weapon affixes (LowHealthDamage)
    let damage = (damage as f32
        * crate::systems::combat::attacker_conditional_damage_mult(world, shooter))
        as i32;

    // Calculate line from shooter to target using Bresenham
    let path = calculate_arrow_path(start_x, start_y, target_x, target_y, arrow_speed, grid);

    if path.is_empty() {
        return ActionResult::Blocked;
    }

    // Firing a bow is loud — wake nearby sleeping enemies (doubled radius if
    // the shooter wears CursedLoud gear).
    let noise = crate::systems::combat::attack_noise_radius(world, shooter, RANGED_NOISE_RADIUS);
    crate::systems::ai::wake_enemies_in_radius(world, (start_x, start_y), noise);

    // Calculate normalized direction
    let dx = target_x - start_x;
    let dy = target_y - start_y;
    let len = ((dx * dx + dy * dy) as f32).sqrt();
    let direction = if len > 0.0 {
        (dx as f32 / len, dy as f32 / len)
    } else {
        (1.0, 0.0)
    };

    // Fire arrows apply Burning on hit and ignite the tile they land on.
    let on_hit_effect = if is_fire_arrow {
        Some((EffectType::Burning, BURNING_DURATION))
    } else {
        None
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
            on_hit_effect,
            hit_enemy: false,
            incendiary: is_fire_arrow,
        },
        ProjectileMarker,
    ));

    // Tint fire arrows red/orange so they read as burning in flight.
    if is_fire_arrow {
        let (r, g, b) = FIRE_ARROW_TINT;
        let _ = world.insert_one(arrow, SpriteTint { r, g, b });
    }

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
        ItemType::WaterFlaskFull => tile_ids::BOTTLE_WATER,
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
            incendiary: false,
        },
        ProjectileMarker,
    ));

    // Tint thrown water flasks light blue so they read as water in flight.
    if potion_type == ItemType::WaterFlaskFull {
        let (r, g, b) = WATER_FLASK_TINT;
        let _ = world.insert_one(potion_projectile, SpriteTint { r, g, b });
    }

    events.push(GameEvent::ProjectileSpawned {
        projectile: potion_projectile,
        source: thrower,
    });

    // Splash effect and status application happen when the projectile lands
    // (handled in projectile system)

    ActionResult::Completed
}

/// Apply a potion's splash effect to all entities in the splash radius.
///
/// `thrower` is the entity that threw the potion: a Confusion potion thrown
/// by the player gets its duration scaled by the player's effective INT.
pub fn apply_potion_splash(
    world: &mut World,
    grid: &Grid,
    thrower: Option<Entity>,
    potion_type: ItemType,
    center_x: i32,
    center_y: i32,
    events: &mut EventQueue,
) {
    // Water flasks don't buff anyone: the splash soaks creatures (Wet),
    // extinguishes fires (entities, grass, oil) and soaks grass tiles. Handled
    // wholesale by the fire system.
    if potion_type == ItemType::WaterFlaskFull {
        crate::systems::fire::splash_water(
            world, grid, center_x, center_y, WATER_SPLASH_RADIUS, events,
        );
        return;
    }
    // INT-scaled confusion when the player throws it (magic-adjacent trick);
    // splashes from other sources use the base duration.
    let confusion_duration = match thrower {
        Some(src) if world.get::<&crate::components::Player>(src).is_ok() => {
            CONFUSION_DURATION * queries::int_power(world, src)
        }
        _ => CONFUSION_DURATION,
    };
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
            if dx <= POTION_SPLASH_RADIUS && dy <= POTION_SPLASH_RADIUS
                && !affected.contains(&entity) {
                    affected.push(entity);
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
            // Confusion only affects enemies (entities with ChaseAI)
            ItemType::ConfusionPotion if world.get::<&ChaseAI>(entity).is_ok() => {
                effects::add_effect_to_entity(world, entity, EffectType::Confused, confusion_duration);
            }
            _ => {}
        }
    }
}
