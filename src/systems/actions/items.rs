//! Equipping, unequipping, and dropping items.

use hecs::Entity;
use crate::world::World;

use crate::components::{
    Equipment, EquippedWeapon, Health, Inventory, ItemInstance, ItemType, RangedWeapon, Weapon,
};
use crate::events::{EventQueue, GameEvent};
use crate::queries;
use crate::systems::held;

use super::ActionResult;

/// Apply a MaxHealth affix delta from equipping/unequipping gear.
///
/// Equipping raises max health without healing (no equip/unequip healing
/// exploit); unequipping lowers it and clamps current health down (min 1 so a
/// swap can never kill outright).
fn adjust_max_health(world: &mut World, entity: Entity, delta: i32) {
    if delta == 0 {
        return;
    }
    if let Ok(mut health) = world.get::<&mut Health>(entity) {
        health.max = (health.max + delta).max(1);
        if health.current > health.max {
            health.current = health.max;
        }
        if delta < 0 && health.current < 1 {
            health.current = 1;
        }
    }
}

/// Build the combat `EquippedWeapon` for a weapon item kind.
/// Returns `None` for non-weapon kinds.
fn equipped_weapon_for_kind(kind: ItemType) -> Option<EquippedWeapon> {
    Some(match kind {
        ItemType::Sword => EquippedWeapon::Melee(Weapon::sword()),
        ItemType::Dagger => EquippedWeapon::Melee(Weapon::dagger()),
        ItemType::Staff => EquippedWeapon::Melee(Weapon::staff()),
        ItemType::Bow => EquippedWeapon::Ranged(RangedWeapon::bow()),
        _ => return None,
    })
}

/// The item kind an equipped weapon is, if it's a real item (not claws).
fn equipped_weapon_item_kind(weapon: &EquippedWeapon) -> Option<ItemType> {
    match weapon {
        EquippedWeapon::Melee(w) => w.item,
        EquippedWeapon::Ranged(_) => Some(ItemType::Bow),
    }
}

/// The equipment field behind a worn slot.
fn slot_mut(
    equipment: &mut Equipment,
    slot: crate::systems::item_defs::ArmorSlot,
) -> &mut Option<Entity> {
    use crate::systems::item_defs::ArmorSlot;
    match slot {
        ArmorSlot::Body => &mut equipment.body,
        ArmorSlot::Head => &mut equipment.head,
        ArmorSlot::Ring => &mut equipment.ring,
        ArmorSlot::Amulet => &mut equipment.amulet,
        ArmorSlot::OffHand => &mut equipment.off_hand,
    }
}

/// The item kind of the equipped weapon, if it's a real item (not claws).
fn equipped_weapon_kind(world: &World, equipment: &Equipment) -> Option<ItemType> {
    equipment
        .weapon_source
        .and_then(|e| held::kind(world, e))
        .or_else(|| equipment.weapon.as_ref().and_then(equipped_weapon_item_kind))
}

fn max_health_bonus(world: &World, item: Option<Entity>) -> i32 {
    item.and_then(|e| held::item(world, e).map(|i| i.max_health_bonus())).unwrap_or(0)
}

/// Take the equipped weapon off `entity`, as an item: its backing item, or
/// (a weapon set straight on `Equipment`, with nothing recorded behind it)
/// a new plain one of its kind. None if it holds no real weapon (claws).
fn take_weapon(world: &mut World, entity: Entity) -> Option<Entity> {
    let (source, kind) = {
        let mut equipment = world.get::<&mut Equipment>(entity).ok()?;
        equipment.weapon.as_ref()?;
        let kind = equipment.weapon.as_ref().and_then(equipped_weapon_item_kind);
        (equipment.weapon_source.take(), kind)
    };
    let item = match source {
        Some(item) => item,
        None => held::spawn_item(world, ItemInstance::plain(kind?)),
    };
    if let Ok(mut equipment) = world.get::<&mut Equipment>(entity) {
        equipment.weapon = None;
    }
    Some(item)
}

/// The item at `item_index` in `entity`'s pack.
fn pack_item(world: &World, entity: Entity, item_index: usize) -> Option<Entity> {
    world.get::<&Inventory>(entity).ok()?.items.get(item_index).copied()
}

/// Put whatever is in the off hand back in the pack, to free the hand for a
/// two-handed weapon.
fn stow_off_hand(world: &mut World, entity: Entity, events: &mut EventQueue) {
    let held = world.get::<&mut Equipment>(entity).ok().and_then(|mut e| e.off_hand.take());
    if let Some(item) = held {
        adjust_max_health(world, entity, -max_health_bonus(world, Some(item)));
        if let Some(kind) = held::kind(world, item) {
            events.push(GameEvent::ItemStowed { entity, item: kind });
        }
        crate::systems::inventory::add_to_inventory(world, entity, item);
    }
}

/// Put a two-handed weapon back in the pack, to free a hand for the off-hand
/// slot. One-handed weapons stay where they are.
fn stow_two_handed_weapon(world: &mut World, entity: Entity, events: &mut EventQueue) {
    let kind = world.get::<&Equipment>(entity).ok().and_then(|e| equipped_weapon_kind(world, &e));
    if kind.is_some_and(crate::systems::item_defs::is_two_handed) && apply_unequip_weapon(world, entity) == ActionResult::Completed {
        if let Some(kind) = kind {
            events.push(GameEvent::ItemStowed { entity, item: kind });
        }
    }
}

/// Apply equip weapon action - equips a weapon from inventory
pub fn apply_equip_weapon(
    world: &mut World,
    entity: Entity,
    item_index: usize,
    events: &mut EventQueue,
) -> ActionResult {
    let Some(new_item) = pack_item(world, entity, item_index) else {
        return ActionResult::Invalid;
    };
    let Some(new_kind) = held::kind(world, new_item) else {
        return ActionResult::Invalid;
    };

    // Create the combat weapon for this kind
    let Some(new_weapon) = equipped_weapon_for_kind(new_kind) else {
        return ActionResult::Invalid; // Not a weapon
    };
    if world.get::<&Equipment>(entity).is_err() {
        return ActionResult::Invalid;
    }

    // A two-handed weapon needs the off hand free.
    if crate::systems::item_defs::is_two_handed(new_kind) {
        stow_off_hand(world, entity, events);
    }

    // Take the previously equipped weapon off, back to the pack.
    let old = take_weapon(world, entity);

    // Swap MaxHealth affix contributions (new gear on, old gear off)
    let health_delta = max_health_bonus(world, Some(new_item)) - max_health_bonus(world, old);

    crate::systems::inventory::remove_from_inventory(world, entity, new_item);
    if let Some(old) = old {
        crate::systems::inventory::add_to_inventory(world, entity, old);
    }

    // Equip the new weapon, recording the item behind it
    if let Ok(mut equipment) = world.get::<&mut Equipment>(entity) {
        equipment.weapon = Some(new_weapon);
        equipment.weapon_source = Some(new_item);
    }

    adjust_max_health(world, entity, health_delta);
    crate::systems::torches::sync_held_torch(world, entity);

    ActionResult::Completed
}

/// Apply equip armor action - equips an armor piece from inventory into its
/// body/head slot, returning any previously equipped piece to inventory.
pub fn apply_equip_armor(
    world: &mut World,
    entity: Entity,
    item_index: usize,
    events: &mut EventQueue,
) -> ActionResult {
    use crate::systems::item_defs::{armor_slot, ArmorSlot};

    let Some(new_item) = pack_item(world, entity, item_index) else {
        return ActionResult::Invalid;
    };
    // Determine which slot this armor occupies
    let Some(slot) = held::kind(world, new_item).and_then(armor_slot) else {
        return ActionResult::Invalid; // Not armor
    };
    if world.get::<&Equipment>(entity).is_err() {
        return ActionResult::Invalid;
    }

    // Taking something in the off hand means putting down a two-hander.
    if slot == ArmorSlot::OffHand {
        stow_two_handed_weapon(world, entity, events);
    }

    // Swap the previously equipped piece (if any) out of the slot
    let old_piece = world.get::<&mut Equipment>(entity).ok().and_then(|mut e| slot_mut(&mut e, slot).take());

    // Swap MaxHealth affix contributions (new gear on, old gear off)
    let health_delta = max_health_bonus(world, Some(new_item)) - max_health_bonus(world, old_piece);

    crate::systems::inventory::remove_from_inventory(world, entity, new_item);
    if let Some(old) = old_piece {
        crate::systems::inventory::add_to_inventory(world, entity, old);
    }
    if let Ok(mut equipment) = world.get::<&mut Equipment>(entity) {
        *slot_mut(&mut equipment, slot) = Some(new_item);
    }

    adjust_max_health(world, entity, health_delta);
    crate::systems::torches::sync_held_torch(world, entity);

    ActionResult::Completed
}

/// Apply unequip armor action - moves the armor in the given slot to inventory.
pub fn apply_unequip_armor(
    world: &mut World,
    entity: Entity,
    slot: crate::systems::item_defs::ArmorSlot,
) -> ActionResult {
    let piece = world.get::<&mut Equipment>(entity).ok().and_then(|mut e| slot_mut(&mut e, slot).take());
    match piece {
        Some(item) => {
            adjust_max_health(world, entity, -max_health_bonus(world, Some(item)));
            crate::systems::inventory::add_to_inventory(world, entity, item);
            crate::systems::torches::sync_held_torch(world, entity);
            ActionResult::Completed
        }
        None => ActionResult::Invalid,
    }
}

/// Apply unequip weapon action - moves current weapon to inventory
pub fn apply_unequip_weapon(
    world: &mut World,
    entity: Entity,
) -> ActionResult {
    let Some(item) = take_weapon(world, entity) else {
        return ActionResult::Invalid; // Nothing to unequip
    };
    adjust_max_health(world, entity, -max_health_bonus(world, Some(item)));
    crate::systems::inventory::add_to_inventory(world, entity, item);
    ActionResult::Completed
}

/// Apply drop item action - takes the item out of the pack and lays it on
/// the ground
pub fn apply_drop_item(
    world: &mut World,
    entity: Entity,
    item_index: usize,
    events: &mut EventQueue,
) -> ActionResult {
    let Some((x, y)) = queries::get_entity_position(world, entity) else {
        return ActionResult::Invalid;
    };
    let Some(item) = pack_item(world, entity, item_index) else {
        return ActionResult::Invalid;
    };
    let Some(item_kind) = held::kind(world, item) else {
        return ActionResult::Invalid;
    };

    crate::systems::inventory::remove_from_inventory(world, entity, item);
    held::put_down(world, item, x, y);

    events.push(GameEvent::ItemDropped {
        entity,
        item: item_kind,
        position: (x, y),
    });

    ActionResult::Completed
}

/// Apply drop equipped weapon action - unequips and drops weapon on ground
pub fn apply_drop_equipped_weapon(
    world: &mut World,
    entity: Entity,
    events: &mut EventQueue,
) -> ActionResult {
    let Some((x, y)) = queries::get_entity_position(world, entity) else {
        return ActionResult::Invalid;
    };
    let Some(item) = take_weapon(world, entity) else {
        return ActionResult::Invalid; // Nothing to drop
    };
    let Some(item_kind) = held::kind(world, item) else {
        return ActionResult::Invalid;
    };
    adjust_max_health(world, entity, -max_health_bonus(world, Some(item)));
    held::put_down(world, item, x, y);

    events.push(GameEvent::ItemDropped {
        entity,
        item: item_kind,
        position: (x, y),
    });

    ActionResult::Completed
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::components::{Affix, Rarity};
    use crate::world::World;

    #[test]
    fn test_max_health_affix_applies_symmetrically_without_healing() {
        let mut world = World::new();

        let armor = ItemInstance {
            kind: ItemType::LeatherArmor,
            rarity: Rarity::Magic,
            affixes: vec![Affix::MaxHealth(5)],
            name: None,
            identified: true,
            identify_progress: 0.0,
        };

        let mut inventory = Inventory::new();
        inventory.items.push(held::spawn_item(&mut world, armor));

        let mut health = Health::new(30);
        health.current = 20; // wounded

        let entity = world.spawn((inventory, Equipment::empty(), health));

        // Equip: max rises, current unchanged (no healing exploit)
        assert_eq!(apply_equip_armor(&mut world, entity, 0, &mut EventQueue::new()), ActionResult::Completed);
        {
            let h = world.get::<&Health>(entity).expect("health");
            assert_eq!(h.max, 35);
            assert_eq!(h.current, 20);
        }

        // Unequip: max drops back, current clamped to max
        let slot = crate::systems::item_defs::ArmorSlot::Body;
        assert_eq!(apply_unequip_armor(&mut world, entity, slot), ActionResult::Completed);
        {
            let h = world.get::<&Health>(entity).expect("health");
            assert_eq!(h.max, 30);
            assert_eq!(h.current, 20);
        }

        // Re-equip then take current up to the boosted max, then unequip:
        // current clamps down to the new max.
        assert_eq!(apply_equip_armor(&mut world, entity, 0, &mut EventQueue::new()), ActionResult::Completed);
        if let Ok(mut h) = world.get::<&mut Health>(entity) {
            h.current = h.max; // 35
        }
        assert_eq!(apply_unequip_armor(&mut world, entity, slot), ActionResult::Completed);
        {
            let h = world.get::<&Health>(entity).expect("health");
            assert_eq!(h.max, 30);
            assert_eq!(h.current, 30);
        }
    }
}
