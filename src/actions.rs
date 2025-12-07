//! Action system with execution-time validation.
//!
//! Actions are validated at execution time against the current world state,
//! ensuring that stale planning data cannot cause invalid moves (like entity swaps).

use crate::components::{
    Actor, Attackable, BlocksMovement, Container, Door, Health,
    LungeAnimation, Position,
};
use crate::constants::{
    COMBAT_CRIT_CHANCE, COMBAT_CRIT_MULTIPLIER, COMBAT_DAMAGE_MAX_MULT, COMBAT_DAMAGE_MIN_MULT,
};
use crate::events::{EventQueue, GameEvent};
use crate::grid::Grid;
use crate::systems::{get_attack_damage, open_chest, open_door};
use hecs::{Entity, World};
use rand::Rng;

/// The result of executing an action
#[derive(Debug, Clone)]
pub enum ActionResult {
    /// Entity moved to a new position
    Moved,
    /// Entity attacked another entity
    Attacked(Entity),
    /// Entity opened a door
    OpenedDoor(Entity),
    /// Entity opened/interacted with a chest
    OpenedChest(Entity),
    /// Action was blocked (something in the way)
    Blocked,
    /// Action is not possible (e.g., tile not walkable)
    Invalid,
}

/// An action that an entity intends to perform
#[derive(Debug, Clone)]
pub enum Action {
    /// Move in a direction (dx, dy). May convert to Attack at execution time.
    Move { dx: i32, dy: i32 },
    /// Wait in place (do nothing but spend energy)
    Wait,
}

impl Action {
    /// Soft check: is this action plausibly possible?
    /// Used for planning/pathfinding. Does NOT check for entities at target.
    pub fn is_possible(&self, world: &World, grid: &Grid, entity: Entity) -> bool {
        match self {
            Action::Move { dx, dy } => {
                let Ok(pos) = world.get::<&Position>(entity) else {
                    return false;
                };
                let target_x = pos.x + dx;
                let target_y = pos.y + dy;

                // Check if tile is walkable
                grid.get(target_x, target_y)
                    .map(|tile| tile.tile_type.is_walkable())
                    .unwrap_or(false)
            }
            Action::Wait => true,
        }
    }

    /// Execute the action with real-time validation.
    /// Checks the current world state and may convert to a different action.
    /// Emits events for other systems to react to.
    pub fn execute(
        &self,
        world: &mut World,
        grid: &Grid,
        entity: Entity,
        events: &mut EventQueue,
    ) -> ActionResult {
        match self {
            Action::Move { dx, dy } => execute_move(world, grid, entity, *dx, *dy, events),
            Action::Wait => {
                // Spend energy for waiting
                if let Ok(mut actor) = world.get::<&mut Actor>(entity) {
                    actor.energy -= actor.speed;
                }
                ActionResult::Moved // Wait counts as a successful action
            }
        }
    }
}

/// Execute a move action with full validation at execution time.
/// This is where the magic happens - we check the CURRENT state, not a snapshot.
fn execute_move(
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

    // Check if tile exists and is walkable
    let tile_walkable = grid
        .get(target_x, target_y)
        .map(|t| t.tile_type.is_walkable())
        .unwrap_or(false);

    if !tile_walkable {
        return ActionResult::Blocked;
    }

    // === EXECUTION-TIME CHECKS (current world state) ===

    // 1. Check for attackable entity at target position
    let mut enemy_to_attack: Option<Entity> = None;
    for (id, (enemy_pos, _attackable)) in world.query::<(&Position, &Attackable)>().iter() {
        if id != entity && enemy_pos.x == target_x && enemy_pos.y == target_y {
            enemy_to_attack = Some(id);
            break;
        }
    }

    if let Some(enemy_id) = enemy_to_attack {
        // Convert move to attack!
        return execute_attack(world, entity, enemy_id, target_x as f32, target_y as f32, events);
    }

    // 2. Check for closed door at target
    let mut door_to_open: Option<Entity> = None;
    for (id, (door_pos, door)) in world.query::<(&Position, &Door)>().iter() {
        if door_pos.x == target_x && door_pos.y == target_y && !door.is_open {
            door_to_open = Some(id);
            break;
        }
    }

    if let Some(door_id) = door_to_open {
        open_door(world, door_id);
        spend_energy(world, entity);
        events.push(GameEvent::DoorOpened {
            door: door_id,
            opener: entity,
        });
        return ActionResult::OpenedDoor(door_id);
    }

    // 3. Check for chest at target (blocking chests only)
    let mut chest_to_interact: Option<(Entity, bool, bool)> = None;
    for (id, (chest_pos, container, _blocks)) in
        world.query::<(&Position, &Container, &BlocksMovement)>().iter()
    {
        if chest_pos.x == target_x && chest_pos.y == target_y {
            chest_to_interact = Some((id, container.is_open, !container.is_empty()));
            break;
        }
    }

    if let Some((chest_id, is_open, has_items)) = chest_to_interact {
        if is_open && !has_items {
            return ActionResult::Blocked;
        }
        if !is_open {
            open_chest(world, chest_id);
            events.push(GameEvent::ChestOpened {
                chest: chest_id,
                opener: entity,
            });
        }
        spend_energy(world, entity);
        return ActionResult::OpenedChest(chest_id);
    }

    // 4. Check for any entity blocking movement at target
    for (id, (blocking_pos, _)) in world.query::<(&Position, &BlocksMovement)>().iter() {
        if id != entity && blocking_pos.x == target_x && blocking_pos.y == target_y {
            return ActionResult::Blocked;
        }
    }

    // 5. All checks passed - execute the move
    spend_energy(world, entity);
    if let Ok(mut pos) = world.get::<&mut Position>(entity) {
        pos.x = target_x;
        pos.y = target_y;
    }

    ActionResult::Moved
}

/// Execute an attack from attacker to target
fn execute_attack(
    world: &mut World,
    attacker: Entity,
    target: Entity,
    target_x: f32,
    target_y: f32,
    events: &mut EventQueue,
) -> ActionResult {
    let base_damage = get_attack_damage(world, attacker);

    // Roll for damage variance and critical hits
    let mut rng = rand::thread_rng();
    let damage_mult = rng.gen_range(COMBAT_DAMAGE_MIN_MULT..=COMBAT_DAMAGE_MAX_MULT);
    let is_crit = rng.gen::<f32>() < COMBAT_CRIT_CHANCE;

    let mut damage = (base_damage as f32 * damage_mult) as i32;
    if is_crit {
        damage = (damage as f32 * COMBAT_CRIT_MULTIPLIER) as i32;
    }

    // Ensure at least 1 damage
    damage = damage.max(1);

    // Apply damage to target
    if let Ok(mut health) = world.get::<&mut Health>(target) {
        health.current -= damage;
    }

    // Add lunge animation to attacker
    let _ = world.insert_one(attacker, LungeAnimation::new(target_x, target_y));

    // Spend energy
    spend_energy(world, attacker);

    // Emit attack event for VFX, sound, etc.
    events.push(GameEvent::AttackHit {
        attacker,
        target,
        target_pos: (target_x + 0.5, target_y + 0.5),
        damage,
    });

    ActionResult::Attacked(target)
}

/// Spend energy for performing an action
fn spend_energy(world: &mut World, entity: Entity) {
    if let Ok(mut actor) = world.get::<&mut Actor>(entity) {
        actor.energy -= actor.speed;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::components::Sprite;
    use crate::grid::Grid;
    use crate::tile::{Tile, TileType};

    /// Create a simple test grid with all floor tiles
    fn make_floor_grid(width: usize, height: usize) -> Grid {
        Grid {
            width,
            height,
            tiles: vec![Tile::new(TileType::Floor); width * height],
            chest_positions: vec![],
            door_positions: vec![],
        }
    }

    fn make_grid_with_wall_at(width: usize, height: usize, wall_x: i32, wall_y: i32) -> Grid {
        let mut tiles = vec![Tile::new(TileType::Floor); width * height];
        tiles[wall_y as usize * width + wall_x as usize] = Tile::new(TileType::Wall);
        Grid {
            width,
            height,
            tiles,
            chest_positions: vec![],
            door_positions: vec![],
        }
    }

    #[test]
    fn test_action_move_is_possible() {
        let mut world = World::new();
        let grid = make_floor_grid(10, 10);

        let entity = world.spawn((Position::new(5, 5),));

        let action = Action::Move { dx: 1, dy: 0 };
        assert!(action.is_possible(&world, &grid, entity));
    }

    #[test]
    fn test_action_move_blocked_by_wall() {
        let mut world = World::new();
        let grid = make_grid_with_wall_at(10, 10, 6, 5);

        let entity = world.spawn((Position::new(5, 5),));

        let action = Action::Move { dx: 1, dy: 0 };
        assert!(!action.is_possible(&world, &grid, entity));
    }

    #[test]
    fn test_action_wait_always_possible() {
        let mut world = World::new();
        let grid = make_floor_grid(10, 10);

        let entity = world.spawn((Position::new(5, 5),));

        let action = Action::Wait;
        assert!(action.is_possible(&world, &grid, entity));
    }

    #[test]
    fn test_execute_move_success() {
        let mut world = World::new();
        let grid = make_floor_grid(10, 10);
        let mut events = EventQueue::new();

        let entity = world.spawn((
            Position::new(5, 5),
            Actor::new(100),
        ));

        let action = Action::Move { dx: 1, dy: 0 };
        let result = action.execute(&mut world, &grid, entity, &mut events);

        assert!(matches!(result, ActionResult::Moved));

        let pos = world.get::<&Position>(entity).unwrap();
        assert_eq!(pos.x, 6);
        assert_eq!(pos.y, 5);
    }

    #[test]
    fn test_execute_move_blocked_by_wall() {
        let mut world = World::new();
        let grid = make_grid_with_wall_at(10, 10, 6, 5);
        let mut events = EventQueue::new();

        let entity = world.spawn((
            Position::new(5, 5),
            Actor::new(100),
        ));

        let action = Action::Move { dx: 1, dy: 0 };
        let result = action.execute(&mut world, &grid, entity, &mut events);

        assert!(matches!(result, ActionResult::Blocked));

        // Position should not have changed
        let pos = world.get::<&Position>(entity).unwrap();
        assert_eq!(pos.x, 5);
        assert_eq!(pos.y, 5);
    }

    #[test]
    fn test_execute_move_attacks_enemy() {
        let mut world = World::new();
        let grid = make_floor_grid(10, 10);
        let mut events = EventQueue::new();

        let attacker = world.spawn((
            Position::new(5, 5),
            Actor::new(100),
        ));

        let target = world.spawn((
            Position::new(6, 5),
            Health::new(100),
            Attackable,
        ));

        let action = Action::Move { dx: 1, dy: 0 };
        let result = action.execute(&mut world, &grid, attacker, &mut events);

        assert!(matches!(result, ActionResult::Attacked(_)));

        // Attacker should not have moved
        let pos = world.get::<&Position>(attacker).unwrap();
        assert_eq!(pos.x, 5);
        assert_eq!(pos.y, 5);

        // Target should have taken damage
        let health = world.get::<&Health>(target).unwrap();
        assert!(health.current < 100);
    }

    #[test]
    fn test_execute_move_opens_door() {
        let mut world = World::new();
        let grid = make_floor_grid(10, 10);
        let mut events = EventQueue::new();

        let entity = world.spawn((
            Position::new(5, 5),
            Actor::new(100),
        ));

        let door = world.spawn((
            Position::new(6, 5),
            Door::new(),
            crate::components::BlocksVision,
            BlocksMovement,
        ));

        let action = Action::Move { dx: 1, dy: 0 };
        let result = action.execute(&mut world, &grid, entity, &mut events);

        assert!(matches!(result, ActionResult::OpenedDoor(_)));

        // Door should now be open
        let door_component = world.get::<&Door>(door).unwrap();
        assert!(door_component.is_open);
    }

    #[test]
    fn test_execute_move_opens_chest() {
        let mut world = World::new();
        let grid = make_floor_grid(10, 10);
        let mut events = EventQueue::new();

        let entity = world.spawn((
            Position::new(5, 5),
            Actor::new(100),
        ));

        let chest = world.spawn((
            Position::new(6, 5),
            Container::new(vec![]),
            BlocksMovement,
            Sprite::new(0),
        ));

        let action = Action::Move { dx: 1, dy: 0 };
        let result = action.execute(&mut world, &grid, entity, &mut events);

        assert!(matches!(result, ActionResult::OpenedChest(_)));

        // Chest should now be open
        let container = world.get::<&Container>(chest).unwrap();
        assert!(container.is_open);
    }

    #[test]
    fn test_execute_wait_spends_energy() {
        let mut world = World::new();
        let grid = make_floor_grid(10, 10);
        let mut events = EventQueue::new();

        let entity = world.spawn((
            Position::new(5, 5),
            Actor { energy: 100, speed: 10 },
        ));

        let action = Action::Wait;
        action.execute(&mut world, &grid, entity, &mut events);

        let actor = world.get::<&Actor>(entity).unwrap();
        assert_eq!(actor.energy, 90);
    }

    #[test]
    fn test_execute_move_blocked_by_entity() {
        let mut world = World::new();
        let grid = make_floor_grid(10, 10);
        let mut events = EventQueue::new();

        let entity = world.spawn((
            Position::new(5, 5),
            Actor::new(100),
        ));

        // Blocking entity (not attackable)
        let _blocker = world.spawn((
            Position::new(6, 5),
            BlocksMovement,
        ));

        let action = Action::Move { dx: 1, dy: 0 };
        let result = action.execute(&mut world, &grid, entity, &mut events);

        assert!(matches!(result, ActionResult::Blocked));
    }
}
