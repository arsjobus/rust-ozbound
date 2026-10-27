//! End-to-end checks of the M12 acceptance criteria through the public API (no Bevy):
//!
//! - the shipped asset files are exactly what the shipped world's objects, people, and
//!   creatures are, so the library cannot drift from the level files;
//! - the terrain meshes of neighbouring scenes stitch: no wall is drawn twice or left out on a
//!   seam, whatever is loaded;
//! - a session of edits (move, remove, place, undo, redo) keeps the world valid, is saved and
//!   replayed to the same world, and shows up in the composed open world;
//! - any edit the editor accepts can be undone to exactly the level it started from.

use std::collections::{BTreeSet, HashMap, HashSet};

use ozbound::asset_file::{export_world, Asset, Library, LIBRARY_DIR};
use ozbound::editor::{self, apply_edit, check_level, replay, Edit, EditLayer, History, Levels, Loaded, ObjectRef};
use ozbound::gathering::ResourceKind;
use ozbound::level::Level;
use ozbound::scene;
use ozbound::terrain_mesh::{build_terrain, Rect, TerrainMesh};
use ozbound::village::{builtin, World};
use ozbound::world::{Tile, TilePos};

fn world() -> World {
    builtin().expect("the shipped world loads")
}

fn tile(x: i32, y: i32) -> TilePos {
    TilePos::new(x, y)
}

fn level_of<'a>(levels: &'a Levels, id: &str) -> &'a Level {
    &levels.iter().find(|(r, _)| r == id).unwrap().1
}

// ---------------------------------------------------------------------------------------------
// Asset files
// ---------------------------------------------------------------------------------------------

#[test]
fn the_shipped_asset_files_are_exactly_what_the_shipped_world_contains() {
    let world = world();
    let expected = export_world(&world);
    let library = Library::builtin().unwrap();
    assert_eq!(library.len(), expected.len(), "one asset file per kind of object, person, and creature");
    for asset in &expected {
        assert_eq!(library.get(&asset.id), Some(asset), "{} differs from the world", asset.id);
        let path = format!("{}/{LIBRARY_DIR}/{}", env!("CARGO_MANIFEST_DIR"), asset.file_name());
        let on_disk = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{path}: {e}"));
        assert_eq!(on_disk, asset.to_text(), "{path} is out of date");
    }
    let on_disk: BTreeSet<String> = std::fs::read_dir(format!("{}/{LIBRARY_DIR}", env!("CARGO_MANIFEST_DIR")))
        .unwrap()
        .filter_map(Result::ok)
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|n| n.ends_with(".asset"))
        .collect();
    let wanted: BTreeSet<String> = expected.iter().map(Asset::file_name).collect();
    assert_eq!(on_disk, wanted, "a file in assets/library that no object uses, or a missing one");
}

#[test]
fn every_person_and_creature_in_the_world_has_an_asset_and_every_object_kind_too() {
    let world = world();
    let library = Library::builtin().unwrap();
    for (id, _, _) in world.level.npc_list() {
        assert!(library.get(id).is_some(), "no asset for the person `{id}`");
    }
    for creature in &world.level.creatures {
        assert!(library.get(&creature.id).is_some(), "no asset for the creature `{}`", creature.id);
    }
    let mut levels: Vec<&Level> = vec![&world.level];
    levels.extend(world.regions.iter().map(|(_, l)| l));
    for level in levels {
        for node in &level.nodes {
            let asset = library.get(ozbound::asset_file::node_asset_id(node.kind)).expect("an asset for every node kind");
            assert_eq!(asset.node_spawn(node.tile), Some(*node), "the asset gives the numbers the level uses");
        }
    }
}

// ---------------------------------------------------------------------------------------------
// Terrain
// ---------------------------------------------------------------------------------------------

/// The terrain of every chunk of the open world, built the way the game builds it: each chunk
/// alone, asking the overworld for what lies beside it.
fn chunk_terrains(world: &World) -> Vec<TerrainMesh> {
    let overworld = world.overworld.as_ref().expect("the shipped world is an open world");
    let levels = world.all_levels();
    let bridges: HashMap<String, HashSet<TilePos>> = levels.iter().map(|(id, l)| (id.clone(), scene::bridge_tiles(&l.map).into_iter().map(|b| b.tile).collect())).collect();
    let neighbour = |at: TilePos| {
        let (chunk, local) = overworld.to_local(at)?;
        let level = level_of(&levels, &chunk.id);
        let found = level.map.tile_at(local).ok()?;
        Some((found, found == Tile::Path && bridges[&chunk.id].contains(&local)))
    };
    overworld
        .chunks()
        .iter()
        .map(|chunk| {
            let level = level_of(&levels, &chunk.id);
            // The chunk's tiles in world coordinates, copied onto a world-sized map the way
            // `Overworld::compose` does, with only this chunk loaded.
            let composite = overworld.compose(&|id| levels.iter().find(|(r, _)| r == id).map(|(_, l)| l), &[chunk.id.as_str()]);
            let own = &bridges[&chunk.id];
            let bridge_at = |world_tile: TilePos| chunk.to_local(world_tile).is_some_and(|local| own.contains(&local));
            let rect = Rect { x0: chunk.origin.x, y0: chunk.origin.y, x1: chunk.origin.x + chunk.width, y1: chunk.origin.y + chunk.height };
            assert_eq!((level.map.width() as i32, level.map.height() as i32), (chunk.width, chunk.height));
            build_terrain(&chunk.id, &composite.map, rect, &bridge_at, &neighbour)
        })
        .collect()
}

#[test]
fn each_chunk_has_two_top_triangles_per_tile_and_belongs_to_its_scene() {
    let world = world();
    let overworld = world.overworld.as_ref().unwrap();
    let meshes = chunk_terrains(&world);
    assert_eq!(meshes.len(), overworld.chunks().len());
    for (mesh, chunk) in meshes.iter().zip(overworld.chunks()) {
        assert_eq!(mesh.scene, chunk.id);
        let tops: usize = mesh.parts.iter().map(|(_, p)| p.indices.chunks(3).filter(|t| p.normals[t[0] as usize][1] > 0.0).count()).sum();
        assert_eq!(tops as i32, 2 * chunk.width * chunk.height, "{}", chunk.id);
        // A chunk's whole ground is a handful of meshes, not a thousand entities.
        assert!(mesh.parts.len() <= 9);
    }
}

#[test]
fn the_ground_of_neighbouring_chunks_meets_without_cracks_or_overlaps() {
    let world = world();
    let meshes = chunk_terrains(&world);
    // Every corner of the surface has one height, whichever chunk draws it, and no triangle is
    // drawn twice (not even on a seam, where both chunks touch).
    let mut heights: HashMap<String, (String, String)> = HashMap::new();
    let mut seen: HashSet<String> = HashSet::new();
    for mesh in &meshes {
        for (_, part) in &mesh.parts {
            for tri in part.indices.chunks(3) {
                let corners: Vec<String> = tri.iter().map(|i| format!("{:.3},{:.3},{:.3}", part.positions[*i as usize][0], part.positions[*i as usize][1], part.positions[*i as usize][2])).collect();
                assert!(seen.insert(corners.join("|")), "a triangle is drawn twice in {}", mesh.scene);
                if part.normals[tri[0] as usize][1] <= 0.0 {
                    continue;
                }
                for i in tri {
                    let p = part.positions[*i as usize];
                    let at = format!("{:.3},{:.3}", p[0], p[2]);
                    let y = format!("{:.4}", p[1]);
                    if let Some((other_scene, other_y)) = heights.insert(at.clone(), (mesh.scene.clone(), y.clone())) {
                        assert_eq!(other_y, y, "a crack at {at}: {other_scene} and {} disagree", mesh.scene);
                    }
                }
            }
        }
    }
    assert!(!heights.is_empty());
}

#[test]
fn the_world_has_hills_and_every_flat_place_is_flat() {
    let world = world();
    let meshes = chunk_terrains(&world);
    let ys: Vec<f32> = meshes
        .iter()
        .flat_map(|m| m.parts.iter())
        .flat_map(|(_, p)| p.indices.chunks(3).filter(|t| p.normals[t[0] as usize][1] > 0.0).flat_map(|t| t.iter().map(|i| p.positions[*i as usize][1])).collect::<Vec<_>>())
        .collect();
    let (low, high) = ys.iter().fold((f32::MAX, f32::MIN), |(lo, hi), v| (lo.min(*v), hi.max(*v)));
    assert!(high - low > 0.3, "the world is not flat: {low}..{high}");
    assert!(high <= ozbound::heightfield::AMPLITUDE + 1e-4 && low >= ozbound::heightfield::WATER_LEVEL - 1e-4, "{low}..{high}");
}

#[test]
fn the_ground_of_a_region_off_the_overworld_is_a_scene_too() {
    let world = world();
    for (id, level) in world.all_levels() {
        let rect = Rect { x0: 0, y0: 0, x1: level.map.width() as i32, y1: level.map.height() as i32 };
        let bridges: HashSet<TilePos> = scene::bridge_tiles(&level.map).into_iter().map(|b| b.tile).collect();
        let mesh = build_terrain(&id, &level.map, rect, &|p| bridges.contains(&p), &|_| None);
        assert_eq!(mesh.scene, id);
        assert!(mesh.triangle_count() >= 2 * level.map.width() * level.map.height());
    }
}

// ---------------------------------------------------------------------------------------------
// Editing
// ---------------------------------------------------------------------------------------------

fn check_for<'a>(world: &'a World, levels: &'a Levels) -> impl Fn(&str, &Level) -> Result<(), String> + 'a {
    move |id, level| {
        let mut edited = world.clone();
        edited.set_levels(levels);
        edited.check_with(id, level)
    }
}

fn first_accepted(levels: &Levels, library: &Library, make: impl Fn(TilePos) -> Edit, region: &str) -> TilePos {
    let level = level_of(levels, region);
    for y in 0..level.map.height() as i32 {
        for x in 0..level.map.width() as i32 {
            let mut trial = levels.clone();
            if apply_edit(&mut trial, &make(tile(x, y)), library, &|_, _| Ok(())).is_ok() {
                return tile(x, y);
            }
        }
    }
    panic!("no tile accepts the edit");
}

#[test]
fn a_session_of_edits_is_saved_replayed_and_seen_in_the_open_world() {
    let world = world();
    let library = Library::builtin().unwrap();
    let road = world.locations.iter().find(|l| l.id == "YELLOW_BRICK_ROAD_EAST").map(|l| l.id.clone()).unwrap();
    let mut levels = world.all_levels();
    let original = levels.clone();
    let start = world.start_id().to_string();
    let mut history = History::default();

    // 1. Move a tree.
    let tree = level_of(&levels, &road).nodes.iter().find(|n| n.kind == ResourceKind::Tree).unwrap().tile;
    let to = first_accepted(&levels, &library, |to| Edit::Move { region: road.clone(), object: ObjectRef::Node(ResourceKind::Tree, tree), to }, &road);
    {
        let snapshot = levels.clone();
        let check = check_for(&world, &snapshot);
        history.apply(&mut levels, Edit::Move { region: road.clone(), object: ObjectRef::Node(ResourceKind::Tree, tree), to }, &library, &check).unwrap();
    }
    // 2. Move a villager.
    let to_pell = first_accepted(&levels, &library, |to| Edit::Move { region: start.clone(), object: ObjectRef::Npc("apothecary".into()), to }, &start);
    {
        let snapshot = levels.clone();
        let check = check_for(&world, &snapshot);
        history.apply(&mut levels, Edit::Move { region: start.clone(), object: ObjectRef::Npc("apothecary".into()), to: to_pell }, &library, &check).unwrap();
    }
    // 3. Remove a sign, 4. place a berry bush, 5. remove then undo a tree (not kept).
    let sign = level_of(&levels, &road).signs[0].tile;
    let bush_at = first_accepted(&levels, &library, |at| Edit::Place { region: road.clone(), asset: "berry_bush".into(), tile: at }, &road);
    for edit in [Edit::Remove { region: road.clone(), object: ObjectRef::Sign(sign) }, Edit::Place { region: road.clone(), asset: "berry_bush".into(), tile: bush_at }] {
        let snapshot = levels.clone();
        let check = check_for(&world, &snapshot);
        history.apply(&mut levels, edit, &library, &check).unwrap();
    }
    {
        let snapshot = levels.clone();
        let check = check_for(&world, &snapshot);
        history.apply(&mut levels, Edit::Remove { region: road.clone(), object: ObjectRef::Node(ResourceKind::Tree, to) }, &library, &check).unwrap();
        history.undo(&mut levels, &library, &check).unwrap();
    }

    // Every region is still a valid level and the whole world still fits together.
    for (id, level) in &levels {
        assert_eq!(check_level(level), Ok(()), "{id}");
    }
    let mut edited = world.clone();
    edited.set_levels(&levels);
    assert_eq!(ozbound::village::check(&edited), Ok(()));

    // Save, read back, and replay over the shipped levels: the same world.
    let dir = std::env::temp_dir().join(format!("ozbound-editor-playthrough-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let path = dir.join("saves").join("edits.txt");
    editor::save_layer(&path, &history.layer()).unwrap();
    let Loaded::Layer(layer) = editor::load_layer(&path) else { panic!("the layer was saved") };
    assert_eq!(layer.edits.len(), 4, "the removed-then-undone tree is not in the layer");
    let mut fresh = original.clone();
    let check = |id: &str, level: &Level| world.check_with(id, level);
    let replayed = replay(&mut fresh, &layer, &library, &check).unwrap();
    assert_eq!(fresh, levels);
    assert_eq!(replayed.len(), 4);
    let _ = std::fs::remove_dir_all(&dir);

    // The open world is composed from these levels, so the edits are what the player sees.
    let overworld = world.overworld.as_ref().unwrap();
    let lookup = |id: &str| levels.iter().find(|(r, _)| r == id).map(|(_, l)| l);
    let loaded: Vec<&str> = overworld.chunks().iter().map(|c| c.id.as_str()).collect();
    let composite = overworld.compose(&lookup, &loaded);
    let world_to = overworld.to_world(&road, to).unwrap();
    let world_from = overworld.to_world(&road, tree).unwrap();
    assert_eq!(composite.map.tile_at(world_to), Ok(Tile::Tree));
    assert_eq!(composite.map.tile_at(world_from), Ok(Tile::Grass));
    assert!(composite.level.nodes.iter().any(|n| n.tile == world_to && n.kind == ResourceKind::Tree));
    let pell = composite.level.npcs.iter().find(|n| n.id == "apothecary").unwrap();
    assert_eq!(overworld.to_local(pell.tile).map(|(_, local)| local), Some(to_pell));

    // Undo all the way back: the shipped world exactly.
    while history.can_undo() {
        let snapshot = levels.clone();
        let check = check_for(&world, &snapshot);
        history.undo(&mut levels, &library, &check).unwrap();
    }
    assert_eq!(levels, original);
}

#[test]
fn an_edit_that_would_break_the_story_is_refused_and_changes_nothing() {
    let world = world();
    let library = Library::builtin().unwrap();
    let mut levels = world.all_levels();
    let before = levels.clone();
    let start = world.start_id().to_string();
    let snapshot = levels.clone();
    let check = check_for(&world, &snapshot);
    let mut history = History::default();
    for who in [ObjectRef::Guide, ObjectRef::Npc("apothecary".into()), ObjectRef::Npc("woodcutter".into())] {
        let error = history.apply(&mut levels, Edit::Remove { region: start.clone(), object: who }, &library, &check).unwrap_err();
        assert!(error.to_string().starts_with("That would break the world"), "{error}");
    }
    assert_eq!(levels, before);
    assert!(history.is_empty());
}

#[test]
fn every_accepted_move_leaves_a_valid_level_and_undoes_exactly() {
    let world = world();
    let library = Library::builtin().unwrap();
    let road = "YELLOW_BRICK_ROAD_EAST";
    let levels = world.all_levels();
    let level = level_of(&levels, road);
    let objects: Vec<ObjectRef> = level
        .nodes
        .iter()
        .map(|n| ObjectRef::Node(n.kind, n.tile))
        .chain(level.signs.iter().map(|s| ObjectRef::Sign(s.tile)))
        .chain(level.moonpetals.iter().map(|t| ObjectRef::Moonpetal(*t)))
        .collect();
    assert!(objects.len() >= 10);
    let (mut accepted, mut refused) = (0, 0);
    for (index, object) in objects.iter().enumerate() {
        // A sample of tiles per object keeps this quick; the offset differs per object.
        for y in (0..level.map.height() as i32).step_by(4) {
            for x in ((index as i32 % 3)..level.map.width() as i32).step_by(5) {
                let mut trial = levels.clone();
                let edit = Edit::Move { region: road.into(), object: object.clone(), to: tile(x, y) };
                match apply_edit(&mut trial, &edit, &library, &|_, _| Ok(())) {
                    Ok((inverse, _)) => {
                        accepted += 1;
                        assert_eq!(check_level(level_of(&trial, road)), Ok(()), "{edit:?}");
                        apply_edit(&mut trial, &inverse, &library, &|_, _| Ok(())).unwrap();
                        assert_eq!(trial, levels, "undoing {edit:?} did not restore the level");
                    }
                    Err(_) => {
                        refused += 1;
                        assert_eq!(trial, levels, "a refused edit changed the level: {edit:?}");
                    }
                }
            }
        }
    }
    assert!(accepted > 20 && refused > 20, "accepted {accepted}, refused {refused}");
}

#[test]
fn an_edit_layer_that_no_longer_fits_the_world_is_refused_whole() {
    let world = world();
    let library = Library::builtin().unwrap();
    let mut levels = world.all_levels();
    let before = levels.clone();
    let layer = EditLayer::parse("ozbound-edits 1\nremove YELLOW_BRICK_ROAD_EAST sign 0 0\n").unwrap();
    let check = |id: &str, level: &Level| world.check_with(id, level);
    let error = replay(&mut levels, &layer, &library, &check).unwrap_err();
    assert!(error.starts_with("edit 1 (remove sign 0 0 in YELLOW_BRICK_ROAD_EAST)"), "{error}");
    assert_eq!(levels, before);
}

#[test]
fn a_thicket_of_the_shipped_road_can_be_moved_removed_and_restored() {
    let world = world();
    let library = Library::builtin().unwrap();
    let road = "YELLOW_BRICK_ROAD_EAST";
    let mut levels = world.all_levels();
    let before = levels.clone();
    let thickets = scene::thicket_tiles(level_of(&levels, road));
    assert!(thickets.len() > 100, "the road is lined with thickets");
    // Every thicket the scene draws is one the editor can find, and nothing else is.
    let level = level_of(&levels, road);
    assert!(thickets.iter().all(|t| editor::is_thicket(level, *t)));
    assert_eq!(editor::object_at(level, thickets[0]).map(|o| o.reference()), Some(ObjectRef::Thicket(thickets[0])));
    // A thicket next to the road goes to the nearest free grass tile that keeps the world valid.
    let bush = thickets.iter().copied().find(|t| t.x > 3 && t.y > 3 && t.y < 36).unwrap();
    let check = |id: &str, l: &Level| world.check_with(id, l);
    let mut history = History::default();
    let to = first_accepted(&levels, &library, |to| Edit::Move { region: road.into(), object: ObjectRef::Thicket(bush), to }, road);
    history.apply(&mut levels, Edit::Move { region: road.into(), object: ObjectRef::Thicket(bush), to }, &library, &check).unwrap();
    let after = level_of(&levels, road);
    assert_eq!(after.map.tile_at(bush), Ok(Tile::Grass));
    assert_eq!(after.map.tile_at(to), Ok(Tile::Tree));
    assert!(scene::thicket_tiles(after).contains(&to), "the scene now draws it at the new tile");
    assert!(!scene::thicket_tiles(after).contains(&bush));
    history.apply(&mut levels, Edit::Remove { region: road.into(), object: ObjectRef::Thicket(to) }, &library, &check).unwrap();
    assert_eq!(level_of(&levels, road).map.tile_at(to), Ok(Tile::Grass));
    history.undo(&mut levels, &library, &check).unwrap();
    history.undo(&mut levels, &library, &check).unwrap();
    assert_eq!(levels, before, "two undos restore the world exactly");
}

// ---------------------------------------------------------------------------------------------
// Houses (M14): objects that cover more than one tile
// ---------------------------------------------------------------------------------------------

/// A one-tile shift of a house that the editor accepts, if any.
fn accepted_house_shift(levels: &Levels, library: &Library, world: &World, region: &str, house: &str) -> Option<(TilePos, Edit)> {
    let origin = level_of(levels, region).houses.iter().find(|h| h.id == house)?.origin;
    let check = |id: &str, l: &Level| world.check_with(id, l);
    [(1, 0), (-1, 0), (0, 1), (0, -1), (2, 0), (-2, 0), (0, 2), (0, -2)].into_iter().find_map(|(dx, dy)| {
        let to = tile(origin.x + dx, origin.y + dy);
        let edit = Edit::Move { region: region.into(), object: ObjectRef::House(house.into()), to };
        let mut trial = levels.clone();
        apply_edit(&mut trial, &edit, library, &check).ok().map(|_| (to, edit))
    })
}

#[test]
fn every_shipped_house_is_an_object_the_editor_can_find_by_any_of_its_tiles() {
    let world = world();
    let levels = world.all_levels();
    let mut found = 0;
    for (_, level) in &levels {
        for house in &level.houses {
            assert_eq!(house.ground.len(), house.footprint().len(), "{} remembers the land under it", house.id);
            for t in house.footprint() {
                // People standing inside win the click; otherwise the house does.
                let object = editor::object_at(level, t).expect("something is editable on every house tile");
                if !matches!(object, editor::Object::House(_)) {
                    assert!(house.interior().contains(&t), "only a person on the floor can hide the house");
                } else {
                    assert_eq!(object.reference(), ObjectRef::House(house.id.clone()));
                }
            }
            found += 1;
        }
    }
    assert_eq!(found, 4, "three houses in Bluebrook and the waystation");
}

#[test]
fn a_shipped_house_moves_with_what_is_inside_and_undo_restores_the_world_exactly() {
    let world = world();
    let library = Library::builtin().unwrap();
    let check = |id: &str, l: &Level| world.check_with(id, l);
    let mut moved_any = false;
    for (region, house_id) in world.all_levels().iter().flat_map(|(r, l)| l.houses.iter().map(move |h| (r.clone(), h.id.clone()))) {
        let mut levels = world.all_levels();
        let before = levels.clone();
        let Some((to, edit)) = accepted_house_shift(&levels, &library, &world, &region, &house_id) else { continue };
        let old = level_of(&levels, &region).houses.iter().find(|h| h.id == house_id).unwrap().clone();
        let inside: Vec<(String, TilePos)> = level_of(&levels, &region).npc_list().into_iter().filter(|(_, t, _)| old.interior().contains(t)).map(|(id, t, _)| (id.to_string(), t)).collect();
        let mut history = History::default();
        history.apply(&mut levels, edit, &library, &check).unwrap();
        let after = level_of(&levels, &region);
        let house = after.houses.iter().find(|h| h.id == house_id).unwrap();
        assert_eq!(house.origin, to);
        let (dx, dy) = (to.x - old.origin.x, to.y - old.origin.y);
        // The new footprint is stamped, the land left behind is as it was before the house.
        for t in house.footprint() {
            assert_eq!(after.map.tile_at(t), Ok(house.tile_at(t).unwrap()));
        }
        for (i, t) in old.footprint().into_iter().enumerate() {
            if !house.contains(t) {
                assert_eq!(after.map.tile_at(t), Ok(old.ground[i]), "{t:?} is open land again");
            }
        }
        // People who were on the floor are still on the floor, one shift away.
        for (id, t) in &inside {
            let now = after.npc_list().into_iter().find(|(n, _, _)| n == id).map(|(_, t, _)| t).unwrap();
            assert_eq!(now, tile(t.x + dx, t.y + dy), "{id} travelled with the house");
            assert!(house.interior().contains(&now));
        }
        assert!(check_level(after).is_ok());
        history.undo(&mut levels, &library, &check).unwrap();
        assert_eq!(levels, before, "undo puts the house, its people, and the land back exactly");
        history.redo(&mut levels, &library, &check).unwrap();
        history.undo(&mut levels, &library, &check).unwrap();
        assert_eq!(levels, before);
        moved_any = true;
    }
    assert!(moved_any, "at least one shipped house can be nudged");
}

#[test]
fn a_house_with_people_inside_exists_and_they_stay_inside_after_a_move() {
    let world = world();
    let levels = world.all_levels();
    let with_people = levels.iter().flat_map(|(r, l)| l.houses.iter().map(move |h| (r, l, h))).filter(|(_, l, h)| l.npc_list().iter().any(|(_, t, _)| h.interior().contains(t))).count();
    assert!(with_people >= 1, "Mistress Pell keeps shop inside her house");
}

#[test]
fn houses_cannot_leave_their_plot_sit_on_water_or_be_removed() {
    let world = world();
    let library = Library::builtin().unwrap();
    let check = |id: &str, l: &Level| world.check_with(id, l);
    let (region, house) = world.all_levels().iter().flat_map(|(r, l)| l.houses.iter().map(move |h| (r.clone(), h.clone()))).next().unwrap();
    let mut levels = world.all_levels();
    let before = levels.clone();
    let far = tile(1, 1);
    for to in [far, tile(-3, 4), tile(house.origin.x + 40, house.origin.y)] {
        let edit = Edit::Move { region: region.clone(), object: ObjectRef::House(house.id.clone()), to };
        assert!(apply_edit(&mut levels, &edit, &library, &check).is_err(), "{to:?} is outside the plot or off the map");
        assert_eq!(levels, before, "a refused move changes nothing");
    }
    let remove = Edit::Remove { region: region.clone(), object: ObjectRef::House(house.id.clone()) };
    let error = apply_edit(&mut levels, &remove, &library, &check).unwrap_err();
    assert!(error.to_string().contains("moved, but not removed"), "{error}");
    assert_eq!(levels, before);
}

#[test]
fn house_edits_are_saved_in_the_edit_layer_and_replay_to_the_same_world() {
    let world = world();
    let library = Library::builtin().unwrap();
    let check = |id: &str, l: &Level| world.check_with(id, l);
    let mut levels = world.all_levels();
    let (region, house) = levels.iter().flat_map(|(r, l)| l.houses.iter().map(move |h| (r.clone(), h.id.clone()))).find(|(r, h)| accepted_house_shift(&world.all_levels(), &library, &world, r, h).is_some()).unwrap();
    let (_, edit) = accepted_house_shift(&levels, &library, &world, &region, &house).unwrap();
    let mut history = History::default();
    history.apply(&mut levels, edit, &library, &check).unwrap();
    let text = history.layer().to_text();
    assert!(text.contains(&format!("house {house}")), "{text}");
    let layer = EditLayer::parse(&text).unwrap();
    let mut replayed = world.all_levels();
    replay(&mut replayed, &layer, &library, &check).unwrap();
    assert_eq!(replayed, levels);
}
