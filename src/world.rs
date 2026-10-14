/// Integer tile coordinate, independent of Bevy's rendering types.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TilePos { pub x: i32, pub y: i32 }

impl TilePos {
    pub const fn new(x: i32, y: i32) -> Self { Self { x, y } }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tile { Grass, Path, Water, Tree, Rock }

impl Tile {
    pub fn is_walkable(self) -> bool { matches!(self, Tile::Grass | Tile::Path) }
}

/// Small rectangular map. Data is supplied row-by-row from bottom to top.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorldMap {
    width: usize,
    height: usize,
    tiles: Vec<Tile>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MapError { Empty, RaggedRows, OutOfBounds }

impl WorldMap {
    pub fn from_rows(rows: Vec<Vec<Tile>>) -> Result<Self, MapError> {
        if rows.is_empty() || rows[0].is_empty() { return Err(MapError::Empty); }
        let width = rows[0].len();
        if rows.iter().any(|r| r.len() != width) { return Err(MapError::RaggedRows); }
        let height = rows.len();
        let tiles = rows.into_iter().flatten().collect();
        Ok(Self { width, height, tiles })
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
}
