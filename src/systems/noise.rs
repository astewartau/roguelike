//! Noise: how sound gets from where something happened to who hears it.
//!
//! A noise has a loudness (`radius`, in tiles of travel) and spreads outward
//! through open ground. Walls stop it; a closed door muffles it, costing
//! `NOISE_DOOR_PENALTY` extra tiles of travel to pass. So the same scuffle is
//! heard across an open hall but not through the wall of the next room, and
//! shutting the door before a fight matters.
//!
//! Enemies that hear a noise go to look at *where it came from*, not where
//! the player is. Asleep, unaware and idle enemies all react; one already
//! investigating is pulled to the newer noise; one chasing the player ignores
//! it. That is what makes noise a lure as well as an alarm: throw something
//! where you want the guards to be, and they go there.

use std::cmp::Reverse;
use std::collections::{BinaryHeap, HashMap};

use hecs::Entity;
use crate::world::World;

use crate::components::{AIState, ChaseAI, Door, Position, SecretDoor};
use crate::constants::*;
use crate::grid::Grid;

/// Make a noise of loudness `radius` at `center`: every enemy it reaches that
/// isn't already chasing someone goes to investigate `center`.
pub fn make_noise(world: &mut World, grid: &Grid, center: (i32, i32), radius: i32) {
    if radius <= 0 {
        return;
    }
    let reach = noise_reach(world, grid, center, radius);
    let hearers: Vec<Entity> = world
        .query::<(&Position, &ChaseAI)>()
        .iter()
        .filter(|(_, (pos, ai))| ai.state != AIState::Chasing && reach.contains_key(&(pos.x, pos.y)))
        .map(|(id, _)| id)
        .collect();
    for e in hearers {
        crate::systems::ai::investigate_noise(world, e, center);
    }
}

/// The tiles a noise of loudness `radius` at `center` reaches, each with the
/// travel it cost to get there. Sound moves through anything walkable (water
/// and tall grass included), never through walls, and pays
/// `NOISE_DOOR_PENALTY` extra to pass a closed (or secret) door. It doesn't squeeze
/// diagonally between two walls.
pub fn noise_reach(
    world: &World,
    grid: &Grid,
    center: (i32, i32),
    radius: i32,
) -> HashMap<(i32, i32), i32> {
    // Doors (and undiscovered secret doors, which are doors dressed as wall)
    // stand on walkable tiles, so it is the penalty that muffles them.
    let closed_doors: std::collections::HashSet<(i32, i32)> = world
        .query::<(&Position, &Door)>()
        .iter()
        .filter(|(_, (_, d))| !d.is_open)
        .map(|(_, (p, _))| (p.x, p.y))
        .chain(
            world
                .query::<(&Position, &SecretDoor)>()
                .iter()
                .map(|(_, (p, _))| (p.x, p.y)),
        )
        .collect();
    let carries = |x: i32, y: i32| grid.is_walkable(x, y);

    let mut cost: HashMap<(i32, i32), i32> = HashMap::new();
    let mut frontier = BinaryHeap::new();
    cost.insert(center, 0);
    frontier.push(Reverse((0, center)));

    while let Some(Reverse((spent, (x, y)))) = frontier.pop() {
        if cost.get(&(x, y)).is_some_and(|&c| c < spent) {
            continue;
        }
        for (dx, dy) in [(-1, 0), (1, 0), (0, -1), (0, 1), (-1, -1), (-1, 1), (1, -1), (1, 1)] {
            let (nx, ny) = (x + dx, y + dy);
            if !carries(nx, ny) {
                continue;
            }
            if dx != 0 && dy != 0 && !carries(x + dx, y) && !carries(x, y + dy) {
                continue;
            }
            let step = if closed_doors.contains(&(nx, ny)) { 1 + NOISE_DOOR_PENALTY } else { 1 };
            let next = spent + step;
            if next > radius || cost.get(&(nx, ny)).is_some_and(|&c| c <= next) {
                continue;
            }
            cost.insert((nx, ny), next);
            frontier.push(Reverse((next, (nx, ny))));
        }
    }
    cost
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tile::TileType;

    /// Two rooms side by side: x 0..=4 and x 6..=10, wall at x = 5 with a
    /// doorway at (5, 2).
    fn two_rooms() -> Grid {
        let mut grid = Grid::filled(11, 5, TileType::Floor);
        for y in 0..5 {
            if y != 2 {
                grid.get_mut(5, y).unwrap().tile_type = TileType::Wall;
            }
        }
        grid
    }

    #[test]
    fn walls_stop_noise_and_a_closed_door_muffles_it() {
        let mut world = World::new();
        let grid = two_rooms();

        // Through the open doorway, the far room hears it.
        let open = noise_reach(&world, &grid, (3, 0), 6);
        assert!(open.contains_key(&(7, 2)), "heard through the doorway");
        // A tile straight through the wall is only reachable the long way.
        assert!(open[&(6, 0)] > 3, "sound goes round, not through, the wall");

        // Close the door: the same noise no longer gets through.
        world.spawn((Position::new(5, 2), Door::new()));
        let shut = noise_reach(&world, &grid, (3, 0), 6);
        assert!(!shut.contains_key(&(7, 2)), "a closed door muffles it");
        assert!(shut.contains_key(&(1, 4)), "the near room still hears it");
    }

    #[test]
    fn enemies_investigate_where_the_noise_was_not_where_you_are() {
        use rand::SeedableRng;
        let mut world = World::new();
        let grid = two_rooms();
        let player = world.spawn((Position::new(0, 0), crate::components::Player, crate::components::Actor::new(1.0)));
        let mut rng = rand::rngs::StdRng::seed_from_u64(2);
        let guard = crate::spawning::enemies::GOBLIN.spawn(&mut world, 9, 2, &mut rng);
        world.get::<&mut ChaseAI>(guard).unwrap().state = AIState::Idle;
        let _ = world.remove_one::<crate::components::Asleep>(guard);

        make_noise(&mut world, &grid, (8, 1), 4);

        let ai = world.get::<&ChaseAI>(guard).unwrap();
        let entry = ai.threat_table.iter().find(|t| t.entity == player).expect("heard it");
        assert_eq!(entry.last_known_pos, Some((8, 1)), "it goes to look at the noise");
    }
}
