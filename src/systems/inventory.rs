//! Inventory and container interaction systems.

use crate::components::{BlocksMovement, Container, GroundItemPile, Inventory, ItemInstance, Position, Sprite, VisualPosition};
use crate::events::{EventQueue, GameEvent};
use crate::spatial_cache::SpatialCache;
use crate::systems::item_defs;
use crate::systems::items::item_weight;
use hecs::{Entity, World};

/// Add an item directly to an entity's inventory
pub fn add_item_to_inventory(world: &mut World, entity: Entity, item: ItemInstance) -> bool {
    if let Ok(mut inventory) = world.get::<&mut Inventory>(entity) {
        inventory.current_weight_kg += item_weight(item.kind);
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
        inventory.current_weight_kg += item_weight(item.kind);
        let kind = item.kind;
        inventory.items.push(item);
        if let Some(events) = events {
            events.push(GameEvent::ItemPickedUp {
                entity: player_entity,
                item: kind,
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
            inventory.current_weight_kg += item_weight(item.kind);
            let kind = item.kind;
            inventory.items.push(item);
            if let Some(ref mut events) = events {
                events.push(GameEvent::ItemPickedUp {
                    entity: player_entity,
                    item: kind,
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
pub fn spawn_ground_item(world: &mut World, x: i32, y: i32, item: ItemInstance) -> Entity {
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
        let sprite_ref = item_defs::get_def(item.kind).sprite;
        let pos = Position::new(x, y);
        world.spawn((
            pos,
            VisualPosition::from_position(&pos),
            Sprite::from_ref(sprite_ref),
            Container::ground_pile(vec![item]),
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

/// Stop a looted container blocking its tile.
///
/// An opened, emptied chest is scenery, and `load_floor` has always restored
/// one as walkable — but nothing in live play ever dropped the flag, so a floor
/// the player stayed on kept its looted chests as obstacles while the same
/// floor revisited let them walk straight over. This is the live half of that,
/// so both agree.
///
/// A sweep rather than a hook inside each take: taking gold, taking one item
/// and taking everything can each be the call that empties a container, and
/// `take_*` has no access to the cache. Runs alongside
/// [`cleanup_empty_ground_piles`] on the same looting paths.
///
/// Clearing the flag in the world is only half the job — the `SpatialCache` is
/// what movement and pathfinding actually read, so it is told too.
pub fn unblock_emptied_containers(world: &mut World, spatial_cache: &mut SpatialCache) {
    let emptied: Vec<Entity> = world
        .query::<(&Container, &BlocksMovement)>()
        .iter()
        .filter(|(_, (container, _))| container.is_looted())
        .map(|(id, _)| id)
        .collect();

    for id in emptied {
        let _ = world.remove_one::<BlocksMovement>(id);
        spatial_cache.clear_blocking_flags(id);
    }
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
            Container::chest(vec![], 100),
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
            Container::chest(vec![ItemInstance::plain(ItemType::HealthPotion)], 50),
        ));

        take_all_from_container(&mut world, player, chest, None);

        let inventory = world.get::<&Inventory>(player).unwrap();
        assert_eq!(inventory.gold, 50);
        assert_eq!(inventory.items.len(), 1);
        assert_eq!(inventory.items[0].kind, ItemType::HealthPotion);

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
            Container::chest(vec![ItemInstance::plain(ItemType::HealthPotion)], 0),
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
            Container::chest(vec![ItemInstance::plain(ItemType::HealthPotion)], 0),
        ));

        let success = take_item_from_container(&mut world, player, chest, 5, None);
        assert!(!success);
    }

    /// Looting a chest dry stops it blocking its tile — in the world *and* in
    /// the SpatialCache, which is what movement and pathfinding read.
    #[test]
    fn looting_a_container_dry_stops_it_blocking() {
        use crate::spatial_cache::SpatialCache;

        let mut world = World::new();
        let player = world.spawn((Position::new(0, 0), Inventory::new()));
        let at = Position::new(1, 1);
        let chest = world.spawn((
            at,
            VisualPosition::from_position(&at),
            Container::chest(vec![ItemInstance::plain(ItemType::HealthPotion)], 7),
            BlocksMovement,
        ));
        let mut cache = SpatialCache::rebuild_from_world(&world);
        assert!(cache.is_blocked((1, 1)), "a full chest blocks");

        // Opened but still holding gold: not looted yet.
        if let Ok(mut container) = world.get::<&mut Container>(chest) {
            container.is_open = true;
        }
        unblock_emptied_containers(&mut world, &mut cache);
        assert!(
            cache.is_blocked((1, 1)),
            "an opened chest with gold left in it still blocks"
        );

        // Now actually empty it.
        take_all_from_container(&mut world, player, chest, None);
        assert!(
            world.get::<&Container>(chest).expect("chest").is_looted(),
            "take-all should leave it open and empty"
        );
        unblock_emptied_containers(&mut world, &mut cache);

        assert!(
            world.get::<&BlocksMovement>(chest).is_err(),
            "a looted chest should not keep BlocksMovement"
        );
        assert!(!cache.is_blocked((1, 1)), "and the cache should agree");
        cache.assert_coherent_with_world(&world, "after looting a chest");
    }

    /// Taking only the gold can be the call that empties a container, so the
    /// sweep has to cover that path too — it is a sweep rather than a hook
    /// inside take-all for exactly this reason.
    #[test]
    fn taking_the_last_gold_also_unblocks() {
        use crate::spatial_cache::SpatialCache;

        let mut world = World::new();
        let player = world.spawn((Position::new(0, 0), Inventory::new()));
        let at = Position::new(2, 2);
        let chest = world.spawn((
            at,
            VisualPosition::from_position(&at),
            Container::chest(Vec::new(), 12),
            BlocksMovement,
        ));
        let mut cache = SpatialCache::rebuild_from_world(&world);
        if let Ok(mut container) = world.get::<&mut Container>(chest) {
            container.is_open = true;
        }

        take_gold_from_container(&mut world, player, chest, None);
        unblock_emptied_containers(&mut world, &mut cache);

        assert!(!cache.is_blocked((2, 2)), "gold-only chest should unblock too");
        cache.assert_coherent_with_world(&world, "after taking the last gold");
    }
}
