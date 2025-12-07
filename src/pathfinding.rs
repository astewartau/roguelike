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
