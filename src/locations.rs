//! The location register: which places exist in Ozbound, where they lead, and which of them
//! can be played yet. It is the data form of `LOCATIONS.md`, read from
//! `assets/world/locations.world`.
//!
//! Pure Rust (no Bevy). The file is a list of `[location ID]` sections with `key = value`
//! fields (the shared reader in `textdata.rs`):
//!
//! ```text
//! [location MUNCHKIN_LANDS]
//! name     = Land of the Munchkins
//! source   = NOVEL_1900                 # NOVEL_1900, LATER_OZ, GAME_ORIGINAL, ADAPTATION
//! priority = P0                         # P0 (first journey) .. P3 (later-book expansion)
//! summary  = One line on what the place is for.
//! exits    = YELLOW_BRICK_ROAD_EAST     # comma-separated location ids (optional)
//! map      = bluebrook.map              # a file in assets/maps/ (optional: playable if present)
//! music    = bluebrook.ogg              # a file in assets/bgm/ (optional)
//! theme    = village                    # built-in fallback theme (optional)
//! start    = yes                        # exactly one playable location starts the game
//! ```
//!
//! Rules (each is a tested error with a line number where one exists): ids are unique and
//! written like `LOCATIONS.md` writes them; every exit names a location in the file and none
//! leads back to its own place; a place with a `map` is playable and a place without one is
//! planned only; `music` and `theme` need a `map`; exactly one playable place has
//! `start = yes`; and a `LATER_OZ` place is never also labelled `NOVEL_1900`, so later-book
//! material cannot pass as the 1900 novel.
//!
//! [`check_world`] then checks the levels the maps load into: every level exit leads to a
//! playable place this place declares an exit to, arriving on a walkable, reachable tile
//! that is not itself an exit, and only the starting region carries story content.

use std::collections::BTreeSet;
use std::fmt;

use crate::level::Level;
use crate::textdata::{check_id, is_valid_location_id, read_lines, ContentError, Item};
use crate::world::TilePos;

/// The shipped register.
pub const DEFAULT_LOCATIONS: &str = include_str!("../assets/world/locations.world");
pub const LOCATIONS_PATH: &str = "assets/world/locations.world";
/// Where `map = ...` file names are looked up.
pub const MAPS_DIR: &str = "assets/maps";

/// Where a place comes from, as labelled in `LOCATIONS.md`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Source {
    Novel1900,
    LaterOz,
    GameOriginal,
    Adaptation,
}

impl Source {
    pub fn from_name(name: &str) -> Option<Self> {
        Some(match name {
            "NOVEL_1900" => Source::Novel1900,
            "LATER_OZ" => Source::LaterOz,
            "GAME_ORIGINAL" => Source::GameOriginal,
            "ADAPTATION" => Source::Adaptation,
            _ => return None,
        })
    }

    pub fn name(self) -> &'static str {
        match self {
            Source::Novel1900 => "NOVEL_1900",
            Source::LaterOz => "LATER_OZ",
            Source::GameOriginal => "GAME_ORIGINAL",
            Source::Adaptation => "ADAPTATION",
        }
    }
}

/// How soon a place is needed. `P0` is the first playable journey; `P3` is expansion.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Priority {
    P0,
    P1,
    P2,
    P3,
}

impl Priority {
    pub fn from_name(name: &str) -> Option<Self> {
        Some(match name {
            "P0" => Priority::P0,
            "P1" => Priority::P1,
            "P2" => Priority::P2,
            "P3" => Priority::P3,
            _ => return None,
        })
    }
}

/// One place in the register.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Location {
    pub id: String,
    pub name: String,
    /// One or more labels, for example `NOVEL_1900 / ADAPTATION`; never empty.
    pub sources: Vec<Source>,
    pub priority: Priority,
    pub summary: String,
    pub exits: Vec<String>,
    /// File name under `assets/maps/`; `Some` means the place can be played.
    pub map: Option<String>,
    /// File name under `assets/bgm/` (see `music.rs`).
    pub music: Option<String>,
    /// Built-in theme id (see `music.rs`).
    pub theme: Option<String>,
    pub start: bool,
}

impl Location {
    pub fn is_playable(&self) -> bool { self.map.is_some() }
}

/// The parsed, validated register.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Locations {
    locations: Vec<Location>,
}

fn err(line: usize, message: impl Into<String>) -> ContentError {
    ContentError::at(line, message)
}

/// A plain file name with the given extension: no folders, no leading dot.
fn is_plain_file(name: &str, extension: &str) -> bool {
    !name.is_empty()
        && !name.starts_with('.')
        && !name.contains(['/', '\\', ':', '\0'])
        && name.rsplit_once('.').is_some_and(|(stem, ext)| !stem.is_empty() && ext == extension)
}

impl Locations {
    pub fn parse(text: &str) -> Result<Self, ContentError> {
        struct Draft {
            line: usize,
            id: String,
            fields: Vec<(usize, String, String)>,
        }
        let mut drafts: Vec<Draft> = Vec::new();
        for line in read_lines(text)? {
            match line.item {
                Item::Section { kind, arg } => {
                    if kind != "location" {
                        return Err(err(line.number, format!("unknown section [{kind}] (only [location ID] sections are used here)")));
                    }
                    if !is_valid_location_id(&arg) {
                        return Err(err(
                            line.number,
                            format!("location id `{arg}` must be capital letters, digits, and underscores, starting with a letter"),
                        ));
                    }
                    if drafts.iter().any(|d| d.id == arg) {
                        return Err(err(line.number, format!("location `{arg}` is listed twice")));
                    }
                    drafts.push(Draft { line: line.number, id: arg, fields: Vec::new() });
                }
                Item::Field { key, value } => match drafts.last_mut() {
                    Some(draft) => draft.fields.push((line.number, key, value)),
                    None => return Err(err(line.number, "a field must come after a [location ID] header")),
                },
            }
        }
        if drafts.is_empty() {
            return Err(err(0, "the register lists no locations"));
        }

        let mut locations = Vec::new();
        for draft in &drafts {
            locations.push(Self::build(draft.line, &draft.id, &draft.fields)?);
        }

        let book = Self { locations };
        book.check_graph(&drafts.iter().map(|d| (d.id.clone(), d.line)).collect::<Vec<_>>())?;
        Ok(book)
    }

    fn build(header_line: usize, id: &str, fields: &[(usize, String, String)]) -> Result<Location, ContentError> {
        let mut name = None;
        let mut sources = None;
        let mut priority = None;
        let mut summary = None;
        let mut exits: Option<Vec<String>> = None;
        let mut map = None;
        let mut music = None;
        let mut theme = None;
        let mut start = None;
        let mut seen: Vec<&str> = Vec::new();
        for (line, key, value) in fields {
            let line = *line;
            if seen.contains(&key.as_str()) {
                return Err(err(line, format!("field `{key}` appears twice in `{id}`")));
            }
            seen.push(key);
            if value.is_empty() {
                return Err(err(line, format!("field `{key}` of `{id}` is empty")));
            }
            match key.as_str() {
                "name" => name = Some(value.clone()),
                "summary" => summary = Some(value.clone()),
                "source" => {
                    let mut parsed = Vec::new();
                    for part in value.split('/').map(str::trim) {
                        let source = Source::from_name(part).ok_or_else(|| {
                            err(line, format!("unknown source `{part}` (use NOVEL_1900, LATER_OZ, GAME_ORIGINAL, or ADAPTATION)"))
                        })?;
                        if parsed.contains(&source) {
                            return Err(err(line, format!("source `{part}` is listed twice")));
                        }
                        parsed.push(source);
                    }
                    if parsed.contains(&Source::LaterOz) && parsed.len() > 1 {
                        return Err(err(line, format!("`{id}` is LATER_OZ, so it cannot also be labelled as part of the 1900 novel or the game's own invention")));
                    }
                    sources = Some(parsed);
                }
                "priority" => {
                    priority = Some(Priority::from_name(value).ok_or_else(|| err(line, format!("unknown priority `{value}` (use P0, P1, P2, or P3)")))?);
                }
                "exits" => {
                    let mut list = Vec::new();
                    for part in value.split(',').map(str::trim) {
                        if !is_valid_location_id(part) {
                            return Err(err(line, format!("exit `{part}` of `{id}` is not a location id")));
                        }
                        if part == id {
                            return Err(err(line, format!("`{id}` lists itself as an exit")));
                        }
                        if list.iter().any(|p| p == part) {
                            return Err(err(line, format!("exit `{part}` of `{id}` is listed twice")));
                        }
                        list.push(part.to_string());
                    }
                    exits = Some(list);
                }
                "map" => {
                    if !is_plain_file(value, "map") {
                        return Err(err(line, format!("map `{value}` must be a plain file name ending in .map (it is looked up in {MAPS_DIR})")));
                    }
                    map = Some(value.clone());
                }
                "music" => {
                    if !crate::music::is_valid_track_name(value) {
                        return Err(err(line, format!("music `{value}` must be a plain file name ending in .ogg, .wav, or .mp4")));
                    }
                    music = Some(value.clone());
                }
                "theme" => {
                    if !crate::music::THEMES.contains(&value.as_str()) {
                        return Err(err(line, format!("unknown theme `{value}` (use {})", crate::music::THEMES.join(" or "))));
                    }
                    theme = Some(value.clone());
                }
                "start" => {
                    start = Some(match value.as_str() {
                        "yes" => true,
                        "no" => false,
                        other => return Err(err(line, format!("`start` must be yes or no, found `{other}`"))),
                    });
                }
                other => return Err(err(line, format!("unknown field `{other}` in `{id}`"))),
            }
        }
        let missing = |what: &str| err(header_line, format!("location `{id}` needs a `{what}` field"));
        check_id(header_line, "location", &id.to_ascii_lowercase()).ok();
        let location = Location {
            id: id.to_string(),
            name: name.ok_or_else(|| missing("name"))?,
            sources: sources.ok_or_else(|| missing("source"))?,
            priority: priority.ok_or_else(|| missing("priority"))?,
            summary: summary.ok_or_else(|| missing("summary"))?,
            exits: exits.unwrap_or_default(),
            map,
            music,
            theme,
            start: start.unwrap_or(false),
        };
        if (location.music.is_some() || location.theme.is_some()) && location.map.is_none() {
            return Err(err(header_line, format!("`{id}` has music but no map; only playable places play music")));
        }
        if location.start && location.map.is_none() {
            return Err(err(header_line, format!("`{id}` is marked as the start but has no map")));
        }
        Ok(location)
    }

    fn check_graph(&self, headers: &[(String, usize)]) -> Result<(), ContentError> {
        let ids: BTreeSet<&str> = self.locations.iter().map(|l| l.id.as_str()).collect();
        for (location, (_, line)) in self.locations.iter().zip(headers) {
            for exit in &location.exits {
                if !ids.contains(exit.as_str()) {
                    return Err(err(*line, format!("`{}` has an exit to `{exit}`, which is not in the register", location.id)));
                }
            }
        }
        let starts: Vec<&Location> = self.locations.iter().filter(|l| l.start).collect();
        match starts.as_slice() {
            [_] => {}
            [] => return Err(err(0, "no location is marked `start = yes`")),
            many => {
                let names: Vec<&str> = many.iter().map(|l| l.id.as_str()).collect();
                return Err(err(0, format!("only one location can be the start, found {}", names.join(", "))));
            }
        }
        let mut maps: BTreeSet<&str> = BTreeSet::new();
        for (location, (_, line)) in self.locations.iter().zip(headers) {
            if let Some(map) = &location.map {
                if !maps.insert(map) {
                    return Err(err(*line, format!("map `{map}` is used by more than one location")));
                }
            }
        }
        Ok(())
    }

    pub fn iter(&self) -> impl Iterator<Item = &Location> { self.locations.iter() }

    pub fn len(&self) -> usize { self.locations.len() }

    pub fn is_empty(&self) -> bool { self.locations.is_empty() }

    pub fn get(&self, id: &str) -> Option<&Location> { self.locations.iter().find(|l| l.id == id) }

    /// The playable place the game starts in.
    pub fn start(&self) -> &Location {
        self.locations.iter().find(|l| l.start).expect("parse guarantees exactly one start")
    }

    /// Places that can be played (they have a map).
    pub fn playable(&self) -> impl Iterator<Item = &Location> { self.locations.iter().filter(|l| l.is_playable()) }

    /// True when `to` is a playable place that `from` declares an exit to.
    pub fn can_travel(&self, from: &str, to: &str) -> bool {
        self.get(from).is_some_and(|f| f.exits.iter().any(|e| e == to)) && self.get(to).is_some_and(Location::is_playable)
    }

    /// Every place reachable from `from` by following exits (including `from`).
    pub fn reachable_from(&self, from: &str) -> BTreeSet<String> {
        let mut seen: BTreeSet<String> = BTreeSet::new();
        let mut queue = vec![from.to_string()];
        while let Some(id) = queue.pop() {
            if !seen.insert(id.clone()) {
                continue;
            }
            if let Some(location) = self.get(&id) {
                queue.extend(location.exits.iter().cloned());
            }
        }
        seen
    }
}

/// A problem found by [`check_world`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorldError {
    pub location: String,
    pub message: String,
}

impl fmt::Display for WorldError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "location `{}`: {}", self.location, self.message)
    }
}

impl std::error::Error for WorldError {}

/// Checks the loaded levels against the register. `level_of` returns the parsed level for a
/// playable location's id (the loader reads `assets/maps/<map>`).
pub fn check_world<'a>(book: &Locations, level_of: impl Fn(&str) -> Option<&'a Level>) -> Result<(), WorldError> {
    let fail = |id: &str, message: String| WorldError { location: id.to_string(), message };
    for location in book.playable() {
        let id = location.id.as_str();
        let Some(level) = level_of(id) else {
            return Err(fail(id, format!("its map {} was not loaded", location.map.as_deref().unwrap_or("?"))));
        };
        if !location.start && level.is_story_level() {
            return Err(fail(id, "only the starting region may have a [guide]; story content is validated for one region".into()));
        }
        if location.start && !level.is_story_level() {
            return Err(fail(id, "the starting region needs a [guide]".into()));
        }
        for exit in &level.exits {
            let here = format!("the exit at ({}, {}) \"{}\"", exit.tile.x, exit.tile.y, exit.label);
            let Some(target) = book.get(&exit.region) else {
                return Err(fail(id, format!("{here} leads to `{}`, which is not in the register", exit.region)));
            };
            if target.id == location.id {
                return Err(fail(id, format!("{here} leads back to the region it is in")));
            }
            if !location.exits.iter().any(|e| *e == target.id) {
                return Err(fail(id, format!("{here} leads to `{}`, but the register does not list it among this place's exits", target.id)));
            }
            if !target.is_playable() {
                return Err(fail(id, format!("{here} leads to `{}`, which has no map yet", target.id)));
            }
            let Some(target_level) = level_of(&target.id) else {
                return Err(fail(id, format!("{here} leads to `{}`, whose map was not loaded", target.id)));
            };
            check_arrival(id, &here, exit.arrive, target.id.as_str(), target_level)?;
        }
    }
    Ok(())
}

fn check_arrival(id: &str, here: &str, arrive: TilePos, target_id: &str, target: &Level) -> Result<(), WorldError> {
    let fail = |message: String| WorldError { location: id.to_string(), message };
    if !target.map.can_walk_to(arrive) {
        return Err(fail(format!("{here} arrives at ({}, {}) in `{target_id}`, which is off the map or cannot be walked on", arrive.x, arrive.y)));
    }
    if !target.map.is_reachable(target.player_start, arrive) {
        return Err(fail(format!("{here} arrives at ({}, {}) in `{target_id}`, which the player cannot walk to from there", arrive.x, arrive.y)));
    }
    if target.exit_at(arrive).is_some() {
        return Err(fail(format!("{here} arrives on an exit tile of `{target_id}`, which would send the player straight back")));
    }
    if target.house_at(arrive).is_some() {
        return Err(fail(format!("{here} arrives inside a house in `{target_id}`")));
    }
    if target.npcs.iter().any(|n| n.tile == arrive) || target.guide == Some(arrive) || target.creatures.iter().any(|c| c.tile == arrive) {
        return Err(fail(format!("{here} arrives on a tile where someone is standing in `{target_id}`")));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const TWO: &str = "\
[location HOME]
name = Home
source = GAME_ORIGINAL
priority = P0
summary = Where it starts.
exits = ROAD
map = home.map
music = home.ogg
theme = village
start = yes

[location ROAD]
name = The Road
source = NOVEL_1900 / ADAPTATION
priority = P0
summary = A road.
exits = HOME, FAR
map = road.map
theme = road

[location FAR]
name = Far Away
source = LATER_OZ
priority = P3
summary = Not built yet.
";

    fn parse(text: &str) -> Result<Locations, ContentError> { Locations::parse(text) }
    fn bad(text: &str) -> ContentError { Locations::parse(text).unwrap_err() }
    fn replace(from: &str, to: &str) -> String {
        assert!(TWO.contains(from), "test bug: {from:?}");
        TWO.replacen(from, to, 1)
    }

    #[test]
    fn parses_places_with_all_their_fields() {
        let book = parse(TWO).unwrap();
        assert_eq!(book.len(), 3);
        let home = book.get("HOME").unwrap();
        assert_eq!(home.name, "Home");
        assert_eq!(home.sources, vec![Source::GameOriginal]);
        assert_eq!(home.priority, Priority::P0);
        assert_eq!(home.exits, vec!["ROAD"]);
        assert_eq!(home.map.as_deref(), Some("home.map"));
        assert_eq!(home.music.as_deref(), Some("home.ogg"));
        assert_eq!(home.theme.as_deref(), Some("village"));
        assert!(home.start && home.is_playable());
        let road = book.get("ROAD").unwrap();
        assert_eq!(road.sources, vec![Source::Novel1900, Source::Adaptation]);
        assert_eq!(road.exits, vec!["HOME", "FAR"]);
        assert!(!road.start);
        let far = book.get("FAR").unwrap();
        assert!(!far.is_playable() && far.exits.is_empty() && far.music.is_none());
        assert_eq!(book.start().id, "HOME");
        assert_eq!(book.playable().count(), 2);
        assert_eq!(book.get("NOPE"), None);
    }

    #[test]
    fn travel_needs_a_declared_exit_and_a_playable_destination() {
        let book = parse(TWO).unwrap();
        assert!(book.can_travel("HOME", "ROAD"));
        assert!(book.can_travel("ROAD", "HOME"));
        assert!(!book.can_travel("ROAD", "FAR"), "declared but not playable yet");
        assert!(!book.can_travel("HOME", "FAR"), "not declared");
        assert!(!book.can_travel("NOPE", "HOME"));
        let reach = book.reachable_from("HOME");
        assert_eq!(reach.into_iter().collect::<Vec<_>>(), vec!["FAR", "HOME", "ROAD"]);
        assert_eq!(book.reachable_from("FAR").len(), 1);
    }

    #[test]
    fn comments_blank_lines_crlf_and_case_in_keys_are_fine() {
        let text = TWO.replace('\n', "\r\n").replace("name = Home", "# a comment\r\nNAME = Home");
        assert!(parse(&text).is_ok());
    }

    #[test]
    fn syntax_errors_name_their_line() {
        assert_eq!(bad("name = x\n").line, Some(1));
        let e = bad("[place HOME]\n");
        assert_eq!(e.line, Some(1));
        assert!(e.message.contains("unknown section"));
        let e = bad("[location home]\n");
        assert!(e.message.contains("capital letters"), "{e}");
        assert_eq!(bad(&format!("{TWO}\n[location HOME]\n")).message, "location `HOME` is listed twice");
        let e = bad("# nothing\n");
        assert!(e.message.contains("no locations"));
        let e = bad(&replace("priority = P0\nsummary = Where", "priority = P9\nsummary = Where"));
        assert_eq!(e.line, Some(4));
        assert!(e.message.contains("unknown priority"));
    }

    #[test]
    fn every_required_field_is_required() {
        for field in ["name = Home\n", "source = GAME_ORIGINAL\n", "priority = P0\n", "summary = Where it starts.\n"] {
            let e = bad(&replace(field, ""));
            assert_eq!(e.line, Some(1), "{field}");
            assert!(e.message.contains("needs a"), "{e}");
        }
    }

    #[test]
    fn field_values_are_validated() {
        let cases = [
            ("source = GAME_ORIGINAL", "source = MADE_UP", "unknown source"),
            ("source = GAME_ORIGINAL", "source = NOVEL_1900 / NOVEL_1900", "listed twice"),
            ("exits = ROAD\nmap = home.map", "exits = road\nmap = home.map", "not a location id"),
            ("exits = ROAD\nmap = home.map", "exits = HOME\nmap = home.map", "lists itself"),
            ("exits = ROAD\nmap = home.map", "exits = ROAD, ROAD\nmap = home.map", "listed twice"),
            ("map = home.map", "map = maps/home.map", "plain file name"),
            ("map = home.map", "map = home.txt", "plain file name"),
            ("map = home.map", "map = ../home.map", "plain file name"),
            ("music = home.ogg", "music = home.mp4", "must be a plain file name"),
            ("music = home.ogg", "music = dir/home.ogg", "must be a plain file name"),
            ("theme = village", "theme = polka", "unknown theme"),
            ("start = yes", "start = maybe", "yes or no"),
            ("start = yes", "mood = calm", "unknown field"),
            ("name = Home", "name =", "is empty"),
            ("name = Home", "name = Home\nname = Again", "appears twice"),
        ];
        for (from, to, expected) in cases {
            let e = bad(&replace(from, to));
            assert!(e.message.contains(expected), "{from:?} -> {to:?}: {e}");
            assert!(e.line.is_some(), "{from:?} -> {to:?}: {e}");
        }
    }

    #[test]
    fn later_book_places_cannot_pass_as_the_1900_novel() {
        let e = bad(&replace("source = NOVEL_1900 / ADAPTATION", "source = LATER_OZ / NOVEL_1900"));
        assert!(e.message.contains("LATER_OZ"), "{e}");
        assert!(parse(&replace("source = NOVEL_1900 / ADAPTATION", "source = LATER_OZ")).is_ok());
    }

    #[test]
    fn exits_must_name_places_in_the_register() {
        let e = bad(&replace("exits = HOME, FAR", "exits = HOME, MISSING"));
        assert_eq!(e.line, Some(12), "points at the ROAD header: {e}");
        assert!(e.message.contains("`MISSING`") && e.message.contains("not in the register"));
    }

    #[test]
    fn exactly_one_playable_start_is_required() {
        assert!(bad(&replace("start = yes\n", "")).message.contains("no location is marked"));
        let two = replace("theme = road\n", "theme = road\nstart = yes\n");
        assert!(bad(&two).message.contains("only one location can be the start"));
        let unplayable = replace("priority = P3\nsummary = Not built yet.\n", "priority = P3\nsummary = Not built yet.\nstart = yes\n");
        assert!(bad(&unplayable).message.contains("no map"));
    }

    #[test]
    fn music_needs_a_map_and_two_places_cannot_share_one() {
        let e = bad(&replace("priority = P3\nsummary = Not built yet.\n", "priority = P3\nsummary = Not built yet.\ntheme = road\n"));
        assert!(e.message.contains("music but no map"), "{e}");
        let shared = replace("map = road.map", "map = home.map");
        assert!(bad(&shared).message.contains("more than one location"));
    }

    // ---- check_world ----

    fn level(extra: &str, guide: bool) -> Level {
        let guide = if guide { "[guide]\n5 0\n[moonpetals]\n0 2\n2 2\n3 3\n" } else { "" };
        Level::parse(&format!("[map]\n......\n......\n......\n......\n\n[player]\n0 0\n{guide}{extra}")).unwrap()
    }

    fn world(home_extra: &str, road_extra: &str) -> (Locations, Level, Level) {
        (parse(TWO).unwrap(), level(home_extra, true), level(road_extra, false))
    }

    fn check(book: &Locations, home: &Level, road: &Level) -> Result<(), WorldError> {
        check_world(book, |id| match id {
            "HOME" => Some(home),
            "ROAD" => Some(road),
            _ => None,
        })
    }

    #[test]
    fn matching_levels_pass() {
        let (book, home, road) = world("[exits]\n5 3 ROAD 2 3 To the road\n", "[exits]\n0 3 HOME 4 3 Back home\n");
        assert_eq!(check(&book, &home, &road), Ok(()));
    }

    #[test]
    fn levels_without_exits_pass_too() {
        let (book, home, road) = world("", "");
        assert_eq!(check(&book, &home, &road), Ok(()));
    }

    #[test]
    fn only_the_start_has_a_guide_and_the_start_must_have_one() {
        let (book, _, _) = world("", "");
        let with_guide = level("", true);
        let without = level("", false);
        let e = check(&book, &with_guide, &with_guide).unwrap_err();
        assert_eq!(e.location, "ROAD");
        assert!(e.message.contains("only the starting region"));
        let e = check(&book, &without, &without).unwrap_err();
        assert_eq!(e.location, "HOME");
        assert!(e.message.contains("needs a [guide]"));
    }

    #[test]
    fn exits_must_lead_somewhere_declared_and_playable() {
        let (book, _, road) = world("", "");
        let unknown = level("[exits]\n5 3 NOWHERE 0 3 Mystery\n", true);
        let e = check(&book, &unknown, &road).unwrap_err();
        assert!(e.message.contains("not in the register"), "{e}");
        let to_self = level("[exits]\n5 3 HOME 0 3 Loop\n", true);
        assert!(check(&book, &to_self, &road).unwrap_err().message.contains("leads back to the region it is in"));
        // FAR exists and ROAD declares it, but it has no map.
        let far = level("[exits]\n5 3 FAR 0 0 Beyond\n", false);
        let e = check(&book, &level("", true), &far).unwrap_err();
        assert!(e.message.contains("no map yet"), "{e}");
        // HOME does not declare FAR as an exit at all.
        let undeclared = level("[exits]\n5 3 FAR 0 0 Beyond\n", true);
        assert!(check(&book, &undeclared, &road).unwrap_err().message.contains("does not list it"));
    }

    #[test]
    fn arrival_tiles_must_be_safe() {
        let (book, _, _) = world("", "");
        let road = level("[exits]\n0 3 HOME 4 3 Back\n", false);
        let go = |arrive: &str| check(&book, &level(&format!("[exits]\n5 3 ROAD {arrive} Go\n"), true), &road);
        assert!(go("2 1").is_ok());
        assert!(go("0 3").unwrap_err().message.contains("exit tile"), "arriving on an exit sends you back");
        assert!(go("9 9").unwrap_err().message.contains("off the map"));
        assert!(go("-1 0").unwrap_err().message.contains("off the map"));
    }

    #[test]
    fn arrival_may_not_be_blocked_walled_off_or_inside_a_house() {
        let book = parse(TWO).unwrap();
        let blocked = Level::parse("[map]\n..T...\n..T...\n..T...\n\n[player]\n0 0\n").unwrap();
        let go = |target: &Level, arrive: &str| check(&book, &level(&format!("[exits]\n5 3 ROAD {arrive} Go\n"), true), target);
        assert!(go(&blocked, "2 1").unwrap_err().message.contains("cannot be walked on"));
        assert!(go(&blocked, "4 1").unwrap_err().message.contains("cannot walk to"));
        let housed = level("[houses]\nhut 1 0 3 3 north The Hut\n", false);
        assert!(go(&housed, "2 1").unwrap_err().message.contains("inside a house"));
        assert!(go(&housed, "5 3").is_ok());
    }

    #[test]
    fn a_missing_map_is_reported_by_place() {
        let book = parse(TWO).unwrap();
        let home = level("", true);
        let e = check_world(&book, |id| (id == "HOME").then_some(&home)).unwrap_err();
        assert_eq!(e.location, "ROAD");
        assert!(e.message.contains("road.map"));
        assert!(e.to_string().starts_with("location `ROAD`:"));
    }

    // ---- the shipped register ----

    #[test]
    fn the_shipped_register_parses_and_starts_in_the_land_of_the_munchkins() {
        let book = parse(DEFAULT_LOCATIONS).expect("the shipped locations.world must parse");
        assert_eq!(book.start().id, "MUNCHKIN_LANDS");
        assert_eq!(book.start().map.as_deref(), Some("bluebrook.map"));
        assert!(book.can_travel("MUNCHKIN_LANDS", "YELLOW_BRICK_ROAD_EAST"));
        assert!(book.can_travel("YELLOW_BRICK_ROAD_EAST", "MUNCHKIN_LANDS"));
    }

    /// The first column of LOCATIONS.md's register table, with each place's first source label
    /// and priority.
    fn register_rows() -> Vec<(String, String, String)> {
        let doc = include_str!("../LOCATIONS.md");
        let start = doc.find("## Location register").expect("register heading");
        let end = doc.find("## Location specifications").expect("specifications heading");
        let mut rows = Vec::new();
        for line in doc[start..end].lines() {
            let Some(rest) = line.strip_prefix("  `") else { continue };
            let Some((id, after)) = rest.split_once('`') else { continue };
            if !is_valid_location_id(id) {
                continue;
            }
            let ticks: Vec<&str> = after.split('`').skip(1).step_by(2).collect();
            let priority = after.split_whitespace().find(|w| matches!(*w, "P0" | "P1" | "P2" | "P3")).unwrap_or("?");
            rows.push((id.to_string(), ticks.first().copied().unwrap_or("?").to_string(), priority.to_string()));
        }
        rows
    }

    #[test]
    fn the_register_matches_locations_md_row_for_row() {
        let book = parse(DEFAULT_LOCATIONS).unwrap();
        let rows = register_rows();
        assert!(rows.len() >= 24, "found only {} rows in LOCATIONS.md", rows.len());
        for (id, source, priority) in &rows {
            let location = book.get(id).unwrap_or_else(|| panic!("LOCATIONS.md lists {id} but locations.world does not"));
            assert_eq!(location.sources[0].name(), source, "{id}: first source label");
            assert_eq!(&format!("{:?}", location.priority), priority, "{id}: priority");
        }
        for location in book.iter() {
            let listed = rows.iter().any(|(id, _, _)| *id == location.id);
            let added_by_game = location.sources == [Source::GameOriginal];
            assert!(listed || added_by_game, "{} is in locations.world but not in LOCATIONS.md", location.id);
        }
    }

    #[test]
    fn the_main_story_route_is_connected_from_the_start() {
        let book = parse(DEFAULT_LOCATIONS).unwrap();
        let reach = book.reachable_from(&book.start().id);
        for id in ["YELLOW_BRICK_ROAD_EAST", "WOODS_FIRST", "EMERALD_CITY_GATE", "EMERALD_CITY", "WIZARD_PALACE", "WESTERN_ROAD", "WINKIE_COUNTRY", "WITCH_FORTRESS", "SOUTHERN_ROUTE", "QUADLING_COUNTRY", "GLINDA_PALACE", "KANSAS_FARM"] {
            assert!(reach.contains(id), "{id} cannot be reached from the start");
        }
    }

    #[test]
    fn later_book_places_stay_separate_from_the_1900_route() {
        let book = parse(DEFAULT_LOCATIONS).unwrap();
        let gillikin = book.get("GILLIKIN_COUNTRY").unwrap();
        assert_eq!(gillikin.sources, vec![Source::LaterOz]);
        assert_eq!(gillikin.priority, Priority::P3);
        assert!(!gillikin.is_playable());
        // Nothing on the main route leads into later-book material.
        for location in book.iter().filter(|l| l.sources.contains(&Source::Novel1900)) {
            assert!(!location.exits.iter().any(|e| e == "GILLIKIN_COUNTRY"), "{}", location.id);
        }
        assert!(book.get("NORTH_REGION").unwrap().summary.contains("Gillikin") || !book.get("NORTH_REGION").unwrap().sources.contains(&Source::LaterOz));
    }

    #[test]
    fn every_shipped_map_file_exists_and_every_level_checks_out_against_the_register() {
        let book = parse(DEFAULT_LOCATIONS).unwrap();
        let mut levels = Vec::new();
        for location in book.playable() {
            let name = location.map.as_deref().unwrap();
            let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(MAPS_DIR).join(name);
            let level = Level::load(&path).unwrap_or_else(|e| panic!("{}: {e}", location.id));
            levels.push((location.id.clone(), level));
        }
        check_world(&book, |id| levels.iter().find(|(l, _)| l == id).map(|(_, level)| level)).unwrap();
        // Every playable place has a theme or a track, so no area is silent.
        for location in book.playable() {
            assert!(location.theme.is_some() || location.music.is_some(), "{} has no music", location.id);
        }
        // The two areas sound different.
        let themes: BTreeSet<_> = book.playable().filter_map(|l| l.theme.clone()).collect();
        assert!(themes.len() >= 2, "each area needs its own theme");
    }
}
