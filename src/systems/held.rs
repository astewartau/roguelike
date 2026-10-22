//! Items are entities.
//!
//! Every item is an entity of its own, with an `ItemInstance` (what it is:
//! its kind, rarity, affixes, how well it's known) and whatever other
//! components it has. Where it is decides the rest:
//!
//! - **Held**: listed in something's `Inventory`, `Equipment` or
//!   `Container`, and with no `Position`. It goes wherever its holder goes,
//!   is saved with it (`component_registry` writes a holder's items out in
//!   full), and is despawned with it (`World::despawn`).
//! - **On the ground**: with a `Position`, a `VisualPosition` and its own
//!   `Sprite`, held by nothing. It's a thing in the world like any other: a
//!   lit torch lying there burns, as itself (`put_down`).
//!
//! Moving an item is moving the entity: take it out of one list (or off the
//! floor, `lift`) and into another. An item is never in two places.

use crate::components::{
    CausesBurning, Container, Equipment, Fuel, Inventory, ItemInstance, ItemType, OnFire, Position, Sprite,
    VisualPosition,
};
use crate::systems::item_defs;
use crate::world::World;
use hecs::Entity;

/// Make a new item entity, held by nothing yet. Put it in a holder's list,
/// or `put_down` it, straight away.
pub fn spawn_item(world: &mut World, item: ItemInstance) -> Entity {
    world.spawn((item,))
}

/// `spawn_item` for each.
pub fn spawn_items(world: &mut World, items: impl IntoIterator<Item = ItemInstance>) -> Vec<Entity> {
    items.into_iter().map(|item| spawn_item(world, item)).collect()
}

/// What item `e` is, if it's one.
pub fn item(world: &hecs::World, e: Entity) -> Option<hecs::Ref<'_, ItemInstance>> {
    world.get::<&ItemInstance>(e).ok()
}

/// What kind of item `e` is, if it's one.
pub fn kind(world: &hecs::World, e: Entity) -> Option<ItemType> {
    item(world, e).map(|i| i.kind)
}

/// A copy of what each of `items` is, in order, skipping anything that
/// isn't an item. For code that reads a list of items as values.
pub fn instances(world: &hecs::World, items: &[Entity]) -> Vec<ItemInstance> {
    items.iter().filter_map(|&e| item(world, e).map(|i| ItemInstance::clone(&i))).collect()
}

/// The items `holder` holds itself: its pack, what it wears, what's in it.
pub fn held_by(world: &hecs::World, holder: Entity) -> Vec<Entity> {
    let mut items = Vec::new();
    if let Ok(inventory) = world.get::<&Inventory>(holder) {
        items.extend(inventory.items.iter().copied());
    }
    if let Ok(equipment) = world.get::<&Equipment>(holder) {
        items.extend(equipment.equipped_items());
    }
    if let Ok(container) = world.get::<&Container>(holder) {
        items.extend(container.items.iter().copied());
    }
    items
}

/// Everything `holder` holds, and everything that holds, all the way down
/// (a bag in a pack, one day).
pub fn held_by_all(world: &hecs::World, holder: Entity) -> Vec<Entity> {
    let mut all = Vec::new();
    let mut open = held_by(world, holder);
    while let Some(e) = open.pop() {
        if all.contains(&e) || e == holder {
            continue;
        }
        all.push(e);
        open.extend(held_by(world, e));
    }
    all
}

/// Whether `e` is an item lying on the ground.
pub fn on_ground(world: &hecs::World, e: Entity) -> bool {
    world.get::<&ItemInstance>(e).is_ok() && world.get::<&Position>(e).is_ok()
}

/// The items lying on `tile`, oldest first.
pub fn on_ground_at(world: &hecs::World, tile: (i32, i32)) -> Vec<Entity> {
    let mut items: Vec<Entity> = world
        .query::<(&Position, &ItemInstance)>()
        .iter()
        .filter(|(_, (p, _))| (p.x, p.y) == tile)
        .map(|(e, _)| e)
        .collect();
    items.sort_by_key(|e| e.id());
    items
}

/// Lay held item `e` down on `(x, y)`: it takes a place and its own
/// picture, and whatever it does lying there starts (a lit torch burns).
/// The caller has already taken it out of whatever held it.
pub fn put_down(world: &mut World, e: Entity, x: i32, y: i32) {
    let Some(kind) = kind(world, e) else {
        return;
    };
    let at = Position::new(x, y);
    let _ = world.insert(e, (at, VisualPosition::from_position(&at), Sprite::from_ref(item_defs::get_def(kind).sprite)));
    if let Some(fuel) = item_defs::burns_lying_down(kind) {
        let base = fuel.burn_time;
        let _ = world.insert_one(e, fuel);
        let remaining = crate::systems::fire::burn_time(base, e, (x, y));
        let _ = world.insert_one(e, OnFire { remaining });
    }
}

/// Make a new item and lay it on `(x, y)`.
pub fn drop_new(world: &mut World, x: i32, y: i32, item: ItemInstance) -> Entity {
    let e = spawn_item(world, item);
    put_down(world, e, x, y);
    e
}

/// Pick item `e` up off the ground: it loses its place and picture, and
/// stops doing what it did lying there (a torch's flame goes with it, to be
/// a light in your hand when you hold it). The caller then puts it in a
/// holder's list.
pub fn lift(world: &mut World, e: Entity) {
    let _ = world.remove_one::<Position>(e);
    let _ = world.remove_one::<VisualPosition>(e);
    let _ = world.remove_one::<Sprite>(e);
    let _ = world.remove_one::<OnFire>(e);
    let _ = world.remove_one::<Fuel>(e);
    let _ = world.remove_one::<CausesBurning>(e);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_lit_torch_burns_as_itself_on_the_ground_and_goes_out_when_lifted() {
        let mut world = World::new();
        let torch = drop_new(&mut world, 3, 4, ItemInstance::plain(ItemType::Torch));
        assert!(on_ground(&world, torch));
        assert!(world.get::<&OnFire>(torch).is_ok(), "it burns where it lies");
        assert_eq!(on_ground_at(&world, (3, 4)), vec![torch]);
        assert!(!crate::systems::light::light_emitters(&world).is_empty(), "and lights the floor");

        lift(&mut world, torch);
        assert!(!on_ground(&world, torch));
        assert!(world.get::<&OnFire>(torch).is_err(), "the flame went with it");
        assert!(crate::systems::light::light_emitters(&world).is_empty(), "nothing burns on the floor");
        assert_eq!(kind(&world, torch), Some(ItemType::Torch), "it's still a lit torch");

        // An apple just lies there.
        let apple = drop_new(&mut world, 1, 1, ItemInstance::plain(ItemType::Apple));
        assert!(world.get::<&OnFire>(apple).is_err());
    }

    #[test]
    fn what_a_holder_holds_goes_with_it() {
        let mut world = World::new();
        let inner = spawn_items(&mut world, [ItemInstance::plain(ItemType::Apple)]);
        let chest = world.spawn((Position::new(0, 0), Container::chest(inner.clone(), 0)));
        let worn = spawn_item(&mut world, ItemInstance::plain(ItemType::Helmet));
        let mut equipment = Equipment::empty();
        equipment.head = Some(worn);
        let carried = spawn_items(&mut world, [ItemInstance::plain(ItemType::Arrow)]);
        let mut inventory = Inventory::new();
        inventory.items = carried.clone();
        let bearer = world.spawn((Position::new(1, 0), inventory, equipment));
        assert_eq!(held_by(&world, chest), inner);
        let mut all = held_by_all(&world, bearer);
        all.sort();
        let mut expected = vec![carried[0], worn];
        expected.sort();
        assert_eq!(all, expected);

        world.despawn(bearer).unwrap();
        assert!(!world.contains(worn) && !world.contains(carried[0]), "despawned with its bearer");
        assert!(world.contains(inner[0]), "the chest's apple is untouched");
    }
}
