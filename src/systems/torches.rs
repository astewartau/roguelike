//! Torches: carried by goblins, and held by the player in the off hand.
//!
//! Anything holding a lit torch (a `CarriedTorch`) is a walking light: it
//! sees its way, you see it coming in the dark, and when it is close its torch
//! lights *you* up too (see `systems::light`) — which cuts both ways when the
//! torch is yours. A soaking puts it out.
//!
//! A torch never sets anything alight by accident: not by being carried
//! through oil, not by lying on the floor, not by a fight going on beside it.
//! It lights things only when someone puts it to them on purpose — a swing at
//! a creature, or holding it to a tile — and a creature only catches if it is
//! covered in oil (`Oiled`).
//!
//! The player's torch is an item (`ItemType::Torch`) in the off-hand slot;
//! `sync_held_torch` keeps the carried light in step with that slot. A
//! doused torch becomes an `UnlitTorch`, relit by interacting with a fire;
//! a lit one can be put to grass or oil the same way.
//!
//! A goblin that dies drops its torch, still burning, as an item lying on the
//! floor: a light, but no hazard. Pick it up while it burns and it's yours;
//! leave it and it gutters out to an unlit torch.

use hecs::Entity;
use crate::world::World;

use crate::components::{CarriedTorch, Equipment, ItemInstance, ItemType, OnFire, Position};
use crate::systems::held;
use crate::events::{EventQueue, GameEvent};
use crate::grid::Grid;

fn lit_torch(world: &World, entity: Entity) -> bool {
    world.get::<&CarriedTorch>(entity).map(|t| t.lit).unwrap_or(false)
}

fn held_kind(world: &World, entity: Entity) -> Option<ItemType> {
    held_torch(world, entity).and_then(|e| held::kind(world, e))
}

/// The item in `entity`'s off hand.
fn held_torch(world: &World, entity: Entity) -> Option<Entity> {
    world.get::<&Equipment>(entity).ok().and_then(|e| e.off_hand)
}

/// Turn item `e` into a `kind` (a torch lit or put out).
fn turn_into(world: &mut World, e: Entity, kind: ItemType) {
    if let Ok(mut item) = world.get::<&mut ItemInstance>(e) {
        item.kind = kind;
    }
}

/// Make an entity's carried light match what is in its off hand: a `Torch`
/// burns (light, lit torch drawn in hand), an `UnlitTorch` is carried dark,
/// anything else means no torch at all. Call after any change to the off-hand
/// slot or the torch in it. The torch in hand is drawn from the
/// `CarriedTorch` itself (see `systems::rendering`).
pub fn sync_held_torch(world: &mut World, entity: Entity) {
    match held_kind(world, entity) {
        Some(ItemType::Torch) => {
            let _ = world.insert_one(entity, CarriedTorch { lit: true });
        }
        Some(ItemType::UnlitTorch) => {
            let _ = world.insert_one(entity, CarriedTorch { lit: false });
        }
        _ => stop_carrying(world, entity),
    }
}

/// Take away an entity's torch (its light is the torch's, so goes with it).
fn stop_carrying(world: &mut World, entity: Entity) {
    let _ = world.remove_one::<CarriedTorch>(entity);
}

/// Put out an entity's torch, if it carries a lit one: no more light, and the
/// carried sprite goes to the unlit torch. A held torch item becomes an
/// `UnlitTorch`. Called when the carrier gets soaked.
pub fn douse_torch(world: &mut World, entity: Entity, events: &mut EventQueue) {
    if !lit_torch(world, entity) {
        return;
    }
    if held_kind(world, entity) == Some(ItemType::Torch) {
        if let Some(torch) = held_torch(world, entity) {
            turn_into(world, torch, ItemType::UnlitTorch);
        }
        sync_held_torch(world, entity);
    } else {
        if let Ok(mut torch) = world.get::<&mut CarriedTorch>(entity) {
            torch.lit = false;
        }
    }
    events.push(GameEvent::TorchDoused { entity });
}

/// Put out every carried torch whose carrier is Wet, however it got that way
/// (wading, a splash, rain, a flask over the head). Run from the fire tick, so
/// no source of Wet has to remember to do it.
pub fn douse_wet_torches(world: &mut World, events: &mut EventQueue) {
    let soaked: Vec<Entity> = world
        .query::<(&CarriedTorch, &crate::components::StatusEffects)>()
        .iter()
        .filter(|(_, (t, s))| {
            t.lit && s.effects.iter().any(|e| e.effect_type == crate::components::EffectType::Wet)
        })
        .map(|(id, _)| id)
        .collect();
    for id in soaked {
        douse_torch(world, id, events);
    }
}

/// Whether a torch can set this entity alight: only if it's oiled. A torch
/// held to a dry creature, or swung at one, doesn't catch it.
fn torch_can_light(world: &World, entity: Entity) -> bool {
    crate::systems::effects::entity_has_effect(world, entity, crate::components::EffectType::Oiled)
}

/// Set `target` alight with a torch, if it's oiled. Returns whether it caught.
fn torch_ignites(world: &mut World, target: Entity, events: &mut EventQueue) -> bool {
    if !torch_can_light(world, target) {
        return false;
    }
    let caught = crate::systems::effects::add_effect_to_entity(
        world,
        target,
        crate::components::EffectType::Burning,
        crate::constants::BURNING_DURATION,
    );
    if caught {
        if let Some(position) = crate::queries::get_entity_position(world, target) {
            events.push(GameEvent::CaughtFire { entity: target, position });
        }
    }
    caught
}

/// A melee hit from someone holding a lit torch: swinging a torch at an
/// oiled target sets it alight. Anyone else just takes the hit — a torch only
/// lights a creature when someone means it to, and only one covered in oil.
/// Works both ways: an oiled player hit by a torch-bearing goblin goes up.
pub fn torch_strike(world: &mut World, attacker: Entity, target: Entity, events: &mut EventQueue) {
    if lit_torch(world, attacker) {
        torch_ignites(world, target, events);
    }
}

/// Is there a fire on (x, y) to light a torch from: a brazier or campfire,
/// burning fuel, or something burning?
fn fire_at(world: &World, x: i32, y: i32) -> bool {
    let here = |p: &Position| p.x == x && p.y == y;
    world
        .query::<&Position>()
        .with::<&crate::components::CausesBurning>()
        .iter()
        .any(|(_, p)| here(p))
        || world.query::<(&Position, &OnFire)>().iter().any(|(_, (p, _))| here(p))
        || world
            .query::<(&Position, &crate::components::StatusEffects)>()
            .iter()
            .any(|(_, (p, s))| {
                here(p)
                    && s.effects
                        .iter()
                        .any(|e| e.effect_type == crate::components::EffectType::Burning)
            })
}

/// The player put their held torch to (x, y). An unlit torch held to a fire
/// catches. A lit one sets alight an oiled creature standing there, and the
/// tile itself (grass, oil, webs) — but never a dry creature. Returns whether
/// anything happened, so the interaction can fall through to doors and the
/// like when it didn't.
pub fn use_held_torch_on(
    world: &mut World,
    grid: &Grid,
    entity: Entity,
    x: i32,
    y: i32,
    events: &mut EventQueue,
) -> bool {
    match held_kind(world, entity) {
        Some(ItemType::UnlitTorch) if fire_at(world, x, y) => {
            if let Some(torch) = held_torch(world, entity) {
                turn_into(world, torch, ItemType::Torch);
            }
            sync_held_torch(world, entity);
            events.push(GameEvent::TorchLit { entity });
            true
        }
        Some(ItemType::Torch) => {
            let oiled: Vec<Entity> = world
                .query::<&Position>()
                .iter()
                .filter(|(id, p)| p.x == x && p.y == y && *id != entity)
                .map(|(id, _)| id)
                .filter(|&id| torch_can_light(world, id))
                .collect();
            let flammable = grid.get(x, y).map(|t| t.tile_type.is_flammable()).unwrap_or(false);
            let fuel = world
                .query::<(&Position, &crate::components::Fuel)>()
                .without::<&OnFire>()
                .iter()
                .any(|(_, (p, _))| p.x == x && p.y == y);
            if oiled.is_empty() && !flammable && !fuel {
                return false;
            }
            for id in oiled {
                torch_ignites(world, id, events);
            }
            crate::systems::fire::ignite_tile(world, grid, x, y);
            true
        }
        _ => false,
    }
}

/// A torch-bearer died: its torch falls burning onto its tile, the torch
/// itself (the one in its hand, or for a goblin, whose torch is part of its
/// kit, a new one), to be picked up. The corpse sheds no more light. An unlit
/// torch just goes with the body.
pub fn drop_torch(world: &mut World, entity: Entity) {
    let Ok(torch) = world.get::<&CarriedTorch>(entity).map(|t| *t) else {
        return;
    };
    let pos = world.get::<&Position>(entity).map(|p| (p.x, p.y)).ok();
    stop_carrying(world, entity);
    let (true, Some((x, y))) = (torch.lit, pos) else {
        return;
    };
    let in_hand = world.get::<&mut Equipment>(entity).ok().and_then(|mut e| {
        let lit = e.off_hand.filter(|&t| held::kind(world, t) == Some(ItemType::Torch));
        if lit.is_some() {
            e.off_hand = None;
        }
        lit
    });
    match in_hand {
        Some(item) => held::put_down(world, item, x, y),
        None => {
            held::drop_new(world, x, y, ItemInstance::plain(ItemType::Torch));
        }
    }
}

/// A burning item lying on the floor goes out, burnt down or doused: it
/// stops burning and is a `leaves` now (a lit torch, an unlit one), still
/// lying there to be picked up.
pub fn item_goes_out(world: &mut World, item: Entity, leaves: ItemType) {
    crate::systems::fire::extinguish(world, item);
    let _ = world.remove_one::<crate::components::Fuel>(item);
    turn_into(world, item, leaves);
    if let Ok(mut sprite) = world.get::<&mut crate::components::Sprite>(item) {
        *sprite = crate::components::Sprite::from_ref(crate::systems::item_defs::get_def(leaves).sprite);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::components::{EffectType, OilPuddle, OnFire};
    use crate::spawning::enemies::GOBLIN;
    use crate::tile::TileType;
    use rand::{rngs::StdRng, SeedableRng};

    fn goblin(world: &mut World, x: i32, y: i32) -> Entity {
        let mut rng = StdRng::seed_from_u64(5);
        GOBLIN.spawn(world, x, y, &mut rng)
    }

    #[test]
    fn goblins_carry_a_light_and_a_soaking_puts_it_out() {
        let mut world = World::new();
        let grid = Grid::filled(9, 9, TileType::Floor);
        let g = goblin(&mut world, 4, 4);
        assert!(crate::systems::light::light_at(&world, &grid, (6, 4)) > 0.0, "its torch lights the floor");

        let mut events = EventQueue::new();
        crate::systems::effects::add_effect_to_entity(&mut world, g, EffectType::Wet, crate::constants::WET_DURATION);
        douse_wet_torches(&mut world, &mut events);
        assert!(!lit_torch(&world, g));
        assert_eq!(crate::systems::light::light_at(&world, &grid, (6, 4)), 0.0, "and now it's dark");
        assert!(events.drain().any(|e| matches!(e, GameEvent::TorchDoused { .. })));
    }

    #[test]
    fn carrying_a_torch_through_oil_lights_nothing() {
        let mut world = World::new();
        let grid = Grid::filled(9, 9, TileType::Floor);
        let g = goblin(&mut world, 3, 4);
        let oil = crate::spawning::spawn_oil_puddle(&mut world, 4, 4);
        world.get::<&mut Position>(g).unwrap().x = 4;

        let mut events = EventQueue::new();
        let mut rng = StdRng::seed_from_u64(1);
        crate::systems::tile_effects::on_enter_tile(&mut world, &grid, g, (4, 4), &mut events, &mut rng);
        assert!(world.get::<&OnFire>(oil).is_err(), "walking a torch through oil doesn't light it");
        assert!(!crate::systems::effects::entity_has_effect(&world, g, EffectType::Burning));
    }

    #[test]
    fn a_torch_swing_lights_only_an_oiled_target() {
        let mut world = World::new();
        let g = goblin(&mut world, 3, 4);
        let target = world.spawn((Position::new(4, 4), crate::components::StatusEffects::new()));
        let mut events = EventQueue::new();

        torch_strike(&mut world, g, target, &mut events);
        assert!(
            !crate::systems::effects::entity_has_effect(&world, target, EffectType::Burning),
            "a dry target just takes the hit"
        );

        crate::systems::effects::add_effect_to_entity(&mut world, target, EffectType::Oiled, crate::constants::OILED_DURATION);
        torch_strike(&mut world, g, target, &mut events);
        assert!(
            crate::systems::effects::entity_has_effect(&world, target, EffectType::Burning),
            "an oiled one goes up"
        );
    }

    #[test]
    fn a_dropped_torch_never_spreads_fire() {
        let mut world = World::new();
        let mut grid = Grid::filled(9, 9, TileType::Floor);
        grid.get_mut(5, 4).unwrap().tile_type = TileType::TallGrass;
        crate::spawning::spawn_dropped_torch(&mut world, 4, 4);
        let bystander = world.spawn((
            Position::new(4, 5),
            crate::components::StatusEffects::new(),
            crate::components::Combustible { flammability: 1.0 },
        ));
        let mut cache = crate::spatial_cache::SpatialCache::rebuild_from_world(&world);
        let mut events = EventQueue::new();
        let mut rng = StdRng::seed_from_u64(3);
        let mut acc = 0.0;
        let mut dirty = false;
        for _ in 0..30 {
            crate::systems::fire::tick_fire(
                &mut crate::engine::EffectCtx {
                    world: &mut world,
                    grid: &mut grid,
                    spatial: &mut cache,
                    events: &mut events,
                    rng: &mut rng,
                },
                crate::constants::FIRE_STEP_INTERVAL,
                &mut acc,
                &mut dirty,
            );
        }
        assert!(!crate::systems::effects::entity_has_effect(&world, bystander, EffectType::Burning));
        assert_eq!(grid.get(5, 4).map(|t| t.tile_type), Some(TileType::TallGrass), "grass untouched");
    }

    #[test]
    fn a_dead_torch_bearer_drops_its_torch_burning() {
        let mut world = World::new();
        let g = goblin(&mut world, 2, 2);
        drop_torch(&mut world, g);
        let corpse_at = world.get::<&Position>(g).map(|p| (p.x, p.y)).unwrap();
        assert!(world.get::<&CarriedTorch>(g).is_err(), "the corpse carries no torch");
        let lights = crate::systems::light::light_emitters(&world);
        assert_eq!(lights.iter().filter(|(at, _)| *at == corpse_at).count(), 1, "only the dropped torch glows there");
        let dropped = world
            .query::<(&Position, &OnFire)>()
            .without::<&OilPuddle>()
            .iter()
            .any(|(_, (p, _))| (p.x, p.y) == (2, 2));
        assert!(dropped, "the torch lies burning where it fell");
    }

    // ----- the player's torch -----

    use crate::components::{Equipment, Inventory, StatusEffects};
    use crate::systems::actions::{apply_equip_armor, apply_equip_weapon, ActionResult};

    /// A player-like entity with an empty inventory and these items in it.
    fn holder(world: &mut World, items: &[ItemType]) -> Entity {
        let mut inv = Inventory::new();
        inv.items = held::spawn_items(world, items.iter().map(|k| ItemInstance::plain(*k)));
        world.spawn((Position::new(4, 4), Equipment::empty(), inv, StatusEffects::new()))
    }

    fn index_of(world: &World, e: Entity, kind: ItemType) -> usize {
        world.get::<&Inventory>(e).unwrap().items.iter().position(|&i| held::kind(world, i) == Some(kind)).unwrap()
    }

    fn in_pack(world: &World, e: Entity, kind: ItemType) -> bool {
        world.get::<&Inventory>(e).unwrap().items.iter().any(|&i| held::kind(world, i) == Some(kind))
    }

    fn equip(world: &mut World, e: Entity, kind: ItemType, events: &mut EventQueue) {
        let i = index_of(world, e, kind);
        let r = if crate::systems::item_defs::armor_slot(kind).is_some() {
            apply_equip_armor(world, e, i, events)
        } else {
            apply_equip_weapon(world, e, i, events)
        };
        assert_eq!(r, ActionResult::Completed);
    }

    #[test]
    fn a_held_torch_lights_you_and_shares_hands_with_your_weapon() {
        let mut world = World::new();
        let grid = Grid::filled(9, 9, TileType::Floor);
        let p = holder(&mut world, &[ItemType::Bow, ItemType::Torch, ItemType::Sword]);
        let mut events = EventQueue::new();

        equip(&mut world, p, ItemType::Bow, &mut events);
        equip(&mut world, p, ItemType::Torch, &mut events);
        assert!(lit_torch(&world, p), "holding a lit torch");
        assert!(crate::systems::light::light_at(&world, &grid, (4, 4)) > 0.0, "and you're lit");
        assert!(in_pack(&world, p, ItemType::Bow), "the two-handed bow went back in the pack");

        // A sword is one-handed: it and the torch can both be held.
        equip(&mut world, p, ItemType::Sword, &mut events);
        assert!(lit_torch(&world, p), "still holding the torch with a sword");

        // Taking up the bow again puts the torch away, and its light with it.
        equip(&mut world, p, ItemType::Bow, &mut events);
        assert!(in_pack(&world, p, ItemType::Torch));
        assert_eq!(crate::systems::light::light_at(&world, &grid, (4, 4)), 0.0);
        assert!(world.get::<&CarriedTorch>(p).is_err());
        let stowed = events.drain().filter(|e| matches!(e, GameEvent::ItemStowed { .. })).count();
        assert_eq!(stowed, 2, "a bow stowed for the torch, then the torch for the bow");
    }

    #[test]
    fn putting_away_or_dying_with_an_unlit_torch_leaves_nothing_in_hand() {
        // An unlit torch has no light. Clearing the hand used to remove the
        // torch and its light as one bundle, which removes nothing at all
        // when one is missing, so the torch stayed drawn in the hand.
        let mut world = World::new();
        let p = holder(&mut world, &[ItemType::Bow, ItemType::UnlitTorch]);
        let mut events = EventQueue::new();
        equip(&mut world, p, ItemType::UnlitTorch, &mut events);
        assert!(world.get::<&CarriedTorch>(p).is_ok_and(|t| !t.lit), "carried dark");
        equip(&mut world, p, ItemType::Bow, &mut events);
        assert!(world.get::<&CarriedTorch>(p).is_err(), "the bow took both hands");

        let g = goblin(&mut world, 2, 2);
        douse_torch(&mut world, g, &mut events);
        drop_torch(&mut world, g);
        assert!(world.get::<&CarriedTorch>(g).is_err(), "a corpse holds no torch");
    }

    #[test]
    fn a_soaked_torch_goes_out_and_relights_at_a_fire() {
        let mut world = World::new();
        let mut grid = Grid::filled(9, 9, TileType::Floor);
        grid.get_mut(6, 4).unwrap().tile_type = TileType::TallGrass;
        let p = holder(&mut world, &[ItemType::Torch]);
        let mut events = EventQueue::new();
        equip(&mut world, p, ItemType::Torch, &mut events);

        crate::systems::effects::add_effect_to_entity(&mut world, p, crate::components::EffectType::Wet, crate::constants::WET_DURATION);
        douse_wet_torches(&mut world, &mut events);
        let held = held_kind(&world, p);
        assert_eq!(held, Some(ItemType::UnlitTorch), "the torch is out");
        assert!(!lit_torch(&world, p));

        // Nothing to light it from: interacting does nothing.
        assert!(!use_held_torch_on(&mut world, &grid, p, 5, 4, &mut events));
        crate::spawning::spawn_campfire(&mut world, 5, 4);
        assert!(use_held_torch_on(&mut world, &grid, p, 5, 4, &mut events), "relit at the fire");
        assert!(lit_torch(&world, p));

        // Held to a dry creature, a torch does nothing to it.
        let dry = world.spawn((Position::new(3, 4), StatusEffects::new()));
        assert!(!use_held_torch_on(&mut world, &grid, p, 3, 4, &mut events));
        assert!(!crate::systems::effects::entity_has_effect(&world, dry, crate::components::EffectType::Burning));

        // And a lit torch puts grass to the flame.
        assert!(use_held_torch_on(&mut world, &grid, p, 6, 4, &mut events));
        assert!(
            world.query::<(&Position, &OnFire)>().iter().any(|(_, (pos, _))| (pos.x, pos.y) == (6, 4)),
            "the grass is burning"
        );
    }

    /// The torch a goblin drops is the torch: it burns on the floor as
    /// itself, and picking it up takes it, flame and all, leaving nothing
    /// burning behind.
    #[test]
    fn a_dropped_torch_picked_up_leaves_no_flame_behind() {
        use crate::systems::inventory::{take_item, LootSource};
        let mut world = World::new();
        let g = goblin(&mut world, 3, 3);
        drop_torch(&mut world, g);
        let on_floor = held::on_ground_at(&world, (3, 3));
        assert_eq!(on_floor.len(), 1, "one torch lies there");
        let torch = on_floor[0];
        assert_eq!(held::kind(&world, torch), Some(ItemType::Torch));
        assert!(world.get::<&OnFire>(torch).is_ok(), "it burns, as itself");

        let player = world.spawn((Position::new(3, 3), Inventory::new()));
        assert!(take_item(&mut world, player, LootSource::Ground((3, 3)), torch, None));
        assert!(world.get::<&Inventory>(player).unwrap().items.contains(&torch), "the same torch, now carried");
        assert!(held::on_ground_at(&world, (3, 3)).is_empty());
        assert!(world.query::<&OnFire>().iter().next().is_none(), "nothing left burning on the floor");
        assert!(
            !crate::systems::light::light_emitters(&world).iter().any(|(at, _)| *at == (3, 3)),
            "and no light where it lay"
        );
    }

    /// Left alone, a dropped torch burns down where it lies, or is doused,
    /// into an unlit torch: the same item, still there to pick up.
    #[test]
    fn a_dropped_torch_left_alone_burns_down_to_an_unlit_one() {
        for doused in [false, true] {
            let mut world = World::new();
            let torch = crate::spawning::spawn_dropped_torch(&mut world, 3, 3);
            if doused {
                crate::systems::fire::douse_fuel(&mut world, torch);
            } else {
                let remaining = world.get::<&OnFire>(torch).unwrap().remaining;
                world.get::<&mut OnFire>(torch).unwrap().remaining = 0.0;
                assert!(remaining > 0.0);
                item_goes_out(&mut world, torch, ItemType::UnlitTorch);
            }
            assert_eq!(held::on_ground_at(&world, (3, 3)), vec![torch], "still lying there");
            assert_eq!(held::kind(&world, torch), Some(ItemType::UnlitTorch));
            assert!(crate::systems::light::light_emitters(&world).is_empty(), "and it's dark");
        }
    }
}
