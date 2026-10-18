//! Movement: glides the player along the planned route, one tile at a time. Which tiles are
//! legal was decided by `ozbound::pathfinding` against the tile map before the route existed,
//! so nothing here needs to test for collision; this module only animates the walk.

use bevy::prelude::*;
use ozbound::{
    task::{facing_yaw, step_toward},
    world::{TilePos, WorldMap},
};

use crate::components::*;

/// Tiles per second while following a route (a brisk run).
const WALK_SPEED: f32 = 3.4;
/// How quickly characters turn toward where they are going.
const TURN_RATE: f32 = 14.0;

/// Glides the player toward the next tile on their route and turns to face it.
pub fn follow_route(
    time: Res<Time>,
    meadow: Res<Meadow>,
    mut progress: ResMut<GameProgress>,
    mut players: Query<(&mut Player, &mut Transform)>,
) {
    let Ok((mut player, mut transform)) = players.get_single_mut() else { return };
    let Some(step) = player.task.next_step() else { return };
    let dest = meadow.0.tile_center_3d(step, 0.0);
    let here = (transform.translation.x, transform.translation.z);
    let dt = time.delta_secs();

    if let Some(yaw) = facing_yaw(dest[0] - here.0, dest[2] - here.1) {
        let turn = 1.0 - (-TURN_RATE * dt).exp();
        transform.rotation = transform.rotation.slerp(Quat::from_rotation_y(yaw), turn);
    }
    let ((x, z), arrived) = step_toward(here, (dest[0], dest[2]), WALK_SPEED * dt);
    transform.translation.x = x;
    transform.translation.z = z;
    if arrived {
        player.task.complete_step();
        player.tile = step;
        // Every tile walked counts toward Wayfinding (it stops teaching at level 3).
        if let Some(line) = progress.skills.walked(1).and_then(|gain| gain.message()) {
            progress.journal = line;
        }
    }
}

/// Turns a character on `from` to look at the tile `to` (used when starting an interaction).
pub fn face_tile(transform: &mut Transform, map: &WorldMap, from: TilePos, to: TilePos) {
    let a = map.tile_center_3d(from, 0.0);
    let b = map.tile_center_3d(to, 0.0);
    if let Some(yaw) = facing_yaw(b[0] - a[0], b[2] - a[2]) {
        transform.rotation = Quat::from_rotation_y(yaw);
    }
}
