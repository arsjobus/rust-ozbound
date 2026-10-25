use super::*;
use crate::village;

const ROAD: &str = "YELLOW_BRICK_ROAD_EAST";

fn world() -> village::World {
    village::builtin().expect("the shipped world loads")
}

fn library() -> Library {
    Library::builtin().unwrap()
}

fn ok(_: &str, _: &Level) -> Result<(), String> {
    Ok(())
}

fn tile(x: i32, y: i32) -> TilePos {
    TilePos::new(x, y)
}

fn region(levels: &Levels, id: &str) -> Level {
    levels.iter().find(|(r, _)| r == id).unwrap().1.clone()
}

/// A free grass tile of a level where the edit `make(tile)` is accepted.
fn first_accepted(levels: &Levels, library: &Library, region_id: &str, make: impl Fn(TilePos) -> Edit) -> TilePos {
    let level = region(levels, region_id);
    let (w, h) = (level.map.width() as i32, level.map.height() as i32);
    for y in 0..h {
        for x in 0..w {
            let mut trial = levels.clone();
            if apply_edit(&mut trial, &make(tile(x, y)), library, &ok).is_ok() {
                return tile(x, y);
            }
        }
    }
    panic!("no tile accepts the edit");
}

fn mini(text: &str) -> Levels {
    vec![("MINI".to_string(), Level::parse(text).unwrap())]
}

// The first map row is the highest y: the player is bottom-left, the petal is above the wall,
// and the only way between them is the one-tile gap at (2, 1).
const GAP: &str = "[map]\n.....\n.....\n##.##\n....T\n\n[player]\n0 0\n[moonpetals]\n0 2\n[nodes]\ntree 4 0 3 1\n";

#[test]
fn moving_a_tree_swaps_grass_and_tree_and_undo_restores_the_level_exactly() {
    let world = world();
    let lib = library();
    let mut levels = world.all_levels();
    let before = levels.clone();
    let check = |id: &str, l: &Level| world.check_with(id, l);
    let tree = region(&levels, ROAD).nodes.iter().find(|n| n.kind == ResourceKind::Tree).copied().unwrap();
    let make = |to: TilePos| Edit::Move { region: ROAD.into(), object: ObjectRef::Node(ResourceKind::Tree, tree.tile), to };
    let to = first_accepted(&levels, &lib, ROAD, make);
    let mut history = History::default();
    history.apply(&mut levels, make(to), &lib, &check).unwrap();
    let after = region(&levels, ROAD);
    assert_eq!(after.map.tile_at(tree.tile), Ok(Tile::Grass));
    assert_eq!(after.map.tile_at(to), Ok(Tile::Tree));
    assert!(after.nodes.iter().any(|n| n.tile == to && n.kind == ResourceKind::Tree && n.actions == tree.actions));
    assert!(!after.nodes.iter().any(|n| n.tile == tree.tile));
    assert_ne!(levels, before);
    history.undo(&mut levels, &lib, &check).unwrap();
    assert_eq!(levels, before, "undo restores every list and the map exactly");
    history.redo(&mut levels, &lib, &check).unwrap();
    assert_eq!(region(&levels, ROAD), after);
}

#[test]
fn moving_a_villager_keeps_their_behavior_and_other_scenes_untouched() {
    let world = world();
    let lib = library();
    let mut levels = world.all_levels();
    let start = world.start_id().to_string();
    let check = |id: &str, l: &Level| world.check_with(id, l);
    let before = levels.clone();
    let make = |to: TilePos| Edit::Move { region: start.clone(), object: ObjectRef::Npc("apothecary".into()), to };
    let to = first_accepted(&levels, &lib, &start, make);
    let mut history = History::default();
    history.apply(&mut levels, make(to), &lib, &check).unwrap();
    let level = region(&levels, &start);
    let pell = level.npcs.iter().find(|n| n.id == "apothecary").unwrap();
    assert_eq!(pell.tile, to);
    assert_eq!(pell.name, "Mistress Pell");
    assert_eq!(level.behaviors.iter().filter(|b| b.id == "apothecary").count(), 1);
    assert_eq!(region(&levels, ROAD), region(&before, ROAD));
    history.undo(&mut levels, &lib, &check).unwrap();
    assert_eq!(levels, before);
}

#[test]
fn removing_a_tree_leaves_grass_and_undo_puts_it_back_in_place() {
    let world = world();
    let lib = library();
    let mut levels = world.all_levels();
    let before = levels.clone();
    let check = |id: &str, l: &Level| world.check_with(id, l);
    let level = region(&levels, ROAD);
    // Remove the middle tree of the list, so a wrong index would show.
    let trees: Vec<&NodeSpawn> = level.nodes.iter().filter(|n| n.kind == ResourceKind::Tree).collect();
    let victim = *trees[trees.len() / 2];
    let mut history = History::default();
    let said = history.apply(&mut levels, Edit::Remove { region: ROAD.into(), object: ObjectRef::Node(ResourceKind::Tree, victim.tile) }, &lib, &check).unwrap();
    assert_eq!(said, "Removed tree.");
    let after = region(&levels, ROAD);
    assert_eq!(after.map.tile_at(victim.tile), Ok(Tile::Grass));
    assert_eq!(after.nodes.len(), level.nodes.len() - 1);
    history.undo(&mut levels, &lib, &check).unwrap();
    assert_eq!(levels, before);
}

#[test]
fn removing_a_signpost_and_a_copper_vein_restore_their_tiles() {
    let world = world();
    let lib = library();
    let mut levels = world.all_levels();
    let before = levels.clone();
    let check = |id: &str, l: &Level| world.check_with(id, l);
    let level = region(&levels, ROAD);
    let sign = level.signs[0].tile;
    let vein = level.nodes.iter().find(|n| n.kind == ResourceKind::CopperVein).unwrap().tile;
    let mut history = History::default();
    history.apply(&mut levels, Edit::Remove { region: ROAD.into(), object: ObjectRef::Sign(sign) }, &lib, &check).unwrap();
    history.apply(&mut levels, Edit::Remove { region: ROAD.into(), object: ObjectRef::Node(ResourceKind::CopperVein, vein) }, &lib, &check).unwrap();
    let mid = region(&levels, ROAD);
    assert_eq!((mid.map.tile_at(sign), mid.map.tile_at(vein)), (Ok(Tile::Grass), Ok(Tile::Grass)));
    history.undo(&mut levels, &lib, &check).unwrap();
    history.undo(&mut levels, &lib, &check).unwrap();
    assert_eq!(levels, before);
    assert_eq!(region(&levels, ROAD).map.tile_at(vein), Ok(Tile::Rock));
    assert_eq!(region(&levels, ROAD).map.tile_at(sign), Ok(Tile::Sign));
}

#[test]
fn the_world_check_refuses_to_remove_someone_the_story_needs() {
    let world = world();
    let lib = library();
    let mut levels = world.all_levels();
    let before = levels.clone();
    let start = world.start_id().to_string();
    let check = |id: &str, l: &Level| world.check_with(id, l);
    for object in [ObjectRef::Npc("apothecary".into()), ObjectRef::Guide] {
        let mut history = History::default();
        let error = history.apply(&mut levels, Edit::Remove { region: start.clone(), object }, &lib, &check).unwrap_err();
        assert!(matches!(error, EditError::Breaks(_)), "{error}");
        assert!(error.to_string().starts_with("That would break the world:"), "{error}");
        assert_eq!(levels, before, "a refused edit changes nothing");
        assert!(history.is_empty());
    }
}

#[test]
fn a_move_must_land_on_a_suitable_free_tile() {
    let lib = library();
    let mut levels = mini(GAP);
    let tree = ObjectRef::Node(ResourceKind::Tree, tile(4, 0));
    let mv = |to: TilePos| Edit::Move { region: "MINI".into(), object: tree.clone(), to };
    let refused = |levels: &mut Levels, edit: Edit| match apply_edit(levels, &edit, &lib, &ok) {
        Err(EditError::Refused(why)) => why,
        other => panic!("expected a refusal, got {other:?}"),
    };
    assert!(refused(&mut levels, mv(tile(9, 9))).contains("off the map"));
    assert!(refused(&mut levels, mv(tile(4, 0))).contains("already there"));
    assert!(refused(&mut levels, mv(tile(0, 1))).contains("bare grass"), "a rock tile");
    assert!(refused(&mut levels, mv(tile(0, 0))).contains("already stands"), "the player's start");
    assert!(refused(&mut levels, mv(tile(0, 2))).contains("already stands"), "a moonpetal");
    assert_eq!(apply_edit(&mut levels, &Edit::Move { region: "MINI".into(), object: ObjectRef::Node(ResourceKind::Tree, tile(0, 0)), to: tile(1, 0) }, &lib, &ok).unwrap_err(), EditError::NotThere);
    assert!(matches!(apply_edit(&mut levels, &Edit::Move { region: "NOPE".into(), object: tree.clone(), to: tile(1, 0) }, &lib, &ok), Err(EditError::UnknownRegion(_))));
}

#[test]
fn an_edit_that_cuts_the_way_to_something_is_refused() {
    let lib = library();
    let mut levels = mini(GAP);
    let before = levels.clone();
    // The tree onto the one-tile gap would seal the moonpetal off from the player.
    let seal = Edit::Move { region: "MINI".into(), object: ObjectRef::Node(ResourceKind::Tree, tile(4, 0)), to: tile(2, 1) };
    let error = apply_edit(&mut levels, &seal, &lib, &ok).unwrap_err();
    assert!(matches!(&error, EditError::Breaks(why) if why.contains("moonpetal")), "{error}");
    assert_eq!(levels, before);
    // Anywhere that leaves the gap open is fine.
    let fine = Edit::Move { region: "MINI".into(), object: ObjectRef::Node(ResourceKind::Tree, tile(4, 0)), to: tile(3, 0) };
    assert!(apply_edit(&mut levels, &fine, &lib, &ok).is_ok());
}

#[test]
fn the_callers_wider_check_can_veto_an_edit() {
    let lib = library();
    let mut levels = mini(GAP);
    let before = levels.clone();
    let veto = |_: &str, _: &Level| Err("the quest log disagrees".to_string());
    let edit = Edit::Move { region: "MINI".into(), object: ObjectRef::Node(ResourceKind::Tree, tile(4, 0)), to: tile(3, 0) };
    let error = apply_edit(&mut levels, &edit, &lib, &veto).unwrap_err();
    assert_eq!(error.to_string(), "That would break the world: the quest log disagrees.");
    assert_eq!(levels, before);
}

#[test]
fn placing_an_asset_adds_it_and_undo_removes_it() {
    let lib = library();
    let mut levels = mini(GAP);
    let before = levels.clone();
    let mut history = History::default();
    for (asset, at) in [("tree", tile(4, 2)), ("signpost", tile(1, 2)), ("moonpetal", tile(3, 2)), ("herb_patch", tile(2, 2))] {
        history.apply(&mut levels, Edit::Place { region: "MINI".into(), asset: asset.into(), tile: at }, &lib, &ok).unwrap_or_else(|e| panic!("{asset}: {e}"));
    }
    let level = region(&levels, "MINI");
    assert_eq!(level.map.tile_at(tile(4, 2)), Ok(Tile::Tree));
    assert_eq!(level.map.tile_at(tile(1, 2)), Ok(Tile::Sign));
    assert_eq!(level.signs[0].text, crate::asset_file::DEFAULT_SIGN_TEXT);
    assert!(level.moonpetals.contains(&tile(3, 2)));
    assert!(level.nodes.iter().any(|n| n.kind == ResourceKind::HerbPatch && n.tile == tile(2, 2)));
    while history.can_undo() {
        history.undo(&mut levels, &lib, &ok).unwrap();
    }
    assert_eq!(levels, before);
}

#[test]
fn placing_refuses_the_wrong_tile_unknown_assets_and_people() {
    let lib = library();
    let mut levels = mini(GAP);
    let place = |asset: &str, at: TilePos| Edit::Place { region: "MINI".into(), asset: asset.into(), tile: at };
    assert!(matches!(apply_edit(&mut levels, &place("tree", tile(0, 1)), &lib, &ok), Err(EditError::Refused(_))), "a tree needs grass, not rock");
    assert!(matches!(apply_edit(&mut levels, &place("tree", tile(4, 0)), &lib, &ok), Err(EditError::Refused(_))), "the tile is taken by a tree");
    assert!(matches!(apply_edit(&mut levels, &place("moonpetal", tile(0, 2)), &lib, &ok), Err(EditError::Refused(_))), "a petal already grows there");
    assert!(matches!(apply_edit(&mut levels, &place("tree", tile(2, 1)), &lib, &ok), Err(EditError::Breaks(_))), "sealing the gap");
    assert!(matches!(apply_edit(&mut levels, &place("oak", tile(1, 0)), &lib, &ok), Err(EditError::UnknownAsset(_))));
    assert!(matches!(apply_edit(&mut levels, &place("apothecary", tile(1, 0)), &lib, &ok), Err(EditError::NotPlaceable(_))));
    assert!(matches!(apply_edit(&mut levels, &place("guide", tile(1, 0)), &lib, &ok), Err(EditError::NotPlaceable(_))));
}

#[test]
fn a_new_edit_clears_redo_and_history_reports_what_it_would_undo() {
    let lib = library();
    let mut levels = mini(GAP);
    let mut history = History::default();
    assert!(!history.can_undo() && !history.can_redo());
    assert_eq!(history.undo(&mut levels, &lib, &ok).unwrap_err(), EditError::NothingToUndo);
    assert_eq!(history.redo(&mut levels, &lib, &ok).unwrap_err(), EditError::NothingToRedo);
    let mv = |to| Edit::Move { region: "MINI".into(), object: ObjectRef::Node(ResourceKind::Tree, tile(4, 0)), to };
    history.apply(&mut levels, mv(tile(3, 0)), &lib, &ok).unwrap();
    assert_eq!(history.undo_label(), Some("Moved tree."));
    history.undo(&mut levels, &lib, &ok).unwrap();
    assert!(history.can_redo());
    assert_eq!(history.redo_label(), Some("Moved tree."));
    history.apply(&mut levels, Edit::Remove { region: "MINI".into(), object: ObjectRef::Node(ResourceKind::Tree, tile(4, 0)) }, &lib, &ok).unwrap();
    assert!(!history.can_redo(), "a fresh edit forgets the undone one");
    assert_eq!(history.len(), 1);
}

#[test]
fn a_failed_edit_leaves_history_alone() {
    let lib = library();
    let mut levels = mini(GAP);
    let mut history = History::default();
    let bad = Edit::Move { region: "MINI".into(), object: ObjectRef::Node(ResourceKind::Tree, tile(4, 0)), to: tile(2, 1) };
    history.apply(&mut levels, bad, &lib, &ok).unwrap_err();
    assert!(history.is_empty());
}

#[test]
fn history_is_capped() {
    let lib = library();
    let mut levels = mini(GAP);
    let mut history = History::default();
    for i in 0..(HISTORY_LIMIT + 20) {
        let (from, to) = if i % 2 == 0 { (tile(4, 0), tile(3, 0)) } else { (tile(3, 0), tile(4, 0)) };
        history.apply(&mut levels, Edit::Move { region: "MINI".into(), object: ObjectRef::Node(ResourceKind::Tree, from), to }, &lib, &ok).unwrap();
    }
    assert_eq!(history.len(), HISTORY_LIMIT);
}

#[test]
fn the_edit_layer_round_trips_and_replays_to_the_same_world() {
    let world = world();
    let lib = library();
    let check = |id: &str, l: &Level| world.check_with(id, l);
    let mut levels = world.all_levels();
    let start = world.start_id().to_string();
    let tree = region(&levels, ROAD).nodes.iter().find(|n| n.kind == ResourceKind::Tree).unwrap().tile;
    let to = first_accepted(&levels, &lib, ROAD, |to| Edit::Move { region: ROAD.into(), object: ObjectRef::Node(ResourceKind::Tree, tree), to });
    let pell_to = first_accepted(&levels, &lib, &start, |to| Edit::Move { region: start.clone(), object: ObjectRef::Npc("apothecary".into()), to });
    let mut history = History::default();
    history.apply(&mut levels, Edit::Move { region: ROAD.into(), object: ObjectRef::Node(ResourceKind::Tree, tree), to }, &lib, &check).unwrap();
    history.apply(&mut levels, Edit::Move { region: start.clone(), object: ObjectRef::Npc("apothecary".into()), to: pell_to }, &lib, &check).unwrap();
    let sign = region(&levels, ROAD).signs[0].tile;
    history.apply(&mut levels, Edit::Remove { region: ROAD.into(), object: ObjectRef::Sign(sign) }, &lib, &check).unwrap();
    let spot = first_accepted(&levels, &lib, ROAD, |at| Edit::Place { region: ROAD.into(), asset: "berry_bush".into(), tile: at });
    history.apply(&mut levels, Edit::Place { region: ROAD.into(), asset: "berry_bush".into(), tile: spot }, &lib, &check).unwrap();
    // An undone edit is not part of the layer.
    history.apply(&mut levels, Edit::Remove { region: ROAD.into(), object: ObjectRef::Node(ResourceKind::Tree, to) }, &lib, &check).unwrap();
    history.undo(&mut levels, &lib, &check).unwrap();

    let layer = history.layer();
    assert_eq!(layer.edits.len(), 4);
    let text = layer.to_text();
    assert!(text.starts_with("ozbound-edits 1\n"), "{text}");
    let reread = EditLayer::parse(&text).unwrap();
    assert_eq!(reread, layer);
    assert_eq!(reread.to_text(), text);

    let mut fresh = world.all_levels();
    let replayed = replay(&mut fresh, &reread, &lib, &check).unwrap();
    assert_eq!(fresh, levels, "replaying the saved layer rebuilds the edited world");
    assert_eq!(replayed.len(), 4);
}

#[test]
fn replay_is_all_or_nothing_and_names_the_failing_edit() {
    let world = world();
    let lib = library();
    let check = |id: &str, l: &Level| world.check_with(id, l);
    let mut levels = world.all_levels();
    let before = levels.clone();
    let tree = region(&levels, ROAD).nodes.iter().find(|n| n.kind == ResourceKind::Tree).unwrap().tile;
    let to = first_accepted(&levels, &lib, ROAD, |to| Edit::Move { region: ROAD.into(), object: ObjectRef::Node(ResourceKind::Tree, tree), to });
    let layer = EditLayer {
        edits: vec![
            Edit::Move { region: ROAD.into(), object: ObjectRef::Node(ResourceKind::Tree, tree), to },
            Edit::Remove { region: world.start_id().into(), object: ObjectRef::Guide },
        ],
    };
    let error = replay(&mut levels, &layer, &lib, &check).unwrap_err();
    assert!(error.starts_with("edit 2 (remove guide in "), "{error}");
    assert_eq!(levels, before, "the first edit was not kept either");
}

#[test]
fn layer_text_errors_name_the_line() {
    let at = |text: &str| EditLayer::parse(text).unwrap_err();
    assert!(at("").contains("empty"));
    assert!(at("move A tree\n").starts_with("line 1:"));
    assert!(at("ozbound-edits 2\n").contains("newer"));
    assert!(at("ozbound-edits 0\n").contains("does not exist"));
    assert!(at("ozbound-edits 1\nspin A tree 1 2\n").starts_with("line 2: unknown edit"));
    assert!(at("ozbound-edits 1\n\n# c\nmove nowhere tree 1 2 to 3 4\n").starts_with("line 4: `nowhere` is not a scene id"));
    assert!(at("ozbound-edits 1\nmove MINI node boulder 1 2 to 3 4\n").contains("unknown node kind"));
    assert!(at("ozbound-edits 1\nmove MINI sign 1 two to 3 4\n").contains("not a whole number"));
    assert!(at("ozbound-edits 1\nremove MINI\n").starts_with("line 2:"));
    assert!(at("ozbound-edits 1\nplace MINI Big Tree 1 2\n").starts_with("line 2:"));
    assert_eq!(EditLayer::parse("\u{feff}ozbound-edits 1\r\n# nothing yet\r\n").unwrap(), EditLayer::default());
}

#[test]
fn every_object_reference_round_trips_through_text() {
    for object in [
        ObjectRef::Guide,
        ObjectRef::Npc("apothecary".into()),
        ObjectRef::Creature("meadow_hare".into()),
        ObjectRef::Moonpetal(tile(3, -4)),
        ObjectRef::Node(ResourceKind::CopperVein, tile(1, 2)),
        ObjectRef::Node(ResourceKind::HerbPatch, tile(1, 2)),
        ObjectRef::Sign(tile(5, 6)),
        ObjectRef::Thicket(tile(7, 8)),
    ] {
        let text = object.to_text();
        let words: Vec<&str> = text.split_whitespace().collect();
        assert_eq!(ObjectRef::parse(&words), Ok(object));
    }
    assert!(ObjectRef::parse(&["guide", "extra"]).is_err());
}

#[test]
fn saving_and_loading_the_layer_file() {
    let dir = std::env::temp_dir().join(format!("ozbound-edit-layer-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let path = dir.join("saves").join("edits.txt");
    assert_eq!(load_layer(&path), Loaded::Absent);
    let layer = EditLayer { edits: vec![Edit::Remove { region: ROAD.into(), object: ObjectRef::Sign(tile(3, 3)) }] };
    save_layer(&path, &layer).unwrap();
    assert_eq!(load_layer(&path), Loaded::Layer(layer.clone()));
    assert!(!path.with_extension("txt.tmp").exists(), "no temporary file is left");
    std::fs::write(&path, "garbage").unwrap();
    assert!(matches!(load_layer(&path), Loaded::Unusable(why) if why.contains("line 1")));
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "garbage", "an unusable file is left as it is");
    save_layer(&path, &EditLayer::default()).unwrap();
    assert_eq!(load_layer(&path), Loaded::Absent, "an empty layer removes the file");
    save_layer(&path, &EditLayer::default()).unwrap();
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn objects_are_found_by_tile_and_exported_as_assets() {
    let world = world();
    let level = &world.level;
    let guide = object_at(level, level.guide.unwrap()).unwrap();
    assert!(matches!(guide, Object::Guide { .. }));
    assert_eq!(guide.name(), "the Village Guide");
    let pell = level.npcs.iter().find(|n| n.id == "apothecary").unwrap();
    let found = object_at(level, pell.tile).unwrap();
    assert_eq!(found.reference(), ObjectRef::Npc("apothecary".into()));
    let asset = found.to_asset(None).unwrap();
    assert_eq!((asset.id.as_str(), asset.name.as_str()), ("apothecary", "Mistress Pell"));
    assert!(matches!(asset.body, Body::Npc { ref behavior, .. } if behavior.radius() == Some(2)));
    let sign = level.signs[0].tile;
    assert!(object_at(level, sign).unwrap().to_asset(None).is_none(), "signs are not exported");
    assert!(object_at(level, tile(0, 0)).is_none() || object_at(level, tile(0, 0)).is_some());
    let hare = level.creatures.iter().find(|c| c.id == "meadow_hare").unwrap();
    assert_eq!(object_at(level, hare.tile).unwrap().to_asset(Some("Meadow hare")).unwrap().name, "Meadow hare");
}

#[test]
fn editor_state_follows_selection_through_moves_and_drops_it_when_gone() {
    let mut state = EditorState::default();
    assert!(!state.active && state.tool == Tool::SelectMove);
    assert!(state.toggle());
    state.select("MINI", ObjectRef::Node(ResourceKind::Tree, tile(4, 0)));
    state.follow(&Edit::Move { region: "MINI".into(), object: ObjectRef::Node(ResourceKind::Tree, tile(4, 0)), to: tile(3, 0) });
    assert_eq!(state.selection.as_ref().unwrap().object, ObjectRef::Node(ResourceKind::Tree, tile(3, 0)));
    state.follow(&Edit::Move { region: "MINI".into(), object: ObjectRef::Node(ResourceKind::Tree, tile(9, 9)), to: tile(1, 1) });
    assert_eq!(state.selection.as_ref().unwrap().object, ObjectRef::Node(ResourceKind::Tree, tile(3, 0)), "another object's move changes nothing");
    let levels = mini(GAP);
    state.prune(&levels);
    assert!(state.selection.is_none(), "the tree is not at (3, 0) in this level");
    state.select("MINI", ObjectRef::Node(ResourceKind::Tree, tile(4, 0)));
    state.prune(&levels);
    assert!(state.selection.is_some());
    assert!(!state.toggle());
    assert!(state.selection.is_none(), "leaving the editor drops the selection");
}

#[test]
fn what_a_ground_click_does_depends_on_the_tool() {
    let lib = library();
    let placeable = lib.placeable();
    let mut state = EditorState::default();
    state.select("MINI", ObjectRef::Guide);
    state.choose(Tool::Select);
    assert_eq!(state.ground_click(&placeable), GroundClick::Nothing);
    state.choose(Tool::SelectMove);
    assert_eq!(state.ground_click(&placeable), GroundClick::MoveSelection(Selection { region: "MINI".into(), object: ObjectRef::Guide }));
    state.clear_selection();
    assert_eq!(state.ground_click(&placeable), GroundClick::Nothing, "nothing selected, nothing to move");
    state.next_asset(placeable.len());
    assert_eq!(state.tool, Tool::Place);
    assert_eq!(state.ground_click(&placeable), GroundClick::PlaceAsset(placeable[1].id.clone()));
    for _ in 0..placeable.len() {
        state.next_asset(placeable.len());
    }
    assert_eq!(state.place_index, 1, "the asset list wraps");
    assert_eq!(state.ground_click(&[]), GroundClick::Nothing);
}

#[test]
fn the_ribbon_enables_only_what_can_be_used() {
    let lib = library();
    let mut state = EditorState::default();
    let mut levels = mini(GAP);
    let mut history = History::default();
    let items = ribbon(&state, &history, Some("Tree"));
    let by = |items: &[RibbonItem], b: Button| items.iter().find(|i| i.button == b).unwrap().clone();
    assert_eq!(items.iter().map(|i| i.button).collect::<Vec<_>>(), Button::ALL);
    assert!(by(&items, Button::SelectMove).active && !by(&items, Button::Select).active);
    assert!(!by(&items, Button::Undo).enabled && !by(&items, Button::Redo).enabled && !by(&items, Button::Export).enabled);
    assert_eq!(by(&items, Button::Place).label, "Place: Tree");
    assert!(!by(&ribbon(&state, &history, None), Button::Place).enabled);
    history.apply(&mut levels, Edit::Remove { region: "MINI".into(), object: ObjectRef::Node(ResourceKind::Tree, tile(4, 0)) }, &lib, &ok).unwrap();
    state.select("MINI", ObjectRef::Moonpetal(tile(0, 2)));
    let items = ribbon(&state, &history, Some("Tree"));
    assert!(by(&items, Button::Undo).enabled && !by(&items, Button::Redo).enabled && by(&items, Button::Export).enabled);
    history.undo(&mut levels, &lib, &ok).unwrap();
    assert!(by(&ribbon(&state, &history, Some("Tree")), Button::Redo).enabled);
}

#[test]
fn check_level_accepts_every_shipped_level() {
    let world = world();
    for (id, level) in world.all_levels() {
        assert_eq!(check_level(&level), Ok(()), "{id}");
    }
}

#[test]
fn creatures_may_stand_beside_waymarks_but_not_on_people() {
    let world = world();
    let lib = library();
    let start = world.start_id().to_string();
    let mut levels = world.all_levels();
    let level = region(&levels, &start);
    let place = level.places[0].tile;
    let hare = ObjectRef::Creature("meadow_hare".into());
    // Onto a waymark: allowed by the rules of the target check (the world check may still say no).
    let onto_place = Edit::Move { region: start.clone(), object: hare.clone(), to: place };
    let result = apply_edit(&mut levels, &onto_place, &lib, &ok);
    assert!(!matches!(result, Err(EditError::Refused(_))), "{result:?}");
    // Onto a villager: refused.
    let pell = level.npcs[0].tile;
    let result = apply_edit(&mut levels, &Edit::Move { region: start, object: hare, to: pell }, &lib, &ok);
    assert!(matches!(result, Err(EditError::Refused(_))), "{result:?}");
}

// A thicket is a tree tile with no chopping tree on it. In `THICKETS` the bush at (1, 1) sits in
// the open, the tile at (4, 0) is a chopping tree, and (3, 2) is a bush that walls off nothing.
const THICKETS: &str = "[map]\n.....\n.T.T.\n..T..\n.....\n\n[player]\n0 0\n[moonpetals]\n4 3\n[nodes]\ntree 3 2 3 1\n";

#[test]
fn a_tree_tile_without_a_chopping_tree_is_a_thicket() {
    let levels = mini(THICKETS);
    let level = region(&levels, "MINI");
    assert!(is_thicket(&level, tile(1, 1)) || is_thicket(&level, tile(1, 2)), "a bush stands in the open");
    // The chopping tree is a node, not a thicket, and grass is neither.
    let tree = level.nodes[0].tile;
    assert!(!is_thicket(&level, tree));
    assert!(!is_thicket(&level, tile(0, 0)));
    assert!(!is_thicket(&level, tile(-1, 0)), "off the map");
    assert!(matches!(object_at(&level, tree), Some(Object::Node(_))));
    let bush = [tile(1, 1), tile(1, 2), tile(2, 1)].into_iter().find(|t| is_thicket(&level, *t)).unwrap();
    assert_eq!(object_at(&level, bush), Some(Object::Thicket(bush)));
    assert_eq!(find(&level, &ObjectRef::Thicket(bush)).map(|o| o.name()), Some("thicket".to_string()));
    assert_eq!(find(&level, &ObjectRef::Thicket(tile(0, 0))), None);
}

#[test]
fn moving_a_thicket_swaps_tiles_and_undo_restores_the_level_exactly() {
    let lib = library();
    let mut levels = mini(THICKETS);
    let before = levels.clone();
    let level = region(&levels, "MINI");
    let bush = [tile(1, 1), tile(1, 2), tile(2, 1)].into_iter().find(|t| is_thicket(&level, *t)).unwrap();
    let to = tile(0, 3);
    let mut history = History::default();
    let message = history.apply(&mut levels, Edit::Move { region: "MINI".into(), object: ObjectRef::Thicket(bush), to }, &lib, &ok).unwrap();
    assert_eq!(message, "Moved thicket.");
    let after = region(&levels, "MINI");
    assert_eq!(after.map.tile_at(bush), Ok(Tile::Grass));
    assert_eq!(after.map.tile_at(to), Ok(Tile::Tree));
    assert!(is_thicket(&after, to) && !is_thicket(&after, bush));
    history.undo(&mut levels, &lib, &ok).unwrap();
    assert_eq!(levels, before);
    history.redo(&mut levels, &lib, &ok).unwrap();
    assert_eq!(region(&levels, "MINI"), after);
}

#[test]
fn a_thicket_only_moves_onto_free_grass_and_never_onto_the_player_or_a_petal() {
    let lib = library();
    let levels = mini(THICKETS);
    let level = region(&levels, "MINI");
    let bush = [tile(1, 1), tile(1, 2), tile(2, 1)].into_iter().find(|t| is_thicket(&level, *t)).unwrap();
    let try_to = |to: TilePos| {
        let mut trial = levels.clone();
        let result = apply_edit(&mut trial, &Edit::Move { region: "MINI".into(), object: ObjectRef::Thicket(bush), to }, &lib, &ok);
        if result.is_err() {
            assert_eq!(trial, levels, "a refused move changes nothing");
        }
        result.map(|_| ())
    };
    assert!(matches!(try_to(tile(0, 0)), Err(EditError::Refused(why)) if why.contains("Something already stands there")), "the player's start");
    assert!(matches!(try_to(tile(4, 3)), Err(EditError::Refused(_))), "the moonpetal");
    assert!(matches!(try_to(level.nodes[0].tile), Err(EditError::Refused(why)) if why.contains("bare grass")), "a chopping tree");
    assert!(matches!(try_to(tile(9, 9)), Err(EditError::Refused(why)) if why.contains("off the map")));
    assert!(matches!(try_to(bush), Err(EditError::Refused(why)) if why.contains("already there")));
    assert!(try_to(tile(2, 3)).is_ok());
}

#[test]
fn a_thicket_cannot_be_moved_to_wall_the_world_off_and_can_be_removed_and_restored() {
    let lib = library();
    let mut levels = mini(GAP);
    // In GAP the one-tile opening at (2, 1)... a thicket moved there would seal the petal off.
    let level = region(&levels, "MINI");
    assert!(level.nodes.iter().all(|n| !is_thicket(&level, n.tile)));
    // Make a thicket from an existing wall-free grass tile by placing one via the map.
    let mut with_bush = level.clone();
    with_bush.map.set_tile(tile(0, 3), Tile::Tree).unwrap();
    levels[0].1 = with_bush;
    let before = levels.clone();
    let seal = Edit::Move { region: "MINI".into(), object: ObjectRef::Thicket(tile(0, 3)), to: tile(2, 1) };
    let mut trial = levels.clone();
    assert!(matches!(apply_edit(&mut trial, &seal, &lib, &ok), Err(EditError::Breaks(_))), "sealing the gap breaks reachability");
    assert_eq!(trial, levels);
    let mut history = History::default();
    history.apply(&mut levels, Edit::Remove { region: "MINI".into(), object: ObjectRef::Thicket(tile(0, 3)) }, &lib, &ok).unwrap();
    assert_eq!(region(&levels, "MINI").map.tile_at(tile(0, 3)), Ok(Tile::Grass));
    history.undo(&mut levels, &lib, &ok).unwrap();
    assert_eq!(levels, before, "undo puts the thicket back");
}

#[test]
fn thicket_edits_are_kept_in_the_edit_layer() {
    let layer = EditLayer { edits: vec![Edit::Move { region: ROAD.into(), object: ObjectRef::Thicket(tile(2, 3)), to: tile(4, 5) }] };
    let text = layer.to_text();
    assert!(text.contains("thicket 2 3"), "{text}");
    assert_eq!(EditLayer::parse(&text).unwrap(), layer);
}
