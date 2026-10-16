//! Purely visual animation: limb swings and floating collectables. Nothing here changes
//! game state.

use bevy::prelude::*;

use crate::components::*;

/// Swings legs while walking and arms while chopping or mining.
pub fn animate_limbs(time: Res<Time>, players: Query<&Player>, mut limbs: Query<(&Limb, &mut Transform)>) {
    let t = time.elapsed_secs();
    let blend = 1.0 - (-18.0 * time.delta_secs()).exp();
    for (limb, mut transform) in &mut limbs {
        let (walking, working) = players
            .get(limb.owner)
            .map(|p| (p.task.is_moving(), p.work.is_some()))
            .unwrap_or((false, false));
        let angle = if working && limb.is_arm {
            -1.0 + (t * 8.0).sin() * 0.55
        } else if walking {
            (t * 10.0).sin() * 0.75 * limb.swing
        } else {
            0.0
        };
        transform.rotation = transform.rotation.slerp(Quat::from_rotation_x(angle), blend);
    }
}

/// Bobs and turns collectables so they catch the eye.
pub fn float_props(time: Res<Time>, mut props: Query<(&Floaty, &mut Transform)>) {
    for (floaty, mut transform) in &mut props {
        transform.translation.y = floaty.base_y + (time.elapsed_secs() * 2.0 + floaty.phase).sin() * 0.05;
        transform.rotate_y(time.delta_secs() * 1.2);
    }
}
