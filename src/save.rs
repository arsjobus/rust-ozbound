//! The full game save as versioned plain text, independent of rendering and of file I/O.
//!
//! Pure Rust (no Bevy, no files): [`SaveGame::to_text`] and [`SaveGame::from_text`] convert
//! between a [`SaveGame`] and a string. `save_store.rs` decides where the text lives and writes
//! it safely. A save holds everything that makes a play session different from a fresh start:
//! where the player stands, the bag and worn gear, skill XP, quest progress, creatures met, and
//! the changes made to the world (moonpetals picked, trees/veins/plants partly or fully used).
//!
//! # Format, version 2
//!
//! One entry per line; blank lines and lines starting with `#` are ignored, and CRLF line
//! endings and a leading BOM are accepted. The first entry is the header and the last must be
//! `end`, so a file cut off mid-write is recognised as such instead of loading as a smaller game.
//!
//! ```text
//! ozbound-save 2
//! region YELLOW_BRICK_ROAD_EAST # optional; the region the player is in (absent = the starting region)
//! world 40 24                 # map width and height (guards against another world's save)
//! player 12 5                 # the tile the player stands on
//! bag_capacity 20             # optional; defaults to the normal bag size
//! bag timber 3                # one line per item with a positive total
//! equip hand walking_staff    # one line per worn item: slot, item
//! skill foraging 120          # optional per skill; missing means 0 XP
//! walked 4                    # tiles walked toward the next Wayfinding point
//! quest moonpetals active 0 2 # quest lines use the quest-log format (see quest_save.rs)
//! seen hare                   # creatures met
//! befriended hare             # creatures befriended (must also be `seen`)
//! picked 10 4                 # the moonpetal on this tile was picked up
//! node 3 4 0                  # the tree/vein/plant on this tile has 0 actions left
//! end
//! ```
//!
//! Version 2 only adds the optional `region` line (version 1 saves have none and load as the
//! starting region). `picked` and `node` lines belong to the saved region's map; changes made
//! in a region the player has since left are not kept (a known limitation: its trees and
//! plants start fresh when it is entered again, and its moonpetals come back).
//! [`SaveGame::peek_region`] reads just that line, so the caller can pick the right level to
//! validate against before loading the rest.
//!
//! # Loading rules
//!//!
//! Loading validates everything against the world the player is about to enter and refuses
//! anything play could not have produced; it never partly succeeds, and every problem names
//! its line. In particular: the header must be `ozbound-save <n>` with `1 <= n <= FORMAT_VERSION`
//! (a newer format is refused with an explanation, never guessed at); `world` must match the
//! current map; the player must stand on a walkable tile that can be reached from the start and
//! that nobody else stands on; items, slots, skills, creatures, and quests must exist; the bag
//! must fit its capacity; gear must fit its slot; XP and walk counts must be possible;
//! `picked` and `node` entries must point at real moonpetals and nodes; nothing may be listed
//! twice. Missing optional entries take their defaults (an empty bag, level-1 skills, nothing
//! met, an untouched world); `world`, `player`, and `end` are required.
//!
//! A tree, vein, or plant saved as spent starts its regrowth timer again when the game loads
//! (timers are not saved), and moonpetals never grow back.
//!
//! # Changing the format
//!
//! Bump [`FORMAT_VERSION`], keep reading every older version (add a migration here, with tests
//! that load a saved sample of each old version), and never reuse a keyword for a new meaning.

use std::collections::HashSet;
use std::str::FromStr;

use crate::bestiary::{Bestiary, FieldNotes};
use crate::equipment::{EquipSlot, Equipment};
use crate::inventory::{Inventory, ItemId};
use crate::level::Level;
use crate::quests::{QuestBook, QuestLog};
use crate::skills::{SkillId, Skills};
use crate::textdata::{is_valid_id, ContentError};
use crate::world::TilePos;

/// The save format this build writes and reads.
pub const FORMAT_VERSION: u32 = 2;

/// The largest bag a save may claim (the normal bag is 20 slots).
pub const MAX_BAG_CAPACITY: usize = 200;

const MAGIC: &str = "ozbound-save";

/// The quest log's own header, used to read the `quest` lines of a save with `QuestLog::from_text`.
const QUEST_HEADER: &str = "ozbound-quests 1";

/// What a save is checked against: the world the player is about to enter.
#[derive(Debug, Clone, Copy)]
pub struct SaveContext<'a> {
    pub level: &'a Level,
    pub quests: &'a QuestBook,
    pub bestiary: &'a Bestiary,
}

/// How the world differs from a fresh start. Both lists are kept sorted (by row, then column)
/// so equal worlds always compare and write equal.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct WorldChanges {
    picked: Vec<TilePos>,
    nodes: Vec<(TilePos, u32)>,
}

fn sort_key(tile: &TilePos) -> (i32, i32) {
    (tile.y, tile.x)
}

impl WorldChanges {
    /// A world nobody has touched.
    pub fn none() -> Self {
        Self::default()
    }

    /// Works out the changes from what is in the world right now: the tiles that still have a
    /// moonpetal, and the actions left on each tree/vein/plant (by tile). A moonpetal of the
    /// level that is no longer there was picked; a node is listed only when it has fewer
    /// actions left than the level gives it. Tiles that are not in the level are ignored.
    pub fn capture(level: &Level, petals_left: &[TilePos], nodes_now: &[(TilePos, u32)]) -> Self {
        let mut picked: Vec<TilePos> = level.moonpetals.iter().filter(|tile| !petals_left.contains(tile)).copied().collect();
        picked.sort_by_key(sort_key);
        picked.dedup();
        let mut nodes: Vec<(TilePos, u32)> = nodes_now
            .iter()
            .filter(|(tile, left)| level.nodes.iter().any(|spawn| spawn.tile == *tile && *left < spawn.actions))
            .copied()
            .collect();
        nodes.sort_by_key(|(tile, _)| sort_key(tile));
        nodes.dedup_by_key(|(tile, _)| *tile);
        Self { picked, nodes }
    }

    /// Whether the moonpetal on `tile` has been picked up.
    pub fn is_picked(&self, tile: TilePos) -> bool {
        self.picked.contains(&tile)
    }

    /// The saved actions left on the node at `tile`, or `None` when it is untouched.
    pub fn node_remaining(&self, tile: TilePos) -> Option<u32> {
        self.nodes.iter().find(|(at, _)| *at == tile).map(|(_, left)| *left)
    }

    /// Tiles whose moonpetal was picked, sorted.
    pub fn picked(&self) -> &[TilePos] {
        &self.picked
    }

    /// Nodes with fewer actions left than they start with, sorted.
    pub fn nodes(&self) -> &[(TilePos, u32)] {
        &self.nodes
    }

    pub fn is_empty(&self) -> bool {
        self.picked.is_empty() && self.nodes.is_empty()
    }
}

/// Everything saved about a play session.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SaveGame {
    /// The region (location id) the player is in; `None` means the starting region.
    pub region: Option<String>,
    /// Width and height of the map the save belongs to.
    pub map_size: (usize, usize),
    pub player: TilePos,
    pub bag: Inventory,
    pub equipment: Equipment,
    pub skills: Skills,
    pub quests: QuestLog,
    pub notes: FieldNotes,
    pub changes: WorldChanges,
}

impl SaveGame {
    /// A new game on `level`: the player at the start, nothing carried, nothing done.
    pub fn new_game(level: &Level) -> Self {
        Self {
            region: None,
            map_size: (level.map.width(), level.map.height()),
            player: level.player_start,
            bag: Inventory::default(),
            equipment: Equipment::new(),
            skills: Skills::new(),
            quests: QuestLog::new(),
            notes: FieldNotes::new(),
            changes: WorldChanges::none(),
        }
    }

    /// Writes the save in the versioned text format. The same game always gives the same text.
    pub fn to_text(&self) -> String {
        let mut out = format!("{MAGIC} {FORMAT_VERSION}\n");
        if let Some(region) = &self.region {
            out.push_str(&format!("region {region}\n"));
        }
        out.push_str(&format!("world {} {}\n", self.map_size.0, self.map_size.1));
        out.push_str(&format!("player {} {}\n", self.player.x, self.player.y));
        out.push_str(&format!("bag_capacity {}\n", self.bag.capacity()));
        for (item, count) in self.bag.stacks() {
            out.push_str(&format!("bag {} {count}\n", item.key()));
        }
        for (slot, item) in self.equipment.iter() {
            out.push_str(&format!("equip {} {}\n", slot.key(), item.key()));
        }
        for skill in SkillId::ALL {
            out.push_str(&format!("skill {} {}\n", skill.key(), self.skills.xp(skill)));
        }
        out.push_str(&format!("walked {}\n", self.skills.walked_progress()));
        // The quest log writes its own header first; the save has its own.
        for line in self.quests.to_text().lines().skip(1) {
            out.push_str(line);
            out.push('\n');
        }
        for id in self.notes.seen() {
            out.push_str(&format!("seen {id}\n"));
        }
        for id in self.notes.befriended() {
            out.push_str(&format!("befriended {id}\n"));
        }
        for tile in self.changes.picked() {
            out.push_str(&format!("picked {} {}\n", tile.x, tile.y));
        }
        for (tile, left) in self.changes.nodes() {
            out.push_str(&format!("node {} {} {left}\n", tile.x, tile.y));
        }
        out.push_str("end\n");
        out
    }

    /// The region named by a save's `region` line, without reading or validating anything else.
    /// `None` when there is no such line (the save is for the starting region) or the text is
    /// not a save at all; loading reports the real problem.
    pub fn peek_region(text: &str) -> Option<String> {
        text.trim_start_matches('\u{feff}').lines().find_map(|line| {
            match line.split_whitespace().collect::<Vec<_>>().as_slice() {
                ["region", id] => Some((*id).to_string()),
                _ => None,
            }
        })
    }

    /// Reads a save written by [`SaveGame::to_text`], validating it against `ctx`.
    pub fn from_text(text: &str, ctx: &SaveContext) -> Result<Self, ContentError> {
        let raw_lines: Vec<&str> = text.trim_start_matches('\u{feff}').lines().collect();
        let mut entries = raw_lines
            .iter()
            .enumerate()
            .map(|(i, raw)| (i + 1, raw.trim()))
            .filter(|(_, line)| !line.is_empty() && !line.starts_with('#'));

        let Some((header_line, header)) = entries.next() else {
            return Err(ContentError::at(0, "the save is empty"));
        };
        read_header(header_line, header)?;

        let mut state = Reading::default();
        let mut last_line = header_line;
        for (number, line) in entries {
            last_line = number;
            if state.end.is_some() {
                return Err(ContentError::at(number, "nothing may follow the `end` line"));
            }
            state.entry(number, line, ctx)?;
        }
        state.finish(last_line, ctx, &raw_lines, header_line)
    }
}

fn read_header(line: usize, header: &str) -> Result<(), ContentError> {
    let version = match header.split_whitespace().collect::<Vec<_>>().as_slice() {
        [magic, version] if *magic == MAGIC => version
            .parse::<u32>()
            .map_err(|_| ContentError::at(line, format!("`{version}` is not a format version number")))?,
        _ => return Err(ContentError::at(line, format!("not an Ozbound save: the first line must be `{MAGIC} <version>`"))),
    };
    if version == 0 {
        return Err(ContentError::at(line, "format version 0 does not exist"));
    }
    if version > FORMAT_VERSION {
        return Err(ContentError::at(
            line,
            format!("this save uses format {version}, but this version of Ozbound reads up to format {FORMAT_VERSION}; update the game to load it"),
        ));
    }
    Ok(())
}

fn parse_num<T: FromStr>(line: usize, what: &str, text: &str) -> Result<T, ContentError> {
    text.parse::<T>().map_err(|_| ContentError::at(line, format!("`{text}` is not a valid {what}")))
}

fn parse_tile(line: usize, x: &str, y: &str) -> Result<TilePos, ContentError> {
    Ok(TilePos::new(parse_num(line, "tile column", x)?, parse_num(line, "tile row", y)?))
}

/// What each entry keyword takes, for the message when its arguments are wrong.
const USAGE: [(&str, &str); 13] = [
    ("region", "<LOCATION_ID>"),
    ("world", "<width> <height>"),
    ("player", "<x> <y>"),
    ("bag_capacity", "<slots>"),
    ("bag", "<item> <count>"),
    ("equip", "<slot> <item>"),
    ("skill", "<skill> <xp>"),
    ("walked", "<tiles>"),
    ("seen", "<creature>"),
    ("befriended", "<creature>"),
    ("picked", "<x> <y>"),
    ("node", "<x> <y> <actions left>"),
    ("end", "nothing after it"),
];

/// Entries gathered while reading, checked against the world once the whole file is read.
#[derive(Default)]
struct Reading {
    region: Option<String>,
    world: Option<(usize, usize)>,
    player: Option<(usize, TilePos)>,
    capacity: Option<(usize, usize)>,
    bag: Vec<(usize, ItemId, u32)>,
    equip: Vec<(usize, EquipSlot, ItemId)>,
    xp: [Option<u32>; 4],
    skill_line: usize,
    walked: Option<(usize, u32)>,
    seen: Vec<(usize, String)>,
    befriended: Vec<(usize, String)>,
    picked: Vec<TilePos>,
    nodes: Vec<(TilePos, u32)>,
    end: Option<usize>,
}

impl Reading {
    fn entry(&mut self, number: usize, line: &str, ctx: &SaveContext) -> Result<(), ContentError> {
        let words: Vec<&str> = line.split_whitespace().collect();
        let once = |taken: bool, name: &str| {
            if taken {
                Err(ContentError::at(number, format!("`{name}` appears twice")))
            } else {
                Ok(())
            }
        };
        match words.as_slice() {
            ["region", id] => {
                once(self.region.is_some(), "region")?;
                if !crate::textdata::is_valid_location_id(id) {
                    return Err(ContentError::at(number, format!("`{id}` is not a location id (capital letters, digits, and underscores)")));
                }
                self.region = Some((*id).to_string());
            }
            ["world", w, h] => {
                once(self.world.is_some(), "world")?;
                let size = (parse_num::<usize>(number, "map width", w)?, parse_num::<usize>(number, "map height", h)?);
                let here = (ctx.level.map.width(), ctx.level.map.height());
                if size != here {
                    return Err(ContentError::at(
                        number,
                        format!("this save was made on a {} x {} map, but this map is {} x {}; it belongs to a different world", size.0, size.1, here.0, here.1),
                    ));
                }
                self.world = Some(size);
            }
            ["player", x, y] => {
                once(self.player.is_some(), "player")?;
                self.player = Some((number, parse_tile(number, x, y)?));
            }
            ["bag_capacity", n] => {
                once(self.capacity.is_some(), "bag_capacity")?;
                let slots = parse_num::<usize>(number, "bag size", n)?;
                if !(1..=MAX_BAG_CAPACITY).contains(&slots) {
                    return Err(ContentError::at(number, format!("the bag size must be from 1 to {MAX_BAG_CAPACITY} slots, found {slots}")));
                }
                self.capacity = Some((number, slots));
            }
            ["bag", key, count] => {
                let item = ItemId::from_key(key).ok_or_else(|| ContentError::at(number, format!("unknown item `{key}`")))?;
                let count = parse_num::<u32>(number, "item count", count)?;
                if count == 0 {
                    return Err(ContentError::at(number, format!("the count of {key} must be at least 1 (leave the item out instead)")));
                }
                if self.bag.iter().any(|(_, seen, _)| *seen == item) {
                    return Err(ContentError::at(number, format!("`{key}` is listed twice in the bag")));
                }
                self.bag.push((number, item, count));
            }
            ["equip", slot, key] => {
                let slot = EquipSlot::from_key(slot).ok_or_else(|| ContentError::at(number, format!("unknown equipment slot `{slot}`")))?;
                let item = ItemId::from_key(key).ok_or_else(|| ContentError::at(number, format!("unknown item `{key}`")))?;
                match item.equip_slot() {
                    None => return Err(ContentError::at(number, format!("the {} cannot be worn or held", item.label()))),
                    Some(right) if right != slot => {
                        return Err(ContentError::at(number, format!("the {} belongs in the {} slot, not {}", item.label(), right.key(), slot.key())));
                    }
                    Some(_) => {}
                }
                if self.equip.iter().any(|(_, taken, _)| *taken == slot) {
                    return Err(ContentError::at(number, format!("the {} slot is listed twice", slot.key())));
                }
                self.equip.push((number, slot, item));
            }
            ["skill", key, xp] => {
                let skill = SkillId::from_key(key)
                    .ok_or_else(|| ContentError::at(number, format!("unknown skill `{key}` (use {})", SkillId::key_list())))?;
                let index = SkillId::ALL.iter().position(|s| *s == skill).unwrap_or(0);
                once(self.xp[index].is_some(), &format!("skill {key}"))?;
                self.xp[index] = Some(parse_num::<u32>(number, "XP amount", xp)?);
                self.skill_line = number;
            }
            ["walked", tiles] => {
                once(self.walked.is_some(), "walked")?;
                self.walked = Some((number, parse_num::<u32>(number, "walked count", tiles)?));
            }
            // Quest lines are read by the quest log itself (see `finish`).
            ["quest", ..] => {}
            [kind @ ("seen" | "befriended"), id] => {
                if !is_valid_id(id) || ctx.bestiary.get(id).is_none() {
                    return Err(ContentError::at(number, format!("unknown creature `{id}`")));
                }
                let list = if *kind == "seen" { &mut self.seen } else { &mut self.befriended };
                if list.iter().any(|(_, taken)| taken == id) {
                    return Err(ContentError::at(number, format!("`{id}` is listed twice under `{kind}`")));
                }
                list.push((number, id.to_string()));
            }
            ["picked", x, y] => {
                let at = parse_tile(number, x, y)?;
                if !ctx.level.moonpetals.contains(&at) {
                    return Err(ContentError::at(number, format!("there is no moonpetal at ({}, {}) in this world", at.x, at.y)));
                }
                if self.picked.contains(&at) {
                    return Err(ContentError::at(number, format!("the moonpetal at ({}, {}) is listed twice", at.x, at.y)));
                }
                self.picked.push(at);
            }
            ["node", x, y, left] => {
                let at = parse_tile(number, x, y)?;
                let left = parse_num::<u32>(number, "number of actions left", left)?;
                let Some(spawn) = ctx.level.nodes.iter().find(|spawn| spawn.tile == at) else {
                    return Err(ContentError::at(number, format!("there is no tree, vein, or plant at ({}, {}) in this world", at.x, at.y)));
                };
                if left > spawn.actions {
                    return Err(ContentError::at(
                        number,
                        format!("the {} at ({}, {}) only has {} actions, so {left} left is impossible", spawn.kind.label(), at.x, at.y, spawn.actions),
                    ));
                }
                if self.nodes.iter().any(|(taken, _)| *taken == at) {
                    return Err(ContentError::at(number, format!("the node at ({}, {}) is listed twice", at.x, at.y)));
                }
                self.nodes.push((at, left));
            }
            ["end"] => self.end = Some(number),
            [keyword, ..] => {
                return Err(match USAGE.iter().find(|(name, _)| name == keyword) {
                    Some((name, usage)) => ContentError::at(number, format!("`{name}` is written `{name} {usage}`, found `{line}`")),
                    None => ContentError::at(number, format!("unknown entry `{keyword}`")),
                });
            }
            [] => {}
        }
        Ok(())
    }

    /// Builds the save once every line has been read, running the checks that need the whole file.
    fn finish(self, last_line: usize, ctx: &SaveContext, raw_lines: &[&str], header_line: usize) -> Result<SaveGame, ContentError> {
        if self.end.is_none() {
            return Err(ContentError::at(last_line, "the save is cut off: the final `end` line is missing"));
        }
        let Some(size) = self.world else {
            return Err(ContentError::at(0, "the save has no `world` line"));
        };
        let Some((player_line, player)) = self.player else {
            return Err(ContentError::at(0, "the save has no `player` line"));
        };
        check_player(player_line, player, ctx)?;

        let (capacity_line, capacity) = self.capacity.unwrap_or((0, Inventory::default().capacity()));
        let bag = Inventory::from_stacks(capacity, self.bag.iter().map(|(_, item, count)| (*item, *count))).map_err(|_| {
            let line = self.bag.last().map_or(capacity_line, |(line, _, _)| *line);
            ContentError::at(line, format!("the bag holds more than its {capacity} slots"))
        })?;
        let equipment = Equipment::from_items(self.equip.iter().map(|(_, slot, item)| (*slot, *item)))
            .map_err(|error| ContentError::at(self.equip.last().map_or(0, |(line, _, _)| *line), error.message()))?;

        let xp = self.xp.map(|value| value.unwrap_or(0));
        let (walked_line, walked) = self.walked.unwrap_or((0, 0));
        let skills = Skills::from_saved(xp, walked)
            .map_err(|message| ContentError::at(if walked_line != 0 { walked_line } else { self.skill_line }, message))?;

        // Quest lines keep their own line numbers: every other line becomes blank, and the save's
        // header line stands in for the quest log's header.
        let quest_text: String = raw_lines
            .iter()
            .enumerate()
            .map(|(i, raw)| {
                let raw = raw.trim();
                if i + 1 == header_line {
                    QUEST_HEADER
                } else if raw.starts_with("quest ") {
                    raw
                } else {
                    ""
                }
            })
            .collect::<Vec<_>>()
            .join("\n");
        let quests = QuestLog::from_text(&quest_text, ctx.quests)?;

        let seen_ids: HashSet<&str> = self.seen.iter().map(|(_, id)| id.as_str()).collect();
        if let Some((line, id)) = self.befriended.iter().find(|(_, id)| !seen_ids.contains(id.as_str())) {
            return Err(ContentError::at(*line, format!("`{id}` is befriended but was never seen")));
        }
        let notes = FieldNotes::from_ids(self.seen.iter().map(|(_, id)| id.clone()), self.befriended.iter().map(|(_, id)| id.clone()))
            .map_err(|message| ContentError::at(0, message))?;

        let mut picked = self.picked;
        picked.sort_by_key(sort_key);
        let mut nodes = self.nodes;
        nodes.sort_by_key(|(tile, _)| sort_key(tile));
        Ok(SaveGame { region: self.region, map_size: size, player, bag, equipment, skills, quests, notes, changes: WorldChanges { picked, nodes } })
    }
}

/// The player must stand somewhere play could have put them.
fn check_player(line: usize, at: TilePos, ctx: &SaveContext) -> Result<(), ContentError> {
    let level = ctx.level;
    if !level.map.can_walk_to(at) {
        return Err(ContentError::at(line, format!("the player cannot stand at ({}, {}): it is outside the map or blocked", at.x, at.y)));
    }
    if !level.map.is_reachable(level.player_start, at) {
        return Err(ContentError::at(line, format!("the player cannot be at ({}, {}): it cannot be reached from the starting tile", at.x, at.y)));
    }
    let occupant = if level.guide == Some(at) {
        Some("the village guide".to_string())
    } else if let Some(npc) = level.npcs.iter().find(|npc| npc.tile == at) {
        Some(npc.name.clone())
    } else {
        level.creatures.iter().find(|creature| creature.tile == at).map(|creature| format!("a creature ({})", creature.id))
    };
    match occupant {
        Some(who) => Err(ContentError::at(line, format!("the player cannot be at ({}, {}): {who} stands there", at.x, at.y))),
        None => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gathering::ResourceKind;
    use crate::quests::QuestEvent;
    use crate::village::{builtin, World};

    fn world() -> World {
        builtin().expect("the shipped world loads")
    }

    fn ctx(world: &World) -> SaveContext<'_> {
        SaveContext { level: &world.level, quests: &world.content.quests, bestiary: &world.bestiary }
    }

    /// A walkable tile next to the start that nothing stands on.
    fn nearby_tile(world: &World) -> TilePos {
        let start = world.level.player_start;
        [(1, 0), (-1, 0), (0, 1), (0, -1), (2, 0), (0, 2)]
            .into_iter()
            .map(|(dx, dy)| TilePos::new(start.x + dx, start.y + dy))
            .find(|t| world.level.map.can_walk_to(*t) && check_player(0, *t, &ctx(world)).is_ok())
            .expect("a free tile near the start")
    }

    fn first_node(world: &World, kind: ResourceKind) -> crate::level::NodeSpawn {
        *world.level.nodes.iter().find(|n| n.kind == kind).expect("the world has this node kind")
    }

    /// A game with something in every part of the save.
    fn busy_game(world: &World) -> SaveGame {
        let mut game = SaveGame::new_game(&world.level);
        game.player = nearby_tile(world);
        game.bag = Inventory::from_stacks(20, [(ItemId::Timber, 150), (ItemId::Moonpetal, 2), (ItemId::MintTea, 1)]).unwrap();
        game.equipment = Equipment::from_items([(EquipSlot::Hand, ItemId::WalkingStaff), (EquipSlot::Charm, ItemId::CopperCharm)]).unwrap();
        game.skills = Skills::from_saved([31, 130, 0, 12], 7).unwrap();
        game.quests.start(&world.content.quests, "moonpetals").unwrap();
        game.quests.record(&world.content.quests, &QuestEvent::Gathered { item: ItemId::Moonpetal, quantity: 2 });
        game.notes = FieldNotes::from_ids(["pond_heron", "meadow_hare"], ["meadow_hare"]).unwrap();
        let tree = first_node(world, ResourceKind::Tree);
        let herb = first_node(world, ResourceKind::HerbPatch);
        game.changes = WorldChanges::capture(
            &world.level,
            &world.level.moonpetals[1..],
            &[(tree.tile, 0), (herb.tile, herb.actions.saturating_sub(1))],
        );
        game
    }

    fn load(text: &str, world: &World) -> Result<SaveGame, ContentError> {
        SaveGame::from_text(text, &ctx(world))
    }

    fn err(text: &str) -> ContentError {
        load(text, &world()).unwrap_err()
    }

    /// Replaces the first line that starts with `prefix` (and the whole text if none does).
    fn with_line(text: &str, prefix: &str, replacement: &str) -> String {
        let mut done = false;
        text.lines()
            .map(|line| {
                if !done && line.starts_with(prefix) {
                    done = true;
                    replacement.to_string()
                } else {
                    line.to_string()
                }
            })
            .collect::<Vec<_>>()
            .join("\n")
            + "\n"
    }

    /// Adds `extra` just before `end`.
    fn with_extra(text: &str, extra: &str) -> String {
        text.replace("end\n", &format!("{extra}\nend\n"))
    }

    fn line_of(text: &str, prefix: &str) -> usize {
        text.lines().position(|l| l.starts_with(prefix)).expect("line exists") + 1
    }

    // ---- round trips ----

    #[test]
    fn a_new_game_round_trips_and_reads_like_the_docs() {
        let world = world();
        let game = SaveGame::new_game(&world.level);
        let text = game.to_text();
        assert!(text.starts_with(&format!("ozbound-save 2\nworld {} {}\nplayer ", world.level.map.width(), world.level.map.height())), "{text}");
        assert!(text.ends_with("end\n"));
        assert_eq!(load(&text, &world).unwrap(), game);
        assert!(game.changes.is_empty());
    }

    #[test]
    fn every_part_of_a_busy_game_round_trips() {
        let world = world();
        let game = busy_game(&world);
        let text = game.to_text();
        let loaded = load(&text, &world).unwrap();
        assert_eq!(loaded, game);
        assert_eq!(loaded.bag.count(ItemId::Timber), 150);
        assert_eq!(loaded.bag.used_slots(), 4, "150 timber take two slots");
        assert_eq!(loaded.equipment.get(EquipSlot::Hand), Some(ItemId::WalkingStaff));
        assert_eq!(loaded.skills.walked_progress(), 7);
        assert_eq!(loaded.quests.current_objective("moonpetals"), Some((0, 2)));
        assert!(loaded.notes.is_befriended("meadow_hare") && loaded.notes.has_seen("pond_heron"));
        assert!(loaded.changes.is_picked(world.level.moonpetals[0]));
        assert!(!loaded.changes.is_picked(world.level.moonpetals[1]));
    }

    #[test]
    fn writing_is_deterministic_and_stable_across_a_reload() {
        let world = world();
        let text = busy_game(&world).to_text();
        assert_eq!(text, busy_game(&world).to_text());
        assert_eq!(load(&text, &world).unwrap().to_text(), text);
    }

    #[test]
    fn play_continues_after_a_load() {
        let world = world();
        let mut loaded = load(&busy_game(&world).to_text(), &world).unwrap();
        loaded.quests.record(&world.content.quests, &QuestEvent::Gathered { item: ItemId::Moonpetal, quantity: 1 });
        loaded.bag.add(ItemId::Honeyberry, 1).unwrap();
        loaded.skills.add_xp(SkillId::Herbalism, 5);
        let again = load(&loaded.to_text(), &world).unwrap();
        assert_eq!(again, loaded);
    }

    // ---- defaults and tolerant reading ----

    #[test]
    fn a_minimal_save_takes_the_defaults() {
        let world = world();
        let start = world.level.player_start;
        let text = format!("ozbound-save 1\nworld {} {}\nplayer {} {}\nend\n", world.level.map.width(), world.level.map.height(), start.x, start.y);
        let game = load(&text, &world).unwrap();
        assert_eq!(game, SaveGame::new_game(&world.level));
        assert_eq!(game.bag.capacity(), Inventory::default().capacity());
        assert_eq!(game.skills.level(SkillId::Woodcraft), 1);
    }

    #[test]
    fn comments_blank_lines_crlf_and_a_bom_are_accepted() {
        let world = world();
        let text = busy_game(&world).to_text();
        let messy = format!("\u{feff}# my save\r\n\r\n{}", text.replace('\n', "\r\n").replace("walked", "# walked below\r\nwalked"));
        assert_eq!(load(&messy, &world).unwrap(), busy_game(&world));
    }

    // ---- headers and versions ----

    #[test]
    fn empty_and_foreign_files_are_refused() {
        assert!(err("").message.contains("empty"));
        assert!(err("\n# only a comment\n").message.contains("empty"));
        let e = err("hello world\n");
        assert_eq!(e.line, Some(1));
        assert!(e.message.contains("not an Ozbound save"), "{e}");
        let quests_only = err("ozbound-quests 1\n");
        assert!(quests_only.message.contains("not an Ozbound save"), "{quests_only}");
    }

    #[test]
    fn bad_zero_and_newer_versions_are_refused_with_a_reason() {
        let world = world();
        let good = busy_game(&world).to_text();
        let e = load(&good.replacen("ozbound-save 2", "ozbound-save one", 1), &world).unwrap_err();
        assert!(e.message.contains("not a format version"), "{e}");
        let e = load(&good.replacen("ozbound-save 2", "ozbound-save 0", 1), &world).unwrap_err();
        assert!(e.message.contains("version 0"), "{e}");
        let e = load(&good.replacen("ozbound-save 2", "ozbound-save 3", 1), &world).unwrap_err();
        assert_eq!(e.line, Some(1));
        assert!(e.message.contains("format 3") && e.message.contains("up to format 2") && e.message.contains("update"), "{e}");
    }

    #[test]
    fn a_version_one_save_still_loads_as_the_starting_region() {
        let world = world();
        let start = world.level.player_start;
        let v1 = format!("ozbound-save 1\nworld {} {}\nplayer {} {}\nend\n", world.level.map.width(), world.level.map.height(), start.x, start.y);
        let game = load(&v1, &world).expect("version 1 saves must keep loading");
        assert_eq!(game.region, None);
        assert_eq!(SaveGame::peek_region(&v1), None);
        // Writing it back upgrades it to the current version and changes nothing else.
        let again = load(&game.to_text(), &world).unwrap();
        assert_eq!(again, game);
        assert!(game.to_text().starts_with("ozbound-save 2\n"));
    }

    #[test]
    fn the_region_line_round_trips_and_is_only_written_away_from_the_start() {
        let world = world();
        let mut game = busy_game(&world);
        assert!(!game.to_text().contains("region"), "the starting region writes no region line");
        game.region = Some("YELLOW_BRICK_ROAD_EAST".to_string());
        let text = game.to_text();
        assert!(text.contains("\nregion YELLOW_BRICK_ROAD_EAST\n"));
        assert_eq!(SaveGame::peek_region(&text).as_deref(), Some("YELLOW_BRICK_ROAD_EAST"));
        assert_eq!(load(&text, &world).unwrap().region.as_deref(), Some("YELLOW_BRICK_ROAD_EAST"));
    }

    #[test]
    fn a_bad_or_repeated_region_line_is_refused_with_its_line() {
        let world = world();
        let text = busy_game(&world).to_text();
        let with = |line: &str| text.replacen("world ", &format!("{line}\nworld "), 1);
        let e = load(&with("region road"), &world).unwrap_err();
        assert!(e.message.contains("not a location id"), "{e}");
        assert_eq!(e.line, Some(2));
        let e = load(&with("region A_B\nregion C_D"), &world).unwrap_err();
        assert!(e.message.contains("`region` appears twice"), "{e}");
        let e = load(&with("region"), &world).unwrap_err();
        assert!(e.message.contains("<LOCATION_ID>"), "{e}");
    }

    #[test]
    fn peek_region_ignores_everything_but_the_region_line() {
        assert_eq!(SaveGame::peek_region(""), None);
        assert_eq!(SaveGame::peek_region("not a save\nregion  A_B \n"), Some("A_B".to_string()));
        assert_eq!(SaveGame::peek_region("\u{feff}ozbound-save 2\nregion A_B\n"), Some("A_B".to_string()));
        assert_eq!(SaveGame::peek_region("region A B\n"), None, "wrong shape");
        assert_eq!(SaveGame::peek_region("regional A_B\n"), None);
    }

    #[test]
    fn the_version_the_game_writes_is_the_version_it_reads() {
        assert!(SaveGame::new_game(&world().level).to_text().starts_with(&format!("{MAGIC} {FORMAT_VERSION}\n")));
    }

    // ---- structure ----

    #[test]
    fn a_save_cut_off_before_end_is_refused() {
        let world = world();
        let text = busy_game(&world).to_text();
        let cut = text.trim_end().strip_suffix("end").unwrap().to_string();
        let e = load(&cut, &world).unwrap_err();
        assert!(e.message.contains("cut off"), "{e}");
        assert!(e.line.is_some());
        // Cutting the file at any line boundary is always caught.
        let lines: Vec<&str> = text.lines().collect();
        for keep in 1..lines.len() {
            let partial = lines[..keep].join("\n");
            assert!(load(&partial, &world).is_err(), "kept {keep} lines");
        }
    }

    #[test]
    fn nothing_may_follow_end() {
        let world = world();
        let text = busy_game(&world).to_text() + "bag timber 1\n";
        let e = load(&text, &world).unwrap_err();
        assert!(e.message.contains("follow"), "{e}");
        assert_eq!(e.line, Some(text.lines().count()));
    }

    #[test]
    fn world_and_player_are_required() {
        let world = world();
        let text = busy_game(&world).to_text();
        let no_world = text.lines().filter(|l| !l.starts_with("world")).collect::<Vec<_>>().join("\n");
        assert!(load(&no_world, &world).unwrap_err().message.contains("`world`"));
        let no_player = text.lines().filter(|l| !l.starts_with("player")).collect::<Vec<_>>().join("\n");
        assert!(load(&no_player, &world).unwrap_err().message.contains("`player`"));
    }

    #[test]
    fn unknown_entries_and_wrong_arguments_name_the_line() {
        let world = world();
        let text = busy_game(&world).to_text();
        let e = load(&with_extra(&text, "teleport 1 2"), &world).unwrap_err();
        assert!(e.message.contains("unknown entry `teleport`"), "{e}");
        assert_eq!(e.line, Some(line_of(&text, "end")));
        let e = load(&with_line(&text, "player", "player 3"), &world).unwrap_err();
        assert_eq!(e.line, Some(line_of(&text, "player")));
        assert!(e.message.contains("player <x> <y>"), "{e}");
        let e = load(&with_line(&text, "player", "player x 3"), &world).unwrap_err();
        assert!(e.message.contains("not a valid tile column"), "{e}");
        let e = load(&with_extra(&text, "end now"), &world).unwrap_err();
        assert!(e.message.contains("end"), "{e}");
    }

    #[test]
    fn single_entries_cannot_repeat() {
        let world = world();
        let text = busy_game(&world).to_text();
        for (prefix, name) in [("world", "world"), ("player", "player"), ("bag_capacity", "bag_capacity"), ("walked", "walked"), ("skill foraging", "skill foraging")] {
            let line = text.lines().find(|l| l.starts_with(prefix)).unwrap();
            let e = load(&with_extra(&text, line), &world).unwrap_err();
            assert!(e.message.contains(&format!("`{name}` appears twice")), "{prefix}: {e}");
        }
    }

    // ---- the world it belongs to ----

    #[test]
    fn a_save_from_another_map_is_refused_with_both_sizes() {
        let world = world();
        let text = busy_game(&world).to_text();
        let e = load(&with_line(&text, "world", "world 20 12"), &world).unwrap_err();
        assert!(e.message.contains("20 x 12") && e.message.contains("different world"), "{e}");
        assert_eq!(e.line, Some(line_of(&text, "world")));
    }

    #[test]
    fn the_player_must_stand_on_a_free_walkable_reachable_tile() {
        let world = world();
        let text = busy_game(&world).to_text();
        let level = &world.level;
        let water = (0..level.map.height() as i32)
            .flat_map(|y| (0..level.map.width() as i32).map(move |x| TilePos::new(x, y)))
            .find(|t| level.map.tile_at(*t).is_ok_and(|tile| tile == crate::world::Tile::Water))
            .expect("Bluebrook has a river");
        for (label, tile, word) in [
            ("water", water, "blocked"),
            ("off the map", TilePos::new(-1, 0), "blocked"),
            ("far off the map", TilePos::new(9999, 9999), "blocked"),
            ("the guide", level.guide.expect("the shipped story level has a guide"), "guide"),
            ("a villager", level.npcs[0].tile, &level.npcs[0].name),
            ("a creature", level.creatures[0].tile, "creature"),
        ] {
            let e = load(&with_line(&text, "player", &format!("player {} {}", tile.x, tile.y)), &world).unwrap_err();
            assert!(e.message.contains(word), "{label}: {e}");
            assert_eq!(e.line, Some(line_of(&text, "player")), "{label}");
        }
    }

    #[test]
    fn every_walkable_tile_the_player_can_reach_is_accepted() {
        let world = world();
        let level = &world.level;
        let text = SaveGame::new_game(level).to_text();
        let mut checked = 0;
        for y in 0..level.map.height() as i32 {
            for x in 0..level.map.width() as i32 {
                let tile = TilePos::new(x, y);
                let free = level.guide != Some(tile) && level.npcs.iter().all(|n| n.tile != tile) && level.creatures.iter().all(|c| c.tile != tile);
                if free && level.map.can_walk_to(tile) && level.map.is_reachable(level.player_start, tile) {
                    let game = load(&with_line(&text, "player", &format!("player {x} {y}")), &world).unwrap();
                    assert_eq!(game.player, tile);
                    checked += 1;
                }
            }
        }
        assert!(checked > 100, "only {checked} tiles checked");
    }

    // ---- bag, gear, skills, creatures ----

    #[test]
    fn bag_problems_are_refused_at_their_line() {
        let world = world();
        let text = busy_game(&world).to_text();
        let cases = [
            ("bag timber", "bag rubies 3", "unknown item `rubies`"),
            ("bag timber", "bag timber 0", "at least 1"),
            ("bag timber", "bag timber many", "item count"),
            ("bag timber", "bag timber -2", "item count"),
            ("bag_capacity", "bag_capacity 0", "from 1 to 200"),
            ("bag_capacity", "bag_capacity 201", "from 1 to 200"),
            ("bag_capacity", "bag_capacity lots", "bag size"),
            ("bag_capacity", "bag_capacity 2", "more than its 2 slots"),
        ];
        for (prefix, replacement, expected) in cases {
            let e = load(&with_line(&text, prefix, replacement), &world).unwrap_err();
            assert!(e.message.contains(expected), "{replacement}: {e}");
            assert!(e.line.is_some(), "{replacement}");
        }
        let e = load(&with_extra(&text, "bag timber 5"), &world).unwrap_err();
        assert!(e.message.contains("listed twice in the bag"), "{e}");
        assert_eq!(e.line, Some(line_of(&text, "end")));
    }

    #[test]
    fn gear_problems_are_refused_at_their_line() {
        let world = world();
        let text = busy_game(&world).to_text();
        let cases = [
            ("equip hand", "equip hand timber", "cannot be worn"),
            ("equip hand", "equip head walking_staff", "belongs in the hand slot"),
            ("equip hand", "equip wings walking_staff", "unknown equipment slot"),
            ("equip hand", "equip hand wand", "unknown item"),
        ];
        for (prefix, replacement, expected) in cases {
            let e = load(&with_line(&text, prefix, replacement), &world).unwrap_err();
            assert!(e.message.contains(expected), "{replacement}: {e}");
            assert_eq!(e.line, Some(line_of(&text, prefix)), "{replacement}");
        }
        let e = load(&with_extra(&text, "equip hand walking_staff"), &world).unwrap_err();
        assert!(e.message.contains("listed twice"), "{e}");
    }

    #[test]
    fn skill_problems_are_refused_at_their_line() {
        let world = world();
        let text = busy_game(&world).to_text();
        let e = load(&with_line(&text, "skill foraging", "skill sorcery 5"), &world).unwrap_err();
        assert!(e.message.contains("unknown skill `sorcery`") && e.message.contains("wayfinding"), "{e}");
        let e = load(&with_line(&text, "skill foraging", "skill foraging 9999"), &world).unwrap_err();
        assert!(e.message.contains("above the maximum"), "{e}");
        assert_eq!(e.line, Some(line_of(&text, "walked")), "range errors are reported on the last skill-related entry");
        let e = load(&with_line(&text, "walked", "walked 10"), &world).unwrap_err();
        assert!(e.message.contains("walked count"), "{e}");
        assert_eq!(e.line, Some(line_of(&text, "walked")));
        let e = load(&with_line(&text, "skill wayfinding", "skill wayfinding 120"), &world).unwrap_err();
        assert!(e.message.contains("not kept"), "a walked count at the walking ceiling is impossible: {e}");
    }

    #[test]
    fn creature_problems_are_refused_at_their_line() {
        let world = world();
        let text = busy_game(&world).to_text();
        let e = load(&with_extra(&text, "seen dragon"), &world).unwrap_err();
        assert!(e.message.contains("unknown creature `dragon`"), "{e}");
        let e = load(&with_extra(&text, "seen meadow_hare"), &world).unwrap_err();
        assert!(e.message.contains("listed twice"), "{e}");
        let e = load(&with_extra(&text, "befriended lantern_moth"), &world).unwrap_err();
        assert!(e.message.contains("never seen"), "{e}");
        assert_eq!(e.line, Some(line_of(&text, "end")));
        let e = load(&with_extra(&text, "seen Not-An-Id"), &world).unwrap_err();
        assert!(e.message.contains("unknown creature"), "{e}");
    }

    // ---- quests keep their real line numbers ----

    #[test]
    fn quest_errors_point_at_the_line_in_the_save() {
        let world = world();
        let text = busy_game(&world).to_text();
        let e = load(&with_extra(&text, "quest dragon_slaying active 0 0"), &world).unwrap_err();
        assert!(e.message.contains("unknown quest"), "{e}");
        assert_eq!(e.line, Some(line_of(&text, "end")));
        let line = line_of(&text, "quest moonpetals");
        let e = load(&with_line(&text, "quest moonpetals", "quest moonpetals active 0 99"), &world).unwrap_err();
        assert_eq!(e.line, Some(line));
        let e = load(&with_line(&text, "quest moonpetals", "quest moonpetals sleeping"), &world).unwrap_err();
        assert_eq!(e.line, Some(line));
        let moonpetals = text.lines().find(|l| l.starts_with("quest moonpetals")).unwrap();
        let e = load(&with_extra(&text, moonpetals), &world).unwrap_err();
        assert!(e.message.contains("twice"), "{e}");
    }

    #[test]
    fn a_quest_started_without_its_prerequisite_is_refused() {
        let world = world();
        let book = &world.content.quests;
        let (dependent, prerequisite) = world
            .content
            .quests
            .iter()
            .find_map(|q| q.prerequisites.first().map(|p| (q.id.clone(), p.clone())))
            .expect("Bluebrook has a quest with a prerequisite");
        let text = busy_game(&world).to_text();
        assert!(book.get(&prerequisite).is_some());
        let e = load(&with_extra(&text, &format!("quest {dependent} active 0 0")), &world).unwrap_err();
        assert!(e.message.contains("prerequisite"), "{e}");
    }

    // ---- world changes ----

    #[test]
    fn world_changes_must_point_at_real_things() {
        let world = world();
        let text = busy_game(&world).to_text();
        let petal = world.level.moonpetals[0];
        let e = load(&with_extra(&text, &format!("picked {} {}", petal.x, petal.y)), &world).unwrap_err();
        assert!(e.message.contains("twice"), "{e}");
        let e = load(&with_extra(&text, "picked 0 0"), &world).unwrap_err();
        assert!(e.message.contains("no moonpetal"), "{e}");
        let e = load(&with_extra(&text, "node 0 0 0"), &world).unwrap_err();
        assert!(e.message.contains("no tree, vein, or plant"), "{e}");
        let berry = first_node(&world, ResourceKind::BerryBush);
        let e = load(&with_extra(&text, &format!("node {} {} {}", berry.tile.x, berry.tile.y, berry.actions + 1)), &world).unwrap_err();
        assert!(e.message.contains("impossible"), "{e}");
        let e = load(&with_extra(&text, &format!("node {} {} lots", berry.tile.x, berry.tile.y)), &world).unwrap_err();
        assert!(e.message.contains("actions left"), "{e}");
        let tree = first_node(&world, ResourceKind::Tree);
        let e = load(&with_extra(&text, &format!("node {} {} 0", tree.tile.x, tree.tile.y)), &world).unwrap_err();
        assert!(e.message.contains("twice"), "{e}");
    }

    #[test]
    fn capture_lists_only_what_differs_and_sorts_it() {
        let world = world();
        let level = &world.level;
        let nodes: Vec<(TilePos, u32)> = level.nodes.iter().map(|n| (n.tile, n.actions)).collect();
        assert!(WorldChanges::capture(level, &level.moonpetals, &nodes).is_empty(), "an untouched world has no changes");

        let mut used = nodes.clone();
        let (last, first) = (used.len() - 1, 0);
        used[last].1 = 0;
        used[first].1 = level.nodes[first].actions - 1;
        let changes = WorldChanges::capture(level, &[], &used);
        assert_eq!(changes.picked().len(), level.moonpetals.len(), "no petals left means all were picked");
        assert_eq!(changes.nodes().len(), 2);
        assert_eq!(changes.node_remaining(used[last].0), Some(0));
        assert_eq!(changes.node_remaining(used[first].0), Some(used[first].1));
        assert_eq!(changes.node_remaining(used[1].0), None);
        let keys: Vec<(i32, i32)> = changes.nodes().iter().map(|(t, _)| (t.y, t.x)).collect();
        assert!(keys.windows(2).all(|w| w[0] <= w[1]), "sorted by row then column");
        // The same state in any order gives the same changes.
        let mut shuffled = used.clone();
        shuffled.reverse();
        assert_eq!(WorldChanges::capture(level, &[], &shuffled), changes);
    }

    #[test]
    fn capture_ignores_tiles_that_are_not_in_the_level_and_duplicates() {
        let world = world();
        let level = &world.level;
        let stray = TilePos::new(0, 0);
        assert!(level.nodes.iter().all(|n| n.tile != stray));
        let tree = first_node(&world, ResourceKind::Tree);
        let changes = WorldChanges::capture(level, &level.moonpetals, &[(stray, 0), (tree.tile, 0), (tree.tile, 0)]);
        assert_eq!(changes.nodes(), &[(tree.tile, 0)]);
        assert!(changes.picked().is_empty());
    }

    #[test]
    fn an_untouched_node_is_accepted_even_when_written_out() {
        let world = world();
        let tree = first_node(&world, ResourceKind::Tree);
        let text = with_extra(&SaveGame::new_game(&world.level).to_text(), &format!("node {} {} {}", tree.tile.x, tree.tile.y, tree.actions));
        let game = load(&text, &world).unwrap();
        assert_eq!(game.changes.node_remaining(tree.tile), Some(tree.actions));
        // Writing it again drops the redundant line only after a capture; the loaded value is kept as read.
        assert_eq!(load(&game.to_text(), &world).unwrap(), game);
    }
}
