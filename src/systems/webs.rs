//! Spider webs.
//!
//! Spiders (`WebSpinner`) periodically lay `Web` entities on their own tile
//! (from the AI decision path — see `systems::ai`). Non-spider entities that
//! step into a web are Rooted briefly and the web is consumed
//! (`trigger_web_at`, called from the movement action). Webs are highly
//! flammable; the fire side lives in `systems::fire`.

use hecs::{Entity, World};

use crate::components::{EffectType, Position, Spider, Web};
use crate::constants::*;
use crate::events::{EventQueue, GameEvent};

/// Is there a web at this tile?
pub fn web_at(world: &World, x: i32, y: i32) -> Option<Entity> {
    world
        .query::<(&Position, &Web)>()
        .iter()
        .find(|(_, (p, _))| p.x == x && p.y == y)
        .map(|(id, _)| id)
}

/// Number of live webs laid by this spinner.
fn live_webs_for(world: &World, spinner: Entity) -> usize {
    world
        .query::<&Web>()
        .iter()
        .filter(|(_, w)| w.spinner == Some(spinner))
        .count()
}

/// Total live webs on the floor.
fn total_webs(world: &World) -> usize {
    world.query::<&Web>().iter().count()
}

/// Try to lay a web at (x, y) for `spinner`. Fails (returns false) if a web
/// already covers the tile, the spinner is at its personal cap
/// (`WEB_MAX_PER_SPIDER`), or the floor is at the global cap (`WEB_TOTAL_CAP`).
pub fn try_lay_web(world: &mut World, spinner: Entity, x: i32, y: i32) -> bool {
    if web_at(world, x, y).is_some() {
        return false;
    }
    if live_webs_for(world, spinner) >= WEB_MAX_PER_SPIDER {
        return false;
    }
    if total_webs(world) >= WEB_TOTAL_CAP {
        return false;
    }
    crate::spawning::spawn_web(world, x, y, Some(spinner));
    true
}

/// A non-spider entity entered (x, y): if a web covers it, root the entity
/// and consume the web. Spiders skitter across their silk freely.
pub fn trigger_web_at(
    world: &mut World,
    entity: Entity,
    x: i32,
    y: i32,
    events: &mut EventQueue,
) {
    if world.get::<&Spider>(entity).is_ok() {
        return;
    }
    let Some(web) = web_at(world, x, y) else {
        return;
    };
    let _ = world.despawn(web);
    crate::systems::effects::add_effect_to_entity(world, entity, EffectType::Rooted, WEB_ROOT_DURATION);
    events.push(GameEvent::WebTouched { victim: entity, position: (x, y) });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::components::StatusEffects;

    #[test]
    fn test_web_per_spider_cap() {
        let mut world = World::new();
        let spider = world.spawn((Position::new(0, 0), Spider));

        // Lay webs on distinct tiles; only WEB_MAX_PER_SPIDER stick.
        let mut laid = 0;
        for x in 0..(WEB_MAX_PER_SPIDER as i32 + 3) {
            if try_lay_web(&mut world, spider, x, 0) {
                laid += 1;
            }
        }
        assert_eq!(laid, WEB_MAX_PER_SPIDER);
        assert_eq!(world.query::<&Web>().iter().count(), WEB_MAX_PER_SPIDER);

        // Same tile twice never stacks.
        let spider2 = world.spawn((Position::new(0, 1), Spider));
        assert!(try_lay_web(&mut world, spider2, 0, 1));
        assert!(!try_lay_web(&mut world, spider2, 0, 1));
    }

    #[test]
    fn test_web_total_cap() {
        let mut world = World::new();
        // Enough spiders to exceed the global cap on their own.
        let spiders: Vec<Entity> = (0..8)
            .map(|i| world.spawn((Position::new(i, -1), Spider)))
            .collect();

        let mut laid = 0;
        let mut x = 0;
        for spider in &spiders {
            for _ in 0..WEB_MAX_PER_SPIDER {
                if try_lay_web(&mut world, *spider, x, 5) {
                    laid += 1;
                }
                x += 1;
            }
        }
        assert_eq!(laid, WEB_TOTAL_CAP);
    }

    #[test]
    fn test_web_roots_non_spider_and_is_consumed() {
        let mut world = World::new();
        let spider = world.spawn((Position::new(0, 0), Spider));
        assert!(try_lay_web(&mut world, spider, 3, 3));

        let victim = world.spawn((Position::new(3, 3), StatusEffects::new()));
        let mut events = EventQueue::new();
        trigger_web_at(&mut world, victim, 3, 3, &mut events);

        assert!(
            crate::systems::effects::entity_has_effect(&world, victim, EffectType::Rooted),
            "web should root the victim"
        );
        assert!(web_at(&world, 3, 3).is_none(), "web is consumed on trigger");

        // A spider never triggers webs.
        assert!(try_lay_web(&mut world, spider, 4, 4));
        trigger_web_at(&mut world, spider, 4, 4, &mut events);
        assert!(web_at(&world, 4, 4).is_some(), "spiders ignore webs");
    }
}
