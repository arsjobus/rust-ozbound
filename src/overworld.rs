//! The open world: playable regions placed side by side as **chunks** of one big map.
//!
//! Pure Rust (no Bevy). `assets/world/overworld.world` says where each region (a level from
//! `assets/maps/`) sits in the world, and which **zones** (music areas) cover it:
//!
//! ```text
//! [world]                       # optional
//! load_margin = 20              # a chunk loads when the player is this many tiles from it
//! unload_margin = 28            # ...and unloads once the player is further than this
//!
//! [chunk MUNCHKIN_LANDS]        # a playable location id from locations.world
//! at = 0 0                      # world tile of the chunk's own tile (0, 0)
//!
//! [zone bluebrook]
//! name = Bluebrook              # shown in the chat when the player enters
//! rect = 0 0 39 23              # x0 y0 x1 y1, world tiles, both corners included
//! music = bluebrook.mp3         # file in assets/bgm (optional)
//! theme = village               # built-in fallback theme (optional; one of the two is needed)
//! ```
//!
//! # Coordinates
//!
//! Every chunk keeps its own level file and its own local tiles. The **world frame** puts
//! them together: world tile = chunk origin + local tile. All origins are non-negative, so
//! the whole overworld fits in one `width x height` rectangle ([`Overworld::bounds`]) whose
//! size never changes while the game runs. That is what lets the game keep one tile map and
//! one coordinate system while chunks come and go.
//!
//! # Streaming
//!
//! [`Overworld::wanted`] says which chunks should be loaded for a player position. A chunk
//! loads when the player is within `load_margin` tiles of it and unloads only beyond
//! `unload_margin` (the gap stops a chunk flickering in and out on its border). The chunk the
//! player stands in is always wanted. [`Overworld::compose`] builds the map and level for a
//! set of loaded chunks; the tiles of chunks that are not loaded are blocked, so a route can
//! never lead into a chunk that is not there.
//!
//! # Rules checked at load
//!
//! * every chunk is a playable location, placed once, at non-negative coordinates, and no
//!   two chunks overlap;
//! * when the world file sets `chunk_size` (the shipped one sets [`CHUNK_SIZE`]), every chunk
//!   is exactly that many tiles square and sits on that grid;
//! * the register's exits agree with the map: two chunks that touch with a walkable crossing
//!   must list each other, and chunks that list each other must touch with a walkable
//!   crossing. Overworld chunks connect by **walking**; a level `[exits]` tile that leads to
//!   another chunk is refused (exits are for caves and other places off the overworld);
//! * everything is reachable on foot from the start;
//! * zones have unique ids, fit inside the world, name a track or a known theme, and every
//!   walkable tile of every chunk is inside at least one zone (so no walkable ground is
//!   silent by accident). Where zones overlap, the one declared last wins.

use std::collections::VecDeque;

use crate::{
    level::Level,
    locations::Locations,
    music,
    save::WorldChanges,
    textdata::{is_valid_id, is_valid_location_id, read_lines, ContentError, Item},
    world::{Tile, TilePos, WorldMap},
};

/// The side of a chunk the shipped world uses, in tiles (`chunk_size` in `overworld.world`).
///
/// 64 x 64 is 4,096 tiles: the ground of a chunk is a handful of merged meshes whatever its
/// size, a power of two keeps chunk origins on an easy grid, and with the default margins (20
/// to load, 28 to drop) the player never has more than four chunks loaded at once. A larger
/// chunk would spawn more props in the one frame it streams in; a smaller one would make
/// the margins load a ring of many chunks.
pub const CHUNK_SIZE: i32 = 64;
/// The smallest and largest `chunk_size` a world file may ask for.
pub const MIN_CHUNK_SIZE: i32 = 8;
/// A chunk is one map file, and a map file may be at most [`crate::level::MAX_MAP_SIZE`] tiles.
pub const MAX_CHUNK_SIZE: i32 = crate::level::MAX_MAP_SIZE as i32;

/// Default distance, in tiles, at which a chunk starts loading.
pub const DEFAULT_LOAD_MARGIN: i32 = 20;
/// Default distance beyond which a loaded chunk is dropped.
pub const DEFAULT_UNLOAD_MARGIN: i32 = 28;

/// One region placed in the world.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Chunk {
    pub id: String,
    /// World tile of the region's local tile (0, 0).
    pub origin: TilePos,
    pub width: i32,
    pub height: i32,
}

impl Chunk {
    /// True when the world tile lies inside the chunk's rectangle.
    pub fn contains(&self, world: TilePos) -> bool {
        world.x >= self.origin.x && world.x < self.origin.x + self.width && world.y >= self.origin.y && world.y < self.origin.y + self.height
    }

    /// Distance in tiles from a world tile to the chunk's rectangle (0 inside it), counting
    /// the larger of the two axes, so a chunk loads as a square halo around it.
    pub fn distance(&self, world: TilePos) -> i32 {
        let dx = (self.origin.x - world.x).max(world.x - (self.origin.x + self.width - 1)).max(0);
        let dy = (self.origin.y - world.y).max(world.y - (self.origin.y + self.height - 1)).max(0);
        dx.max(dy)
    }

    pub fn to_world(&self, local: TilePos) -> TilePos {
        TilePos::new(self.origin.x + local.x, self.origin.y + local.y)
    }

    /// The local tile for a world tile inside the chunk.
    pub fn to_local(&self, world: TilePos) -> Option<TilePos> {
        self.contains(world).then(|| TilePos::new(world.x - self.origin.x, world.y - self.origin.y))
    }
}

/// What a zone plays. Two zones with equal specs share one track, so crossing between them
/// does not restart or fade the music.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct TrackSpec {
    /// File name in `assets/bgm`.
    pub music: Option<String>,
    /// Built-in theme id used when there is no file (or it cannot be played).
    pub theme: Option<String>,
}

/// A rectangle of the world with a name and a music track.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Zone {
    pub id: String,
    pub name: String,
    pub min: TilePos,
    /// Inclusive.
    pub max: TilePos,
    pub track: TrackSpec,
}

impl Zone {
    pub fn contains(&self, world: TilePos) -> bool {
        world.x >= self.min.x && world.x <= self.max.x && world.y >= self.min.y && world.y <= self.max.y
    }

    fn overlaps(&self, chunk: &Chunk) -> bool {
        self.min.x < chunk.origin.x + chunk.width
            && self.max.x >= chunk.origin.x
            && self.min.y < chunk.origin.y + chunk.height
            && self.max.y >= chunk.origin.y
    }
}

/// The placed chunks and zones of the open world.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Overworld {
    chunks: Vec<Chunk>,
    zones: Vec<Zone>,
    load_margin: i32,
    unload_margin: i32,
    chunk_size: Option<i32>,
    width: usize,
    height: usize,
}

/// The map and level of the loaded chunks, in world coordinates.
#[derive(Debug, Clone)]
pub struct Composite {
    pub map: WorldMap,
    pub level: Level,
}

fn err(line: usize, message: impl Into<String>) -> ContentError {
    ContentError::at(line, message)
}

fn numbers(line: usize, what: &str, value: &str, count: usize) -> Result<Vec<i32>, ContentError> {
    let parts: Vec<&str> = value.split_whitespace().collect();
    if parts.len() != count {
        return Err(err(line, format!("{what} needs {count} whole numbers, found `{value}`")));
    }
    parts.iter().map(|p| p.parse::<i32>().map_err(|_| err(line, format!("`{p}` in {what} is not a whole number")))).collect()
}

struct Draft {
    line: usize,
    id: String,
    fields: Vec<(usize, String, String)>,
}

impl Overworld {
    /// Parses and checks `text`. `level_of` gives the level of a playable location (its map
    /// size and contents decide how chunks fit together).
    pub fn parse<'a>(text: &str, book: &Locations, level_of: impl Fn(&str) -> Option<&'a Level>) -> Result<Self, ContentError> {
        let mut world_fields: Option<(usize, Vec<(usize, String, String)>)> = None;
        let mut chunks: Vec<Draft> = Vec::new();
        let mut zones: Vec<Draft> = Vec::new();
        // 0 = [world], 1 = chunk, 2 = zone
        let mut current: Option<u8> = None;
        for line in read_lines(text)? {
            match line.item {
                Item::Section { kind, arg } => match kind.as_str() {
                    "world" => {
                        if world_fields.is_some() {
                            return Err(err(line.number, "[world] appears twice"));
                        }
                        world_fields = Some((line.number, Vec::new()));
                        current = Some(0);
                    }
                    "chunk" => {
                        if !is_valid_location_id(&arg) {
                            return Err(err(line.number, format!("chunk id `{arg}` must be a location id (capital letters, digits, underscores)")));
                        }
                        if chunks.iter().any(|c| c.id == arg) {
                            return Err(err(line.number, format!("chunk `{arg}` is placed twice")));
                        }
                        chunks.push(Draft { line: line.number, id: arg, fields: Vec::new() });
                        current = Some(1);
                    }
                    "zone" => {
                        if !is_valid_id(&arg) {
                            return Err(err(line.number, format!("zone id `{arg}` must use only lower-case letters, digits, and underscores")));
                        }
                        if zones.iter().any(|z| z.id == arg) {
                            return Err(err(line.number, format!("zone `{arg}` is listed twice")));
                        }
                        zones.push(Draft { line: line.number, id: arg, fields: Vec::new() });
                        current = Some(2);
                    }
                    other => return Err(err(line.number, format!("unknown section [{other}] (use [world], [chunk ID], or [zone id])"))),
                },
                Item::Field { key, value } => {
                    let entry = (line.number, key, value);
                    match current {
                        Some(0) => world_fields.as_mut().expect("set with the section").1.push(entry),
                        Some(1) => chunks.last_mut().expect("set with the section").fields.push(entry),
                        Some(2) => zones.last_mut().expect("set with the section").fields.push(entry),
                        _ => return Err(err(line.number, "a field must come after a section header")),
                    }
                }
            }
        }
        if chunks.is_empty() {
            return Err(err(0, "the overworld places no chunks"));
        }

        let (mut load_margin, mut unload_margin) = (DEFAULT_LOAD_MARGIN, DEFAULT_UNLOAD_MARGIN);
        let mut chunk_size: Option<i32> = None;
        if let Some((_, fields)) = &world_fields {
            for (line, key, value) in fields {
                let parsed: i32 = value.parse().map_err(|_| err(*line, format!("`{key}` must be a whole number of tiles, found `{value}`")))?;
                match key.as_str() {
                    "load_margin" => load_margin = parsed,
                    "unload_margin" => unload_margin = parsed,
                    "chunk_size" => {
                        if !(MIN_CHUNK_SIZE..=MAX_CHUNK_SIZE).contains(&parsed) {
                            return Err(err(*line, format!("chunk_size must be between {MIN_CHUNK_SIZE} and {MAX_CHUNK_SIZE} tiles, found {parsed}")));
                        }
                        chunk_size = Some(parsed);
                    }
                    _ => return Err(err(*line, format!("unknown [world] field `{key}` (use load_margin, unload_margin, or chunk_size)"))),
                }
            }
        }
        let world_line = world_fields.as_ref().map_or(0, |(line, _)| *line);
        if load_margin < 1 {
            return Err(err(world_line, "load_margin must be at least 1 tile"));
        }
        if unload_margin < load_margin {
            return Err(err(world_line, format!("unload_margin ({unload_margin}) must not be smaller than load_margin ({load_margin}), or chunks would flicker")));
        }

        let mut placed = Vec::new();
        for draft in &chunks {
            let Some(location) = book.get(&draft.id) else {
                return Err(err(draft.line, format!("chunk `{}` is not in the location register", draft.id)));
            };
            let Some(level) = location.is_playable().then(|| level_of(&draft.id)).flatten() else {
                return Err(err(draft.line, format!("chunk `{}` is not a playable location (it needs a map)", draft.id)));
            };
            let mut at = None;
            for (line, key, value) in &draft.fields {
                match key.as_str() {
                    "at" => {
                        if at.is_some() {
                            return Err(err(*line, format!("`at` appears twice in chunk `{}`", draft.id)));
                        }
                        let n = numbers(*line, "`at`", value, 2)?;
                        if n[0] < 0 || n[1] < 0 {
                            return Err(err(*line, format!("`at` of chunk `{}` must not be negative (the world starts at 0 0), found {} {}", draft.id, n[0], n[1])));
                        }
                        at = Some(TilePos::new(n[0], n[1]));
                    }
                    other => return Err(err(*line, format!("unknown chunk field `{other}` (use `at`)"))),
                }
            }
            let Some(origin) = at else {
                return Err(err(draft.line, format!("chunk `{}` needs an `at = x y` field", draft.id)));
            };
            let (width, height) = (level.map.width() as i32, level.map.height() as i32);
            if let Some(size) = chunk_size {
                if (width, height) != (size, size) {
                    return Err(err(draft.line, format!("chunk `{}` is {width} x {height} tiles, but every chunk of this world is {size} x {size} (`chunk_size` in [world]); resize its map", draft.id)));
                }
                if origin.x % size != 0 || origin.y % size != 0 {
                    return Err(err(draft.line, format!("chunk `{}` is placed at {} {}, which is not on the {size}-tile chunk grid (each coordinate must be a multiple of {size})", draft.id, origin.x, origin.y)));
                }
            }
            placed.push(Chunk { id: draft.id.clone(), origin, width, height });
        }

        for (i, a) in placed.iter().enumerate() {
            for b in &placed[i + 1..] {
                let apart = a.origin.x + a.width <= b.origin.x || b.origin.x + b.width <= a.origin.x || a.origin.y + a.height <= b.origin.y || b.origin.y + b.height <= a.origin.y;
                if !apart {
                    let line = chunks.iter().find(|d| d.id == b.id).map_or(0, |d| d.line);
                    return Err(err(line, format!("chunks `{}` and `{}` overlap", a.id, b.id)));
                }
            }
        }
        let width = placed.iter().map(|c| c.origin.x + c.width).max().unwrap_or(0) as usize;
        let height = placed.iter().map(|c| c.origin.y + c.height).max().unwrap_or(0) as usize;

        let mut built_zones = Vec::new();
        for draft in &zones {
            built_zones.push(Self::build_zone(draft, width as i32, height as i32)?);
        }
        let world = Self { chunks: placed, zones: built_zones, load_margin, unload_margin, chunk_size, width, height };
        world.check_start(book, &chunks)?;
        world.check_exits(book, &level_of, &chunks)?;
        world.check_house_ids(&level_of, &chunks)?;
        world.check_reachable(&level_of, &chunks)?;
        world.check_zone_cover(&level_of, &zones)?;
        Ok(world)
    }

    fn build_zone(draft: &Draft, width: i32, height: i32) -> Result<Zone, ContentError> {
        let (mut name, mut rect, mut music_file, mut theme) = (None, None, None, None);
        for (line, key, value) in &draft.fields {
            if value.is_empty() {
                return Err(err(*line, format!("field `{key}` of zone `{}` is empty", draft.id)));
            }
            match key.as_str() {
                "name" => name = Some(value.clone()),
                "rect" => {
                    let n = numbers(*line, "`rect`", value, 4)?;
                    if n[0] > n[2] || n[1] > n[3] {
                        return Err(err(*line, format!("`rect` of zone `{}` must list the lower-left corner first (x0 y0 x1 y1)", draft.id)));
                    }
                    if n[0] < 0 || n[1] < 0 || n[2] >= width || n[3] >= height {
                        return Err(err(*line, format!("`rect` of zone `{}` reaches outside the world ({width} x {height} tiles)", draft.id)));
                    }
                    rect = Some((TilePos::new(n[0], n[1]), TilePos::new(n[2], n[3])));
                }
                "music" => {
                    if !music::is_valid_track_name(value) {
                        return Err(err(*line, format!("`{value}` is not a valid track name (a plain file name ending in .ogg, .wav, or .mp3)")));
                    }
                    music_file = Some(value.clone());
                }
                "theme" => {
                    if !music::THEMES.contains(&value.as_str()) {
                        return Err(err(*line, format!("unknown built-in theme `{value}` (use {})", music::THEMES.join(" or "))));
                    }
                    theme = Some(value.clone());
                }
                other => return Err(err(*line, format!("unknown zone field `{other}` (use name, rect, music, or theme)"))),
            }
        }
        let missing = |what: &str| err(draft.line, format!("zone `{}` needs a `{what}` field", draft.id));
        let name = name.ok_or_else(|| missing("name"))?;
        let (min, max) = rect.ok_or_else(|| missing("rect"))?;
        if music_file.is_none() && theme.is_none() {
            return Err(err(draft.line, format!("zone `{}` needs `music` or `theme`, or the area would be silent", draft.id)));
        }
        Ok(Zone { id: draft.id.clone(), name, min, max, track: TrackSpec { music: music_file, theme } })
    }

    fn line_of(drafts: &[Draft], id: &str) -> usize {
        drafts.iter().find(|d| d.id == id).map_or(0, |d| d.line)
    }

    fn check_start(&self, book: &Locations, drafts: &[Draft]) -> Result<(), ContentError> {
        let start = book.start();
        if self.chunk(&start.id).is_none() {
            return Err(err(0, format!("the starting place `{}` is not placed as a chunk; the player would start off the overworld", start.id)));
        }
        let _ = drafts;
        Ok(())
    }

    fn check_exits<'a>(&self, book: &Locations, level_of: &impl Fn(&str) -> Option<&'a Level>, drafts: &[Draft]) -> Result<(), ContentError> {
        for (i, a) in self.chunks.iter().enumerate() {
            let line = Self::line_of(drafts, &a.id);
            let level = level_of(&a.id).expect("checked while placing");
            for exit in &level.exits {
                if self.chunk(&exit.region).is_some() {
                    return Err(err(
                        line,
                        format!(
                            "the exit at ({}, {}) in `{}` leads to `{}`, which is another overworld chunk; chunks connect by walking, so remove the exit and join the paths at the border",
                            exit.tile.x, exit.tile.y, a.id, exit.region
                        ),
                    ));
                }
            }
            for b in &self.chunks[i + 1..] {
                let crossings = self.crossings(a, b, level, level_of(&b.id).expect("checked while placing"));
                let a_lists = book.get(&a.id).is_some_and(|l| l.exits.contains(&b.id));
                let b_lists = book.get(&b.id).is_some_and(|l| l.exits.contains(&a.id));
                let b_line = Self::line_of(drafts, &b.id);
                if crossings > 0 && !(a_lists && b_lists) {
                    return Err(err(b_line, format!("`{}` and `{}` are joined by a walkable path, but the register does not list each as an exit of the other", a.id, b.id)));
                }
                if (a_lists || b_lists) && crossings == 0 {
                    return Err(err(
                        b_line,
                        format!("the register links `{}` and `{}`, but no path crosses from one to the other; make them touch and join a path at the border", a.id, b.id),
                    ));
                }
            }
        }
        Ok(())
    }

    /// Number of tile pairs across the shared border of two chunks where both sides are walkable.
    fn crossings(&self, a: &Chunk, b: &Chunk, level_a: &Level, level_b: &Level) -> usize {
        let mut count = 0;
        let walk = |chunk: &Chunk, level: &Level, world: TilePos| chunk.to_local(world).is_some_and(|local| level.map.can_walk_to(local));
        for (from, to, level_from, level_to) in [(a, b, level_a, level_b), (b, a, level_b, level_a)] {
            // Pairs are counted once, from the chunk on the low side of the border.
            let east = from.origin.x + from.width == to.origin.x;
            let north = from.origin.y + from.height == to.origin.y;
            if east {
                for y in from.origin.y..from.origin.y + from.height {
                    let (left, right) = (TilePos::new(from.origin.x + from.width - 1, y), TilePos::new(to.origin.x, y));
                    if walk(from, level_from, left) && walk(to, level_to, right) {
                        count += 1;
                    }
                }
            }
            if north {
                for x in from.origin.x..from.origin.x + from.width {
                    let (below, above) = (TilePos::new(x, from.origin.y + from.height - 1), TilePos::new(x, to.origin.y));
                    if walk(from, level_from, below) && walk(to, level_to, above) {
                        count += 1;
                    }
                }
            }
        }
        count
    }

    /// Houses are found by id while the game runs (the list of loaded houses changes as chunks
    /// come and go), so an id must not be used in two chunks.
    fn check_house_ids<'a>(&self, level_of: &impl Fn(&str) -> Option<&'a Level>, drafts: &[Draft]) -> Result<(), ContentError> {
        let mut seen: Vec<(&str, &str)> = Vec::new();
        for chunk in &self.chunks {
            let level = level_of(&chunk.id).expect("checked while placing");
            for house in &level.houses {
                if let Some((_, other)) = seen.iter().find(|(id, _)| *id == house.id) {
                    return Err(err(Self::line_of(drafts, &chunk.id), format!("house id `{}` is used in both `{other}` and `{}`; ids must be unique across the overworld", house.id, chunk.id)));
                }
                seen.push((house.id.as_str(), chunk.id.as_str()));
            }
        }
        Ok(())
    }

    fn check_reachable<'a>(&self, level_of: &impl Fn(&str) -> Option<&'a Level>, drafts: &[Draft]) -> Result<(), ContentError> {
        let all: Vec<&str> = self.chunks.iter().map(|c| c.id.as_str()).collect();
        let composite = self.compose(level_of, &all);
        let start = self.chunks.iter().find_map(|c| level_of(&c.id).filter(|l| l.is_story_level()).map(|l| c.to_world(l.player_start)));
        let Some(start) = start.or(Some(composite.level.player_start)) else { return Ok(()) };
        let mut seen = vec![false; self.width * self.height];
        let idx = |p: TilePos| p.y as usize * self.width + p.x as usize;
        let mut queue = VecDeque::from([start]);
        seen[idx(start)] = true;
        while let Some(cur) = queue.pop_front() {
            for (dx, dy) in [(1, 0), (-1, 0), (0, 1), (0, -1)] {
                let next = TilePos::new(cur.x + dx, cur.y + dy);
                if composite.map.can_walk_to(next) && !seen[idx(next)] {
                    seen[idx(next)] = true;
                    queue.push_back(next);
                }
            }
        }
        for chunk in &self.chunks {
            let level = level_of(&chunk.id).expect("checked while placing");
            let spot = chunk.to_world(level.player_start);
            if !seen[idx(spot)] {
                return Err(err(Self::line_of(drafts, &chunk.id), format!("`{}` cannot be reached on foot from the start; join its path to the rest of the world", chunk.id)));
            }
        }
        Ok(())
    }

    fn check_zone_cover<'a>(&self, level_of: &impl Fn(&str) -> Option<&'a Level>, zone_drafts: &[Draft]) -> Result<(), ContentError> {
        for zone in &self.zones {
            if !self.chunks.iter().any(|c| zone.overlaps(c)) {
                return Err(err(Self::line_of(zone_drafts, &zone.id), format!("zone `{}` covers no chunk", zone.id)));
            }
        }
        for chunk in &self.chunks {
            let level = level_of(&chunk.id).expect("checked while placing");
            for y in 0..chunk.height {
                for x in 0..chunk.width {
                    let local = TilePos::new(x, y);
                    if level.map.can_walk_to(local) && self.zone_at(chunk.to_world(local)).is_none() {
                        let w = chunk.to_world(local);
                        return Err(err(
                            0,
                            format!("walkable tile ({}, {}) of `{}` (world tile {}, {}) is in no zone, so no music would play there", x, y, chunk.id, w.x, w.y),
                        ));
                    }
                }
            }
        }
        Ok(())
    }

    /// Width and height, in tiles, of the rectangle that holds every chunk.
    pub fn bounds(&self) -> (usize, usize) {
        (self.width, self.height)
    }

    pub fn chunks(&self) -> &[Chunk] {
        &self.chunks
    }

    pub fn zones(&self) -> &[Zone] {
        &self.zones
    }

    pub fn chunk(&self, id: &str) -> Option<&Chunk> {
        self.chunks.iter().find(|c| c.id == id)
    }

    /// The side, in tiles, that every chunk must have (and the grid they sit on), when the
    /// world file sets `chunk_size`.
    pub fn chunk_size(&self) -> Option<i32> {
        self.chunk_size
    }

    pub fn load_margin(&self) -> i32 {
        self.load_margin
    }

    pub fn unload_margin(&self) -> i32 {
        self.unload_margin
    }

    /// The chunk that holds a world tile.
    pub fn chunk_at(&self, world: TilePos) -> Option<&Chunk> {
        self.chunks.iter().find(|c| c.contains(world))
    }

    /// The zone of a world tile: the last declared zone that contains it.
    pub fn zone_at(&self, world: TilePos) -> Option<&Zone> {
        self.zones.iter().rev().find(|z| z.contains(world))
    }

    /// The zones that overlap a chunk (the music its tiles can play).
    pub fn zones_in(&self, chunk: &str) -> Vec<&Zone> {
        self.chunk(chunk).map_or_else(Vec::new, |c| self.zones.iter().filter(|z| z.overlaps(c)).collect())
    }

    /// The chunk and local tile for a world tile.
    pub fn to_local(&self, world: TilePos) -> Option<(&Chunk, TilePos)> {
        let chunk = self.chunk_at(world)?;
        Some((chunk, chunk.to_local(world)?))
    }

    /// The world tile for a chunk's local tile.
    pub fn to_world(&self, chunk: &str, local: TilePos) -> Option<TilePos> {
        self.chunk(chunk).map(|c| c.to_world(local))
    }

    /// The chunks that should be loaded with the player on `player`, given the ones loaded
    /// now. Declaration order; the chunk under the player is always included.
    pub fn wanted(&self, player: TilePos, loaded: &[String]) -> Vec<String> {
        self.chunks
            .iter()
            .filter(|c| {
                let d = c.distance(player);
                d == 0 || d <= self.load_margin || (d <= self.unload_margin && loaded.contains(&c.id))
            })
            .map(|c| c.id.clone())
            .collect()
    }

    /// The map and level of the `loaded` chunks in world coordinates. Tiles outside every
    /// loaded chunk are blocked. The level's player start, guide, and every spawn come from
    /// the loaded chunks, moved to their world tiles.
    pub fn compose<'a>(&self, level_of: &impl Fn(&str) -> Option<&'a Level>, loaded: &[impl AsRef<str>]) -> Composite {
        let mut rows = vec![vec![Tile::Tree; self.width]; self.height];
        let chunks: Vec<&Chunk> = self.chunks.iter().filter(|c| loaded.iter().any(|id| id.as_ref() == c.id)).collect();
        for chunk in &chunks {
            let Some(level) = level_of(&chunk.id) else { continue };
            for y in 0..chunk.height {
                for x in 0..chunk.width {
                    let local = TilePos::new(x, y);
                    let world = chunk.to_world(local);
                    rows[world.y as usize][world.x as usize] = level.map.tile_at(local).expect("inside the chunk");
                }
            }
        }
        let map = WorldMap::from_rows(rows).expect("the world is at least one tile");
        let first = chunks.first().and_then(|c| level_of(&c.id).map(|l| c.to_world(l.player_start))).unwrap_or(TilePos::new(0, 0));
        let mut level = Level {
            map: map.clone(),
            player_start: first,
            guide: None,
            moonpetals: Vec::new(),
            nodes: Vec::new(),
            signs: Vec::new(),
            npcs: Vec::new(),
            behaviors: Vec::new(),
            places: Vec::new(),
            creatures: Vec::new(),
            houses: Vec::new(),
            plots: Vec::new(),
            exits: Vec::new(),
        };
        for chunk in &chunks {
            let Some(local) = level_of(&chunk.id) else { continue };
            let at = |t: TilePos| chunk.to_world(t);
            if local.is_story_level() {
                level.player_start = at(local.player_start);
            }
            level.guide = level.guide.or(local.guide.map(at));
            level.moonpetals.extend(local.moonpetals.iter().copied().map(at));
            level.nodes.extend(local.nodes.iter().map(|n| crate::level::NodeSpawn { tile: at(n.tile), ..*n }));
            level.signs.extend(local.signs.iter().map(|s| crate::level::SignSpawn { tile: at(s.tile), text: s.text.clone() }));
            level.npcs.extend(local.npcs.iter().map(|n| crate::level::NpcSpawn { tile: at(n.tile), ..n.clone() }));
            level.behaviors.extend(local.behaviors.iter().cloned());
            level.places.extend(local.places.iter().map(|p| crate::level::PlaceSpawn { tile: at(p.tile), ..p.clone() }));
            level.creatures.extend(local.creatures.iter().map(|c| crate::level::CreatureSpawn { tile: at(c.tile), ..c.clone() }));
            level.houses.extend(local.houses.iter().map(|h| crate::homestead::HouseSpawn { origin: at(h.origin), ..h.clone() }));
            level.plots.extend(local.plots.iter().map(|p| crate::homestead::PlotSpawn { origin: at(p.origin), ..p.clone() }));
            level.exits.extend(local.exits.iter().map(|e| crate::level::ExitSpawn { tile: at(e.tile), ..e.clone() }));
        }
        Composite { map, level }
    }

    /// What the player has changed in one chunk, in world tiles, from what is in the world now:
    /// `petals_left` and `nodes_now` are the moonpetals still standing and the actions left on
    /// every tree, vein, and plant, in world tiles. Anything outside the chunk is ignored, so
    /// the lists may hold the whole loaded world.
    pub fn capture_changes(&self, chunk: &str, level: &Level, petals_left: &[TilePos], nodes_now: &[(TilePos, u32)]) -> WorldChanges {
        let Some(c) = self.chunk(chunk) else { return WorldChanges::none() };
        let petals: Vec<TilePos> = petals_left.iter().filter_map(|t| c.to_local(*t)).collect();
        let nodes: Vec<(TilePos, u32)> = nodes_now.iter().filter_map(|(t, left)| c.to_local(*t).map(|l| (l, *left))).collect();
        self.changes_to_world(chunk, &WorldChanges::capture(level, &petals, &nodes))
    }

    /// Changes recorded in a chunk's own tiles, moved to world tiles.
    pub fn changes_to_world(&self, chunk: &str, changes: &WorldChanges) -> WorldChanges {
        self.chunk(chunk).map_or_else(WorldChanges::none, |c| changes.shifted(c.origin.x, c.origin.y))
    }

    /// Changes recorded in world tiles, moved to a chunk's own tiles.
    pub fn changes_to_local(&self, chunk: &str, changes: &WorldChanges) -> WorldChanges {
        self.chunk(chunk).map_or_else(WorldChanges::none, |c| changes.shifted(-c.origin.x, -c.origin.y))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::locations::Locations;

    const REGISTER: &str = "\
[location HOME]
name = Home
source = NOVEL_1900
priority = P0
summary = Start.
exits = ROAD
map = home.map
start = yes

[location ROAD]
name = Road
source = NOVEL_1900
priority = P0
summary = The road.
exits = HOME, CAVE
map = road.map

[location CAVE]
name = Cave
source = GAME_ORIGINAL
priority = P1
summary = A cave.
exits = ROAD
map = cave.map
";

    // 6 wide x 4 tall. The path runs along y = 1 and reaches both side edges.
    const HOME: &str = "[map]\nTTTTTT\nTTTTTT\n======\nTTTTTT\n\n[player]\n1 1\n\n[guide]\n2 1\n\n[moonpetals]\n0 1\n3 1\n4 1\n";
    // The road's path also runs along y = 1.
    const ROAD: &str = "[map]\nTTTTTT\nTTTTTT\n======\nTTTTTT\n\n[player]\n1 1\n";
    const CAVE: &str = "[map]\n....\n....\n\n[player]\n1 1\n";

    fn levels() -> Vec<(&'static str, Level)> {
        vec![("HOME", Level::parse(HOME).unwrap()), ("ROAD", Level::parse(ROAD).unwrap()), ("CAVE", Level::parse(CAVE).unwrap())]
    }

    fn parse_with(text: &str, home: &str, road: &str) -> Result<Overworld, ContentError> {
        let book = Locations::parse(REGISTER).unwrap();
        let ls = vec![("HOME", Level::parse(home).unwrap()), ("ROAD", Level::parse(road).unwrap()), ("CAVE", Level::parse(CAVE).unwrap())];
        Overworld::parse(text, &book, |id| ls.iter().find(|(n, _)| *n == id).map(|(_, l)| l))
    }

    const WORLD: &str = "\
[world]
load_margin = 3
unload_margin = 5

[chunk HOME]
at = 0 0

[chunk ROAD]
at = 6 0

[zone home]
name = Home
rect = 0 0 5 3
theme = village

[zone road]
name = The Road
rect = 6 0 11 3
music = road.ogg
theme = road
";

    fn ok() -> Overworld {
        parse_with(WORLD, HOME, ROAD).expect("the sample world is valid")
    }

    fn bad(text: &str) -> String {
        parse_with(text, HOME, ROAD).unwrap_err().to_string()
    }

    fn find<'a>(ls: &'a [(&'static str, Level)], id: &str) -> Option<&'a Level> {
        ls.iter().find(|(name, _)| *name == id).map(|(_, level)| level)
    }

    #[test]
    fn a_valid_world_parses_with_its_bounds_margins_and_zones() {
        let world = ok();
        assert_eq!(world.bounds(), (12, 4));
        assert_eq!((world.load_margin(), world.unload_margin()), (3, 5));
        assert_eq!(world.chunks().len(), 2);
        assert_eq!(world.zones().len(), 2);
        assert_eq!(world.chunk("ROAD").unwrap().origin, TilePos::new(6, 0));
    }

    #[test]
    fn a_chunk_size_refuses_any_chunk_that_is_not_that_square() {
        let text = WORLD.replace("unload_margin = 5\n", "unload_margin = 5\nchunk_size = 8\n");
        let why = bad(&text);
        assert!(why.contains("6 x 4") && why.contains("8 x 8") && why.contains("chunk_size"), "{why}");
        let text = WORLD.replace("unload_margin = 5\n", "unload_margin = 5\nchunk_size = 16\n");
        assert!(bad(&text).contains("16 x 16"), "a chunk that is too small is refused too");
    }

    #[test]
    fn chunk_size_must_be_a_sensible_whole_number() {
        for value in ["0", "7", "-8", "65", "300"] {
            let text = WORLD.replace("unload_margin = 5\n", &format!("unload_margin = 5\nchunk_size = {value}\n"));
            assert!(bad(&text).contains("chunk_size must be between"), "{value}");
        }
        let text = WORLD.replace("unload_margin = 5\n", "unload_margin = 5\nchunk_size = big\n");
        assert!(bad(&text).contains("whole number"));
        assert_eq!(ok().chunk_size(), None, "without the key chunks may be any size");
    }

    #[test]
    fn the_shipped_world_is_made_of_equal_square_chunks_on_a_grid() {
        let world = crate::village::builtin().expect("shipped world loads");
        let overworld = world.overworld.as_ref().expect("the open world");
        assert_eq!(overworld.chunk_size(), Some(CHUNK_SIZE));
        assert!(overworld.chunks().len() >= 2);
        for chunk in overworld.chunks() {
            assert_eq!((chunk.width, chunk.height), (CHUNK_SIZE, CHUNK_SIZE), "{}", chunk.id);
            assert_eq!((chunk.origin.x % CHUNK_SIZE, chunk.origin.y % CHUNK_SIZE), (0, 0), "{} is on the grid", chunk.id);
        }
    }

    #[test]
    fn margins_default_when_the_world_section_is_missing() {
        let text = WORLD.replace("[world]\nload_margin = 3\nunload_margin = 5\n", "");
        let world = parse_with(&text, HOME, ROAD).unwrap();
        assert_eq!((world.load_margin(), world.unload_margin()), (DEFAULT_LOAD_MARGIN, DEFAULT_UNLOAD_MARGIN));
    }

    #[test]
    fn world_and_local_tiles_convert_both_ways() {
        let world = ok();
        assert_eq!(world.to_world("ROAD", TilePos::new(1, 1)), Some(TilePos::new(7, 1)));
        let (chunk, local) = world.to_local(TilePos::new(7, 1)).unwrap();
        assert_eq!((chunk.id.as_str(), local), ("ROAD", TilePos::new(1, 1)));
        assert!(world.to_local(TilePos::new(12, 1)).is_none());
        assert!(world.to_world("NOPE", TilePos::new(0, 0)).is_none());
        for chunk in world.chunks() {
            for y in 0..chunk.height {
                for x in 0..chunk.width {
                    let local = TilePos::new(x, y);
                    assert_eq!(chunk.to_local(chunk.to_world(local)), Some(local));
                }
            }
        }
    }

    #[test]
    fn distance_is_zero_inside_and_counts_the_larger_axis_outside() {
        let world = ok();
        let road = world.chunk("ROAD").unwrap();
        assert_eq!(road.distance(TilePos::new(8, 2)), 0);
        assert_eq!(road.distance(TilePos::new(5, 1)), 1);
        assert_eq!(road.distance(TilePos::new(0, 1)), 6);
        assert_eq!(road.distance(TilePos::new(3, 10)), 7, "the larger of 3 across and 7 up");
    }

    fn wanted(world: &Overworld, player: (i32, i32), loaded: &[&str]) -> Vec<String> {
        world.wanted(TilePos::new(player.0, player.1), &loaded.iter().map(|s| s.to_string()).collect::<Vec<_>>())
    }

    #[test]
    fn the_chunk_under_the_player_always_loads_and_neighbours_load_inside_the_margin() {
        let world = ok();
        assert_eq!(wanted(&world, (0, 1), &[]), ["HOME"], "the road is 6 tiles away");
        assert_eq!(wanted(&world, (3, 1), &[]), ["HOME", "ROAD"], "3 tiles from the road");
        assert_eq!(wanted(&world, (8, 1), &[]), ["HOME", "ROAD"], "home's edge is 3 tiles behind");
        assert_eq!(wanted(&world, (11, 1), &[]), ["ROAD"], "home is 6 tiles behind");
    }

    #[test]
    fn a_loaded_chunk_stays_until_the_unload_margin_so_borders_do_not_flicker() {
        let world = ok();
        // 4 and 5 tiles from home: too far to load it, near enough to keep it.
        assert_eq!(wanted(&world, (9, 1), &[]), ["ROAD"]);
        assert_eq!(wanted(&world, (9, 1), &["HOME", "ROAD"]), ["HOME", "ROAD"]);
        assert_eq!(wanted(&world, (10, 1), &["HOME", "ROAD"]), ["HOME", "ROAD"]);
        assert_eq!(wanted(&world, (11, 1), &["HOME", "ROAD"]), ["ROAD"], "6 tiles is beyond the unload margin");
    }

    #[test]
    fn walking_along_the_whole_world_never_loses_the_chunk_underfoot() {
        let world = ok();
        let mut loaded: Vec<String> = Vec::new();
        for x in (0..12).chain((0..12).rev()) {
            let player = TilePos::new(x, 1);
            loaded = world.wanted(player, &loaded);
            let here = world.chunk_at(player).unwrap();
            assert!(loaded.contains(&here.id), "x = {x}");
        }
    }

    #[test]
    fn zones_are_found_by_world_tile_and_the_last_declared_wins() {
        let text = format!("{WORLD}\n[zone gate]\nname = The Gate\nrect = 5 0 6 3\ntheme = road\n");
        let world = parse_with(&text, HOME, ROAD).unwrap();
        assert_eq!(world.zone_at(TilePos::new(2, 1)).unwrap().id, "home");
        assert_eq!(world.zone_at(TilePos::new(9, 1)).unwrap().id, "road");
        assert_eq!(world.zone_at(TilePos::new(5, 1)).unwrap().id, "gate", "overlaps home; declared later");
        assert_eq!(world.zone_at(TilePos::new(6, 1)).unwrap().id, "gate");
        assert!(world.zone_at(TilePos::new(40, 1)).is_none());
    }

    #[test]
    fn zones_may_be_smaller_than_a_chunk_and_span_a_border() {
        let text = WORLD.replace("rect = 0 0 5 3", "rect = 0 0 7 3").replace("rect = 6 0 11 3", "rect = 8 0 11 3");
        let world = parse_with(&text, HOME, ROAD).unwrap();
        assert_eq!(world.zone_at(TilePos::new(7, 1)).unwrap().id, "home", "the home zone reaches into the road chunk");
        assert_eq!(world.zone_at(TilePos::new(8, 1)).unwrap().id, "road");
    }

    #[test]
    fn compose_places_every_loaded_chunk_in_world_coordinates() {
        let world = ok();
        let ls = levels();
        let composite = world.compose(&|id: &str| find(&ls, id), &["HOME", "ROAD"]);
        assert_eq!((composite.map.width(), composite.map.height()), (12, 4));
        // The path of both chunks joins at the border.
        for x in 0..12 {
            assert!(composite.map.can_walk_to(TilePos::new(x, 1)), "x = {x}");
        }
        assert!(composite.map.is_reachable(TilePos::new(1, 1), TilePos::new(10, 1)));
        assert_eq!(composite.level.player_start, TilePos::new(1, 1), "the story chunk's start");
        assert_eq!(composite.level.guide, Some(TilePos::new(2, 1)));
    }

    #[test]
    fn tiles_of_unloaded_chunks_are_blocked_so_no_route_enters_them() {
        let world = ok();
        let ls = levels();
        let only_home = world.compose(&|id: &str| find(&ls, id), &["HOME"]);
        assert_eq!((only_home.map.width(), only_home.map.height()), (12, 4), "the frame never changes size");
        assert!(only_home.map.can_walk_to(TilePos::new(5, 1)));
        assert!(!only_home.map.can_walk_to(TilePos::new(6, 1)));
        assert!(!only_home.map.is_reachable(TilePos::new(1, 1), TilePos::new(9, 1)));
        let only_road = world.compose(&|id: &str| find(&ls, id), &["ROAD"]);
        assert!(only_road.map.can_walk_to(TilePos::new(6, 1)) && !only_road.map.can_walk_to(TilePos::new(5, 1)));
        assert_eq!(only_road.level.guide, None, "the guide lives in the home chunk");
    }

    #[test]
    fn compose_moves_every_kind_of_spawn_by_the_chunk_origin() {
        let road_text = "[map]\nTTTTTT\nTTSTTT\n======\nTTTTTT\n\n[player]\n1 1\n\n[moonpetals]\n0 1\n\n[nodes]\ntree 3 2 3 1\n\n[signs]\n2 2 hello\n";
        let world = parse_with(WORLD, HOME, road_text).unwrap();
        let ls = vec![("HOME", Level::parse(HOME).unwrap()), ("ROAD", Level::parse(road_text).unwrap())];
        let composite = world.compose(&|id: &str| find(&ls, id), &["HOME", "ROAD"]);
        assert_eq!(composite.level.moonpetals.len(), 4, "three from home and one from the road");
        assert!(composite.level.moonpetals.contains(&TilePos::new(6, 1)));
        assert_eq!(composite.level.nodes[0].tile, TilePos::new(9, 2));
        assert_eq!(composite.level.signs[0].tile, TilePos::new(8, 2));
        assert_eq!(composite.level.signs[0].text, "hello");
        assert_eq!(composite.map.tile_at(TilePos::new(9, 2)), Ok(Tile::Tree));
        assert_eq!(composite.map.tile_at(TilePos::new(8, 2)), Ok(Tile::Sign));
    }

    #[test]
    fn changes_round_trip_between_local_and_world_tiles() {
        let world = ok();
        let level = Level::parse(&ROAD.replace("[player]", "[moonpetals]\n1 1\n\n[player]")).unwrap();
        let local = WorldChanges::capture(&level, &[], &[]);
        assert_eq!(local.picked(), [TilePos::new(1, 1)]);
        let in_world = world.changes_to_world("ROAD", &local);
        assert_eq!(in_world.picked(), [TilePos::new(7, 1)]);
        assert_eq!(world.changes_to_local("ROAD", &in_world), local);
        assert!(world.changes_to_world("NOPE", &local).is_empty());
    }

    #[test]
    fn capture_changes_reads_only_the_named_chunk_and_answers_in_world_tiles() {
        let world = ok();
        let road_text = "[map]\nTTTTTT\nTTTTTT\n======\nTTTTTT\n\n[player]\n1 1\n\n[moonpetals]\n1 1\n3 1\n\n[nodes]\ntree 3 2 3 1\n";
        let road = Level::parse(road_text).unwrap();
        // The road's petal at local (1, 1) is gone and its tree has 1 of 3 actions left. The lists
        // also hold things from the home chunk, which must not leak into the road's changes.
        let petals_left = [TilePos::new(9, 1), TilePos::new(1, 1)];
        let nodes_now = [(TilePos::new(9, 2), 1), (TilePos::new(2, 2), 0)];
        let changes = world.capture_changes("ROAD", &road, &petals_left, &nodes_now);
        assert_eq!(changes.picked(), [TilePos::new(7, 1)]);
        assert_eq!(changes.nodes(), [(TilePos::new(9, 2), 1)]);
        assert!(world.capture_changes("NOPE", &road, &petals_left, &nodes_now).is_empty());
        // Applied again in local tiles it is what the save file would hold.
        assert_eq!(world.changes_to_local("ROAD", &changes).picked(), [TilePos::new(1, 1)]);
    }

    fn bad_with(text: &str, home: &str, road: &str) -> String {
        parse_with(text, home, road).unwrap_err().to_string()
    }

    #[test]
    fn chunk_sections_are_checked() {
        assert!(bad(&WORLD.replace("[chunk ROAD]", "[chunk MISSING]")).contains("not in the location register"));
        assert!(bad(&WORLD.replace("[chunk ROAD]", "[chunk road]")).contains("must be a location id"));
        assert!(bad(&WORLD.replace("[chunk ROAD]\nat = 6 0", "[chunk HOME]\nat = 6 0")).contains("placed twice"));
        assert!(bad(&WORLD.replace("at = 6 0", "at = -1 0")).contains("must not be negative"));
        assert!(bad(&WORLD.replace("at = 6 0", "at = 6")).contains("needs 2 whole numbers"));
        assert!(bad(&WORLD.replace("at = 6 0", "at = 6 x")).contains("not a whole number"));
        assert!(bad(&WORLD.replace("at = 6 0", "")).contains("needs an `at = x y` field"));
        assert!(bad(&WORLD.replace("at = 6 0", "at = 6 0\nat = 6 0")).contains("appears twice"));
        assert!(bad(&WORLD.replace("at = 6 0", "at = 6 0\ncolour = red")).contains("unknown chunk field"));
        assert!(bad(&WORLD.replace("[world]", "[planet]")).contains("unknown section"));
        assert!(bad("[world]\n").contains("places no chunks"));
        assert!(bad("load_margin = 3\n").contains("after a section header"));
    }

    #[test]
    fn overlapping_chunks_are_refused_with_the_line_of_the_second() {
        let e = bad(&WORLD.replace("at = 6 0", "at = 5 0"));
        assert!(e.contains("overlap") && e.starts_with("line "), "{e}");
    }

    #[test]
    fn the_start_must_be_placed() {
        let text = "[chunk ROAD]\nat = 0 0\n\n[zone road]\nname = Road\nrect = 0 0 5 3\ntheme = road\n";
        assert!(bad(text).contains("starting place"));
    }

    #[test]
    fn margins_are_validated() {
        assert!(bad(&WORLD.replace("unload_margin = 5", "unload_margin = 2")).contains("must not be smaller"));
        assert!(bad(&WORLD.replace("load_margin = 3", "load_margin = 0").replace("unload_margin = 5", "unload_margin = 0")).contains("at least 1"));
        assert!(bad(&WORLD.replace("load_margin = 3", "load_margin = far")).contains("whole number of tiles"));
        assert!(bad(&WORLD.replace("load_margin = 3", "zoom = 3")).contains("unknown [world] field"));
        assert!(bad(&WORLD.replace("[chunk HOME]", "[world]\n\n[chunk HOME]")).contains("appears twice"));
    }

    #[test]
    fn touching_chunks_need_a_walkable_crossing_when_the_register_links_them() {
        // A gap column between the chunks.
        assert!(bad(&WORLD.replace("at = 6 0", "at = 7 0").replace("rect = 6 0 11 3", "rect = 6 0 12 3")).contains("no path crosses"));
        // Touching, but the road's path is on a different row.
        let shifted = "[map]\nTTTTTT\n======\nTTTTTT\nTTTTTT\n\n[player]\n1 2\n";
        assert!(bad_with(WORLD, HOME, shifted).contains("no path crosses"));
    }

    #[test]
    fn a_crossing_the_register_does_not_know_about_is_refused() {
        let book = Locations::parse(&REGISTER.replace("exits = ROAD\nmap = home.map", "map = home.map")).unwrap();
        let ls = levels();
        let e = Overworld::parse(WORLD, &book, |id| find(&ls, id)).unwrap_err().to_string();
        assert!(e.contains("does not list each as an exit of the other"), "{e}");
    }

    #[test]
    fn an_exit_to_another_chunk_is_refused_but_an_exit_to_a_cave_is_fine() {
        let with_exit = |target: &str| HOME.replace("[player]", &format!("[exits]\n5 1 {target} 1 1 Onward\n\n[player]"));
        let e = bad_with(WORLD, &with_exit("ROAD"), ROAD);
        assert!(e.contains("another overworld chunk") && e.contains("walking"), "{e}");
        assert!(parse_with(WORLD, &with_exit("CAVE"), ROAD).is_ok());
    }

    #[test]
    fn every_chunk_must_be_reachable_on_foot() {
        // The road's path is cut in two and its start is on the far piece.
        let cut = "[map]\nTTTTTT\nTTTTTT\n==TT==\nTTTTTT\n\n[player]\n4 1\n";
        let e = bad_with(WORLD, HOME, cut);
        assert!(e.contains("cannot be reached on foot"), "{e}");
    }

    #[test]
    fn zone_sections_are_checked() {
        assert!(bad(&WORLD.replace("name = Home\n", "")).contains("needs a `name`"));
        assert!(bad(&WORLD.replace("rect = 0 0 5 3", "")).contains("needs a `rect`"));
        assert!(bad(&WORLD.replace("theme = village", "")).contains("would be silent"));
        assert!(bad(&WORLD.replace("theme = village", "theme = jazz")).contains("unknown built-in theme"));
        assert!(bad(&WORLD.replace("music = road.ogg", "music = ../road.ogg")).contains("not a valid track name"));
        assert!(bad(&WORLD.replace("rect = 0 0 5 3", "rect = 5 3 0 0")).contains("lower-left corner first"));
        assert!(bad(&WORLD.replace("rect = 6 0 11 3", "rect = 6 0 12 3")).contains("outside the world"));
        assert!(bad(&WORLD.replace("[zone road]", "[zone home]")).contains("listed twice"));
        assert!(bad(&WORLD.replace("[zone home]", "[zone Home]")).contains("lower-case"));
        assert!(bad(&WORLD.replace("theme = village", "theme = village\nvolume = 3")).contains("unknown zone field"));
        assert!(bad(&WORLD.replace("name = Home", "name =")).contains("is empty"));
    }

    #[test]
    fn every_walkable_tile_must_be_inside_a_zone() {
        let e = bad(&WORLD.replace("rect = 0 0 5 3", "rect = 0 0 4 3"));
        assert!(e.contains("is in no zone") && e.contains("`HOME`"), "{e}");
    }

    #[test]
    fn a_house_id_may_not_repeat_across_chunks() {
        let map = "[map]\n......\n......\n......\n......\n======\n......\n\n[player]\n0 1\n";
        let house = "\n[houses]\nhut 1 2 3 3 south The Hut\n";
        let home = format!("{map}{house}\n[guide]\n5 1\n\n[moonpetals]\n0 1\n3 1\n4 1\n");
        let road = format!("{map}{house}");
        let e = bad_with(WORLD, &home, &road);
        assert!(e.contains("house id `hut`") && e.contains("unique"), "{e}");
    }

    #[test]
    fn zones_in_a_chunk_lists_the_zones_that_overlap_it() {
        let world = ok();
        let ids = |chunk: &str| world.zones_in(chunk).iter().map(|z| z.id.clone()).collect::<Vec<_>>();
        assert_eq!(ids("HOME"), ["home"]);
        assert_eq!(ids("ROAD"), ["road"]);
        assert!(ids("NOPE").is_empty());
    }

    #[test]
    fn zones_with_equal_tracks_compare_equal_so_the_music_is_not_restarted() {
        let world = ok();
        let a = TrackSpec { music: None, theme: Some("village".into()) };
        assert_eq!(world.zones()[0].track, a);
        assert_ne!(world.zones()[0].track, world.zones()[1].track);
    }

    #[test]
    fn the_shipped_overworld_is_valid_and_connects_bluebrook_to_the_road() {
        let world = crate::village::builtin().expect("shipped world loads");
        let overworld = world.overworld.as_ref().expect("the shipped world has an overworld");
        let bluebrook = overworld.chunk("MUNCHKIN_LANDS").unwrap();
        let road = overworld.chunk("YELLOW_BRICK_ROAD_EAST").unwrap();
        assert_eq!(bluebrook.origin, TilePos::new(0, 0));
        assert_eq!(road.origin.x, bluebrook.width, "the road starts where the village ends");
        let level_of = |id: &str| world.level_of(id);
        let composite = overworld.compose(&level_of, &["MUNCHKIN_LANDS", "YELLOW_BRICK_ROAD_EAST"]);
        let start = composite.level.player_start;
        let far_east = road.to_world(TilePos::new(road.width - 3, 20));
        assert!(composite.map.is_reachable(start, far_east), "one continuous walk from the village to the end of the road");
    }
}
