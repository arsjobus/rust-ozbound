//! Houses and plots: the reusable "walk-in house" asset and the plots that hold houses,
//! gardens, and vineyards.
//!
//! Pure Rust (no Bevy). A level file lists houses in a `[houses]` section and plots in a
//! `[plots]` section; this module turns them into tiles and checks that they make sense.
//!
//! # The house asset
//!
//! A house is a rectangle of at least 3 x 3 tiles (at most [`MAX_HOUSE`] on a side):
//!
//! * the outer ring is wall (`W`, blocks movement),
//! * one tile in the middle of the chosen side is the doorway (`+`, walkable),
//! * everything inside is floor (`,`, walkable).
//!
//! Stamping a house onto a map never decides collision by itself: it writes those tiles, and
//! [`crate::world::Tile::is_walkable`] stays the one place that says what blocks. The 3D layer
//! draws walls, a roof, and a floor from the same rectangle and hides the roof while the
//! player stands inside ([`HouseSpawn::is_inside`]).
//!
//! A house may only be stamped over open ground (grass or path), never over trees, water,
//! fences, or another house, and its doorway must open onto walkable ground the player can
//! reach, so a house can never be built shut.
//!
//! # Plots
//!
//! A plot is a named rectangle that holds at least one whole house and at least one growing
//! tile (a garden bed `g` or a vineyard row `v`). Plots do not overlap, and a house is either
//! wholly inside a plot or wholly outside every plot, never straddling an edge.

use std::fmt;

use crate::world::{MapError, Tile, TilePos, WorldMap};

/// Smallest house side: one wall, one floor tile, one wall.
pub const MIN_HOUSE: i32 = 3;
/// Largest house side. A typo such as `50` should be an error, not a village-sized house.
pub const MAX_HOUSE: i32 = 12;
/// Smallest plot side.
pub const MIN_PLOT: i32 = 3;

/// Which side of a house the doorway is on. `North` is map `+y`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Side {
    North,
    South,
    East,
    West,
}

impl Side {
    pub const ALL: [Side; 4] = [Side::North, Side::South, Side::East, Side::West];

    pub fn from_name(name: &str) -> Option<Self> {
        Some(match name {
            "north" => Side::North,
            "south" => Side::South,
            "east" => Side::East,
            "west" => Side::West,
            _ => return None,
        })
    }

    pub fn name(self) -> &'static str {
        match self {
            Side::North => "north",
            Side::South => "south",
            Side::East => "east",
            Side::West => "west",
        }
    }

    /// Unit map offset pointing out of the house through this side.
    pub fn offset(self) -> (i32, i32) {
        match self {
            Side::North => (0, 1),
            Side::South => (0, -1),
            Side::East => (1, 0),
            Side::West => (-1, 0),
        }
    }
}

/// A house stamped onto the map. `origin` is its south-west (lowest `x`, lowest `y`) corner.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HouseSpawn {
    pub id: String,
    pub origin: TilePos,
    pub width: i32,
    pub height: i32,
    pub door: Side,
    pub name: String,
}

impl HouseSpawn {
    /// True when `tile` is anywhere in the house's rectangle, walls included.
    pub fn contains(&self, tile: TilePos) -> bool {
        tile.x >= self.origin.x
            && tile.x < self.origin.x + self.width
            && tile.y >= self.origin.y
            && tile.y < self.origin.y + self.height
    }

    /// The doorway tile: the middle of the chosen side (the lower-left of two middle tiles
    /// when the side has an even length).
    pub fn door_tile(&self) -> TilePos {
        let mid_x = self.origin.x + (self.width - 1) / 2;
        let mid_y = self.origin.y + (self.height - 1) / 2;
        match self.door {
            Side::South => TilePos::new(mid_x, self.origin.y),
            Side::North => TilePos::new(mid_x, self.origin.y + self.height - 1),
            Side::West => TilePos::new(self.origin.x, mid_y),
            Side::East => TilePos::new(self.origin.x + self.width - 1, mid_y),
        }
    }

    /// The tile just outside the doorway, where a visitor stands before stepping in.
    pub fn outside_tile(&self) -> TilePos {
        let door = self.door_tile();
        let (dx, dy) = self.door.offset();
        TilePos::new(door.x + dx, door.y + dy)
    }

    /// True for the wall ring and the doorway's neighbours alike: the outer ring of tiles.
    fn on_ring(&self, tile: TilePos) -> bool {
        tile.x == self.origin.x
            || tile.x == self.origin.x + self.width - 1
            || tile.y == self.origin.y
            || tile.y == self.origin.y + self.height - 1
    }

    /// The tile each footprint position becomes: wall on the ring, a doorway in one side,
    /// floor inside. Positions outside the rectangle give `None`.
    pub fn tile_at(&self, tile: TilePos) -> Option<Tile> {
        if !self.contains(tile) {
            return None;
        }
        Some(if tile == self.door_tile() {
            Tile::Door
        } else if self.on_ring(tile) {
            Tile::Wall
        } else {
            Tile::Floor
        })
    }

    /// Every footprint position, bottom row first.
    pub fn footprint(&self) -> Vec<TilePos> {
        let mut out = Vec::with_capacity((self.width * self.height).max(0) as usize);
        for y in self.origin.y..self.origin.y + self.height {
            for x in self.origin.x..self.origin.x + self.width {
                out.push(TilePos::new(x, y));
            }
        }
        out
    }

    /// The floor tiles inside the walls.
    pub fn interior(&self) -> Vec<TilePos> {
        self.footprint().into_iter().filter(|t| self.tile_at(*t) == Some(Tile::Floor)).collect()
    }

    /// True when the player is in the house: on the doorway or on the floor. The roof is
    /// hidden while this holds, so the camera can see the room.
    pub fn is_inside(&self, tile: TilePos) -> bool {
        matches!(self.tile_at(tile), Some(Tile::Door | Tile::Floor))
    }
}

/// A named rectangle of land holding houses, gardens, and vineyards.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlotSpawn {
    pub id: String,
    pub origin: TilePos,
    pub width: i32,
    pub height: i32,
    pub name: String,
}

impl PlotSpawn {
    pub fn contains(&self, tile: TilePos) -> bool {
        tile.x >= self.origin.x
            && tile.x < self.origin.x + self.width
            && tile.y >= self.origin.y
            && tile.y < self.origin.y + self.height
    }

    /// True when every tile of `house` is inside this plot.
    pub fn holds(&self, house: &HouseSpawn) -> bool {
        house.footprint().into_iter().all(|t| self.contains(t))
    }

    fn overlaps(&self, other: &PlotSpawn) -> bool {
        self.origin.x < other.origin.x + other.width
            && other.origin.x < self.origin.x + self.width
            && self.origin.y < other.origin.y + other.height
            && other.origin.y < self.origin.y + self.height
    }
}

/// Why a house or plot was refused. `id` is the entry's id in the level file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HomesteadError {
    HouseSize { id: String, width: i32, height: i32 },
    HouseOffMap { id: String },
    /// A footprint tile is not open ground (grass or path).
    HouseBlocked { id: String, tile: TilePos, found: Tile },
    HouseOverlap { id: String, other: String },
    /// The tile outside the doorway is off the map or cannot be walked on.
    DoorBlocked { id: String, tile: TilePos },
    /// The player cannot walk from the start tile to the doorway or into the house.
    DoorUnreachable { id: String },
    PlotSize { id: String, width: i32, height: i32 },
    PlotOffMap { id: String },
    PlotOverlap { id: String, other: String },
    /// A house crosses the edge of a plot.
    PlotStraddled { id: String, house: String },
    PlotWithoutHouse { id: String },
    PlotWithoutCrops { id: String },
}

impl fmt::Display for HomesteadError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        use HomesteadError::*;
        match self {
            HouseSize { id, width, height } => write!(
                f,
                "house `{id}` is {width} x {height}; each side must be between {MIN_HOUSE} and {MAX_HOUSE} tiles"
            ),
            HouseOffMap { id } => write!(f, "house `{id}` does not fit on the map"),
            HouseBlocked { id, tile, found } => write!(
                f,
                "house `{id}` would stand on {found:?} at ({}, {}); houses need open grass or path",
                tile.x, tile.y
            ),
            HouseOverlap { id, other } => write!(f, "house `{id}` overlaps house `{other}`"),
            DoorBlocked { id, tile } => write!(
                f,
                "the doorway of house `{id}` opens onto ({}, {}), which cannot be walked on",
                tile.x, tile.y
            ),
            DoorUnreachable { id } => write!(f, "the player cannot walk to the doorway of house `{id}`"),
            PlotSize { id, width, height } => {
                write!(f, "plot `{id}` is {width} x {height}; each side must be at least {MIN_PLOT} tiles")
            }
            PlotOffMap { id } => write!(f, "plot `{id}` does not fit on the map"),
            PlotOverlap { id, other } => write!(f, "plot `{id}` overlaps plot `{other}`"),
            PlotStraddled { id, house } => write!(f, "house `{house}` crosses the edge of plot `{id}`"),
            PlotWithoutHouse { id } => write!(f, "plot `{id}` holds no house"),
            PlotWithoutCrops { id } => write!(f, "plot `{id}` has no garden bed (g) or vineyard row (v)"),
        }
    }
}

impl std::error::Error for HomesteadError {}

fn rectangle_on_map(map: &WorldMap, origin: TilePos, width: i32, height: i32) -> bool {
    origin.x >= 0
        && origin.y >= 0
        && width > 0
        && height > 0
        && origin.x.checked_add(width).is_some_and(|right| right <= map.width() as i32)
        && origin.y.checked_add(height).is_some_and(|top| top <= map.height() as i32)
}

/// Checks every house against the map as drawn in the file, then stamps them all onto it.
/// Nothing is stamped unless every house is acceptable. On failure returns the index of
/// the offending house in `houses` and why.
pub fn stamp_houses(map: &mut WorldMap, houses: &[HouseSpawn]) -> Result<(), (usize, HomesteadError)> {
    for (i, house) in houses.iter().enumerate() {
        let id = house.id.clone();
        let size_ok = |side: i32| (MIN_HOUSE..=MAX_HOUSE).contains(&side);
        if !size_ok(house.width) || !size_ok(house.height) {
            return Err((i, HomesteadError::HouseSize { id, width: house.width, height: house.height }));
        }
        if !rectangle_on_map(map, house.origin, house.width, house.height) {
            return Err((i, HomesteadError::HouseOffMap { id }));
        }
        if let Some(other) = houses[..i].iter().find(|o| house.footprint().iter().any(|t| o.contains(*t))) {
            return Err((i, HomesteadError::HouseOverlap { id, other: other.id.clone() }));
        }
        for tile in house.footprint() {
            let found = map.tile_at(tile).expect("rectangle was checked to lie on the map");
            if !matches!(found, Tile::Grass | Tile::Path) {
                return Err((i, HomesteadError::HouseBlocked { id, tile, found }));
            }
        }
    }
    for house in houses {
        for tile in house.footprint() {
            let stamped = house.tile_at(tile).expect("footprint tiles are inside the house");
            map.set_tile(tile, stamped).map_err(|_: MapError| unreachable!("rectangle was checked"))?;
        }
    }
    Ok(())
}

/// After stamping: each doorway opens onto walkable ground, and the player can walk from
/// `start` through the doorway to every floor tile inside.
pub fn check_doors(map: &WorldMap, houses: &[HouseSpawn], start: TilePos) -> Result<(), (usize, HomesteadError)> {
    for (i, house) in houses.iter().enumerate() {
        let id = house.id.clone();
        let outside = house.outside_tile();
        if !map.can_walk_to(outside) {
            return Err((i, HomesteadError::DoorBlocked { id, tile: outside }));
        }
        if !map.is_reachable(start, house.door_tile()) || house.interior().into_iter().any(|t| !map.is_reachable(start, t)) {
            return Err((i, HomesteadError::DoorUnreachable { id }));
        }
    }
    Ok(())
}

/// Checks plots against the (already stamped) map and the houses.
pub fn check_plots(map: &WorldMap, houses: &[HouseSpawn], plots: &[PlotSpawn]) -> Result<(), (usize, HomesteadError)> {
    for (i, plot) in plots.iter().enumerate() {
        let id = plot.id.clone();
        if plot.width < MIN_PLOT || plot.height < MIN_PLOT {
            return Err((i, HomesteadError::PlotSize { id, width: plot.width, height: plot.height }));
        }
        if !rectangle_on_map(map, plot.origin, plot.width, plot.height) {
            return Err((i, HomesteadError::PlotOffMap { id }));
        }
        if let Some(other) = plots[..i].iter().find(|o| plot.overlaps(o)) {
            return Err((i, HomesteadError::PlotOverlap { id, other: other.id.clone() }));
        }
        let mut holds_a_house = false;
        for house in houses {
            let inside = house.footprint().into_iter().filter(|t| plot.contains(*t)).count();
            if inside == 0 {
                continue;
            }
            if inside != (house.width * house.height) as usize {
                return Err((i, HomesteadError::PlotStraddled { id, house: house.id.clone() }));
            }
            holds_a_house = true;
        }
        if !holds_a_house {
            return Err((i, HomesteadError::PlotWithoutHouse { id }));
        }
        let mut has_crops = false;
        for y in plot.origin.y..plot.origin.y + plot.height {
            for x in plot.origin.x..plot.origin.x + plot.width {
                if matches!(map.tile_at(TilePos::new(x, y)), Ok(Tile::Garden | Tile::Vine)) {
                    has_crops = true;
                }
            }
        }
        if !has_crops {
            return Err((i, HomesteadError::PlotWithoutCrops { id }));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn house(id: &str, x: i32, y: i32, w: i32, h: i32, door: Side) -> HouseSpawn {
        HouseSpawn { id: id.into(), origin: TilePos::new(x, y), width: w, height: h, door, name: id.into() }
    }

    fn plot(id: &str, x: i32, y: i32, w: i32, h: i32) -> PlotSpawn {
        PlotSpawn { id: id.into(), origin: TilePos::new(x, y), width: w, height: h, name: id.into() }
    }

    fn open(w: usize, h: usize) -> WorldMap {
        WorldMap::from_ascii(&format!("{}\n", ".".repeat(w)).repeat(h)).unwrap()
    }

    #[test]
    fn side_names_round_trip_and_offsets_point_outwards() {
        for side in Side::ALL {
            assert_eq!(Side::from_name(side.name()), Some(side));
        }
        assert_eq!(Side::from_name("South"), None, "names are lower-case");
        assert_eq!(Side::from_name("up"), None);
        assert_eq!(Side::North.offset(), (0, 1));
        assert_eq!(Side::South.offset(), (0, -1));
        assert_eq!(Side::East.offset(), (1, 0));
        assert_eq!(Side::West.offset(), (-1, 0));
    }

    #[test]
    fn a_three_by_three_house_is_a_ring_of_wall_around_one_floor_tile() {
        let h = house("a", 2, 2, 3, 3, Side::South);
        assert_eq!(h.interior(), vec![TilePos::new(3, 3)]);
        assert_eq!(h.door_tile(), TilePos::new(3, 2));
        assert_eq!(h.tile_at(TilePos::new(3, 2)), Some(Tile::Door));
        assert_eq!(h.tile_at(TilePos::new(3, 3)), Some(Tile::Floor));
        for corner in [(2, 2), (4, 2), (2, 4), (4, 4), (3, 4), (2, 3), (4, 3)] {
            assert_eq!(h.tile_at(TilePos::new(corner.0, corner.1)), Some(Tile::Wall), "{corner:?}");
        }
        assert_eq!(h.tile_at(TilePos::new(5, 3)), None);
        assert_eq!(h.footprint().len(), 9);
    }

    #[test]
    fn the_doorway_sits_in_the_middle_of_each_side() {
        let (x, y, w, hgt) = (10, 20, 5, 4);
        let south = house("s", x, y, w, hgt, Side::South);
        let north = house("n", x, y, w, hgt, Side::North);
        let west = house("w", x, y, w, hgt, Side::West);
        let east = house("e", x, y, w, hgt, Side::East);
        assert_eq!(south.door_tile(), TilePos::new(12, 20));
        assert_eq!(north.door_tile(), TilePos::new(12, 23));
        // An even side has two middle tiles; the lower one is used, always the same one.
        assert_eq!(west.door_tile(), TilePos::new(10, 21));
        assert_eq!(east.door_tile(), TilePos::new(14, 21));
        assert_eq!(south.outside_tile(), TilePos::new(12, 19));
        assert_eq!(north.outside_tile(), TilePos::new(12, 24));
        assert_eq!(west.outside_tile(), TilePos::new(9, 21));
        assert_eq!(east.outside_tile(), TilePos::new(15, 21));
    }

    #[test]
    fn exactly_one_doorway_and_the_rest_of_the_ring_is_wall_for_every_size_and_side() {
        for w in MIN_HOUSE..=MAX_HOUSE {
            for h in MIN_HOUSE..=MAX_HOUSE {
                for side in Side::ALL {
                    let hs = house("x", 0, 0, w, h, side);
                    let tiles: Vec<Tile> = hs.footprint().iter().map(|t| hs.tile_at(*t).unwrap()).collect();
                    assert_eq!(tiles.iter().filter(|t| **t == Tile::Door).count(), 1, "{w}x{h} {side:?}");
                    assert_eq!(tiles.iter().filter(|t| **t == Tile::Floor).count() as i32, (w - 2) * (h - 2));
                    assert_eq!(tiles.iter().filter(|t| **t == Tile::Wall).count() as i32, 2 * (w + h) - 4 - 1);
                    assert!(hs.contains(hs.door_tile()));
                    assert!(!hs.contains(hs.outside_tile()), "outside is outside the house");
                }
            }
        }
    }

    #[test]
    fn is_inside_is_the_floor_and_the_doorway_only() {
        let h = house("a", 0, 0, 4, 4, Side::North);
        assert!(h.is_inside(h.door_tile()));
        for t in h.interior() {
            assert!(h.is_inside(t));
        }
        assert!(!h.is_inside(TilePos::new(0, 0)), "a wall corner is not inside");
        assert!(!h.is_inside(h.outside_tile()));
        assert!(!h.is_inside(TilePos::new(-1, -1)));
    }

    #[test]
    fn stamping_writes_walls_door_and_floor_and_collision_follows() {
        let mut map = open(8, 8);
        let h = house("a", 1, 1, 5, 4, Side::South);
        stamp_houses(&mut map, std::slice::from_ref(&h)).unwrap();
        assert_eq!(map.tile_at(h.door_tile()), Ok(Tile::Door));
        assert_eq!(map.tile_at(TilePos::new(1, 1)), Ok(Tile::Wall));
        assert_eq!(map.tile_at(TilePos::new(3, 2)), Ok(Tile::Floor));
        assert!(map.can_walk_to(h.door_tile()));
        assert!(map.can_walk_to(TilePos::new(3, 2)));
        assert!(!map.can_walk_to(TilePos::new(1, 2)), "walls block");
        // From outside the doorway the player can reach every floor tile, nothing else inside.
        for t in h.interior() {
            assert!(map.is_reachable(h.outside_tile(), t), "{t:?}");
        }
        assert!(!map.is_reachable(h.outside_tile(), TilePos::new(1, 2)));
    }

    #[test]
    fn a_house_is_a_dead_end_room_so_walls_stop_you_walking_through_it() {
        // Houses fill a row from edge to edge: the only way across is through the house, and
        // walls block that, so the two sides are cut off from each other.
        let mut map = open(5, 3);
        let wall_off = house("a", 0, 0, 5, 3, Side::North);
        stamp_houses(&mut map, std::slice::from_ref(&wall_off)).unwrap();
        assert!(!map.is_reachable(TilePos::new(0, 0), TilePos::new(4, 0)), "walls block");
    }

    #[test]
    fn bad_sizes_are_refused_with_the_house_id() {
        let mut map = open(20, 20);
        for (w, h) in [(2, 5), (5, 2), (0, 0), (-3, 4), (MAX_HOUSE + 1, 4), (4, MAX_HOUSE + 1)] {
            let err = stamp_houses(&mut map, &[house("tiny", 1, 1, w, h, Side::South)]).unwrap_err();
            assert_eq!(err.0, 0);
            assert!(matches!(err.1, HomesteadError::HouseSize { .. }), "{w}x{h}");
            assert!(err.1.to_string().contains("`tiny`"));
        }
    }

    #[test]
    fn houses_off_the_map_are_refused_including_extreme_origins() {
        let mut map = open(6, 6);
        for (x, y) in [(4, 0), (0, 4), (-1, 0), (0, -1), (i32::MAX - 1, 0), (0, i32::MAX - 1)] {
            let err = stamp_houses(&mut map, &[house("a", x, y, 3, 3, Side::South)]).unwrap_err();
            assert!(matches!(err.1, HomesteadError::HouseOffMap { .. }), "({x}, {y})");
        }
        assert!(stamp_houses(&mut map, &[house("a", 3, 3, 3, 3, Side::South)]).is_ok(), "flush with the edge is fine");
    }

    #[test]
    fn houses_only_stand_on_open_ground() {
        for (glyph, found) in [('T', Tile::Tree), ('~', Tile::Water), ('#', Tile::Rock), ('F', Tile::Fence), ('S', Tile::Sign)] {
            let mut rows = vec![".".repeat(6); 6];
            rows[2].replace_range(2..3, &glyph.to_string());
            let mut map = WorldMap::from_ascii(&rows.join("\n")).unwrap();
            let before = map.clone();
            let err = stamp_houses(&mut map, &[house("a", 1, 1, 4, 4, Side::South)]).unwrap_err();
            assert!(matches!(err.1, HomesteadError::HouseBlocked { found: f, .. } if f == found), "{glyph}");
            assert_eq!(map, before, "a refused house stamps nothing");
        }
        // Path is open ground too.
        let mut map = WorldMap::from_ascii("=====\n=====\n=====\n=====").unwrap();
        assert!(stamp_houses(&mut map, &[house("a", 0, 0, 4, 4, Side::East)]).is_ok());
    }

    #[test]
    fn houses_may_not_overlap_and_a_failure_stamps_nothing() {
        let mut map = open(12, 6);
        let before = map.clone();
        let first = house("first", 0, 0, 4, 4, Side::South);
        let second = house("second", 3, 1, 4, 4, Side::South);
        let err = stamp_houses(&mut map, &[first.clone(), second]).unwrap_err();
        assert_eq!(err.0, 1);
        assert_eq!(err.1, HomesteadError::HouseOverlap { id: "second".into(), other: "first".into() });
        assert_eq!(map, before);
        // Side by side with no gap is allowed.
        assert!(stamp_houses(&mut map, &[first, house("next", 4, 0, 4, 4, Side::South)]).is_ok());
    }

    #[test]
    fn a_doorway_must_open_onto_walkable_ground() {
        // Text rows run top to bottom, so the rock on the sixth row is at y = 2.
        let mut map = WorldMap::from_ascii("........\n........\n........\n........\n........\n...#....\n........\n........").unwrap();
        let h = house("a", 2, 3, 3, 3, Side::South);
        stamp_houses(&mut map, std::slice::from_ref(&h)).unwrap();
        assert_eq!(h.outside_tile(), TilePos::new(3, 2));
        let err = check_doors(&map, &[h], TilePos::new(0, 0)).unwrap_err();
        assert!(matches!(err.1, HomesteadError::DoorBlocked { tile, .. } if tile == TilePos::new(3, 2)));
    }

    #[test]
    fn a_doorway_on_the_map_edge_opens_onto_nothing_and_is_refused() {
        let mut map = open(6, 6);
        let h = house("a", 1, 0, 3, 3, Side::South);
        stamp_houses(&mut map, std::slice::from_ref(&h)).unwrap();
        let err = check_doors(&map, &[h], TilePos::new(5, 5)).unwrap_err();
        assert!(matches!(err.1, HomesteadError::DoorBlocked { .. }));
    }

    #[test]
    fn a_house_the_player_cannot_get_to_is_refused() {
        // A fence line splits the map; the house is on the far side from the player.
        let mut map = WorldMap::from_ascii(".....F.....\n.....F.....\n.....F.....\n.....F.....\n.....F.....\n.....F.....\n.....F.....").unwrap();
        let h = house("far", 6, 2, 3, 3, Side::North);
        stamp_houses(&mut map, std::slice::from_ref(&h)).unwrap();
        let err = check_doors(&map, std::slice::from_ref(&h), TilePos::new(0, 0)).unwrap_err();
        assert_eq!(err.1, HomesteadError::DoorUnreachable { id: "far".into() });
        assert!(check_doors(&map, &[h], TilePos::new(10, 0)).is_ok());
    }

    #[test]
    fn plots_need_a_house_and_something_growing() {
        let mut map = open(16, 10);
        let h = house("h", 2, 2, 4, 4, Side::South);
        stamp_houses(&mut map, std::slice::from_ref(&h)).unwrap();
        let p = plot("p", 1, 1, 8, 8);
        // No crops yet.
        let err = check_plots(&map, std::slice::from_ref(&h), std::slice::from_ref(&p)).unwrap_err();
        assert_eq!(err.1, HomesteadError::PlotWithoutCrops { id: "p".into() });
        map.set_tile(TilePos::new(7, 4), Tile::Garden).unwrap();
        assert!(check_plots(&map, std::slice::from_ref(&h), std::slice::from_ref(&p)).is_ok());
        // A vineyard row counts as growing too.
        let mut vines = map.clone();
        vines.set_tile(TilePos::new(7, 4), Tile::Grass).unwrap();
        vines.set_tile(TilePos::new(7, 5), Tile::Vine).unwrap();
        assert!(check_plots(&vines, std::slice::from_ref(&h), std::slice::from_ref(&p)).is_ok());
        // An empty plot with crops but no house.
        let empty = plot("empty", 10, 1, 5, 5);
        map.set_tile(TilePos::new(11, 2), Tile::Vine).unwrap();
        let err = check_plots(&map, std::slice::from_ref(&h), &[empty]).unwrap_err();
        assert_eq!(err.1, HomesteadError::PlotWithoutHouse { id: "empty".into() });
    }

    #[test]
    fn plots_may_not_overlap_straddle_houses_or_leave_the_map() {
        let mut map = open(20, 10);
        let h = house("h", 2, 2, 4, 4, Side::South);
        stamp_houses(&mut map, std::slice::from_ref(&h)).unwrap();
        map.set_tile(TilePos::new(7, 3), Tile::Garden).unwrap();
        let ok = plot("ok", 1, 1, 8, 8);
        let straddling = plot("cut", 4, 1, 8, 8); // slices through the house
        let err = check_plots(&map, std::slice::from_ref(&h), &[straddling]).unwrap_err();
        assert_eq!(err.1, HomesteadError::PlotStraddled { id: "cut".into(), house: "h".into() });
        let overlapping = plot("over", 5, 1, 8, 8);
        let err = check_plots(&map, std::slice::from_ref(&h), &[ok.clone(), overlapping]).unwrap_err();
        assert_eq!(err.0, 1);
        assert_eq!(err.1, HomesteadError::PlotOverlap { id: "over".into(), other: "ok".into() });
        let off = plot("off", 15, 1, 8, 8);
        assert!(matches!(check_plots(&map, &[], &[off]).unwrap_err().1, HomesteadError::PlotOffMap { .. }));
        let thin = plot("thin", 1, 1, 2, 8);
        assert!(matches!(check_plots(&map, &[], &[thin]).unwrap_err().1, HomesteadError::PlotSize { .. }));
        let huge = plot("huge", i32::MAX - 1, 0, 5, 5);
        assert!(matches!(check_plots(&map, &[], &[huge]).unwrap_err().1, HomesteadError::PlotOffMap { .. }));
    }

    #[test]
    fn plot_holds_matches_whole_houses_only() {
        let h = house("h", 2, 2, 4, 4, Side::South);
        assert!(plot("a", 0, 0, 10, 10).holds(&h));
        assert!(plot("b", 2, 2, 4, 4).holds(&h), "an exact fit counts");
        assert!(!plot("c", 3, 2, 10, 10).holds(&h));
        assert!(!plot("d", 8, 8, 4, 4).holds(&h));
    }

    #[test]
    fn error_messages_name_the_thing_and_the_fix() {
        let errors = [
            HomesteadError::HouseSize { id: "a".into(), width: 2, height: 9 },
            HomesteadError::HouseBlocked { id: "a".into(), tile: TilePos::new(1, 2), found: Tile::Water },
            HomesteadError::DoorBlocked { id: "a".into(), tile: TilePos::new(1, 2) },
            HomesteadError::PlotWithoutCrops { id: "p".into() },
        ];
        let text: Vec<String> = errors.iter().map(ToString::to_string).collect();
        assert!(text[0].contains("between 3 and 12"));
        assert!(text[1].contains("Water") && text[1].contains("(1, 2)"));
        assert!(text[2].contains("cannot be walked on"));
        assert!(text[3].contains("garden bed (g) or vineyard row (v)"));
    }
}
