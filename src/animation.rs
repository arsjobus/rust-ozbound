//! Purely visual animation: limb swings, emotes, and floating collectables. Nothing here
//! changes game state.

use bevy::prelude::*;

use crate::components::*;
use ozbound::emote::Pose;

/// Swings legs while walking, arms while chopping or mining, and holds the pose of any emote
/// being played.
pub fn animate_limbs(time: Res<Time>, players: Query<&Player>, mut limbs: Query<(&Limb, &mut Transform)>) {
    let t = time.elapsed_secs();
    let blend = 1.0 - (-18.0 * time.delta_secs()).exp();
    for (limb, mut transform) in &mut limbs {
        let player = players.get(limb.owner).ok();
        let (walking, working) = player.map_or((false, false), |p| (p.task.is_moving(), p.work.is_some()));
        let emote_pose = player.and_then(|p| p.emote).map(|active| active.pose());
        let target = if let Some(pose) = emote_pose {
            limb_rotation(&pose, limb)
        } else {
            let angle = if working && limb.is_arm {
                -1.0 + (t * 8.0).sin() * 0.55
            } else if walking {
                (t * 10.0).sin() * 0.75 * limb.swing
            } else {
                0.0
            };
            Quat::from_rotation_x(angle)
        };
        transform.rotation = transform.rotation.slerp(target, blend);
    }
}

/// The rotation an emote pose asks of one limb: sideways raise (roll) applied after the
/// forward/back swing (pitch). Positive roll lifts a limb away from the body.
fn limb_rotation(pose: &Pose, limb: &Limb) -> Quat {
    let index = usize::from(limb.side > 0.0);
    let held = if limb.is_arm { pose.arms[index] } else { pose.legs[index] };
    Quat::from_rotation_z(held.roll * limb.side) * Quat::from_rotation_x(held.pitch)
}

/// Plays the player's emote: lifts the body for hops, ends it when time is up, and ends it
/// at once if the player starts walking or working.
pub fn tick_emotes(time: Res<Time>, meadow: Res<Meadow>, mut players: Query<(&mut Player, &mut Transform)>) {
    for (mut player, mut transform) in &mut players {
        let Some(mut active) = player.emote else { continue };
        let rest_height = meadow.0.tile_center_3d(player.tile, 0.0)[1];
        active.tick(time.delta_secs());
        if player.task.is_moving() || player.work.is_some() || active.is_finished() {
            player.emote = None;
            transform.translation.y = rest_height;
        } else {
            transform.translation.y = rest_height + active.pose().hop;
            player.emote = Some(active);
        }
    }
}

/// Bobs and turns collectables so they catch the eye.
pub fn float_props(time: Res<Time>, mut props: Query<(&Floaty, &mut Transform)>) {
    for (floaty, mut transform) in &mut props {
        transform.translation.y = floaty.base_y + (time.elapsed_secs() * 2.0 + floaty.phase).sin() * 0.05;
        transform.rotate_y(time.delta_secs() * 1.2);
    }
}
