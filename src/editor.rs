//! The world editor's rules: what can be selected, moved, removed, and placed in a scene, how
//! each edit is undone and redone, how edits are kept between sessions, and what the ribbon
//! and the extended right-click menu offer.
//!
//! Pure Rust (no Bevy). The Bevy layer (`editor_ui.rs`) only reads the mouse, draws the ribbon,
//! and rebuilds a scene after an edit; every decision is made here and unit-tested.
//!
//! # The model
//!
//! An edit changes a region's [`Level`] (the data a scene is built from), never an entity, so
//! the world after an edit is exactly the world a restart would build. An edit names the region
//! (the scene) it happens in and works in that region's own tiles; an object is moved *within*
//! its scene. The object kinds that can be edited are the ones a player can already click or
//! talk to: the guide and villagers, creatures, moonpetals, resource nodes (trees, copper
//! veins, herb patches, berry bushes), readable signs, and thickets (the dense bushes).
//!
//! * **Move** an object to another tile. Trees, copper veins, and signs *are* their tile (the
//!   map holds a tree or rock or signpost there), so they move onto free grass and leave grass
//!   behind (a thicket is the same: a tree tile of the map with no chopping tree on it). Everything else moves onto free walkable ground.
//! * **Remove** an object (a tree leaves grass behind). Everything it took with it, such as a
//!   villager's behavior line, is kept so that undo puts it back exactly, in the same place in
//!   the level's lists.
//! * **Place** an asset from the library (see `asset_file.rs`): a tree, vein, plant, moonpetal,
//!   or sign. People and creatures are not placed, because they need dialogue or a bestiary
//!   entry to be part of the story.
//!
//! # Safety
//!
//! Every edit is tried on a copy of the level and only kept if the copy is still a good level
//! ([`check_level`]: everything reachable, nothing overlapping, doorways clear, enough
//! moonpetals) and the caller's wider check passes (the game passes the whole-world check of
//! `village::check`, so no edit can leave a quest nobody can finish). A refused edit changes
//! nothing and says why.
//!
//! # History and persistence
//!
//! [`History`] keeps each applied edit with its inverse, so undo and redo are exact and a new
//! edit forgets what was undone. The edits that were done (in order) are the *edit layer*:
//! [`EditLayer`] writes them as `ozbound-edits 1` text and [`replay`] applies them over the
//! shipped levels at start-up, all or nothing, so the shipped level files are never rewritten.

use std::collections::HashSet;
use std::fmt;
use std::path::Path;

use crate::asset_file::{Asset, Body, Library};
use crate::gathering::ResourceKind;
use crate::level::{BehaviorSpawn, CreatureSpawn, Level, NodeSpawn, NpcSpawn, SignSpawn, GUIDE_ID};
use crate::npc::{self, Behavior};
use crate::pathfinding::Approach;
use crate::quest::MoonpetalQuest;
use crate::textdata::{is_valid_id, is_valid_location_id};
use crate::world::{Tile, TilePos, WorldMap};

/// Every region's level by location id (the shape of `Atlas::levels`).
pub type Levels = Vec<(String, Level)>;

/// The caller's wider check of an edited level (region id, new level). `Ok` keeps the edit.
pub type Check<'a> = &'a dyn Fn(&str, &Level) -> Result<(), String>;

/// How many edits are remembered for undo.
pub const HISTORY_LIMIT: usize = 200;

// ---------------------------------------------------------------------------------------------
// Objects
// ---------------------------------------------------------------------------------------------

/// Names one object of a level: people and creatures by id, the rest by what and where.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ObjectRef {
    Guide,
    Npc(String),
    Creature(String),
    Moonpetal(TilePos),
    Node(ResourceKind, TilePos),
    Sign(TilePos),
    /// A tree tile that is not a chopping tree: the dense bush drawn as a thicket.
    Thicket(TilePos),
}

/// A full copy of one object, with what it needs to be put back.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Object {
    Guide { tile: TilePos, behavior: Option<Behavior> },
    Npc { spawn: NpcSpawn, behavior: Option<Behavior> },
    Creature(CreatureSpawn),
    Moonpetal(TilePos),
    Node(NodeSpawn),
    Sign(SignSpawn),
    Thicket(TilePos),
}

/// An object taken out of a level, with where it stood in the level's lists so that putting it
/// back restores the level exactly.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Removed {
    pub object: Object,
    pub index: usize,
    pub behavior_index: Option<usize>,
}

impl ObjectRef {
    /// The text form used in the edit layer file: `guide`, `npc id`, `creature id`,
    /// `moonpetal x y`, `node <kind> x y`, `sign x y`, `thicket x y`.
    pub fn to_text(&self) -> String {
        match self {
            Self::Guide => "guide".into(),
            Self::Npc(id) => format!("npc {id}"),
            Self::Creature(id) => format!("creature {id}"),
            Self::Moonpetal(t) => format!("moonpetal {} {}", t.x, t.y),
            Self::Node(kind, t) => format!("node {} {} {}", node_word(*kind), t.x, t.y),
            Self::Sign(t) => format!("sign {} {}", t.x, t.y),
            Self::Thicket(t) => format!("thicket {} {}", t.x, t.y),
        }
    }

    /// Reads the words of [`ObjectRef::to_text`]; all of them must be used.
    pub fn parse(words: &[&str]) -> Result<Self, String> {
        let int = |w: &str| w.parse::<i32>().map_err(|_| format!("`{w}` is not a whole number"));
        let tile = |x: &str, y: &str| Ok::<_, String>(TilePos::new(int(x)?, int(y)?));
        let wrong = || format!("`{}` is not an object (use guide, npc <id>, creature <id>, moonpetal <x> <y>, node <kind> <x> <y>, sign <x> <y>, or thicket <x> <y>)", words.join(" "));
        let found = match words {
            ["guide"] => Self::Guide,
            ["npc", id] if is_valid_id(id) => Self::Npc((*id).to_string()),
            ["creature", id] if is_valid_id(id) => Self::Creature((*id).to_string()),
            ["moonpetal", x, y] => Self::Moonpetal(tile(x, y)?),
            ["node", kind, x, y] => Self::Node(node_from_word(kind).ok_or_else(|| format!("unknown node kind `{kind}` (use tree, copper, herb, or berry)"))?, tile(x, y)?),
            ["sign", x, y] => Self::Sign(tile(x, y)?),
            ["thicket", x, y] => Self::Thicket(tile(x, y)?),
            _ => return Err(wrong()),
        };
        Ok(found)
    }
}

fn node_word(kind: ResourceKind) -> &'static str {
    match kind {
        ResourceKind::Tree => "tree",
        ResourceKind::CopperVein => "copper",
        ResourceKind::HerbPatch => "herb",
        ResourceKind::BerryBush => "berry",
        ResourceKind::Moonpetal => "moonpetal",
    }
}

fn node_from_word(word: &str) -> Option<ResourceKind> {
    [ResourceKind::Tree, ResourceKind::CopperVein, ResourceKind::HerbPatch, ResourceKind::BerryBush].into_iter().find(|k| node_word(*k) == word)
}

/// The tile a node, sign, or object is drawn on, as the map must hold it.
fn tile_of_node(kind: ResourceKind) -> Option<Tile> {
    match kind {
        ResourceKind::Tree => Some(Tile::Tree),
        ResourceKind::CopperVein => Some(Tile::Rock),
        _ => None,
    }
}

impl Object {
    pub fn reference(&self) -> ObjectRef {
        match self {
            Self::Guide { .. } => ObjectRef::Guide,
            Self::Npc { spawn, .. } => ObjectRef::Npc(spawn.id.clone()),
            Self::Creature(c) => ObjectRef::Creature(c.id.clone()),
            Self::Moonpetal(t) => ObjectRef::Moonpetal(*t),
            Self::Node(n) => ObjectRef::Node(n.kind, n.tile),
            Self::Sign(s) => ObjectRef::Sign(s.tile),
            Self::Thicket(t) => ObjectRef::Thicket(*t),
        }
    }

    pub fn tile(&self) -> TilePos {
        match self {
            Self::Guide { tile, .. } => *tile,
            Self::Npc { spawn, .. } => spawn.tile,
            Self::Creature(c) => c.tile,
            Self::Moonpetal(t) => *t,
            Self::Node(n) => n.tile,
            Self::Sign(s) => s.tile,
            Self::Thicket(t) => *t,
        }
    }

    /// What to call it in messages.
    pub fn name(&self) -> String {
        match self {
            Self::Guide { .. } => "the Village Guide".into(),
            Self::Npc { spawn, .. } => spawn.name.clone(),
            Self::Creature(c) => c.id.replace('_', " "),
            Self::Moonpetal(_) => "moonpetal".into(),
            Self::Node(n) => n.kind.label().into(),
            Self::Sign(_) => "signpost".into(),
            Self::Thicket(_) => "thicket".into(),
        }
    }

    /// True for the objects whose place in the world is a tile of the map (a tree, a rock, a
    /// signpost): moving or removing one changes the map.
    pub fn is_tile_bound(&self) -> bool {
        matches!(self, Self::Sign(_) | Self::Thicket(_)) || matches!(self, Self::Node(n) if tile_of_node(n.kind).is_some())
    }

    /// The asset file this object would be exported as. Signs are not exported: each carries
    /// its own text and a single `signpost` asset would be overwritten by it. `creature_name`
    /// is the bestiary's name for a creature.
    pub fn to_asset(&self, creature_name: Option<&str>) -> Option<Asset> {
        match self {
            Self::Guide { behavior, .. } => Asset::guide(&behavior.clone().unwrap_or_default()),
            Self::Npc { spawn, behavior } => Some(Asset::from_npc(spawn, &behavior.clone().unwrap_or_default())),
            Self::Creature(c) => Some(Asset::from_creature(c, creature_name.unwrap_or(&c.id))),
            Self::Moonpetal(_) => Some(Asset::moonpetal()),
            Self::Node(n) => Some(Asset::from_node(n)),
            Self::Sign(_) | Self::Thicket(_) => None,
        }
    }

    /// The same object standing on `tile`.
    fn at(&self, tile: TilePos) -> Object {
        let mut moved = self.clone();
        match &mut moved {
            Self::Guide { tile: t, .. } => *t = tile,
            Self::Npc { spawn, .. } => spawn.tile = tile,
            Self::Creature(c) => c.tile = tile,
            Self::Moonpetal(t) => *t = tile,
            Self::Node(n) => n.tile = tile,
            Self::Sign(s) => s.tile = tile,
            Self::Thicket(t) => *t = tile,
        }
        moved
    }
}

/// Finds the object a reference names.
pub fn find(level: &Level, object: &ObjectRef) -> Option<Object> {
    let behavior = |id: &str| level.behaviors.iter().find(|b| b.id == id).map(|b| b.behavior.clone());
    match object {
        ObjectRef::Guide => level.guide.map(|tile| Object::Guide { tile, behavior: behavior(GUIDE_ID) }),
        ObjectRef::Npc(id) => level.npcs.iter().find(|n| n.id == *id).map(|spawn| Object::Npc { spawn: spawn.clone(), behavior: behavior(id) }),
        ObjectRef::Creature(id) => level.creatures.iter().find(|c| c.id == *id).cloned().map(Object::Creature),
        ObjectRef::Moonpetal(tile) => level.moonpetals.contains(tile).then_some(Object::Moonpetal(*tile)),
        ObjectRef::Node(kind, tile) => level.nodes.iter().find(|n| n.kind == *kind && n.tile == *tile).copied().map(Object::Node),
        ObjectRef::Sign(tile) => level.signs.iter().find(|s| s.tile == *tile).cloned().map(Object::Sign),
        ObjectRef::Thicket(tile) => is_thicket(level, *tile).then_some(Object::Thicket(*tile)),
    }
}

/// True when `tile` is a thicket: a tree tile of the map with no chopping tree on it (the same
/// rule as `scene::thicket_tiles`, which decides what is drawn as one).
pub fn is_thicket(level: &Level, tile: TilePos) -> bool {
    level.map.tile_at(tile) == Ok(Tile::Tree) && !level.nodes.iter().any(|n| n.tile == tile)
}

/// The editable object standing on `tile`, if there is one (the first match: nothing in a valid
/// level shares a tile, except a place or plant with a moonpetal, which are not both editable
/// here).
pub fn object_at(level: &Level, tile: TilePos) -> Option<Object> {
    let candidates = [
        level.guide.filter(|t| *t == tile).map(|_| ObjectRef::Guide),
        level.npcs.iter().find(|n| n.tile == tile).map(|n| ObjectRef::Npc(n.id.clone())),
        level.creatures.iter().find(|c| c.tile == tile).map(|c| ObjectRef::Creature(c.id.clone())),
        level.nodes.iter().find(|n| n.tile == tile).map(|n| ObjectRef::Node(n.kind, tile)),
        level.signs.iter().find(|s| s.tile == tile).map(|_| ObjectRef::Sign(tile)),
        level.moonpetals.iter().find(|t| **t == tile).map(|_| ObjectRef::Moonpetal(tile)),
        is_thicket(level, tile).then_some(ObjectRef::Thicket(tile)),
    ];
    candidates.into_iter().flatten().find_map(|r| find(level, &r))
}

fn set_tile(level: &mut Level, tile: TilePos, to: Tile) {
    // Every caller has already checked the tile is on the map.
    let _ = level.map.set_tile(tile, to);
}

/// Takes an object out of the level (a tree, rock, or sign leaves grass behind).
fn take(level: &mut Level, object: &ObjectRef) -> Option<Removed> {
    let found = find(level, object)?;
    let (index, behavior_index);
    match &found {
        Object::Guide { .. } => {
            index = 0;
            level.guide = None;
            behavior_index = take_behavior(level, GUIDE_ID);
        }
        Object::Npc { spawn, .. } => {
            index = level.npcs.iter().position(|n| n.id == spawn.id)?;
            level.npcs.remove(index);
            behavior_index = take_behavior(level, &spawn.id);
        }
        Object::Creature(c) => {
            index = level.creatures.iter().position(|x| x.id == c.id)?;
            level.creatures.remove(index);
            behavior_index = None;
        }
        Object::Moonpetal(t) => {
            index = level.moonpetals.iter().position(|x| x == t)?;
            level.moonpetals.remove(index);
            behavior_index = None;
        }
        Object::Node(n) => {
            index = level.nodes.iter().position(|x| x == n)?;
            level.nodes.remove(index);
            behavior_index = None;
            if tile_of_node(n.kind).is_some() {
                set_tile(level, n.tile, Tile::Grass);
            }
        }
        Object::Sign(s) => {
            index = level.signs.iter().position(|x| x == s)?;
            level.signs.remove(index);
            behavior_index = None;
            set_tile(level, s.tile, Tile::Grass);
        }
        Object::Thicket(t) => {
            index = 0;
            behavior_index = None;
            set_tile(level, *t, Tile::Grass);
        }
    }
    Some(Removed { object: found, index, behavior_index })
}

fn take_behavior(level: &mut Level, id: &str) -> Option<usize> {
    let at = level.behaviors.iter().position(|b| b.id == id)?;
    level.behaviors.remove(at);
    Some(at)
}

fn insert_at<T>(list: &mut Vec<T>, index: usize, value: T) {
    let at = index.min(list.len());
    list.insert(at, value);
}

/// Puts an object back where it stood in the level's lists.
fn put(level: &mut Level, removed: &Removed) {
    let behavior_entry = |id: &str, behavior: &Option<Behavior>| behavior.clone().map(|behavior| BehaviorSpawn { id: id.to_string(), behavior });
    match &removed.object {
        Object::Guide { tile, behavior } => {
            level.guide = Some(*tile);
            if let (Some(entry), Some(at)) = (behavior_entry(GUIDE_ID, behavior), removed.behavior_index) {
                insert_at(&mut level.behaviors, at, entry);
            }
        }
        Object::Npc { spawn, behavior } => {
            insert_at(&mut level.npcs, removed.index, spawn.clone());
            if let (Some(entry), Some(at)) = (behavior_entry(&spawn.id, behavior), removed.behavior_index) {
                insert_at(&mut level.behaviors, at, entry);
            }
        }
        Object::Creature(c) => insert_at(&mut level.creatures, removed.index, c.clone()),
        Object::Moonpetal(t) => insert_at(&mut level.moonpetals, removed.index, *t),
        Object::Node(n) => {
            insert_at(&mut level.nodes, removed.index, *n);
            if let Some(tile) = tile_of_node(n.kind) {
                set_tile(level, n.tile, tile);
            }
        }
        Object::Sign(s) => {
            insert_at(&mut level.signs, removed.index, s.clone());
            set_tile(level, s.tile, Tile::Sign);
        }
        Object::Thicket(t) => set_tile(level, *t, Tile::Tree),
    }
}

// ---------------------------------------------------------------------------------------------
// Rules about a level
// ---------------------------------------------------------------------------------------------

fn neighbours(tile: TilePos) -> [TilePos; 4] {
    [(1, 0), (-1, 0), (0, 1), (0, -1)].map(|(dx, dy)| TilePos::new(tile.x + dx, tile.y + dy))
}

fn has_reachable_neighbour(map: &WorldMap, start: TilePos, tile: TilePos) -> bool {
    neighbours(tile).iter().any(|n| map.is_reachable(start, *n))
}

/// Tiles that people and plants may not share: where something already stands on open ground.
/// Trees, rocks, and signs are not listed because the map already blocks those tiles.
fn occupied(level: &Level) -> HashSet<TilePos> {
    let mut tiles: HashSet<TilePos> = HashSet::new();
    tiles.insert(level.player_start);
    tiles.extend(level.guide);
    tiles.extend(level.npcs.iter().map(|n| n.tile));
    tiles.extend(level.creatures.iter().map(|c| c.tile));
    tiles.extend(level.places.iter().map(|p| p.tile));
    tiles.extend(level.moonpetals.iter().copied());
    tiles.extend(level.nodes.iter().filter(|n| n.kind.interaction().approach() == Approach::OnTile).map(|n| n.tile));
    tiles.extend(level.exits.iter().map(|e| e.tile));
    tiles.extend(level.houses.iter().map(|h| h.door_tile()));
    tiles
}

/// Checks that a level is still a level the game can play, with the same rules the level file
/// parser applies: every object reachable, a wanderer with room to roam, doorways clear, and
/// enough moonpetals in a story level.
pub fn check_level(level: &Level) -> Result<(), String> {
    let map = &level.map;
    let start = level.player_start;
    let reach = |what: &str, tile: TilePos, on_tile: bool| -> Result<(), String> {
        let ok = if on_tile { map.is_reachable(start, tile) } else { has_reachable_neighbour(map, start, tile) };
        if ok {
            Ok(())
        } else {
            Err(format!("the {what} at ({}, {}) could no longer be reached on foot", tile.x, tile.y))
        }
    };
    if let Some(guide) = level.guide {
        reach("guide", guide, true)?;
    }
    for npc in &level.npcs {
        reach(&npc.name, npc.tile, true)?;
    }
    for place in &level.places {
        reach("place", place.tile, true)?;
    }
    for petal in &level.moonpetals {
        reach("moonpetal", *petal, true)?;
    }
    for node in &level.nodes {
        reach(node.kind.label(), node.tile, node.kind.interaction().approach() == Approach::OnTile)?;
    }
    for sign in &level.signs {
        reach("signpost", sign.tile, false)?;
    }
    for creature in &level.creatures {
        reach("creature", creature.tile, false)?;
    }
    for exit in &level.exits {
        reach("exit", exit.tile, true)?;
    }
    let need = MoonpetalQuest::default().required() as usize;
    if level.is_story_level() && level.moonpetals.len() < need {
        return Err(format!("the quest needs {need} moonpetals in the world, and only {} would be left", level.moonpetals.len()));
    }
    crate::homestead::check_doors(map, &level.houses, start).map_err(|(_, error)| error.to_string())?;
    let keep_out: HashSet<TilePos> = level.npc_keep_out().into_iter().collect();
    let people: Vec<(&str, TilePos)> = level.guide.iter().map(|t| (GUIDE_ID, *t)).chain(level.npcs.iter().map(|n| (n.id.as_str(), n.tile))).collect();
    for entry in &level.behaviors {
        let Some((_, home)) = people.iter().find(|(id, _)| *id == entry.id) else { continue };
        if let Some(radius) = entry.behavior.radius() {
            if npc::wander_area(map, *home, radius, &keep_out) == 0 {
                return Err(format!("`{}` would have nowhere to wander at ({}, {})", entry.id, home.x, home.y));
            }
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------------------------
// Edits
// ---------------------------------------------------------------------------------------------

/// One change to one region (scene).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Edit {
    Move { region: String, object: ObjectRef, to: TilePos },
    Remove { region: String, object: ObjectRef },
    /// Places the library asset `asset` on `tile`.
    Place { region: String, asset: String, tile: TilePos },
    /// Puts back what a `Remove` took. Only ever the inverse of a removal, so it is never
    /// written to the edit layer.
    Restore { region: String, removed: Removed },
}

impl Edit {
    pub fn region(&self) -> &str {
        match self {
            Self::Move { region, .. } | Self::Remove { region, .. } | Self::Place { region, .. } | Self::Restore { region, .. } => region,
        }
    }

    /// Whether the edit is saved in the edit layer (everything but the internal `Restore`).
    pub fn is_persistent(&self) -> bool {
        !matches!(self, Self::Restore { .. })
    }
}

/// Why an edit was not made.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EditError {
    UnknownRegion(String),
    /// The object is not (or no longer) in the scene.
    NotThere,
    UnknownAsset(String),
    /// The asset cannot be placed by the editor.
    NotPlaceable(String),
    /// The target tile is wrong for this object; the text says how.
    Refused(String),
    /// The edit would leave the scene or the world broken; the text says what.
    Breaks(String),
    NothingToUndo,
    NothingToRedo,
}

impl fmt::Display for EditError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnknownRegion(r) => write!(f, "There is no scene called {r}."),
            Self::NotThere => write!(f, "That is no longer there."),
            Self::UnknownAsset(a) => write!(f, "There is no asset called {a} in the library."),
            Self::NotPlaceable(a) => write!(f, "{a} cannot be placed by the editor: people and creatures need dialogue or a bestiary entry."),
            Self::Refused(why) => write!(f, "{why}"),
            Self::Breaks(why) => write!(f, "That would break the world: {why}."),
            Self::NothingToUndo => write!(f, "Nothing to undo."),
            Self::NothingToRedo => write!(f, "Nothing to redo."),
        }
    }
}

impl std::error::Error for EditError {}

fn refuse<T>(why: impl Into<String>) -> Result<T, EditError> {
    Err(EditError::Refused(why.into()))
}

/// Checks that `to` is a tile this object may stand on, in `level` (with the object still in it).
fn check_target(level: &Level, object: &Object, to: TilePos) -> Result<(), EditError> {
    let Ok(found) = level.map.tile_at(to) else { return refuse("That tile is off the map.") };
    if to == object.tile() {
        return refuse("It is already there.");
    }
    let taken = occupied(level);
    // A creature may share a tile with a waymark; nothing else may share a tile with anything.
    let busy = taken.contains(&to) && !(matches!(object, Object::Creature(_)) && level.places.iter().any(|p| p.tile == to));
    match object {
        Object::Node(n) if tile_of_node(n.kind).is_some() => {
            if found != Tile::Grass {
                return refuse(format!("A {} can only stand on bare grass.", n.kind.label()));
            }
            if busy {
                return refuse("Something already stands there.");
            }
        }
        Object::Thicket(_) => {
            if found != Tile::Grass {
                return refuse("A thicket can only grow on bare grass.");
            }
            if busy {
                return refuse("Something already stands there.");
            }
        }
        Object::Sign(_) => {
            if found != Tile::Grass {
                return refuse("A signpost can only stand on bare grass.");
            }
            if busy {
                return refuse("Something already stands there.");
            }
        }
        Object::Node(n) => {
            if !matches!(found, Tile::Grass | Tile::Path | Tile::Floor) {
                return refuse(format!("A {} needs open ground.", n.kind.label()));
            }
            if busy {
                return refuse("Something already stands there.");
            }
        }
        _ => {
            if !found.is_walkable() {
                return refuse("That tile cannot be walked on.");
            }
            if busy {
                return refuse("Something already stands there.");
            }
        }
    }
    Ok(())
}

/// Applies an edit to a copy of the level and returns the copy and the edit that undoes it.
fn try_edit(level: &Level, edit: &Edit, library: &Library) -> Result<(Level, Edit, String), EditError> {
    let region = edit.region().to_string();
    let mut next = level.clone();
    match edit {
        Edit::Move { object, to, .. } => {
            let found = find(level, object).ok_or(EditError::NotThere)?;
            check_target(level, &found, *to)?;
            let from = found.tile();
            let removed = take(&mut next, object).ok_or(EditError::NotThere)?;
            let moved = removed.object.at(*to);
            put(&mut next, &Removed { object: moved.clone(), ..removed });
            // Putting a tile-bound object back writes the tile it needs; the old tile is grass.
            let inverse = Edit::Move { region, object: moved.reference(), to: from };
            Ok((next, inverse, format!("Moved {}.", found.name())))
        }
        Edit::Remove { object, .. } => {
            let removed = take(&mut next, object).ok_or(EditError::NotThere)?;
            let message = format!("Removed {}.", removed.object.name());
            Ok((next, Edit::Restore { region, removed }, message))
        }
        Edit::Place { asset, tile, .. } => {
            let found = library.get(asset).ok_or_else(|| EditError::UnknownAsset(asset.clone()))?;
            if !found.is_placeable() {
                return Err(EditError::NotPlaceable(found.name.clone()));
            }
            let object = match &found.body {
                Body::Node { .. } => Object::Node(found.node_spawn(*tile).expect("a node asset makes a node")),
                Body::Moonpetal => Object::Moonpetal(*tile),
                Body::Sign { .. } => Object::Sign(found.sign_spawn(*tile).expect("a sign asset makes a sign")),
                _ => return Err(EditError::NotPlaceable(found.name.clone())),
            };
            // Check the tile as if the object were arriving from the neighbouring tile.
            let probe = object.at(TilePos::new(tile.x.wrapping_add(1_000_000), tile.y));
            check_target(level, &probe, *tile)?;
            if let Object::Moonpetal(t) = &object {
                if level.moonpetals.contains(t) {
                    return refuse("A moonpetal already grows there.");
                }
            }
            let appended = Removed { object: object.clone(), index: usize::MAX, behavior_index: None };
            put(&mut next, &appended);
            let message = format!("Placed {}.", object.name());
            Ok((next, Edit::Remove { region, object: object.reference() }, message))
        }
        Edit::Restore { removed, .. } => {
            let tile = removed.object.tile();
            if level.map.tile_at(tile).is_err() {
                return refuse("That tile is off the map.");
            }
            put(&mut next, removed);
            let message = format!("Put back {}.", removed.object.name());
            Ok((next, Edit::Remove { region, object: removed.object.reference() }, message))
        }
    }
}

/// Applies one edit to `levels`. On success returns the edit that undoes it and a message for
/// the chat; on any refusal `levels` is unchanged.
pub fn apply_edit(levels: &mut Levels, edit: &Edit, library: &Library, check: Check) -> Result<(Edit, String), EditError> {
    let region = edit.region();
    let slot = levels.iter().position(|(id, _)| id == region).ok_or_else(|| EditError::UnknownRegion(region.to_string()))?;
    let (next, inverse, message) = try_edit(&levels[slot].1, edit, library)?;
    check_level(&next).map_err(EditError::Breaks)?;
    check(region, &next).map_err(EditError::Breaks)?;
    levels[slot].1 = next;
    Ok((inverse, message))
}

// ---------------------------------------------------------------------------------------------
// History
// ---------------------------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq)]
struct Applied {
    edit: Edit,
    inverse: Edit,
    message: String,
}

/// Every edit made this session, ready to undo and redo.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct History {
    done: Vec<Applied>,
    undone: Vec<Applied>,
}

impl History {
    /// Makes an edit. A new edit forgets what was undone.
    pub fn apply(&mut self, levels: &mut Levels, edit: Edit, library: &Library, check: Check) -> Result<String, EditError> {
        let (inverse, message) = apply_edit(levels, &edit, library, check)?;
        self.undone.clear();
        self.done.push(Applied { edit, inverse, message: message.clone() });
        if self.done.len() > HISTORY_LIMIT {
            self.done.remove(0);
        }
        Ok(message)
    }

    pub fn undo(&mut self, levels: &mut Levels, library: &Library, check: Check) -> Result<String, EditError> {
        let last = self.done.last().ok_or(EditError::NothingToUndo)?;
        let (_, message) = apply_edit(levels, &last.inverse, library, check)?;
        let applied = self.done.pop().expect("there was a last edit");
        let said = format!("Undid: {}", applied.message);
        let _ = message;
        self.undone.push(applied);
        Ok(said)
    }

    pub fn redo(&mut self, levels: &mut Levels, library: &Library, check: Check) -> Result<String, EditError> {
        let next = self.undone.last().ok_or(EditError::NothingToRedo)?;
        let (_, _) = apply_edit(levels, &next.edit, library, check)?;
        let applied = self.undone.pop().expect("there was an undone edit");
        let said = format!("Redid: {}", applied.message);
        self.done.push(applied);
        Ok(said)
    }

    pub fn can_undo(&self) -> bool {
        !self.done.is_empty()
    }

    pub fn can_redo(&self) -> bool {
        !self.undone.is_empty()
    }

    /// What undo would take back, for the ribbon.
    pub fn undo_label(&self) -> Option<&str> {
        self.done.last().map(|a| a.message.as_str())
    }

    pub fn redo_label(&self) -> Option<&str> {
        self.undone.last().map(|a| a.message.as_str())
    }

    pub fn len(&self) -> usize {
        self.done.len()
    }

    pub fn is_empty(&self) -> bool {
        self.done.is_empty()
    }

    /// The edits that are in effect, oldest first: the edit layer to save.
    pub fn layer(&self) -> EditLayer {
        EditLayer { edits: self.done.iter().map(|a| a.edit.clone()).filter(Edit::is_persistent).collect() }
    }
}

// ---------------------------------------------------------------------------------------------
// The edit layer file
// ---------------------------------------------------------------------------------------------

/// The newest layer format this code reads and the one it writes.
pub const LAYER_VERSION: u32 = 1;
const LAYER_HEADER: &str = "ozbound-edits";
/// Where the edit layer is kept (override with `OZBOUND_EDITS`).
pub const LAYER_PATH: &str = "saves/edits.txt";

/// The edits that turn the shipped levels into the player's levels.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct EditLayer {
    pub edits: Vec<Edit>,
}

impl EditLayer {
    /// `ozbound-edits 1`, then one line per edit: `move REGION <object> to x y`,
    /// `remove REGION <object>`, or `place REGION asset x y`.
    pub fn to_text(&self) -> String {
        let mut text = format!("{LAYER_HEADER} {LAYER_VERSION}\n");
        for edit in &self.edits {
            match edit {
                Edit::Move { region, object, to } => text.push_str(&format!("move {region} {} to {} {}\n", object.to_text(), to.x, to.y)),
                Edit::Remove { region, object } => text.push_str(&format!("remove {region} {}\n", object.to_text())),
                Edit::Place { region, asset, tile } => text.push_str(&format!("place {region} {asset} {} {}\n", tile.x, tile.y)),
                Edit::Restore { .. } => {}
            }
        }
        text
    }

    /// Reads layer text. Every error names its line; nothing loads partly.
    pub fn parse(text: &str) -> Result<Self, String> {
        let mut header = false;
        let mut edits = Vec::new();
        for (index, raw) in text.trim_start_matches('\u{feff}').lines().enumerate() {
            let number = index + 1;
            let line = raw.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let words: Vec<&str> = line.split_whitespace().collect();
            let at = |why: String| format!("line {number}: {why}");
            if !header {
                if words.len() != 2 || words[0] != LAYER_HEADER {
                    return Err(at(format!("an edit file starts with `{LAYER_HEADER} {LAYER_VERSION}`")));
                }
                let version: u32 = words[1].parse().map_err(|_| at("the header needs a version number".into()))?;
                if version == 0 {
                    return Err(at("format version 0 does not exist".into()));
                }
                if version > LAYER_VERSION {
                    return Err(at(format!("this edit file is format version {version}, newer than this game understands ({LAYER_VERSION})")));
                }
                header = true;
                continue;
            }
            let int = |w: &str| w.parse::<i32>().map_err(|_| at(format!("`{w}` is not a whole number")));
            let region = |w: &str| if is_valid_location_id(w) { Ok(w.to_string()) } else { Err(at(format!("`{w}` is not a scene id (like YELLOW_BRICK_ROAD_EAST)"))) };
            let edit = match words.as_slice() {
                ["move", r, rest @ .., "to", x, y] if !rest.is_empty() => Edit::Move { region: region(r)?, object: ObjectRef::parse(rest).map_err(at)?, to: TilePos::new(int(x)?, int(y)?) },
                ["remove", r, rest @ ..] if !rest.is_empty() => Edit::Remove { region: region(r)?, object: ObjectRef::parse(rest).map_err(at)? },
                ["place", r, asset, x, y] if is_valid_id(asset) => Edit::Place { region: region(r)?, asset: (*asset).to_string(), tile: TilePos::new(int(x)?, int(y)?) },
                [word, ..] if !["move", "remove", "place"].contains(word) => return Err(at(format!("unknown edit `{word}` (use move, remove, or place)"))),
                _ => return Err(at("expected `move REGION <object> to x y`, `remove REGION <object>`, or `place REGION asset x y`".into())),
            };
            edits.push(edit);
        }
        if !header {
            return Err(format!("the file is empty (it must start with `{LAYER_HEADER} {LAYER_VERSION}`)"));
        }
        Ok(Self { edits })
    }
}

/// Applies a saved layer over `levels`, as the edits were made. All or nothing: on an error
/// `levels` is untouched, and the error names the edit (counting from 1) that failed.
pub fn replay(levels: &mut Levels, layer: &EditLayer, library: &Library, check: Check) -> Result<History, String> {
    let mut working = levels.clone();
    let mut history = History::default();
    for (index, edit) in layer.edits.iter().enumerate() {
        history.apply(&mut working, edit.clone(), library, check).map_err(|error| format!("edit {} ({}): {error}", index + 1, edit_summary(edit)))?;
    }
    *levels = working;
    Ok(history)
}

fn edit_summary(edit: &Edit) -> String {
    match edit {
        Edit::Move { region, object, to } => format!("move {} in {region} to {},{}", object.to_text(), to.x, to.y),
        Edit::Remove { region, object } => format!("remove {} in {region}", object.to_text()),
        Edit::Place { region, asset, tile } => format!("place {asset} in {region} at {},{}", tile.x, tile.y),
        Edit::Restore { region, .. } => format!("restore in {region}"),
    }
}

/// What reading the edit layer file gave.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Loaded {
    /// No file: nothing to apply.
    Absent,
    Layer(EditLayer),
    /// The file exists but cannot be used; it is left as it is.
    Unusable(String),
}

pub fn load_layer(path: &Path) -> Loaded {
    match std::fs::read_to_string(path) {
        Ok(text) => match EditLayer::parse(&text) {
            Ok(layer) => Loaded::Layer(layer),
            Err(error) => Loaded::Unusable(format!("{}: {error}", path.display())),
        },
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Loaded::Absent,
        Err(error) => Loaded::Unusable(format!("cannot read {}: {error}", path.display())),
    }
}

/// Writes the layer through a temporary file and a rename, so a failed write never leaves half
/// a file. An empty layer removes the file (there is nothing to keep).
pub fn save_layer(path: &Path, layer: &EditLayer) -> Result<(), String> {
    if layer.edits.is_empty() {
        return match std::fs::remove_file(path) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(error) => Err(format!("cannot remove {}: {error}", path.display())),
        };
    }
    if let Some(dir) = path.parent().filter(|d| !d.as_os_str().is_empty()) {
        std::fs::create_dir_all(dir).map_err(|error| format!("cannot create {}: {error}", dir.display()))?;
    }
    let temp = path.with_extension("txt.tmp");
    let write = || -> std::io::Result<()> {
        use std::io::Write;
        let mut file = std::fs::File::create(&temp)?;
        file.write_all(layer.to_text().as_bytes())?;
        file.sync_all()?;
        std::fs::rename(&temp, path)
    };
    write().map_err(|error| {
        let _ = std::fs::remove_file(&temp);
        format!("cannot save {}: {error}", path.display())
    })
}

// ---------------------------------------------------------------------------------------------
// Tools, selection, and what a click means
// ---------------------------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tool {
    /// Clicking an object selects it; nothing moves.
    Select,
    /// Clicking an object selects it; clicking free ground then moves it there.
    SelectMove,
    /// Clicking free ground places the chosen library asset.
    Place,
}

/// The object that is selected, in its scene.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Selection {
    pub region: String,
    pub object: ObjectRef,
}

/// What a click on free ground does right now.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GroundClick {
    Nothing,
    MoveSelection(Selection),
    PlaceAsset(String),
}

/// The editor's own state: whether it is on, which tool is active, what is selected, and which
/// library asset the Place tool uses.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EditorState {
    pub active: bool,
    pub tool: Tool,
    pub selection: Option<Selection>,
    /// Index into the list of placeable assets.
    pub place_index: usize,
}

impl Default for EditorState {
    fn default() -> Self {
        Self { active: false, tool: Tool::SelectMove, selection: None, place_index: 0 }
    }
}

impl EditorState {
    /// `/edit`: turns the editor on or off. Turning it off drops the selection.
    pub fn toggle(&mut self) -> bool {
        self.active = !self.active;
        if !self.active {
            self.selection = None;
        }
        self.active
    }

    pub fn choose(&mut self, tool: Tool) {
        self.tool = tool;
    }

    /// Cycles the Place tool to the next placeable asset (and chooses the Place tool).
    pub fn next_asset(&mut self, count: usize) {
        self.tool = Tool::Place;
        self.place_index = if count == 0 { 0 } else { (self.place_index + 1) % count };
    }

    pub fn select(&mut self, region: &str, object: ObjectRef) {
        self.selection = Some(Selection { region: region.to_string(), object });
    }

    pub fn clear_selection(&mut self) {
        self.selection = None;
    }

    /// Follows a selected object that has moved, so the selection never points at nothing.
    pub fn follow(&mut self, edit: &Edit) {
        let Some(selection) = &mut self.selection else { return };
        if let Edit::Move { region, object, to } = edit {
            if *region == selection.region && *object == selection.object {
                selection.object = match object {
                    ObjectRef::Moonpetal(_) => ObjectRef::Moonpetal(*to),
                    ObjectRef::Node(kind, _) => ObjectRef::Node(*kind, *to),
                    ObjectRef::Sign(_) => ObjectRef::Sign(*to),
                    other => other.clone(),
                };
            }
        }
    }

    /// Drops the selection when the object it names is gone.
    pub fn prune(&mut self, levels: &Levels) {
        let gone = self.selection.as_ref().is_some_and(|s| !levels.iter().any(|(id, level)| *id == s.region && find(level, &s.object).is_some()));
        if gone {
            self.selection = None;
        }
    }

    /// What a left-click on free ground means with the current tool and selection.
    pub fn ground_click(&self, placeable: &[&Asset]) -> GroundClick {
        match self.tool {
            Tool::Select => GroundClick::Nothing,
            Tool::SelectMove => self.selection.clone().map_or(GroundClick::Nothing, GroundClick::MoveSelection),
            Tool::Place => placeable.get(self.place_index % placeable.len().max(1)).map_or(GroundClick::Nothing, |a| GroundClick::PlaceAsset(a.id.clone())),
        }
    }
}

/// The ribbon's buttons, left to right.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Button {
    Select,
    SelectMove,
    Place,
    Undo,
    Redo,
    Export,
    Exit,
}

impl Button {
    pub const ALL: [Button; 7] = [Self::Select, Self::SelectMove, Self::Place, Self::Undo, Self::Redo, Self::Export, Self::Exit];
}

/// How one button looks right now.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RibbonItem {
    pub button: Button,
    pub label: String,
    pub enabled: bool,
    /// The tool in use.
    pub active: bool,
}

/// What the ribbon shows. `place_label` is the asset the Place tool would place.
pub fn ribbon(state: &EditorState, history: &History, place_label: Option<&str>) -> Vec<RibbonItem> {
    Button::ALL
        .iter()
        .map(|&button| {
            let (label, enabled, active) = match button {
                Button::Select => ("Select".to_string(), true, state.tool == Tool::Select),
                Button::SelectMove => ("Select & Move".to_string(), true, state.tool == Tool::SelectMove),
                Button::Place => (format!("Place: {}", place_label.unwrap_or("nothing")), place_label.is_some(), state.tool == Tool::Place),
                Button::Undo => ("Undo".to_string(), history.can_undo(), false),
                Button::Redo => ("Redo".to_string(), history.can_redo(), false),
                Button::Export => ("Export asset".to_string(), state.selection.is_some(), false),
                Button::Exit => ("Exit editor".to_string(), true, false),
            };
            RibbonItem { button, label, enabled, active }
        })
        .collect()
}

#[cfg(test)]
mod tests;
