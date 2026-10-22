//! Inventory and container interaction systems.
//!
//! Items are entities (`systems::held`): looting moves them, entity and
//! all, from a container or the floor into a pack.

use crate::components::{BlocksMovement, Container, Inventory, ItemInstance, Position};
use crate::events::{EventQueue, GameEvent};
use crate::spatial_cache::SpatialCache;
use crate::systems::held;
use crate::systems::items::item_weight;
use hecs::Entity;
use crate::world::World;

/// Put held item `item` in `entity`'s pack. The caller has taken it out of
/// wherever it was (`held::take_out`, `held::lift`).
pub fn add_to_inventory(world: &mut World, entity: Entity, item: Entity) -> bool {
    let weight = held::kind(world, item).map_or(0.0, item_weight);
    if let Ok(mut inventory) = world.get::<&mut Inventory>(entity) {
        inventory.current_weight_kg += weight;
        inventory.items.push(item);
        true
    } else {
        false
    }
}

/// The item at `index` in `entity`'s pack.
pub fn inventory_item(world: &hecs::World, entity: Entity, index: usize) -> Option<Entity> {
    world.get::<&Inventory>(entity).ok()?.items.get(index).copied()
}

/// The kind of the item at `index` in `entity`'s pack.
pub fn inventory_kind(world: &hecs::World, entity: Entity, index: usize) -> Option<crate::components::ItemType> {
    inventory_item(world, entity, index).and_then(|e| held::kind(world, e))
}

/// Make a new item and put it in `entity`'s pack.
pub fn add_item_to_inventory(world: &mut World, entity: Entity, item: ItemInstance) -> bool {
    if world.get::<&Inventory>(entity).is_err() {
        return false;
    }
    let item = held::spawn_item(world, item);
    add_to_inventory(world, entity, item)
}

/// Take `item` out of `entity`'s pack (it stays an entity: put it somewhere
/// or despawn it). Returns whether it was there.
pub fn remove_from_inventory(world: &mut World, entity: Entity, item: Entity) -> bool {
    let weight = held::kind(world, item).map_or(0.0, item_weight);
    let Ok(mut inventory) = world.get::<&mut Inventory>(entity) else {
        return false;
    };
    let Some(i) = inventory.items.iter().position(|&e| e == item) else {
        return false;
    };
    inventory.items.remove(i);
    inventory.current_weight_kg = (inventory.current_weight_kg - weight).max(0.0);
    true
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

/// Move held items into an inventory, announcing one `ItemPickedUp` per
/// stack rather than per item, so a bundle of arrows is one log line and
/// one sound.
fn give_items(world: &mut World, player_entity: Entity, items: Vec<Entity>, events: Option<&mut EventQueue>) -> bool {
    if world.get::<&Inventory>(player_entity).is_err() {
        return false;
    }
    let values = held::instances(world, &items);
    if let Some(events) = events {
        for stack in stack_items(&values) {
            events.push(GameEvent::ItemPickedUp {
                entity: player_entity,
                item: values[stack.first_index].kind,
                count: stack.count,
            });
        }
    }
    for item in items {
        add_to_inventory(world, player_entity, item);
    }
    true
}

/// Where loot comes from: a container (a chest, a corpse), or whatever
/// lies loose on a tile.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LootSource {
    Container(Entity),
    Ground((i32, i32)),
}

/// The items a source holds, in order.
pub fn source_items(world: &hecs::World, source: LootSource) -> Vec<Entity> {
    match source {
        LootSource::Container(e) => world.get::<&Container>(e).map(|c| c.items.clone()).unwrap_or_default(),
        LootSource::Ground(tile) => held::on_ground_at(world, tile),
    }
}

/// The gold a source holds.
pub fn source_gold(world: &hecs::World, source: LootSource) -> u32 {
    match source {
        LootSource::Container(e) => world.get::<&Container>(e).map(|c| c.gold).unwrap_or(0),
        LootSource::Ground(_) => 0,
    }
}

/// Take `item` from `source` — and, if it is a stackable kind, every other
/// item of that kind there, so clicking a stack of arrows takes the whole
/// stack.
pub fn take_item(
    world: &mut World,
    player_entity: Entity,
    source: LootSource,
    item: Entity,
    events: Option<&mut EventQueue>,
) -> bool {
    if world.get::<&Inventory>(player_entity).is_err() {
        return false;
    }
    let there = source_items(world, source);
    let Some(kind) = there.contains(&item).then(|| held::kind(world, item)).flatten() else {
        return false;
    };
    let taken: Vec<Entity> = if kind.is_stackable() {
        there.into_iter().filter(|&e| held::kind(world, e) == Some(kind)).collect()
    } else {
        vec![item]
    };
    for &e in &taken {
        lift_from(world, source, e);
    }
    give_items(world, player_entity, taken, events)
}

/// Take `item` out of `source`, ready to go somewhere else.
fn lift_from(world: &mut World, source: LootSource, item: Entity) {
    match source {
        LootSource::Container(c) => {
            if let Ok(mut container) = world.get::<&mut Container>(c) {
                container.items.retain(|&e| e != item);
            }
        }
        LootSource::Ground(_) => held::lift(world, item),
    }
}

/// Take the item at `item_index` from a container (and the rest of its
/// stack: `take_item`).
#[cfg(test)]
pub fn take_item_from_container(
    world: &mut World,
    player_entity: Entity,
    container_entity: Entity,
    item_index: usize,
    events: Option<&mut EventQueue>,
) -> bool {
    let source = LootSource::Container(container_entity);
    let Some(&item) = source_items(world, source).get(item_index) else {
        return false;
    };
    take_item(world, player_entity, source, item, events)
}

/// Take everything a source holds, gold and all.
pub fn take_all_from_source(world: &mut World, player_entity: Entity, source: LootSource, mut events: Option<&mut EventQueue>) {
    if world.get::<&Inventory>(player_entity).is_err() {
        return;
    }
    let items = source_items(world, source);
    for &e in &items {
        lift_from(world, source, e);
    }
    give_items(world, player_entity, items, events.as_deref_mut());
    take_gold(world, player_entity, source, events);
}

/// Take all items and gold from a container and add them to player inventory
#[cfg(test)]
pub fn take_all_from_container(
    world: &mut World,
    player_entity: Entity,
    container_entity: Entity,
    events: Option<&mut EventQueue>,
) {
    take_all_from_source(world, player_entity, LootSource::Container(container_entity), events);
}

/// Take everything from every source in a loot session (see
/// [`loot_sources`]).
pub fn take_all_from_sources(
    world: &mut World,
    player_entity: Entity,
    sources: &[LootSource],
    mut events: Option<&mut EventQueue>,
) {
    for &source in sources {
        take_all_from_source(world, player_entity, source, events.as_deref_mut());
    }
}

/// Everything the loot window shows together.
///
/// Looting is per tile, not per container: a corpse lying on another corpse
/// and some dropped arrows should take one visit, not three. This is the
/// container that was opened, plus every other walkable container on the
/// same tile that still holds something, plus whatever lies loose there.
/// Blocking containers (chests, coffins, barrels) are never swept in — they
/// are opened by bumping them, and a closed one may hold a skeleton.
///
/// `primary` may already be gone, or be a loose item rather than a
/// container, in which case the rest of the tile is still returned. Ordered
/// chests first, then corpses, then loose items, so the sections do not
/// reshuffle between frames.
pub fn loot_sources(world: &hecs::World, primary: Option<Entity>, tile: (i32, i32)) -> Vec<LootSource> {
    let rank = |id: Entity| if world.get::<&BlocksMovement>(id).is_ok() { 0 } else { 1 };

    let mut containers: Vec<(u8, u32, Entity)> = Vec::new();
    if let Some(primary) = primary {
        if world.get::<&Container>(primary).is_ok() {
            containers.push((rank(primary), primary.id(), primary));
        }
    }
    for (id, (pos, container)) in world.query::<(&Position, &Container)>().iter() {
        if Some(id) == primary || (pos.x, pos.y) != tile || container.is_empty() {
            continue;
        }
        if world.get::<&BlocksMovement>(id).is_ok() {
            continue;
        }
        containers.push((rank(id), id.id(), id));
    }
    containers.sort_unstable_by_key(|&(rank, id, _)| (rank, id));
    let mut sources: Vec<LootSource> = containers.into_iter().map(|(_, _, e)| LootSource::Container(e)).collect();
    if !held::on_ground_at(world, tile).is_empty() {
        sources.push(LootSource::Ground(tile));
    }
    sources
}

/// What the loot window calls a source: "Chest", "Goblin's corpse",
/// "On the ground". A corpse is named for whatever died, which is what tells
/// two corpses on one tile apart.
pub fn loot_source_label(world: &hecs::World, source: LootSource) -> String {
    use crate::components::Name;

    match source {
        LootSource::Ground(_) => "On the ground".to_string(),
        LootSource::Container(e) => {
            if world.get::<&Container>(e).is_err() {
                return "Loot".to_string();
            }
            world.get::<&Name>(e).map(|n| n.0.clone()).unwrap_or_else(|_| "Remains".to_string())
        }
    }
}

/// Take the gold a source holds.
pub fn take_gold(world: &mut World, player_entity: Entity, source: LootSource, events: Option<&mut EventQueue>) {
    if let LootSource::Container(e) = source {
        take_gold_from_container(world, player_entity, e, events);
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

/// Something to loot where the player stands: a walkable container with
/// something in it (remains), or else an item lying there. Chests are
/// opened by bumping, not this.
pub fn find_container_at_player(world: &World, player_entity: Entity) -> Option<Entity> {
    let player_pos = world.get::<&Position>(player_entity).ok()?;
    let tile = (player_pos.x, player_pos.y);

    for (id, (pos, container)) in world.query::<(&Position, &Container)>().iter() {
        // Skip if it's a chest (has BlocksMovement) - those are handled by bumping
        if world.get::<&BlocksMovement>(id).is_ok() {
            continue;
        }
        if (pos.x, pos.y) == tile && !container.is_empty() {
            return Some(id);
        }
    }
    held::on_ground_at(world, tile).first().copied()
}

/// Use up the first item in `entity`'s pack of the first of `kinds` it has
/// (a preferred arrow, else another): it's gone. Returns which kind.
pub fn use_up_first_of(world: &mut World, entity: Entity, kinds: &[crate::components::ItemType]) -> Option<crate::components::ItemType> {
    let items = world.get::<&Inventory>(entity).ok()?.items.clone();
    let (item, kind) = kinds
        .iter()
        .find_map(|&k| items.iter().find(|&&e| held::kind(world, e) == Some(k)).map(|&e| (e, k)))?;
    remove_from_inventory(world, entity, item);
    let _ = world.despawn(item);
    Some(kind)
}

/// Leave `gold` lying on `(x, y)`: a few coins, looted like remains.
pub fn drop_gold(world: &mut World, x: i32, y: i32, gold: u32) -> Option<Entity> {
    if gold == 0 {
        return None;
    }
    let at = Position::new(x, y);
    Some(world.spawn((
        at,
        crate::components::VisualPosition::from_position(&at),
        crate::components::Sprite::from_ref(crate::tile::tile_ids::COINS),
        Container::corpse(Vec::new(), gold),
        crate::components::Name::new("Coins"),
    )))
}

/// Make a new item lying loose on `(x, y)`. Returns the item.
pub fn spawn_ground_item(world: &mut World, x: i32, y: i32, item: ItemInstance) -> Entity {
    held::drop_new(world, x, y, item)
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
/// `take_*` has no access to the cache.
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::components::{ItemType, VisualPosition};

    fn items(world: &mut World, kinds: &[ItemType]) -> Vec<Entity> {
        held::spawn_items(world, kinds.iter().map(|&k| ItemInstance::plain(k)))
    }

    fn kinds(world: &World, list: &[Entity]) -> Vec<ItemType> {
        list.iter().filter_map(|&e| held::kind(world, e)).collect()
    }

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

        let held_items = items(&mut world, &[ItemType::HealthPotion]);
        let chest = world.spawn((
            Position::new(1, 1),
            Container::chest(held_items, 50),
        ));

        take_all_from_container(&mut world, player, chest, None);

        let inventory = world.get::<&Inventory>(player).unwrap();
        assert_eq!(inventory.gold, 50);
        assert_eq!(inventory.items.len(), 1);
        assert_eq!(kinds(&world, &inventory.items), vec![ItemType::HealthPotion]);

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

        let held_items = items(&mut world, &[ItemType::HealthPotion]);
        let chest = world.spawn((
            Position::new(1, 1),
            Container::chest(held_items, 0),
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

        let held_items = items(&mut world, &[ItemType::HealthPotion]);
        let chest = world.spawn((
            Position::new(1, 1),
            Container::chest(held_items, 0),
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
        let held_items = items(&mut world, &[ItemType::HealthPotion]);
        let chest = world.spawn((
            at,
            VisualPosition::from_position(&at),
            Container::chest(held_items, 7),
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

    fn arrow_values(n: usize) -> Vec<ItemInstance> {
        (0..n).map(|_| ItemInstance::plain(ItemType::Arrow)).collect()
    }

    fn arrows(world: &mut World, n: usize) -> Vec<Entity> {
        held::spawn_items(world, arrow_values(n))
    }

    /// Arrows collapse into one row wherever they sit in the list; other items
    /// keep a row each, in first-appearance order.
    #[test]
    fn stack_items_groups_only_stackables() {
        let mut items = arrow_values(1);
        items.push(ItemInstance::plain(ItemType::HealthPotion));
        items.extend(arrow_values(2));
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
        let mut items = arrows(&mut world, 4);
        let potion = held::spawn_item(&mut world, ItemInstance::plain(ItemType::HealthPotion));
        items.insert(2, potion);
        let chest = world.spawn((Position::new(1, 1), Container::chest(items, 0)));

        let mut events = EventQueue::new();
        assert!(take_item_from_container(&mut world, player, chest, 0, Some(&mut events)));

        let container = world.get::<&Container>(chest).unwrap();
        assert_eq!(container.items, vec![potion]);
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

    /// Two corpses and an item dropped on one tile are one loot session; a
    /// corpse next door and a closed chest on the tile are not part of it.
    #[test]
    fn loot_sources_cover_every_walkable_container_on_the_tile() {
        let mut world = World::new();
        let player = world.spawn((Position::new(0, 0), Inventory::new()));
        let tile = (3, 3);
        let at = Position::new(tile.0, tile.1);
        let two = arrows(&mut world, 2);
        let corpse_a = world.spawn((at, Container::corpse(two, 5)));
        let one = arrows(&mut world, 1);
        let corpse_b = world.spawn((at, Container::corpse(one, 0)));
        let apple = spawn_ground_item(&mut world, tile.0, tile.1, ItemInstance::plain(ItemType::Apple));
        let _empty_corpse = world.spawn((at, Container::corpse(Vec::new(), 0)));
        let nine = arrows(&mut world, 9);
        let _closed_chest = world.spawn((at, Container::chest(nine, 0), BlocksMovement));
        let next_door = arrows(&mut world, 1);
        let _neighbour = world.spawn((Position::new(4, 3), Container::corpse(next_door, 0)));

        // Opened via the apple, but corpses still list first.
        let sources = loot_sources(&world, Some(apple), tile);
        assert_eq!(
            sources,
            vec![LootSource::Container(corpse_a), LootSource::Container(corpse_b), LootSource::Ground(tile)]
        );

        take_all_from_sources(&mut world, player, &sources, None);
        let inventory = world.get::<&Inventory>(player).unwrap();
        assert_eq!(inventory.items.len(), 4);
        assert_eq!(inventory.gold, 5);
        drop(inventory);
        assert!(!held::on_ground(&world, apple), "the apple was picked up, entity and all");
        assert!(world.get::<&Inventory>(player).unwrap().items.contains(&apple));
    }

    /// The last loose item picked up mid-session; the rest of the tile must
    /// still be reachable through the stale primary.
    #[test]
    fn loot_sources_survive_the_primary_going() {
        let mut world = World::new();
        let at = Position::new(2, 2);
        let player = world.spawn((Position::new(0, 0), Inventory::new()));
        let apple = spawn_ground_item(&mut world, 2, 2, ItemInstance::plain(ItemType::Apple));
        let one = arrows(&mut world, 1);
        let corpse = world.spawn((at, Container::corpse(one, 0)));
        take_item(&mut world, player, LootSource::Ground((2, 2)), apple, None);

        assert_eq!(loot_sources(&world, Some(apple), (2, 2)), vec![LootSource::Container(corpse)]);
    }

    #[test]
    fn corpses_are_labelled_by_what_died() {
        use crate::components::Name;
        let mut world = World::new();
        let named = world.spawn((Position::new(0, 0), Container::corpse(Vec::new(), 0), Name::new("Goblin's corpse")));
        let anon = world.spawn((Position::new(0, 0), Container::corpse(Vec::new(), 0)));
        assert_eq!(loot_source_label(&world, LootSource::Container(named)), "Goblin's corpse");
        assert_eq!(loot_source_label(&world, LootSource::Container(anon)), "Remains");
        assert_eq!(loot_source_label(&world, LootSource::Ground((0, 0))), "On the ground");
    }
}
