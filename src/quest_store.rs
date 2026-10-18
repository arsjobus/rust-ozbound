//! Reading and writing the quest log file (`QuestLog::to_text` / `from_text`).
//!
//! Pure Rust (no Bevy). This was the whole save system before M6; since then it only reads the
//! old quest-only save so `save_store::load` can carry it into the full save (`save.rs`).
//!
//! Safety rules: a missing file is a normal first run (`Ok(None)`); an unreadable or invalid
//! file is an error and is never touched, so the player's data can still be recovered; a save
//! writes a temporary file next to the target and then renames it over the target, so a crash
//! mid-write cannot leave a half-written log.

use std::fmt;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};

use crate::quests::{QuestBook, QuestLog};
use crate::textdata::ContentError;

#[derive(Debug)]
pub enum StoreError {
    Read { path: PathBuf, error: std::io::Error },
    Invalid { path: PathBuf, error: ContentError },
    Write { path: PathBuf, error: std::io::Error },
}

impl fmt::Display for StoreError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Read { path, error } => write!(f, "could not read quest save {}: {error}", path.display()),
            Self::Invalid { path, error } => write!(f, "invalid quest save {}: {error}", path.display()),
            Self::Write { path, error } => write!(f, "could not write quest save {}: {error}", path.display()),
        }
    }
}

impl std::error::Error for StoreError {}

/// Loads the saved quest log. `Ok(None)` means there is no save yet.
pub fn load(path: &Path, book: &QuestBook) -> Result<Option<QuestLog>, StoreError> {
    let text = match std::fs::read_to_string(path) {
        Ok(text) => text,
        Err(error) if error.kind() == ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(StoreError::Read { path: path.to_path_buf(), error }),
    };
    QuestLog::from_text(&text, book)
        .map(Some)
        .map_err(|error| StoreError::Invalid { path: path.to_path_buf(), error })
}

fn temp_path(path: &Path) -> PathBuf {
    let mut name = path.file_name().map(|n| n.to_os_string()).unwrap_or_default();
    name.push(".tmp");
    path.with_file_name(name)
}

/// Writes the quest log, creating the folder if needed, via a temporary file and a rename.
pub fn save(path: &Path, log: &QuestLog) -> Result<(), StoreError> {
    let write_error = |error| StoreError::Write { path: path.to_path_buf(), error };
    if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
        std::fs::create_dir_all(parent).map_err(write_error)?;
    }
    let temp = temp_path(path);
    std::fs::write(&temp, log.to_text()).map_err(write_error)?;
    if let Err(error) = std::fs::rename(&temp, path) {
        let _ = std::fs::remove_file(&temp);
        return Err(write_error(error));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::inventory::{Inventory, ItemId};
    use crate::quests::{QuestEvent, QuestStatus, DEFAULT_QUESTS};
    use std::sync::atomic::{AtomicU32, Ordering};

    fn scratch() -> PathBuf {
        static NEXT: AtomicU32 = AtomicU32::new(0);
        let dir = std::env::temp_dir().join(format!(
            "ozbound-store-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn book() -> QuestBook {
        QuestBook::parse(DEFAULT_QUESTS).unwrap()
    }

    fn mid_quest_log(book: &QuestBook) -> QuestLog {
        let mut log = QuestLog::new();
        log.start(book, "moonpetals").unwrap();
        log.record(book, &QuestEvent::Gathered { item: ItemId::Moonpetal, quantity: 2 });
        log
    }

    #[test]
    fn missing_file_is_a_first_run() {
        let dir = scratch();
        assert!(load(&dir.join("quests.txt"), &book()).unwrap().is_none());
    }

    #[test]
    fn save_then_load_round_trips() {
        let dir = scratch();
        let path = dir.join("quests.txt");
        let book = book();
        let log = mid_quest_log(&book);
        save(&path, &log).unwrap();
        let loaded = load(&path, &book).unwrap().unwrap();
        assert_eq!(loaded, log);
        assert_eq!(loaded.current_objective("moonpetals"), Some((0, 2)));
    }

    #[test]
    fn save_creates_missing_folders_and_leaves_no_temp_file() {
        let dir = scratch();
        let path = dir.join("deep").join("er").join("quests.txt");
        save(&path, &QuestLog::new()).unwrap();
        assert!(path.exists());
        assert!(!temp_path(&path).exists());
    }

    #[test]
    fn saving_again_replaces_the_old_file() {
        let dir = scratch();
        let path = dir.join("quests.txt");
        let book = book();
        save(&path, &QuestLog::new()).unwrap();
        let log = mid_quest_log(&book);
        save(&path, &log).unwrap();
        assert_eq!(load(&path, &book).unwrap().unwrap(), log);
    }

    #[test]
    fn invalid_file_is_reported_and_left_untouched() {
        let dir = scratch();
        let path = dir.join("quests.txt");
        let garbage = "ozbound-quests 1\nquest nonexistent active 0 0\n";
        std::fs::write(&path, garbage).unwrap();
        let error = load(&path, &book()).unwrap_err();
        assert!(matches!(error, StoreError::Invalid { .. }));
        let message = error.to_string();
        assert!(message.contains("quests.txt"), "{message}");
        assert_eq!(std::fs::read_to_string(&path).unwrap(), garbage);
    }

    #[test]
    fn unreadable_path_is_a_read_error() {
        let dir = scratch();
        // A directory where the file should be cannot be read as text.
        let error = load(&dir, &book()).unwrap_err();
        assert!(matches!(error, StoreError::Read { .. }), "{error}");
    }

    #[test]
    fn write_failure_is_reported() {
        let dir = scratch();
        let blocker = dir.join("file");
        std::fs::write(&blocker, "x").unwrap();
        // The "folder" is actually a file, so the save cannot be created under it.
        let error = save(&blocker.join("quests.txt"), &QuestLog::new()).unwrap_err();
        assert!(matches!(error, StoreError::Write { .. }), "{error}");
    }

    #[test]
    fn a_completed_quest_survives_a_round_trip() {
        let dir = scratch();
        let path = dir.join("quests.txt");
        let book = book();
        let mut log = mid_quest_log(&book);
        log.record(&book, &QuestEvent::Gathered { item: ItemId::Moonpetal, quantity: 1 });
        let mut bag = Inventory::default();
        bag.add(ItemId::Moonpetal, 3).unwrap();
        log.complete(&book, "moonpetals", &mut bag).unwrap();
        save(&path, &log).unwrap();
        let loaded = load(&path, &book).unwrap().unwrap();
        assert_eq!(loaded.status("moonpetals"), QuestStatus::Completed);
    }
}
