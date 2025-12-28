//! Inventory and container interaction systems.

use crate::components::{BlocksMovement, Container, GroundItemPile, Inventory, ItemType, Position, Sprite, VisualPosition};
use crate::events::{EventQueue, GameEvent};
use crate::systems::item_defs;
use crate::systems::items::item_weight;
use hecs::{Entity, World};

/// Add an item directly to an entity's inventory
pub fn add_item_to_inventory(world: &mut World, entity: Entity, item: ItemType) -> bool {
    if let Ok(mut inventory) = world.get::<&mut Inventory>(entity) {
        inventory.current_weight_kg += item_weight(item);
        inventory.items.push(item);
        true
    } else {
        false
    }
}

/// Take a single item from a container and add it to player inventory
pub fn take_item_from_container(
    world: &mut World,
    player_entity: Entity,
    container_entity: Entity,
    item_index: usize,
    events: Option<&mut EventQueue>,
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
        if let Some(events) = events {
            events.push(GameEvent::ItemPickedUp {
                entity: player_entity,
                item,
            });
        }
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
    mut events: Option<&mut EventQueue>,
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
            if let Some(ref mut events) = events {
                events.push(GameEvent::ItemPickedUp {
                    entity: player_entity,
                    item,
                });
            }
        }
        inventory.gold += gold;
        if gold > 0 {
            if let Some(events) = events {
                events.push(GameEvent::GoldPickedUp {
                    entity: player_entity,
                    amount: gold,
                });
            }
        }
    }
}

/// Take gold from a container
pub fn take_gold_from_container(
    world: &mut World,
    player_entity: Entity,
    container_entity: Entity,
    events: Option<&mut EventQueue>,
) {
    let gold = {
        let Ok(mut container) = world.get::<&mut Container>(container_entity) else {
            return;
        };
        let gold = container.gold;
        container.gold = 0;
        gold
    };

    if gold > 0 {
        if let Ok(mut inventory) = world.get::<&mut Inventory>(player_entity) {
            inventory.gold += gold;
            if let Some(events) = events {
                events.push(GameEvent::GoldPickedUp {
                    entity: player_entity,
                    amount: gold,
                });
            }
        }
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

/// Spawn a ground item pile at a position, or add to existing pile
/// Returns the entity ID of the pile
pub fn spawn_ground_item(world: &mut World, x: i32, y: i32, item: ItemType) -> Entity {
    // Check for existing ground item pile at this position
    let existing_pile = find_ground_items_at_position(world, x, y);

    if let Some(pile_entity) = existing_pile {
        // Add to existing pile
        if let Ok(mut container) = world.get::<&mut Container>(pile_entity) {
            container.items.push(item);
        }
        pile_entity
    } else {
        // Create new ground item pile
        let sprite_ref = item_defs::get_def(item).sprite;
        let pos = Position::new(x, y);
        world.spawn((
            pos,
            VisualPosition::from_position(&pos),
            Sprite::from_ref(sprite_ref),
            Container::new(vec![item]),
            GroundItemPile,
        ))
    }
}

/// Find a ground item pile at a specific position
pub fn find_ground_items_at_position(world: &World, x: i32, y: i32) -> Option<Entity> {
    for (id, (pos, container, _pile)) in world.query::<(&Position, &Container, &GroundItemPile)>().iter() {
        if pos.x == x && pos.y == y && !container.is_empty() {
            return Some(id);
        }
    }
    None
}

/// Find a ground item pile at the player's position
#[allow(dead_code)] // Reserved for future ground item interaction features
pub fn find_ground_items_at_player(world: &World, player_entity: Entity) -> Option<Entity> {
    let player_pos = world.get::<&Position>(player_entity).ok()?;
    find_ground_items_at_position(world, player_pos.x, player_pos.y)
}

/// Remove ground item piles that are empty
pub fn cleanup_empty_ground_piles(world: &mut World) {
    let empty_piles: Vec<Entity> = world
        .query::<(&Container, &GroundItemPile)>()
        .iter()
        .filter(|(_, (container, _))| container.is_empty())
        .map(|(id, _)| id)
        .collect();

    for id in empty_piles {
        let _ = world.despawn(id);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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

        take_gold_from_container(&mut world, player, chest, None);

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

        take_all_from_container(&mut world, player, chest, None);

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

        let success = take_item_from_container(&mut world, player, chest, 0, None);
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

        let success = take_item_from_container(&mut world, player, chest, 5, None);
        assert!(!success);
    }
}
