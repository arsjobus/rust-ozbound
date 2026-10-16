//! What happens after a click: gliding along the route, then talking, picking up, or
//! gathering when the player arrives. Rules come from the tested pure modules.

use bevy::prelude::*;
use ozbound::{
    gathering::{should_keep_gathering, GatherError, ResourceKind},
    interaction::InteractionKind,
    inventory::{InventoryError, ItemId},
    task::{facing_yaw, step_toward},
    world::{TilePos, WorldMap},
};

use crate::components::*;

/// Tiles per second while following a route (a brisk run).
const WALK_SPEED: f32 = 3.4;
/// Seconds between gather attempts while chopping or mining.
const GATHER_INTERVAL: f32 = 1.4;
/// How quickly characters turn toward where they are going.
const TURN_RATE: f32 = 14.0;

/// Glides the player toward the next tile on their route and turns to face it.
pub fn follow_route(time: Res<Time>, meadow: Res<Meadow>, mut players: Query<(&mut Player, &mut Transform)>) {
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
    }
}

fn face_tile(transform: &mut Transform, map: &WorldMap, from: TilePos, to: TilePos) {
    let a = map.tile_center_3d(from, 0.0);
    let b = map.tile_center_3d(to, 0.0);
    if let Some(yaw) = facing_yaw(b[0] - a[0], b[2] - a[2]) {
        transform.rotation = Quat::from_rotation_y(yaw);
    }
}

fn gather_message(kind: ResourceKind, outcome: &Result<u32, GatherError>, remaining: u32) -> String {
    let label = kind.label();
    match outcome {
        Ok(amount) if remaining == 0 => format!("You gathered {amount} from the {label}. It is spent for now."),
        Ok(amount) => format!("You gathered {amount} from the {label} ({remaining} left)."),
        Err(GatherError::Depleted) => format!("This {label} has nothing left to give."),
        Err(GatherError::Inventory(InventoryError::Full)) => "Your inventory is full. Make room before gathering more.".into(),
        Err(GatherError::Inventory(_)) => "You cannot carry that right now.".into(),
    }
}

/// Runs the pending interaction when the player reaches their target, then keeps
/// gathering automatically until the node is spent, the bag is full, or the player moves away.
pub fn interact(
    mut commands: Commands,
    time: Res<Time>,
    meadow: Res<Meadow>,
    mut progress: ResMut<GameProgress>,
    mut players: Query<(&mut Player, &mut Transform)>,
    interactables: Query<&Interactable>,
    mut nodes: Query<(&mut ResourceNode, &SpentVisual)>,
    mut visibilities: Query<&mut Visibility>,
) {
    let Ok((mut player, mut transform)) = players.get_single_mut() else { return };

    if let Some(target) = player.task.take_ready_target() {
        if let Ok(it) = interactables.get(target) {
            face_tile(&mut transform, &meadow.0, player.tile, it.tile);
            match it.kind {
                InteractionKind::TalkTo => {
                    progress.spoke_to_guide = true;
                    progress.journal = "Guide: The moonpetals glow after dusk. Please gather three for the village apothecary.".into();
                }
                InteractionKind::Pick => {
                    // Inventory first: a full bag must never silently consume a world resource.
                    if progress.inventory.add(ItemId::Moonpetal, 1).is_ok() {
                        commands.entity(target).despawn_recursive();
                        progress.quest.collect_one();
                        progress.journal = if progress.quest.is_complete() {
                            "Quest objective complete! Return to the guide with the moonpetals.".into()
                        } else {
                            format!("You gathered a moonpetal. {}", progress.quest.progress_text())
                        };
                    } else {
                        progress.journal = "Your inventory is full. Make room before gathering more.".into();
                    }
                }
                InteractionKind::ChopDown | InteractionKind::Mine => {
                    if let Ok((node, _)) = nodes.get(target) {
                        let label = node.0.kind().label();
                        if node.0.is_depleted() {
                            progress.journal = format!("This {label} has nothing left to give.");
                        } else {
                            let action = if it.kind == InteractionKind::ChopDown { "chopping" } else { "mining" };
                            progress.journal = format!("You start {action} the {label}...");
                            player.work = Some(Work { target, timer: Timer::from_seconds(GATHER_INTERVAL, TimerMode::Repeating) });
                        }
                    }
                }
            }
        }
    }

    // Walking away abandons the job.
    if player.task.is_moving() {
        player.work = None;
    }

    let mut attempt = None;
    if let Some(work) = player.work.as_mut() {
        work.timer.tick(time.delta());
        if work.timer.just_finished() {
            attempt = Some(work.target);
        }
    }
    if let Some(target) = attempt {
        let mut keep_going = false;
        if let Ok((mut node, spent)) = nodes.get_mut(target) {
            let kind = node.0.kind();
            let outcome = node.0.gather(&mut progress.inventory);
            progress.journal = gather_message(kind, &outcome, node.0.remaining_actions());
            if node.0.is_depleted() {
                if let Ok(mut visibility) = visibilities.get_mut(spent.0) {
                    *visibility = Visibility::Hidden;
                }
            }
            keep_going = should_keep_gathering(&outcome, &node.0);
        }
        if !keep_going {
            player.work = None;
        }
    }
}

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
