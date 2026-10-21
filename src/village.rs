//! The Bluebrook world as one loaded, cross-checked unit: level, quests, dialogue, bestiary,
//! and recipes.
//!
//! Pure Rust (no Bevy). Each file is validated on its own by its parser; [`check`] then
//! validates how they fit together, so a content mistake that no single parser can see
//! (a quest nobody offers, a `visit` to a place that is not on the map, a quest that asks for
//! an item the world never supplies) stops startup with a plain explanation instead of
//! leaving the player stuck.
//!
//! Loading follows the same rules as the other content files: a custom path must load, an
//! absent default file falls back to the identical built-in copy with a note, and an
//! existing but invalid file is never replaced by the built-in copy.

use std::collections::BTreeSet;
use std::io::ErrorKind;
use std::path::Path;

use crate::bestiary::{Bestiary, BESTIARY_PATH, DEFAULT_BESTIARY};
use crate::content::Content;
use crate::crafting::{self, RecipeBook};
use crate::dialogue::{DialogueBook, Effect};
use crate::inventory::ItemId;
use crate::level::{Level, GUIDE_ID};
use crate::locations::{self, Locations, DEFAULT_LOCATIONS, LOCATIONS_PATH, MAPS_DIR};
use crate::quests::{Objective, QuestBook};

/// The built-in copies, identical to the files under `assets/`.
pub const BLUEBROOK_MAP: &str = include_str!("../assets/maps/bluebrook.map");
pub const BLUEBROOK_QUESTS: &str = include_str!("../assets/quests/bluebrook.quest");
pub const BLUEBROOK_DIALOGUE: &str = include_str!("../assets/dialogue/bluebrook.dialogue");

/// Built-in copies of the other regions' maps, by file name (see `assets/world/locations.world`).
pub const BUILTIN_REGION_MAPS: [(&str, &str); 1] = [("yellow_road_east.map", include_str!("../assets/maps/yellow_road_east.map"))];

pub const MAP_PATH: &str = "assets/maps/bluebrook.map";
pub const QUESTS_PATH: &str = "assets/quests/bluebrook.quest";
pub const DIALOGUE_PATH: &str = "assets/dialogue/bluebrook.dialogue";

/// Everything the game needs to start playing Bluebrook.
#[derive(Debug, Clone)]
pub struct World {
    /// The starting region's level: the story level, with the guide, villagers, and quests.
    pub level: Level,
    /// The location register (see `locations.rs`).
    pub locations: Locations,
    /// Every other playable region, as `(location id, level)`. Regions carry no story content.
    pub regions: Vec<(String, Level)>,
    pub content: Content,
    pub bestiary: Bestiary,
    pub recipes: RecipeBook,
}

/// Which files to read. `None` for a custom path means "use the default".
#[derive(Debug, Clone, Copy)]
pub struct Sources<'a> {
    pub map_custom: Option<&'a Path>,
    pub quests_custom: Option<&'a Path>,
    pub dialogue_custom: Option<&'a Path>,
    pub bestiary_custom: Option<&'a Path>,
    pub recipes_custom: Option<&'a Path>,
    pub locations_custom: Option<&'a Path>,
    pub locations_default: &'a Path,
    /// Where the other regions' maps are looked up by file name.
    pub maps_dir: &'a Path,
    pub map_default: &'a Path,
    pub quests_default: &'a Path,
    pub dialogue_default: &'a Path,
    pub bestiary_default: &'a Path,
    pub recipes_default: &'a Path,
}

impl Sources<'_> {
    /// The shipped files, relative to the working directory, with no custom overrides.
    pub fn defaults() -> Sources<'static> {
        Sources {
            map_custom: None,
            quests_custom: None,
            dialogue_custom: None,
            bestiary_custom: None,
            recipes_custom: None,
            locations_custom: None,
            locations_default: Path::new(LOCATIONS_PATH),
            maps_dir: Path::new(MAPS_DIR),
            map_default: Path::new(MAP_PATH),
            quests_default: Path::new(QUESTS_PATH),
            dialogue_default: Path::new(DIALOGUE_PATH),
            bestiary_default: Path::new(BESTIARY_PATH),
            recipes_default: Path::new(crafting::RECIPES_PATH),
        }
    }
}

#[derive(Debug, Clone)]
pub struct Loaded {
    pub world: World,
    /// Notes to print, for example that a built-in copy was used.
    pub notes: Vec<String>,
}

/// Reads a text file: the custom path (which must exist), else the default path, else the
/// built-in copy with a note.
fn read(what: &str, custom: Option<&Path>, default: &Path, builtin: &str) -> Result<(String, String, Option<String>), String> {
    if let Some(path) = custom {
        let text = std::fs::read_to_string(path).map_err(|e| format!("could not read {what} file {}: {e}", path.display()))?;
        return Ok((text, path.display().to_string(), None));
    }
    match std::fs::read_to_string(default) {
        Ok(text) => Ok((text, default.display().to_string(), None)),
        Err(e) if e.kind() == ErrorKind::NotFound => Ok((
            builtin.to_string(),
            format!("built-in {what}"),
            Some(format!("note: {} not found; using the built-in {what}", default.display())),
        )),
        Err(e) => Err(format!("could not read {what} file {}: {e}", default.display())),
    }
}

fn invalid(what: &str, label: &str, error: impl std::fmt::Display) -> String {
    if label.starts_with("built-in") {
        format!("{label} is invalid: {error}")
    } else {
        format!("invalid {what} file {label}: {error}")
    }
}

/// Loads and cross-checks the whole world.
pub fn load(sources: &Sources) -> Result<Loaded, String> {
    let mut notes = Vec::new();
    let mut take = |note: Option<String>| notes.extend(note);

    let (text, label, note) = read("level", sources.map_custom, sources.map_default, BLUEBROOK_MAP)?;
    take(note);
    let level = Level::parse(&text).map_err(|e| invalid("level", &label, e))?;

    let (text, quest_label, note) = read("quests", sources.quests_custom, sources.quests_default, BLUEBROOK_QUESTS)?;
    take(note);
    let quests = QuestBook::parse(&text).map_err(|e| invalid("quest", &quest_label, e))?;

    let (text, dialogue_label, note) = read("dialogue", sources.dialogue_custom, sources.dialogue_default, BLUEBROOK_DIALOGUE)?;
    take(note);
    let dialogue = DialogueBook::parse(&text).map_err(|e| invalid("dialogue", &dialogue_label, e))?;
    dialogue
        .check_against(&quests)
        .map_err(|e| format!("dialogue {dialogue_label} does not match the quests in {quest_label}: {e}"))?;

    let (text, label, note) = read("bestiary", sources.bestiary_custom, sources.bestiary_default, DEFAULT_BESTIARY)?;
    take(note);
    let bestiary = Bestiary::parse(&text).map_err(|e| invalid("bestiary", &label, e))?;

    let recipes = crafting::load(sources.recipes_custom, sources.recipes_default)?;
    take(recipes.note.clone());

    let (text, locations_label, note) = read("locations", sources.locations_custom, sources.locations_default, DEFAULT_LOCATIONS)?;
    take(note);
    let locations = Locations::parse(&text).map_err(|e| invalid("locations", &locations_label, e))?;
    let regions = load_regions(&locations, sources, &mut notes)?;

    let world = World { level, locations, regions, content: Content { quests, dialogue }, bestiary, recipes: recipes.book };
    check(&world).map_err(|e| format!("the world does not hold together: {e}"))?;
    Ok(Loaded { world, notes })
}

/// Loads every playable region except the starting one (whose level is read separately, so a
/// custom `OZBOUND_MAP` can replace it). A map file is read from `sources.maps_dir`; a missing
/// file falls back to the identical built-in copy with a note, like the other content files.
fn load_regions(locations: &Locations, sources: &Sources, notes: &mut Vec<String>) -> Result<Vec<(String, Level)>, String> {
    let mut regions = Vec::new();
    for location in locations.playable().filter(|l| !l.start) {
        let file = location.map.as_deref().expect("playable locations have a map");
        let builtin = BUILTIN_REGION_MAPS
            .iter()
            .find(|(name, _)| *name == file)
            .map(|(_, text)| *text)
            .ok_or_else(|| format!("location `{}` uses map {file}, which has no built-in copy; add it to BUILTIN_REGION_MAPS", location.id))?;
        let (text, label, note) = read("region map", None, &sources.maps_dir.join(file), builtin)?;
        notes.extend(note);
        let level = Level::parse(&text).map_err(|e| invalid("region map", &label, e))?;
        regions.push((location.id.clone(), level));
    }
    Ok(regions)
}

impl World {
    /// The level of a playable region by location id: the story level for the starting
    /// region, the loaded level for any other.
    pub fn level_of(&self, id: &str) -> Option<&Level> {
        if id == self.locations.start().id {
            return Some(&self.level);
        }
        self.regions.iter().find(|(region, _)| region == id).map(|(_, level)| level)
    }

    /// The location id of the starting region.
    pub fn start_id(&self) -> &str {
        &self.locations.start().id
    }
}

/// The built-in world (used by tests, and by the game when no files are found).
pub fn builtin() -> Result<World, String> {
    let sources = Sources {
        locations_default: Path::new("/nonexistent/locations.world"),
        maps_dir: Path::new("/nonexistent/maps"),
        map_default: Path::new("/nonexistent/bluebrook.map"),
        quests_default: Path::new("/nonexistent/bluebrook.quest"),
        dialogue_default: Path::new("/nonexistent/bluebrook.dialogue"),
        bestiary_default: Path::new("/nonexistent/bluebrook.bestiary"),
        recipes_default: Path::new("/nonexistent/meadow.recipes"),
        ..Sources::defaults()
    };
    load(&sources).map(|loaded| loaded.world)
}

/// Every item the player can end up holding in this world, ignoring quest rewards (a quest
/// must never depend on its own reward).
fn gatherable_items(world: &World) -> BTreeSet<ItemId> {
    let mut items: BTreeSet<ItemId> = world.level.nodes.iter().map(|n| n.kind.item()).collect();
    if !world.level.moonpetals.is_empty() {
        items.insert(ItemId::Moonpetal);
    }
    items
}

/// `gathered` plus everything craftable from what is available, to a fixed point.
fn obtainable_items(world: &World, gathered: &BTreeSet<ItemId>) -> BTreeSet<ItemId> {
    let mut items = gathered.clone();
    for creature in world.bestiary.iter().filter(|c| world.level.creatures.iter().any(|s| s.id == c.id)) {
        if let Some(gift) = creature.gift {
            items.insert(gift.item);
        }
    }
    loop {
        let before = items.len();
        for recipe in world.recipes.all() {
            if recipe.inputs.iter().all(|i| items.contains(&i.item)) {
                items.insert(recipe.output.item);
            }
        }
        if items.len() == before {
            return items;
        }
    }
}

/// Checks that the files fit together. See the module docs for why.
pub fn check(world: &World) -> Result<(), String> {
    locations::check_world(&world.locations, |id| world.level_of(id)).map_err(|e| e.to_string())?;
    let level = &world.level;
    let quests = &world.content.quests;
    let dialogue = &world.content.dialogue;

    // People: everyone in the level can be talked to, and every conversation has someone to hold it.
    let npc_ids: BTreeSet<&str> = level.npc_list().into_iter().map(|(id, _, _)| id).collect();
    for id in &npc_ids {
        if dialogue.conversation_for_npc(id).is_none() {
            return Err(format!("`{id}` stands in the level but has no conversation in the dialogue file"));
        }
    }
    for conversation in dialogue.iter() {
        if !npc_ids.contains(conversation.npc.as_str()) {
            return Err(format!(
                "conversation `{}` is for `{}`, who does not appear in the level (the guide is `{GUIDE_ID}`; others come from [npcs])",
                conversation.id, conversation.npc
            ));
        }
    }

    // Creatures in the level must be in the bestiary.
    for spawn in &level.creatures {
        if world.bestiary.get(&spawn.id).is_none() {
            return Err(format!("creature `{}` stands in the level but is not in the bestiary", spawn.id));
        }
    }

    // Quest targets exist, and the items quests ask for can really be had.
    let gathered = gatherable_items(world);
    let obtainable = obtainable_items(world, &gathered);
    for quest in quests.iter() {
        let here = |msg: String| format!("quest `{}`: {msg}", quest.id);
        for objective in &quest.objectives {
            match objective {
                Objective::TalkTo { npc } if !npc_ids.contains(npc.as_str()) => {
                    return Err(here(format!("talks to `{npc}`, who does not appear in the level")));
                }
                Objective::Visit { place } if !level.places.iter().any(|p| &p.id == place) => {
                    return Err(here(format!("visits `{place}`, which is not in the level's [places]")));
                }
                Objective::Befriend { creature } => {
                    if world.bestiary.get(creature).is_none() {
                        return Err(here(format!("befriends `{creature}`, who is not in the bestiary")));
                    }
                    if !level.creatures.iter().any(|c| &c.id == creature) {
                        return Err(here(format!("befriends `{creature}`, who is not in the level's [creatures]")));
                    }
                }
                Objective::Collect { item, count } => {
                    if !gathered.contains(item) {
                        return Err(here(format!("collects {}, which nothing in the level can be gathered for", item.label())));
                    }
                    if *item == ItemId::Moonpetal && (*count as usize) > level.moonpetals.len() {
                        return Err(here(format!(
                            "collects {count} moonpetals but the level has only {} (moonpetals do not grow back)",
                            level.moonpetals.len()
                        )));
                    }
                }
                Objective::Craft { item, .. } => {
                    let makers: Vec<_> = world.recipes.all().iter().filter(|r| r.output.item == *item).collect();
                    if makers.is_empty() {
                        return Err(here(format!("crafts {}, but no recipe makes it", item.label())));
                    }
                    if !makers.iter().any(|r| r.inputs.iter().all(|i| obtainable.contains(&i.item))) {
                        return Err(here(format!("crafts {}, but its ingredients cannot all be obtained", item.label())));
                    }
                }
                _ => {}
            }
        }
        for take in &quest.takes {
            if !obtainable.contains(&take.item) {
                return Err(here(format!("takes {}, which the player can never have", take.item.label())));
            }
        }
    }

    // Every quest can be offered and handed in somewhere.
    let mut started: BTreeSet<&str> = BTreeSet::new();
    let mut completed: BTreeSet<&str> = BTreeSet::new();
    for conversation in dialogue.iter() {
        for node in &conversation.nodes {
            for effect in &node.effects {
                match effect {
                    Effect::Start(id) => {
                        started.insert(id.as_str());
                    }
                    Effect::Complete(id) => {
                        completed.insert(id.as_str());
                    }
                }
            }
        }
    }
    for quest in quests.iter() {
        if !started.contains(quest.id.as_str()) {
            return Err(format!("quest `{}` is never offered: no dialogue node has `effect = start {}`", quest.id, quest.id));
        }
        if !completed.contains(quest.id.as_str()) {
            return Err(format!("quest `{}` can never be handed in: no dialogue node has `effect = complete {}`", quest.id, quest.id));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn world() -> World {
        builtin().expect("the built-in Bluebrook world must load")
    }

    fn rebuild(map: &str, quests: &str, dialogue: &str, bestiary: &str) -> Result<(), String> {
        let locations = Locations::parse(DEFAULT_LOCATIONS).map_err(|e| e.to_string())?;
        let mut notes = Vec::new();
        let regions = load_regions(&locations, &Sources { maps_dir: Path::new("/nonexistent/maps"), ..Sources::defaults() }, &mut notes)?;
        let world = World {
            level: Level::parse(map).map_err(|e| e.to_string())?,
            locations,
            regions,
            content: Content {
                quests: QuestBook::parse(quests).map_err(|e| e.to_string())?,
                dialogue: DialogueBook::parse(dialogue).map_err(|e| e.to_string())?,
            },
            bestiary: Bestiary::parse(bestiary).map_err(|e| e.to_string())?,
            recipes: RecipeBook::builtin().map_err(|e| e.to_string())?,
        };
        check(&world)
    }

    fn with(map: Option<(&str, &str)>, quests: Option<(&str, &str)>, dialogue: Option<(&str, &str)>, bestiary: Option<(&str, &str)>) -> Result<(), String> {
        let swap = |text: &str, change: Option<(&str, &str)>| match change {
            Some((from, to)) => {
                assert!(text.contains(from), "test bug: {from:?} not found");
                text.replacen(from, to, 1)
            }
            None => text.to_string(),
        };
        rebuild(
            &swap(BLUEBROOK_MAP, map),
            &swap(BLUEBROOK_QUESTS, quests),
            &swap(BLUEBROOK_DIALOGUE, dialogue),
            &swap(DEFAULT_BESTIARY, bestiary),
        )
    }

    #[test]
    fn the_shipped_world_loads_and_holds_together() {
        let world = world();
        assert_eq!((world.level.map.width(), world.level.map.height()), (40, 24));
        assert_eq!(world.content.quests.len(), 5);
        assert_eq!(world.level.npc_list().len(), 5);
        assert_eq!(world.bestiary.len(), 3);
    }

    #[test]
    fn the_shipped_files_equal_the_built_in_copies() {
        let root = env!("CARGO_MANIFEST_DIR");
        for (path, builtin) in [
            ("assets/maps/bluebrook.map", BLUEBROOK_MAP),
            ("assets/quests/bluebrook.quest", BLUEBROOK_QUESTS),
            ("assets/dialogue/bluebrook.dialogue", BLUEBROOK_DIALOGUE),
        ] {
            assert_eq!(std::fs::read_to_string(format!("{root}/{path}")).unwrap(), builtin, "{path}");
        }
    }

    #[test]
    fn the_quests_use_five_different_objective_types() {
        let world = world();
        let mut kinds = std::collections::HashSet::new();
        for quest in world.content.quests.iter() {
            for o in &quest.objectives {
                kinds.insert(std::mem::discriminant(o));
            }
        }
        assert_eq!(kinds.len(), 5, "talk, collect, craft, visit, and befriend should all be used");
        assert!(world.content.quests.len() >= 3);
    }

    #[test]
    fn loading_from_the_shipped_directory_matches_the_built_in_world() {
        let root = env!("CARGO_MANIFEST_DIR");
        let join = |p: &str| Path::new(root).join(p);
        let (map, quests, dialogue, bestiary, recipes) = (
            join(MAP_PATH),
            join(QUESTS_PATH),
            join(DIALOGUE_PATH),
            join(BESTIARY_PATH),
            join(crafting::RECIPES_PATH),
        );
        let loaded = load(&Sources {
            map_default: &map,
            quests_default: &quests,
            dialogue_default: &dialogue,
            bestiary_default: &bestiary,
            recipes_default: &recipes,
            ..Sources::defaults()
        })
        .unwrap();
        assert!(loaded.notes.is_empty(), "{:?}", loaded.notes);
        assert_eq!(loaded.world.level, world().level);
        assert_eq!(loaded.world.bestiary, world().bestiary);
    }

    #[test]
    fn absent_default_files_fall_back_to_the_built_in_world_with_notes() {
        let loaded = load(&Sources {
            map_default: Path::new("/nonexistent/a.map"),
            quests_default: Path::new("/nonexistent/a.quest"),
            dialogue_default: Path::new("/nonexistent/a.dialogue"),
            bestiary_default: Path::new("/nonexistent/a.bestiary"),
            recipes_default: Path::new("/nonexistent/a.recipes"),
            ..Sources::defaults()
        })
        .unwrap();
        assert_eq!(loaded.notes.len(), 5);
        assert!(loaded.notes.iter().all(|n| n.starts_with("note: ") && n.contains("built-in")));
    }

    #[test]
    fn a_custom_path_that_is_missing_is_an_error_not_a_fallback() {
        let missing = Path::new("/nonexistent/custom.map");
        let error = load(&Sources { map_custom: Some(missing), ..Sources::defaults() }).unwrap_err();
        assert!(error.contains("could not read level file") && error.contains("custom.map"), "{error}");
    }

    #[test]
    fn an_invalid_existing_file_is_reported_with_its_name_and_line() {
        let dir = std::env::temp_dir().join(format!("ozbound-village-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let bad = dir.join("bad.bestiary");
        std::fs::write(&bad, "[creature x]\nwings = 2\n").unwrap();
        let error = load(&Sources { bestiary_custom: Some(&bad), ..Sources::defaults() }).unwrap_err();
        assert!(error.contains("invalid bestiary file") && error.contains("bad.bestiary") && error.contains("line 2"), "{error}");
        std::fs::remove_dir_all(&dir).ok();
    }

    // ---- cross-file mistakes, each with a plain explanation ----

    #[test]
    fn an_npc_without_a_conversation_is_refused() {
        let error = with(None, None, Some(("[conversation surveyor]\nnpc = surveyor", "[conversation surveyor]\nnpc = guide2")), None).unwrap_err();
        assert!(error.contains("conversation") || error.contains("npc"), "{error}");
        let error = with(None, None, Some(("npc = woodcutter", "npc = miller")), None).unwrap_err();
        assert!(error.contains("`woodcutter` stands in the level but has no conversation") || error.contains("miller"), "{error}");
    }

    #[test]
    fn a_conversation_for_someone_absent_is_refused() {
        let extra = format!("{BLUEBROOK_DIALOGUE}\n[conversation ghost]\nnpc = ghost\nspeaker = Ghost\nentry = hi\n\n[node hi]\ntext = Boo.\nchoice = Bye. -> end\n");
        let error = rebuild(BLUEBROOK_MAP, BLUEBROOK_QUESTS, &extra, DEFAULT_BESTIARY).unwrap_err();
        assert!(error.contains("`ghost`, who does not appear in the level"), "{error}");
    }

    #[test]
    fn a_visit_to_an_unknown_place_is_refused() {
        let error = with(None, Some(("visit lookout", "visit summit")), None, None).unwrap_err();
        assert!(error.contains("`waymarks`") && error.contains("`summit`") && error.contains("[places]"), "{error}");
    }

    #[test]
    fn befriending_needs_a_bestiary_entry_and_a_creature_in_the_level() {
        let error = with(None, Some(("befriend meadow_hare", "befriend dragon")), None, None).unwrap_err();
        assert!(error.contains("`dragon`") && error.contains("bestiary"), "{error}");
        let error = with(Some(("meadow_hare 6 6\n", "")), None, None, None).unwrap_err();
        assert!(error.contains("`meadow_hare`") && error.contains("[creatures]"), "{error}");
    }

    #[test]
    fn a_creature_missing_from_the_bestiary_is_refused() {
        let error = with(Some(("lantern_moth 27 16", "swamp_toad 27 16")), None, None, None).unwrap_err();
        assert!(error.contains("`swamp_toad`") && error.contains("not in the bestiary"), "{error}");
    }

    #[test]
    fn a_talk_objective_for_someone_absent_is_refused() {
        let error = with(None, Some(("talk surveyor", "talk cartographer")), None, None).unwrap_err();
        assert!(error.contains("`cartographer`"), "{error}");
    }

    #[test]
    fn quests_cannot_ask_for_what_the_world_never_supplies() {
        // No copper veins and no timber trees means the staff and charm chains are impossible.
        let no_trees = BLUEBROOK_MAP
            .lines()
            .filter(|l| !l.starts_with("tree "))
            .collect::<Vec<_>>()
            .join("\n");
        let error = rebuild(&no_trees, BLUEBROOK_QUESTS, BLUEBROOK_DIALOGUE, DEFAULT_BESTIARY).unwrap_err();
        assert!(error.contains("bridge_staff") && error.contains("ingredients cannot all be obtained"), "{error}");
        let error = with(None, Some(("collect moonpetal 3", "collect moonpetal 5")), None, None).unwrap_err();
        assert!(error.contains("moonpetals but the level has only 4"), "{error}");
    }

    #[test]
    fn a_craft_objective_needs_a_recipe() {
        let error = with(None, Some(("craft walking_staff 1", "craft mint_tea 1")), None, None);
        // mint tea has a recipe and its ingredients are available, so this one is fine ...
        assert!(error.is_ok(), "{error:?}");
        // ... but an item no recipe makes is refused.
        let error = with(None, Some(("craft walking_staff 1", "craft moonpetal 1")), None, None).unwrap_err();
        assert!(error.contains("no recipe makes it"), "{error}");
    }

    #[test]
    fn a_quest_nobody_offers_or_accepts_is_refused() {
        let error = with(None, None, Some(("effect = start bridge_staff", "")), None).unwrap_err();
        assert!(error.contains("`bridge_staff` is never offered"), "{error}");
        let error = with(None, None, Some(("effect = complete waymarks", "")), None).unwrap_err();
        assert!(error.contains("`waymarks` can never be handed in"), "{error}");
    }

    #[test]
    fn handing_in_an_item_that_cannot_be_had_is_refused() {
        let no_copper = BLUEBROOK_MAP.lines().filter(|l| !l.starts_with("copper ")).collect::<Vec<_>>().join("\n");
        let quests = BLUEBROOK_QUESTS.replace("takes = walking_staff 1", "takes = copper_charm 1");
        let error = rebuild(&no_copper, &quests, BLUEBROOK_DIALOGUE, DEFAULT_BESTIARY).unwrap_err();
        assert!(error.contains("`bridge_staff`") && error.contains("copper charm"), "{error}");
    }

    #[test]
    fn creature_gifts_count_as_obtainable_but_not_as_gathered() {
        let world = world();
        let gathered = gatherable_items(&world);
        let obtainable = obtainable_items(&world, &gathered);
        assert!(gathered.contains(&ItemId::Moonpetal) && gathered.contains(&ItemId::Timber));
        assert!(obtainable.is_superset(&gathered));
        assert!(obtainable.contains(&ItemId::WalkingStaff));
    }
}
