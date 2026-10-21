//! The open world end to end, without Bevy: a player walks from the village start to the far end
//! of the road while chunks stream in and out around them, the zone music crossfades at the
//! zone border, and the walk never meets a loading boundary.

use ozbound::overworld::TrackSpec;
use ozbound::pathfinding::{plan_route, Approach};
use ozbound::soundscape::{Mixer, FADE_SECONDS};
use ozbound::village::builtin;
use ozbound::world::TilePos;

#[test]
fn one_walk_from_the_village_to_the_end_of_the_road_streams_chunks_and_fades_the_music() {
    let world = builtin().expect("shipped world loads");
    let overworld = world.overworld.as_ref().expect("an open world");
    let level_of = |id: &str| world.level_of(id);

    let start = overworld.to_world("MUNCHKIN_LANDS", world.level.player_start).unwrap();
    let road = overworld.chunk("YELLOW_BRICK_ROAD_EAST").unwrap();
    let end = road.to_world(TilePos::new(road.width - 3, 9), );

    // The player starts with the village loaded and the road not yet.
    let mut loaded = overworld.wanted(start, &[]);
    assert_eq!(loaded, ["MUNCHKIN_LANDS"], "the road is still far away at the start");
    let mut composite = overworld.compose(&level_of, &loaded);

    // Plan the whole walk on the fully loaded world, then replay it step by step the way the
    // game does: each step must be walkable on the map that is loaded at that moment.
    let everything: Vec<String> = overworld.chunks().iter().map(|c| c.id.clone()).collect();
    let full = overworld.compose(&level_of, &everything);
    let route = plan_route(&full.map, start, end, Approach::OnTile).expect("the world is connected");

    let first_zone = overworld.zone_at(start).unwrap();
    let mut mixer = Mixer::starting_with(first_zone.track.clone(), FADE_SECONDS);
    mixer.tick(0.0);
    let mut zones_seen: Vec<String> = vec![first_zone.id.clone()];
    let mut road_loaded_at = None;
    let mut village_dropped_at = None;
    let mut crossfade_midpoint_seen = false;
    let mut at = start;

    for step in &route {
        assert!(composite.map.can_walk_to(*step), "{step:?} is not walkable on the chunks loaded at that moment (loaded: {loaded:?})");
        assert!(composite.level.exit_at(*step).is_none(), "no scene change on the way");
        at = *step;

        let wanted = overworld.wanted(at, &loaded);
        if wanted != loaded {
            if wanted.contains(&"YELLOW_BRICK_ROAD_EAST".to_string()) && road_loaded_at.is_none() {
                road_loaded_at = Some(at.x);
            }
            if !wanted.contains(&"MUNCHKIN_LANDS".to_string()) && village_dropped_at.is_none() {
                village_dropped_at = Some(at.x);
            }
            loaded = wanted;
            composite = overworld.compose(&level_of, &loaded);
        }
        assert!(loaded.contains(&overworld.chunk_at(at).unwrap().id), "the chunk under the player is always loaded");

        let zone = overworld.zone_at(at).unwrap();
        if zones_seen.last() != Some(&zone.id) {
            zones_seen.push(zone.id.clone());
            mixer.set_target(Some(zone.track.clone()));
        }
        // About a quarter of a second per tile.
        let frame = mixer.tick(0.25);
        if frame.gains.len() == 2 {
            let (a, b) = (frame.gains[0].1, frame.gains[1].1);
            crossfade_midpoint_seen |= a > 0.0 && a < 1.0 && b > 0.0 && b < 1.0;
        }
    }

    assert_eq!(at, end);
    assert_eq!(zones_seen, ["bluebrook", "yellow_brick_road"]);
    let seam = road.origin.x;
    let loaded_at = road_loaded_at.expect("the road streamed in");
    assert!(loaded_at < seam - 5, "the road loads well before the player reaches the border (x = {loaded_at}, border = {seam})");
    let dropped_at = village_dropped_at.expect("the village was dropped once far behind");
    assert!(dropped_at > seam, "the village stays loaded until the player is well past the border (x = {dropped_at})");
    assert!(crossfade_midpoint_seen, "both tracks were audible together while fading");
    let road_track = TrackSpec { music: Some("yellow_road_east.ogg".into()), theme: Some("road".into()) };
    assert_eq!(mixer.voice_count(), 1, "the village music has faded out and stopped by the end of the walk");
    assert_eq!(mixer.gain_of(&road_track), 1.0);
}

#[test]
fn the_start_of_the_game_is_unchanged_a_player_in_a_save_resumes_in_their_chunk() {
    let world = builtin().unwrap();
    let overworld = world.overworld.as_ref().unwrap();
    // A save in the road's own tiles maps to world tiles and back to the same local tile.
    let local = world.level_of("YELLOW_BRICK_ROAD_EAST").unwrap().player_start;
    let in_world = overworld.to_world("YELLOW_BRICK_ROAD_EAST", local).unwrap();
    let (chunk, back) = overworld.to_local(in_world).unwrap();
    assert_eq!((chunk.id.as_str(), back), ("YELLOW_BRICK_ROAD_EAST", local));
    // Resuming there loads the road, and the village only if it is near enough.
    let wanted = overworld.wanted(in_world, &[]);
    assert!(wanted.contains(&"YELLOW_BRICK_ROAD_EAST".to_string()));
}

#[test]
fn every_walkable_tile_has_a_zone_and_the_zones_name_real_music() {
    let world = builtin().unwrap();
    let overworld = world.overworld.as_ref().unwrap();
    for zone in overworld.zones() {
        let choice = ozbound::music::choose(std::path::Path::new("/nonexistent"), zone.track.music.as_deref(), zone.track.theme.as_deref());
        assert!(!matches!(choice.track, ozbound::music::Track::Silence), "zone {} would be silent", zone.id);
    }
}
