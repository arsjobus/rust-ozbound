//! Journeys that leave the continuous overworld, and the roofs of walk-in houses.
//!
//! Regions placed in `assets/world/overworld.world` are joined by walking: their paths meet at
//! the shared border and `streaming::stream_chunks` loads the next chunk as the player nears
//! it, so there is no journey at all. What is left for this module is the one place a scene
//! change still happens: stepping onto a level `[exits]` tile (a cave, say), which leads to a
//! region off the overworld, and the exit back.
//!
//! Stepping onto an exit sets [`TravelOrder`] (see `movement.rs`). Next frame [`apply_travel`]
//! checks the journey against the location register, stores what the player did in the chunks
//! being left (`ChunkMemory`), despawns the old world, points `CurrentRegion` (and, for a place
//! off the overworld, `Meadow` and `LevelData`) at the destination, and asks for the new world
//! to be built; `setup::build_world` then runs in the same frame and [`finish_rebuild`] clears
//! the request.
//!
//! The rules (which exits exist, where they arrive) are in `ozbound::locations`; this module
//! only moves the pieces.

use bevy::prelude::*;
use ozbound::{
    save::WorldChanges,
    world::TilePos,
};

use crate::components::*;
use crate::streaming;

/// Starts a journey when an exit has been stepped on.
pub fn apply_travel(
    mut commands: Commands,
    mut order: ResMut<TravelOrder>,
    atlas: Res<Atlas>,
    streaming: Res<Streaming>,
    mut memory: ResMut<ChunkMemory>,
    mut current: ResMut<CurrentRegion>,
    mut meadow: ResMut<Meadow>,
    mut level: ResMut<LevelData>,
    mut starting: ResMut<StartingSave>,
    mut progress: ResMut<GameProgress>,
    mut pointing: streaming::Pointing,
    mine: Query<ProgressRef, With<Player>>,
    old: Query<Entity, Or<(With<RegionEntity>, With<ChunkEntity>, With<Player>, With<WorldFloor>)>>,
    petals: Query<&Interactable, With<Moonpetal>>,
    nodes: Query<(&ResourceNode, &Interactable)>,
) {
    let Some(exit) = order.0.take() else { return };
    // The register is the authority: an exit only works if the place declares it and the
    // destination can be played. The data checks make this true for shipped maps; a custom
    // map that breaks the rule gets a message instead of a broken world.
    if !atlas.locations.can_travel(&current.0, &exit.region) {
        progress.notice = format!("\"{}\" is not open yet.", exit.label);
        return;
    }
    let Some(target) = atlas.level(&exit.region) else {
        progress.notice = format!("\"{}\" is not open yet.", exit.label);
        return;
    };
    let arrive: TilePos = exit.arrive;
    if !target.map.can_walk_to(arrive) {
        progress.notice = format!("\"{}\" leads nowhere you can walk.", exit.label);
        return;
    }
    let name = atlas.locations.get(&exit.region).map_or(exit.region.as_str(), |l| l.name.as_str()).to_string();

    // What the player did in the chunks about to be dropped is kept for the way back.
    if let (false, Some(overworld)) = (streaming.interior, atlas.overworld.as_ref()) {
        let petal_tiles: Vec<TilePos> = petals.iter().map(|p| p.tile).collect();
        let node_state: Vec<(TilePos, u32)> = nodes.iter().map(|(n, at)| (at.tile, n.0.remaining_actions())).collect();
        streaming::stash(&mut memory, overworld, &atlas, &streaming.loaded, &petal_tiles, &node_state);
    }
    for entity in &old {
        commands.entity(entity).despawn_recursive();
    }
    // A destination on the overworld is stitched together by `build_world`; any other place is
    // played on its own map in its own coordinates.
    if atlas.overworld.as_ref().is_none_or(|o| o.chunk(&exit.region).is_none()) {
        meadow.0 = target.map.clone();
        level.0 = target.clone();
    }
    // Changes made in a place just left are kept only for overworld chunks (see
    // `ozbound::save`); the new place starts from its map as written.
    let mut game = starting.0.clone();
    // The player entity is about to be despawned and spawned again by `build_world`, which
    // reads its progress from `StartingSave`, so what the player has goes in here first.
    if let Ok(progress) = mine.get_single() {
        progress.write_to(&mut game);
    }
    game.region = (exit.region != atlas.locations.start().id).then(|| exit.region.clone());
    game.map_size = (target.map.width(), target.map.height());
    game.player = arrive;
    game.changes = WorldChanges::default();
    starting.0 = game;
    current.0 = exit.region.clone();

    // Nothing that pointed at the old world's entities may survive the journey.
    pointing.clear();
    progress.notice = format!("You travel to {name}.");
    commands.insert_resource(RebuildWorld);
}

/// Ends the rebuild request once `setup::build_world` has run.
pub fn finish_rebuild(mut commands: Commands, rebuild: Option<Res<RebuildWorld>>) {
    if rebuild.is_some() {
        commands.remove_resource::<RebuildWorld>();
    }
}

/// While the player is inside a house the roof is hidden and the walls drop to a low rim, so
/// the room can be seen from the follow camera; outside, the house is whole again.
pub fn house_cutaways(
    level: Res<LevelData>,
    players: Query<&Player>,
    mut roofs: Query<(&HouseRoof, &mut Visibility)>,
    mut walls: Query<(&HouseWalls, &mut Transform)>,
) {
    let inside = players.get_single().ok().and_then(|player| level.0.house_at(player.tile)).map(|house| house.id.as_str());
    for (roof, mut visibility) in &mut roofs {
        let wanted = if inside == Some(roof.0.as_str()) { Visibility::Hidden } else { Visibility::Inherited };
        if *visibility != wanted {
            *visibility = wanted;
        }
    }
    for (wall, mut transform) in &mut walls {
        let wanted = if inside == Some(wall.0.as_str()) { 0.3 } else { 1.0 };
        if (transform.scale.y - wanted).abs() > 1.0e-3 {
            transform.scale.y = wanted;
        }
    }
}
