use crate::grid::Grid;
use std::collections::{BinaryHeap, HashMap, HashSet};
use std::cmp::Ordering;

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
struct Node {
    x: i32,
    y: i32,
}

#[derive(Clone, Copy, PartialEq, Eq)]
struct ScoredNode {
    node: Node,
    f_score: i32, // g_score + heuristic
}

// BinaryHeap is a max-heap, so we reverse the ordering for min-heap behavior
impl Ord for ScoredNode {
    fn cmp(&self, other: &Self) -> Ordering {
        other.f_score.cmp(&self.f_score)
    }
}

impl PartialOrd for ScoredNode {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

/// Find a path from start to goal using A* algorithm.
/// Returns the path as a vector of (x, y) positions, excluding the start position.
/// Returns None if no path exists.
pub fn find_path(
    grid: &Grid,
    start: (i32, i32),
    goal: (i32, i32),
    blocked: &HashSet<(i32, i32)>,
) -> Option<Vec<(i32, i32)>> {
    let start_node = Node { x: start.0, y: start.1 };
    let goal_node = Node { x: goal.0, y: goal.1 };

    // Check if goal is walkable (but allow blocked tiles as goal - e.g., attacking an enemy)
    if let Some(tile) = grid.get(goal.0, goal.1) {
        if !tile.tile_type.is_walkable() {
            return None;
        }
    } else {
        return None;
    }

    let mut open_set = BinaryHeap::new();
    let mut came_from: HashMap<Node, Node> = HashMap::new();
    let mut g_score: HashMap<Node, i32> = HashMap::new();

    g_score.insert(start_node, 0);
    open_set.push(ScoredNode {
        node: start_node,
        f_score: heuristic(start, goal),
    });

    while let Some(current) = open_set.pop() {
        if current.node == goal_node {
            // Reconstruct path
            return Some(reconstruct_path(&came_from, current.node));
        }

        let current_g = *g_score.get(&current.node).unwrap_or(&i32::MAX);

        // Check all 4 neighbors
        for (dx, dy) in [(0, 1), (0, -1), (1, 0), (-1, 0)] {
            let nx = current.node.x + dx;
            let ny = current.node.y + dy;
            let neighbor = Node { x: nx, y: ny };

            // Check if walkable
            let walkable = if let Some(tile) = grid.get(nx, ny) {
                tile.tile_type.is_walkable()
            } else {
                false
            };

            if !walkable {
                continue;
            }

            // Check if blocked by entity (but allow the goal position)
            if blocked.contains(&(nx, ny)) && (nx, ny) != goal {
                continue;
            }

            let tentative_g = current_g + 1;
            let neighbor_g = *g_score.get(&neighbor).unwrap_or(&i32::MAX);

            if tentative_g < neighbor_g {
                came_from.insert(neighbor, current.node);
                g_score.insert(neighbor, tentative_g);
                open_set.push(ScoredNode {
                    node: neighbor,
                    f_score: tentative_g + heuristic((nx, ny), goal),
                });
            }
        }
    }

    None // No path found
}

/// Get just the next step toward a goal.
/// Returns None if no path exists or already at goal.
pub fn next_step_toward(
    grid: &Grid,
    start: (i32, i32),
    goal: (i32, i32),
    blocked: &HashSet<(i32, i32)>,
) -> Option<(i32, i32)> {
    if start == goal {
        return None;
    }

    find_path(grid, start, goal, blocked).and_then(|path| path.first().copied())
}

/// Manhattan distance heuristic
fn heuristic(from: (i32, i32), to: (i32, i32)) -> i32 {
    (from.0 - to.0).abs() + (from.1 - to.1).abs()
}

/// Reconstruct the path from came_from map
fn reconstruct_path(came_from: &HashMap<Node, Node>, mut current: Node) -> Vec<(i32, i32)> {
    let mut path = vec![(current.x, current.y)];

    while let Some(&prev) = came_from.get(&current) {
        path.push((prev.x, prev.y));
        current = prev;
    }

    path.reverse();
    // Remove the start position
    if !path.is_empty() {
        path.remove(0);
    }
    path
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tile::{Tile, TileType};

    /// Create a simple test grid with all floor tiles
    fn make_floor_grid(width: usize, height: usize) -> Grid {
        Grid {
            width,
            height,
            tiles: vec![Tile::new(TileType::Floor); width * height],
            chest_positions: vec![],
            door_positions: vec![],
            decals: vec![],
            stairs_up_pos: None,
            stairs_down_pos: None,
        }
    }

    /// Create a grid with a wall in the middle
    fn make_grid_with_wall() -> Grid {
        // 5x5 grid with a vertical wall at x=2 (except y=0 which is open)
        let mut tiles = vec![Tile::new(TileType::Floor); 25];
        // Wall at (2,1), (2,2), (2,3), (2,4)
        for y in 1..5 {
            tiles[y * 5 + 2] = Tile::new(TileType::Wall);
        }
        Grid {
            width: 5,
            height: 5,
            tiles,
            chest_positions: vec![],
            door_positions: vec![],
            decals: vec![],
            stairs_up_pos: None,
            stairs_down_pos: None,
        }
    }

    #[test]
    fn test_heuristic() {
        assert_eq!(heuristic((0, 0), (0, 0)), 0);
        assert_eq!(heuristic((0, 0), (1, 0)), 1);
        assert_eq!(heuristic((0, 0), (0, 1)), 1);
        assert_eq!(heuristic((0, 0), (3, 4)), 7);
        assert_eq!(heuristic((5, 5), (2, 1)), 7);
    }

    #[test]
    fn test_find_path_straight_line() {
        let grid = make_floor_grid(10, 10);
        let blocked = HashSet::new();

        let path = find_path(&grid, (0, 0), (5, 0), &blocked);
        assert!(path.is_some());
        let path = path.unwrap();
        assert_eq!(path.len(), 5);
        assert_eq!(path.last(), Some(&(5, 0)));
    }

    #[test]
    fn test_find_path_diagonal() {
        let grid = make_floor_grid(10, 10);
        let blocked = HashSet::new();

        let path = find_path(&grid, (0, 0), (3, 3), &blocked);
        assert!(path.is_some());
        let path = path.unwrap();
        // Manhattan distance is 6, so path should be 6 steps
        assert_eq!(path.len(), 6);
        assert_eq!(path.last(), Some(&(3, 3)));
    }

    #[test]
    fn test_find_path_around_wall() {
        let grid = make_grid_with_wall();
        let blocked = HashSet::new();

        // Path from (0,2) to (4,2) must go around the wall
        let path = find_path(&grid, (0, 2), (4, 2), &blocked);
        assert!(path.is_some());
        let path = path.unwrap();
        // Must go up to y=0, across, and back down
        assert!(path.len() > 4); // Would be 4 if direct, must be longer
        assert_eq!(path.last(), Some(&(4, 2)));
    }

    #[test]
    fn test_find_path_blocked_by_entity() {
        let grid = make_floor_grid(5, 5);
        let mut blocked = HashSet::new();
        blocked.insert((2, 0));
        blocked.insert((2, 1));
        blocked.insert((2, 2));

        // Path from (0,1) to (4,1) with entities blocking
        let path = find_path(&grid, (0, 1), (4, 1), &blocked);
        assert!(path.is_some());
        let path = path.unwrap();
        // Path should avoid blocked tiles
        for (x, y) in &path {
            if (*x, *y) != (4, 1) {
                // Goal is allowed even if blocked
                assert!(!blocked.contains(&(*x, *y)));
            }
        }
    }

    #[test]
    fn test_find_path_no_path() {
        // Create a grid where the goal is completely surrounded by walls
        let mut tiles = vec![Tile::new(TileType::Floor); 25];
        // Wall around (2,2)
        tiles[1 * 5 + 1] = Tile::new(TileType::Wall);
        tiles[1 * 5 + 2] = Tile::new(TileType::Wall);
        tiles[1 * 5 + 3] = Tile::new(TileType::Wall);
        tiles[2 * 5 + 1] = Tile::new(TileType::Wall);
        tiles[2 * 5 + 3] = Tile::new(TileType::Wall);
        tiles[3 * 5 + 1] = Tile::new(TileType::Wall);
        tiles[3 * 5 + 2] = Tile::new(TileType::Wall);
        tiles[3 * 5 + 3] = Tile::new(TileType::Wall);
        let grid = Grid {
            width: 5,
            height: 5,
            tiles,
            chest_positions: vec![],
            door_positions: vec![],
            decals: vec![],
            stairs_up_pos: None,
            stairs_down_pos: None,
        };

        let path = find_path(&grid, (0, 0), (2, 2), &HashSet::new());
        assert!(path.is_none());
    }

    #[test]
    fn test_find_path_to_unwalkable_tile() {
        let mut tiles = vec![Tile::new(TileType::Floor); 25];
        tiles[2 * 5 + 2] = Tile::new(TileType::Wall); // Goal is a wall
        let grid = Grid {
            width: 5,
            height: 5,
            tiles,
            chest_positions: vec![],
            door_positions: vec![],
            decals: vec![],
            stairs_up_pos: None,
            stairs_down_pos: None,
        };

        let path = find_path(&grid, (0, 0), (2, 2), &HashSet::new());
        assert!(path.is_none());
    }

    #[test]
    fn test_next_step_toward() {
        let grid = make_floor_grid(10, 10);
        let blocked = HashSet::new();

        let next = next_step_toward(&grid, (0, 0), (5, 0), &blocked);
        assert!(next.is_some());
        let (nx, ny) = next.unwrap();
        // First step should be adjacent to start
        assert!(((nx - 0).abs() + (ny - 0).abs()) == 1);
    }

    #[test]
    fn test_next_step_at_goal() {
        let grid = make_floor_grid(10, 10);
        let blocked = HashSet::new();

        let next = next_step_toward(&grid, (5, 5), (5, 5), &blocked);
        assert!(next.is_none()); // Already at goal
    }

    #[test]
    fn test_path_allows_goal_even_if_blocked() {
        let grid = make_floor_grid(5, 5);
        let mut blocked = HashSet::new();
        blocked.insert((2, 0)); // Block the goal

        // Should still find a path to (2, 0) even though it's blocked
        // (useful for attacking enemies at that position)
        let path = find_path(&grid, (0, 0), (2, 0), &blocked);
        assert!(path.is_some());
        assert_eq!(path.unwrap().last(), Some(&(2, 0)));
    }
}
