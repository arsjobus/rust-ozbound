//! Mouse control: hover labels and left-click to walk or interact.
//! The cursor becomes a ray; `ozbound::picking` decides what it hits and
//! `ozbound::pathfinding` plans the route. This module only connects them to Bevy.

use std::f32::consts::FRAC_PI_4;

use bevy::prelude::*;
use bevy::window::PrimaryWindow;
use ozbound::{
    interaction::hover_text,
    pathfinding::{find_path, plan_route},
    picking::{ground_to_tile, pick, Pick, PickBox},
    world::TilePos,
};

use crate::components::*;

/// Height of the walkable ground plane the cursor ray is intersected with.
const GROUND_Y: f32 = 0.0;
const MARKER_SECONDS: f32 = 0.7;

pub fn pointer(
    mut commands: Commands,
    windows: Query<&Window, With<PrimaryWindow>>,
    cameras: Query<(&Camera, &GlobalTransform), With<MainCamera>>,
    buttons: Res<ButtonInput<MouseButton>>,
    meadow: Res<Meadow>,
    assets: Res<ClickAssets>,
    targets: Query<(Entity, &Interactable)>,
    mut players: Query<&mut Player>,
    mut hover: ResMut<HoverText>,
    mut progress: ResMut<GameProgress>,
) {
    hover.0.clear();
    let Ok(window) = windows.get_single() else { return };
    let Some(cursor) = window.cursor_position() else { return };
    let Ok((camera, camera_transform)) = cameras.get_single() else { return };
    let Ok(ray) = camera.viewport_to_world(camera_transform, cursor) else { return };
    let Ok(mut player) = players.get_single_mut() else { return };
    let map = &meadow.0;

    let boxes = targets.iter().map(|(entity, it)| PickBox { id: entity, min: it.pick_min, max: it.pick_max });
    let picked = pick(ray.origin.to_array(), ray.direction.as_vec3().to_array(), GROUND_Y, boxes);

    // Label under the cursor.
    match picked {
        Pick::Object { id, .. } => {
            if let Ok((_, it)) = targets.get(id) {
                hover.0 = hover_text(it.kind, it.name);
            }
        }
        Pick::Ground(hit) => {
            if ground_to_tile(map, hit.x, hit.z).is_some_and(|tile| map.can_walk_to(tile)) {
                hover.0 = "Walk here".into();
            }
        }
        Pick::Nothing => {}
    }

    if !buttons.just_pressed(MouseButton::Left) {
        return;
    }

    // Plan from the tile being walked toward, so a new click doesn't snap the player backwards.
    let origin = player.task.planning_origin(player.tile);
    match picked {
        Pick::Object { id, .. } => {
            let Ok((_, it)) = targets.get(id) else { return };
            match plan_route(map, origin, it.tile, it.approach) {
                Some(route) => {
                    let task = player.task.rerouted(route, Some(id));
                    player.task = task;
                    player.work = None;
                    spawn_marker(&mut commands, &assets, marker_position(map, it.tile), true);
                }
                None => progress.journal = "You can't reach that.".into(),
            }
        }
        Pick::Ground(hit) => {
            let Some(tile) = ground_to_tile(map, hit.x, hit.z) else { return };
            if !map.can_walk_to(tile) {
                progress.journal = "You can't walk there.".into();
                return;
            }
            match find_path(map, origin, tile) {
                Some(route) => {
                    let task = player.task.rerouted(route, None);
                    player.task = task;
                    player.work = None;
                    spawn_marker(&mut commands, &assets, marker_position(map, tile), false);
                }
                None => progress.journal = "You can't reach that.".into(),
            }
        }
        Pick::Nothing => {}
    }
}

fn marker_position(map: &ozbound::world::WorldMap, tile: TilePos) -> Vec3 {
    Vec3::from_array(map.tile_center_3d(tile, 0.06))
}

fn spawn_marker(commands: &mut Commands, assets: &ClickAssets, at: Vec3, interact: bool) {
    let material = if interact { assets.interact.clone() } else { assets.walk.clone() };
    commands
        .spawn((
            Transform::from_translation(at),
            Visibility::default(),
            ClickMarker { timer: Timer::from_seconds(MARKER_SECONDS, TimerMode::Once) },
        ))
        .with_children(|marker| {
            for angle in [FRAC_PI_4, -FRAC_PI_4] {
                marker.spawn((
                    Mesh3d(assets.bar.clone()),
                    MeshMaterial3d(material.clone()),
                    Transform::from_rotation(Quat::from_rotation_y(angle)),
                ));
            }
        });
}

/// Shrinks click markers away and removes them when their time is up.
pub fn fade_markers(mut commands: Commands, time: Res<Time>, mut markers: Query<(Entity, &mut ClickMarker, &mut Transform)>) {
    for (entity, mut marker, mut transform) in &mut markers {
        marker.timer.tick(time.delta());
        transform.scale = Vec3::splat(0.4 + 0.6 * (1.0 - marker.timer.fraction()));
        if marker.timer.finished() {
            commands.entity(entity).despawn_recursive();
        }
    }
}
