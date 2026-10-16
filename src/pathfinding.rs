//! Tile pathfinding for click-to-move. Pure Rust: no Bevy, fully unit-tested.
//!
//! Routes use breadth-first search over the 8-neighbourhood, so a route has the fewest
//! steps possible (a diagonal counts as one step, like classic tile-based MMORPGs).
//! Diagonal steps never cut a blocked corner, so reachability is identical to
//! [`WorldMap::is_reachable`].

use std::collections::VecDeque;

use crate::world::{TilePos, WorldMap};

/// How a route should end relative to the tile that was clicked.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Approach {
    /// Stand exactly on the target tile (ground clicks, pickups lying on the ground).
    OnTile,
    /// Stand on a walkable tile orthogonally next to the target (trees, ore, NPCs).
    Adjacent,
}

/// Orthogonal steps come first so equal-length routes prefer straight movement.
const STEPS: [(i32, i32); 8] = [(0, 1), (1, 0), (0, -1), (-1, 0), (1, 1), (1, -1), (-1, -1), (-1, 1)];

/// True when `a` and `b` share an edge (diagonal neighbours do not count).
pub fn is_orthogonally_adjacent(a: TilePos, b: TilePos) -> bool {
    (a.x - b.x).abs() + (a.y - b.y).abs() == 1
}

/// True when moving by `(dx, dy)` from `from` is allowed: the destination is walkable and,
/// for diagonals, both orthogonal neighbours are walkable too (no corner cutting).
fn step_allowed(map: &WorldMap, from: TilePos, dx: i32, dy: i32) -> bool {
    if !map.can_walk_to(TilePos::new(from.x + dx, from.y + dy)) {
        return false;
    }
    if dx != 0 && dy != 0 {
        return map.can_walk_to(TilePos::new(from.x + dx, from.y))
            && map.can_walk_to(TilePos::new(from.x, from.y + dy));
    }
    true
}

/// Breadth-first search from `start` to the nearest tile satisfying `is_goal`.
/// Returns the steps to take, excluding `start` itself (empty when already there).
fn search(map: &WorldMap, start: TilePos, is_goal: impl Fn(TilePos) -> bool) -> Option<Vec<TilePos>> {
    if !map.can_walk_to(start) {
        return None;
    }
    if is_goal(start) {
        return Some(Vec::new());
    }
    let width = map.width();
    let index = |p: TilePos| p.y as usize * width + p.x as usize;
    // `parent[i]` is the tile we came from; the start points at itself.
    let mut parent: Vec<Option<TilePos>> = vec![None; width * map.height()];
    parent[index(start)] = Some(start);
    let mut queue = VecDeque::from([start]);

    while let Some(current) = queue.pop_front() {
        for (dx, dy) in STEPS {
            if !step_allowed(map, current, dx, dy) {
                continue;
            }
            let next = TilePos::new(current.x + dx, current.y + dy);
            if parent[index(next)].is_some() {
                continue;
            }
            parent[index(next)] = Some(current);
            if is_goal(next) {
                let mut route = vec![next];
                let mut at = next;
                while let Some(prev) = parent[index(at)] {
                    if prev == start {
                        break;
                    }
                    route.push(prev);
                    at = prev;
                }
                route.reverse();
                return Some(route);
            }
            queue.push_back(next);
        }
    }
    None
}

/// Shortest route to stand exactly on `goal`. `None` if `goal` is blocked, off the map,
/// or unreachable.
pub fn find_path(map: &WorldMap, start: TilePos, goal: TilePos) -> Option<Vec<TilePos>> {
    if !map.can_walk_to(goal) {
        return None;
    }
    search(map, start, |p| p == goal)
}

/// Shortest route to a walkable tile orthogonally next to `target` (which may itself be
/// blocked, like a tree). Returns an empty route if `start` is already beside it.
pub fn find_path_to_adjacent(map: &WorldMap, start: TilePos, target: TilePos) -> Option<Vec<TilePos>> {
    if map.tile_at(target).is_err() {
        return None;
    }
    search(map, start, |p| is_orthogonally_adjacent(p, target))
}

/// Plans a route to `target` using the requested [`Approach`].
pub fn plan_route(map: &WorldMap, start: TilePos, target: TilePos, approach: Approach) -> Option<Vec<TilePos>> {
    match approach {
        Approach::OnTile => find_path(map, start, target),
        Approach::Adjacent => find_path_to_adjacent(map, start, target),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn map(text: &str) -> WorldMap {
        WorldMap::from_ascii(text).unwrap()
    }

    /// Every step is one king-move, lands on a walkable tile, and never cuts a corner.
    fn assert_valid_route(m: &WorldMap, start: TilePos, route: &[TilePos]) {
        let mut at = start;
        for &next in route {
            let (dx, dy) = (next.x - at.x, next.y - at.y);
            assert!(dx.abs() <= 1 && dy.abs() <= 1 && (dx, dy) != (0, 0), "bad step {at:?} -> {next:?}");
            assert!(step_allowed(m, at, dx, dy), "illegal step {at:?} -> {next:?}");
            at = next;
        }
    }

    #[test]
    fn straight_route_excludes_start_and_includes_goal() {
        let m = map(".....");
        let route = find_path(&m, TilePos::new(0, 0), TilePos::new(4, 0)).unwrap();
        assert_eq!(route.len(), 4);
        assert_eq!(route.first(), Some(&TilePos::new(1, 0)));
        assert_eq!(route.last(), Some(&TilePos::new(4, 0)));
    }

    #[test]
    fn diagonals_count_as_a_single_step() {
        let m = map("....\n....\n....\n....");
        let route = find_path(&m, TilePos::new(0, 0), TilePos::new(3, 3)).unwrap();
        assert_eq!(route.len(), 3);
        assert_valid_route(&m, TilePos::new(0, 0), &route);
    }

    #[test]
    fn standing_on_the_goal_needs_no_steps() {
        let m = map("...");
        assert_eq!(find_path(&m, TilePos::new(1, 0), TilePos::new(1, 0)), Some(vec![]));
    }

    #[test]
    fn diagonal_never_cuts_a_blocked_corner() {
        // (0,1) '.'  (1,1) 'T'
        // (0,0) 'T'  (1,0) '.'   -> the two open tiles only touch at a corner.
        let m = map(".T\nT.");
        assert_eq!(find_path(&m, TilePos::new(0, 1), TilePos::new(1, 0)), None);
        assert!(!m.is_reachable(TilePos::new(0, 1), TilePos::new(1, 0)));
    }

    #[test]
    fn routes_bend_around_walls_through_the_gap() {
        // Wall at x=2 with a single gap at y=1.
        let m = map("..T..\n.....\n..T..");
        let start = TilePos::new(0, 2);
        let goal = TilePos::new(4, 2);
        let route = find_path(&m, start, goal).unwrap();
        assert_valid_route(&m, start, &route);
        assert!(route.iter().any(|p| p.x == 2 && p.y == 1), "must use the gap");
        assert_eq!(route.last(), Some(&goal));
    }

    #[test]
    fn blocked_off_map_and_enclosed_goals_have_no_route() {
        let m = map("..T..\n..T..\n..T..");
        let start = TilePos::new(0, 0);
        assert_eq!(find_path(&m, start, TilePos::new(2, 0)), None, "goal blocked");
        assert_eq!(find_path(&m, start, TilePos::new(9, 9)), None, "goal off map");
        assert_eq!(find_path(&m, start, TilePos::new(4, 0)), None, "walled off");
        assert_eq!(find_path(&m, TilePos::new(2, 0), TilePos::new(0, 0)), None, "start blocked");
    }

    #[test]
    fn adjacent_approach_ends_beside_a_blocked_target() {
        let m = map(".....\n..T..\n.....");
        let tree = TilePos::new(2, 1);
        let start = TilePos::new(0, 1);
        let route = find_path_to_adjacent(&m, start, tree).unwrap();
        assert_valid_route(&m, start, &route);
        let end = route.last().copied().unwrap_or(start);
        assert!(is_orthogonally_adjacent(end, tree));
        assert_eq!(route.len(), 1, "(1,1) is one step away and is beside the tree");
    }

    #[test]
    fn adjacent_approach_is_empty_when_already_beside_the_target() {
        let m = map(".....\n..T..\n.....");
        let route = find_path_to_adjacent(&m, TilePos::new(1, 1), TilePos::new(2, 1));
        assert_eq!(route, Some(vec![]));
    }

    #[test]
    fn diagonal_neighbours_are_not_adjacent_enough() {
        let m = map(".....\n..T..\n.....");
        let route = find_path_to_adjacent(&m, TilePos::new(1, 0), TilePos::new(2, 1)).unwrap();
        assert!(!route.is_empty(), "diagonal to the tree still needs a step to be beside it");
    }

    #[test]
    fn adjacent_approach_rejects_off_map_targets_and_walled_in_targets() {
        let m = map(".....\n..T..\n.....");
        assert_eq!(find_path_to_adjacent(&m, TilePos::new(0, 0), TilePos::new(9, 9)), None);
        // A target completely surrounded by blocked tiles cannot be approached.
        let sealed = map("TTT\nT.T\nTTT");
        assert_eq!(find_path_to_adjacent(&sealed, TilePos::new(1, 1), TilePos::new(0, 0)), None);
    }

    #[test]
    fn plan_route_dispatches_on_approach() {
        let m = map(".....\n..T..\n.....");
        let start = TilePos::new(0, 1);
        assert_eq!(plan_route(&m, start, TilePos::new(2, 1), Approach::OnTile), None);
        assert!(plan_route(&m, start, TilePos::new(2, 1), Approach::Adjacent).is_some());
        assert_eq!(plan_route(&m, start, TilePos::new(4, 1), Approach::OnTile).map(|r| r.len()), Some(4));
    }

    #[test]
    fn pathfinding_agrees_with_four_way_reachability_everywhere() {
        let m = crate::scene::meadow_map().unwrap();
        let start = crate::scene::PLAYER_START;
        for y in -1..=m.height() as i32 {
            for x in -1..=m.width() as i32 {
                let goal = TilePos::new(x, y);
                let route = find_path(&m, start, goal);
                assert_eq!(route.is_some(), m.is_reachable(start, goal), "mismatch at {goal:?}");
                if let Some(route) = route {
                    assert_valid_route(&m, start, &route);
                }
            }
        }
    }
}
