//! Standalone, reusable asset files for the things that stand in the world.
//!
//! Pure Rust (no Bevy). Every kind of object (a tree, a copper vein, a herb patch, a berry
//! bush, a moonpetal, a signpost) and every person and creature has its own small plain-text
//! file under `assets/library/`, named after its id (`tree.asset`, `apothecary.asset`). A level
//! file says *where* things stand; an asset file says *what* they are, so the same definition
//! can be placed in any scene and exported from one.
//!
//! ```text
//! ozbound-asset 1
//! id = tree
//! kind = tree          # tree | copper | herb | berry | moonpetal | sign | npc | creature
//! name = Tree
//! actions = 3          # resource nodes only
//! yield = 1            # resource nodes only
//! ```
//!
//! Signposts carry `text = ...`; people carry `look = <appearance key>` and an optional
//! `behavior = <what a [behaviors] line says>`. Lines starting with `#` are comments, a file
//! must start with the header (a newer version is refused, never half-read), keys are lower
//! case, and every error names its line. To change the format, bump [`FORMAT_VERSION`] and keep
//! reading every older version.
//!
//! The shipped files are generated from, and tested against, the shipped world
//! ([`export_world`]), so they cannot drift from the level files.

use std::fmt;
use std::path::{Path, PathBuf};

use crate::appearance::Appearance;
use crate::gathering::ResourceKind;
use crate::level::{CreatureSpawn, Level, NodeSpawn, NpcSpawn, SignSpawn, GUIDE_ID};
use crate::npc::Behavior;
use crate::textdata::is_valid_id;
use crate::village::World;
use crate::world::TilePos;

/// The version this code writes, and the newest it reads.
pub const FORMAT_VERSION: u32 = 1;
const HEADER_WORD: &str = "ozbound-asset";
/// Where the shipped library lives, and the file extension of an asset.
pub const LIBRARY_DIR: &str = "assets/library";
pub const EXTENSION: &str = "asset";

/// The text of a sign placed from the library, until someone edits it.
pub const DEFAULT_SIGN_TEXT: &str = "A weathered signpost. Someone has scratched a few words into it, but they have worn away.";

/// What an asset is.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Body {
    /// A tree, copper vein, herb patch, or berry bush with how much it gives.
    Node { kind: ResourceKind, actions: u32, yield_per_action: u32 },
    Moonpetal,
    Sign { text: String },
    /// A person (the guide included): their look and how they behave when left alone.
    Npc { look: Appearance, behavior: Behavior },
    /// A creature from the bestiary (the bestiary holds its rules).
    Creature,
    /// One tile of wooden fence. It joins the fence tiles beside it by itself (the scene draws a
    /// rail toward each neighbouring fence tile), so a run is built one tile at a time.
    Fence,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Asset {
    /// Unique, lower-case letters, digits, and underscores; also the file's name.
    pub id: String,
    /// Shown to the player and in the editor.
    pub name: String,
    pub body: Body,
}

/// A problem with an asset file; `line` is 1-based, or 0 for a problem with the whole file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AssetError {
    pub line: usize,
    pub message: String,
}

impl AssetError {
    fn at(line: usize, message: impl Into<String>) -> Self {
        Self { line, message: message.into() }
    }
}

impl fmt::Display for AssetError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.line == 0 {
            write!(f, "{}", self.message)
        } else {
            write!(f, "line {}: {}", self.line, self.message)
        }
    }
}

impl std::error::Error for AssetError {}

fn node_keyword(kind: ResourceKind) -> Option<&'static str> {
    match kind {
        ResourceKind::Tree => Some("tree"),
        ResourceKind::CopperVein => Some("copper"),
        ResourceKind::HerbPatch => Some("herb"),
        ResourceKind::BerryBush => Some("berry"),
        ResourceKind::Moonpetal => None,
    }
}

fn node_from_keyword(word: &str) -> Option<ResourceKind> {
    [ResourceKind::Tree, ResourceKind::CopperVein, ResourceKind::HerbPatch, ResourceKind::BerryBush].into_iter().find(|k| node_keyword(*k) == Some(word))
}

/// The id of the library asset for a kind of node.
pub fn node_asset_id(kind: ResourceKind) -> &'static str {
    match kind {
        ResourceKind::Tree => "tree",
        ResourceKind::CopperVein => "copper_vein",
        ResourceKind::HerbPatch => "herb_patch",
        ResourceKind::BerryBush => "berry_bush",
        ResourceKind::Moonpetal => "moonpetal",
    }
}

/// The id of the library asset for a signpost.
pub const SIGN_ASSET_ID: &str = "signpost";
/// The id of the library asset for a piece of fence.
pub const FENCE_ASSET_ID: &str = "fence";

fn capitalised(text: &str) -> String {
    let mut chars = text.chars();
    chars.next().map_or_else(String::new, |c| c.to_uppercase().collect::<String>() + chars.as_str())
}

impl Asset {
    /// The asset describing a kind of resource node, with this node's numbers.
    pub fn from_node(node: &NodeSpawn) -> Self {
        Self {
            id: node_asset_id(node.kind).to_string(),
            name: capitalised(node.kind.label()),
            body: Body::Node { kind: node.kind, actions: node.actions, yield_per_action: node.yield_per_action },
        }
    }

    pub fn moonpetal() -> Self {
        Self { id: node_asset_id(ResourceKind::Moonpetal).to_string(), name: "Moonpetal".into(), body: Body::Moonpetal }
    }

    pub fn signpost(text: &str) -> Self {
        Self { id: SIGN_ASSET_ID.to_string(), name: "Signpost".into(), body: Body::Sign { text: text.to_string() } }
    }

    /// The one-tile piece of modular fence.
    pub fn fence() -> Self {
        Self { id: FENCE_ASSET_ID.to_string(), name: "Fence".into(), body: Body::Fence }
    }

    pub fn from_npc(npc: &NpcSpawn, behavior: &Behavior) -> Self {
        Self { id: npc.id.clone(), name: npc.name.clone(), body: Body::Npc { look: npc.look, behavior: behavior.clone() } }
    }

    /// The guide has no `[npcs]` line, so it is described here.
    pub fn guide(behavior: &Behavior) -> Option<Self> {
        Some(Self { id: GUIDE_ID.to_string(), name: "Village Guide".into(), body: Body::Npc { look: Appearance::from_key("guide")?, behavior: behavior.clone() } })
    }

    pub fn from_creature(spawn: &CreatureSpawn, name: &str) -> Self {
        Self { id: spawn.id.clone(), name: name.to_string(), body: Body::Creature }
    }

    /// The node this asset places on `tile`, if it is a resource node.
    pub fn node_spawn(&self, tile: TilePos) -> Option<NodeSpawn> {
        match self.body {
            Body::Node { kind, actions, yield_per_action } => Some(NodeSpawn { tile, kind, actions, yield_per_action }),
            _ => None,
        }
    }

    pub fn sign_spawn(&self, tile: TilePos) -> Option<SignSpawn> {
        match &self.body {
            Body::Sign { text } => Some(SignSpawn { tile, text: text.clone() }),
            _ => None,
        }
    }

    pub fn npc_spawn(&self, tile: TilePos) -> Option<NpcSpawn> {
        match &self.body {
            Body::Npc { look, .. } => Some(NpcSpawn { id: self.id.clone(), tile, look: *look, name: self.name.clone() }),
            _ => None,
        }
    }

    /// True for what the editor can put into a scene on its own: nodes, moonpetals, signs, fence.
    /// People and creatures also need dialogue or bestiary entries, so they are only moved.
    pub fn is_placeable(&self) -> bool {
        matches!(self.body, Body::Node { .. } | Body::Moonpetal | Body::Sign { .. } | Body::Fence)
    }

    /// A short line for the editor: what this is and what it gives.
    pub fn describe(&self) -> String {
        match &self.body {
            Body::Node { kind, actions, yield_per_action } => format!("{}: {actions} x {yield_per_action} {}", self.name, kind.item().label()),
            Body::Moonpetal => format!("{}: one-off pickup", self.name),
            Body::Sign { .. } => format!("{}: readable", self.name),
            Body::Npc { behavior, .. } => format!("{}: {}", self.name, behavior.to_text()),
            Body::Creature => format!("{}: creature", self.name),
            Body::Fence => format!("{}: joins the fence beside it", self.name),
        }
    }

    /// The file text, always in the same order, so the same asset gives the same bytes.
    pub fn to_text(&self) -> String {
        let kind = match &self.body {
            Body::Node { kind, .. } => node_keyword(*kind).unwrap_or("tree"),
            Body::Moonpetal => "moonpetal",
            Body::Sign { .. } => "sign",
            Body::Npc { .. } => "npc",
            Body::Creature => "creature",
            Body::Fence => "fence",
        };
        let mut text = format!("{HEADER_WORD} {FORMAT_VERSION}\nid = {}\nkind = {kind}\nname = {}\n", self.id, self.name);
        match &self.body {
            Body::Node { actions, yield_per_action, .. } => text.push_str(&format!("actions = {actions}\nyield = {yield_per_action}\n")),
            Body::Sign { text: message } => text.push_str(&format!("text = {message}\n")),
            Body::Npc { look, behavior } => {
                text.push_str(&format!("look = {}\n", look.key().unwrap_or("traveller")));
                text.push_str(&format!("behavior = {}\n", behavior.to_text()));
            }
            Body::Moonpetal | Body::Creature | Body::Fence => {}
        }
        text
    }

    /// The file name this asset is saved under.
    pub fn file_name(&self) -> String {
        format!("{}.{EXTENSION}", self.id)
    }

    /// Parses and validates one asset file.
    pub fn parse(text: &str) -> Result<Self, AssetError> {
        let mut header_seen = false;
        let mut fields: Vec<(usize, String, String)> = Vec::new();
        for (index, raw) in text.trim_start_matches('\u{feff}').lines().enumerate() {
            let number = index + 1;
            let line = raw.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            if !header_seen {
                let mut words = line.split_whitespace();
                if words.next() != Some(HEADER_WORD) {
                    return Err(AssetError::at(number, format!("an asset file starts with `{HEADER_WORD} {FORMAT_VERSION}`")));
                }
                let version: u32 = words.next().and_then(|v| v.parse().ok()).ok_or_else(|| AssetError::at(number, "the header needs a version number, as in `ozbound-asset 1`"))?;
                if words.next().is_some() {
                    return Err(AssetError::at(number, "the header is just the name and a version"));
                }
                if version == 0 {
                    return Err(AssetError::at(number, "format version 0 does not exist"));
                }
                if version > FORMAT_VERSION {
                    return Err(AssetError::at(number, format!("this asset is format version {version}, newer than this game understands ({FORMAT_VERSION}); update the game")));
                }
                header_seen = true;
                continue;
            }
            let (key, value) = line.split_once('=').ok_or_else(|| AssetError::at(number, "expected `key = value`"))?;
            let key = key.trim().to_ascii_lowercase();
            if fields.iter().any(|(_, k, _)| *k == key) {
                return Err(AssetError::at(number, format!("`{key}` is given twice")));
            }
            fields.push((number, key, value.trim().to_string()));
        }
        if !header_seen {
            return Err(AssetError::at(0, format!("the file is empty (it must start with `{HEADER_WORD} {FORMAT_VERSION}`)")));
        }
        let get = |key: &str| fields.iter().find(|(_, k, _)| k == key);
        let need = |key: &str| get(key).ok_or_else(|| AssetError::at(0, format!("missing `{key} = ...`")));
        let (id_line, _, id) = need("id")?;
        if !is_valid_id(id) {
            return Err(AssetError::at(*id_line, format!("id `{id}` must use only lower-case letters, digits, and underscores")));
        }
        let (kind_line, _, kind) = need("kind")?;
        let (name_line, _, name) = need("name")?;
        if name.is_empty() {
            return Err(AssetError::at(*name_line, "the name is empty"));
        }
        let allowed: &[&str] = match kind.as_str() {
            "tree" | "copper" | "herb" | "berry" => &["id", "kind", "name", "actions", "yield"],
            "moonpetal" | "creature" | "fence" => &["id", "kind", "name"],
            "sign" => &["id", "kind", "name", "text"],
            "npc" => &["id", "kind", "name", "look", "behavior"],
            other => return Err(AssetError::at(*kind_line, format!("unknown kind `{other}` (use tree, copper, herb, berry, moonpetal, sign, fence, npc, or creature)"))),
        };
        if let Some((line, key, _)) = fields.iter().find(|(_, k, _)| !allowed.contains(&k.as_str())) {
            return Err(AssetError::at(*line, format!("`{key}` does not belong on a {kind} asset")));
        }
        let number = |key: &str| -> Result<u32, AssetError> {
            let (line, _, value) = need(key)?;
            match value.parse::<u32>() {
                Ok(n) if n > 0 => Ok(n),
                _ => Err(AssetError::at(*line, format!("`{key}` must be a whole number above zero, not `{value}`"))),
            }
        };
        let body = match kind.as_str() {
            "tree" | "copper" | "herb" | "berry" => Body::Node {
                kind: node_from_keyword(kind).expect("keyword was matched above"),
                actions: number("actions")?,
                yield_per_action: number("yield")?,
            },
            "moonpetal" => Body::Moonpetal,
            "creature" => Body::Creature,
            "fence" => Body::Fence,
            "sign" => {
                let (line, _, message) = need("text")?;
                if message.is_empty() {
                    return Err(AssetError::at(*line, "the sign has no text"));
                }
                Body::Sign { text: message.clone() }
            }
            _ => {
                let (line, _, key) = need("look")?;
                let look = Appearance::from_key(key).ok_or_else(|| AssetError::at(*line, format!("unknown appearance `{key}` (known: {})", Appearance::KEYS.join(", "))))?;
                let behavior = match get("behavior") {
                    Some((line, _, text)) => Behavior::parse(text).map_err(|why| AssetError::at(*line, why))?,
                    None => Behavior::default(),
                };
                Body::Npc { look, behavior }
            }
        };
        Ok(Self { id: id.clone(), name: name.clone(), body })
    }
}

/// Every asset of a library, in file-name order.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Library {
    assets: Vec<Asset>,
}

/// The shipped files, compiled in as the fallback for a game started from another directory.
pub const BUILTIN: [(&str, &str); 15] = [
    ("apothecary.asset", include_str!("../assets/library/apothecary.asset")),
    ("berry_bush.asset", include_str!("../assets/library/berry_bush.asset")),
    ("copper_vein.asset", include_str!("../assets/library/copper_vein.asset")),
    ("fence.asset", include_str!("../assets/library/fence.asset")),
    ("ferrywoman.asset", include_str!("../assets/library/ferrywoman.asset")),
    ("guide.asset", include_str!("../assets/library/guide.asset")),
    ("herb_patch.asset", include_str!("../assets/library/herb_patch.asset")),
    ("lantern_moth.asset", include_str!("../assets/library/lantern_moth.asset")),
    ("meadow_hare.asset", include_str!("../assets/library/meadow_hare.asset")),
    ("moonpetal.asset", include_str!("../assets/library/moonpetal.asset")),
    ("pond_heron.asset", include_str!("../assets/library/pond_heron.asset")),
    ("signpost.asset", include_str!("../assets/library/signpost.asset")),
    ("surveyor.asset", include_str!("../assets/library/surveyor.asset")),
    ("tree.asset", include_str!("../assets/library/tree.asset")),
    ("woodcutter.asset", include_str!("../assets/library/woodcutter.asset")),
];

impl Library {
    /// Parses `(file name, text)` pairs. A file is named `<id>.asset`; ids are unique. The error
    /// names the file (and its line).
    pub fn from_files(files: &[(String, String)]) -> Result<Self, String> {
        let mut assets: Vec<Asset> = Vec::new();
        for (name, text) in files {
            let asset = Asset::parse(text).map_err(|error| format!("{name}: {error}"))?;
            if *name != asset.file_name() {
                return Err(format!("{name}: the file holds the asset `{}`, so it must be named `{}`", asset.id, asset.file_name()));
            }
            if assets.iter().any(|a| a.id == asset.id) {
                return Err(format!("{name}: there are two assets called `{}`", asset.id));
            }
            assets.push(asset);
        }
        assets.sort_by(|a, b| a.id.cmp(&b.id));
        Ok(Self { assets })
    }

    /// The compiled-in copy of the shipped library.
    pub fn builtin() -> Result<Self, String> {
        let files: Vec<(String, String)> = BUILTIN.iter().filter(|(name, _)| name.ends_with(".asset")).map(|(n, t)| (n.to_string(), t.to_string())).collect();
        Self::from_files(&files)
    }

    /// Reads every `*.asset` file in `dir`. A missing directory gives the built-in copy and a
    /// note; an unreadable or invalid file is an error naming it (nothing half loads).
    pub fn load(dir: &Path) -> Result<(Self, Option<String>), String> {
        let entries = match std::fs::read_dir(dir) {
            Ok(entries) => entries,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return Ok((Self::builtin()?, Some(format!("{} not found; using the built-in asset library", dir.display()))));
            }
            Err(error) => return Err(format!("cannot read {}: {error}", dir.display())),
        };
        let mut paths: Vec<PathBuf> = entries.filter_map(Result::ok).map(|e| e.path()).filter(|p| p.extension().is_some_and(|e| e == EXTENSION)).collect();
        paths.sort();
        let mut files = Vec::new();
        for path in paths {
            let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
            let text = std::fs::read_to_string(&path).map_err(|error| format!("cannot read {}: {error}", path.display()))?;
            files.push((name, text));
        }
        Ok((Self::from_files(&files)?, None))
    }

    pub fn get(&self, id: &str) -> Option<&Asset> {
        self.assets.iter().find(|a| a.id == id)
    }

    pub fn iter(&self) -> impl Iterator<Item = &Asset> {
        self.assets.iter()
    }

    pub fn len(&self) -> usize {
        self.assets.len()
    }

    pub fn is_empty(&self) -> bool {
        self.assets.is_empty()
    }

    /// Adds an asset, or replaces the one with the same id (keeping id order).
    pub fn upsert(&mut self, asset: Asset) {
        match self.assets.iter_mut().find(|a| a.id == asset.id) {
            Some(slot) => *slot = asset,
            None => {
                self.assets.push(asset);
                self.assets.sort_by(|a, b| a.id.cmp(&b.id));
            }
        }
    }

    /// What the editor can place, in id order.
    pub fn placeable(&self) -> Vec<&Asset> {
        self.assets.iter().filter(|a| a.is_placeable()).collect()
    }
}

/// Writes `asset` to `dir/<id>.asset` through a temporary file and a rename, so a failed write
/// never leaves half a file. Returns the path written.
pub fn save_asset(dir: &Path, asset: &Asset) -> Result<PathBuf, String> {
    std::fs::create_dir_all(dir).map_err(|error| format!("cannot create {}: {error}", dir.display()))?;
    let path = dir.join(asset.file_name());
    let temp = dir.join(format!("{}.tmp", asset.file_name()));
    let write = || -> std::io::Result<()> {
        use std::io::Write;
        let mut file = std::fs::File::create(&temp)?;
        file.write_all(asset.to_text().as_bytes())?;
        file.sync_all()?;
        std::fs::rename(&temp, &path)
    };
    write().map_err(|error| {
        let _ = std::fs::remove_file(&temp);
        format!("cannot save {}: {error}", path.display())
    })?;
    Ok(path)
}

/// Every asset the shipped world uses, one per kind of object and one per person or creature.
/// This is what `assets/library/` holds; a test compares the two.
pub fn export_world(world: &World) -> Vec<Asset> {
    let mut out: Vec<Asset> = Vec::new();
    let mut levels: Vec<&Level> = vec![&world.level];
    levels.extend(world.regions.iter().map(|(_, l)| l));
    for level in levels {
        for node in &level.nodes {
            let asset = Asset::from_node(node);
            if !out.iter().any(|a| a.id == asset.id) {
                out.push(asset);
            }
        }
    }
    out.push(Asset::moonpetal());
    out.push(Asset::signpost(DEFAULT_SIGN_TEXT));
    out.push(Asset::fence());
    if let Some(guide) = Asset::guide(&world.level.behavior_of(GUIDE_ID)) {
        out.push(guide);
    }
    for npc in &world.level.npcs {
        out.push(Asset::from_npc(npc, &world.level.behavior_of(&npc.id)));
    }
    for spawn in &world.level.creatures {
        if let Some(creature) = world.bestiary.get(&spawn.id) {
            out.push(Asset::from_creature(spawn, &creature.name));
        }
    }
    out.sort_by(|a, b| a.id.cmp(&b.id));
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tree() -> Asset {
        Asset { id: "tree".into(), name: "Tree".into(), body: Body::Node { kind: ResourceKind::Tree, actions: 3, yield_per_action: 1 } }
    }

    #[test]
    fn every_kind_round_trips_through_text() {
        let pell = Asset { id: "pell".into(), name: "Mistress Pell".into(), body: Body::Npc { look: Appearance::from_key("apothecary").unwrap(), behavior: Behavior::parse("wander 2 pause 4 9 emotes wave").unwrap() } };
        let all = [
            tree(),
            Asset { id: "copper_vein".into(), name: "Copper vein".into(), body: Body::Node { kind: ResourceKind::CopperVein, actions: 2, yield_per_action: 4 } },
            Asset { id: "herb_patch".into(), name: "Herbs".into(), body: Body::Node { kind: ResourceKind::HerbPatch, actions: 1, yield_per_action: 1 } },
            Asset { id: "berry_bush".into(), name: "Berries".into(), body: Body::Node { kind: ResourceKind::BerryBush, actions: 5, yield_per_action: 2 } },
            Asset::moonpetal(),
            Asset::signpost("Keep to the road, = and beware."),
            pell,
            Asset { id: "meadow_hare".into(), name: "Meadow hare".into(), body: Body::Creature },
        ];
        for asset in all {
            let text = asset.to_text();
            assert_eq!(Asset::parse(&text).as_ref(), Ok(&asset), "{text}");
            assert_eq!(Asset::parse(&text).unwrap().to_text(), text);
        }
    }

    #[test]
    fn comments_blank_lines_crlf_bom_and_key_case_are_tolerated() {
        let text = "\u{feff}# a tree\r\nozbound-asset 1\r\n\r\nID = tree\r\nKind = tree\r\nname = Tree\r\nactions = 3  \r\nyield = 1\r\n";
        assert_eq!(Asset::parse(text), Ok(tree()));
    }

    #[test]
    fn a_missing_wrong_or_newer_header_is_refused() {
        assert_eq!(Asset::parse("").unwrap_err().line, 0);
        assert_eq!(Asset::parse("id = tree\n").unwrap_err().line, 1);
        assert_eq!(Asset::parse("ozbound-asset\nid = x\n").unwrap_err().line, 1);
        assert!(Asset::parse("ozbound-asset 0\n").unwrap_err().message.contains("does not exist"));
        let newer = Asset::parse("ozbound-asset 2\nid = tree\n").unwrap_err();
        assert!(newer.message.contains("newer"), "{newer}");
        assert!(Asset::parse("ozbound-asset 1 extra\n").is_err());
    }

    #[test]
    fn errors_name_the_line() {
        let base = "ozbound-asset 1\nid = tree\nkind = tree\nname = Tree\nactions = 3\nyield = 1\n";
        assert_eq!(Asset::parse(&format!("{base}yield = 2\n")).unwrap_err().line, 7, "duplicate key");
        assert_eq!(Asset::parse(&base.replace("actions = 3", "actions = 0")).unwrap_err().line, 5);
        assert_eq!(Asset::parse(&base.replace("yield = 1", "yield = lots")).unwrap_err().line, 6);
        assert_eq!(Asset::parse(&base.replace("id = tree", "id = Big Tree")).unwrap_err().line, 2);
        assert_eq!(Asset::parse(&base.replace("kind = tree", "kind = boulder")).unwrap_err().line, 3);
        assert_eq!(Asset::parse(&base.replace("name = Tree", "name =")).unwrap_err().line, 4);
        assert_eq!(Asset::parse(&format!("{base}colour = green\n")).unwrap_err().line, 7);
        assert_eq!(Asset::parse(&format!("{base}nonsense\n")).unwrap_err().line, 7);
    }

    #[test]
    fn a_field_must_belong_to_the_kind_and_required_fields_must_be_there() {
        assert!(Asset::parse("ozbound-asset 1\nid = s\nkind = sign\nname = S\nactions = 1\ntext = hi\n").unwrap_err().message.contains("does not belong"));
        assert!(Asset::parse("ozbound-asset 1\nid = s\nkind = sign\nname = S\n").unwrap_err().message.contains("missing `text"));
        assert!(Asset::parse("ozbound-asset 1\nid = t\nkind = tree\nname = T\nactions = 1\n").unwrap_err().message.contains("missing `yield"));
        assert!(Asset::parse("ozbound-asset 1\nkind = tree\nname = T\n").unwrap_err().message.contains("missing `id"));
        assert!(Asset::parse("ozbound-asset 1\nid = s\nkind = sign\nname = S\ntext =\n").is_err());
    }

    #[test]
    fn people_need_a_known_look_and_a_valid_behavior() {
        let npc = |extra: &str| format!("ozbound-asset 1\nid = p\nkind = npc\nname = P\n{extra}");
        assert!(Asset::parse(&npc("look = wizard\n")).unwrap_err().message.contains("unknown appearance"));
        assert!(Asset::parse(&npc("look = guide\nbehavior = dance\n")).unwrap_err().message.contains("unknown behavior"));
        assert!(Asset::parse(&npc("")).unwrap_err().message.contains("missing `look"));
        // A person with no behavior stands still.
        let still = Asset::parse(&npc("look = guide\n")).unwrap();
        assert!(matches!(still.body, Body::Npc { ref behavior, .. } if *behavior == Behavior::default()));
    }

    #[test]
    fn conversions_to_spawns_keep_every_field() {
        let tile = TilePos::new(4, 7);
        let node = tree().node_spawn(tile).unwrap();
        assert_eq!(node, NodeSpawn { tile, kind: ResourceKind::Tree, actions: 3, yield_per_action: 1 });
        assert_eq!(Asset::from_node(&node), tree());
        assert!(tree().sign_spawn(tile).is_none() && tree().npc_spawn(tile).is_none());
        let sign = Asset::signpost("Hello").sign_spawn(tile).unwrap();
        assert_eq!((sign.tile, sign.text.as_str()), (tile, "Hello"));
        let npc = NpcSpawn { id: "pell".into(), tile, look: Appearance::from_key("apothecary").unwrap(), name: "Mistress Pell".into() };
        let asset = Asset::from_npc(&npc, &Behavior::default());
        assert_eq!(asset.npc_spawn(tile), Some(npc));
        assert!(Asset::moonpetal().node_spawn(tile).is_none());
    }

    #[test]
    fn only_objects_are_placeable() {
        assert!(tree().is_placeable() && Asset::moonpetal().is_placeable() && Asset::signpost("x").is_placeable());
        assert!(!Asset::guide(&Behavior::default()).unwrap().is_placeable());
        assert!(!Asset { id: "hare".into(), name: "Hare".into(), body: Body::Creature }.is_placeable());
    }

    #[test]
    fn a_library_checks_names_and_duplicates() {
        let file = |name: &str, asset: &Asset| (name.to_string(), asset.to_text());
        assert!(Library::from_files(&[file("tree.asset", &tree())]).is_ok());
        let wrong_name = Library::from_files(&[file("oak.asset", &tree())]).unwrap_err();
        assert!(wrong_name.contains("must be named `tree.asset`"), "{wrong_name}");
        let bad = Library::from_files(&[("tree.asset".to_string(), "ozbound-asset 1\n".to_string())]).unwrap_err();
        assert!(bad.starts_with("tree.asset:"), "{bad}");
    }

    #[test]
    fn the_shipped_library_loads_and_is_in_id_order() {
        let library = Library::builtin().unwrap();
        assert_eq!(library.len(), 15);
        let ids: Vec<&str> = library.iter().map(|a| a.id.as_str()).collect();
        let mut sorted = ids.clone();
        sorted.sort();
        assert_eq!(ids, sorted);
        assert_eq!(library.placeable().len(), 7);
        assert!(library.get("tree").is_some() && library.get("nothing").is_none());
    }

    #[test]
    fn a_missing_directory_falls_back_and_a_bad_file_stops_loading() {
        let missing = std::env::temp_dir().join("ozbound-no-such-library-dir");
        let (library, note) = Library::load(&missing).unwrap();
        assert!(note.is_some() && library.len() == 15);

        let dir = std::env::temp_dir().join(format!("ozbound-asset-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        save_asset(&dir, &tree()).unwrap();
        let (loaded, note) = Library::load(&dir).unwrap();
        assert!(note.is_none());
        assert_eq!(loaded.get("tree"), Some(&tree()));
        std::fs::write(dir.join("broken.asset"), "ozbound-asset 1\nid = broken\nkind = tree\nname = B\n").unwrap();
        let error = Library::load(&dir).unwrap_err();
        assert!(error.starts_with("broken.asset:"), "{error}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn saving_replaces_a_file_and_leaves_no_temporary_file() {
        let dir = std::env::temp_dir().join(format!("ozbound-asset-save-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let path = save_asset(&dir, &tree()).unwrap();
        let mut bigger = tree();
        bigger.body = Body::Node { kind: ResourceKind::Tree, actions: 9, yield_per_action: 1 };
        save_asset(&dir, &bigger).unwrap();
        assert_eq!(Asset::parse(&std::fs::read_to_string(&path).unwrap()), Ok(bigger));
        let names: Vec<String> = std::fs::read_dir(&dir).unwrap().filter_map(Result::ok).map(|e| e.file_name().to_string_lossy().into_owned()).collect();
        assert_eq!(names, ["tree.asset"]);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn describing_an_asset_says_what_it_gives() {
        assert_eq!(tree().describe(), "Tree: 3 x 1 timber");
        assert!(Asset::guide(&Behavior::parse("stay emotes wave").unwrap()).unwrap().describe().contains("stay emotes wave"));
    }
}
