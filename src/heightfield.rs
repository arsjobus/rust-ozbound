//! The height of the ground: gentle rolling hills that stay flat where something is built.
//!
//! Pure Rust (no Bevy). Heights belong to tile **corners** (vertices), not to tiles, so the
//! ground is one continuous surface made of triangles: every tile is two triangles over its four
//! corners, and neighbouring tiles share corners exactly. Gameplay never reads a height:
//! walkability, routes, and interaction range are still decided on tiles (`Tile::is_walkable`).
//! Heights only decide where things are drawn and where a click lands.
//!
//! # The rule
//!
//! A corner touches up to four tiles. A tile of some kinds forces the corners it touches to a
//! fixed level so that what stands there sits flat:
//!
//! | tile | level |
//! | --- | --- |
//! | house wall, doorway, floor, garden bed, vine row | `0.0` ([`STRUCTURE_LEVEL`]) |
//! | path | `-0.02` ([`PATH_LEVEL`]) |
//! | water and bridge decks (the river runs under them) | `-0.2` ([`WATER_LEVEL`]) |
//!
//! When a corner touches several kinds the first row of the table wins, so a doorstep stays level
//! with the floor and a bridge's end meets the road. Every other corner is rolling ground: a
//! deterministic value noise of its world position (up to [`AMPLITUDE`]), faded in over
//! [`RAMP_TILES`] tiles of distance from the nearest forced tile, so hills rise gradually away
//! from paths, water, and buildings and never form a cliff next to them.
//!
//! Only the tile kinds in the table matter. Trees, rocks, fences, signposts, thickets, and plain
//! grass do not change the ground, so moving or removing them in the editor never moves a hill.
//!
//! # Stitching
//!
//! A corner's height is a pure function of its own position and the tiles around it, asked of a
//! [`TileLookup`] that answers for *any* world tile (the overworld knows every chunk, loaded or
//! not). Two chunks therefore agree on the corners they share whichever loads first, and so do
//! the terrain mesh, the height of a person standing on a tile, and a click on the ground.

use crate::world::{Tile, TilePos};

/// Height of corners touching a built tile (wall, door, floor, garden, vine).
pub const STRUCTURE_LEVEL: f32 = 0.0;
/// Height of corners touching a path tile.
pub const PATH_LEVEL: f32 = -0.02;
/// Height of corners touching water or a bridge deck.
pub const WATER_LEVEL: f32 = -0.2;
/// The tallest a hill gets above `0.0`.
pub const AMPLITUDE: f32 = 0.55;
/// How many tiles from a forced tile the hills take to reach full height.
pub const RAMP_TILES: i32 = 3;

/// The tile at a world tile and whether it is a bridge deck, or `None` beyond the world.
pub type TileLookup<'a> = &'a dyn Fn(TilePos) -> Option<(Tile, bool)>;

/// Why a tile forces the corners it touches, in the order they win.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Flat {
    Structure,
    Path,
    Water,
}

impl Flat {
    pub fn level(self) -> f32 {
        match self {
            Self::Structure => STRUCTURE_LEVEL,
            Self::Path => PATH_LEVEL,
            Self::Water => WATER_LEVEL,
        }
    }
}

/// The flattening a tile causes, if any.
pub fn flat_class(tile: Tile, bridge: bool) -> Option<Flat> {
    match tile {
        Tile::Wall | Tile::Door | Tile::Floor | Tile::Garden | Tile::Vine => Some(Flat::Structure),
        Tile::Path if bridge => Some(Flat::Water),
        Tile::Path => Some(Flat::Path),
        Tile::Water => Some(Flat::Water),
        Tile::Grass | Tile::Tree | Tile::Rock | Tile::Fence | Tile::Sign => None,
    }
}

fn hash(i: i32, j: i32, seed: u32) -> f32 {
    let mut h = (i as u32).wrapping_mul(0x9E37_79B1) ^ (j as u32).wrapping_mul(0x85EB_CA77) ^ seed.wrapping_mul(0xC2B2_AE3D);
    h ^= h >> 15;
    h = h.wrapping_mul(0x2C1B_3C6D);
    h ^= h >> 12;
    h = h.wrapping_mul(0x297A_2D39);
    h ^= h >> 15;
    h as f32 / u32::MAX as f32
}

fn smooth(t: f32) -> f32 {
    t * t * (3.0 - 2.0 * t)
}

/// Value noise in `[0, 1]` with lattice points every `spacing` tiles.
fn value_noise(x: f32, y: f32, spacing: f32, seed: u32) -> f32 {
    let (fx, fy) = (x / spacing, y / spacing);
    let (ix, iy) = (fx.floor(), fy.floor());
    let (tx, ty) = (smooth(fx - ix), smooth(fy - iy));
    let (ix, iy) = (ix as i32, iy as i32);
    let top = hash(ix, iy, seed) * (1.0 - tx) + hash(ix + 1, iy, seed) * tx;
    let bottom = hash(ix, iy + 1, seed) * (1.0 - tx) + hash(ix + 1, iy + 1, seed) * tx;
    top * (1.0 - ty) + bottom * ty
}

/// The unflattened rolling height of the corner `(i, j)`, in `[0, AMPLITUDE]`.
pub fn rolling_height(i: i32, j: i32) -> f32 {
    let (x, y) = (i as f32, j as f32);
    let broad = value_noise(x, y, 9.0, 0x0AB1);
    let fine = value_noise(x, y, 4.0, 0x0AB2);
    (0.72 * broad + 0.28 * fine).clamp(0.0, 1.0) * AMPLITUDE
}

/// The four tiles a corner touches: the corner `(i, j)` is the south-west corner of tile
/// `(i, j)` (map `y` grows north).
fn touching(i: i32, j: i32) -> [TilePos; 4] {
    [TilePos::new(i - 1, j - 1), TilePos::new(i, j - 1), TilePos::new(i - 1, j), TilePos::new(i, j)]
}

/// The height of corner `(i, j)` (the south-west corner of tile `(i, j)`), by the rule in the
/// module docs.
pub fn vertex_height(lookup: TileLookup, i: i32, j: i32) -> f32 {
    let class_of = |pos: TilePos| lookup(pos).and_then(|(tile, bridge)| flat_class(tile, bridge));
    if let Some(forced) = touching(i, j).into_iter().filter_map(class_of).min() {
        return forced.level();
    }
    // Distance from the corner to the nearest forcing tile, in tiles (to the tile's edge).
    let mut nearest = f32::INFINITY;
    for ty in (j - RAMP_TILES - 1)..=(j + RAMP_TILES) {
        for tx in (i - RAMP_TILES - 1)..=(i + RAMP_TILES) {
            if class_of(TilePos::new(tx, ty)).is_none() {
                continue;
            }
            let dx = (tx - i).max(0).max(i - (tx + 1)) as f32;
            let dy = (ty - j).max(0).max(j - (ty + 1)) as f32;
            nearest = nearest.min(dx.hypot(dy));
        }
    }
    let ramp = if nearest.is_finite() { smooth(((nearest - 0.5) / (RAMP_TILES as f32 - 0.5)).clamp(0.0, 1.0)) } else { 1.0 };
    rolling_height(i, j) * ramp
}

/// How a tile is cut into two triangles: along the corner `(x, y)`-`(x + 1, y + 1)` when `true`,
/// along `(x + 1, y)`-`(x, y + 1)` when `false`. Alternating by parity keeps hills from leaning
/// one way.
pub fn splits_south_west_to_north_east(pos: TilePos) -> bool {
    (pos.x + pos.y).rem_euclid(2) == 0
}

/// The heights of one tile's corners: `[south-west, south-east, north-east, north-west]`.
pub type Corners = [f32; 4];

/// A block of corner heights, so that entities and clicks can ask the ground for its height
/// without asking the whole world again. Outside the block the ground is flat at `0.0`.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct HeightGrid {
    x0: i32,
    y0: i32,
    cols: usize,
    rows: usize,
    heights: Vec<f32>,
    /// Half the width and height of the map the world frame is centred on (see
    /// `WorldMap::tile_center`).
    half: (f32, f32),
}

impl HeightGrid {
    /// The corners of tiles `x0..x1` by `y0..y1` (so `x1 - x0 + 1` by `y1 - y0 + 1` corners).
    /// `half` is half the map's width and height (the world frame is centred on the map).
    pub fn build(x0: i32, y0: i32, x1: i32, y1: i32, half: (f32, f32), lookup: TileLookup) -> Self {
        let cols = (x1 - x0 + 1).max(0) as usize;
        let rows = (y1 - y0 + 1).max(0) as usize;
        let mut heights = Vec::with_capacity(cols * rows);
        for j in 0..rows as i32 {
            for i in 0..cols as i32 {
                heights.push(vertex_height(lookup, x0 + i, y0 + j));
            }
        }
        Self { x0, y0, cols, rows, heights, half }
    }

    /// A grid with no corners: the ground is flat everywhere.
    pub fn flat(half: (f32, f32)) -> Self {
        Self { half, ..Self::default() }
    }

    pub fn is_empty(&self) -> bool {
        self.heights.is_empty()
    }

    /// True when the block holds all four corners of this tile.
    pub fn covers_tile(&self, pos: TilePos) -> bool {
        let (dx, dy) = (pos.x - self.x0, pos.y - self.y0);
        dx >= 0 && dy >= 0 && (dx as usize) + 1 < self.cols && (dy as usize) + 1 < self.rows
    }

    /// The height of corner `(i, j)`, or `0.0` outside the block.
    pub fn vertex(&self, i: i32, j: i32) -> f32 {
        let (dx, dy) = (i - self.x0, j - self.y0);
        if dx < 0 || dy < 0 || dx as usize >= self.cols || dy as usize >= self.rows {
            return 0.0;
        }
        self.heights[dy as usize * self.cols + dx as usize]
    }

    /// The corners of a tile.
    pub fn corners(&self, pos: TilePos) -> Corners {
        corners_with(&|i, j| self.vertex(i, j), pos)
    }

    /// The height at the middle of a tile: where people and plants stand. The middle lies on the
    /// tile's diagonal, so it is the mean of that diagonal's two corners.
    pub fn tile_height(&self, pos: TilePos) -> f32 {
        let [sw, se, ne, nw] = self.corners(pos);
        if splits_south_west_to_north_east(pos) {
            (sw + ne) / 2.0
        } else {
            (se + nw) / 2.0
        }
    }

    /// The height of the surface at `(u, v)` in tile space (tile `(x, y)` covers `x..x + 1` by
    /// `y..y + 1`), on the triangle that is actually drawn there.
    pub fn height_at_tile_space(&self, u: f32, v: f32) -> f32 {
        if !u.is_finite() || !v.is_finite() {
            return 0.0;
        }
        let pos = TilePos::new(u.floor() as i32, v.floor() as i32);
        let (fx, fy) = (u - pos.x as f32, v - pos.y as f32);
        triangle_height(self.corners(pos), splits_south_west_to_north_east(pos), fx, fy)
    }

    /// The height of the ground at a world point `(x, z)` (north is `-z`).
    pub fn height_at_world(&self, x: f32, z: f32) -> f32 {
        self.height_at_tile_space(x + self.half.0, -z + self.half.1)
    }
}

/// The height blocks of every scene the game has built so far (one per overworld chunk, or the
/// single block of a region off the overworld), so heights can be asked of the whole world at
/// once. Blocks agree on the corners they share (see the module docs), so which block answers
/// does not matter. Ground nobody has built a block for is flat at `0.0`.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct HeightSet {
    half: (f32, f32),
    blocks: Vec<(String, HeightGrid)>,
}

impl HeightSet {
    /// An empty set for a world frame centred on a map of this half width and half height.
    pub fn new(half: (f32, f32)) -> Self {
        Self { half, blocks: Vec::new() }
    }

    pub fn half(&self) -> (f32, f32) {
        self.half
    }

    pub fn has(&self, scene: &str) -> bool {
        self.blocks.iter().any(|(id, _)| id == scene)
    }

    /// Adds (or replaces) the block of a scene.
    pub fn insert(&mut self, scene: &str, grid: HeightGrid) {
        match self.blocks.iter_mut().find(|(id, _)| id == scene) {
            Some(slot) => slot.1 = grid,
            None => self.blocks.push((scene.to_string(), grid)),
        }
    }

    pub fn len(&self) -> usize {
        self.blocks.len()
    }

    pub fn is_empty(&self) -> bool {
        self.blocks.is_empty()
    }

    /// The height at the middle of a tile, where people and plants stand.
    pub fn tile_height(&self, pos: TilePos) -> f32 {
        self.blocks.iter().find(|(_, g)| g.covers_tile(pos)).map_or(0.0, |(_, g)| g.tile_height(pos))
    }

    /// The height of the surface at a world point `(x, z)` (north is `-z`).
    pub fn height_at_world(&self, x: f32, z: f32) -> f32 {
        if !x.is_finite() || !z.is_finite() {
            return 0.0;
        }
        let (u, v) = (x + self.half.0, -z + self.half.1);
        let pos = TilePos::new(u.floor() as i32, v.floor() as i32);
        self.blocks.iter().find(|(_, g)| g.covers_tile(pos)).map_or(0.0, |(_, g)| g.height_at_tile_space(u, v))
    }
}

/// The corners of a tile from any source of corner heights.
pub fn corners_with(vertex: &dyn Fn(i32, i32) -> f32, pos: TilePos) -> Corners {
    [vertex(pos.x, pos.y), vertex(pos.x + 1, pos.y), vertex(pos.x + 1, pos.y + 1), vertex(pos.x, pos.y + 1)]
}

/// Height on a tile's surface at `(fx, fy)` inside the tile (both in `[0, 1]`), where `south_west`
/// says which diagonal cuts it.
pub fn triangle_height(corners: Corners, south_west: bool, fx: f32, fy: f32) -> f32 {
    let [sw, se, ne, nw] = corners;
    let (fx, fy) = (fx.clamp(0.0, 1.0), fy.clamp(0.0, 1.0));
    if south_west {
        // Diagonal sw-ne: the lower-right triangle is sw, se, ne; the upper-left is sw, ne, nw.
        if fx >= fy {
            sw + (se - sw) * (fx - fy) + (ne - sw) * fy
        } else {
            sw + (ne - sw) * fx + (nw - sw) * (fy - fx)
        }
    } else {
        // Diagonal se-nw: the lower-left triangle is sw, se, nw; the upper-right is se, ne, nw.
        if fx + fy <= 1.0 {
            sw + (se - sw) * fx + (nw - sw) * fy
        } else {
            ne + (nw - ne) * (1.0 - fx) + (se - ne) * (1.0 - fy)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn open(_: TilePos) -> Option<(Tile, bool)> {
        Some((Tile::Grass, false))
    }

    /// A lookup over an ASCII picture (first row is the highest y), grass beyond it.
    fn picture(rows: &'static str) -> impl Fn(TilePos) -> Option<(Tile, bool)> {
        let lines: Vec<&str> = rows.lines().collect();
        move |pos| {
            let height = lines.len() as i32;
            let row = lines.get((height - 1 - pos.y).max(0) as usize).filter(|_| pos.y >= 0 && pos.y < height)?;
            let tile = match row.chars().nth(pos.x.max(0) as usize).filter(|_| pos.x >= 0)? {
                '~' => Tile::Water,
                '=' => Tile::Path,
                '#' => Tile::Wall,
                'g' => Tile::Garden,
                'T' => Tile::Tree,
                _ => Tile::Grass,
            };
            Some((tile, false))
        }
    }

    #[test]
    fn each_kind_of_tile_forces_its_own_level() {
        assert_eq!(flat_class(Tile::Wall, false), Some(Flat::Structure));
        assert_eq!(flat_class(Tile::Door, false), Some(Flat::Structure));
        assert_eq!(flat_class(Tile::Floor, false), Some(Flat::Structure));
        assert_eq!(flat_class(Tile::Garden, false), Some(Flat::Structure));
        assert_eq!(flat_class(Tile::Vine, false), Some(Flat::Structure));
        assert_eq!(flat_class(Tile::Path, false), Some(Flat::Path));
        assert_eq!(flat_class(Tile::Path, true), Some(Flat::Water), "a bridge deck is over water");
        assert_eq!(flat_class(Tile::Water, false), Some(Flat::Water));
        for tile in [Tile::Grass, Tile::Tree, Tile::Rock, Tile::Fence, Tile::Sign] {
            assert_eq!(flat_class(tile, false), None, "{tile:?} does not change the ground");
        }
        assert_eq!(Flat::Structure.level(), 0.0);
        assert!(Flat::Water.level() < Flat::Path.level() && Flat::Path.level() <= Flat::Structure.level());
    }

    #[test]
    fn rolling_ground_is_deterministic_and_stays_in_range() {
        for j in -40..40 {
            for i in -40..40 {
                let h = rolling_height(i, j);
                assert!((0.0..=AMPLITUDE).contains(&h), "({i}, {j}): {h}");
                assert_eq!(h, rolling_height(i, j));
            }
        }
        let values: Vec<f32> = (0..64).map(|i| rolling_height(i, 5)).collect();
        let (low, high) = values.iter().fold((f32::MAX, f32::MIN), |(lo, hi), v| (lo.min(*v), hi.max(*v)));
        assert!(high - low > 0.15, "the ground actually varies ({low}..{high})");
    }

    #[test]
    fn neighbouring_corners_never_differ_by_a_cliff() {
        let mut steepest = 0.0f32;
        for j in -30..30 {
            for i in -30..30 {
                let h = rolling_height(i, j);
                steepest = steepest.max((h - rolling_height(i + 1, j)).abs()).max((h - rolling_height(i, j + 1)).abs());
            }
        }
        assert!(steepest < 0.25, "steepest step between neighbouring corners: {steepest}");
    }

    #[test]
    fn corners_touching_built_ground_are_forced_and_the_first_kind_wins() {
        let lookup = picture("~~=##\n~~=##\n=====\n");
        // The corner between the water, the path, and the wall rows: structure beats path beats water.
        assert_eq!(vertex_height(&lookup, 3, 2), STRUCTURE_LEVEL, "wall corner");
        assert_eq!(vertex_height(&lookup, 2, 1), PATH_LEVEL, "path against water");
        assert_eq!(vertex_height(&lookup, 1, 2), WATER_LEVEL, "inside the water");
        assert_eq!(vertex_height(&lookup, 0, 3), WATER_LEVEL, "outer corner of the water");
    }

    #[test]
    fn hills_fade_in_with_distance_from_forced_ground() {
        // One row of path along y = 0, grass everywhere else.
        let strip = |pos: TilePos| Some((if pos.y == 0 { Tile::Path } else { Tile::Grass }, false));
        let h = |j: i32| vertex_height(&strip, 20, j);
        assert_eq!(h(0), PATH_LEVEL);
        assert_eq!(h(1), PATH_LEVEL, "the corner above the path row touches it");
        // Away from the strip the height approaches the full rolling value.
        assert!(h(2) <= rolling_height(20, 2) * 0.12 + 1e-6, "one tile away it is mostly flat");
        assert!(h(3) > h(2) - 1e-6 || rolling_height(20, 3) < rolling_height(20, 2), "and it rises with distance");
        assert!((h(8) - rolling_height(20, 8)).abs() < 1e-6, "far away it is the rolling ground itself");
        assert!(h(2).abs() <= rolling_height(20, 2) + 1e-6);
    }

    #[test]
    fn a_whole_meadow_of_grass_is_just_rolling_ground() {
        for (i, j) in [(0, 0), (5, 9), (-3, 4), (30, 30)] {
            assert_eq!(vertex_height(&open, i, j), rolling_height(i, j));
        }
    }

    #[test]
    fn nothing_beyond_the_world_forces_anything() {
        let nowhere = |_: TilePos| None;
        assert_eq!(vertex_height(&nowhere, 3, 3), rolling_height(3, 3));
    }

    #[test]
    fn corner_heights_do_not_depend_on_which_scene_asks() {
        // The same lookup answers for the whole world, so a block built in pieces matches one built whole.
        let lookup = picture("~~====\n~=g###\n=====~\n..T..~\n");
        let whole = HeightGrid::build(-2, -2, 9, 7, (3.0, 2.0), &lookup);
        let left = HeightGrid::build(-2, -2, 3, 7, (3.0, 2.0), &lookup);
        let right = HeightGrid::build(3, -2, 9, 7, (3.0, 2.0), &lookup);
        for j in -2..=7 {
            for i in -2..=9 {
                assert_eq!(whole.vertex(i, j), vertex_height(&lookup, i, j));
                if i <= 3 {
                    assert_eq!(left.vertex(i, j), whole.vertex(i, j));
                }
                if i >= 3 {
                    assert_eq!(right.vertex(i, j), whole.vertex(i, j), "the shared column agrees");
                }
            }
        }
    }

    #[test]
    fn the_height_of_a_tile_is_the_mean_of_its_diagonal() {
        let grid = HeightGrid::build(0, 0, 4, 4, (2.0, 2.0), &open);
        for y in 0..4 {
            for x in 0..4 {
                let pos = TilePos::new(x, y);
                let [sw, se, ne, nw] = grid.corners(pos);
                let expected = if splits_south_west_to_north_east(pos) { (sw + ne) / 2.0 } else { (se + nw) / 2.0 };
                assert_eq!(grid.tile_height(pos), expected);
                assert!((grid.height_at_tile_space(x as f32 + 0.5, y as f32 + 0.5) - expected).abs() < 1e-5, "the middle lies on the diagonal");
            }
        }
    }

    #[test]
    fn the_surface_passes_through_every_corner_and_is_continuous_across_tiles() {
        let grid = HeightGrid::build(-1, -1, 6, 6, (3.0, 3.0), &open);
        for j in 0..=5 {
            for i in 0..=5 {
                let at = grid.height_at_tile_space(i as f32, j as f32);
                assert!((at - grid.vertex(i, j)).abs() < 1e-5, "corner ({i}, {j})");
            }
        }
        // Approaching a shared edge from either side gives the same height.
        for y in 0..4 {
            for x in 0..4 {
                for step in 0..=4 {
                    let t = step as f32 / 4.0;
                    let east = (grid.height_at_tile_space(x as f32 + 1.0 - 1e-4, y as f32 + t) - grid.height_at_tile_space(x as f32 + 1.0 + 1e-4, y as f32 + t)).abs();
                    let north = (grid.height_at_tile_space(x as f32 + t, y as f32 + 1.0 - 1e-4) - grid.height_at_tile_space(x as f32 + t, y as f32 + 1.0 + 1e-4)).abs();
                    assert!(east < 1e-3 && north < 1e-3, "({x}, {y}) t={t}: {east} {north}");
                }
            }
        }
    }

    #[test]
    fn the_height_between_corners_never_leaves_the_range_of_its_corners() {
        let grid = HeightGrid::build(0, 0, 5, 5, (2.5, 2.5), &open);
        for y in 0..5 {
            for x in 0..5 {
                let pos = TilePos::new(x, y);
                let corners = grid.corners(pos);
                let (low, high) = corners.iter().fold((f32::MAX, f32::MIN), |(lo, hi), v| (lo.min(*v), hi.max(*v)));
                for sy in 0..=8 {
                    for sx in 0..=8 {
                        let h = grid.height_at_tile_space(x as f32 + sx as f32 / 8.0, y as f32 + sy as f32 / 8.0);
                        assert!(h >= low - 1e-5 && h <= high + 1e-5, "({x}, {y}) {sx},{sy}: {h} not in {low}..{high}");
                    }
                }
            }
        }
    }

    #[test]
    fn world_points_map_through_the_centred_frame() {
        // A 4 x 4 map centred on the origin: tile (0, 0) is centred at world (-1.5, +1.5).
        let grid = HeightGrid::build(0, 0, 4, 4, (2.0, 2.0), &open);
        let tile = TilePos::new(0, 0);
        assert!((grid.height_at_world(-1.5, 1.5) - grid.tile_height(tile)).abs() < 1e-6);
        let far = TilePos::new(3, 3);
        assert!((grid.height_at_world(1.5, -1.5) - grid.tile_height(far)).abs() < 1e-6);
    }

    #[test]
    fn outside_the_block_and_for_bad_input_the_ground_is_flat() {
        let grid = HeightGrid::build(0, 0, 3, 3, (1.5, 1.5), &open);
        assert_eq!(grid.vertex(-1, 0), 0.0);
        assert_eq!(grid.vertex(0, 99), 0.0);
        assert_eq!(grid.height_at_tile_space(f32::NAN, 1.0), 0.0);
        assert_eq!(grid.height_at_world(f32::INFINITY, 0.0), 0.0);
        let flat = HeightGrid::flat((1.0, 1.0));
        assert!(flat.is_empty());
        assert_eq!(flat.tile_height(TilePos::new(2, 2)), 0.0);
        assert_eq!(HeightGrid::build(5, 5, 4, 4, (0.0, 0.0), &open).vertex(5, 5), 0.0, "an inverted block is empty");
    }

    #[test]
    fn both_diagonals_are_used_and_interpolate_their_own_triangles() {
        assert!(splits_south_west_to_north_east(TilePos::new(0, 0)));
        assert!(!splits_south_west_to_north_east(TilePos::new(1, 0)));
        assert!(splits_south_west_to_north_east(TilePos::new(-1, -1)));
        // A saddle: sw and ne low, se and nw high. The cut decides the middle.
        let corners = [0.0, 1.0, 0.0, 1.0];
        assert_eq!(triangle_height(corners, true, 0.5, 0.5), 0.0);
        assert_eq!(triangle_height(corners, false, 0.5, 0.5), 1.0);
        for south_west in [true, false] {
            assert_eq!(triangle_height(corners, south_west, 0.0, 0.0), 0.0);
            assert_eq!(triangle_height(corners, south_west, 1.0, 0.0), 1.0);
            assert_eq!(triangle_height(corners, south_west, 1.0, 1.0), 0.0);
            assert_eq!(triangle_height(corners, south_west, 0.0, 1.0), 1.0);
        }
    }

    #[test]
    fn a_height_set_answers_from_whichever_block_covers_the_tile() {
        let lookup = picture("~~====\n~=g###\n=====~\n..T..~\n");
        let half = (3.0, 2.0);
        let whole = HeightGrid::build(0, 0, 6, 4, half, &lookup);
        let mut set = HeightSet::new(half);
        set.insert("LEFT", HeightGrid::build(0, 0, 3, 4, half, &lookup));
        set.insert("RIGHT", HeightGrid::build(3, 0, 6, 4, half, &lookup));
        assert!(set.has("LEFT") && set.has("RIGHT") && !set.has("MIDDLE"));
        assert_eq!(set.len(), 2);
        for y in 0..4 {
            for x in 0..6 {
                let pos = TilePos::new(x, y);
                assert_eq!(set.tile_height(pos), whole.tile_height(pos), "({x}, {y})");
            }
        }
        for step in 0..60 {
            let (u, v) = (0.05 + step as f32 * 0.1, 0.07 + step as f32 * 0.06);
            let (x, z) = (u - half.0, -(v - half.1));
            assert!((set.height_at_world(x, z) - whole.height_at_tile_space(u, v)).abs() < 1e-6, "({u}, {v})");
        }
    }

    #[test]
    fn replacing_a_block_replaces_it_and_unknown_ground_is_flat() {
        let mut set = HeightSet::new((1.0, 1.0));
        assert!(set.is_empty());
        assert_eq!(set.tile_height(TilePos::new(0, 0)), 0.0);
        set.insert("A", HeightGrid::build(0, 0, 2, 2, (1.0, 1.0), &open));
        let before = set.tile_height(TilePos::new(1, 1));
        set.insert("A", HeightGrid::flat((1.0, 1.0)));
        assert_eq!(set.len(), 1, "same scene, same slot");
        assert_eq!(set.tile_height(TilePos::new(1, 1)), 0.0);
        assert!(before >= 0.0);
        assert_eq!(set.height_at_world(f32::NAN, 0.0), 0.0);
        assert_eq!(set.height_at_world(500.0, 500.0), 0.0);
    }

    #[test]
    fn a_block_covers_exactly_the_tiles_it_has_all_four_corners_for() {
        let grid = HeightGrid::build(2, 3, 5, 6, (0.0, 0.0), &open);
        assert!(grid.covers_tile(TilePos::new(2, 3)) && grid.covers_tile(TilePos::new(4, 5)));
        assert!(!grid.covers_tile(TilePos::new(5, 3)) && !grid.covers_tile(TilePos::new(2, 6)) && !grid.covers_tile(TilePos::new(1, 3)));
        assert!(!HeightGrid::flat((0.0, 0.0)).covers_tile(TilePos::new(0, 0)));
    }
}
