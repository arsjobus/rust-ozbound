//! Level data: a plain-text file describing the map and everything standing on it.
//!
//! Pure Rust (no Bevy). Parsing reports the line that is wrong, and a parsed [`Level`] has
//! already been checked: the player and guide stand on walkable tiles that connect, every
//! resource sits on a suitable tile and can be reached, and the quest can be completed.
//! Signposts (`S` tiles listed in `[signs]`) are validated like resources: right tile,
//! no duplicates, reachable from the player start.
//! The file format is documented at the top of `assets/maps/meadow.map`.
//!
//! A level with a `[guide]` is a *story level*: the guide, villagers, places, and creatures
//! that quests and dialogue refer to live there. A level without one is a *region*: scenery,
//! resources, signs, houses, and exits only. Regions may not list `[npcs]`, `[places]`, or
//! `[creatures]` yet, because story content is validated against a single story level.
//! `[houses]` stamps walk-in houses onto the map, `[plots]` groups houses with gardens and
//! vineyards (see `homestead.rs`), and `[exits]` links tiles to other regions (see
//! `locations.rs`).

use std::fmt;
use std::path::{Path, PathBuf};

use crate::appearance::Appearance;
use crate::gathering::{validate_node_placement, PlacementError, ResourceKind};
use crate::npc::{self, Behavior};
use crate::homestead::{self, HomesteadError, HouseSpawn, PlotSpawn, Side};
use crate::pathfinding::Approach;
use crate::quest::MoonpetalQuest;
use crate::textdata::{is_valid_id, is_valid_location_id};
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

/// A villager other than the guide, standing on a walkable tile. `id` is the `npc = ...` name
/// used in the dialogue file and in `talk` objectives.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NpcSpawn {
    pub id: String,
    pub tile: TilePos,
    pub look: Appearance,
    /// Shown when hovering ("Talk to Mistress Pell").
    pub name: String,
}

/// How one NPC behaves when left alone (`[behaviors]`): `id` is an NPC's id or `guide`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BehaviorSpawn {
    pub id: String,
    pub behavior: Behavior,
}

/// A named place the player can reach, for `visit` objectives.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlaceSpawn {
    pub id: String,
    pub tile: TilePos,
    pub name: String,
}

/// A creature from the bestiary standing on a walkable tile.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CreatureSpawn {
    pub id: String,
    pub tile: TilePos,
}

/// A tile that leads to another region. Walking onto it travels to `region`, arriving on
/// `arrive` (a tile of that region's map; `locations::check_world` validates it).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExitSpawn {
    pub tile: TilePos,
    /// The destination's location id (for example `YELLOW_BRICK_ROAD_EAST`).
    pub region: String,
    pub arrive: TilePos,
    /// Shown when hovering the exit and in the chat when travelling.
    pub label: String,
}

/// Everything the world needs to be built: the tile map plus what stands on it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Level {
    pub map: WorldMap,
    pub player_start: TilePos,
    /// The village guide. `None` marks a region (no story content); see the module docs.
    pub guide: Option<TilePos>,
    pub moonpetals: Vec<TilePos>,
    /// Trees, copper veins, herb patches, and berry bushes. Moonpetals are listed separately.
    pub nodes: Vec<NodeSpawn>,
    /// Readable signposts. An `S` tile with no entry here is plain scenery.
    pub signs: Vec<SignSpawn>,
    /// Villagers besides the guide (the guide is always the NPC with id `guide`).
    pub npcs: Vec<NpcSpawn>,
    /// How listed NPCs behave when left alone; anyone not listed stands still.
    pub behaviors: Vec<BehaviorSpawn>,
    /// Named places for `visit` objectives.
    pub places: Vec<PlaceSpawn>,
    /// Creatures to meet (see `bestiary.rs`).
    pub creatures: Vec<CreatureSpawn>,
    /// Walk-in houses, already stamped onto `map` as walls, doorways, and floor.
    pub houses: Vec<HouseSpawn>,
    /// Plots that group houses with gardens and vineyards.
    pub plots: Vec<PlotSpawn>,
    /// Tiles that lead to other regions.
    pub exits: Vec<ExitSpawn>,
}

/// The id the guide always has in dialogue and quests.
pub const GUIDE_ID: &str = "guide";

impl Level {
    /// True when this is a story level (it has a guide) rather than a region.
    pub fn is_story_level(&self) -> bool { self.guide.is_some() }

    /// The house the player is standing in (on its floor or doorway), if any.
    pub fn house_at(&self, tile: TilePos) -> Option<&HouseSpawn> {
        self.houses.iter().find(|house| house.is_inside(tile))
    }

    /// The exit on `tile`, if any.
    pub fn exit_at(&self, tile: TilePos) -> Option<&ExitSpawn> {
        self.exits.iter().find(|exit| exit.tile == tile)
    }

    /// How the NPC with this id behaves: its `[behaviors]` entry, or standing still.
    pub fn behavior_of(&self, id: &str) -> Behavior {
        self.behaviors.iter().find(|b| b.id == id).map(|b| b.behavior.clone()).unwrap_or_default()
    }

    /// Tiles an NPC must never step onto: waymarks, creatures, moonpetals, plants on open
    /// ground, exits, and doorways (the player must always be able to use these).
    pub fn npc_keep_out(&self) -> Vec<TilePos> {
        let mut tiles: Vec<TilePos> = self.places.iter().map(|p| p.tile).collect();
        tiles.extend(self.creatures.iter().map(|c| c.tile));
        tiles.extend(self.moonpetals.iter().copied());
        tiles.extend(self.nodes.iter().filter(|n| n.kind.interaction().approach() == Approach::OnTile).map(|n| n.tile));
        tiles.extend(self.exits.iter().map(|e| e.tile));
        tiles.extend(self.houses.iter().map(|h| h.door_tile()));
        tiles
    }

    /// Every NPC as `(id, tile, display name)`, the guide first.
    pub fn npc_list(&self) -> Vec<(&str, TilePos, &str)> {
        let mut all = Vec::new();
        if let Some(guide) = self.guide {
            all.push((GUIDE_ID, guide, "Village Guide"));
        }
        all.extend(self.npcs.iter().map(|n| (n.id.as_str(), n.tile, n.name.as_str())));
        all
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Section { Map, Player, Guide, Moonpetals, Nodes, Signs, Npcs, Behaviors, Places, Creatures, Houses, Plots, Exits }

impl Section {
    fn from_name(name: &str) -> Option<Self> {
        Some(match name {
            "map" => Self::Map,
            "player" => Self::Player,
            "guide" => Self::Guide,
            "moonpetals" => Self::Moonpetals,
            "nodes" => Self::Nodes,
            "signs" => Self::Signs,
            "npcs" => Self::Npcs,
            "behaviors" => Self::Behaviors,
            "places" => Self::Places,
            "creatures" => Self::Creatures,
            "houses" => Self::Houses,
            "plots" => Self::Plots,
            "exits" => Self::Exits,
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
            Self::Npcs => "npcs",
            Self::Behaviors => "behaviors",
            Self::Places => "places",
            Self::Creatures => "creatures",
            Self::Houses => "houses",
            Self::Plots => "plots",
            Self::Exits => "exits",
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
    /// An NPC, place, or creature id that is not lower-case letters, digits, and underscores.
    InvalidId { what: &'static str, id: String },
    /// The id `guide` belongs to the `[guide]` entry.
    ReservedId(String),
    DuplicateId { what: &'static str, id: String },
    UnknownAppearance(String),
    /// Two things stand on the same tile (an NPC on a place, a creature on a moonpetal, ...).
    Overlap { what: &'static str, other: &'static str, tile: TilePos },
    /// A house or plot problem (see `homestead.rs`).
    Homestead(HomesteadError),
    /// A `[behaviors]` line that does not parse (the message says why).
    BadBehavior(String),
    /// A `[behaviors]` entry for someone who is neither the guide nor in `[npcs]`.
    UnknownBehaviorTarget(String),
    /// A wanderer with nowhere to wander: every tile around it is blocked or must stay clear.
    WanderStuck { id: String, tile: TilePos, radius: u32 },
    /// A region (a level with no `[guide]`) lists story content.
    RegionHasStory(&'static str),
    /// An exit's destination is not a location id such as `YELLOW_BRICK_ROAD_EAST`.
    InvalidLocationId(String),
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
                "unknown or unclosed section header {name:?} (expected [map], [player], [guide], [moonpetals], [nodes], [signs], [npcs], [behaviors], [places], [creatures], [houses], [plots] or [exits])"
            ),
            DuplicateSection(name) => write!(f, "section [{name}] appears more than once"),
            MissingSection(name) => write!(f, "required section [{name}] is missing"),
            EmptyMap => write!(f, "[map] has no rows"),
            RaggedMapRow { expected, found } => {
                write!(f, "map row is {found} tiles wide but the first row is {expected}")
            }
            UnknownGlyph { glyph, column } => write!(
                f,
                "unknown map glyph {glyph:?} at column {column} (use . grass, = path, ~ water, T tree, # rock, F fence, S signpost, W wall, + doorway, , floor, g garden, v vineyard)"
            ),
            Malformed { expected, found } => write!(f, "expected {expected}, found {found:?}"),
            UnknownResource(name) => write!(f, "unknown resource {name:?} (use tree, copper, herb, or berry)"),
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
            InvalidId { what, id } => {
                write!(f, "{what} id {id:?} must use only lower-case letters, digits, and underscores")
            }
            ReservedId(id) => write!(f, "the id {id:?} is reserved for the [guide] entry"),
            DuplicateId { what, id } => write!(f, "{what} id {id:?} is used twice"),
            UnknownAppearance(name) => {
                write!(f, "unknown appearance {name:?} (use {})", Appearance::KEYS.join(", "))
            }
            Overlap { what, other, tile } => {
                write!(f, "{what} at ({}, {}) stands on the same tile as a {other}", tile.x, tile.y)
            }
            Homestead(error) => write!(f, "{error}"),
            RegionHasStory(section) => write!(
                f,
                "[{section}] needs a [guide]: a level without one is a region, and regions cannot have villagers, places, or creatures yet"
            ),
            BadBehavior(why) => write!(f, "bad behavior: {why}"),
            UnknownBehaviorTarget(id) => write!(
                f,
                "behavior for {id:?}, who is neither the guide nor listed in [npcs]"
            ),
            WanderStuck { id, tile, radius } => write!(
                f,
                "{id:?} at ({}, {}) cannot move anywhere within {radius} tiles: the ground around is blocked or must stay clear (a waymark, plant, creature, doorway, or exit)",
                tile.x, tile.y
            ),
            InvalidLocationId(id) => write!(
                f,
                "exit destination {id:?} must be a location id: capital letters, digits, and underscores (for example YELLOW_BRICK_ROAD_EAST)"
            ),
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
        "herb" => ResourceKind::HerbPatch,
        "berry" => ResourceKind::BerryBush,
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

/// `id x y appearance name...`: an NPC. The name may contain spaces.
fn parse_npc(text: &str, line: usize) -> Result<NpcSpawn, LevelError> {
    let malformed = || LevelError::at(line, LevelErrorKind::Malformed { expected: "\"id x y appearance name\" for an NPC", found: text.to_string() });
    let mut parts = text.splitn(5, char::is_whitespace);
    let id = parts.next().filter(|t| !t.is_empty()).ok_or_else(malformed)?;
    let x = parts.next().and_then(|t| t.parse().ok()).ok_or_else(malformed)?;
    let y = parts.next().and_then(|t| t.parse().ok()).ok_or_else(malformed)?;
    let look_key = parts.next().ok_or_else(malformed)?;
    let name = parts.next().map(str::trim).filter(|m| !m.is_empty()).ok_or_else(malformed)?;
    if !is_valid_id(id) {
        return Err(LevelError::at(line, LevelErrorKind::InvalidId { what: "npc", id: id.to_string() }));
    }
    let look = Appearance::from_key(look_key)
        .ok_or_else(|| LevelError::at(line, LevelErrorKind::UnknownAppearance(look_key.to_string())))?;
    Ok(NpcSpawn { id: id.to_string(), tile: TilePos::new(x, y), look, name: name.to_string() })
}

/// `id behavior...`: how an NPC behaves when left alone (see `npc::Behavior::parse`).
fn parse_behavior(text: &str, line: usize) -> Result<BehaviorSpawn, LevelError> {
    let bad = |why: String| LevelError::at(line, LevelErrorKind::BadBehavior(why));
    let (id, rest) = text.split_once(char::is_whitespace).ok_or_else(|| bad("expected \"id stay\" or \"id wander <radius>\"".into()))?;
    if !is_valid_id(id) {
        return Err(LevelError::at(line, LevelErrorKind::InvalidId { what: "behavior", id: id.to_string() }));
    }
    let behavior = Behavior::parse(rest).map_err(bad)?;
    Ok(BehaviorSpawn { id: id.to_string(), behavior })
}

/// `id x y name...`: a named place. The name may contain spaces.
fn parse_place(text: &str, line: usize) -> Result<PlaceSpawn, LevelError> {
    let malformed = || LevelError::at(line, LevelErrorKind::Malformed { expected: "\"id x y name\" for a place", found: text.to_string() });
    let mut parts = text.splitn(4, char::is_whitespace);
    let id = parts.next().filter(|t| !t.is_empty()).ok_or_else(malformed)?;
    let x = parts.next().and_then(|t| t.parse().ok()).ok_or_else(malformed)?;
    let y = parts.next().and_then(|t| t.parse().ok()).ok_or_else(malformed)?;
    let name = parts.next().map(str::trim).filter(|m| !m.is_empty()).ok_or_else(malformed)?;
    if !is_valid_id(id) {
        return Err(LevelError::at(line, LevelErrorKind::InvalidId { what: "place", id: id.to_string() }));
    }
    Ok(PlaceSpawn { id: id.to_string(), tile: TilePos::new(x, y), name: name.to_string() })
}

/// `id x y`: a creature.
fn parse_creature(text: &str, line: usize) -> Result<CreatureSpawn, LevelError> {
    let malformed = || LevelError::at(line, LevelErrorKind::Malformed { expected: "\"id x y\" for a creature", found: text.to_string() });
    let tokens: Vec<&str> = text.split_whitespace().collect();
    let [id, x, y] = tokens.as_slice() else { return Err(malformed()) };
    if !is_valid_id(id) {
        return Err(LevelError::at(line, LevelErrorKind::InvalidId { what: "creature", id: (*id).to_string() }));
    }
    Ok(CreatureSpawn {
        id: (*id).to_string(),
        tile: TilePos::new(x.parse().map_err(|_| malformed())?, y.parse().map_err(|_| malformed())?),
    })
}

/// `id x y width height door_side name...`: a walk-in house. `x y` is its south-west corner.
fn parse_house(text: &str, line: usize) -> Result<HouseSpawn, LevelError> {
    let malformed = || LevelError::at(line, LevelErrorKind::Malformed { expected: "\"id x y width height door_side name\" for a house", found: text.to_string() });
    let mut parts = text.splitn(7, char::is_whitespace);
    let id = parts.next().filter(|t| !t.is_empty()).ok_or_else(malformed)?;
    let x = parts.next().and_then(|t| t.parse().ok()).ok_or_else(malformed)?;
    let y = parts.next().and_then(|t| t.parse().ok()).ok_or_else(malformed)?;
    let width = parts.next().and_then(|t| t.parse().ok()).ok_or_else(malformed)?;
    let height = parts.next().and_then(|t| t.parse().ok()).ok_or_else(malformed)?;
    let side = parts.next().ok_or_else(malformed)?;
    let name = parts.next().map(str::trim).filter(|m| !m.is_empty()).ok_or_else(malformed)?;
    if !is_valid_id(id) {
        return Err(LevelError::at(line, LevelErrorKind::InvalidId { what: "house", id: id.to_string() }));
    }
    let door = Side::from_name(side).ok_or_else(|| {
        LevelError::at(line, LevelErrorKind::Malformed { expected: "a door side: north, south, east, or west", found: side.to_string() })
    })?;
    Ok(HouseSpawn { id: id.to_string(), origin: TilePos::new(x, y), width, height, door, name: name.to_string() })
}

/// `id x y width height name...`: a plot of land. `x y` is its south-west corner.
fn parse_plot(text: &str, line: usize) -> Result<PlotSpawn, LevelError> {
    let malformed = || LevelError::at(line, LevelErrorKind::Malformed { expected: "\"id x y width height name\" for a plot", found: text.to_string() });
    let mut parts = text.splitn(6, char::is_whitespace);
    let id = parts.next().filter(|t| !t.is_empty()).ok_or_else(malformed)?;
    let x = parts.next().and_then(|t| t.parse().ok()).ok_or_else(malformed)?;
    let y = parts.next().and_then(|t| t.parse().ok()).ok_or_else(malformed)?;
    let width = parts.next().and_then(|t| t.parse().ok()).ok_or_else(malformed)?;
    let height = parts.next().and_then(|t| t.parse().ok()).ok_or_else(malformed)?;
    let name = parts.next().map(str::trim).filter(|m| !m.is_empty()).ok_or_else(malformed)?;
    if !is_valid_id(id) {
        return Err(LevelError::at(line, LevelErrorKind::InvalidId { what: "plot", id: id.to_string() }));
    }
    Ok(PlotSpawn { id: id.to_string(), origin: TilePos::new(x, y), width, height, name: name.to_string() })
}

/// `x y DESTINATION tx ty label...`: a tile that leads to another region.
fn parse_exit(text: &str, line: usize) -> Result<ExitSpawn, LevelError> {
    let malformed = || LevelError::at(line, LevelErrorKind::Malformed { expected: "\"x y DESTINATION tx ty label\" for an exit", found: text.to_string() });
    let mut parts = text.splitn(6, char::is_whitespace);
    let x = parts.next().and_then(|t| t.parse().ok()).ok_or_else(malformed)?;
    let y = parts.next().and_then(|t| t.parse().ok()).ok_or_else(malformed)?;
    let region = parts.next().filter(|t| !t.is_empty()).ok_or_else(malformed)?;
    let tx = parts.next().and_then(|t| t.parse().ok()).ok_or_else(malformed)?;
    let ty = parts.next().and_then(|t| t.parse().ok()).ok_or_else(malformed)?;
    let label = parts.next().map(str::trim).filter(|m| !m.is_empty()).ok_or_else(malformed)?;
    if !is_valid_location_id(region) {
        return Err(LevelError::at(line, LevelErrorKind::InvalidLocationId(region.to_string())));
    }
    Ok(ExitSpawn { tile: TilePos::new(x, y), region: region.to_string(), arrive: TilePos::new(tx, ty), label: label.to_string() })
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
        let mut npcs: Vec<Entry<NpcSpawn>> = Vec::new();
        let mut behaviors: Vec<Entry<BehaviorSpawn>> = Vec::new();
        let mut places: Vec<Entry<PlaceSpawn>> = Vec::new();
        let mut creatures: Vec<Entry<CreatureSpawn>> = Vec::new();
        let mut houses: Vec<Entry<HouseSpawn>> = Vec::new();
        let mut plots: Vec<Entry<PlotSpawn>> = Vec::new();
        let mut exits: Vec<Entry<ExitSpawn>> = Vec::new();

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
                Section::Npcs => npcs.push(Entry { line: line_no, value: parse_npc(line, line_no)? }),
                Section::Behaviors => behaviors.push(Entry { line: line_no, value: parse_behavior(line, line_no)? }),
                Section::Places => places.push(Entry { line: line_no, value: parse_place(line, line_no)? }),
                Section::Creatures => creatures.push(Entry { line: line_no, value: parse_creature(line, line_no)? }),
                Section::Houses => houses.push(Entry { line: line_no, value: parse_house(line, line_no)? }),
                Section::Plots => plots.push(Entry { line: line_no, value: parse_plot(line, line_no)? }),
                Section::Exits => exits.push(Entry { line: line_no, value: parse_exit(line, line_no)? }),
            }
        }

        for (required, section) in [("map", Section::Map), ("player", Section::Player)] {
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
        let mut map = WorldMap::from_ascii(&map_text).map_err(|error| match error {
            MapError::UnknownGlyph { glyph, line, column } => {
                LevelError::at(line, LevelErrorKind::UnknownGlyph { glyph, column })
            }
            // Rows were checked above, so only an empty map can remain.
            _ => LevelError::general(LevelErrorKind::EmptyMap),
        })?;

        // Houses are stamped onto the map first, so every later check (player, guide, villagers,
        // reachability) sees walls, doorways, and floors exactly as the player will.
        let house_list: Vec<HouseSpawn> = houses.iter().map(|e| e.value.clone()).collect();
        homestead::stamp_houses(&mut map, &house_list)
            .map_err(|(i, error)| LevelError::at(houses[i].line, LevelErrorKind::Homestead(error)))?;

        let single = |entries: &[Entry<TilePos>], section: &'static str| -> Result<(usize, TilePos), LevelError> {
            match entries {
                [only] => Ok((only.line, only.value)),
                _ => Err(LevelError::general(LevelErrorKind::WrongEntryCount { section, found: entries.len() })),
            }
        };
        let (player_line, player_start) = single(&player, "player")?;
        check_walkable(&map, "player start", player_start, player_line)?;

        // The guide is optional: without one the level is a region (scenery, resources, signs,
        // houses, and exits), and it may not carry story content.
        let guide_tile = if seen.contains(&Section::Guide) {
            let (guide_line, guide_tile) = single(&guide, "guide")?;
            check_walkable(&map, "guide", guide_tile, guide_line)?;
            if guide_tile == player_start {
                return Err(LevelError::at(guide_line, LevelErrorKind::Duplicate { what: "guide (on the player start)", tile: guide_tile }));
            }
            if !map.is_reachable(player_start, guide_tile) {
                return Err(LevelError::at(guide_line, LevelErrorKind::Unreachable { what: "guide", tile: guide_tile }));
            }
            Some(guide_tile)
        } else {
            for (section, found) in [("npcs", &npcs.first().map(|e| e.line)), ("behaviors", &behaviors.first().map(|e| e.line)), ("places", &places.first().map(|e| e.line)), ("creatures", &creatures.first().map(|e| e.line))] {
                if let Some(line) = found {
                    return Err(LevelError::at(*line, LevelErrorKind::RegionHasStory(section)));
                }
            }
            None
        };

        homestead::check_doors(&map, &house_list, player_start)
            .map_err(|(i, error)| LevelError::at(houses[i].line, LevelErrorKind::Homestead(error)))?;
        let plot_list: Vec<PlotSpawn> = plots.iter().map(|e| e.value.clone()).collect();
        homestead::check_plots(&map, &house_list, &plot_list)
            .map_err(|(i, error)| LevelError::at(plots[i].line, LevelErrorKind::Homestead(error)))?;

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
        if guide_tile.is_some() && moonpetals.len() < need {
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
            // Plants on open ground are used from their own tile; trees and rocks from beside them.
            let reachable = match node.kind.interaction().approach() {
                Approach::OnTile => map.is_reachable(player_start, node.tile),
                Approach::Adjacent => has_reachable_neighbour(&map, player_start, node.tile),
            };
            if !reachable {
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


        // Villagers, places, and creatures. Each needs a valid unique id, a walkable and
        // reachable tile, and a tile of its own, so nothing is hidden under or blocks another.
        let mut occupied: Vec<(&'static str, TilePos)> = vec![("player start", player_start)];
        occupied.extend(guide_tile.map(|tile| ("guide", tile)));
        // A doorway must stay clear of people and creatures (a place or plant may mark it).
        occupied.extend(house_list.iter().map(|h| ("doorway", h.door_tile())));
        occupied.extend(moonpetals.iter().map(|e| ("moonpetal", e.value)));
        occupied.extend(nodes.iter().filter(|e| e.value.kind.interaction().approach() == Approach::OnTile).map(|e| ("plant", e.value.tile)));
        let mut claim = |what: &'static str, tile: TilePos, line: usize| -> Result<(), LevelError> {
            if let Some((other, _)) = occupied.iter().find(|(_, t)| *t == tile) {
                return Err(LevelError::at(line, LevelErrorKind::Overlap { what, other, tile }));
            }
            occupied.push((what, tile));
            Ok(())
        };
        for (i, entry) in npcs.iter().enumerate() {
            let npc = &entry.value;
            if npc.id == GUIDE_ID {
                return Err(LevelError::at(entry.line, LevelErrorKind::ReservedId(npc.id.clone())));
            }
            if npcs[..i].iter().any(|other| other.value.id == npc.id) {
                return Err(LevelError::at(entry.line, LevelErrorKind::DuplicateId { what: "npc", id: npc.id.clone() }));
            }
            check_walkable(&map, "npc", npc.tile, entry.line)?;
            if !map.is_reachable(player_start, npc.tile) {
                return Err(LevelError::at(entry.line, LevelErrorKind::Unreachable { what: "npc", tile: npc.tile }));
            }
            claim("npc", npc.tile, entry.line)?;
        }
        for (i, entry) in places.iter().enumerate() {
            let place = &entry.value;
            if places[..i].iter().any(|other| other.value.id == place.id) {
                return Err(LevelError::at(entry.line, LevelErrorKind::DuplicateId { what: "place", id: place.id.clone() }));
            }
            check_walkable(&map, "place", place.tile, entry.line)?;
            if !map.is_reachable(player_start, place.tile) {
                return Err(LevelError::at(entry.line, LevelErrorKind::Unreachable { what: "place", tile: place.tile }));
            }
            // A place may share a tile with a moonpetal or plant, but not with a person: the
            // visit would then happen while talking, which is confusing to read about.
            if let Some((other, _)) = occupied.iter().find(|(what, t)| *t == place.tile && matches!(*what, "npc" | "guide" | "creature" | "place")) {
                return Err(LevelError::at(entry.line, LevelErrorKind::Overlap { what: "place", other, tile: place.tile }));
            }
            occupied.push(("place", place.tile));
        }
        for (i, entry) in creatures.iter().enumerate() {
            let creature = &entry.value;
            if creatures[..i].iter().any(|other| other.value.id == creature.id) {
                return Err(LevelError::at(entry.line, LevelErrorKind::DuplicateId { what: "creature", id: creature.id.clone() }));
            }
            check_walkable(&map, "creature", creature.tile, entry.line)?;
            if !has_reachable_neighbour(&map, player_start, creature.tile) {
                return Err(LevelError::at(entry.line, LevelErrorKind::Unreachable { what: "creature", tile: creature.tile }));
            }
            if let Some((other, _)) = occupied.iter().find(|(what, t)| *t == creature.tile && *what != "place") {
                return Err(LevelError::at(entry.line, LevelErrorKind::Overlap { what: "creature", other, tile: creature.tile }));
            }
            occupied.push(("creature", creature.tile));
        }

        // Behaviors: one per person, only for people who exist, and a wanderer needs room to roam.
        let people: Vec<(&str, TilePos)> = guide_tile.iter().map(|t| (GUIDE_ID, *t)).chain(npcs.iter().map(|e| (e.value.id.as_str(), e.value.tile))).collect();
        let keep_out: std::collections::HashSet<TilePos> = {
            let mut tiles: Vec<TilePos> = places.iter().map(|e| e.value.tile).collect();
            tiles.extend(creatures.iter().map(|e| e.value.tile));
            tiles.extend(moonpetals.iter().map(|e| e.value));
            tiles.extend(nodes.iter().filter(|e| e.value.kind.interaction().approach() == Approach::OnTile).map(|e| e.value.tile));
            tiles.extend(exits.iter().map(|e| e.value.tile));
            tiles.extend(house_list.iter().map(|h| h.door_tile()));
            tiles.into_iter().collect()
        };
        for (i, entry) in behaviors.iter().enumerate() {
            let spawn = &entry.value;
            let Some(&(_, home)) = people.iter().find(|(id, _)| *id == spawn.id) else {
                return Err(LevelError::at(entry.line, LevelErrorKind::UnknownBehaviorTarget(spawn.id.clone())));
            };
            if behaviors[..i].iter().any(|other| other.value.id == spawn.id) {
                return Err(LevelError::at(entry.line, LevelErrorKind::DuplicateId { what: "behavior", id: spawn.id.clone() }));
            }
            if let Some(radius) = spawn.behavior.radius() {
                if npc::wander_area(&map, home, radius, &keep_out) == 0 {
                    return Err(LevelError::at(entry.line, LevelErrorKind::WanderStuck { id: spawn.id.clone(), tile: home, radius }));
                }
            }
        }

        // Exits: a walkable, reachable tile of their own, listed once. The destination is
        // checked against the other region's map by `locations::check_world`.
        for (i, entry) in exits.iter().enumerate() {
            let exit = &entry.value;
            check_walkable(&map, "exit", exit.tile, entry.line)?;
            if exits[..i].iter().any(|other| other.value.tile == exit.tile) {
                return Err(LevelError::at(entry.line, LevelErrorKind::Duplicate { what: "exit", tile: exit.tile }));
            }
            if !map.is_reachable(player_start, exit.tile) {
                return Err(LevelError::at(entry.line, LevelErrorKind::Unreachable { what: "exit", tile: exit.tile }));
            }
            if let Some((other, _)) = occupied.iter().find(|(_, t)| *t == exit.tile) {
                return Err(LevelError::at(entry.line, LevelErrorKind::Overlap { what: "exit", other, tile: exit.tile }));
            }
        }

        Ok(Self {
            map,
            player_start,
            guide: guide_tile,
            houses: house_list,
            plots: plot_list,
            exits: exits.into_iter().map(|e| e.value).collect(),
            moonpetals: moonpetals.into_iter().map(|e| e.value).collect(),
            nodes: nodes.into_iter().map(|e| e.value).collect(),
            signs: signs.into_iter().map(|e| e.value).collect(),
            npcs: npcs.into_iter().map(|e| e.value).collect(),
            behaviors: behaviors.into_iter().map(|e| e.value).collect(),
            places: places.into_iter().map(|e| e.value).collect(),
            creatures: creatures.into_iter().map(|e| e.value).collect(),
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
        assert_eq!(level.guide, Some(TilePos::new(24, 9)));
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
                (ResourceKind::HerbPatch, 5, 8, 3, 1),
                (ResourceKind::HerbPatch, 20, 15, 3, 1),
                (ResourceKind::BerryBush, 18, 13, 3, 1),
                (ResourceKind::BerryBush, 9, 1, 3, 1),
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
        let guide = level.guide.expect("the meadow has a guide");
        let to_guide = plan_route(&level.map, start, guide, Approach::Adjacent).expect("guide route");
        assert!(ends_beside(&to_guide, guide));
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
        assert_eq!(level.guide, Some(TilePos::new(5, 0)));
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
        // Without a guide the level is a region, which is valid (and has no story content).
        let region = Level::parse(&small_with("[guide]\n5 0\n", "")).expect("a level with no guide is a region");
        assert_eq!(region.guide, None);
        assert!(!region.is_story_level());
        assert!(region.npc_list().is_empty());
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

    // ---- herb patches and berry bushes ----

    #[test]
    fn herb_and_berry_lines_parse() {
        let level = Level::parse(&small_with("copper 1 1 2 1", "copper 1 1 2 1\nherb 2 0 3 1\nberry 3 1 2 2")).unwrap();
        let kinds: Vec<_> = level.nodes.iter().map(|n| (n.kind, n.tile, n.actions, n.yield_per_action)).collect();
        assert!(kinds.contains(&(ResourceKind::HerbPatch, TilePos::new(2, 0), 3, 1)));
        assert!(kinds.contains(&(ResourceKind::BerryBush, TilePos::new(3, 1), 2, 2)));
    }

    #[test]
    fn herbs_and_berries_must_stand_on_walkable_ground() {
        for kind in ["herb", "berry"] {
            // (1, 1) is a rock tile in SMALL and (5, 3) is a tree tile.
            for tile in ["1 1", "5 3"] {
                let e = err(&small_with("copper 1 1 2 1", &format!("copper 1 1 2 1\n{kind} {tile} 3 1")));
                assert!(matches!(e.kind, LevelErrorKind::Placement { .. }), "{kind} at {tile}: {e}");
                assert!(e.to_string().contains("walkable ground"), "{e}");
            }
        }
    }

    #[test]
    fn herbs_and_berries_cannot_share_a_tile_or_never_yield() {
        let e = err(&small_with("copper 1 1 2 1", "copper 1 1 2 1\nherb 2 0 3 1\nberry 2 0 3 1"));
        assert!(matches!(e.kind, LevelErrorKind::Duplicate { .. }), "{e}");
        let e = err(&small_with("copper 1 1 2 1", "copper 1 1 2 1\nherb 2 0 0 1"));
        assert!(matches!(e.kind, LevelErrorKind::NeverYields { .. }), "{e}");
        let e = err(&small_with("copper 1 1 2 1", "copper 1 1 2 1\nberry 2 0 3 0"));
        assert!(matches!(e.kind, LevelErrorKind::NeverYields { .. }), "{e}");
    }

    #[test]
    fn a_plant_in_a_walled_off_pocket_is_unreachable_even_if_it_has_open_neighbours() {
        // The herb tile itself is walkable grass, but it is sealed inside rock, so it cannot be
        // stood on. (A tree there would also fail, for the same reason, via its neighbours.)
        let sealed = "[map]\n.....\n.###.\n.#.#.\n.###.\n.....\n\n[player]\n0 0\n[guide]\n4 0\n[moonpetals]\n1 4\n3 4\n0 2\n[nodes]\nherb 2 2 3 1\n";
        let e = err(sealed);
        assert_eq!(e.kind, LevelErrorKind::Unreachable { what: "meadowmint patch", tile: TilePos::new(2, 2) });
        let sealed_berry = sealed.replace("herb 2 2 3 1", "berry 2 2 3 1");
        let e = err(&sealed_berry);
        assert_eq!(e.kind, LevelErrorKind::Unreachable { what: "honeyberry bush", tile: TilePos::new(2, 2) });
    }

    #[test]
    fn unknown_resource_message_lists_the_valid_kinds() {
        let e = err(&small_with("tree 5 3 3 1", "dragon 5 3 3 1"));
        let text = e.to_string();
        for kind in ["tree", "copper", "herb", "berry"] {
            assert!(text.contains(kind), "{text}");
        }
    }

    #[test]
    fn shipped_meadow_has_plants_to_forage_and_they_are_reachable_on_foot() {
        let level = meadow();
        let herbs = level.nodes.iter().filter(|n| n.kind == ResourceKind::HerbPatch).count();
        let berries = level.nodes.iter().filter(|n| n.kind == ResourceKind::BerryBush).count();
        assert!(herbs >= 2 && berries >= 2, "{herbs} herb patches, {berries} berry bushes");
        for node in level.nodes.iter().filter(|n| n.kind.interaction().approach() == Approach::OnTile) {
            let route = plan_route(&level.map, level.player_start, node.tile, Approach::OnTile).expect("route");
            assert_eq!(route.last().copied().unwrap_or(level.player_start), node.tile);
        }
    }
}

#[cfg(test)]
mod village_section_tests {
    use super::*;

    /// Same shape as the small test level: 6 x 4, player (0, 0), guide (5, 0), petals on (0, 2),
    /// (2, 2), (3, 3), a tree on (5, 3) and copper on (1, 1). y counts up from the bottom row.
    const BASE: &str = "\
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

    fn with(extra: &str) -> String {
        format!("{BASE}\n{extra}\n")
    }

    fn err(extra: &str) -> LevelError {
        Level::parse(&with(extra)).expect_err("level should be rejected")
    }

    #[test]
    fn a_level_without_the_new_sections_has_none() {
        let level = Level::parse(BASE).unwrap();
        assert!(level.npcs.is_empty() && level.places.is_empty() && level.creatures.is_empty());
        assert_eq!(level.npc_list(), vec![("guide", TilePos::new(5, 0), "Village Guide")]);
    }

    #[test]
    fn npcs_places_and_creatures_parse() {
        let level = Level::parse(&with(
            "[npcs]\nmiller 3 0 woodcutter Old Miller Brae\n\n[places]\nold_mill 4 2 The Old Mill\n\n[creatures]\nmeadow_hare 1 3\n",
        ))
        .unwrap();
        let miller = &level.npcs[0];
        assert_eq!(miller.id, "miller");
        assert_eq!(miller.tile, TilePos::new(3, 0));
        assert_eq!(miller.name, "Old Miller Brae");
        assert_eq!(Some(miller.look), Appearance::from_key("woodcutter"));
        assert_eq!(level.places, vec![PlaceSpawn { id: "old_mill".into(), tile: TilePos::new(4, 2), name: "The Old Mill".into() }]);
        assert_eq!(level.creatures, vec![CreatureSpawn { id: "meadow_hare".into(), tile: TilePos::new(1, 3) }]);
        let list = level.npc_list();
        assert_eq!(list.len(), 2);
        assert_eq!(list[0].0, "guide");
        assert_eq!(list[1], ("miller", TilePos::new(3, 0), "Old Miller Brae"));
    }

    #[test]
    fn comments_and_blank_lines_are_ignored_in_the_new_sections() {
        let level = Level::parse(&with("[npcs]\n; the miller\n\nmiller 3 0 woodcutter Miller\n")).unwrap();
        assert_eq!(level.npcs.len(), 1);
    }

    #[test]
    fn malformed_lines_name_their_line() {
        let base_lines = BASE.lines().count() + 2; // blank line from `with`, then the header
        for (extra, what) in [
            ("[npcs]\nmiller 3 0 woodcutter", "npc"),
            ("[npcs]\nmiller x 0 woodcutter Miller", "npc"),
            ("[npcs]\nmiller 3 0", "npc"),
            ("[places]\nold_mill 4 2", "place"),
            ("[places]\nold_mill four 2 Mill", "place"),
            ("[creatures]\nhare 1", "creature"),
            ("[creatures]\nhare 1 3 4", "creature"),
        ] {
            let e = err(extra);
            assert_eq!(e.line, Some(base_lines + 1), "{extra:?}");
            assert!(matches!(e.kind, LevelErrorKind::Malformed { .. }), "{extra:?}: {e}");
            assert!(e.to_string().to_lowercase().contains(what), "{extra:?}: {e}");
        }
    }

    #[test]
    fn ids_must_be_valid_unique_and_not_the_guide() {
        assert!(matches!(err("[npcs]\nBad-Id 3 0 woodcutter Miller").kind, LevelErrorKind::InvalidId { what: "npc", .. }));
        assert!(matches!(err("[places]\nOld_Mill 4 2 Mill").kind, LevelErrorKind::InvalidId { what: "place", .. }));
        assert!(matches!(err("[creatures]\nHare 1 3").kind, LevelErrorKind::InvalidId { what: "creature", .. }));
        assert_eq!(err("[npcs]\nguide 3 0 woodcutter Miller").kind, LevelErrorKind::ReservedId("guide".into()));
        let twice = err("[npcs]\nmiller 3 0 woodcutter A\nmiller 4 0 woodcutter B");
        assert_eq!(twice.kind, LevelErrorKind::DuplicateId { what: "npc", id: "miller".into() });
        let twice = err("[places]\nmill 4 2 A\nmill 3 2 B");
        assert_eq!(twice.kind, LevelErrorKind::DuplicateId { what: "place", id: "mill".into() });
        let twice = err("[creatures]\nhare 1 3\nhare 2 3");
        assert_eq!(twice.kind, LevelErrorKind::DuplicateId { what: "creature", id: "hare".into() });
    }

    #[test]
    fn unknown_appearances_list_the_valid_ones() {
        let e = err("[npcs]\nmiller 3 0 wizard Miller");
        assert_eq!(e.kind, LevelErrorKind::UnknownAppearance("wizard".into()));
        let text = e.to_string();
        assert!(text.contains("wizard") && text.contains("apothecary") && text.contains("ferrywoman"), "{text}");
    }

    #[test]
    fn everything_must_stand_on_reachable_walkable_ground() {
        // (1, 1) is rock, (5, 3) is a tree, (9, 9) is off the map.
        assert!(matches!(err("[npcs]\nmiller 1 1 woodcutter M").kind, LevelErrorKind::NotWalkable { what: "npc", .. }));
        assert!(matches!(err("[places]\nmill 5 3 Mill").kind, LevelErrorKind::NotWalkable { what: "place", .. }));
        assert!(matches!(err("[creatures]\nhare 5 3").kind, LevelErrorKind::NotWalkable { what: "creature", .. }));
        assert!(matches!(err("[npcs]\nmiller 9 9 woodcutter M").kind, LevelErrorKind::OffMap { what: "npc", .. }));
        assert!(matches!(err("[places]\nmill 9 9 Mill").kind, LevelErrorKind::OffMap { what: "place", .. }));
        assert!(matches!(err("[creatures]\nhare 9 9").kind, LevelErrorKind::OffMap { what: "creature", .. }));
    }

    #[test]
    fn walled_off_things_are_unreachable() {
        // A wall of rock across the middle row: row y=1 fully rock splits the map in two.
        let walled = "\
[map]
......
######
......

[player]
0 0

[guide]
5 0

[moonpetals]
1 0
2 0
3 0
";
        let ok = |extra: &str| Level::parse(&format!("{walled}\n{extra}\n"));
        assert!(matches!(ok("[npcs]\nmiller 3 2 woodcutter M").unwrap_err().kind, LevelErrorKind::Unreachable { what: "npc", .. }));
        assert!(matches!(ok("[places]\nmill 3 2 Mill").unwrap_err().kind, LevelErrorKind::Unreachable { what: "place", .. }));
        assert!(matches!(ok("[creatures]\nhare 3 2").unwrap_err().kind, LevelErrorKind::Unreachable { what: "creature", .. }));
        assert!(ok("[npcs]\nmiller 4 0 woodcutter M").is_ok());
    }

    #[test]
    fn people_and_creatures_never_share_a_tile_with_something_else() {
        // On the player start, the guide, a moonpetal, another npc.
        assert!(matches!(err("[npcs]\nmiller 0 0 woodcutter M").kind, LevelErrorKind::Overlap { what: "npc", other: "player start", .. }));
        assert!(matches!(err("[npcs]\nmiller 5 0 woodcutter M").kind, LevelErrorKind::Overlap { what: "npc", other: "guide", .. }));
        assert!(matches!(err("[npcs]\nmiller 2 2 woodcutter M").kind, LevelErrorKind::Overlap { what: "npc", other: "moonpetal", .. }));
        assert!(matches!(
            err("[npcs]\na 3 0 woodcutter A\nb 3 0 surveyor B").kind,
            LevelErrorKind::Overlap { what: "npc", other: "npc", .. }
        ));
        assert!(matches!(err("[creatures]\nhare 0 0").kind, LevelErrorKind::Overlap { what: "creature", other: "player start", .. }));
        assert!(matches!(err("[creatures]\nhare 2 2").kind, LevelErrorKind::Overlap { what: "creature", other: "moonpetal", .. }));
        assert!(matches!(
            err("[npcs]\nmiller 3 0 woodcutter M\n[creatures]\nhare 3 0").kind,
            LevelErrorKind::Overlap { what: "creature", other: "npc", .. }
        ));
        assert!(matches!(
            err("[creatures]\na 1 3\nb 1 3").kind,
            LevelErrorKind::Overlap { what: "creature", other: "creature", .. }
        ));
        assert!(matches!(
            err("[npcs]\nmiller 3 0 woodcutter M\n[places]\nmill 3 0 Mill").kind,
            LevelErrorKind::Overlap { what: "place", other: "npc", .. }
        ));
        assert!(matches!(
            err("[places]\na 4 2 A\nb 4 2 B").kind,
            LevelErrorKind::Overlap { what: "place", other: "place", .. }
        ));
    }

    #[test]
    fn a_place_may_sit_on_a_moonpetal_and_a_creature_may_stand_on_a_place() {
        // Walking over a moonpetal's tile is harmless, and a place is just a spot on the ground.
        assert!(Level::parse(&with("[places]\nmill 2 2 Mill")).is_ok());
        assert!(Level::parse(&with("[places]\nmill 4 2 Mill\n[creatures]\nhare 4 2")).is_ok());
    }

    #[test]
    fn a_duplicate_new_section_is_refused() {
        let e = err("[places]\nmill 4 2 Mill\n[places]\nmill2 3 2 Mill");
        assert_eq!(e.kind, LevelErrorKind::DuplicateSection("places"));
    }

    #[test]
    fn unknown_section_message_lists_the_new_sections() {
        let e = err("[monsters]\nx");
        let text = e.to_string();
        assert!(text.contains("[npcs]") && text.contains("[places]") && text.contains("[creatures]"), "{text}");
    }

    // ---- houses, plots, exits, and regions ----

    /// A 12 x 8 open map with a guide, so houses have room. `x y` coordinates count up from the
    /// bottom row.
    fn open_level(extra: &str) -> String {
        format!(
            "[map]\n{}\n[player]\n0 0\n[guide]\n11 0\n[moonpetals]\n0 7\n1 7\n2 7\n{extra}",
            vec!["............"; 8].join("\n")
        )
    }

    fn open_err(extra: &str) -> LevelError { Level::parse(&open_level(extra)).unwrap_err() }

    #[test]
    fn a_house_is_stamped_onto_the_map_and_found_by_house_at() {
        let level = Level::parse(&open_level("[houses]\nhut 3 2 5 4 south The Hut\n")).unwrap();
        assert_eq!(level.houses.len(), 1);
        assert_eq!(level.houses[0].name, "The Hut");
        assert_eq!(level.map.tile_at(TilePos::new(5, 2)), Ok(Tile::Door), "the middle of the south side");
        assert_eq!(level.map.tile_at(TilePos::new(3, 2)), Ok(Tile::Wall));
        assert_eq!(level.map.tile_at(TilePos::new(5, 3)), Ok(Tile::Floor));
        assert!(level.map.can_walk_to(TilePos::new(5, 3)));
        assert!(!level.map.can_walk_to(TilePos::new(3, 3)));
        assert!(level.house_at(TilePos::new(5, 3)).is_some());
        assert!(level.house_at(TilePos::new(5, 2)).is_some(), "the doorway counts as inside");
        assert!(level.house_at(TilePos::new(5, 1)).is_none(), "outside the door");
        assert!(level.house_at(TilePos::new(3, 3)).is_none(), "a wall");
        // The player can walk from the start into the house through its door.
        assert!(level.map.is_reachable(level.player_start, TilePos::new(5, 3)));
    }

    #[test]
    fn house_lines_must_be_well_formed_and_errors_name_the_line() {
        for (line, what) in [
            ("hut 3 2 5 4 south", "house"),
            ("hut 3 2 5 4 up The Hut", "door side"),
            ("hut x 2 5 4 south The Hut", "house"),
            ("hut 3 2 5 four south The Hut", "house"),
            ("hut 3 2 5 south The Hut", "house"),
        ] {
            let e = open_err(&format!("[houses]\n{line}\n"));
            assert!(matches!(e.kind, LevelErrorKind::Malformed { .. }), "{line}: {e}");
            assert!(e.to_string().contains(what), "{line}: {e}");
            assert!(e.line.is_some());
        }
        let e = open_err("[houses]\nBad-Id 3 2 5 4 south The Hut\n");
        assert!(matches!(e.kind, LevelErrorKind::InvalidId { what: "house", .. }));
    }

    #[test]
    fn houses_that_do_not_fit_or_overlap_are_refused() {
        let e = open_err("[houses]\nhut 9 2 5 4 south The Hut\n");
        assert!(matches!(e.kind, LevelErrorKind::Homestead(HomesteadError::HouseOffMap { .. })), "{e}");
        assert_eq!(e.line, Some(open_level("").lines().count() + 2), "points at the house line, below its [houses] header");
        let e = open_err("[houses]\nhut 3 2 5 4 south A\nshed 5 3 4 4 south B\n");
        assert!(matches!(e.kind, LevelErrorKind::Homestead(HomesteadError::HouseOverlap { .. })), "{e}");
        let e = open_err("[houses]\nhut 3 2 2 4 south A\n");
        assert!(matches!(e.kind, LevelErrorKind::Homestead(HomesteadError::HouseSize { .. })), "{e}");
    }

    #[test]
    fn a_house_whose_door_faces_the_map_edge_is_refused() {
        let e = open_err("[houses]\nhut 3 0 5 4 south A\n");
        assert!(matches!(e.kind, LevelErrorKind::Homestead(HomesteadError::DoorBlocked { .. })), "{e}");
    }

    #[test]
    fn houses_only_go_on_open_ground_not_trees_or_water() {
        let with_tree = open_level("[houses]\nhut 3 2 5 4 south A\n").replacen("............", "......T.....", 3);
        let e = Level::parse(&with_tree).unwrap_err();
        assert!(matches!(e.kind, LevelErrorKind::Homestead(HomesteadError::HouseBlocked { .. })), "{e}");
    }

    #[test]
    fn people_may_stand_inside_a_house_but_not_in_its_doorway() {
        let inside = "[houses]\nhut 3 2 5 4 south The Hut\n[npcs]\nmiller 5 3 woodcutter Old Miller\n";
        let level = Level::parse(&open_level(inside)).unwrap();
        assert_eq!(level.npcs[0].tile, TilePos::new(5, 3));
        let doorway = "[houses]\nhut 3 2 5 4 south The Hut\n[npcs]\nmiller 5 2 woodcutter Old Miller\n";
        let e = open_err(doorway);
        assert!(matches!(e.kind, LevelErrorKind::Overlap { what: "npc", other: "doorway", .. }), "{e}");
        let creature = "[houses]\nhut 3 2 5 4 south The Hut\n[creatures]\nmeadow_hare 5 2\n";
        let e = open_err(creature);
        assert!(matches!(e.kind, LevelErrorKind::Overlap { what: "creature", other: "doorway", .. }), "{e}");
        // Nothing may be drawn into a wall either.
        let walled = "[houses]\nhut 3 2 5 4 south The Hut\n[npcs]\nmiller 3 3 woodcutter Old Miller\n";
        assert!(matches!(open_err(walled).kind, LevelErrorKind::NotWalkable { what: "npc", found: Tile::Wall, .. }));
    }

    #[test]
    fn a_plot_needs_its_house_and_something_growing() {
        let house = "[houses]\nhut 3 2 5 4 south The Hut\n";
        let e = open_err(&format!("{house}[plots]\nfarm 2 1 8 6 The Farm\n"));
        assert!(matches!(e.kind, LevelErrorKind::Homestead(HomesteadError::PlotWithoutCrops { .. })), "{e}");
        let crops = open_level(&format!("{house}[plots]\nfarm 2 1 8 6 The Farm\n")).replacen("............", "..g.........", 1);
        // The first map row is the top (y = 7), outside the plot, so the plot is still bare.
        assert!(Level::parse(&crops).is_err());
        // A garden bed on y = 1, inside the plot (and not in the way of the door at (5, 2)).
        let rows: Vec<&str> = vec!["............"; 8];
        let mut rows: Vec<String> = rows.iter().map(|r| r.to_string()).collect();
        rows[6].replace_range(8..9, "g"); // text row 6 is y = 1
        let text = format!("[map]\n{}\n[player]\n0 0\n[guide]\n11 0\n[moonpetals]\n0 7\n1 7\n2 7\n{house}[plots]\nfarm 2 1 8 6 The Farm\n", rows.join("\n"));
        let level = Level::parse(&text).unwrap();
        assert_eq!(level.plots.len(), 1);
        assert_eq!(level.plots[0].name, "The Farm");
        let e = Level::parse(&text.replace("farm 2 1 8 6", "farm 2 1 8 6")).map(|_| ());
        assert!(e.is_ok());
    }

    #[test]
    fn plot_lines_must_be_well_formed() {
        for line in ["farm 2 1 8 6", "farm 2 1 8 six The Farm", "farm 2 1 8 The Farm"] {
            let e = open_err(&format!("[plots]\n{line}\n"));
            assert!(matches!(e.kind, LevelErrorKind::Malformed { .. }), "{line}: {e}");
        }
        assert!(matches!(open_err("[plots]\nFarm 2 1 8 6 The Farm\n").kind, LevelErrorKind::InvalidId { what: "plot", .. }));
    }

    #[test]
    fn exits_parse_validate_and_are_found_by_exit_at() {
        let level = Level::parse(&open_level("[exits]\n11 4 ROAD_EAST 2 9 To the road\n")).unwrap();
        assert_eq!(level.exits.len(), 1);
        let exit = level.exit_at(TilePos::new(11, 4)).expect("exit");
        assert_eq!((exit.region.as_str(), exit.arrive, exit.label.as_str()), ("ROAD_EAST", TilePos::new(2, 9), "To the road"));
        assert!(level.exit_at(TilePos::new(10, 4)).is_none());
        for line in ["11 4 ROAD_EAST 2 9", "11 4 ROAD_EAST 2 x Label", "11 4 ROAD_EAST 2 Label", "x 4 ROAD_EAST 2 9 L"] {
            let e = open_err(&format!("[exits]\n{line}\n"));
            assert!(matches!(e.kind, LevelErrorKind::Malformed { .. }), "{line}: {e}");
        }
        let e = open_err("[exits]\n11 4 road_east 2 9 To the road\n");
        assert_eq!(e.kind, LevelErrorKind::InvalidLocationId("road_east".into()));
        assert!(e.to_string().contains("capital letters"));
    }

    #[test]
    fn exits_need_walkable_reachable_unique_unshared_tiles() {
        let e = open_err("[exits]\n12 4 ROAD_EAST 2 9 L\n");
        assert!(matches!(e.kind, LevelErrorKind::OffMap { what: "exit", .. }), "{e}");
        let e = open_err("[exits]\n11 4 ROAD_EAST 2 9 A\n11 4 ROAD_EAST 2 9 B\n");
        assert!(matches!(e.kind, LevelErrorKind::Duplicate { what: "exit", .. }), "{e}");
        let e = open_err("[exits]\n11 0 ROAD_EAST 2 9 On the guide\n");
        assert!(matches!(e.kind, LevelErrorKind::Overlap { what: "exit", other: "guide", .. }), "{e}");
        let e = open_err("[exits]\n0 0 ROAD_EAST 2 9 On the start\n");
        assert!(matches!(e.kind, LevelErrorKind::Overlap { what: "exit", other: "player start", .. }), "{e}");
        let e = open_err("[houses]\nhut 3 2 5 4 south A\n[exits]\n3 3 ROAD_EAST 2 9 In a wall\n");
        assert!(matches!(e.kind, LevelErrorKind::NotWalkable { what: "exit", .. }), "{e}");
        let walled = "[map]\n..T.\n..T.\n\n[player]\n0 0\n[exits]\n3 0 ROAD_EAST 2 9 Cut off\n";
        assert!(matches!(Level::parse(walled).unwrap_err().kind, LevelErrorKind::Unreachable { what: "exit", .. }));
    }

    #[test]
    fn behaviors_parse_for_the_guide_and_for_listed_villagers() {
        let level = Level::parse(&with("[npcs]\nmiller 3 0 woodcutter Miller\n\n[behaviors]\n; who roams\nguide stay emotes wave\nmiller wander 2 pause 2 4\n")).unwrap();
        assert_eq!(level.behaviors.len(), 2);
        assert_eq!(level.behavior_of("guide").emotes.len(), 1);
        assert_eq!(level.behavior_of("miller").radius(), Some(2));
        assert_eq!(level.behavior_of("miller").pause, (2, 4));
    }

    #[test]
    fn anyone_not_listed_stands_still() {
        let level = Level::parse(&with("[npcs]\nmiller 3 0 woodcutter Miller\n")).unwrap();
        assert!(level.behaviors.is_empty());
        assert_eq!(level.behavior_of("miller"), Behavior::default());
        assert_eq!(level.behavior_of("nobody"), Behavior::default());
    }

    #[test]
    fn bad_behavior_lines_are_refused_with_their_line_number() {
        for text in ["miller", "miller dance", "miller wander", "miller wander 99", "miller stay pause 9 1", "miller stay emotes jump"] {
            let e = err(&format!("[npcs]\nmiller 3 0 woodcutter M\n[behaviors]\n{text}"));
            assert!(matches!(e.kind, LevelErrorKind::BadBehavior(_)), "{text}: {e}");
            assert!(e.line.is_some(), "{text}");
            assert!(e.to_string().contains("bad behavior"), "{text}");
        }
        assert!(matches!(err("[npcs]\nmiller 3 0 woodcutter M\n[behaviors]\nBad-Id stay").kind, LevelErrorKind::InvalidId { what: "behavior", .. }));
    }

    #[test]
    fn a_behavior_needs_a_real_person_and_only_one_line() {
        let e = err("[behaviors]\nghost stay");
        assert_eq!(e.kind, LevelErrorKind::UnknownBehaviorTarget("ghost".into()));
        assert!(e.to_string().contains("neither the guide nor listed in [npcs]"));
        let twice = err("[npcs]\nmiller 3 0 woodcutter M\n[behaviors]\nmiller stay\nmiller wander 2");
        assert_eq!(twice.kind, LevelErrorKind::DuplicateId { what: "behavior", id: "miller".into() });
        assert_eq!(twice.line, Some(twice.line.unwrap()));
    }

    #[test]
    fn a_wanderer_with_nowhere_to_go_is_refused() {
        // Waymarks on every tile around the miller leave him no room to move.
        let e = err("[npcs]\nmiller 3 0 woodcutter M\n[places]\na 2 0 A\nb 4 0 B\nc 3 1 C\n[behaviors]\nmiller wander 1");
        assert_eq!(e.kind, LevelErrorKind::WanderStuck { id: "miller".into(), tile: TilePos::new(3, 0), radius: 1 });
        assert!(e.to_string().contains("cannot move anywhere within 1 tiles"));
        // A wider range reaches open ground, and standing still never needs room.
        assert!(Level::parse(&with("[npcs]\nmiller 3 0 woodcutter M\n[places]\na 2 0 A\nb 4 0 B\nc 3 1 C\n[behaviors]\nmiller wander 3")).is_ok());
        assert!(Level::parse(&with("[npcs]\nmiller 3 0 woodcutter M\n[places]\na 2 0 A\nb 4 0 B\nc 3 1 C\n[behaviors]\nmiller stay")).is_ok());
    }

    #[test]
    fn keep_out_tiles_cover_everything_the_player_must_be_able_to_use() {
        let level = Level::parse(&with("herb 1 0 3 1\n[npcs]\nmiller 3 0 woodcutter M\n[places]\nmill 2 0 The Mill\n[creatures]\nmeadow_hare 4 0\n")).unwrap();
        let keep = level.npc_keep_out();
        for tile in [TilePos::new(2, 0), TilePos::new(4, 0), TilePos::new(1, 0), TilePos::new(0, 2), TilePos::new(2, 2), TilePos::new(3, 3)] {
            assert!(keep.contains(&tile), "{tile:?}");
        }
        assert!(!keep.contains(&TilePos::new(5, 3)), "a tree is not walkable anyway and is not listed twice");
    }

    #[test]
    fn a_level_without_a_guide_is_a_region_and_may_not_hold_story_content() {
        let region = "[map]\n......\n......\n\n[player]\n0 0\n";
        let level = Level::parse(region).unwrap();
        assert!(!level.is_story_level());
        assert!(level.moonpetals.is_empty(), "regions need no moonpetals");
        for (section, entry) in [("npcs", "miller 3 0 woodcutter Old Miller"), ("behaviors", "miller stay"), ("places", "mill 3 0 The Mill"), ("creatures", "meadow_hare 3 0")] {
            let e = Level::parse(&format!("{region}[{section}]\n{entry}\n")).unwrap_err();
            assert_eq!(e.kind, LevelErrorKind::RegionHasStory(match section { "npcs" => "npcs", "behaviors" => "behaviors", "places" => "places", _ => "creatures" }));
            assert!(e.line.is_some());
            assert!(e.to_string().contains("needs a [guide]"));
        }
        // Resources, signs, houses, and exits are fine in a region.
        let rich = format!("{region}[nodes]\nherb 4 1 3 1\n[exits]\n5 0 ROAD_EAST 1 1 Onward\n");
        assert!(Level::parse(&rich).is_ok());
    }

    #[test]
    fn story_levels_still_need_enough_moonpetals() {
        let e = Level::parse("[map]\n......\n......\n\n[player]\n0 0\n[guide]\n5 0\n").unwrap_err();
        assert!(matches!(e.kind, LevelErrorKind::NotEnoughMoonpetals { have: 0, .. }), "{e}");
    }

    #[test]
    fn the_shipped_bluebrook_has_plots_houses_gardens_vineyards_and_a_road_east() {
        let level = Level::parse(crate::village::BLUEBROOK_MAP).unwrap();
        assert!(level.plots.len() >= 3, "several plots");
        assert!(level.houses.len() >= 3, "a house on each plot");
        for plot in &level.plots {
            let holds: Vec<_> = level.houses.iter().filter(|h| plot.holds(h)).collect();
            assert!(!holds.is_empty(), "plot {} has no house", plot.id);
        }
        let count = |wanted: Tile| {
            (0..level.map.height() as i32)
                .flat_map(|y| (0..level.map.width() as i32).map(move |x| TilePos::new(x, y)))
                .filter(|t| level.map.tile_at(*t) == Ok(wanted))
                .count()
        };
        assert!(count(Tile::Garden) >= 5, "gardens");
        assert!(count(Tile::Vine) >= 5, "a vineyard");
        assert!(level.exits.is_empty(), "the road east is joined by walking: the overworld places it against the east edge");
        let east = TilePos::new(level.map.width() as i32 - 1, 20);
        assert!(level.map.can_walk_to(east) && level.map.is_reachable(level.player_start, east), "the road runs off the east edge");
        // The apothecary now keeps shop inside her house.
        let pell = level.npcs.iter().find(|n| n.id == "apothecary").expect("Mistress Pell");
        assert!(level.house_at(pell.tile).is_some(), "Pell stands inside her house");
        // You can walk in through every door.
        for house in &level.houses {
            assert!(level.map.is_reachable(level.player_start, house.door_tile()), "{}", house.id);
        }
    }

    #[test]
    fn the_shipped_road_region_is_a_region_with_a_wooden_bridge_and_a_way_back() {
        let level = Level::parse(crate::village::BUILTIN_REGION_MAPS[0].1).unwrap();
        assert!(!level.is_story_level());
        assert!(level.exits.is_empty(), "the road is joined to Bluebrook by walking, not by an exit");
        let west = TilePos::new(0, 20);
        assert!(level.map.can_walk_to(west) && level.map.is_reachable(level.player_start, west), "the road runs off the west edge");
        assert!(!level.houses.is_empty() && !level.plots.is_empty());
    }
}
