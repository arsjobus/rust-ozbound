//! Level data: a plain-text file describing the map and everything standing on it.
//!
//! Pure Rust (no Bevy). Parsing reports the line that is wrong, and a parsed [`Level`] has
//! already been checked: the player and guide stand on walkable tiles that connect, every
//! resource sits on a suitable tile and can be reached, and the quest can be completed.
//! Signposts (`S` tiles listed in `[signs]`) are validated like resources: right tile,
//! no duplicates, reachable from the player start.
//! The file format is documented at the top of `assets/maps/meadow.map`.

use std::fmt;
use std::path::{Path, PathBuf};

use crate::gathering::{validate_node_placement, PlacementError, ResourceKind};
use crate::quest::MoonpetalQuest;
use crate::world::{MapError, Tile, TilePos, WorldMap};

/// A gatherable resource standing on a specific tile.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NodeSpawn {
    pub tile: TilePos,
    pub kind: ResourceKind,
    pub actions: u32,
    pub yield_per_action: u32,
}

/// A signpost standing on an `S` tile; clicking it reads `text` in the journal.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SignSpawn {
    pub tile: TilePos,
    pub text: String,
}

/// Everything the world needs to be built: the tile map plus what stands on it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Level {
    pub map: WorldMap,
    pub player_start: TilePos,
    pub guide: TilePos,
    pub moonpetals: Vec<TilePos>,
    /// Trees and copper veins. Moonpetals are listed separately.
    pub nodes: Vec<NodeSpawn>,
    /// Readable signposts. An `S` tile with no entry here is plain scenery.
    pub signs: Vec<SignSpawn>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Section { Map, Player, Guide, Moonpetals, Nodes, Signs }

impl Section {
    fn from_name(name: &str) -> Option<Self> {
        Some(match name {
            "map" => Self::Map,
            "player" => Self::Player,
            "guide" => Self::Guide,
            "moonpetals" => Self::Moonpetals,
            "nodes" => Self::Nodes,
            "signs" => Self::Signs,
            _ => return None,
        })
    }
    fn name(self) -> &'static str {
        match self {
            Self::Map => "map",
            Self::Player => "player",
            Self::Guide => "guide",
            Self::Moonpetals => "moonpetals",
            Self::Nodes => "nodes",
            Self::Signs => "signs",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LevelErrorKind {
    /// Text appeared before the first `[section]` header.
    ContentBeforeSection,
    /// A `[header]` that is not closed or not one of the known sections.
    UnknownSection(String),
    DuplicateSection(&'static str),
    /// A required section is absent from the file.
    MissingSection(&'static str),
    /// The `[map]` section has no rows.
    EmptyMap,
    /// A map row whose length differs from the first row.
    RaggedMapRow { expected: usize, found: usize },
    UnknownGlyph { glyph: char, column: usize },
    /// A line that does not have the shape its section expects.
    Malformed { expected: &'static str, found: String },
    UnknownResource(String),
    /// `[player]` or `[guide]` needs exactly one entry.
    WrongEntryCount { section: &'static str, found: usize },
    OffMap { what: &'static str, tile: TilePos },
    NotWalkable { what: &'static str, tile: TilePos, found: Tile },
    Placement { what: &'static str, tile: TilePos, error: PlacementError },
    Duplicate { what: &'static str, tile: TilePos },
    /// The player cannot walk from the start tile to this thing.
    Unreachable { what: &'static str, tile: TilePos },
    /// A node that would never give anything (zero actions or zero yield).
    NeverYields { tile: TilePos },
    NotEnoughMoonpetals { have: usize, need: usize },
}

/// A level-data problem, with the 1-based line of the file it belongs to when there is one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LevelError {
    pub line: Option<usize>,
    pub kind: LevelErrorKind,
}

impl LevelError {
    fn at(line: usize, kind: LevelErrorKind) -> Self { Self { line: Some(line), kind } }
    fn general(kind: LevelErrorKind) -> Self { Self { line: None, kind } }
}

impl fmt::Display for LevelError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let Some(line) = self.line { write!(f, "line {line}: ")?; }
        use LevelErrorKind::*;
        match &self.kind {
            ContentBeforeSection => write!(f, "content before the first [section] header"),
            UnknownSection(name) => write!(
                f,
                "unknown or unclosed section header {name:?} (expected [map], [player], [guide], [moonpetals], [nodes] or [signs])"
            ),
            DuplicateSection(name) => write!(f, "section [{name}] appears more than once"),
            MissingSection(name) => write!(f, "required section [{name}] is missing"),
            EmptyMap => write!(f, "[map] has no rows"),
            RaggedMapRow { expected, found } => {
                write!(f, "map row is {found} tiles wide but the first row is {expected}")
            }
            UnknownGlyph { glyph, column } => write!(
                f,
                "unknown map glyph {glyph:?} at column {column} (use . grass, = path, ~ water, T tree, # rock, F fence, S signpost)"
            ),
            Malformed { expected, found } => write!(f, "expected {expected}, found {found:?}"),
            UnknownResource(name) => write!(f, "unknown resource {name:?} (use tree or copper)"),
            WrongEntryCount { section, found } => {
                write!(f, "[{section}] needs exactly one entry, found {found}")
            }
            OffMap { what, tile } => write!(f, "{what} at ({}, {}) is outside the map", tile.x, tile.y),
            NotWalkable { what, tile, found } => {
                write!(f, "{what} at ({}, {}) must be on walkable ground, but the tile is {found:?}", tile.x, tile.y)
            }
            Placement { what, tile, error } => match error {
                PlacementError::OutOfBounds => {
                    write!(f, "{what} at ({}, {}) is outside the map", tile.x, tile.y)
                }
                PlacementError::WrongTile { expected, found } => write!(
                    f,
                    "{what} at ({}, {}) needs a {expected} tile, but the tile is {found:?}",
                    tile.x, tile.y
                ),
            },
            Duplicate { what, tile } => write!(f, "{what} at ({}, {}) is placed twice", tile.x, tile.y),
            Unreachable { what, tile } => {
                write!(f, "{what} at ({}, {}) cannot be reached from the player start", tile.x, tile.y)
            }
            NeverYields { tile } => {
                write!(f, "node at ({}, {}) needs at least 1 action and a yield of at least 1", tile.x, tile.y)
            }
            NotEnoughMoonpetals { have, need } => {
                write!(f, "the quest needs {need} moonpetals but the level has only {have}")
            }
        }
    }
}

impl std::error::Error for LevelError {}

/// Why a level file could not be loaded.
#[derive(Debug)]
pub enum LoadError {
    Io { path: PathBuf, error: std::io::Error },
    Parse { path: PathBuf, error: LevelError },
}

impl fmt::Display for LoadError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io { path, error } => write!(f, "could not read level file {}: {error}", path.display()),
            Self::Parse { path, error } => write!(f, "invalid level file {}: {error}", path.display()),
        }
    }
}

impl std::error::Error for LoadError {}

/// A parsed entry remembers its source line so later checks can point at it.
struct Entry<T> { line: usize, value: T }

fn parse_int<T: std::str::FromStr>(token: &str, expected: &'static str, line: usize) -> Result<T, LevelError> {
    token
        .parse()
        .map_err(|_| LevelError::at(line, LevelErrorKind::Malformed { expected, found: token.to_string() }))
}

/// `x y`, exactly two whole numbers.
fn parse_tile(text: &str, line: usize) -> Result<TilePos, LevelError> {
    let malformed = || LevelError::at(line, LevelErrorKind::Malformed { expected: "\"x y\" tile coordinates", found: text.to_string() });
    let tokens: Vec<&str> = text.split_whitespace().collect();
    if tokens.len() != 2 { return Err(malformed()); }
    let x = tokens[0].parse().map_err(|_| malformed())?;
    let y = tokens[1].parse().map_err(|_| malformed())?;
    Ok(TilePos::new(x, y))
}

/// `kind x y actions yield_per_action`.
fn parse_node(text: &str, line: usize) -> Result<NodeSpawn, LevelError> {
    let tokens: Vec<&str> = text.split_whitespace().collect();
    if tokens.len() != 5 {
        return Err(LevelError::at(
            line,
            LevelErrorKind::Malformed { expected: "\"kind x y actions yield\"", found: text.to_string() },
        ));
    }
    let kind = match tokens[0] {
        "tree" => ResourceKind::Tree,
        "copper" => ResourceKind::CopperVein,
        other => return Err(LevelError::at(line, LevelErrorKind::UnknownResource(other.to_string()))),
    };
    Ok(NodeSpawn {
        tile: TilePos::new(parse_int(tokens[1], "a whole-number x", line)?, parse_int(tokens[2], "a whole-number y", line)?),
        kind,
        actions: parse_int(tokens[3], "a non-negative number of actions", line)?,
        yield_per_action: parse_int(tokens[4], "a non-negative yield", line)?,
    })
}

/// `x y message`: two whole numbers, then the sign's text (which may contain spaces).
fn parse_sign(text: &str, line: usize) -> Result<SignSpawn, LevelError> {
    let malformed = || LevelError::at(line, LevelErrorKind::Malformed { expected: "\"x y message\" for a sign", found: text.to_string() });
    let mut parts = text.splitn(3, char::is_whitespace);
    let x = parts.next().and_then(|t| t.parse().ok()).ok_or_else(malformed)?;
    let y = parts.next().and_then(|t| t.parse().ok()).ok_or_else(malformed)?;
    let message = parts.next().map(str::trim).filter(|m| !m.is_empty()).ok_or_else(malformed)?;
    Ok(SignSpawn { tile: TilePos::new(x, y), text: message.to_string() })
}

fn check_walkable(map: &WorldMap, what: &'static str, tile: TilePos, line: usize) -> Result<(), LevelError> {
    match map.tile_at(tile) {
        Err(_) => Err(LevelError::at(line, LevelErrorKind::OffMap { what, tile })),
        Ok(found) if !found.is_walkable() => Err(LevelError::at(line, LevelErrorKind::NotWalkable { what, tile, found })),
        Ok(_) => Ok(()),
    }
}

/// True when some orthogonal neighbour of `tile` can be walked to from `start`.
fn has_reachable_neighbour(map: &WorldMap, start: TilePos, tile: TilePos) -> bool {
    [(1, 0), (-1, 0), (0, 1), (0, -1)]
        .iter()
        .any(|(dx, dy)| map.is_reachable(start, TilePos::new(tile.x + dx, tile.y + dy)))
}

impl Level {
    /// Parses and validates level text. See `assets/maps/meadow.map` for the format.
    pub fn parse(text: &str) -> Result<Self, LevelError> {
        let mut section: Option<Section> = None;
        let mut seen: Vec<Section> = Vec::new();
        // Map text keeps one line per source line (blank outside the map), so the map
        // parser's line numbers match the file's.
        let mut map_text = String::new();
        let mut map_rows: Vec<(usize, usize)> = Vec::new();
        let mut player: Vec<Entry<TilePos>> = Vec::new();
        let mut guide: Vec<Entry<TilePos>> = Vec::new();
        let mut moonpetals: Vec<Entry<TilePos>> = Vec::new();
        let mut nodes: Vec<Entry<NodeSpawn>> = Vec::new();
        let mut signs: Vec<Entry<SignSpawn>> = Vec::new();

        for (index, raw) in text.lines().enumerate() {
            let line_no = index + 1;
            let line = raw.trim();
            let in_map = section == Some(Section::Map);
            if line.is_empty() || line.starts_with(';') {
                map_text.push('\n');
                continue;
            }
            if line.starts_with('[') {
                let name = line.strip_prefix('[').and_then(|rest| rest.strip_suffix(']')).map(str::trim);
                let found = name
                    .and_then(|n| Section::from_name(&n.to_ascii_lowercase()))
                    .ok_or_else(|| LevelError::at(line_no, LevelErrorKind::UnknownSection(line.to_string())))?;
                if seen.contains(&found) {
                    return Err(LevelError::at(line_no, LevelErrorKind::DuplicateSection(found.name())));
                }
                seen.push(found);
                section = Some(found);
                map_text.push('\n');
                continue;
            }
            let Some(current) = section else {
                return Err(LevelError::at(line_no, LevelErrorKind::ContentBeforeSection));
            };
            if in_map {
                map_rows.push((line_no, line.chars().count()));
                map_text.push_str(line);
            }
            map_text.push('\n');
            match current {
                Section::Map => {}
                Section::Player => player.push(Entry { line: line_no, value: parse_tile(line, line_no)? }),
                Section::Guide => guide.push(Entry { line: line_no, value: parse_tile(line, line_no)? }),
                Section::Moonpetals => moonpetals.push(Entry { line: line_no, value: parse_tile(line, line_no)? }),
                Section::Nodes => nodes.push(Entry { line: line_no, value: parse_node(line, line_no)? }),
                Section::Signs => signs.push(Entry { line: line_no, value: parse_sign(line, line_no)? }),
            }
        }

        for (required, section) in [("map", Section::Map), ("player", Section::Player), ("guide", Section::Guide)] {
            if !seen.contains(&section) {
                return Err(LevelError::general(LevelErrorKind::MissingSection(required)));
            }
        }

        let Some(&(_, expected)) = map_rows.first() else {
            return Err(LevelError::general(LevelErrorKind::EmptyMap));
        };
        if let Some(&(line, found)) = map_rows.iter().find(|(_, width)| *width != expected) {
            return Err(LevelError::at(line, LevelErrorKind::RaggedMapRow { expected, found }));
        }
        let map = WorldMap::from_ascii(&map_text).map_err(|error| match error {
            MapError::UnknownGlyph { glyph, line, column } => {
                LevelError::at(line, LevelErrorKind::UnknownGlyph { glyph, column })
            }
            // Rows were checked above, so only an empty map can remain.
            _ => LevelError::general(LevelErrorKind::EmptyMap),
        })?;

        let single = |entries: &[Entry<TilePos>], section: &'static str| -> Result<(usize, TilePos), LevelError> {
            match entries {
                [only] => Ok((only.line, only.value)),
                _ => Err(LevelError::general(LevelErrorKind::WrongEntryCount { section, found: entries.len() })),
            }
        };
        let (player_line, player_start) = single(&player, "player")?;
        let (guide_line, guide_tile) = single(&guide, "guide")?;

        check_walkable(&map, "player start", player_start, player_line)?;
        check_walkable(&map, "guide", guide_tile, guide_line)?;
        if guide_tile == player_start {
            return Err(LevelError::at(guide_line, LevelErrorKind::Duplicate { what: "guide (on the player start)", tile: guide_tile }));
        }
        if !map.is_reachable(player_start, guide_tile) {
            return Err(LevelError::at(guide_line, LevelErrorKind::Unreachable { what: "guide", tile: guide_tile }));
        }

        for (i, petal) in moonpetals.iter().enumerate() {
            let tile = petal.value;
            validate_node_placement(&map, tile, ResourceKind::Moonpetal).map_err(|error| {
                LevelError::at(petal.line, LevelErrorKind::Placement { what: "moonpetal", tile, error })
            })?;
            if moonpetals[..i].iter().any(|other| other.value == tile) {
                return Err(LevelError::at(petal.line, LevelErrorKind::Duplicate { what: "moonpetal", tile }));
            }
            if !map.is_reachable(player_start, tile) {
                return Err(LevelError::at(petal.line, LevelErrorKind::Unreachable { what: "moonpetal", tile }));
            }
        }
        let need = MoonpetalQuest::default().required() as usize;
        if moonpetals.len() < need {
            return Err(LevelError::general(LevelErrorKind::NotEnoughMoonpetals { have: moonpetals.len(), need }));
        }

        for (i, entry) in nodes.iter().enumerate() {
            let node = entry.value;
            let what = node.kind.label();
            validate_node_placement(&map, node.tile, node.kind)
                .map_err(|error| LevelError::at(entry.line, LevelErrorKind::Placement { what, tile: node.tile, error }))?;
            if node.actions == 0 || node.yield_per_action == 0 {
                return Err(LevelError::at(entry.line, LevelErrorKind::NeverYields { tile: node.tile }));
            }
            if nodes[..i].iter().any(|other| other.value.tile == node.tile) {
                return Err(LevelError::at(entry.line, LevelErrorKind::Duplicate { what, tile: node.tile }));
            }
            if !has_reachable_neighbour(&map, player_start, node.tile) {
                return Err(LevelError::at(entry.line, LevelErrorKind::Unreachable { what, tile: node.tile }));
            }
        }

        for (i, entry) in signs.iter().enumerate() {
            let sign = &entry.value;
            let tile = sign.tile;
            let found = map
                .tile_at(tile)
                .map_err(|_| LevelError::at(entry.line, LevelErrorKind::OffMap { what: "signpost", tile }))?;
            if found != Tile::Sign {
                let error = PlacementError::WrongTile { expected: "signpost", found };
                return Err(LevelError::at(entry.line, LevelErrorKind::Placement { what: "signpost", tile, error }));
            }
            if signs[..i].iter().any(|other| other.value.tile == tile) {
                return Err(LevelError::at(entry.line, LevelErrorKind::Duplicate { what: "signpost", tile }));
            }
            if !has_reachable_neighbour(&map, player_start, tile) {
                return Err(LevelError::at(entry.line, LevelErrorKind::Unreachable { what: "signpost", tile }));
            }
        }

        Ok(Self {
            map,
            player_start,
            guide: guide_tile,
            moonpetals: moonpetals.into_iter().map(|e| e.value).collect(),
            nodes: nodes.into_iter().map(|e| e.value).collect(),
            signs: signs.into_iter().map(|e| e.value).collect(),
        })
    }

    /// Reads, parses, and validates a level file.
    pub fn load(path: impl AsRef<Path>) -> Result<Self, LoadError> {
        let path = path.as_ref();
        let text = std::fs::read_to_string(path)
            .map_err(|error| LoadError::Io { path: path.to_path_buf(), error })?;
        Self::parse(&text).map_err(|error| LoadError::Parse { path: path.to_path_buf(), error })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pathfinding::{is_orthogonally_adjacent, plan_route, Approach};
    use crate::world::TILE_SIZE;

    /// A small valid level to mutate in tests (y counts up from the bottom row).
    const SMALL: &str = "\
[map]
.....T
......
.#....
......

[player]
0 0

[guide]
5 0

[moonpetals]
0 2
2 2
3 3

[nodes]
tree 5 3 3 1
copper 1 1 2 1
";

    fn err(text: &str) -> LevelError { Level::parse(text).expect_err("level should be rejected") }

    /// `SMALL` with one exact substring swapped.
    fn small_with(from: &str, to: &str) -> String {
        assert!(SMALL.contains(from), "test bug: {from:?} not in SMALL");
        SMALL.replacen(from, to, 1)
    }

    fn meadow() -> Level { crate::scene::meadow_level().expect("built-in meadow must parse") }

    // ---- the shipped meadow ----

    #[test]
    fn meadow_has_expected_size() {
        let level = meadow();
        assert_eq!((level.map.width(), level.map.height()), (30, 20));
        // One tile is one world unit, so the whole meadow is 30 x 20 units in 3D.
        assert_eq!(level.map.width() as f32 * TILE_SIZE, 30.0);
    }

    #[test]
    fn meadow_matches_the_original_hand_written_layout() {
        let level = meadow();
        assert_eq!(level.player_start, TilePos::new(8, 3));
        assert_eq!(level.guide, TilePos::new(24, 9));
        assert_eq!(level.moonpetals, vec![TilePos::new(14, 2), TilePos::new(21, 3), TilePos::new(23, 12)]);
        let nodes: Vec<(ResourceKind, i32, i32, u32, u32)> =
            level.nodes.iter().map(|n| (n.kind, n.tile.x, n.tile.y, n.actions, n.yield_per_action)).collect();
        assert_eq!(
            nodes,
            vec![
                (ResourceKind::Tree, 6, 16, 3, 1),
                (ResourceKind::Tree, 10, 6, 3, 1),
                (ResourceKind::Tree, 26, 6, 3, 1),
                (ResourceKind::CopperVein, 4, 10, 2, 1),
                (ResourceKind::CopperVein, 27, 16, 2, 1),
            ]
        );
        // Spot-check the map: top-left corner is a tree, the water pond and path exist.
        assert_eq!(level.map.tile_at(TilePos::new(0, 19)), Ok(Tile::Tree));
        assert_eq!(level.map.tile_at(TilePos::new(12, 10)), Ok(Tile::Water));
        assert_eq!(level.map.tile_at(TilePos::new(22, 9)), Ok(Tile::Path));
        assert_eq!(level.map.tile_at(TilePos::new(4, 10)), Ok(Tile::Rock));
    }

    #[test]
    fn click_routes_reach_the_guide_petals_and_resources_from_the_start() {
        let level = meadow();
        let start = level.player_start;
        let ends_beside = |route: &Vec<TilePos>, target: TilePos| {
            is_orthogonally_adjacent(route.last().copied().unwrap_or(start), target)
        };
        let to_guide = plan_route(&level.map, start, level.guide, Approach::Adjacent).expect("guide route");
        assert!(ends_beside(&to_guide, level.guide));
        for petal in &level.moonpetals {
            let route = plan_route(&level.map, start, *petal, Approach::OnTile).expect("petal route");
            assert_eq!(route.last(), Some(petal));
        }
        for node in &level.nodes {
            let route = plan_route(&level.map, start, node.tile, Approach::Adjacent).expect("node route");
            assert!(ends_beside(&route, node.tile), "route to {:?} must end beside it", node.tile);
        }
    }

    #[test]
    fn the_shipped_data_file_is_the_built_in_default() {
        let path = concat!(env!("CARGO_MANIFEST_DIR"), "/assets/maps/meadow.map");
        assert_eq!(Level::load(path).expect("assets/maps/meadow.map must load"), meadow());
    }

    // ---- parsing ----

    #[test]
    fn parses_a_small_level_with_y_counting_up_from_the_bottom() {
        let level = Level::parse(SMALL).unwrap();
        assert_eq!((level.map.width(), level.map.height()), (6, 4));
        // The first listed row is the top (y = 3), so the tree in "T" is at (5, 3).
        assert_eq!(level.map.tile_at(TilePos::new(5, 3)), Ok(Tile::Tree));
        assert_eq!(level.map.tile_at(TilePos::new(1, 1)), Ok(Tile::Rock));
        assert_eq!(level.player_start, TilePos::new(0, 0));
        assert_eq!(level.guide, TilePos::new(5, 0));
        assert_eq!(level.moonpetals.len(), 3);
        assert_eq!(level.nodes.len(), 2);
    }

    #[test]
    fn comments_blank_lines_case_and_spacing_are_tolerated() {
        let text = format!(
            "; leading comment\n\n  [ MAP ]  \n; a comment inside the map\n{}",
            SMALL.trim_start_matches("[map]\n")
        );
        assert_eq!(Level::parse(&text).unwrap(), Level::parse(SMALL).unwrap());
        let spaced = small_with("tree 5 3 3 1", "  tree   5  3   3  1  ");
        assert!(Level::parse(&spaced).is_ok());
    }

    #[test]
    fn rock_glyph_at_the_start_of_a_map_row_is_not_a_comment() {
        let text = small_with("......\n.#....", "......\n#.....");
        // '#' is now at (0, 1); the copper vein at (1, 1) is on grass, so placement fails,
        // which proves the row was read as map data rather than skipped.
        let e = err(&text);
        assert!(matches!(e.kind, LevelErrorKind::Placement { what: "copper vein", .. }), "{e}");
    }

    #[test]
    fn optional_sections_may_be_left_out_unless_the_quest_needs_them() {
        let no_nodes = small_with("[nodes]\ntree 5 3 3 1\ncopper 1 1 2 1\n", "");
        assert!(Level::parse(&no_nodes).unwrap().nodes.is_empty());
        let no_petals = small_with("[moonpetals]\n0 2\n2 2\n3 3\n", "");
        assert_eq!(
            err(&no_petals).kind,
            LevelErrorKind::NotEnoughMoonpetals { have: 0, need: 3 }
        );
    }

    #[test]
    fn crlf_line_endings_parse() {
        let crlf = SMALL.replace('\n', "\r\n");
        assert_eq!(Level::parse(&crlf).unwrap(), Level::parse(SMALL).unwrap());
    }

    // ---- structure errors ----

    #[test]
    fn empty_text_reports_the_first_missing_section() {
        assert_eq!(err("").kind, LevelErrorKind::MissingSection("map"));
        assert_eq!(err("; only a comment\n").kind, LevelErrorKind::MissingSection("map"));
    }

    #[test]
    fn missing_required_sections_are_named() {
        assert_eq!(err(&small_with("[player]\n0 0\n", "")).kind, LevelErrorKind::MissingSection("player"));
        assert_eq!(err(&small_with("[guide]\n5 0\n", "")).kind, LevelErrorKind::MissingSection("guide"));
        assert_eq!(err("[player]\n0 0\n[guide]\n1 0\n").kind, LevelErrorKind::MissingSection("map"));
    }

    #[test]
    fn content_before_a_header_is_reported_with_its_line() {
        let e = err("...\n[map]\n.\n");
        assert_eq!(e, LevelError { line: Some(1), kind: LevelErrorKind::ContentBeforeSection });
    }

    #[test]
    fn bad_headers_and_duplicates_point_at_their_line() {
        let e = err("[map]\n.\n[monsters]\n");
        assert_eq!(e.line, Some(3));
        assert!(matches!(e.kind, LevelErrorKind::UnknownSection(ref s) if s == "[monsters]"));
        assert!(matches!(err("[map\n").kind, LevelErrorKind::UnknownSection(_)), "unclosed header");
        let dup = err(&format!("{SMALL}\n[guide]\n1 1\n"));
        assert_eq!(dup.kind, LevelErrorKind::DuplicateSection("guide"));
        assert_eq!(dup.line, Some(SMALL.lines().count() + 2));
    }

    #[test]
    fn empty_map_section_is_rejected() {
        let text = "[map]\n; nothing here\n[player]\n0 0\n[guide]\n1 0\n";
        assert_eq!(err(text).kind, LevelErrorKind::EmptyMap);
    }

    // ---- map errors ----

    #[test]
    fn unknown_glyph_reports_file_line_and_column() {
        let text = small_with("......\n.#....", "......\n.#.?..");
        let e = err(&text);
        // [map] is line 1, so the third row is line 4.
        assert_eq!(e, LevelError { line: Some(4), kind: LevelErrorKind::UnknownGlyph { glyph: '?', column: 4 } });
        assert!(e.to_string().contains("line 4"), "{e}");
    }

    #[test]
    fn ragged_rows_report_the_offending_line() {
        let text = small_with("......\n.#....", "......\n.#...");
        let e = err(&text);
        assert_eq!(e, LevelError { line: Some(4), kind: LevelErrorKind::RaggedMapRow { expected: 6, found: 5 } });
    }

    #[test]
    fn glyph_line_numbers_account_for_comments_and_blank_lines_above() {
        let text = format!("; one\n; two\n\n{}", small_with("......\n.#....", "......\n.#.?.."));
        assert_eq!(err(&text).line, Some(7));
    }

    // ---- line-shape errors ----

    #[test]
    fn malformed_coordinates_are_reported() {
        for (from, to) in [("[player]\n0 0", "[player]\n0"), ("[player]\n0 0", "[player]\n0 0 0"), ("[player]\n0 0", "[player]\na b"), ("[player]\n0 0", "[player]\n0.5 1")] {
            let e = err(&small_with(from, to));
            assert!(matches!(e.kind, LevelErrorKind::Malformed { .. }), "{to:?} gave {e}");
            assert!(e.line.is_some());
        }
    }

    #[test]
    fn malformed_and_unknown_nodes_are_reported() {
        let e = err(&small_with("tree 5 3 3 1", "tree 5 3 3"));
        assert!(matches!(e.kind, LevelErrorKind::Malformed { .. }), "{e}");
        let e = err(&small_with("tree 5 3 3 1", "tree 5 3 x 1"));
        assert!(matches!(e.kind, LevelErrorKind::Malformed { .. }), "{e}");
        let e = err(&small_with("tree 5 3 3 1", "tree 5 3 -1 1"));
        assert!(matches!(e.kind, LevelErrorKind::Malformed { .. }), "negative counts: {e}");
        let e = err(&small_with("tree 5 3 3 1", "dragon 5 3 3 1"));
        assert!(matches!(e.kind, LevelErrorKind::UnknownResource(ref s) if s == "dragon"), "{e}");
        let e = err(&small_with("tree 5 3 3 1", "moonpetal 5 3 1 1"));
        assert!(matches!(e.kind, LevelErrorKind::UnknownResource(_)), "moonpetals have their own section: {e}");
    }

    #[test]
    fn player_and_guide_need_exactly_one_entry() {
        let two = err(&small_with("[player]\n0 0", "[player]\n0 0\n1 0"));
        assert_eq!(two.kind, LevelErrorKind::WrongEntryCount { section: "player", found: 2 });
        let none = err(&small_with("[guide]\n5 0", "[guide]\n"));
        assert_eq!(none.kind, LevelErrorKind::WrongEntryCount { section: "guide", found: 0 });
    }

    // ---- placement and reachability errors ----

    #[test]
    fn player_and_guide_must_stand_on_walkable_in_bounds_tiles() {
        let e = err(&small_with("[player]\n0 0", "[player]\n9 9"));
        assert!(matches!(e.kind, LevelErrorKind::OffMap { what: "player start", .. }), "{e}");
        let e = err(&small_with("[player]\n0 0", "[player]\n-1 0"));
        assert!(matches!(e.kind, LevelErrorKind::OffMap { .. }), "negative coordinates: {e}");
        let e = err(&small_with("[guide]\n5 0", "[guide]\n5 3"));
        assert_eq!(e.kind, LevelErrorKind::NotWalkable { what: "guide", tile: TilePos::new(5, 3), found: Tile::Tree });
    }

    #[test]
    fn guide_cannot_share_the_players_tile_or_be_walled_off() {
        let same = err(&small_with("[guide]\n5 0", "[guide]\n0 0"));
        assert!(matches!(same.kind, LevelErrorKind::Duplicate { .. }), "{same}");
        let walled = "[map]\n..T.\n..T.\n..T.\n\n[player]\n0 0\n[guide]\n3 0\n";
        let e = err(walled);
        assert_eq!(e.kind, LevelErrorKind::Unreachable { what: "guide", tile: TilePos::new(3, 0) });
    }

    #[test]
    fn moonpetals_must_be_on_walkable_distinct_reachable_tiles() {
        // (5, 3) is a tree tile.
        let e = err(&small_with("0 2\n2 2\n3 3", "0 2\n2 2\n5 3"));
        assert!(matches!(e.kind, LevelErrorKind::Placement { what: "moonpetal", .. }), "{e}");
        let e = err(&small_with("0 2\n2 2\n3 3", "0 2\n2 2\n2 2"));
        assert!(matches!(e.kind, LevelErrorKind::Duplicate { what: "moonpetal", .. }), "{e}");
        let e = err(&small_with("0 2\n2 2\n3 3", "0 2\n2 2\n3 9"));
        assert!(matches!(e.kind, LevelErrorKind::Placement { error: PlacementError::OutOfBounds, .. }), "{e}");
        let islands = "[map]\n.T.\n.T.\n\n[player]\n0 0\n[guide]\n0 1\n[moonpetals]\n2 0\n2 1\n0 0\n";
        let e = err(islands);
        assert_eq!(e.kind, LevelErrorKind::Unreachable { what: "moonpetal", tile: TilePos::new(2, 0) });
    }

    #[test]
    fn nodes_must_match_their_tile_and_actually_yield() {
        let e = err(&small_with("tree 5 3 3 1", "tree 0 1 3 1"));
        assert!(
            matches!(e.kind, LevelErrorKind::Placement { error: PlacementError::WrongTile { expected: "tree", found: Tile::Grass }, .. }),
            "{e}"
        );
        let e = err(&small_with("copper 1 1 2 1", "copper 5 3 2 1"));
        assert!(matches!(e.kind, LevelErrorKind::Placement { .. }), "copper on a tree tile: {e}");
        for zero in ["tree 5 3 0 1", "tree 5 3 3 0"] {
            let e = err(&small_with("tree 5 3 3 1", zero));
            assert_eq!(e.kind, LevelErrorKind::NeverYields { tile: TilePos::new(5, 3) }, "{zero}");
        }
    }

    #[test]
    fn nodes_cannot_overlap_and_must_be_approachable() {
        let e = err(&small_with("tree 5 3 3 1", "copper 1 1 2 1\ncopper 1 1 2 1"));
        assert!(matches!(e.kind, LevelErrorKind::Duplicate { .. }), "{e}");
        // A tree sealed inside rock on all sides cannot be reached.
        let sealed = "[map]\n.....\n.###.\n.#T#.\n.###.\n.....\n\n[player]\n0 0\n[guide]\n4 0\n[moonpetals]\n1 4\n3 4\n0 2\n[nodes]\ntree 2 2 3 1\n";
        let e = err(sealed);
        assert_eq!(e.kind, LevelErrorKind::Unreachable { what: "tree", tile: TilePos::new(2, 2) });
    }

    #[test]
    fn errors_name_their_line_and_read_clearly() {
        let e = err(&small_with("copper 1 1 2 1", "copper 0 1 2 1"));
        let line = SMALL.lines().position(|l| l == "copper 1 1 2 1").unwrap() + 1;
        assert_eq!(e.line, Some(line));
        let text = e.to_string();
        assert!(text.starts_with(&format!("line {line}: ")), "{text}");
        assert!(text.contains("copper vein") && text.contains("rock"), "{text}");
        // Errors without a line omit the prefix.
        assert!(!err("").to_string().starts_with("line"));
    }

    // ---- fences and signposts ----

    /// `SMALL` with a signpost at (1, 2) and the given `[signs]` body appended.
    fn with_signs(body: &str) -> String {
        let mut text = small_with(".....T\n......\n.#....", ".....T\n.S....\n.#....");
        text.push_str("\n[signs]\n");
        text.push_str(body);
        text.push('\n');
        text
    }

    #[test]
    fn meadow_has_a_fence_rail_and_readable_signposts() {
        let level = meadow();
        for x in 11..=15 {
            assert_eq!(level.map.tile_at(TilePos::new(x, 7)), Ok(Tile::Fence), "fence at x={x}");
            assert!(!level.map.can_walk_to(TilePos::new(x, 7)));
        }
        assert_eq!(level.signs.len(), 2);
        for sign in &level.signs {
            assert_eq!(level.map.tile_at(sign.tile), Ok(Tile::Sign));
            assert!(!sign.text.is_empty());
        }
    }

    #[test]
    fn the_fence_rail_does_not_cut_the_meadow_in_two() {
        let level = meadow();
        let start = level.player_start;
        for y in 0..level.map.height() as i32 {
            for x in 0..level.map.width() as i32 {
                let tile = TilePos::new(x, y);
                if level.map.can_walk_to(tile) {
                    assert!(level.map.is_reachable(start, tile), "{tile:?} became unreachable");
                }
            }
        }
    }

    #[test]
    fn parses_signs_with_spaces_in_their_text() {
        let level = Level::parse(&with_signs("1 2   Welcome to the meadow, traveller!  ")).unwrap();
        assert_eq!(level.signs, vec![SignSpawn { tile: TilePos::new(1, 2), text: "Welcome to the meadow, traveller!".into() }]);
        assert_eq!(level.map.tile_at(TilePos::new(1, 2)), Ok(Tile::Sign));
    }

    #[test]
    fn a_level_without_signs_has_none_and_an_unlisted_post_is_scenery() {
        assert!(Level::parse(SMALL).unwrap().signs.is_empty());
        let mut text = small_with(".....T\n......\n.#....", ".....T\n.S....\n.#....");
        text.push('\n');
        assert!(Level::parse(&text).unwrap().signs.is_empty());
    }

    #[test]
    fn malformed_signs_are_reported_with_their_line() {
        for bad in ["1 2", "1 2    ", "x 2 hello", "1 y hello", "1", "hello"] {
            let text = with_signs(bad);
            let line = text.lines().count();
            let e = err(&text);
            assert_eq!(e.line, Some(line), "{bad:?}");
            assert!(matches!(e.kind, LevelErrorKind::Malformed { .. }), "{bad:?}: {e}");
        }
    }

    #[test]
    fn signs_must_stand_on_signpost_tiles_inside_the_map() {
        let e = err(&with_signs("3 2 Wrong tile"));
        assert_eq!(
            e.kind,
            LevelErrorKind::Placement {
                what: "signpost",
                tile: TilePos::new(3, 2),
                error: PlacementError::WrongTile { expected: "signpost", found: Tile::Grass },
            }
        );
        assert!(e.to_string().contains("signpost"), "{e}");
        assert_eq!(err(&with_signs("9 9 Off the map")).kind, LevelErrorKind::OffMap { what: "signpost", tile: TilePos::new(9, 9) });
        assert_eq!(err(&with_signs("-1 0 Off the map")).kind, LevelErrorKind::OffMap { what: "signpost", tile: TilePos::new(-1, 0) });
    }

    #[test]
    fn signs_cannot_overlap_or_be_walled_off() {
        let e = err(&with_signs("1 2 First\n1 2 Second"));
        assert_eq!(e.kind, LevelErrorKind::Duplicate { what: "signpost", tile: TilePos::new(1, 2) });
        // Wall the post and the whole top row off from the player with fences.
        let text = SMALL
            .replacen(".....T\n......\n.#....\n......", "FFFFFF\nFSFFFF\nF#....\n......", 1)
            .replace("0 2\n2 2\n3 3\n", "0 0\n2 0\n3 0\n")
            .replace("tree 5 3 3 1\n", "")
            + "\n[signs]\n1 2 Hidden\n";
        let e = err(&text);
        assert_eq!(e.kind, LevelErrorKind::Unreachable { what: "signpost", tile: TilePos::new(1, 2) });
    }

    #[test]
    fn fences_block_the_player_start_the_guide_and_pickups() {
        let on_fence = SMALL.replacen(".....T\n", "F....T\n", 1);
        // (0, 3) is now a fence; a guide there is rejected as not walkable.
        let e = err(&on_fence.replace("[guide]\n5 0", "[guide]\n0 3"));
        assert_eq!(e.kind, LevelErrorKind::NotWalkable { what: "guide", tile: TilePos::new(0, 3), found: Tile::Fence });
        assert!(e.to_string().contains("Fence"), "{e}");
    }

    // ---- files ----

    #[test]
    fn loading_a_missing_file_reports_the_path() {
        let e = Level::load("definitely/not/here.map").unwrap_err();
        assert!(matches!(e, LoadError::Io { .. }));
        assert!(e.to_string().contains("definitely/not/here.map"), "{e}");
    }

    #[test]
    fn loading_an_invalid_file_reports_path_and_line() {
        let dir = std::env::temp_dir().join(format!("ozbound-level-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("bad.map");
        std::fs::write(&path, small_with("......\n.#....", "......\n.#.?..")).unwrap();
        let e = Level::load(&path).unwrap_err();
        assert!(matches!(e, LoadError::Parse { .. }));
        let text = e.to_string();
        assert!(text.contains("bad.map") && text.contains("line 4") && text.contains("'?'"), "{text}");
        let good = dir.join("good.map");
        std::fs::write(&good, SMALL).unwrap();
        assert_eq!(Level::load(&good).unwrap(), Level::parse(SMALL).unwrap());
        std::fs::remove_dir_all(&dir).ok();
    }
}
