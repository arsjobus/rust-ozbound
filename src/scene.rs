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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gathering::validate_node_placement;
    use crate::world::TILE_SIZE;

    fn map() -> WorldMap { meadow_map().expect("meadow map must parse") }

    #[test]
    fn meadow_has_expected_size_and_fits_the_window() {
        let m = map();
        assert_eq!((m.width(), m.height()), (30, 20));
        assert!(m.width() as f32 * TILE_SIZE <= 1000.0 && m.height() as f32 * TILE_SIZE <= 700.0);
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
}
