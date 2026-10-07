//! Passive item identification.
//!
//! Magic+ gear drops unidentified: the tooltip shows its rarity but its affix
//! components read as "???" (and Legendary names stay hidden). Simply carrying
//! an item — in the inventory or equipped — identifies it after a while,
//! faster with higher Intelligence:
//!
//! `identify_seconds = IDENTIFY_BASE_SECONDS / (1 + (int - 10) * 0.1)`
//!
//! Curses are only revealed on identify, but they apply while equipped
//! regardless — equipping unidentified gear is the gamble.
//!
//! Runs from the engine tick in game-time via the accumulator pattern (see
//! `fire::tick_fire`), so it freezes while the game is paused/idle.

use hecs::{Entity, World};

use crate::components::{Equipment, Inventory, ItemInstance};
use crate::constants::*;
use crate::events::{EventQueue, GameEvent};

/// Game-time seconds needed to identify an item at the given Intelligence.
/// The INT factor is clamped so extreme stats stay sane.
pub fn identify_seconds(intelligence: i32) -> f32 {
    let factor = (1.0 + (intelligence - 10) as f32 * 0.1)
        .clamp(IDENTIFY_INT_FACTOR_MIN, IDENTIFY_INT_FACTOR_MAX);
    IDENTIFY_BASE_SECONDS / factor
}

/// Advance identification progress on everything the player carries
/// (inventory items and all equipped slots). `game_dt` is elapsed game-time
/// this frame; `accumulator` carries fractional time between discrete steps.
/// Emits `ItemIdentified` for each item that completes.
pub fn tick_identification(
    world: &mut World,
    player: Entity,
    game_dt: f32,
    accumulator: &mut f32,
    events: &mut EventQueue,
) {
    if game_dt <= 0.0 {
        return;
    }
    *accumulator += game_dt;
    if *accumulator < IDENTIFY_TICK_INTERVAL {
        return;
    }
    let step = *accumulator;
    *accumulator = 0.0;

    // INT read through effective stats so gear (e.g. an Intelligence ring)
    // speeds up identifying the rest of the haul.
    let required = identify_seconds(crate::queries::effective_stats(world, player).intelligence);

    let mut completed: Vec<(String, bool)> = Vec::new();

    if let Ok(mut inventory) = world.get::<&mut Inventory>(player) {
        for inst in inventory.items.iter_mut() {
            progress_item(inst, step, required, &mut completed);
        }
    }
    if let Ok(mut equipment) = world.get::<&mut Equipment>(player) {
        for inst in equipment.equipped_instances_mut() {
            progress_item(inst, step, required, &mut completed);
        }
    }

    for (name, cursed) in completed {
        events.push(GameEvent::ItemIdentified { name, cursed });
    }
}

/// Advance one item's identification; on completion, record its (now fully
/// visible) display name and whether it turned out to be cursed.
fn progress_item(
    inst: &mut ItemInstance,
    step: f32,
    required: f32,
    completed: &mut Vec<(String, bool)>,
) {
    if inst.identified {
        return;
    }
    inst.identify_progress += step;
    if inst.identify_progress >= required {
        inst.identified = true;
        completed.push((inst.display_name(), inst.has_curse()));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::components::{Affix, ItemType, Rarity, Stats};

    fn unidentified(kind: ItemType, affixes: Vec<Affix>) -> ItemInstance {
        ItemInstance {
            kind,
            rarity: Rarity::Magic,
            affixes,
            name: None,
            identified: false,
            identify_progress: 0.0,
        }
    }

    #[test]
    fn test_identify_seconds_scales_with_int() {
        // Baseline INT 10 = base time.
        assert!((identify_seconds(10) - IDENTIFY_BASE_SECONDS).abs() < f32::EPSILON);
        // INT 20 identifies twice as fast.
        assert!((identify_seconds(20) - IDENTIFY_BASE_SECONDS / 2.0).abs() < 0.001);
        // Higher INT is never slower.
        assert!(identify_seconds(18) < identify_seconds(12));
        // Very low INT clamps at the minimum factor instead of exploding
        // (or dividing by zero / going negative).
        let slowest = IDENTIFY_BASE_SECONDS / IDENTIFY_INT_FACTOR_MIN;
        assert!((identify_seconds(0) - slowest).abs() < 0.001);
        assert!((identify_seconds(-50) - slowest).abs() < 0.001);
        // Very high INT clamps at the maximum factor.
        let fastest = IDENTIFY_BASE_SECONDS / IDENTIFY_INT_FACTOR_MAX;
        assert!((identify_seconds(1000) - fastest).abs() < 0.001);
    }

    #[test]
    fn test_tick_identifies_carried_and_equipped_items() {
        let mut world = World::new();

        let mut inventory = Inventory::new();
        inventory.items.push(unidentified(ItemType::Sword, vec![Affix::Damage(2)]));

        let mut equipment = Equipment::empty();
        equipment.ring = Some(unidentified(ItemType::Ring, vec![Affix::CursedLoud]));

        let player = world.spawn((Stats::new(10, 10, 10), inventory, equipment));
        let mut events = EventQueue::new();
        let mut accumulator = 0.0;

        // Not enough time yet: nothing identifies.
        tick_identification(&mut world, player, 1.0, &mut accumulator, &mut events);
        {
            let inv = world.get::<&Inventory>(player).expect("inventory");
            assert!(!inv.items[0].identified);
        }

        // Push past the requirement at INT 10.
        tick_identification(
            &mut world,
            player,
            IDENTIFY_BASE_SECONDS,
            &mut accumulator,
            &mut events,
        );
        let inv = world.get::<&Inventory>(player).expect("inventory");
        assert!(inv.items[0].identified, "inventory item should identify");
        let eq = world.get::<&Equipment>(player).expect("equipment");
        assert!(
            eq.ring.as_ref().map(|r| r.identified).unwrap_or(false),
            "equipped ring should identify too"
        );

        // Both completions emitted events; the cursed ring is flagged.
        let identified: Vec<(String, bool)> = events
            .drain()
            .filter_map(|e| match e {
                GameEvent::ItemIdentified { name, cursed } => Some((name, cursed)),
                _ => None,
            })
            .collect();
        assert_eq!(identified.len(), 2);
        assert!(identified.iter().any(|(name, cursed)| name == "Sword" && !cursed));
        assert!(identified.iter().any(|(name, cursed)| name == "Ring" && *cursed));
    }

    #[test]
    fn test_identified_display_name_hides_legendary_until_known() {
        let mut inst = ItemInstance {
            kind: ItemType::Sword,
            rarity: Rarity::Legendary,
            affixes: vec![Affix::OnHitIgnite(0.2), Affix::Damage(2)],
            name: Some("Emberfang, Sword of the Wolf".to_string()),
            identified: false,
            identify_progress: 0.0,
        };
        assert_eq!(inst.display_name(), "Unidentified Sword");
        inst.identified = true;
        assert_eq!(inst.display_name(), "Emberfang, Sword of the Wolf");
    }
}
