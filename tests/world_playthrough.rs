//! End-to-end check of the M9 acceptance criteria using only the public API (no Bevy): a
//! player walks the village, goes in and out of every house, leaves for the next region over
//! the bridge, comes back, and a save made away from the start survives a restart.

use std::path::{Path, PathBuf};

use ozbound::level::Level;
use ozbound::locations::check_world;
use ozbound::music::{self, Track};
use ozbound::pathfinding::{plan_route, Approach};
use ozbound::save::{SaveContext, SaveGame};
use ozbound::scene;
use ozbound::village::{builtin, World};
use ozbound::world::{Tile, TilePos};

fn world() -> World { builtin().expect("the built-in world is valid") }

fn walk(level: &Level, from: TilePos, to: TilePos) -> Vec<TilePos> {
    plan_route(&level.map, from, to, Approach::OnTile).unwrap_or_else(|| panic!("no route from {from:?} to {to:?}"))
}

#[test]
fn the_register_the_levels_and_the_exits_agree() {
    let world = world();
    assert_eq!(world.start_id(), "MUNCHKIN_LANDS");
    assert!(world.level_of("YELLOW_BRICK_ROAD_EAST").is_some());
    assert!(world.level_of("WOODS_FIRST").is_none(), "planned places have no level yet");
    check_world(&world.locations, |id| world.level_of(id)).expect("every exit arrives somewhere safe");
}

#[test]
fn every_house_can_be_walked_into_and_out_of() {
    let world = world();
    for (id, level) in std::iter::once((world.start_id().to_string(), &world.level)).chain(world.regions.iter().map(|(id, l)| (id.clone(), l))) {
        for house in &level.houses {
            let route = walk(level, level.player_start, house.door_tile());
            assert_eq!(*route.last().unwrap(), house.door_tile(), "{id}/{}", house.id);
            // Through the door and onto every floor tile, then back out to where we started.
            for floor in house.interior() {
                let inside = walk(level, house.door_tile(), floor);
                assert!(inside.iter().all(|t| level.map.can_walk_to(*t)), "{id}/{}", house.id);
                assert!(inside.iter().all(|t| house.contains(*t)), "walls stop you leaving any way but the door: {id}/{}", house.id);
                let out = walk(level, floor, house.outside_tile());
                assert!(out.contains(&house.door_tile()), "the only way out is the door: {id}/{}", house.id);
            }
            // From outside the walls on any other side there is no way through.
            for wall in house.footprint().into_iter().filter(|t| level.map.tile_at(*t) == Ok(Tile::Wall)) {
                assert!(!level.map.can_walk_to(wall), "{id}/{}: walls block", house.id);
            }
        }
    }
}

#[test]
fn people_can_be_visited_inside_their_houses() {
    let world = world();
    let level = &world.level;
    let pell = level.npcs.iter().find(|n| n.id == "apothecary").unwrap();
    let house = level.house_at(pell.tile).expect("Pell keeps shop indoors");
    let route = plan_route(&level.map, level.player_start, pell.tile, Approach::Adjacent).expect("the player can walk up to Pell");
    assert!(route.contains(&house.door_tile()), "reaching her means going in through the door");
}

#[test]
fn a_journey_east_across_the_bridge_and_back_again_without_a_scene_change() {
    let world = world();
    let overworld = world.overworld.as_ref().expect("the shipped world is an open world");
    let village = world.level_of("MUNCHKIN_LANDS").unwrap();
    let road = world.level_of("YELLOW_BRICK_ROAD_EAST").unwrap();
    assert!(village.exits.is_empty() && road.exits.is_empty(), "no exit tile stands between the village and the road");
    assert!(world.locations.can_travel("MUNCHKIN_LANDS", "YELLOW_BRICK_ROAD_EAST"));

    // Both chunks stitched into one map: one continuous route from the village start to the far bank.
    let both = ["MUNCHKIN_LANDS", "YELLOW_BRICK_ROAD_EAST"];
    let composite = overworld.compose(&|id: &str| world.level_of(id), &both);
    let origin = overworld.chunk("YELLOW_BRICK_ROAD_EAST").unwrap().origin;
    let bridges: Vec<_> = scene::bridge_tiles(&road.map).into_iter().map(|b| TilePos::new(b.tile.x + origin.x, b.tile.y + origin.y)).collect();
    assert!(bridges.len() >= 3, "a proper bridge");
    let far_bank = TilePos::new(bridges.last().unwrap().x + 2, bridges[0].y);
    let across = walk(&composite.level, composite.level.player_start, far_bank);
    for b in &bridges {
        assert!(across.contains(b), "the crossing goes over {b:?}");
    }
    assert!(across.iter().all(|t| composite.level.exit_at(*t).is_none()), "no tile on the way sends you anywhere");
    let seam_y = (0..village.map.height() as i32).find(|y| village.map.tile_at(TilePos::new(village.map.width() as i32 - 1, *y)) == Ok(Tile::Path)).expect("the village road reaches its east edge");
    let seam = TilePos::new(origin.x, seam_y);
    assert!(across.contains(&seam) && across.contains(&TilePos::new(origin.x - 1, seam_y)), "the route runs straight over the border");

    // And back again.
    let home = walk(&composite.level, far_bank, composite.level.player_start);
    assert_eq!(*home.last().unwrap(), composite.level.player_start);
}

#[test]
fn the_thickets_close_the_paths_and_trees_you_can_chop_are_elsewhere() {
    let world = world();
    for (_, level) in world.regions.iter().map(|(i, l)| (i, l)).chain(std::iter::once((&String::new(), &world.level))) {
        for tile in scene::thicket_tiles(level) {
            assert!(!level.map.can_walk_to(tile));
            assert!(level.nodes.iter().all(|n| n.tile != tile));
        }
    }
}

fn ctx(world: &World, id: &str) -> (Level, ()) { (world.level_of(id).unwrap().clone(), ()) }

#[test]
fn a_save_made_in_the_next_region_survives_a_restart() {
    let world = world();
    let (road, ()) = ctx(&world, "YELLOW_BRICK_ROAD_EAST");
    let mut game = SaveGame::new_game(&road);
    game.region = Some("YELLOW_BRICK_ROAD_EAST".to_string());
    // Every chunk is the same size now, so the saved map size no longer tells two regions apart:
    // the `region` line picks the level, and the player stands somewhere only the road has.
    assert_eq!((road.map.width(), road.map.height()), (world.level.map.width(), world.level.map.height()));
    let (w, h) = (road.map.width() as i32, road.map.height() as i32);
    let only_on_the_road = (0..h)
        .flat_map(|y| (0..w).map(move |x| ozbound::world::TilePos::new(x, y)))
        .find(|t| road.map.can_walk_to(*t) && road.map.is_reachable(road.player_start, *t) && !world.level.map.can_walk_to(*t))
        .expect("the road has ground the village lacks");
    game.player = only_on_the_road;
    let text = game.to_text();

    // "Restart": read only the region line, pick that region's level, load the rest.
    let region = SaveGame::peek_region(&text).expect("a region line");
    let level = world.level_of(&region).expect("a playable region");
    let loaded = SaveGame::from_text(&text, &SaveContext { level, quests: &world.content.quests, bestiary: &world.bestiary }).expect("loads against the road map");
    assert_eq!(loaded, game);

    // The same file read against the village's map is refused, so a wrong guess cannot slip by.
    let wrong = SaveGame::from_text(&text, &SaveContext { level: &world.level, quests: &world.content.quests, bestiary: &world.bestiary });
    assert!(wrong.is_err(), "the road's save does not fit the village map");
}

#[test]
fn a_save_from_before_regions_still_loads_in_the_village() {
    let world = world();
    let start = world.level.player_start;
    let v1 = format!("ozbound-save 1\nworld {} {}\nplayer {} {}\nend\n", world.level.map.width(), world.level.map.height(), start.x, start.y);
    let game = SaveGame::from_text(&v1, &SaveContext { level: &world.level, quests: &world.content.quests, bestiary: &world.bestiary }).unwrap();
    assert_eq!(game.region, None);
}

#[test]
fn every_region_has_its_own_music_and_a_missing_track_never_means_silence() {
    let world = world();
    let empty = scratch_dir();
    let mut themes = Vec::new();
    for location in world.locations.playable() {
        let choice = music::choose(&empty, location.music.as_deref(), location.theme.as_deref());
        match &choice.track {
            Track::Builtin { theme, bytes } => {
                assert!(music::looks_like("wav", bytes), "{}", location.id);
                themes.push(theme.clone());
            }
            other => panic!("{}: expected the built-in theme, got {}", location.id, other.label()),
        }
        assert!(choice.note.is_some(), "{}: the missing file is reported", location.id);
    }
    themes.sort();
    themes.dedup();
    assert_eq!(themes.len(), world.locations.playable().count(), "no two regions share a theme");
}

#[test]
fn a_file_in_the_bgm_folder_replaces_the_builtin_theme() {
    let world = world();
    let dir = scratch_dir();
    let location = world.locations.start();
    let name = location.music.clone().expect("the start region names a track");
    let bytes: &[u8] = if name.ends_with(".mp3") { b"ID3\x04\0\0\0\0\0\x22pretend this is an mp3" } else { b"OggS\0\x02pretend this is vorbis" };
    std::fs::write(dir.join(&name), bytes).unwrap();
    let choice = music::choose(&dir, Some(&name), location.theme.as_deref());
    assert!(matches!(choice.track, Track::File { .. }), "{}", choice.track.label());
    assert_eq!(choice.note, None);
}

/// The start village's track is the mp3 that ships in `assets/bgm/`: the location register and
/// the overworld zone agree on its name, the file passes the signature check, and `choose`
/// plays that file (not the built-in theme).
#[test]
fn the_shipped_start_village_mp3_is_found_and_chosen() {
    let world = world();
    let location = world.locations.start();
    assert_eq!(location.music.as_deref(), Some("bluebrook.mp3"));
    let zone = world.overworld.as_ref().unwrap().zone_at(world.level.player_start).unwrap();
    assert_eq!(zone.track.music.as_deref(), Some("bluebrook.mp3"), "the zone and the register name the same file");
    let choice = music::choose(Path::new(music::BGM_DIR), zone.track.music.as_deref(), zone.track.theme.as_deref());
    assert_eq!(choice.note, None, "{:?}", choice.note);
    match choice.track {
        Track::File { name, bytes } => {
            assert_eq!(name, "bluebrook.mp3");
            assert!(music::looks_like("mp3", &bytes));
        }
        other => panic!("expected the mp3 file, got {}", other.label()),
    }
}

fn scratch_dir() -> PathBuf {
    use std::sync::atomic::{AtomicU32, Ordering};
    static NEXT: AtomicU32 = AtomicU32::new(0);
    let dir = std::env::temp_dir().join(format!("ozbound-world-{}-{}", std::process::id(), NEXT.fetch_add(1, Ordering::Relaxed)));
    std::fs::create_dir_all(&dir).unwrap();
    assert!(Path::new(&dir).is_dir());
    dir
}
