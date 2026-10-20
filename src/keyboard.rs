//! Keyboard-only play: WASD steps, Tab to choose something to visit, Enter or Space to go and
//! use it, and X to examine it. Which targets exist and in what order, which way each key steps,
//! and whether a step is allowed all come from the tested `ozbound::targeting`, which uses the
//! same route planner and walkability rule as the mouse. Nothing here can reach anything a
//! click could not.
//!
//! These systems stand aside while a conversation, the crafting panel, or the right-click menu
//! is open (they own the keyboard and mouse) and while the game is paused.

use bevy::prelude::*;
use ozbound::chat::Kind;
use ozbound::{
    interaction::{examine_text, hover_text},
    pathfinding::plan_route,
    targeting::{self, Candidate, Dir, Ranked},
    task::Task,
    world::{TilePos, WorldMap},
};

use crate::camera_rig::Rig;
use crate::components::*;
use crate::pointer::{marker_position, order_interact, spawn_marker};

/// Everything that can be selected from where the player will be standing, nearest first. Spent
/// trees and veins are left out (there is nothing to do there until they grow back).
fn ranked_targets(map: &WorldMap, origin: TilePos, targets: &Query<(Entity, &Interactable)>, nodes: &Query<&ResourceNode>) -> Vec<Ranked<Entity>> {
    let list: Vec<Candidate<Entity>> = targets
        .iter()
        .filter(|(entity, _)| !nodes.get(*entity).is_ok_and(|node| node.0.is_depleted()))
        .map(|(entity, it)| Candidate { id: entity, tile: it.tile, approach: it.approach, label: hover_text(it.kind, it.name) })
        .collect();
    targeting::rank(map, origin, &list)
}

/// W, A, S, D take one step relative to the camera; holding a key keeps walking tile by tile.
pub fn keyboard_walk(
    keys: Res<ButtonInput<KeyCode>>,
    rig: Res<Rig>,
    meadow: Res<Meadow>,
    talk: Res<TalkState>,
    menu: Res<ContextMenuState>,
    pack: Res<PackState>,
    chat: Res<ChatState>,
    mut selected: ResMut<KeyTarget>,
    mut progress: ResMut<GameProgress>,
    mut players: Query<&mut Player>,
) {
    if talk.0.is_some() || menu.0.is_some() || pack.0 || chat.keys_captured {
        return;
    }
    let held = [(KeyCode::KeyW, Dir::Forward), (KeyCode::KeyS, Dir::Back), (KeyCode::KeyA, Dir::Left), (KeyCode::KeyD, Dir::Right)]
        .into_iter()
        .find(|(key, _)| keys.pressed(*key));
    let Some((key, dir)) = held else { return };
    let Ok(mut player) = players.get_single_mut() else { return };
    // Finish the stride in progress first, so holding a key walks smoothly.
    if player.task.is_moving() {
        return;
    }
    let delta = targeting::step_direction(rig.orbit.yaw(), dir);
    match targeting::step_to(&meadow.0, player.tile, delta) {
        Ok(to) => {
            player.task = Task::walk(vec![to]);
            player.work = None;
            if selected.0.is_some() {
                selected.0 = None;
            }
        }
        Err(_) => {
            if keys.just_pressed(key) {
                progress.notice = "Something is in the way.".into();
            }
        }
    }
}

/// Tab and Shift+Tab choose the next or previous thing to visit; Enter or Space goes to the chosen
/// thing and uses it (with nothing chosen, to the nearest); X examines the chosen thing.
pub fn keyboard_target(
    mut commands: Commands,
    keys: Res<ButtonInput<KeyCode>>,
    meadow: Res<Meadow>,
    assets: Res<ClickAssets>,
    talk: Res<TalkState>,
    menu: Res<ContextMenuState>,
    pack: Res<PackState>,
    mut chat: ResMut<ChatState>,
    mut selected: ResMut<KeyTarget>,
    mut progress: ResMut<GameProgress>,
    targets: Query<(Entity, &Interactable)>,
    nodes: Query<&ResourceNode>,
    mut players: Query<&mut Player>,
) {
    if talk.0.is_some() || menu.0.is_some() || pack.0 || chat.keys_captured {
        return;
    }
    let tab = keys.just_pressed(KeyCode::Tab);
    let go = keys.any_just_pressed([KeyCode::Enter, KeyCode::NumpadEnter, KeyCode::Space]);
    let examine = keys.just_pressed(KeyCode::KeyX);
    if !(tab || go || examine) {
        return;
    }
    let Ok(mut player) = players.get_single_mut() else { return };
    let map = &meadow.0;
    let origin = player.task.planning_origin(player.tile);

    if tab {
        let backwards = keys.any_pressed([KeyCode::ShiftLeft, KeyCode::ShiftRight]);
        let list = ranked_targets(map, origin, &targets, &nodes);
        match targeting::cycle(&list, selected.0, !backwards) {
            Some(entity) => {
                selected.0 = Some(entity);
                if let Ok((_, it)) = targets.get(entity) {
                    spawn_marker(&mut commands, &assets, marker_position(map, it.tile), true);
                }
            }
            None => progress.notice = "There is nothing to visit right now.".into(),
        }
        return;
    }

    if go {
        let chosen = selected.0.filter(|entity| targets.get(*entity).is_ok()).or_else(|| ranked_targets(map, origin, &targets, &nodes).first().map(|r| r.id));
        match chosen.and_then(|entity| targets.get(entity).ok()) {
            Some((entity, it)) => {
                order_interact(&mut commands, map, &assets, &mut player, &mut progress, entity, it);
                selected.0 = None;
            }
            None => progress.notice = "There is nothing to visit right now.".into(),
        }
        return;
    }

    // Examine.
    match selected.0.and_then(|entity| targets.get(entity).ok()) {
        Some((entity, it)) => {
            let spent = nodes.get(entity).is_ok_and(|node| node.0.is_depleted());
            chat.log.push(Kind::Examine, examine_text(it.kind, spent));
        }
        None => progress.notice = "Press Tab to choose something, then X to examine it.".into(),
    }
}

/// Shows what Tab chose, and how far away it is, in the hover label (unless the mouse is over
/// something, which wins).
pub fn keyboard_hint(
    selected: Res<KeyTarget>,
    meadow: Res<Meadow>,
    targets: Query<&Interactable>,
    players: Query<&Player>,
    mut hover: ResMut<HoverText>,
) {
    let Some(entity) = selected.0 else { return };
    let Ok(it) = targets.get(entity) else { return };
    let Ok(player) = players.get_single() else { return };
    if !hover.0.is_empty() {
        return;
    }
    let label = hover_text(it.kind, it.name);
    hover.0 = match plan_route(&meadow.0, player.task.planning_origin(player.tile), it.tile, it.approach) {
        Some(route) => format!("> {}", targeting::describe(&Ranked { id: entity, steps: route.len(), label })),
        None => format!("> {label}"),
    };
}
