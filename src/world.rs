/// Side length of one tile in world units. In the 3D prototype one tile is one world unit,
/// so characters, trees, and camera distances can all be reasoned about in tiles.
pub const TILE_SIZE: f32 = 1.0;

/// Integer tile coordinate, independent of Bevy's rendering types.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TilePos { pub x: i32, pub y: i32 }

impl TilePos {
    pub const fn new(x: i32, y: i32) -> Self { Self { x, y } }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tile { Grass, Path, Water, Tree, Rock, Fence, Sign }

impl Tile {
    pub fn is_walkable(self) -> bool { matches!(self, Tile::Grass | Tile::Path) }

    /// Legend used by text maps: `.` grass, `=` path, `~` water, `T` tree, `#` rock,
    /// `F` fence, `S` signpost. Only grass and path can be walked on; everything else blocks
    /// movement, so collision is decided in one place: [`Tile::is_walkable`].
    pub fn from_glyph(glyph: char) -> Option<Self> {
        Some(match glyph {
            '.' => Tile::Grass,
            '=' => Tile::Path,
            '~' => Tile::Water,
            'T' => Tile::Tree,
            '#' => Tile::Rock,
            'F' => Tile::Fence,
            'S' => Tile::Sign,
            _ => return None,
        })
    }
}

/// Small rectangular map. Data is supplied row-by-row from bottom to top.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorldMap {
    width: usize,
    height: usize,
    tiles: Vec<Tile>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MapError {
    Empty,
    RaggedRows,
    OutOfBounds,
    /// A character in text map data is not part of the tile legend.
    /// `line` and `column` are 1-based positions in the source text.
    UnknownGlyph { glyph: char, line: usize, column: usize },
}

impl WorldMap {
    pub fn from_rows(rows: Vec<Vec<Tile>>) -> Result<Self, MapError> {
        if rows.is_empty() || rows[0].is_empty() { return Err(MapError::Empty); }
        let width = rows[0].len();
        if rows.iter().any(|r| r.len() != width) { return Err(MapError::RaggedRows); }
        let height = rows.len();
        let tiles = rows.into_iter().flatten().collect();
        Ok(Self { width, height, tiles })
    }
    /// Parses a text map. The first line is the *top* of the map, so lines are
    /// stored in reverse to keep `y` increasing upward like the rest of the world.
    /// Blank lines and surrounding whitespace are ignored.
    pub fn from_ascii(text: &str) -> Result<Self, MapError> {
        let mut rows = Vec::new();
        for (line_no, line) in text.lines().enumerate() {
            let line = line.trim();
            if line.is_empty() { continue; }
            let mut row = Vec::with_capacity(line.len());
            for (col, glyph) in line.chars().enumerate() {
                let tile = Tile::from_glyph(glyph).ok_or(MapError::UnknownGlyph {
                    glyph,
                    line: line_no + 1,
                    column: col + 1,
                })?;
                row.push(tile);
            }
            rows.push(row);
        }
        rows.reverse();
        Self::from_rows(rows)
    }

    pub fn width(&self) -> usize { self.width }
    pub fn height(&self) -> usize { self.height }
    pub fn tile_at(&self, pos: TilePos) -> Result<Tile, MapError> {
        if pos.x < 0 || pos.y < 0 || pos.x as usize >= self.width || pos.y as usize >= self.height {
            return Err(MapError::OutOfBounds);
        }
        Ok(self.tiles[pos.y as usize * self.width + pos.x as usize])
    }
    pub fn can_walk_to(&self, pos: TilePos) -> bool {
        self.tile_at(pos).is_ok_and(Tile::is_walkable)
    }

    /// World-space center of a tile. The map is centered on the world origin.
    pub fn tile_center(&self, pos: TilePos) -> (f32, f32) {
        (
            (pos.x as f32 + 0.5 - self.width as f32 / 2.0) * TILE_SIZE,
            (pos.y as f32 + 0.5 - self.height as f32 / 2.0) * TILE_SIZE,
        )
    }

    /// 3D world position (x, height, z) of a tile center. The map plane's `y` axis (up the
    /// map, "north") becomes `-z`, so north is Bevy's forward direction and the picture is
    /// not mirrored. Use together with [`WorldMap::world_to_tile`] via `(x, -z)`.
    pub fn tile_center_3d(&self, pos: TilePos, height: f32) -> [f32; 3] {
        let (x, y) = self.tile_center(pos);
        [x, height, -y]
    }

    /// Tile containing a world-space point. The result may lie outside the map;
    /// use [`WorldMap::tile_at`] to find out. Non-finite input yields `None`.
    pub fn world_to_tile(&self, point: (f32, f32)) -> Option<TilePos> {
        if !point.0.is_finite() || !point.1.is_finite() { return None; }
        let x = (point.0 / TILE_SIZE + self.width as f32 / 2.0).floor();
        let y = (point.1 / TILE_SIZE + self.height as f32 / 2.0).floor();
        // Clamp before casting so absurd coordinates stay out-of-bounds, not UB-adjacent.
        let clamp = |v: f32| v.clamp(-1.0e6, 1.0e6) as i32;
        Some(TilePos::new(clamp(x), clamp(y)))
    }

    /// True when `to` can be reached from `from` by 4-directional walking.
    /// Both endpoints must themselves be walkable.
    pub fn is_reachable(&self, from: TilePos, to: TilePos) -> bool {
        if !self.can_walk_to(from) || !self.can_walk_to(to) { return false; }
        let mut seen = vec![false; self.width * self.height];
        let idx = |p: TilePos| p.y as usize * self.width + p.x as usize;
        let mut queue = std::collections::VecDeque::from([from]);
        seen[idx(from)] = true;
        while let Some(cur) = queue.pop_front() {
            if cur == to { return true; }
            for (dx, dy) in [(1, 0), (-1, 0), (0, 1), (0, -1)] {
                let next = TilePos::new(cur.x + dx, cur.y + dy);
                if self.can_walk_to(next) && !seen[idx(next)] {
                    seen[idx(next)] = true;
                    queue.push_back(next);
                }
            }
        }
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn reads_tiles_and_walkability() {
        let map = WorldMap::from_rows(vec![vec![Tile::Grass, Tile::Water], vec![Tile::Path, Tile::Tree]]).unwrap();
        assert_eq!(map.tile_at(TilePos::new(0, 1)), Ok(Tile::Path));
        assert!(map.can_walk_to(TilePos::new(0, 1)));
        assert!(!map.can_walk_to(TilePos::new(1, 0)));
        assert!(!map.can_walk_to(TilePos::new(1, 1)));
    }
    #[test]
    fn rejects_empty_ragged_and_out_of_bounds_maps() {
        assert_eq!(WorldMap::from_rows(vec![]), Err(MapError::Empty));
        assert_eq!(WorldMap::from_rows(vec![vec![Tile::Grass], vec![Tile::Path, Tile::Water]]), Err(MapError::RaggedRows));
        let map = WorldMap::from_rows(vec![vec![Tile::Grass]]).unwrap();
        assert_eq!(map.tile_at(TilePos::new(-1, 0)), Err(MapError::OutOfBounds));
        assert_eq!(map.tile_at(TilePos::new(1, 0)), Err(MapError::OutOfBounds));
    }
    #[test]
    fn parses_ascii_with_first_line_as_top_row() {
        let map = WorldMap::from_ascii("T.\n=~\n").unwrap();
        assert_eq!((map.width(), map.height()), (2, 2));
        assert_eq!(map.tile_at(TilePos::new(0, 1)), Ok(Tile::Tree));
        assert_eq!(map.tile_at(TilePos::new(1, 0)), Ok(Tile::Water));
    }
    #[test]
    fn ascii_errors_name_the_glyph_and_position() {
        assert_eq!(
            WorldMap::from_ascii("..\n.?"),
            Err(MapError::UnknownGlyph { glyph: '?', line: 2, column: 2 })
        );
        assert_eq!(WorldMap::from_ascii("  \n"), Err(MapError::Empty));
        assert_eq!(WorldMap::from_ascii("..\n."), Err(MapError::RaggedRows));
    }
    #[test]
    fn every_blocking_tile_kind_refuses_walking() {
        // One of each tile in a row: only grass and path may be walked on.
        let map = WorldMap::from_ascii(".=~T#FS").unwrap();
        let walkable: Vec<bool> = (0..7).map(|x| map.can_walk_to(TilePos::new(x, 0))).collect();
        assert_eq!(walkable, vec![true, true, false, false, false, false, false]);
    }

    #[test]
    fn fence_and_sign_glyphs_parse_and_block() {
        let map = WorldMap::from_ascii("F.S").unwrap();
        assert_eq!(map.tile_at(TilePos::new(0, 0)), Ok(Tile::Fence));
        assert_eq!(map.tile_at(TilePos::new(2, 0)), Ok(Tile::Sign));
        assert!(!Tile::Fence.is_walkable() && !Tile::Sign.is_walkable());
        assert_eq!(Tile::from_glyph('f'), None, "glyphs are case-sensitive");
    }

    #[test]
    fn fences_wall_off_reachability() {
        let open = WorldMap::from_ascii("..F..\n..F..\n..F..").unwrap();
        assert!(!open.is_reachable(TilePos::new(0, 0), TilePos::new(4, 0)), "a full fence line blocks");
        let gate = WorldMap::from_ascii("..F..\n.....\n..F..").unwrap();
        assert!(gate.is_reachable(TilePos::new(0, 0), TilePos::new(4, 0)), "a gap lets you through");
        assert!(!gate.is_reachable(TilePos::new(0, 0), TilePos::new(2, 0)), "the fence tile itself is blocked");
    }

    #[test]
    fn tile_and_world_coordinates_round_trip() {
        let map = WorldMap::from_ascii("....\n....\n....").unwrap();
        for y in 0..3 { for x in 0..4 {
            let pos = TilePos::new(x, y);
            assert_eq!(map.world_to_tile(map.tile_center(pos)), Some(pos));
        } }
        // Exactly on a shared edge belongs to the higher tile.
        assert_eq!(map.world_to_tile((0.0, 0.0)), Some(TilePos::new(2, 1)));
        assert_eq!(map.world_to_tile((f32::NAN, 0.0)), None);
        let far = map.world_to_tile((1.0e30, -1.0e30)).unwrap();
        assert_eq!(map.tile_at(far), Err(MapError::OutOfBounds));
    }
    #[test]
    fn reachability_respects_blocked_tiles() {
        let map = WorldMap::from_ascii("..T.\n..T.\n..T.").unwrap();
        assert!(map.is_reachable(TilePos::new(0, 0), TilePos::new(1, 2)));
        assert!(!map.is_reachable(TilePos::new(0, 0), TilePos::new(3, 0)));
        assert!(!map.is_reachable(TilePos::new(0, 0), TilePos::new(2, 0))); // target blocked
        assert!(!map.is_reachable(TilePos::new(0, 0), TilePos::new(9, 9))); // out of bounds
    }
    #[test]
    fn three_d_centers_map_north_to_negative_z() {
        let map = WorldMap::from_ascii("....\n....\n....").unwrap();
        let south = map.tile_center_3d(TilePos::new(1, 0), 0.5);
        let north = map.tile_center_3d(TilePos::new(1, 2), 0.5);
        assert_eq!(south[1], 0.5);
        assert_eq!(south[0], north[0]);
        assert!(north[2] < south[2], "north must lie further along -z");
        assert!((south[2] - north[2] - 2.0 * TILE_SIZE).abs() < 1e-5);
    }
}
