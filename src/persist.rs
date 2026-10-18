//! Saves the game while it is played. What is saved, the file format, and the safe write
//! (temporary file + rename) are the tested `ozbound::save` and `ozbound::save_store`; this
//! module only gathers the current state from the world and decides when to write.
//!
//! The game is written at most once a second, and only when something changed, plus once more
//! when the game exits. A write that fails turns saving off for the session instead of retrying
//! every second, and says so in the journal line and on the console.

use bevy::prelude::*;
use ozbound::{
    level::Level,
    save::{SaveGame, WorldChanges},
    save_store,
    world::TilePos,
};

use crate::components::*;

/// Seconds between checks for changes.
const SAVE_INTERVAL: f32 = 1.0;

/// The game as it is right now: the player, bag, gear, skills, quests, creatures met, and what
/// has been picked or used up in the world.
fn current_game(
    level: &Level,
    progress: &GameProgress,
    wildlife: &Wildlife,
    players: &Query<&Player>,
    petals: &Query<&Interactable, With<Moonpetal>>,
    nodes: &Query<(&ResourceNode, &Interactable)>,
) -> Option<SaveGame> {
    let player = players.get_single().ok()?;
    let mut game = SaveGame::new_game(level);
    game.player = player.tile;
    game.bag = progress.inventory.clone();
    game.equipment = progress.equipment.clone();
    game.skills = progress.skills.clone();
    game.quests = progress.log.clone();
    game.notes = wildlife.notes.clone();
    let petals_left: Vec<TilePos> = petals.iter().map(|petal| petal.tile).collect();
    let node_state: Vec<(TilePos, u32)> = nodes.iter().map(|(node, at)| (at.tile, node.0.remaining_actions())).collect();
    game.changes = WorldChanges::capture(level, &petals_left, &node_state);
    Some(game)
}

/// Writes `game` if it differs from what is on disk.
fn write(save: &mut SaveState, progress: &mut GameProgress, game: SaveGame) {
    if game == save.saved {
        return;
    }
    match save_store::save(&save.path, &game) {
        Ok(()) => save.saved = game,
        Err(error) => {
            eprintln!("warning: {error}; progress will not be saved this session");
            save.enabled = false;
            progress.journal = "Your progress could not be saved. See the console for details.".into();
        }
    }
}

pub fn persist_game(
    time: Res<Time>,
    level: Res<LevelData>,
    mut progress: ResMut<GameProgress>,
    wildlife: Res<Wildlife>,
    mut save: ResMut<SaveState>,
    players: Query<&Player>,
    petals: Query<&Interactable, With<Moonpetal>>,
    nodes: Query<(&ResourceNode, &Interactable)>,
) {
    if !save.enabled {
        return;
    }
    save.idle += time.delta_secs();
    if save.idle < SAVE_INTERVAL {
        return;
    }
    save.idle = 0.0;
    if let Some(game) = current_game(&level.0, &progress, &wildlife, &players, &petals, &nodes) {
        write(&mut save, &mut progress, game);
    }
}

/// Saves once more as the game closes, so the last second of play is not lost. Runs in `Last`,
/// after every `Update` system, so it also sees an exit requested by closing the window.
pub fn save_on_exit(
    mut exits: EventReader<AppExit>,
    level: Res<LevelData>,
    mut progress: ResMut<GameProgress>,
    wildlife: Res<Wildlife>,
    mut save: ResMut<SaveState>,
    players: Query<&Player>,
    petals: Query<&Interactable, With<Moonpetal>>,
    nodes: Query<(&ResourceNode, &Interactable)>,
) {
    if exits.read().next().is_none() || !save.enabled {
        return;
    }
    if let Some(game) = current_game(&level.0, &progress, &wildlife, &players, &petals, &nodes) {
        write(&mut save, &mut progress, game);
    }
}
