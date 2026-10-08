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

/// One row of a list of items as the player sees it: a stackable kind
/// collapsed into a single entry with a count, anything else on its own.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ItemStack {
    /// Index of the stack's first item in the source list. Acting on a stack
    /// (taking, dropping, sacrificing one) goes through this index.
    pub first_index: usize,
    /// How many items the row stands for. Always 1 for non-stackables.
    pub count: u32,
}

/// Group a list of items for display, stacking every stackable kind (arrows)
/// into one row. Rows come out in order of each stack's first appearance, so
/// the list reads in the same order as the underlying `Vec`.
///
/// Items are stored individually; stacking is purely how they are shown and
/// how many a single click moves. Shared by the inventory, loot, altar and
/// shop windows so they all agree on what a stack is.
pub fn stack_items(items: &[ItemInstance]) -> Vec<ItemStack> {
    let mut stacks: Vec<ItemStack> = Vec::new();
    for (i, item) in items.iter().enumerate() {
        if item.kind.is_stackable() {
            if let Some(stack) = stacks
                .iter_mut()
                .find(|s| items[s.first_index].kind == item.kind)
            {
                stack.count += 1;
                continue;
            }
        }
        stacks.push(ItemStack { first_index: i, count: 1 });
    }
    stacks
}

/// Move items into an inventory, announcing one `ItemPickedUp` per stack
/// rather than per item, so a bundle of arrows is one log line and one sound.
fn give_items(
    world: &mut World,
    player_entity: Entity,
    items: Vec<ItemInstance>,
    events: Option<&mut EventQueue>,
) -> bool {
    let stacks = stack_items(&items);
    let Ok(mut inventory) = world.get::<&mut Inventory>(player_entity) else {
        return false;
    };
    for item in &items {
        inventory.current_weight_kg += item_weight(item.kind);
    }
    if let Some(events) = events {
        for stack in &stacks {
            events.push(GameEvent::ItemPickedUp {
                entity: player_entity,
                item: items[stack.first_index].kind,
                count: stack.count,
            });
        }
    }
    inventory.items.extend(items);
    true
}

/// Take the item at `item_index` from a container — and, if it is a
/// stackable kind, every other item of that kind in the same container, so
/// clicking a stack of arrows takes the whole stack.
pub fn take_item_from_container(
    world: &mut World,
    player_entity: Entity,
    container_entity: Entity,
    item_index: usize,
    events: Option<&mut EventQueue>,
) -> bool {
    if world.get::<&Inventory>(player_entity).is_err() {
        return false;
    }
    let taken = {
        let Ok(mut container) = world.get::<&mut Container>(container_entity) else {
            return false;
        };
        let Some(kind) = container.items.get(item_index).map(|i| i.kind) else {
            return false;
        };
        if kind.is_stackable() {
            let (taken, kept) = std::mem::take(&mut container.items)
                .into_iter()
                .partition(|i| i.kind == kind);
            container.items = kept;
            taken
        } else {
            vec![container.items.remove(item_index)]
        }
    };
    give_items(world, player_entity, taken, events)
}

/// Take all items and gold from a container and add them to player inventory
pub fn take_all_from_container(
    world: &mut World,
    player_entity: Entity,
    container_entity: Entity,
    mut events: Option<&mut EventQueue>,
) {
    if world.get::<&Inventory>(player_entity).is_err() {
        return;
    }
    let items = {
        let Ok(mut container) = world.get::<&mut Container>(container_entity) else {
            return;
        };
        std::mem::take(&mut container.items)
    };
    give_items(world, player_entity, items, events.as_deref_mut());
    take_gold_from_container(world, player_entity, container_entity, events);
}

/// Take everything from every container in a loot session (see
/// [`loot_sources`]).
pub fn take_all_from_sources(
    world: &mut World,
    player_entity: Entity,
    sources: &[Entity],
    mut events: Option<&mut EventQueue>,
) {
    for &source in sources {
        take_all_from_container(world, player_entity, source, events.as_deref_mut());
    }
}

/// Every container the loot window shows together.
///
/// Looting is per tile, not per container: a corpse lying on another corpse
/// and a pile of dropped arrows should take one visit, not three. This is the
/// container that was opened, plus every other walkable container on the
/// same tile that still holds something. Blocking containers (chests,
/// coffins, barrels) are never swept in — they are opened by bumping them,
/// and a closed one may hold a skeleton.
///
/// `primary` may already be gone — an emptied ground pile is despawned — in
/// which case the rest of the tile is still returned. Ordered chests first,
/// then corpses, then loose items, so the sections do not reshuffle between
/// frames.
pub fn loot_sources(world: &World, primary: Option<Entity>, tile: (i32, i32)) -> Vec<Entity> {
    use crate::components::ContainerType;

    let rank = |t: ContainerType| match t {
        ContainerType::Chest | ContainerType::Coffin | ContainerType::Barrel => 0,
        ContainerType::Corpse => 1,
        ContainerType::GroundPile => 2,
    };

    let mut sources: Vec<(u8, u32, Entity)> = Vec::new();
    if let Some(primary) = primary {
        if let Ok(container) = world.get::<&Container>(primary) {
            sources.push((rank(container.container_type), primary.id(), primary));
        }
    }
    for (id, (pos, container)) in world.query::<(&Position, &Container)>().iter() {
        if Some(id) == primary || (pos.x, pos.y) != tile || container.is_empty() {
            continue;
        }
        if world.get::<&BlocksMovement>(id).is_ok() {
            continue;
        }
        sources.push((rank(container.container_type), id.id(), id));
    }
    sources.sort_unstable_by_key(|&(rank, id, _)| (rank, id));
    sources.into_iter().map(|(_, _, e)| e).collect()
}

/// What the loot window calls a container: "Chest", "Goblin's corpse",
/// "On the ground". A corpse is named for whatever died, which is what tells
/// two corpses on one tile apart.
pub fn loot_source_label(world: &World, container_entity: Entity) -> String {
    use crate::components::{ContainerType, Name};

    let Ok(container) = world.get::<&Container>(container_entity) else {
        return "Loot".to_string();
    };
    match container.container_type {
        ContainerType::Chest => "Chest".to_string(),
        ContainerType::Coffin => "Coffin".to_string(),
        ContainerType::Barrel => "Barrel".to_string(),
        ContainerType::GroundPile => "On the ground".to_string(),
        ContainerType::Corpse => world
            .get::<&Name>(container_entity)
            .map(|n| format!("{}'s corpse", n.0))
            .unwrap_or_else(|_| "Remains".to_string()),
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

    fn arrows(n: usize) -> Vec<ItemInstance> {
        (0..n).map(|_| ItemInstance::plain(ItemType::Arrow)).collect()
    }

    /// Arrows collapse into one row wherever they sit in the list; other items
    /// keep a row each, in first-appearance order.
    #[test]
    fn stack_items_groups_only_stackables() {
        let mut items = arrows(1);
        items.push(ItemInstance::plain(ItemType::HealthPotion));
        items.extend(arrows(2));
        items.push(ItemInstance::plain(ItemType::HealthPotion));
        items.push(ItemInstance::plain(ItemType::FireArrow));

        let stacks = stack_items(&items);
        assert_eq!(
            stacks,
            vec![
                ItemStack { first_index: 0, count: 3 },
                ItemStack { first_index: 1, count: 1 },
                ItemStack { first_index: 4, count: 1 },
                ItemStack { first_index: 5, count: 1 },
            ]
        );
    }

    /// Clicking a stack of arrows in a chest takes all of them, and announces
    /// it once rather than once per arrow.
    #[test]
    fn taking_a_stack_takes_every_item_of_that_kind() {
        let mut world = World::new();
        let player = world.spawn((Position::new(0, 0), Inventory::new()));
        let mut items = arrows(4);
        items.insert(2, ItemInstance::plain(ItemType::HealthPotion));
        let chest = world.spawn((Position::new(1, 1), Container::chest(items, 0)));

        let mut events = EventQueue::new();
        assert!(take_item_from_container(&mut world, player, chest, 0, Some(&mut events)));

        let container = world.get::<&Container>(chest).unwrap();
        assert_eq!(container.items.len(), 1);
        assert_eq!(container.items[0].kind, ItemType::HealthPotion);
        let inventory = world.get::<&Inventory>(player).unwrap();
        assert_eq!(inventory.items.len(), 4);

        let pickups: Vec<_> = events
            .drain()
            .filter_map(|e| match e {
                GameEvent::ItemPickedUp { item, count, .. } => Some((item, count)),
                _ => None,
            })
            .collect();
        assert_eq!(pickups, vec![(ItemType::Arrow, 4)]);
    }

    /// Two corpses and a dropped pile on one tile are one loot session; a
    /// corpse next door and a closed chest on the tile are not part of it.
    #[test]
    fn loot_sources_cover_every_walkable_container_on_the_tile() {
        let mut world = World::new();
        let player = world.spawn((Position::new(0, 0), Inventory::new()));
        let tile = (3, 3);
        let at = Position::new(tile.0, tile.1);
        let corpse_a = world.spawn((at, Container::corpse(arrows(2), 5)));
        let corpse_b = world.spawn((at, Container::corpse(arrows(1), 0)));
        let pile = world.spawn((
            at,
            Container::ground_pile(vec![ItemInstance::plain(ItemType::Apple)]),
            GroundItemPile,
        ));
        let _empty_corpse = world.spawn((at, Container::corpse(Vec::new(), 0)));
        let _closed_chest = world.spawn((at, Container::chest(arrows(9), 0), BlocksMovement));
        let _neighbour = world.spawn((Position::new(4, 3), Container::corpse(arrows(1), 0)));

        // Opened via the pile, but corpses still list first.
        let sources = loot_sources(&world, Some(pile), tile);
        assert_eq!(sources, vec![corpse_a, corpse_b, pile]);

        take_all_from_sources(&mut world, player, &sources, None);
        let inventory = world.get::<&Inventory>(player).unwrap();
        assert_eq!(inventory.items.len(), 4);
        assert_eq!(inventory.gold, 5);
    }

    /// An emptied ground pile is despawned mid-session; the rest of the tile
    /// must still be reachable through the stale primary.
    #[test]
    fn loot_sources_survive_the_primary_despawning() {
        let mut world = World::new();
        let at = Position::new(2, 2);
        let pile = world.spawn((at, Container::ground_pile(Vec::new()), GroundItemPile));
        let corpse = world.spawn((at, Container::corpse(arrows(1), 0)));
        cleanup_empty_ground_piles(&mut world);

        assert_eq!(loot_sources(&world, Some(pile), (2, 2)), vec![corpse]);
    }

    #[test]
    fn corpses_are_labelled_by_what_died() {
        use crate::components::Name;
        let mut world = World::new();
        let named = world.spawn((Position::new(0, 0), Container::corpse(Vec::new(), 0), Name::new("Goblin")));
        let anon = world.spawn((Position::new(0, 0), Container::corpse(Vec::new(), 0)));
        assert_eq!(loot_source_label(&world, named), "Goblin's corpse");
        assert_eq!(loot_source_label(&world, anon), "Remains");
    }
}
