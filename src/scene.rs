//! The built-in starter meadow, plus deterministic per-tile variation for the 3D scene.
//! Kept free of Bevy so both can be unit-tested. The layout itself is level data: see
//! [`crate::level`] and `assets/maps/meadow.map`.

use crate::gathering::ResourceKind;
use crate::level::{Level, LevelError};
use crate::world::{Tile, TilePos, WorldMap};

/// The starter meadow's level data, compiled in so the game still starts when it is run
/// from another directory. It is the same file as `assets/maps/meadow.map`, which the game
/// loads from disk in preference when present.
pub const DEFAULT_LEVEL: &str = include_str!("../assets/maps/meadow.map");

/// The built-in meadow, parsed and validated.
pub fn meadow_level() -> Result<Level, LevelError> { Level::parse(DEFAULT_LEVEL) }

pub fn meadow_map() -> Result<WorldMap, LevelError> { meadow_level().map(|level| level.map) }

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

/// Which of a fence tile's four orthogonal neighbours are fences too. The renderer draws a
/// rail toward each linked neighbour, so a row of fence tiles reads as one continuous fence.
/// `north` is map `+y`, which is `-z` in the 3D world.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FenceLinks {
    pub east: bool,
    pub west: bool,
    pub north: bool,
    pub south: bool,
}

impl FenceLinks {
    /// The directions to draw rails in, as unit `(dx, dy)` map offsets. A lone post with no
    /// neighbours gets a short east-west rail so it still looks like a piece of fence.
    pub fn rails(self) -> Vec<(i32, i32)> {
        let mut rails = Vec::new();
        if self.east { rails.push((1, 0)); }
        if self.west { rails.push((-1, 0)); }
        if self.north { rails.push((0, 1)); }
        if self.south { rails.push((0, -1)); }
        if rails.is_empty() { rails = vec![(1, 0), (-1, 0)]; }
        rails
    }
}

/// Fence links for the tile at `pos`. Off-map neighbours and non-fence tiles do not link.
pub fn fence_links(map: &WorldMap, pos: TilePos) -> FenceLinks {
    let is_fence = |dx: i32, dy: i32| map.tile_at(TilePos::new(pos.x + dx, pos.y + dy)) == Ok(Tile::Fence);
    FenceLinks { east: is_fence(1, 0), west: is_fence(-1, 0), north: is_fence(0, 1), south: is_fence(0, -1) }
}

/// One tile of a bridge: a path tile with water on both sides of its direction of travel.
/// The 3D layer replaces the flat path slab with a wooden deck, rails, and end posts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BridgeTile {
    pub tile: TilePos,
    /// True when the bridge runs east-west (water lies north and south of it); false when it
    /// runs north-south.
    pub along_x: bool,
    /// True when the next tile *behind* the bridge (west or south) is not part of it.
    pub first: bool,
    /// True when the next tile *ahead* of the bridge (east or north) is not part of it.
    pub last: bool,
}

/// Whether the path tile at `pos` is a bridge tile, and which way it runs. A tile with water
/// on all four sides is a stepping stone, not a bridge; so is a path tile on dry land.
fn bridge_axis(map: &WorldMap, pos: TilePos) -> Option<bool> {
    if map.tile_at(pos) != Ok(Tile::Path) {
        return None;
    }
    let water = |dx: i32, dy: i32| map.tile_at(TilePos::new(pos.x + dx, pos.y + dy)) == Ok(Tile::Water);
    let flanked_north_south = water(0, 1) && water(0, -1);
    let flanked_east_west = water(1, 0) && water(-1, 0);
    match (flanked_north_south, flanked_east_west) {
        (true, false) => Some(true),
        (false, true) => Some(false),
        _ => None,
    }
}

/// Every bridge tile on the map, bottom row first. Bridges are not marked in level files:
/// a path that crosses water is a bridge, so `Tile::is_walkable` stays the only collision rule
/// and the deck can never disagree with where the player may walk.
pub fn bridge_tiles(map: &WorldMap) -> Vec<BridgeTile> {
    let mut out = Vec::new();
    for y in 0..map.height() as i32 {
        for x in 0..map.width() as i32 {
            let tile = TilePos::new(x, y);
            let Some(along_x) = bridge_axis(map, tile) else { continue };
            let same = |dx: i32, dy: i32| bridge_axis(map, TilePos::new(x + dx, y + dy)) == Some(along_x);
            let (back, ahead) = if along_x { ((-1, 0), (1, 0)) } else { ((0, -1), (0, 1)) };
            out.push(BridgeTile { tile, along_x, first: !same(back.0, back.1), last: !same(ahead.0, ahead.1) });
        }
    }
    out
}

/// Which way a vineyard row runs: true for east-west. A row follows its neighbours (the
/// vine tiles left and right, or above and below); a lone vine tile, or a tie, runs east-west.
pub fn vine_runs_along_x(map: &WorldMap, pos: TilePos) -> bool {
    let vine = |dx: i32, dy: i32| map.tile_at(TilePos::new(pos.x + dx, pos.y + dy)) == Ok(Tile::Vine);
    let sideways = usize::from(vine(1, 0)) + usize::from(vine(-1, 0));
    let vertical = usize::from(vine(0, 1)) + usize::from(vine(0, -1));
    sideways >= vertical
}

/// Tree tiles that cannot be harvested: every `T` tile that is not a listed tree resource.
/// They are drawn as dense thickets, so it is clear at a glance which trees can be chopped.
pub fn thicket_tiles(level: &Level) -> Vec<TilePos> {
    let mut out = Vec::new();
    for y in 0..level.map.height() as i32 {
        for x in 0..level.map.width() as i32 {
            let tile = TilePos::new(x, y);
            let harvestable = level.nodes.iter().any(|n| n.tile == tile && n.kind == ResourceKind::Tree);
            if level.map.tile_at(tile) == Ok(Tile::Tree) && !harvestable {
                out.push(tile);
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn map() -> WorldMap { meadow_map().expect("meadow map must parse") }

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

    #[test]
    fn fence_rows_link_along_their_length_and_stop_at_the_ends() {
        let m = WorldMap::from_ascii("FFF\n...").unwrap();
        let left = fence_links(&m, TilePos::new(0, 1));
        let middle = fence_links(&m, TilePos::new(1, 1));
        let right = fence_links(&m, TilePos::new(2, 1));
        assert_eq!(left.rails(), vec![(1, 0)]);
        assert_eq!(middle.rails(), vec![(1, 0), (-1, 0)]);
        assert_eq!(right.rails(), vec![(-1, 0)]);
    }

    #[test]
    fn fence_corners_and_columns_link_in_the_right_directions() {
        // y counts up from the bottom row: the top text row is y = 2.
        let m = WorldMap::from_ascii("F.\nFF\n.F").unwrap();
        let corner = fence_links(&m, TilePos::new(0, 1));
        assert_eq!((corner.east, corner.west, corner.north, corner.south), (true, false, true, false));
        let bottom = fence_links(&m, TilePos::new(1, 0));
        assert_eq!((bottom.east, bottom.west, bottom.north, bottom.south), (false, false, true, false));
        assert_eq!(bottom.rails(), vec![(0, 1)]);
    }

    #[test]
    fn a_lone_fence_post_still_gets_a_rail_and_other_tiles_do_not_link() {
        let m = WorldMap::from_ascii("T#S\n.F=\n~..").unwrap();
        let lone = fence_links(&m, TilePos::new(1, 1));
        assert_eq!((lone.east, lone.west, lone.north, lone.south), (false, false, false, false));
        assert_eq!(lone.rails(), vec![(1, 0), (-1, 0)]);
    }

    #[test]
    fn fences_on_the_map_edge_ignore_off_map_neighbours() {
        let m = WorldMap::from_ascii("F").unwrap();
        let links = fence_links(&m, TilePos::new(0, 0));
        assert_eq!((links.east, links.west, links.north, links.south), (false, false, false, false));
        // Asking about a tile outside the map is harmless.
        assert!(!fence_links(&m, TilePos::new(-5, 9)).east);
    }

    #[test]
    fn the_meadow_fence_rail_links_into_one_continuous_run() {
        let m = meadow_map().unwrap();
        // Fence at x = 11..=15, y = 7: the ends link one way, the middle both ways.
        assert_eq!(fence_links(&m, TilePos::new(11, 7)).rails(), vec![(1, 0)]);
        assert_eq!(fence_links(&m, TilePos::new(13, 7)).rails(), vec![(1, 0), (-1, 0)]);
        assert_eq!(fence_links(&m, TilePos::new(15, 7)).rails(), vec![(-1, 0)]);
    }

    // ---- bridges and thickets ----

    fn bridges_of(text: &str) -> Vec<BridgeTile> { bridge_tiles(&WorldMap::from_ascii(text).unwrap()) }

    #[test]
    fn a_path_across_water_is_a_bridge_running_the_way_you_walk() {
        // Water above and below a path row: an east-west bridge two tiles long.
        let east_west = bridges_of("~~~~\n.==.\n~~~~");
        assert_eq!(east_west.len(), 2);
        assert!(east_west.iter().all(|b| b.along_x));
        assert_eq!((east_west[0].first, east_west[0].last), (true, false));
        assert_eq!((east_west[1].first, east_west[1].last), (false, true));
        // Water left and right of a path column: a north-south bridge.
        let north_south = bridges_of("~.~\n~=~\n~=~\n~=~\n~.~");
        assert_eq!(north_south.len(), 3, "the grass ends of the column are not bridge");
        assert!(north_south.iter().all(|b| !b.along_x));
        assert!(north_south.iter().any(|b| b.first) && north_south.iter().any(|b| b.last));
    }

    #[test]
    fn a_one_tile_causeway_is_both_the_first_and_last_tile() {
        let b = bridges_of("~~~\n.=.\n~~~");
        assert_eq!(b, vec![BridgeTile { tile: TilePos::new(1, 1), along_x: true, first: true, last: true }]);
    }

    #[test]
    fn dry_paths_stepping_stones_and_map_edges_are_not_bridges() {
        assert!(bridges_of("....\n====\n....").is_empty(), "a path on dry land");
        assert!(bridges_of("~~~\n~=~\n~~~").is_empty(), "a stepping stone in a pond");
        assert!(bridges_of("~=.\n~=.").is_empty(), "water on one side only is a riverbank path");
        assert!(bridges_of(".=.\n.=.").is_empty(), "off-map neighbours are not water");
        assert!(bridges_of("~~~\n...\n~~~").is_empty(), "grass is not a bridge");
        assert!(bridges_of("TTT\n===\nTTT").is_empty(), "trees beside a path are not water");
    }

    #[test]
    fn bridge_tiles_are_always_walkable_so_the_deck_never_hides_a_wall() {
        let m = WorldMap::from_ascii("~~~~\n.==.\n~~~~").unwrap();
        for b in bridge_tiles(&m) {
            assert!(m.can_walk_to(b.tile));
        }
    }

    #[test]
    fn bluebrook_has_exactly_one_wooden_bridge_and_it_spans_the_river() {
        let level = Level::parse(crate::village::BLUEBROOK_MAP).unwrap();
        let bridges = bridge_tiles(&level.map);
        assert!(bridges.len() >= 2, "the river crossing needs at least two deck tiles");
        // Every deck tile belongs to one connected run along the main road.
        assert!(bridges.iter().all(|b| b.along_x && b.tile.y == bridges[0].tile.y));
        assert_eq!(bridges.iter().filter(|b| b.first).count(), 1);
        assert_eq!(bridges.iter().filter(|b| b.last).count(), 1);
        assert!(bridges.windows(2).all(|w| w[1].tile.x == w[0].tile.x + 1), "consecutive tiles");
    }

    #[test]
    fn thickets_are_exactly_the_tree_tiles_nobody_can_chop() {
        let level = Level::parse(crate::village::BLUEBROOK_MAP).unwrap();
        let thickets = thicket_tiles(&level);
        assert!(thickets.len() > 40, "the forest edge should be dense; found {}", thickets.len());
        for tile in &thickets {
            assert_eq!(level.map.tile_at(*tile), Ok(Tile::Tree));
            assert!(!level.nodes.iter().any(|n| n.tile == *tile), "{tile:?} is harvestable");
        }
        // Every listed tree is a tree tile and is not a thicket.
        for node in level.nodes.iter().filter(|n| n.kind == ResourceKind::Tree) {
            assert!(!thickets.contains(&node.tile), "{:?}", node.tile);
        }
        // Together they cover every tree tile on the map.
        let trees = (0..level.map.height() as i32)
            .flat_map(|y| (0..level.map.width() as i32).map(move |x| TilePos::new(x, y)))
            .filter(|t| level.map.tile_at(*t) == Ok(Tile::Tree))
            .count();
        let chop = level.nodes.iter().filter(|n| n.kind == ResourceKind::Tree).count();
        assert_eq!(thickets.len() + chop, trees);
    }

    #[test]
    fn thickets_block_movement_like_any_tree() {
        let level = Level::parse(crate::village::BLUEBROOK_MAP).unwrap();
        for tile in thicket_tiles(&level) {
            assert!(!level.map.can_walk_to(tile), "{tile:?}");
        }
    }

    #[test]
    fn vine_rows_follow_their_neighbours() {
        let across = WorldMap::from_ascii("vvvv").unwrap();
        assert!((0..4).all(|x| vine_runs_along_x(&across, TilePos::new(x, 0))));
        let down = WorldMap::from_ascii("v\nv\nv").unwrap();
        assert!((0..3).all(|y| !vine_runs_along_x(&down, TilePos::new(0, y))));
        let lone = WorldMap::from_ascii("...\n.v.\n...").unwrap();
        assert!(vine_runs_along_x(&lone, TilePos::new(1, 1)), "a lone vine runs east-west");
        let corner = WorldMap::from_ascii("vv\nv.").unwrap();
        assert!(vine_runs_along_x(&corner, TilePos::new(0, 1)), "a tie runs east-west");
    }
}
