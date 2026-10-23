//! World interaction: what happens when the player arrives at a clicked object (talk, pick,
//! read, chop, mine) and how gathered nodes grow back. Game rules come from the tested pure
//! modules; in particular every interaction re-checks `ozbound::interaction::in_reach`, so
//! an object can only be used from the tiles its `Approach` allows.

use bevy::prelude::*;
use ozbound::{
    bestiary::{approach as approach_creature, Encounter},
    gathering::{should_keep_gathering, GatherError, ResourceKind},
    interaction::{in_reach, InteractionKind},
    inventory::{InventoryError, ItemId},
    journal::describe_update,
    quests::{QuestBook, QuestEvent, QuestLog, QuestUpdate},
    respawn::RespawnTimer,
    skills::XpGain,
    talk::Talk,
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

/// Tells the quest log that `quantity` of `item` was gathered and returns what that changed
/// (nothing when no active quest wants the item).
fn record_gathered(book: &QuestBook, log: &mut QuestLog, item: ItemId, quantity: u32) -> Vec<QuestUpdate> {
    log.record(book, &QuestEvent::Gathered { item, quantity })
}

/// `base`, followed by the level-up line when the XP just earned raised a skill level.
fn with_level_up(base: String, gain: Option<XpGain>) -> String {
    match gain.and_then(|g| g.message()) {
        Some(line) => format!("{base} {line}"),
        None => base,
    }
}

/// `base`, followed by one sentence per quest update.
fn with_updates(base: String, book: &QuestBook, updates: &[QuestUpdate]) -> String {
    let mut text = base;
    for update in updates {
        text.push(' ');
        text.push_str(&describe_update(book, update));
    }
    text
}

/// Runs the pending interaction when the player reaches their target, then keeps
/// gathering automatically until the node is spent, the bag is full, or the player moves away.
pub fn interact(
    mut commands: Commands,
    time: Res<Time>,
    meadow: Res<Meadow>,
    content: Res<GameContent>,
    mut progress: ResMut<GameProgress>,
    mut talk: ResMut<TalkState>,
    mut partner: ResMut<TalkPartner>,
    portraits: Res<Portraits>,
    mut portrait: ResMut<TalkPortrait>,
    wildlife: Res<Wildlife>,
    creatures: Query<&Creature>,
    mut players: Query<(&mut Player, &mut Transform)>,
    mut progress_of: Query<ProgressMut, With<Player>>,
    interactables: Query<(&Interactable, Option<&SignText>, Option<&Npc>)>,
    mut nodes: Query<(&mut ResourceNode, &SpentVisual)>,
    mut visibilities: Query<&mut Visibility>,
) {
    let Ok((mut player, mut transform)) = players.get_single_mut() else { return };
    let Ok(mut mine) = progress_of.get_single_mut() else { return };

    if let Some(target) = player.task.take_ready_target() {
        if let Ok((it, sign, npc)) = interactables.get(target) {
            if !in_reach(player.tile, it.tile, it.approach) {
                progress.notice = TOO_FAR.into();
            } else {
                face_tile(&mut transform, &meadow.0, player.tile, it.tile);
                match it.kind {
                    InteractionKind::TalkTo => {
                        let started = match npc {
                            Some(npc) => Talk::start(&content.0, &mut mine.log.0, &mut mine.inventory.0, &mine.skills.0, &npc.0),
                            None => Err("They have nothing to say right now.".to_string()),
                        };
                        match started {
                            Ok((opened, messages)) => {
                                if !messages.is_empty() {
                                    progress.notice = messages.join(" ");
                                }
                                talk.0 = Some(opened);
                                // The NPC stops, faces the player, and waves when this closes (`npc_ui`).
                                partner.0 = npc.map(|_| target);
                                portrait.0 = npc.and_then(|n| portraits.0.get(&n.0).cloned());
                            }
                            Err(message) => progress.notice = message,
                        }
                    }
                    InteractionKind::Read => match sign {
                        Some(text) => {
                            // The sign opens in the dialogue panel; the chat keeps a record.
                            progress.notice = format!("The signpost reads: \"{}\"", text.0);
                            talk.0 = Some(Talk::reading("Signpost", &text.0));
                            partner.0 = None;
                            portrait.0 = None;
                        }
                        None => progress.notice = "The signpost is too weathered to read.".into(),
                    },
                    InteractionKind::Approach => {
                        progress.notice = match creatures.get(target) {
                            Ok(Creature(id)) => match approach_creature(&wildlife.bestiary, &mut mine.notes.0, &mut mine.inventory.0, id) {
                                Ok(Encounter::Befriended { creature, taken, gift, text }) => {
                                    let mut line = format!("{text} (You gave {} {}", taken.quantity, taken.item.label());
                                    if let Some(gift) = gift {
                                        line.push_str(&format!("; you received {} {}", gift.quantity, gift.item.label()));
                                    }
                                    line.push_str(".)");
                                    let updates = mine.log.0.record(&content.0.quests, &QuestEvent::Befriended { creature });
                                    with_updates(line, &content.0.quests, &updates)
                                }
                                Ok(other) => other.text().to_string(),
                                Err(_) => "Nothing happens.".to_string(),
                            },
                            Err(_) => "Nothing happens.".to_string(),
                        };
                    }
                    InteractionKind::Pick => {
                        // Inventory first: a full bag must never silently consume a world resource.
                        if mine.inventory.0.add(ItemId::Moonpetal, 1).is_ok() {
                            commands.entity(target).despawn_recursive();
                            let updates = record_gathered(&content.0.quests, &mut mine.log.0, ItemId::Moonpetal, 1);
                            let xp = mine.skills.0.gathered(ResourceKind::Moonpetal);
                            let message = with_updates("You gathered a moonpetal.".into(), &content.0.quests, &updates);
                            progress.notice = with_level_up(message, xp);
                        } else {
                            progress.notice = "Your inventory is full. Make room before gathering more.".into();
                        }
                    }
                    InteractionKind::ChopDown | InteractionKind::Mine | InteractionKind::Forage => {
                        if let Ok((node, _)) = nodes.get(target) {
                            let label = node.0.kind().label();
                            if node.0.is_depleted() {
                                progress.notice = format!("This {label} has nothing left to give.");
                            } else {
                                let action = match it.kind {
                                    InteractionKind::ChopDown => "chopping",
                                    InteractionKind::Forage => "foraging",
                                    _ => "mining",
                                };
                                progress.notice = format!("You start {action} the {label}...");
                                player.work = Some(Work { target, timer: Timer::from_seconds(GATHER_INTERVAL, TimerMode::Repeating) });
                            }
                        }
                    }
                }
            }
        }
    }

    // Walking away abandons the job and ends any conversation.
    if player.task.is_moving() {
        player.work = None;
        talk.0 = None;
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
        let in_range = interactables.get(target).is_ok_and(|(it, _, _)| in_reach(player.tile, it.tile, it.approach));
        let mut keep_going = false;
        if !in_range {
            progress.notice = TOO_FAR.into();
        } else if let Ok((mut node, spent)) = nodes.get_mut(target) {
            let kind = node.0.kind();
            let outcome = node.0.gather(&mut mine.inventory.0);
            let mut message = gather_message(kind, &outcome, node.0.remaining_actions());
            if let Ok(amount) = outcome {
                let updates = record_gathered(&content.0.quests, &mut mine.log.0, kind.item(), amount);
                message = with_updates(message, &content.0.quests, &updates);
                message = with_level_up(message, mine.skills.0.gathered(kind));
            }
            progress.notice = message;
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
