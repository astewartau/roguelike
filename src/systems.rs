use crate::components::{Actor, AIState, Attackable, BlocksMovement, BlocksVision, ChaseAI, Container, Door, Equipment, Experience, Health, HitFlash, Inventory, LungeAnimation, Position, Sprite, Stats, VisualPosition, Weapon};
use crate::fov::FOV;
use crate::grid::Grid;
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
    let lerp_speed = dt * 25.0;
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
            let lunge_distance = 0.5 * if lunge.returning { 1.0 - lunge_amount } else { lunge_amount };

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
    let lunge_speed = 12.0;  // Fast, snappy attack
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
    level * 100
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
        ItemType::HealthPotion => 0.5,
    }
}

/// Get the heal amount for healing items (0 for non-healing items)
pub fn item_heal_amount(item: ItemType) -> i32 {
    match item {
        ItemType::HealthPotion => 50,
    }
}

/// Turn dead entities into bones (health <= 0) and grant XP to player
pub fn remove_dead_entities(world: &mut World, player_entity: hecs::Entity, rng: &mut impl Rng) {
    let mut to_convert = Vec::new();

    for (id, (health, stats)) in world.query::<(&Health, Option<&Stats>)>().iter() {
        if health.current <= 0 {
            let xp = calculate_xp_value(stats);
            to_convert.push((id, xp));
        }
    }

    // Grant XP to player
    let total_xp: u32 = to_convert.iter().map(|(_, xp)| xp).sum();
    if total_xp > 0 {
        if let Ok(mut exp) = world.get::<&mut Experience>(player_entity) {
            grant_xp(&mut exp, total_xp);
        }
    }

    for (id, _xp) in to_convert {
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

        // Add loot container with random gold (1-10 pieces)
        let gold = rng.gen_range(1..=10);
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

/// Collect entities that should be rendered, with fog of war applied
/// Returns (x, y, sprite, fog, has_border, has_hit_flash)
pub fn collect_renderables(
    world: &World,
    grid: &Grid,
    player_entity: hecs::Entity,
) -> Vec<(f32, f32, Sprite, f32, bool, bool)> {
    let mut entities_to_render: Vec<(f32, f32, Sprite, f32, bool, bool)> = Vec::new();
    let mut player_render: Option<(f32, f32, Sprite, f32, bool, bool)> = None;

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

        if id == player_entity {
            player_render = Some((vis_pos.x, vis_pos.y, *sprite, 1.0, false, false));
        } else if is_visible {
            // Open doors render at 50% brightness
            let brightness = if is_open_door { 0.5 } else { 1.0 };
            entities_to_render.push((vis_pos.x, vis_pos.y, *sprite, brightness, is_chasing, has_hit_flash));
        } else if is_explored && !is_actor {
            // In fog but explored - only show non-actors (chests, items)
            // Open doors in fog render even darker
            let brightness = if is_open_door { 0.25 } else { 0.5 };
            entities_to_render.push((vis_pos.x, vis_pos.y, *sprite, brightness, false, false));
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
pub fn ai_chase(world: &mut World, grid: &Grid, player_entity: hecs::Entity, rng: &mut impl Rng) {
    // Get player position
    let player_pos = match world.get::<&Position>(player_entity) {
        Ok(p) => (p.x, p.y),
        Err(_) => return,
    };

    // Collect decisions first to avoid borrow conflicts
    // (entity, new_x, new_y, new_state, last_known_pos)
    let mut ai_moves: Vec<(hecs::Entity, i32, i32, AIState, Option<(i32, i32)>)> = Vec::new();

    // Collect blocking positions for enemy FOV
    let vision_blocking: HashSet<(i32, i32)> = world
        .query::<(&Position, &BlocksVision)>()
        .iter()
        .map(|(_, (pos, _))| (pos.x, pos.y))
        .collect();

    // Collect movement blocking positions
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
                    // Spotted the player! Start chasing
                    (AIState::Chasing, Some(player_pos), Some(player_pos))
                } else {
                    // Keep wandering randomly
                    (AIState::Idle, None, None)
                }
            }
            AIState::Chasing => {
                if can_see_player {
                    // Still see the player, keep chasing
                    (AIState::Chasing, Some(player_pos), Some(player_pos))
                } else {
                    // Lost sight, switch to investigating last known position
                    (AIState::Investigating, chase.last_known_pos, chase.last_known_pos)
                }
            }
            AIState::Investigating => {
                if can_see_player {
                    // Found the player again!
                    (AIState::Chasing, Some(player_pos), Some(player_pos))
                } else if let Some(last_pos) = chase.last_known_pos {
                    // Check if we've reached the last known position
                    if pos.x == last_pos.0 && pos.y == last_pos.1 {
                        // Reached last known position, still don't see player -> go back to idle
                        (AIState::Idle, None, None)
                    } else {
                        // Keep investigating toward last known position
                        (AIState::Investigating, Some(last_pos), Some(last_pos))
                    }
                } else {
                    // No last known position, go idle
                    (AIState::Idle, None, None)
                }
            }
        };

        // Determine movement based on state
        let (new_x, new_y) = if let Some((tx, ty)) = target {
            // Move toward target (Chasing or Investigating)
            let dx = (tx - pos.x).signum();
            let dy = (ty - pos.y).signum();

            let mut nx = pos.x;
            let mut ny = pos.y;

            // Try horizontal first, then vertical
            if dx != 0 {
                let try_x = pos.x + dx;
                if let Some(tile) = grid.get(try_x, pos.y) {
                    if tile.tile_type.is_walkable() && !movement_blocking.contains(&(try_x, pos.y)) {
                        nx = try_x;
                    }
                }
            }
            if nx == pos.x && dy != 0 {
                let try_y = pos.y + dy;
                if let Some(tile) = grid.get(pos.x, try_y) {
                    if tile.tile_type.is_walkable() && !movement_blocking.contains(&(pos.x, try_y)) {
                        ny = try_y;
                    }
                }
            }
            (nx, ny)
        } else {
            // Idle: wander randomly
            let dirs = [(0, 1), (0, -1), (1, 0), (-1, 0)];
            let (dx, dy) = dirs[rng.gen_range(0..4)];
            let new_x = pos.x + dx;
            let new_y = pos.y + dy;

            if let Some(tile) = grid.get(new_x, new_y) {
                if tile.tile_type.is_walkable() && !movement_blocking.contains(&(new_x, new_y)) {
                    (new_x, new_y)
                } else {
                    (pos.x, pos.y)
                }
            } else {
                (pos.x, pos.y)
            }
        };

        ai_moves.push((id, new_x, new_y, new_state, last_known));
    }

    // Apply moves and state updates
    for (id, new_x, new_y, new_state, last_known) in ai_moves {
        // Update chase state
        if let Ok(mut chase) = world.get::<&mut ChaseAI>(id) {
            chase.state = new_state;
            chase.last_known_pos = last_known;
        }

        // Get current position to check if we're actually moving
        let current_pos = world.get::<&Position>(id).ok().map(|p| (p.x, p.y));

        if let Some((cx, cy)) = current_pos {
            // Spend energy (even if we couldn't move)
            if let Ok(mut actor) = world.get::<&mut Actor>(id) {
                actor.energy -= actor.speed;
            }

            if new_x != cx || new_y != cy {
                // Move
                if let Ok(mut pos) = world.get::<&mut Position>(id) {
                    pos.x = new_x;
                    pos.y = new_y;
                }
            }
        }
    }
}


/// Handle player movement, door interaction, chest interaction, and combat.
pub fn player_move(
    world: &mut World,
    player_entity: hecs::Entity,
    dx: i32,
    dy: i32,
) -> MoveResult {
    let current_pos = world.get::<&Position>(player_entity).ok().map(|p| *p);
    let Some(pos) = current_pos else { return MoveResult::Blocked };

    let target_x = pos.x + dx;
    let target_y = pos.y + dy;

    // Check for attackable entity at target position
    let mut enemy_to_attack: Option<hecs::Entity> = None;
    for (id, (enemy_pos, _attackable)) in world.query::<(&Position, &Attackable)>().iter() {
        if enemy_pos.x == target_x && enemy_pos.y == target_y {
            enemy_to_attack = Some(id);
            break;
        }
    }

    // If there's an enemy, attack it
    if let Some(enemy_id) = enemy_to_attack {
        perform_attack(world, player_entity, enemy_id, target_x as f32, target_y as f32);
        // Spend energy for attacking
        if let Ok(mut actor) = world.get::<&mut Actor>(player_entity) {
            actor.energy -= actor.speed;
        }
        return MoveResult::Attacked(enemy_id);
    }

    // Check for door at target position
    let mut door_to_open: Option<hecs::Entity> = None;
    for (id, (door_pos, door)) in world.query::<(&Position, &Door)>().iter() {
        if door_pos.x == target_x && door_pos.y == target_y && !door.is_open {
            door_to_open = Some(id);
            break;
        }
    }

    // If there's a closed door, open it instead of moving
    if let Some(door_id) = door_to_open {
        open_door(world, door_id);
        // Spend energy for opening the door
        if let Ok(mut actor) = world.get::<&mut Actor>(player_entity) {
            actor.energy -= actor.speed;
        }
        return MoveResult::Moved;
    }

    // Check for chest at target position (open or closed) - but not bones (walkable containers)
    let mut chest_to_interact: Option<(hecs::Entity, bool, bool)> = None;
    for (id, (chest_pos, container, _blocks)) in world.query::<(&Position, &Container, &BlocksMovement)>().iter() {
        if chest_pos.x == target_x && chest_pos.y == target_y {
            chest_to_interact = Some((id, container.is_open, !container.is_empty()));
            break;
        }
    }

    // If there's a chest with items (or closed), interact with it
    if let Some((chest_id, is_open, has_items)) = chest_to_interact {
        // Skip interaction if chest is open and empty
        if is_open && !has_items {
            return MoveResult::Blocked;
        }
        if !is_open {
            open_chest(world, chest_id);
        }
        // Spend energy for interacting with the chest
        if let Ok(mut actor) = world.get::<&mut Actor>(player_entity) {
            actor.energy -= actor.speed;
        }
        return MoveResult::OpenedChest(chest_id);
    }

    // Check for any entity blocking movement at target position
    for (id, (blocking_pos, _)) in world.query::<(&Position, &BlocksMovement)>().iter() {
        if id != player_entity && blocking_pos.x == target_x && blocking_pos.y == target_y {
            return MoveResult::Blocked;
        }
    }

    let target_pos = Position::new(target_x, target_y);

    // Spend energy and move
    if let Ok(mut actor) = world.get::<&mut Actor>(player_entity) {
        actor.energy -= actor.speed;
    }
    if let Ok(mut pos) = world.get::<&mut Position>(player_entity) {
        *pos = target_pos;
    }

    MoveResult::Moved
}

// === Combat System Functions ===

/// Calculate total damage for a weapon
pub fn weapon_damage(weapon: &Weapon) -> i32 {
    weapon.base_damage + weapon.damage_bonus
}

/// Get the damage an entity deals (from equipped weapon or unarmed)
pub fn get_attack_damage(world: &World, attacker: hecs::Entity) -> i32 {
    if let Ok(equipment) = world.get::<&Equipment>(attacker) {
        equipment.weapon.as_ref().map(|w| weapon_damage(w)).unwrap_or(1)
    } else {
        1  // Unarmed = 1 damage
    }
}

/// Perform an attack from attacker to target
fn perform_attack(world: &mut World, attacker: hecs::Entity, target: hecs::Entity, target_x: f32, target_y: f32) {
    let damage = get_attack_damage(world, attacker);

    // Apply damage to target
    if let Ok(mut health) = world.get::<&mut Health>(target) {
        health.current -= damage;
    }

    // Add lunge animation to attacker
    let _ = world.insert_one(attacker, LungeAnimation::new(target_x, target_y));

    // Add hit flash to target
    let _ = world.insert_one(target, HitFlash::new());
}

/// Open a door - remove blocking components (sprite stays the same but renders darker)
fn open_door(world: &mut World, door_id: hecs::Entity) {
    // Mark as open
    if let Ok(mut door) = world.get::<&mut Door>(door_id) {
        door.is_open = true;
    }

    // Remove blocking components
    let _ = world.remove_one::<BlocksVision>(door_id);
    let _ = world.remove_one::<BlocksMovement>(door_id);
}

/// Open a chest - mark as open and change sprite (keeps blocking movement)
fn open_chest(world: &mut World, chest_id: hecs::Entity) {
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
