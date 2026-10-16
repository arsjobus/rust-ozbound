//! World interaction: what happens when the player arrives at a clicked object (talk, pick,
//! read, chop, mine) and how gathered nodes grow back. Game rules come from the tested pure
//! modules; in particular every interaction re-checks `ozbound::interaction::in_reach`, so
//! an object can only be used from the tiles its `Approach` allows.

use bevy::prelude::*;
use ozbound::{
    gathering::{should_keep_gathering, GatherError, ResourceKind},
    interaction::{in_reach, InteractionKind},
    inventory::{InventoryError, ItemId},
    respawn::RespawnTimer,
};

use crate::components::*;
use crate::movement::face_tile;

/// Seconds between gather attempts while chopping or mining.
const GATHER_INTERVAL: f32 = 1.4;
/// Shown when an interaction is attempted from outside its range.
const TOO_FAR: &str = "You are too far away to do that.";

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
    interactables: Query<(&Interactable, Option<&SignText>)>,
    mut nodes: Query<(&mut ResourceNode, &SpentVisual)>,
    mut visibilities: Query<&mut Visibility>,
) {
    let Ok((mut player, mut transform)) = players.get_single_mut() else { return };

    if let Some(target) = player.task.take_ready_target() {
        if let Ok((it, sign)) = interactables.get(target) {
            if !in_reach(player.tile, it.tile, it.approach) {
                progress.journal = TOO_FAR.into();
            } else {
                face_tile(&mut transform, &meadow.0, player.tile, it.tile);
                match it.kind {
                    InteractionKind::TalkTo => {
                        progress.spoke_to_guide = true;
                        progress.journal = "Guide: The moonpetals glow after dusk. Please gather three for the village apothecary.".into();
                    }
                    InteractionKind::Read => {
                        progress.journal = match sign {
                            Some(text) => format!("The signpost reads: \"{}\"", text.0),
                            None => "The signpost is too weathered to read.".into(),
                        };
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
        // Each attempt needs the player still in reach, not just at the moment of arrival.
        let in_range = interactables.get(target).is_ok_and(|(it, _)| in_reach(player.tile, it.tile, it.approach));
        let mut keep_going = false;
        if !in_range {
            progress.journal = TOO_FAR.into();
        } else if let Ok((mut node, spent)) = nodes.get_mut(target) {
            let kind = node.0.kind();
            let outcome = node.0.gather(&mut progress.inventory);
            progress.journal = gather_message(kind, &outcome, node.0.remaining_actions());
            if outcome.is_ok() && node.0.is_depleted() {
                if let Ok(mut visibility) = visibilities.get_mut(spent.0) {
                    *visibility = Visibility::Hidden;
                }
                // Trees and ore grow back after a while; start the clock the moment they run dry.
                if let Some(seconds) = kind.respawn_seconds() {
                    commands.entity(target).insert(Respawning(RespawnTimer::new(seconds)));
                }
            }
            keep_going = should_keep_gathering(&outcome, &node.0);
        }
        if !keep_going {
            player.work = None;
        }
    }
}

/// Counts down spent trees and veins, then restocks them and brings their visuals back.
pub fn respawn_nodes(
    mut commands: Commands,
    time: Res<Time>,
    mut nodes: Query<(Entity, &mut ResourceNode, &SpentVisual, &mut Respawning)>,
    mut visibilities: Query<&mut Visibility>,
) {
    for (entity, mut node, spent, mut respawn) in &mut nodes {
        if respawn.0.tick(time.delta_secs()) {
            node.0.restock();
            if let Ok(mut visibility) = visibilities.get_mut(spent.0) {
                *visibility = Visibility::Inherited;
            }
            commands.entity(entity).remove::<Respawning>();
        }
    }
}
