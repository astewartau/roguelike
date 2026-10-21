//! What an ability or action can be used on: one answer per question,
//! asked the same way everywhere.
//!
//! The targeting highlight, the click, the right-click menu, the AI and the
//! action itself all used to ask "can this be tamed?" (or raised, or
//! drained) in their own words, and disagreed: the highlight offered a
//! Tameable door that the tame then couldn't do anything with. Each of them
//! now calls the function here, so what the game offers is what works.

use hecs::Entity;

use crate::components::{Actor, ChaseAI, Follower, Health, Player, Raisable, Tameable};
use crate::world::World;

/// Alive: it has no Health to lose, or some left.
fn alive(world: &World, e: Entity) -> bool {
    world.get::<&Health>(e).map(|h| h.current > 0).unwrap_or(true)
}

/// A living creature: something that takes turns and hasn't died.
pub fn is_living_creature(world: &World, e: Entity) -> bool {
    world.get::<&Actor>(e).is_ok() && alive(world, e)
}

/// Can be tamed: a living, hostile creature marked Tameable (taming
/// replaces its hostile mind with a companion's, so it needs one), and
/// nobody's companion yet.
pub fn can_be_tamed(world: &World, e: Entity) -> bool {
    world.get::<&Tameable>(e).is_ok()
        && world.get::<&ChaseAI>(e).is_ok()
        && world.get::<&Follower>(e).is_err()
        && is_living_creature(world, e)
}

/// Remains: what Raise Dead raises, Corpse Explosion detonates and Bone
/// Ward draws on. Raisable (which always comes with the Container of what
/// it held), and not something still living.
pub fn is_remains(world: &World, e: Entity) -> bool {
    world.get::<&Raisable>(e).is_ok() && !(world.get::<&Health>(e).is_ok_and(|h| h.current > 0))
}

/// Can have its life drained by `caster`: a living creature with Health,
/// not the caster, the player, or the caster's own companion.
pub fn can_be_drained(world: &World, caster: Entity, e: Entity) -> bool {
    e != caster
        && world.get::<&Player>(e).is_err()
        && world.get::<&Follower>(e).map(|f| f.leader != caster).unwrap_or(true)
        && world.get::<&Health>(e).is_ok_and(|h| h.current > 0)
        && is_living_creature(world, e)
}

/// The first entity at `tile` that passes `check`.
pub fn at_tile(world: &World, tile: (i32, i32), check: impl Fn(Entity) -> bool) -> Option<Entity> {
    world
        .query::<&crate::components::Position>()
        .iter()
        .find(|(e, p)| (p.x, p.y) == tile && check(*e))
        .map(|(e, _)| e)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::components::{Container, Door, Position};

    /// The cases the audit found offered but not working.
    #[test]
    fn only_what_works_is_a_target() {
        let mut world = World::new();
        let player = world.spawn((Position::new(0, 0), Player, Health::new(30), Actor::new(1.0)));

        // A Tameable door: not a creature, so not tameable.
        let door = world.spawn((Position::new(1, 0), Door::new(), Tameable));
        assert!(!can_be_tamed(&world, door));
        // A rat is; its corpse, and a rat already tamed, aren't.
        let rat = world.spawn((Position::new(2, 0), Health::new(5), ChaseAI::new(4), Tameable));
        assert!(can_be_tamed(&world, rat));
        world.get::<&mut Health>(rat).unwrap().current = 0;
        assert!(!can_be_tamed(&world, rat), "not as a corpse");
        let pet = world.spawn((Position::new(3, 0), Health::new(5), ChaseAI::new(4), Tameable, Follower { leader: player }));
        assert!(!can_be_tamed(&world, pet), "not someone's companion");

        // A living goblin marked Raisable isn't remains; its bones are.
        let living = world.spawn((Position::new(4, 0), Health::new(9), Raisable));
        assert!(!is_remains(&world, living));
        let bones = world.spawn((Position::new(5, 0), Container::corpse(Vec::new(), 0), Raisable));
        assert!(is_remains(&world, bones));

        // Life Drain takes from a living creature, not a barrel with health.
        let barrel = world.spawn((Position::new(6, 0), Health::new(10)));
        assert!(!can_be_drained(&world, player, barrel));
        let goblin = world.spawn((Position::new(7, 0), Health::new(10), ChaseAI::new(6)));
        assert!(can_be_drained(&world, player, goblin));
        assert!(!can_be_drained(&world, player, player));
    }
}
