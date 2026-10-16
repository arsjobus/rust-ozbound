//! The built-in starter meadow, plus deterministic per-tile variation for the 3D scene.
//! Kept free of Bevy so both can be unit-tested. The layout itself is level data: see
//! [`crate::level`] and `assets/maps/meadow.map`.

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
}
