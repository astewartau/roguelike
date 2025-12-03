use crate::components::{Actor, Container, Inventory, Position, RandomWanderAI, Sprite, VisualPosition};
use crate::fov::FOV;
use crate::grid::Grid;
use hecs::World;
use rand::Rng;

/// Smoothly interpolate visual positions toward logical positions
pub fn visual_lerp(world: &mut World, dt: f32) {
    let lerp_speed = dt * 50.0;
    for (_id, (pos, vis_pos)) in world.query_mut::<(&Position, &mut VisualPosition)>() {
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

/// Update field of view from player position
pub fn update_fov(world: &World, grid: &mut Grid, player_entity: hecs::Entity, radius: i32) {
    let Ok(player_pos) = world.get::<&Position>(player_entity) else {
        return;
    };

    // Clear visibility
    for tile in &mut grid.tiles {
        tile.visible = false;
    }

    // Calculate and apply FOV
    let visible_tiles = FOV::calculate(grid, player_pos.x, player_pos.y, radius);
    for (x, y) in visible_tiles {
        if let Some(tile) = grid.get_mut(x, y) {
            tile.visible = true;
            tile.explored = true;
        }
    }
}

/// Collect entities that should be rendered, with fog of war applied
pub fn collect_renderables(
    world: &World,
    grid: &Grid,
    player_entity: hecs::Entity,
) -> Vec<(f32, f32, Sprite, f32)> {
    let mut entities_to_render: Vec<(f32, f32, Sprite, f32)> = Vec::new();
    let mut player_render: Option<(f32, f32, Sprite, f32)> = None;

    for (id, (pos, vis_pos, sprite)) in world.query::<(&Position, &VisualPosition, &Sprite)>().iter() {
        let (is_explored, is_visible) = grid.get(pos.x, pos.y)
            .map(|tile| (tile.explored, tile.visible))
            .unwrap_or((false, false));

        // Actors (enemies) are only visible in FOV, not in fog
        let is_actor = world.get::<&Actor>(id).is_ok();

        if id == player_entity {
            player_render = Some((vis_pos.x, vis_pos.y, *sprite, 1.0));
        } else if is_visible {
            entities_to_render.push((vis_pos.x, vis_pos.y, *sprite, 1.0));
        } else if is_explored && !is_actor {
            // In fog but explored - only show non-actors (chests, items)
            entities_to_render.push((vis_pos.x, vis_pos.y, *sprite, 0.5));
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

/// Move entities with RandomWanderAI in a random direction
pub fn ai_wander(world: &mut World, grid: &Grid, rng: &mut impl Rng) {
    // Collect moves first to avoid borrow conflicts
    let mut wander_moves: Vec<(hecs::Entity, i32, i32)> = Vec::new();

    for (id, (pos, actor, _wander)) in world.query::<(&Position, &Actor, &RandomWanderAI)>().iter() {
        if actor.energy >= actor.speed {
            let dirs = [(0, 1), (0, -1), (1, 0), (-1, 0)];
            let (dx, dy) = dirs[rng.gen_range(0..4)];
            let new_x = pos.x + dx;
            let new_y = pos.y + dy;
            if let Some(tile) = grid.get(new_x, new_y) {
                if tile.tile_type.is_walkable() {
                    wander_moves.push((id, new_x, new_y));
                }
            }
        }
    }

    // Apply moves
    for (id, new_x, new_y) in wander_moves {
        if let Ok(mut actor) = world.get::<&mut Actor>(id) {
            actor.energy -= actor.speed;
        }
        if let Ok(mut pos) = world.get::<&mut Position>(id) {
            pos.x = new_x;
            pos.y = new_y;
        }
    }
}

/// Handle player movement and item pickup
pub fn player_move(
    world: &mut World,
    player_entity: hecs::Entity,
    dx: i32,
    dy: i32,
) {
    let current_pos = world.get::<&Position>(player_entity).ok().map(|p| *p);
    let Some(pos) = current_pos else { return };

    let target_pos = Position::new(pos.x + dx, pos.y + dy);

    // Spend energy and move
    if let Ok(mut actor) = world.get::<&mut Actor>(player_entity) {
        actor.energy -= actor.speed;
    }
    if let Ok(mut pos) = world.get::<&mut Position>(player_entity) {
        *pos = target_pos;
    }

    // Check for chest pickup
    pickup_items(world, player_entity, target_pos);
}

/// Pick up items from containers at the given position
fn pickup_items(world: &mut World, player_entity: hecs::Entity, pos: Position) {
    let mut collected_items = Vec::new();

    for (_id, (chest_pos, container)) in world.query_mut::<(&Position, &mut Container)>() {
        if chest_pos.x == pos.x && chest_pos.y == pos.y && !container.is_open {
            container.is_open = true;
            collected_items = std::mem::take(&mut container.items);
            break;
        }
    }

    if !collected_items.is_empty() {
        if let Ok(mut inventory) = world.get::<&mut Inventory>(player_entity) {
            for item in collected_items {
                inventory.current_weight_kg += item.weight_kg();
                inventory.items.push(item);
            }
        }
    }
}
