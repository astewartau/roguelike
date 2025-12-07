use crate::actions::{Action, ActionResult};
use crate::components::{Actor, AIState, Attackable, BlocksMovement, BlocksVision, ChaseAI, Container, Door, Equipment, Experience, Health, HitFlash, Inventory, LungeAnimation, Position, Sprite, Stats, VisualPosition, Weapon};
use crate::constants::*;
use crate::events::EventQueue;
use crate::fov::FOV;
use crate::grid::Grid;
use crate::pathfinding;
use crate::tile::tile_ids;
use hecs::World;
use rand::Rng;
use std::collections::HashSet;

/// Result of a player move attempt
pub enum MoveResult {
    Moved,
    OpenedChest(hecs::Entity),
    Attacked(hecs::Entity),
    Blocked,
}

/// Smoothly interpolate visual positions toward logical positions
pub fn visual_lerp(world: &mut World, dt: f32) {
    let lerp_speed = dt * VISUAL_LERP_SPEED;
    for (_id, (pos, vis_pos, lunge)) in world.query_mut::<(&Position, &mut VisualPosition, Option<&LungeAnimation>)>() {
        // If lunging, offset visual position toward target
        if let Some(lunge) = lunge {
            let base_x = pos.x as f32;
            let base_y = pos.y as f32;

            // Calculate lunge offset (move 0.5 tiles toward target at peak)
            // Use ease-out for punch, ease-in for return
            let lunge_amount = if lunge.returning {
                let t = lunge.progress;
                t * t  // Ease-in (slow start, fast end)
            } else {
                let t = lunge.progress;
                1.0 - (1.0 - t) * (1.0 - t)  // Ease-out (fast start, slow end)
            };
            let lunge_distance = LUNGE_DISTANCE * if lunge.returning { 1.0 - lunge_amount } else { lunge_amount };

            let dx = lunge.target_x - base_x;
            let dy = lunge.target_y - base_y;
            let dist = (dx * dx + dy * dy).sqrt().max(0.001);
            let dir_x = dx / dist;
            let dir_y = dy / dist;

            vis_pos.x = base_x + dir_x * lunge_distance;
            vis_pos.y = base_y + dir_y * lunge_distance;
        } else {
            // Normal interpolation
            let tx = pos.x as f32;
            let ty = pos.y as f32;
            let dx = tx - vis_pos.x;
            let dy = ty - vis_pos.y;
            let dist = (dx * dx + dy * dy).sqrt();
            if dist < 0.01 {
                vis_pos.x = tx;
                vis_pos.y = ty;
            } else {
                let t = lerp_speed.min(1.0);
                vis_pos.x += dx * t;
                vis_pos.y += dy * t;
            }
        }
    }
}

/// Update lunge animations
pub fn update_lunge_animations(world: &mut World, dt: f32) {
    let lunge_speed = LUNGE_ANIMATION_SPEED;
    let mut to_remove = Vec::new();

    for (id, lunge) in world.query_mut::<&mut LungeAnimation>() {
        lunge.progress += dt * lunge_speed;

        if lunge.progress >= 1.0 {
            if lunge.returning {
                // Animation complete
                to_remove.push(id);
            } else {
                // Start return
                lunge.returning = true;
                lunge.progress = 0.0;
            }
        }
    }

    for id in to_remove {
        let _ = world.remove_one::<LungeAnimation>(id);
    }
}

/// Update hit flash effects
pub fn update_hit_flashes(world: &mut World, dt: f32) {
    let mut to_remove = Vec::new();

    for (id, flash) in world.query_mut::<&mut HitFlash>() {
        flash.timer -= dt;
        if flash.timer <= 0.0 {
            to_remove.push(id);
        }
    }

    for id in to_remove {
        let _ = world.remove_one::<HitFlash>(id);
    }
}

// === Experience System Functions ===

/// XP needed to reach the next level
pub fn xp_for_level(level: u32) -> u32 {
    level * XP_PER_LEVEL_MULTIPLIER
}

/// Calculate XP progress toward next level (0.0 to 1.0)
pub fn xp_progress(exp: &Experience) -> f32 {
    exp.current as f32 / xp_for_level(exp.level) as f32
}

/// Add XP to an experience component, handling level ups
pub fn grant_xp(exp: &mut Experience, amount: u32) -> bool {
    exp.current += amount;
    let mut leveled_up = false;
    while exp.current >= xp_for_level(exp.level) {
        exp.current -= xp_for_level(exp.level);
        exp.level += 1;
        leveled_up = true;
    }
    leveled_up
}

// === Stats System Functions ===

/// Calculate total stat points
pub fn stats_total(stats: &Stats) -> i32 {
    stats.strength + stats.intelligence + stats.agility
}

/// Calculate XP value of an entity based on its stats
pub fn calculate_xp_value(stats: Option<&Stats>) -> u32 {
    stats.map(|s| stats_total(s) as u32).unwrap_or(5)
}

// === Item System Functions ===

use crate::components::ItemType;

/// Get the display name of an item
pub fn item_name(item: ItemType) -> &'static str {
    match item {
        ItemType::HealthPotion => "Health Potion",
    }
}

/// Get the weight of an item in kg
pub fn item_weight(item: ItemType) -> f32 {
    match item {
        ItemType::HealthPotion => HEALTH_POTION_WEIGHT,
    }
}

/// Get the heal amount for healing items (0 for non-healing items)
pub fn item_heal_amount(item: ItemType) -> i32 {
    match item {
        ItemType::HealthPotion => HEALTH_POTION_HEAL,
    }
}

/// Turn dead entities into bones (health <= 0) and grant XP to player
pub fn remove_dead_entities(world: &mut World, player_entity: hecs::Entity, rng: &mut impl Rng, events: &mut EventQueue) {
    let mut to_convert = Vec::new();

    for (id, (pos, health, stats)) in world.query::<(&Position, &Health, Option<&Stats>)>().iter() {
        if health.current <= 0 {
            let xp = calculate_xp_value(stats);
            to_convert.push((id, (pos.x as f32 + 0.5, pos.y as f32 + 0.5), xp));
        }
    }

    // Grant XP to player
    let total_xp: u32 = to_convert.iter().map(|(_, _, xp)| xp).sum();
    if total_xp > 0 {
        if let Ok(mut exp) = world.get::<&mut Experience>(player_entity) {
            let leveled_up = grant_xp(&mut exp, total_xp);
            if leveled_up {
                events.push(crate::events::GameEvent::LevelUp {
                    new_level: exp.level,
                });
            }
        }
    }

    for (id, position, _xp) in to_convert {
        // Emit death event
        events.push(crate::events::GameEvent::EntityDied {
            entity: id,
            position,
        });

        // Remove AI, Actor, Attackable, Stats components - turn into decoration
        let _ = world.remove_one::<Actor>(id);
        let _ = world.remove_one::<ChaseAI>(id);
        let _ = world.remove_one::<Attackable>(id);
        let _ = world.remove_one::<Health>(id);
        let _ = world.remove_one::<HitFlash>(id);
        let _ = world.remove_one::<BlocksMovement>(id);  // Bones are walkable
        let _ = world.remove_one::<Stats>(id);

        // Change sprite to bones
        if let Ok(mut sprite) = world.get::<&mut Sprite>(id) {
            sprite.tile_id = tile_ids::BONES;
        }

        // Add loot container with random gold
        let gold = rng.gen_range(ENEMY_GOLD_DROP_MIN..=ENEMY_GOLD_DROP_MAX);
        let _ = world.insert_one(id, Container::with_gold(vec![], gold));
    }
}

/// Update field of view from player position
pub fn update_fov(world: &World, grid: &mut Grid, player_entity: hecs::Entity, radius: i32) {
    let Ok(player_pos) = world.get::<&Position>(player_entity) else {
        return;
    };

    // Clear visibility
    for tile in &mut grid.tiles {
        tile.visible = false;
    }

    // Collect positions of entities that block vision
    let blocking_positions: HashSet<(i32, i32)> = world
        .query::<(&Position, &BlocksVision)>()
        .iter()
        .map(|(_, (pos, _))| (pos.x, pos.y))
        .collect();

    // Calculate and apply FOV
    let visible_tiles = FOV::calculate(
        grid,
        player_pos.x,
        player_pos.y,
        radius,
        Some(|x: i32, y: i32| blocking_positions.contains(&(x, y))),
    );
    for (x, y) in visible_tiles {
        if let Some(tile) = grid.get_mut(x, y) {
            tile.visible = true;
            tile.explored = true;
        }
    }
}

/// Visual effect flags (bitfield)
pub mod effects {
    pub const NONE: u32 = 0;
    pub const AGGRO_BORDER: u32 = 1 << 0;  // Red border when chasing
    pub const HIT_FLASH: u32 = 1 << 1;     // White flash when damaged
    // Future effects:
    // pub const POISONED: u32 = 1 << 2;
    // pub const BURNING: u32 = 1 << 3;
    // pub const FROZEN: u32 = 1 << 4;
    // pub const SHIELDED: u32 = 1 << 5;
}

/// Entity ready for rendering with all visual state
pub struct RenderEntity {
    pub x: f32,
    pub y: f32,
    pub sprite: Sprite,
    pub brightness: f32,
    pub effects: u32,  // Bitfield of active effects
}

/// Collect entities that should be rendered, with fog of war applied
pub fn collect_renderables(
    world: &World,
    grid: &Grid,
    player_entity: hecs::Entity,
) -> Vec<RenderEntity> {
    let mut entities_to_render: Vec<RenderEntity> = Vec::new();
    let mut player_render: Option<RenderEntity> = None;

    for (id, (pos, vis_pos, sprite)) in world.query::<(&Position, &VisualPosition, &Sprite)>().iter() {
        let (is_explored, is_visible) = grid.get(pos.x, pos.y)
            .map(|tile| (tile.explored, tile.visible))
            .unwrap_or((false, false));

        // Actors (enemies) are only visible in FOV, not in fog
        let is_actor = world.get::<&Actor>(id).is_ok();

        // Check if this entity is chasing (for red border)
        let is_chasing = world.get::<&ChaseAI>(id)
            .map(|chase| chase.state == AIState::Chasing)
            .unwrap_or(false);

        // Check if entity has hit flash
        let has_hit_flash = world.get::<&HitFlash>(id).is_ok();

        // Check if this is an open door (render darker)
        let is_open_door = world.get::<&Door>(id)
            .map(|door| door.is_open)
            .unwrap_or(false);

        // Build effect flags
        let mut entity_effects = effects::NONE;
        if is_chasing {
            entity_effects |= effects::AGGRO_BORDER;
        }
        if has_hit_flash {
            entity_effects |= effects::HIT_FLASH;
        }

        if id == player_entity {
            player_render = Some(RenderEntity {
                x: vis_pos.x,
                y: vis_pos.y,
                sprite: *sprite,
                brightness: 1.0,
                effects: effects::NONE,
            });
        } else if is_visible {
            // Open doors render at 50% brightness
            let brightness = if is_open_door { 0.5 } else { 1.0 };
            entities_to_render.push(RenderEntity {
                x: vis_pos.x,
                y: vis_pos.y,
                sprite: *sprite,
                brightness,
                effects: entity_effects,
            });
        } else if is_explored && !is_actor {
            // In fog but explored - only show non-actors (chests, items)
            // Open doors in fog render even darker
            let brightness = if is_open_door { 0.25 } else { 0.5 };
            entities_to_render.push(RenderEntity {
                x: vis_pos.x,
                y: vis_pos.y,
                sprite: *sprite,
                brightness,
                effects: effects::NONE,
            });
        }
    }

    // Player is always rendered last (on top)
    if let Some(player) = player_render {
        entities_to_render.push(player);
    }

    entities_to_render
}

/// Give all actors +1 energy per tick
pub fn tick_energy(world: &mut World) {
    for (_id, actor) in world.query_mut::<&mut Actor>() {
        actor.energy += 1;
    }
}

/// AI state machine: Idle (wander) -> Chasing (sees player) -> Investigating (lost sight)
/// Uses execution-time validation to prevent invalid moves like entity swaps.
pub fn ai_chase(world: &mut World, grid: &Grid, player_entity: hecs::Entity, rng: &mut impl Rng, events: &mut EventQueue) {
    // Get player position
    let player_pos = match world.get::<&Position>(player_entity) {
        Ok(p) => (p.x, p.y),
        Err(_) => return,
    };

    // Collect AI decisions first to avoid borrow conflicts
    // We store: (entity, dx, dy, new_state, last_known_pos)
    // Note: We now store deltas (dx, dy) not absolute positions
    let mut ai_decisions: Vec<(hecs::Entity, i32, i32, AIState, Option<(i32, i32)>)> = Vec::new();

    // Collect blocking positions for enemy FOV (used for planning, not execution)
    let vision_blocking: HashSet<(i32, i32)> = world
        .query::<(&Position, &BlocksVision)>()
        .iter()
        .map(|(_, (pos, _))| (pos.x, pos.y))
        .collect();

    // Collect movement blocking for pathfinding (soft check for planning)
    let movement_blocking: HashSet<(i32, i32)> = world
        .query::<(&Position, &BlocksMovement)>()
        .iter()
        .map(|(_, (pos, _))| (pos.x, pos.y))
        .collect();

    for (id, (pos, actor, chase)) in world.query::<(&Position, &Actor, &ChaseAI)>().iter() {
        if actor.energy < actor.speed {
            continue; // Not enough energy to act
        }

        // Calculate FOV from this enemy's position
        let visible_tiles: HashSet<(i32, i32)> = FOV::calculate(
            grid,
            pos.x,
            pos.y,
            chase.sight_radius,
            Some(|x: i32, y: i32| vision_blocking.contains(&(x, y))),
        )
        .into_iter()
        .collect();

        let can_see_player = visible_tiles.contains(&player_pos);

        // Determine new state and target
        let (new_state, target, last_known) = match chase.state {
            AIState::Idle => {
                if can_see_player {
                    (AIState::Chasing, Some(player_pos), Some(player_pos))
                } else {
                    (AIState::Idle, None, None)
                }
            }
            AIState::Chasing => {
                if can_see_player {
                    (AIState::Chasing, Some(player_pos), Some(player_pos))
                } else {
                    (AIState::Investigating, chase.last_known_pos, chase.last_known_pos)
                }
            }
            AIState::Investigating => {
                if can_see_player {
                    (AIState::Chasing, Some(player_pos), Some(player_pos))
                } else if let Some(last_pos) = chase.last_known_pos {
                    if pos.x == last_pos.0 && pos.y == last_pos.1 {
                        (AIState::Idle, None, None)
                    } else {
                        (AIState::Investigating, Some(last_pos), Some(last_pos))
                    }
                } else {
                    (AIState::Idle, None, None)
                }
            }
        };

        // Determine intended movement (as delta, not absolute)
        let (dx, dy) = if let Some((tx, ty)) = target {
            // Move toward target using A* pathfinding
            if let Some((nx, ny)) = pathfinding::next_step_toward(
                grid,
                (pos.x, pos.y),
                (tx, ty),
                &movement_blocking,
            ) {
                (nx - pos.x, ny - pos.y)
            } else {
                (0, 0) // Can't pathfind, stay in place
            }
        } else {
            // Idle: wander randomly
            let dirs = [(0, 1), (0, -1), (1, 0), (-1, 0)];
            let (dx, dy) = dirs[rng.gen_range(0..4)];
            let new_x = pos.x + dx;
            let new_y = pos.y + dy;

            // Soft check for planning - actual validation happens at execution
            if let Some(tile) = grid.get(new_x, new_y) {
                if tile.tile_type.is_walkable() {
                    (dx, dy)
                } else {
                    (0, 0)
                }
            } else {
                (0, 0)
            }
        };

        ai_decisions.push((id, dx, dy, new_state, last_known));
    }

    // Execute each action with real-time validation
    // This is the key fix: each action checks the CURRENT world state
    for (id, dx, dy, new_state, last_known) in ai_decisions {
        // Update chase state first
        if let Ok(mut chase) = world.get::<&mut ChaseAI>(id) {
            chase.state = new_state;
            chase.last_known_pos = last_known;
        }

        // Execute the move action with real-time validation
        // This will:
        // - Convert to attack if there's an attackable entity at target
        // - Block if something is in the way
        // - Move if the path is clear
        if dx != 0 || dy != 0 {
            let action = Action::Move { dx, dy };
            let _result = action.execute(world, grid, id, events);
            // Note: We don't need to handle the result specially here.
            // If blocked or attacked, that's fine - the action system handles it.
        } else {
            // No movement intended, just wait (spend energy)
            let action = Action::Wait;
            let _result = action.execute(world, grid, id, events);
        }
    }
}


/// Handle player movement, door interaction, chest interaction, and combat.
/// Uses the unified Action system for execution-time validation.
pub fn player_move(
    world: &mut World,
    grid: &Grid,
    player_entity: hecs::Entity,
    dx: i32,
    dy: i32,
    events: &mut EventQueue,
) -> MoveResult {
    let action = Action::Move { dx, dy };
    let result = action.execute(world, grid, player_entity, events);

    // Convert ActionResult to MoveResult for backwards compatibility
    match result {
        ActionResult::Moved => MoveResult::Moved,
        ActionResult::Attacked(entity) => MoveResult::Attacked(entity),
        ActionResult::OpenedDoor(_) => MoveResult::Moved,
        ActionResult::OpenedChest(entity) => MoveResult::OpenedChest(entity),
        ActionResult::Blocked | ActionResult::Invalid => MoveResult::Blocked,
    }
}

// === Combat System Functions ===

/// Calculate total damage for a weapon
pub fn weapon_damage(weapon: &Weapon) -> i32 {
    weapon.base_damage + weapon.damage_bonus
}

/// Get the damage an entity deals (from equipped weapon or unarmed)
pub fn get_attack_damage(world: &World, attacker: hecs::Entity) -> i32 {
    if let Ok(equipment) = world.get::<&Equipment>(attacker) {
        equipment.weapon.as_ref().map(|w| weapon_damage(w)).unwrap_or(UNARMED_DAMAGE)
    } else {
        UNARMED_DAMAGE
    }
}

/// Open a door - remove blocking components (sprite stays the same but renders darker)
pub fn open_door(world: &mut World, door_id: hecs::Entity) {
    // Mark as open
    if let Ok(mut door) = world.get::<&mut Door>(door_id) {
        door.is_open = true;
    }

    // Remove blocking components
    let _ = world.remove_one::<BlocksVision>(door_id);
    let _ = world.remove_one::<BlocksMovement>(door_id);
}

/// Open a chest - mark as open and change sprite (keeps blocking movement)
pub fn open_chest(world: &mut World, chest_id: hecs::Entity) {
    // Mark as open
    if let Ok(mut container) = world.get::<&mut Container>(chest_id) {
        container.is_open = true;
    }

    // Change sprite to open chest
    if let Ok(mut sprite) = world.get::<&mut Sprite>(chest_id) {
        sprite.tile_id = tile_ids::CHEST_OPEN;
    }
}

/// Take a single item from a container and add it to player inventory
pub fn take_item_from_container(
    world: &mut World,
    player_entity: hecs::Entity,
    container_entity: hecs::Entity,
    item_index: usize,
) -> bool {
    // Get the item from the container
    let item = {
        let Ok(mut container) = world.get::<&mut Container>(container_entity) else {
            return false;
        };
        if item_index >= container.items.len() {
            return false;
        }
        container.items.remove(item_index)
    };

    // Add to player inventory
    if let Ok(mut inventory) = world.get::<&mut Inventory>(player_entity) {
        inventory.current_weight_kg += item_weight(item);
        inventory.items.push(item);
        true
    } else {
        false
    }
}

/// Take all items and gold from a container and add them to player inventory
pub fn take_all_from_container(
    world: &mut World,
    player_entity: hecs::Entity,
    container_entity: hecs::Entity,
) {
    // Get all items and gold from the container
    let (items, gold) = {
        let Ok(mut container) = world.get::<&mut Container>(container_entity) else {
            return;
        };
        let items = std::mem::take(&mut container.items);
        let gold = container.gold;
        container.gold = 0;
        (items, gold)
    };

    // Add to player inventory
    if let Ok(mut inventory) = world.get::<&mut Inventory>(player_entity) {
        for item in items {
            inventory.current_weight_kg += item_weight(item);
            inventory.items.push(item);
        }
        inventory.gold += gold;
    }
}

/// Take gold from a container
pub fn take_gold_from_container(
    world: &mut World,
    player_entity: hecs::Entity,
    container_entity: hecs::Entity,
) {
    let gold = {
        let Ok(mut container) = world.get::<&mut Container>(container_entity) else {
            return;
        };
        let gold = container.gold;
        container.gold = 0;
        gold
    };

    if let Ok(mut inventory) = world.get::<&mut Inventory>(player_entity) {
        inventory.gold += gold;
    }
}

/// Find a lootable container at the player's position (for bones)
pub fn find_container_at_player(
    world: &World,
    player_entity: hecs::Entity,
) -> Option<hecs::Entity> {
    let player_pos = world.get::<&Position>(player_entity).ok()?;

    for (id, (pos, container)) in world.query::<(&Position, &Container)>().iter() {
        // Skip if it's a chest (has BlocksMovement) - those are handled by bumping
        if world.get::<&BlocksMovement>(id).is_ok() {
            continue;
        }
        if pos.x == player_pos.x && pos.y == player_pos.y && !container.is_empty() {
            return Some(id);
        }
    }
    None
}
