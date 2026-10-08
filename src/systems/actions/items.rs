//! Equipping, unequipping, and dropping items.

use hecs::{Entity, World};

use crate::components::{
    Equipment, EquippedWeapon, Health, Inventory, ItemInstance, ItemType, RangedWeapon, Weapon,
};
use crate::events::{EventQueue, GameEvent};
use crate::queries;

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

/// Reconstruct a plain `ItemInstance` from an equipped weapon by its name.
/// Used as a fallback when no `weapon_source` is recorded (e.g. the player's
/// starting weapon set directly on `Equipment`). Returns `None` for enemy
/// weapons like claws, which are not real items.
fn equipped_weapon_to_instance(weapon: &EquippedWeapon) -> Option<ItemInstance> {
    let kind = match weapon {
        EquippedWeapon::Melee(w) => match w.name.as_str() {
            "Sword" => ItemType::Sword,
            "Dagger" => ItemType::Dagger,
            "Staff" => ItemType::Staff,
            _ => return None,
        },
        EquippedWeapon::Ranged(_) => ItemType::Bow,
    };
    Some(ItemInstance::plain(kind))
}

/// Apply equip weapon action - equips a weapon from inventory
pub fn apply_equip_weapon(
    world: &mut World,
    entity: Entity,
    item_index: usize,
) -> ActionResult {
    // Get the full instance from inventory (carries rarity/affixes)
    let new_instance = {
        let Ok(inventory) = world.get::<&Inventory>(entity) else {
            return ActionResult::Invalid;
        };
        if item_index >= inventory.items.len() {
            return ActionResult::Invalid;
        }
        inventory.items[item_index].clone()
    };

    // Create the combat weapon for this kind
    let Some(new_weapon) = equipped_weapon_for_kind(new_instance.kind) else {
        return ActionResult::Invalid; // Not a weapon
    };

    // Take the previously equipped weapon's source instance to return to inventory
    // (falling back to a plain reconstruction for the starting weapon).
    let old_source = {
        let Ok(mut equipment) = world.get::<&mut Equipment>(entity) else {
            return ActionResult::Invalid;
        };
        match equipment.weapon_source.take() {
            Some(inst) => Some(inst),
            None => equipment.weapon.as_ref().and_then(equipped_weapon_to_instance),
        }
    };

    // Swap MaxHealth affix contributions (new gear on, old gear off)
    let health_delta = new_instance.max_health_bonus()
        - old_source.as_ref().map_or(0, |o| o.max_health_bonus());

    // Remove the item we're equipping from inventory
    crate::systems::items::remove_item_from_inventory(world, entity, item_index);

    // Add the old weapon back to inventory if there was one
    if let Some(old) = old_source {
        crate::systems::inventory::add_item_to_inventory(world, entity, old);
    }

    // Equip the new weapon, recording its source instance
    if let Ok(mut equipment) = world.get::<&mut Equipment>(entity) {
        equipment.weapon = Some(new_weapon);
        equipment.weapon_source = Some(new_instance);
    }

    adjust_max_health(world, entity, health_delta);

    ActionResult::Completed
}

/// Apply equip armor action - equips an armor piece from inventory into its
/// body/head slot, returning any previously equipped piece to inventory.
pub fn apply_equip_armor(
    world: &mut World,
    entity: Entity,
    item_index: usize,
) -> ActionResult {
    use crate::systems::item_defs::{armor_slot, ArmorSlot};

    // Get the full instance from inventory (carries rarity/affixes)
    let new_instance = {
        let Ok(inventory) = world.get::<&Inventory>(entity) else {
            return ActionResult::Invalid;
        };
        if item_index >= inventory.items.len() {
            return ActionResult::Invalid;
        }
        inventory.items[item_index].clone()
    };

    // Determine which slot this armor occupies
    let Some(slot) = armor_slot(new_instance.kind) else {
        return ActionResult::Invalid; // Not armor
    };

    // Swap the previously equipped piece (if any) out of the slot
    let old_piece = {
        let Ok(mut equipment) = world.get::<&mut Equipment>(entity) else {
            return ActionResult::Invalid;
        };
        match slot {
            ArmorSlot::Body => equipment.body.take(),
            ArmorSlot::Head => equipment.head.take(),
            ArmorSlot::Ring => equipment.ring.take(),
            ArmorSlot::Amulet => equipment.amulet.take(),
        }
    };

    // Swap MaxHealth affix contributions (new gear on, old gear off)
    let health_delta = new_instance.max_health_bonus()
        - old_piece.as_ref().map_or(0, |o| o.max_health_bonus());

    // Remove the item we're equipping from inventory
    crate::systems::items::remove_item_from_inventory(world, entity, item_index);

    // Return the old piece to inventory
    if let Some(old) = old_piece {
        crate::systems::inventory::add_item_to_inventory(world, entity, old);
    }

    // Equip the new piece into its slot
    if let Ok(mut equipment) = world.get::<&mut Equipment>(entity) {
        match slot {
            ArmorSlot::Body => equipment.body = Some(new_instance),
            ArmorSlot::Head => equipment.head = Some(new_instance),
            ArmorSlot::Ring => equipment.ring = Some(new_instance),
            ArmorSlot::Amulet => equipment.amulet = Some(new_instance),
        }
    }

    adjust_max_health(world, entity, health_delta);

    ActionResult::Completed
}

/// Apply unequip armor action - moves the armor in the given slot to inventory.
pub fn apply_unequip_armor(
    world: &mut World,
    entity: Entity,
    slot: crate::systems::item_defs::ArmorSlot,
) -> ActionResult {
    use crate::systems::item_defs::ArmorSlot;

    let piece = {
        let Ok(mut equipment) = world.get::<&mut Equipment>(entity) else {
            return ActionResult::Invalid;
        };
        match slot {
            ArmorSlot::Body => equipment.body.take(),
            ArmorSlot::Head => equipment.head.take(),
            ArmorSlot::Ring => equipment.ring.take(),
            ArmorSlot::Amulet => equipment.amulet.take(),
        }
    };

    match piece {
        Some(instance) => {
            adjust_max_health(world, entity, -instance.max_health_bonus());
            crate::systems::inventory::add_item_to_inventory(world, entity, instance);
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
    // Get the currently equipped weapon's source instance (with affixes), or
    // reconstruct a plain instance for the starting weapon.
    let weapon_item = {
        let Ok(mut equipment) = world.get::<&mut Equipment>(entity) else {
            return ActionResult::Invalid;
        };
        if equipment.weapon.is_none() {
            return ActionResult::Invalid; // Nothing to unequip
        }
        match equipment.weapon_source.take() {
            Some(inst) => inst,
            None => match equipment.weapon.as_ref().and_then(equipped_weapon_to_instance) {
                Some(inst) => inst,
                None => return ActionResult::Invalid,
            },
        }
    };

    adjust_max_health(world, entity, -weapon_item.max_health_bonus());

    // Add the weapon to inventory
    crate::systems::inventory::add_item_to_inventory(world, entity, weapon_item);

    // Remove weapon from equipment
    if let Ok(mut equipment) = world.get::<&mut Equipment>(entity) {
        equipment.weapon = None;
    }

    ActionResult::Completed
}

/// Apply drop item action - removes item from inventory and spawns on ground
pub fn apply_drop_item(
    world: &mut World,
    entity: Entity,
    item_index: usize,
    events: &mut EventQueue,
) -> ActionResult {
    // Get entity position
    let (x, y) = match queries::get_entity_position(world, entity) {
        Some(p) => p,
        None => return ActionResult::Invalid,
    };

    // Get the full instance from inventory (carries rarity/affixes)
    let item_instance = {
        let Ok(inventory) = world.get::<&Inventory>(entity) else {
            return ActionResult::Invalid;
        };
        if item_index >= inventory.items.len() {
            return ActionResult::Invalid;
        }
        inventory.items[item_index].clone()
    };
    let item_kind = item_instance.kind;

    // Remove from inventory
    crate::systems::items::remove_item_from_inventory(world, entity, item_index);

    // Spawn on ground
    crate::systems::inventory::spawn_ground_item(world, x, y, item_instance);

    // Emit event
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
    // Get entity position
    let (x, y) = match queries::get_entity_position(world, entity) {
        Some(p) => p,
        None => return ActionResult::Invalid,
    };

    // Get the currently equipped weapon's source instance (with affixes), or
    // reconstruct a plain instance for the starting weapon.
    let weapon_instance = {
        let Ok(mut equipment) = world.get::<&mut Equipment>(entity) else {
            return ActionResult::Invalid;
        };
        if equipment.weapon.is_none() {
            return ActionResult::Invalid; // Nothing to drop
        }
        match equipment.weapon_source.take() {
            Some(inst) => inst,
            None => match equipment.weapon.as_ref().and_then(equipped_weapon_to_instance) {
                Some(inst) => inst,
                None => return ActionResult::Invalid,
            },
        }
    };
    let item_kind = weapon_instance.kind;

    adjust_max_health(world, entity, -weapon_instance.max_health_bonus());

    // Remove weapon from equipment
    if let Ok(mut equipment) = world.get::<&mut Equipment>(entity) {
        equipment.weapon = None;
    }

    // Spawn on ground
    crate::systems::inventory::spawn_ground_item(world, x, y, weapon_instance);

    // Emit event
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
    use hecs::World;

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
        inventory.items.push(armor);

        let mut health = Health::new(30);
        health.current = 20; // wounded

        let entity = world.spawn((inventory, Equipment::empty(), health));

        // Equip: max rises, current unchanged (no healing exploit)
        assert_eq!(apply_equip_armor(&mut world, entity, 0), ActionResult::Completed);
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
        assert_eq!(apply_equip_armor(&mut world, entity, 0), ActionResult::Completed);
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
