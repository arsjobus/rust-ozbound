//! NPC behavior in the Bevy layer: connects each villager's `ozbound::npc::Brain` to its body.
//!
//! The brain decides (which tile to step onto, what to face, which emote to play); this module
//! tells the brain what the world looks like, then glides the body, turns it, plays the emote
//! pose, and keeps the click box and the logical tile in step with where the body is.
//!
//! - A villager's `Interactable::tile` is the tile it is gliding onto (the brain claims a tile
//!   the moment it steps), so a click always plans to where the NPC will stand. The pick box
//!   follows the body itself, so clicking what you see works mid-stride.
//! - Conversation state comes from `TalkState` and `TalkPartner`; the player walking toward an
//!   NPC comes from the player's pending interaction target.
//! - The goodbye wave is written to the chat here, since it is what the NPC does, not the player.

use std::collections::HashSet;

use bevy::prelude::*;
use ozbound::{
    chat::Kind,
    emote::{ActiveEmote, Emote},
    npc::{Attention, Senses},
    task::{facing_yaw, step_toward},
    world::TilePos,
};

use crate::components::*;

/// Tiles per second while an NPC strolls (slower than the player's run).
const NPC_SPEED: f32 = 1.7;
/// How quickly an NPC turns toward where it is going or toward the player.
const TURN_RATE: f32 = 9.0;
/// Half the width and the height of an NPC's click box.
const PICK_HALF: f32 = 0.4;
const PICK_HEIGHT: f32 = 1.95;

/// Thinks for every NPC, then moves and poses its body for this frame.
pub fn drive_npcs(
    time: Res<Time>,
    meadow: Res<Meadow>,
    heights: Res<GroundHeights>,
    level: Res<LevelData>,
    talk: Res<TalkState>,
    mut partner: ResMut<TalkPartner>,
    mut chat: ResMut<ChatState>,
    mut keep_out: Local<HashSet<TilePos>>,
    players: Query<&Player>,
    mut npcs: Query<(Entity, &mut NpcBrain, &mut NpcBody, &mut Interactable, &mut Transform), Without<Player>>,
) {
    let Ok(player) = players.get_single() else { return };
    // Waymarks, plants, creatures, doorways, and exits stay clear; rebuilt when chunks stream.
    if level.is_changed() {
        *keep_out = level.0.npc_keep_out().into_iter().collect();
    }
    let dt = time.delta_secs();
    let map = &meadow.0;

    // Nobody may step onto a tile another NPC holds, or onto the player's tile, next step, or goal.
    let mut avoid: HashSet<TilePos> = keep_out.clone();
    avoid.extend(npcs.iter().map(|(_, brain, ..)| brain.0.tile()));
    avoid.insert(player.tile);
    avoid.extend(player.task.next_step());
    avoid.extend(player.task.destination());
    let approached = player.task.pending_target();

    for (entity, mut brain, mut body, mut interactable, mut transform) in &mut npcs {
        let attention = if talk.0.is_some() && partner.0 == Some(entity) {
            Attention::Talking
        } else if approached == Some(entity) {
            Attention::Approached
        } else {
            Attention::Free
        };
        let senses = Senses { map, attention, player_tile: player.tile, striding: body.glide.is_some(), avoid: &avoid };
        let intent = brain.0.tick(dt, &senses);

        if let Some(step) = intent.step {
            interactable.tile = step;
            avoid.insert(step);
            let center = map.tile_center_3d(step, 0.0);
            body.glide = Some(Vec3::new(center[0], transform.translation.y, center[2]));
            body.emote = None;
        }
        if let Some(order) = intent.emote {
            body.emote = Some(ActiveEmote::start(order.emote));
            if order.farewell {
                chat.log.push(Kind::Emote, Emote::farewell_message(interactable.name));
            }
        }
        if let Some(target) = intent.face {
            let center = map.tile_center_3d(target, 0.0);
            body.face_yaw = facing_yaw(center[0] - transform.translation.x, center[2] - transform.translation.z);
        }

        // Glide onto the claimed tile, turning toward where it is going.
        let turn = 1.0 - (-TURN_RATE * dt).exp();
        if let Some(goal) = body.glide {
            let here = (transform.translation.x, transform.translation.z);
            if let Some(yaw) = facing_yaw(goal.x - here.0, goal.z - here.1) {
                transform.rotation = transform.rotation.slerp(Quat::from_rotation_y(yaw), turn);
            }
            let ((x, z), arrived) = step_toward(here, (goal.x, goal.z), NPC_SPEED * dt);
            transform.translation.x = x;
            transform.translation.z = z;
            transform.translation.y = heights.0.height_at_world(x, z);
            if arrived {
                body.glide = None;
                body.face_yaw = None;
            }
        } else if let Some(yaw) = body.face_yaw {
            transform.rotation = transform.rotation.slerp(Quat::from_rotation_y(yaw), turn);
        }

        // Play the emote: hops lift the body, walking off or the end of the emote stops it.
        let rest_height = heights.0.tile_height(interactable.tile);
        if let Some(mut active) = body.emote {
            active.tick(dt);
            if active.is_finished() || body.glide.is_some() {
                body.emote = None;
                transform.translation.y = rest_height;
            } else {
                transform.translation.y = rest_height + active.pose().hop;
                body.emote = Some(active);
            }
        }

        // The click box follows the body, so what you see is what you click.
        let (x, z) = (transform.translation.x, transform.translation.z);
        let base = heights.0.height_at_world(x, z);
        let (min, max) = ([x - PICK_HALF, base, z - PICK_HALF], [x + PICK_HALF, base + PICK_HEIGHT, z + PICK_HALF]);
        if interactable.pick_min != min || interactable.pick_max != max {
            interactable.pick_min = min;
            interactable.pick_max = max;
        }
    }

    // Once the conversation is closed there is no partner (the brain has already seen the end).
    if talk.0.is_none() {
        partner.0 = None;
    }
}
