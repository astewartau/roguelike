//! Something on an entity is used up — a fire burns out, a web is broken, a
//! trap goes off, a wet patch dries — and only that goes.
//!
//! These all used to delete the whole entity, which was right when the
//! entity was nothing but that thing (a patch of fire, a web), and wrong
//! when it was more: a web or a trap on a chest, fuel on a creature, a rain
//! cloud on a door. Now the thing being used up names the components it
//! owns; if, without them, the entity would be only looks (where it is, how
//! it's drawn, its name, a light), it *was* that thing and is despawned,
//! and otherwise only its own components are removed.

use hecs::Entity;

use crate::component_registry as registry;
use crate::world::World;

/// Components that are only how a thing is placed and drawn: an entity
/// left with nothing else has no reason to exist.
const LOOKS: &[&str] = &[
    "Position",
    "VisualPosition",
    "Sprite",
    "AnimatedSprite",
    "SpriteTint",
    "OverlaySprite",
    "FlameOverlay",
    "Name",
    "LightSource",
];

/// What a fire burning out (or put out for good) takes with it: the fire,
/// and the fuel it was, whether loose fuel, spilled oil or a web.
pub const BURNT_AWAY: &[&str] = &["Fuel", "OnFire", "CausesBurning", "OilPuddle", "Web"];

/// `own` on `entity` is used up: remove it, or the whole entity if that's
/// all it was. Returns whether the entity was despawned.
pub fn use_up(world: &mut World, entity: Entity, own: &[&str]) -> bool {
    if !world.contains(entity) {
        return false;
    }
    let (known, unknown) = registry::components_of(world, entity);
    let something_else = unknown > 0 || known.iter().any(|c| !own.contains(&c.name) && !LOOKS.contains(&c.name));
    if !something_else {
        let _ = world.despawn(entity);
        return true;
    }
    for name in own {
        if let Some(entry) = registry::find(name) {
            (entry.remove)(world, entity);
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::components::{Container, Fuel, Position, Sprite, Web};
    use crate::tile::tile_ids;

    #[test]
    fn a_web_is_used_up_but_a_chest_with_a_web_on_it_stays_a_chest() {
        let mut world = World::new();
        let web = world.spawn((Position::new(1, 1), Sprite::from_ref(tile_ids::WEB), Web { spinner: None }, Fuel::web()));
        assert!(use_up(&mut world, web, &["Web", "Fuel"]));
        assert!(!world.contains(web), "a web on its own is gone");

        let chest = world.spawn((
            Position::new(2, 2),
            Sprite::from_ref(tile_ids::CHEST_CLOSED),
            Container::chest(Vec::new(), 5),
            Web { spinner: None },
        ));
        assert!(!use_up(&mut world, chest, &["Web", "Fuel"]));
        assert!(world.contains(chest), "the chest is still there");
        assert!(world.get::<&Web>(chest).is_err(), "without its web");
        assert_eq!(world.get::<&Container>(chest).unwrap().gold, 5, "and with its gold");
    }
}
