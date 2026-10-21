//! Travel between regions, and the systems that follow the player into and out of houses.
//!
//! Stepping onto an exit tile sets [`TravelOrder`] (see `movement.rs`). Next frame
//! [`apply_travel`] checks the journey against the location register, switches `Meadow`,
//! `LevelData`, and `CurrentRegion` to the destination, despawns everything the old region put
//! into the world, and asks for the new world to be built; `setup::build_world` then runs in the
//! same frame, [`finish_rebuild`] clears the request, and [`tag_region_entities`] marks what was
//! just spawned so the next journey can remove it too.
//!
//! The rules (which exits exist, where they arrive) are in `ozbound::locations`; this module
//! only moves the pieces.

use bevy::prelude::*;
use ozbound::{
    save::WorldChanges,
    world::TilePos,
};

use crate::components::*;

/// Starts a journey when an exit has been stepped on.
pub fn apply_travel(
    mut commands: Commands,
    mut order: ResMut<TravelOrder>,
    atlas: Res<Atlas>,
    mut current: ResMut<CurrentRegion>,
    mut meadow: ResMut<Meadow>,
    mut level: ResMut<LevelData>,
    mut starting: ResMut<StartingSave>,
    mut progress: ResMut<GameProgress>,
    mut talk: ResMut<TalkState>,
    mut key: ResMut<KeyTarget>,
    mut menu: ResMut<ContextMenuState>,
    mut hover: ResMut<HoverText>,
    old: Query<Entity, With<RegionEntity>>,
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

    for entity in &old {
        commands.entity(entity).despawn_recursive();
    }
    meadow.0 = target.map.clone();
    level.0 = target.clone();
    // Changes made in the region just left are not kept (see `ozbound::save`), so the new
    // region starts from its map as written.
    let mut game = starting.0.clone();
    game.region = (exit.region != atlas.locations.start().id).then(|| exit.region.clone());
    game.map_size = (target.map.width(), target.map.height());
    game.player = arrive;
    game.changes = WorldChanges::default();
    starting.0 = game;
    current.0 = exit.region.clone();

    // Nothing that pointed at the old world's entities may survive the journey.
    talk.0 = None;
    key.0 = None;
    *menu = ContextMenuState::default();
    *hover = HoverText::default();
    progress.notice = format!("You travel to {name}.");
    commands.insert_resource(RebuildWorld);
}

/// Ends the rebuild request once `setup::build_world` has run.
pub fn finish_rebuild(mut commands: Commands, rebuild: Option<Res<RebuildWorld>>) {
    if rebuild.is_some() {
        commands.remove_resource::<RebuildWorld>();
    }
}

/// Marks every root world entity that does not carry `RegionEntity` yet, so a later journey can
/// despawn it. The camera, lights, UI nodes, and children are left alone: children go with their
/// parents, and the rest belongs to the game shell.
pub fn tag_region_entities(
    mut commands: Commands,
    fresh: Query<Entity, (Added<Transform>, Without<RegionEntity>, Without<Parent>, Without<Node>, Without<Camera>, Without<DirectionalLight>)>,
) {
    for entity in &fresh {
        commands.entity(entity).insert(RegionEntity);
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
    let inside = players.get_single().ok().and_then(|player| {
        let house = level.0.house_at(player.tile)?;
        level.0.houses.iter().position(|h| h.id == house.id)
    });
    for (roof, mut visibility) in &mut roofs {
        let wanted = if inside == Some(roof.0) { Visibility::Hidden } else { Visibility::Inherited };
        if *visibility != wanted {
            *visibility = wanted;
        }
    }
    for (wall, mut transform) in &mut walls {
        let wanted = if inside == Some(wall.0) { 0.3 } else { 1.0 };
        if (transform.scale.y - wanted).abs() > 1.0e-3 {
            transform.scale.y = wanted;
        }
    }
}
