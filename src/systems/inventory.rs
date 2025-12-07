//! Inventory and container interaction systems.

use crate::components::{BlocksMovement, Container, Inventory, Position};
use crate::systems::items::item_weight;
use hecs::{Entity, World};

/// Take a single item from a container and add it to player inventory
pub fn take_item_from_container(
    world: &mut World,
    player_entity: Entity,
    container_entity: Entity,
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
    player_entity: Entity,
    container_entity: Entity,
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
    player_entity: Entity,
    container_entity: Entity,
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
pub fn find_container_at_player(world: &World, player_entity: Entity) -> Option<Entity> {
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::components::ItemType;

    #[test]
    fn test_take_gold_from_container() {
        let mut world = World::new();

        let player = world.spawn((
            Position::new(0, 0),
            Inventory::new(),
        ));

        let chest = world.spawn((
            Position::new(1, 1),
            Container::with_gold(vec![], 100),
        ));

        take_gold_from_container(&mut world, player, chest);

        let inventory = world.get::<&Inventory>(player).unwrap();
        assert_eq!(inventory.gold, 100);

        let container = world.get::<&Container>(chest).unwrap();
        assert_eq!(container.gold, 0);
    }

    #[test]
    fn test_take_all_from_container() {
        let mut world = World::new();

        let player = world.spawn((
            Position::new(0, 0),
            Inventory::new(),
        ));

        let chest = world.spawn((
            Position::new(1, 1),
            Container::with_gold(vec![ItemType::HealthPotion], 50),
        ));

        take_all_from_container(&mut world, player, chest);

        let inventory = world.get::<&Inventory>(player).unwrap();
        assert_eq!(inventory.gold, 50);
        assert_eq!(inventory.items.len(), 1);
        assert_eq!(inventory.items[0], ItemType::HealthPotion);

        let container = world.get::<&Container>(chest).unwrap();
        assert!(container.is_empty());
    }

    #[test]
    fn test_take_item_from_container() {
        let mut world = World::new();

        let player = world.spawn((
            Position::new(0, 0),
            Inventory::new(),
        ));

        let chest = world.spawn((
            Position::new(1, 1),
            Container::new(vec![ItemType::HealthPotion]),
        ));

        let success = take_item_from_container(&mut world, player, chest, 0);
        assert!(success);

        let inventory = world.get::<&Inventory>(player).unwrap();
        assert_eq!(inventory.items.len(), 1);

        let container = world.get::<&Container>(chest).unwrap();
        assert!(container.items.is_empty());
    }

    #[test]
    fn test_take_item_invalid_index() {
        let mut world = World::new();

        let player = world.spawn((
            Position::new(0, 0),
            Inventory::new(),
        ));

        let chest = world.spawn((
            Position::new(1, 1),
            Container::new(vec![ItemType::HealthPotion]),
        ));

        let success = take_item_from_container(&mut world, player, chest, 5);
        assert!(!success);
    }
}
