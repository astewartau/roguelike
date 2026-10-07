//! Room furniture interactions: fountains, altars, and shrines.
//!
//! Furniture is placed by dungeon generation (roughly one piece per three
//! rooms, never in shops) and used by bumping into it or with
//! Ctrl+direction — the same interaction path as doors and braziers.
//!
//! - **Fountain**: drink for a random outcome (full heal / restore hunger /
//!   random buff / mild debuff). One use, then dry.
//! - **Altar**: opens a small sacrifice window (see `ui::altar_window`);
//!   sacrificing an inventory item rolls blessing vs curse with odds scaled
//!   by the item's value. Reusable.
//! - **Shrine**: instantly identifies everything carried/equipped and grants
//!   a Protected ward. One use, then inert.

use hecs::{Entity, World};
use rand::Rng;

use crate::components::{
    EffectType, Equipment, Furniture, FurnitureKind, Health, Hunger, Inventory, ItemInstance,
    Rarity, SpriteTint, Stats,
};
use crate::constants::*;
use crate::events::{EventQueue, FountainOutcome, GameEvent};
use crate::systems::actions::ActionResult;
use crate::systems::effects;

/// Use a piece of furniture. `user` should be the player (call sites gate on
/// the `Player` component). Returns `Invalid` if the entity isn't furniture.
pub fn use_furniture(
    world: &mut World,
    user: Entity,
    furniture: Entity,
    events: &mut EventQueue,
) -> ActionResult {
    let Ok(kind) = world.get::<&Furniture>(furniture).map(|f| f.kind) else {
        return ActionResult::Invalid;
    };
    match kind {
        FurnitureKind::Fountain => use_fountain(world, user, furniture, events),
        FurnitureKind::Shrine => use_shrine(world, user, furniture, events),
        FurnitureKind::Altar => {
            // The altar interaction is a UI prompt: pick an inventory item to
            // sacrifice (see ui::altar_window / perform_altar_sacrifice).
            events.push(GameEvent::AltarOpened { altar: furniture, player: user });
            ActionResult::Completed
        }
    }
}

/// Flip a one-use piece to spent and grey out its sprite. Also used when
/// restoring a saved floor.
pub fn mark_spent(world: &mut World, furniture: Entity) {
    if let Ok(mut f) = world.get::<&mut Furniture>(furniture) {
        f.used = true;
    }
    let (r, g, b) = FURNITURE_SPENT_TINT;
    let _ = world.insert_one(furniture, SpriteTint { r, g, b });
}

/// Drink from a fountain: 35% full heal, 25% restore hunger, 20% random buff,
/// 20% mild debuff. One use, then dry.
fn use_fountain(
    world: &mut World,
    user: Entity,
    fountain: Entity,
    events: &mut EventQueue,
) -> ActionResult {
    let used = world.get::<&Furniture>(fountain).map(|f| f.used).unwrap_or(true);
    if used {
        events.push(GameEvent::FountainUsed { entity: user, outcome: FountainOutcome::Dry });
        return ActionResult::Completed;
    }

    let mut rng = rand::thread_rng();
    let roll: f64 = rng.gen();
    let outcome = if roll < FOUNTAIN_HEAL_CHANCE {
        if let Ok(mut health) = world.get::<&mut Health>(user) {
            health.current = health.max;
        }
        FountainOutcome::Heal
    } else if roll < FOUNTAIN_HEAL_CHANCE + FOUNTAIN_FOOD_CHANCE {
        if let Ok(mut hunger) = world.get::<&mut Hunger>(user) {
            hunger.eat(HUNGER_MAX);
        }
        FountainOutcome::Food
    } else if roll < FOUNTAIN_HEAL_CHANCE + FOUNTAIN_FOOD_CHANCE + FOUNTAIN_BUFF_CHANCE {
        let buffs = [
            EffectType::Regenerating,
            EffectType::Protected,
            EffectType::Strengthened,
        ];
        let buff = buffs[rng.gen_range(0..buffs.len())];
        effects::add_effect_to_entity(world, user, buff, FOUNTAIN_BUFF_DURATION);
        FountainOutcome::Buff(buff)
    } else if rng.gen_bool(0.5) {
        effects::add_effect_to_entity(world, user, EffectType::Confused, FOUNTAIN_CONFUSED_DURATION);
        FountainOutcome::Bad(EffectType::Confused)
    } else {
        effects::add_effect_to_entity(world, user, EffectType::Slowed, FOUNTAIN_SLOWED_DURATION);
        FountainOutcome::Bad(EffectType::Slowed)
    };

    mark_spent(world, fountain);
    events.push(GameEvent::FountainUsed { entity: user, outcome });
    ActionResult::Completed
}

/// Touch a shrine: identifies everything carried/equipped and grants a
/// Protected ward. One use, then inert.
fn use_shrine(
    world: &mut World,
    user: Entity,
    shrine: Entity,
    events: &mut EventQueue,
) -> ActionResult {
    let used = world.get::<&Furniture>(shrine).map(|f| f.used).unwrap_or(true);
    if used {
        events.push(GameEvent::ShrineUsed { entity: user, fresh: false });
        return ActionResult::Completed;
    }

    identify_all_carried(world, user);
    effects::add_effect_to_entity(world, user, EffectType::Protected, SHRINE_PROTECT_DURATION);

    mark_spent(world, shrine);
    events.push(GameEvent::ShrineUsed { entity: user, fresh: true });
    ActionResult::Completed
}

/// Instantly identify every inventory and equipped item. Returns how many
/// items were newly identified.
pub fn identify_all_carried(world: &mut World, entity: Entity) -> usize {
    let mut count = 0;
    let mut identify = |inst: &mut ItemInstance| {
        if !inst.identified {
            inst.identified = true;
            count += 1;
        }
    };
    if let Ok(mut inv) = world.get::<&mut Inventory>(entity) {
        for inst in inv.items.iter_mut() {
            identify(inst);
        }
    }
    if let Ok(mut eq) = world.get::<&mut Equipment>(entity) {
        for inst in eq.equipped_instances_mut() {
            identify(inst);
        }
    }
    count
}

/// Sacrifice value of an item: base price scaled by rarity.
pub fn altar_item_value(inst: &ItemInstance) -> u32 {
    let mult = match inst.rarity {
        Rarity::Common => 1,
        Rarity::Magic => 2,
        Rarity::Rare => 4,
        Rarity::Legendary => 8,
    };
    crate::systems::item_defs::get_price(inst.kind) * mult
}

/// Odds that a sacrifice of the given value yields a blessing:
/// `clamp(0.25 + value / 400, 0.25, 0.9)`.
pub fn altar_blessing_chance(value: u32) -> f64 {
    (ALTAR_BLESS_BASE + value as f64 / ALTAR_BLESS_VALUE_DIVISOR)
        .clamp(ALTAR_BLESS_BASE, ALTAR_BLESS_MAX)
}

/// Sacrifice the inventory item at `item_index` on an altar. The item is
/// consumed; better items give better blessing odds. Blessing: permanent +1
/// to a random stat, or identify-all-carried (falls back to the stat boost if
/// everything is already identified). Curse: a random debuff for 60 seconds.
pub fn perform_altar_sacrifice(
    world: &mut World,
    player: Entity,
    item_index: usize,
    events: &mut EventQueue,
    rng: &mut impl Rng,
) {
    // Look up (and price) the offering.
    let offering: Option<(String, u32)> = world
        .get::<&Inventory>(player)
        .ok()
        .and_then(|inv| {
            inv.items
                .get(item_index)
                .map(|inst| (inst.display_name(), altar_item_value(inst)))
        });
    let Some((item_name, value)) = offering else {
        return;
    };

    // The altar always takes its due.
    crate::systems::remove_item_from_inventory(world, player, item_index);

    let blessed = rng.gen_bool(altar_blessing_chance(value));

    let detail = if blessed {
        // 50/50: permanent +1 random stat, or identify everything carried.
        // Identification falls back to the stat boost when there is nothing
        // left to identify.
        let try_identify = rng.gen_bool(0.5);
        if try_identify && identify_all_carried(world, player) > 0 {
            "Divine insight floods your mind — all your possessions are revealed!".to_string()
        } else {
            let stat = rng.gen_range(0..3);
            let name = if let Ok(mut stats) = world.get::<&mut Stats>(player) {
                match stat {
                    0 => {
                        stats.strength += 1;
                        "Strength"
                    }
                    1 => {
                        stats.agility += 1;
                        "Agility"
                    }
                    _ => {
                        stats.intelligence += 1;
                        "Intelligence"
                    }
                }
            } else {
                "resolve"
            };
            format!("Power surges through you — permanent +1 {name}!")
        }
    } else {
        // The offering displeases: a lingering debuff.
        if rng.gen_bool(0.5) {
            effects::add_effect_to_entity(world, player, EffectType::Slowed, ALTAR_CURSE_DURATION);
            "The altar rejects your offering — your limbs grow heavy.".to_string()
        } else {
            effects::add_effect_to_entity(world, player, EffectType::Confused, ALTAR_CURSE_DURATION);
            "The altar rejects your offering — your mind clouds.".to_string()
        }
    };

    events.push(GameEvent::AltarSacrificed { item_name, blessed, detail });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::components::{Affix, ItemType};

    #[test]
    fn test_altar_blessing_chance_clamps() {
        // Worthless offering: floor odds.
        assert!((altar_blessing_chance(0) - ALTAR_BLESS_BASE).abs() < 1e-9);
        // Mid-value offering scales linearly: 100 gold -> 0.5.
        assert!((altar_blessing_chance(100) - 0.5).abs() < 1e-9);
        // Priceless offering caps below certainty.
        assert!((altar_blessing_chance(100_000) - ALTAR_BLESS_MAX).abs() < 1e-9);
        // Monotonic in value.
        assert!(altar_blessing_chance(200) > altar_blessing_chance(50));
    }

    #[test]
    fn test_altar_item_value_scales_with_rarity() {
        let common = ItemInstance::plain(ItemType::Sword);
        let legendary = ItemInstance {
            kind: ItemType::Sword,
            rarity: Rarity::Legendary,
            affixes: vec![Affix::Damage(2)],
            name: None,
            identified: true,
            identify_progress: 0.0,
        };
        assert_eq!(altar_item_value(&legendary), altar_item_value(&common) * 8);
    }

    #[test]
    fn test_identify_all_carried_covers_inventory_and_equipment() {
        let mut world = World::new();
        let mut inv = Inventory::new();
        inv.items.push(ItemInstance {
            kind: ItemType::Sword,
            rarity: Rarity::Magic,
            affixes: vec![Affix::Damage(2)],
            name: None,
            identified: false,
            identify_progress: 0.0,
        });
        let mut eq = Equipment::empty();
        eq.ring = Some(ItemInstance {
            kind: ItemType::Ring,
            rarity: Rarity::Magic,
            affixes: vec![Affix::Agility(1)],
            name: None,
            identified: false,
            identify_progress: 0.0,
        });
        let player = world.spawn((inv, eq));

        assert_eq!(identify_all_carried(&mut world, player), 2);
        // Second pass finds nothing left.
        assert_eq!(identify_all_carried(&mut world, player), 0);
    }
}
