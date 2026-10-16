//! The built-in starter meadow, plus deterministic per-tile variation for the 3D scene.
//! Kept free of Bevy so both can be unit-tested. The layout itself is level data: see
//! [`crate::level`] and `assets/maps/meadow.map`.

use crate::level::{Level, LevelError};
use crate::world::{TilePos, WorldMap};

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
}
