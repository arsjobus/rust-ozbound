//! Saves quest progress whenever it changes, so it survives a restart. The file format and the
//! safe write (temporary file + rename) are the tested `ozbound::quest_store`; the full save
//! system (position, bag, skills, world changes) is M6.

use bevy::prelude::*;
use ozbound::quest_store;

use crate::components::*;

pub fn persist_quests(mut progress: ResMut<GameProgress>, mut save: ResMut<QuestSave>) {
    // Reading through `ResMut` does not count as a change, so this costs one comparison a frame.
    if !save.enabled || progress.log == save.saved {
        return;
    }
    match quest_store::save(&save.path, &progress.log) {
        Ok(()) => save.saved = progress.log.clone(),
        Err(error) => {
            eprintln!("warning: {error}; quest progress will not be saved this session");
            save.enabled = false;
            progress.journal = "Your quest progress could not be saved. See the console for details.".into();
        }
    }
}
