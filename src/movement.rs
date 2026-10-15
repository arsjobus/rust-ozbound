use crate::world::{TilePos, WorldMap, TILE_SIZE};

/// Tolerance so a box resting exactly on a tile edge does not count as overlapping it.
const EDGE_EPSILON: f32 = 1.0e-3;

/// True when an axis-aligned box (center + half extents) overlaps only walkable tiles.
/// Anything touching tiles outside the map, or with non-finite values, is blocked.
pub fn box_is_clear(map: &WorldMap, center: (f32, f32), half: (f32, f32)) -> bool {
    if !half.0.is_finite() || !half.1.is_finite() || half.0 < 0.0 || half.1 < 0.0 { return false; }
    let min = map.world_to_tile((center.0 - half.0, center.1 - half.1));
    let max = map.world_to_tile((center.0 + half.0 - EDGE_EPSILON, center.1 + half.1 - EDGE_EPSILON));
    let (Some(min), Some(max)) = (min, max) else { return false };
    // Reject early so an oversized box can never trigger a long scan.
    if min.x < 0 || min.y < 0 || max.x >= map.width() as i32 || max.y >= map.height() as i32 {
        return false;
    }
    (min.y..=max.y).all(|y| (min.x..=max.x).all(|x| map.can_walk_to(TilePos::new(x, y))))
}

/// Moves along one axis by up to `step`, stopping flush against the first blocked tile.
/// If the full step is blocked, bisects to the farthest clear distance so the player
/// does not stop a visible gap short of a wall.
fn advance(map: &WorldMap, from: (f32, f32), step: f32, horizontal: bool, half: (f32, f32)) -> (f32, f32) {
    let at = |t: f32| if horizontal { (from.0 + step * t, from.1) } else { (from.0, from.1 + step * t) };
    if box_is_clear(map, at(1.0), half) { return at(1.0); }
    let (mut clear, mut blocked) = (0.0_f32, 1.0_f32);
    for _ in 0..10 {
        let mid = (clear + blocked) / 2.0;
        if box_is_clear(map, at(mid), half) { clear = mid; } else { blocked = mid; }
    }
    // `clear == 0.0` returns the original position, including when it started inside a wall.
    at(clear)
}

/// Moves a box by `delta`, stopping at blocked tiles and sliding along walls.
///
/// Axes are resolved independently, in small sub-steps, so fast movement cannot
/// tunnel through a one-tile wall. A box that already starts in a blocked area
/// cannot move; invalid (non-finite) deltas leave the position unchanged.
pub fn resolve_move(
    map: &WorldMap,
    pos: (f32, f32),
    delta: (f32, f32),
    half: (f32, f32),
) -> (f32, f32) {
    if !delta.0.is_finite() || !delta.1.is_finite() { return pos; }
    let max_step = TILE_SIZE / 2.0;
    let steps = (delta.0.abs().max(delta.1.abs()) / max_step).ceil().max(1.0).min(1024.0);
    let step = (delta.0 / steps, delta.1 / steps);
    let mut current = pos;
    for _ in 0..steps as u32 {
        current = advance(map, current, step.0, true, half);
        current = advance(map, current, step.1, false, half);
    }
    current
}

#[cfg(test)]
mod tests {
    use super::*;

    // 5x3 map, top row first. Column 2 is a tree wall except for the middle row.
    //   y=2  . . T . .
    //   y=1  . . . . .
    //   y=0  . . T . .
    fn map() -> WorldMap { WorldMap::from_ascii("..T..\n.....\n..T..").unwrap() }
    const HALF: (f32, f32) = (8.0, 8.0);

    #[test]
    fn free_movement_applies_full_delta() {
        let m = map();
        let start = m.tile_center(TilePos::new(0, 1));
        let end = resolve_move(&m, start, (10.0, 0.0), HALF);
        assert!((end.0 - (start.0 + 10.0)).abs() < 1e-4 && end.1 == start.1);
    }

    #[test]
    fn blocked_tile_stops_the_player_flush_against_it() {
        let m = map();
        let start = m.tile_center(TilePos::new(1, 2));
        let end = resolve_move(&m, start, (200.0, 0.0), HALF);
        let tree_left_edge = m.tile_center(TilePos::new(2, 2)).0 - TILE_SIZE / 2.0;
        assert!(end.0 + HALF.0 <= tree_left_edge + 0.5, "box must not enter the tree tile");
        assert!(end.0 > start.0, "player should still reach the wall");
    }

    #[test]
    fn slides_along_a_wall_instead_of_sticking() {
        let m = map();
        let start = m.tile_center(TilePos::new(1, 2));
        let end = resolve_move(&m, start, (30.0, -20.0), HALF);
        assert!(end.1 < start.1, "vertical component should still apply");
    }

    #[test]
    fn can_pass_through_the_gap() {
        let m = map();
        let start = m.tile_center(TilePos::new(1, 1));
        let end = resolve_move(&m, start, (96.0, 0.0), HALF);
        assert_eq!(m.world_to_tile(end), Some(TilePos::new(4, 1)));
    }

    #[test]
    fn huge_delta_cannot_tunnel_through_a_wall() {
        let m = map();
        let start = m.tile_center(TilePos::new(1, 2));
        let end = resolve_move(&m, start, (500.0, 0.0), HALF);
        assert!(m.world_to_tile(end).unwrap().x <= 1);
    }

    #[test]
    fn map_edge_blocks_movement() {
        let m = map();
        let start = m.tile_center(TilePos::new(0, 1));
        let end = resolve_move(&m, start, (-500.0, 0.0), HALF);
        assert!(end.0 - HALF.0 >= m.tile_center(TilePos::new(0, 1)).0 - TILE_SIZE / 2.0 - 0.5);
        assert!(box_is_clear(&m, end, HALF));
    }

    #[test]
    fn invalid_inputs_do_not_move_or_panic() {
        let m = map();
        let start = m.tile_center(TilePos::new(0, 1));
        assert_eq!(resolve_move(&m, start, (f32::NAN, 1.0), HALF), start);
        assert_eq!(resolve_move(&m, start, (f32::INFINITY, 0.0), HALF), start);
        assert!(!box_is_clear(&m, (f32::NAN, 0.0), HALF));
        assert!(!box_is_clear(&m, start, (-1.0, 4.0)));
        assert!(!box_is_clear(&m, start, (1.0e9, 1.0e9)));
    }

    #[test]
    fn standing_inside_a_wall_is_not_clear_and_cannot_move() {
        let m = map();
        let in_tree = m.tile_center(TilePos::new(2, 2));
        assert!(!box_is_clear(&m, in_tree, HALF));
        assert_eq!(resolve_move(&m, in_tree, (5.0, 0.0), HALF), in_tree);
    }

    #[test]
    fn box_spanning_two_tiles_needs_both_clear() {
        let m = map();
        // Straddles the boundary between (1,1) [grass] and (1,2) [grass] -> fine.
        let c = (m.tile_center(TilePos::new(1, 1)).0, m.tile_center(TilePos::new(1, 1)).1 + TILE_SIZE / 2.0);
        assert!(box_is_clear(&m, c, HALF));
        // Straddles (1,2) and (2,2) [tree] -> blocked.
        let c = (m.tile_center(TilePos::new(1, 2)).0 + TILE_SIZE / 2.0, m.tile_center(TilePos::new(1, 2)).1);
        assert!(!box_is_clear(&m, c, HALF));
    }
}
