//! Pure description of the starter meadow: map layout and where things stand on it.
//! Kept free of Bevy so the layout can be validated by unit tests.

use crate::gathering::ResourceKind;
use crate::world::{MapError, TilePos, WorldMap};

/// Top line is the top of the map. `.` grass, `=` path, `~` water, `T` tree, `#` rock.
pub const MEADOW_MAP: &str = "\
TTTTTTTTTTTTTTTTTTTTTTTTTTTTTT
T............................T
T............................T
T.....T....................#.T
T............................T
T...=========................T
T.....................=......T
T.....................=......T
T...........~~~.......=......T
T...#.......~~~.......=......T
T...........~~~.......=......T
T.....................=......T
T.....................=......T
T.........T...........=...T..T
T.....................=......T
T.....................=......T
T..========================..T
T............................T
T............................T
TTTTTTTTTTTTTTTTTTTTTTTTTTTTTT
";

pub const PLAYER_START: TilePos = TilePos::new(8, 3);
pub const GUIDE_TILE: TilePos = TilePos::new(24, 9);
pub const MOONPETAL_TILES: [TilePos; 3] = [TilePos::new(14, 2), TilePos::new(21, 3), TilePos::new(23, 12)];

/// A gatherable resource standing on a specific tile.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NodeSpawn {
    pub tile: TilePos,
    pub kind: ResourceKind,
    pub actions: u32,
    pub yield_per_action: u32,
}

/// Trees and copper veins the player can harvest. Each sits on a blocking tile and is
/// gathered from an adjacent walkable tile.
pub const RESOURCE_NODES: [NodeSpawn; 5] = [
    NodeSpawn { tile: TilePos::new(6, 16), kind: ResourceKind::Tree, actions: 3, yield_per_action: 1 },
    NodeSpawn { tile: TilePos::new(10, 6), kind: ResourceKind::Tree, actions: 3, yield_per_action: 1 },
    NodeSpawn { tile: TilePos::new(26, 6), kind: ResourceKind::Tree, actions: 3, yield_per_action: 1 },
    NodeSpawn { tile: TilePos::new(4, 10), kind: ResourceKind::CopperVein, actions: 2, yield_per_action: 1 },
    NodeSpawn { tile: TilePos::new(27, 16), kind: ResourceKind::CopperVein, actions: 2, yield_per_action: 1 },
];

pub fn meadow_map() -> Result<WorldMap, MapError> { WorldMap::from_ascii(MEADOW_MAP) }

/// Deterministic pseudo-random value in `[0, 1)` for a tile. Gives the low-poly meadow its
/// hand-placed look (shade, tree size, rotation) without a random-number dependency, and
/// keeps the same world on every run. Different `salt`s give independent values.
pub fn tile_noise(pos: TilePos, salt: u32) -> f32 {
    let mut h = (pos.x as u32).wrapping_mul(0x9E37_79B1)
        ^ (pos.y as u32).wrapping_mul(0x85EB_CA77)
        ^ salt.wrapping_mul(0xC2B2_AE3D);
    h ^= h >> 15;
    h = h.wrapping_mul(0x2C1B_3C6D);
    h ^= h >> 12;
    h = h.wrapping_mul(0x297A_2D39);
    h ^= h >> 15;
    (h >> 8) as f32 / (1u32 << 24) as f32
}

/// Picks one of `count` variants (for example shades) for a tile. Always `0` when `count <= 1`.
pub fn tile_variant(pos: TilePos, salt: u32, count: usize) -> usize {
    ((tile_noise(pos, salt) * count as f32) as usize).min(count.saturating_sub(1))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gathering::validate_node_placement;
    use crate::world::TILE_SIZE;

    fn map() -> WorldMap { meadow_map().expect("meadow map must parse") }

    #[test]
    fn meadow_has_expected_size() {
        let m = map();
        assert_eq!((m.width(), m.height()), (30, 20));
        // One tile is one world unit, so the whole meadow is 30 x 20 units in 3D.
        assert_eq!(m.width() as f32 * TILE_SIZE, 30.0);
    }

    #[test]
    fn start_guide_and_petals_stand_on_walkable_reachable_tiles() {
        let m = map();
        assert!(m.can_walk_to(PLAYER_START));
        assert!(m.is_reachable(PLAYER_START, GUIDE_TILE), "guide unreachable");
        for petal in MOONPETAL_TILES {
            assert_eq!(validate_node_placement(&m, petal, ResourceKind::Moonpetal), Ok(()));
            assert!(m.is_reachable(PLAYER_START, petal), "petal at {petal:?} unreachable");
        }
    }

    #[test]
    fn every_resource_node_is_valid_distinct_and_harvestable() {
        let m = map();
        for (i, node) in RESOURCE_NODES.iter().enumerate() {
            assert_eq!(validate_node_placement(&m, node.tile, node.kind), Ok(()), "node {i}");
            assert!(node.actions > 0 && node.yield_per_action > 0, "node {i} would never yield");
            assert!(RESOURCE_NODES[..i].iter().all(|o| o.tile != node.tile), "duplicate node tile");
            let reachable_neighbour = [(1, 0), (-1, 0), (0, 1), (0, -1)].iter().any(|(dx, dy)| {
                let n = TilePos::new(node.tile.x + dx, node.tile.y + dy);
                m.is_reachable(PLAYER_START, n)
            });
            assert!(reachable_neighbour, "node {i} has no reachable adjacent tile");
        }
    }

    #[test]
    fn click_routes_reach_the_guide_petals_and_resources_from_the_start() {
        use crate::pathfinding::{plan_route, Approach};
        let m = map();
        let ends_beside = |route: &Vec<TilePos>, target: TilePos| {
            crate::pathfinding::is_orthogonally_adjacent(route.last().copied().unwrap_or(PLAYER_START), target)
        };
        let to_guide = plan_route(&m, PLAYER_START, GUIDE_TILE, Approach::Adjacent).expect("guide route");
        assert!(ends_beside(&to_guide, GUIDE_TILE));
        for petal in MOONPETAL_TILES {
            let route = plan_route(&m, PLAYER_START, petal, Approach::OnTile).expect("petal route");
            assert_eq!(route.last(), Some(&petal));
        }
        for node in RESOURCE_NODES {
            let route = plan_route(&m, PLAYER_START, node.tile, Approach::Adjacent).expect("node route");
            assert!(ends_beside(&route, node.tile), "route to {:?} must end beside it", node.tile);
        }
    }

    #[test]
    fn tile_noise_is_deterministic_in_range_and_salt_sensitive() {
        let m = map();
        let mut differing_salts = 0;
        for y in 0..m.height() as i32 {
            for x in 0..m.width() as i32 {
                let pos = TilePos::new(x, y);
                let n = tile_noise(pos, 1);
                assert!((0.0..1.0).contains(&n), "{n} out of range at {pos:?}");
                assert_eq!(n, tile_noise(pos, 1), "must be deterministic");
                if n != tile_noise(pos, 2) { differing_salts += 1; }
            }
        }
        assert!(differing_salts > 500, "salts should give independent values");
    }

    #[test]
    fn tile_variants_use_every_shade_and_stay_in_bounds() {
        let m = map();
        let mut seen = [false; 3];
        for y in 0..m.height() as i32 {
            for x in 0..m.width() as i32 {
                let v = tile_variant(TilePos::new(x, y), 7, 3);
                assert!(v < 3);
                seen[v] = true;
            }
        }
        assert_eq!(seen, [true; 3], "every shade should appear somewhere on the meadow");
        assert_eq!(tile_variant(TilePos::new(3, 3), 7, 1), 0);
        assert_eq!(tile_variant(TilePos::new(3, 3), 7, 0), 0);
    }
}
