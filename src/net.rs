//! Groundwork for networked play (M12). Nothing here opens a socket: it fixes the vocabulary a
//! networked game would need, so that the rules stay on one side of a clean line and the
//! rendering and input on the other. See `docs/NETWORKING.md` for the design.
//!
//! Pure Rust (no Bevy). Two pieces:
//! - [`NetId`]: a name for a world object (a tree, a villager, a moonpetal) that is the same on
//!   every machine and every run, because it is computed from what the object *is* and where it
//!   stands, never from the order things were spawned. A server can say "the tree
//!   `3f2a...` has 2 actions left" and any client knows which tree that is.
//! - [`Command`]: everything a player can ask the world to do, as plain data. Input systems turn
//!   clicks and keys into commands; only the simulation applies them. A client would send them,
//!   a server would check and apply them, and a single-player game applies them locally.
//!
//! Commands travel as one line of text (`encode` / `decode`), in the same spirit as the save
//! format: easy to read, easy to log, and every malformed line is refused with a reason.

use std::fmt;

use crate::level::Level;
use crate::world::TilePos;

/// The scope of every object on the open overworld. Their tiles are world tiles, which are the
/// same everywhere, so one scope covers all of its chunks. A region off the overworld (a cave)
/// uses its own region id as scope, since its tiles are local to it.
pub const OVERWORLD_SCOPE: &str = "overworld";

/// A stable name for one object in the world.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct NetId(pub u64);

impl NetId {
    /// The id of the object of `kind` standing on `tile` in `scope`. 64-bit FNV-1a over the
    /// text `scope|kind|x|y`: the same on every platform and release, with no randomness and no
    /// dependency. (It is a name, not a secret: a server must still check every command.)
    pub fn of(scope: &str, kind: &str, tile: TilePos) -> Self {
        let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
        for byte in format!("{scope}|{kind}|{}|{}", tile.x, tile.y).bytes() {
            hash ^= u64::from(byte);
            hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
        }
        Self(hash)
    }
}

impl fmt::Display for NetId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:016x}", self.0)
    }
}

impl std::str::FromStr for NetId {
    type Err = String;
    fn from_str(text: &str) -> Result<Self, String> {
        if text.len() != 16 || !text.bytes().all(|b| b.is_ascii_hexdigit()) {
            return Err(format!("`{text}` is not an object id (16 hex digits)"));
        }
        u64::from_str_radix(text, 16).map(Self).map_err(|e| e.to_string())
    }
}

/// Who is playing. In single-player this is always [`PlayerId::LOCAL`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct PlayerId(pub u32);

impl PlayerId {
    pub const LOCAL: PlayerId = PlayerId(0);
}

/// The kind names that go into a [`NetId`]. The game spawns objects with these same functions,
/// so [`objects`] and the running world always agree.
pub fn kind_moonpetal() -> &'static str {
    "moonpetal"
}
pub fn kind_node(label: &str) -> String {
    format!("node:{label}")
}
pub fn kind_npc(id: &str) -> String {
    format!("npc:{id}")
}
pub fn kind_creature(id: &str) -> String {
    format!("creature:{id}")
}
pub fn kind_sign() -> &'static str {
    "sign"
}

/// Every interactable object a level places, with its id: moonpetals, trees, veins, plants,
/// signposts, the guide, villagers, and creatures. `scope` is [`OVERWORLD_SCOPE`] for a level
/// composed from overworld chunks, or the region id for a level played on its own.
pub fn objects(level: &Level, scope: &str) -> Vec<(NetId, String)> {
    let mut all = Vec::new();
    let mut add = |kind: String, tile: TilePos| all.push((NetId::of(scope, &kind, tile), kind));
    for tile in &level.moonpetals {
        add(kind_moonpetal().to_string(), *tile);
    }
    for node in &level.nodes {
        add(kind_node(node.kind.label()), node.tile);
    }
    for sign in &level.signs {
        add(kind_sign().to_string(), sign.tile);
    }
    if let Some(tile) = level.guide {
        add(kind_npc(crate::level::GUIDE_ID), tile);
    }
    for npc in &level.npcs {
        add(kind_npc(&npc.id), npc.tile);
    }
    for creature in &level.creatures {
        add(kind_creature(&creature.id), creature.tile);
    }
    all
}

/// One thing a player asks the world to do.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    /// Walk to a tile.
    WalkTo(TilePos),
    /// Use an object: talk, gather, pick, approach, read.
    Interact(NetId),
    /// Pick a numbered choice (0-based) in the open conversation.
    Choose(usize),
    /// Use a numbered row (0-based) of the crafting-and-gear panel.
    PackRow(usize),
    /// Say something in the chat.
    Say(String),
}

/// The longest chat line a command may carry.
pub const MAX_SAY: usize = 200;

impl Command {
    /// One line of text, without a trailing newline.
    pub fn encode(&self) -> String {
        match self {
            Self::WalkTo(tile) => format!("walk {} {}", tile.x, tile.y),
            Self::Interact(id) => format!("interact {id}"),
            Self::Choose(index) => format!("choose {index}"),
            Self::PackRow(index) => format!("pack {index}"),
            Self::Say(text) => format!("say {text}"),
        }
    }

    /// Reads one line written by [`Command::encode`]. Anything else is refused with a reason.
    pub fn decode(line: &str) -> Result<Self, String> {
        let line = line.trim_end_matches(['\r', '\n']);
        let (word, rest) = line.split_once(' ').unwrap_or((line, ""));
        let number = |text: &str| text.parse::<usize>().map_err(|_| format!("`{text}` is not a number"));
        match word {
            "walk" => match rest.split_whitespace().collect::<Vec<_>>().as_slice() {
                [x, y] => {
                    let coord = |text: &str| text.parse::<i32>().map_err(|_| format!("`{text}` is not a tile coordinate"));
                    Ok(Self::WalkTo(TilePos::new(coord(x)?, coord(y)?)))
                }
                _ => Err("walk needs two numbers: x and y".into()),
            },
            "interact" => rest.parse().map(Self::Interact),
            "choose" => number(rest).map(Self::Choose),
            "pack" => number(rest).map(Self::PackRow),
            "say" => {
                if rest.is_empty() {
                    Err("say needs some text".into())
                } else if rest.chars().count() > MAX_SAY {
                    Err(format!("a chat line is at most {MAX_SAY} characters"))
                } else if rest.chars().any(char::is_control) {
                    Err("a chat line cannot hold control characters".into())
                } else {
                    Ok(Self::Say(rest.to_string()))
                }
            }
            "" => Err("the command is empty".into()),
            other => Err(format!("`{other}` is not a command")),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::village::builtin;
    use std::collections::HashSet;

    #[test]
    fn ids_are_stable_and_depend_on_every_part() {
        let tile = TilePos::new(12, 7);
        let id = NetId::of("overworld", "node:tree", tile);
        assert_eq!(id, NetId::of("overworld", "node:tree", tile));
        for other in [
            NetId::of("cave", "node:tree", tile),
            NetId::of("overworld", "node:vein", tile),
            NetId::of("overworld", "node:tree", TilePos::new(7, 12)),
            NetId::of("overworld", "node:tree", TilePos::new(12, 8)),
        ] {
            assert_ne!(id, other);
        }
    }

    #[test]
    fn the_hash_is_the_documented_fnv_1a() {
        // FNV-1a 64 of the empty text is its offset basis; of "a" it is a well-known value.
        let fnv = |text: &str| {
            let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
            for byte in text.bytes() {
                hash ^= u64::from(byte);
                hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
            }
            hash
        };
        assert_eq!(fnv(""), 0xcbf2_9ce4_8422_2325);
        assert_eq!(fnv("a"), 0xaf63_dc4c_8601_ec8c);
        assert_eq!(NetId::of("s", "k", TilePos::new(1, 2)).0, fnv("s|k|1|2"));
    }

    #[test]
    fn ids_print_and_parse_as_sixteen_hex_digits() {
        let id = NetId(0xab);
        assert_eq!(id.to_string(), "00000000000000ab");
        assert_eq!("00000000000000ab".parse::<NetId>(), Ok(id));
        for bad in ["", "ab", "00000000000000AZ", "000000000000000ab", "0x0000000000000ab"] {
            assert!(bad.parse::<NetId>().is_err(), "{bad:?}");
        }
    }

    #[test]
    fn every_object_of_every_shipped_region_has_its_own_id() {
        let world = builtin().expect("the shipped world loads");
        let mut levels = vec![("start".to_string(), &world.level)];
        levels.extend(world.regions.iter().map(|(id, level)| (id.clone(), level)));
        for (id, level) in levels {
            let all = objects(level, &id);
            assert!(!all.is_empty(), "{id} places nothing");
            let unique: HashSet<NetId> = all.iter().map(|(id, _)| *id).collect();
            assert_eq!(unique.len(), all.len(), "two objects of {id} share an id");
        }
    }

    #[test]
    fn every_object_of_the_whole_overworld_has_its_own_id() {
        let world = builtin().expect("the shipped world loads");
        let overworld = world.overworld.as_ref().expect("the shipped world has an overworld");
        let ids: Vec<String> = overworld.chunks().iter().map(|c| c.id.clone()).collect();
        let composite = overworld.compose(&|id| world.level_of(id), &ids);
        let all = objects(&composite.level, OVERWORLD_SCOPE);
        let unique: HashSet<NetId> = all.iter().map(|(id, _)| *id).collect();
        assert_eq!(unique.len(), all.len(), "two objects of the overworld share an id");
        assert!(all.len() > 50, "only {} objects", all.len());
        // The same world always gives the same ids, in the same order.
        assert_eq!(all, objects(&composite.level, OVERWORLD_SCOPE));
    }

    #[test]
    fn commands_round_trip_through_a_line_of_text() {
        for command in [
            Command::WalkTo(TilePos::new(3, -4)),
            Command::Interact(NetId::of("overworld", "npc:guide", TilePos::new(5, 5))),
            Command::Choose(0),
            Command::PackRow(12),
            Command::Say("hello, Bluebrook!".into()),
        ] {
            let line = command.encode();
            assert!(!line.contains('\n'));
            assert_eq!(Command::decode(&line), Ok(command));
        }
        assert_eq!(Command::decode("choose 2\r\n"), Ok(Command::Choose(2)));
    }

    #[test]
    fn malformed_commands_are_refused_with_a_reason() {
        for (line, word) in [
            ("", "empty"),
            ("dance", "not a command"),
            ("walk 3", "two numbers"),
            ("walk x 3", "coordinate"),
            ("interact 12", "object id"),
            ("choose -1", "not a number"),
            ("pack", "not a number"),
            ("say ", "some text"),
            ("say bell\u{7}", "control"),
        ] {
            let e = Command::decode(line).unwrap_err();
            assert!(e.contains(word), "{line:?}: {e}");
        }
        let long = format!("say {}", "x".repeat(MAX_SAY + 1));
        assert!(Command::decode(&long).unwrap_err().contains("at most"));
        assert!(Command::decode(&format!("say {}", "x".repeat(MAX_SAY))).is_ok());
    }
}
