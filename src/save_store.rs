//! Reading and writing the save file (`SaveGame::to_text` / `from_text`).
//!
//! Pure Rust (no Bevy). This is the file layer of the M6 save system; the format and every
//! validation rule live in `save.rs`.
//!
//! Safety rules:
//! - A missing file is a normal first run, not an error.
//! - An unreadable or invalid file is an error and is **never touched**, so the player's data can
//!   still be recovered by hand. The error names the file and line and says what to do next.
//! - A save writes a temporary file next to the target, flushes it to disk, and then renames it
//!   over the target, so a crash mid-write leaves the previous save whole. A stale temporary file
//!   from an earlier crash is ignored on load and replaced by the next save.
//! - After a successful load the game keeps a copy of that file as `<name>.bak` (`back_up`), so
//!   the last save that was known to load survives even if a later session misbehaves.
//! - Saves from before M6 kept only quest progress (`saves/quests.txt`). When there is no full
//!   save yet, that file is read once and becomes the quest log of a new game; it is left in place.

use std::fmt;
use std::io::{ErrorKind, Write};
use std::path::{Path, PathBuf};

use crate::quest_store;
use crate::save::{SaveContext, SaveGame};
use crate::textdata::ContentError;

#[derive(Debug)]
pub enum StoreError {
    Read { path: PathBuf, error: std::io::Error },
    /// The file loaded but is not a save this game can use. `backup` is set when a copy from
    /// an earlier successful start exists.
    Invalid { path: PathBuf, error: ContentError, backup: Option<PathBuf> },
    Write { path: PathBuf, error: std::io::Error },
    /// The old quest-only save could not be used.
    LegacyQuests(quest_store::StoreError),
}

impl fmt::Display for StoreError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Read { path, error } => write!(f, "could not read save {}: {error}", path.display()),
            Self::Invalid { path, error, backup } => {
                write!(f, "invalid save {}: {error}. Your progress is still in that file: fix the line above, or move the file aside to start a new game", path.display())?;
                if let Some(backup) = backup {
                    write!(f, ". A copy from the last successful start is at {}", backup.display())?;
                }
                Ok(())
            }
            Self::Write { path, error } => write!(f, "could not write save {}: {error}", path.display()),
            Self::LegacyQuests(error) => write!(f, "{error}. Fix or move that file to start a new game"),
        }
    }
}

impl StoreError {
    /// A save that names a region this build does not have (a newer game wrote it, or a custom
    /// map changed). Reported like any invalid save: the file is left alone.
    pub fn unknown_region(path: &Path, region: &str) -> Self {
        let error = ContentError::at(0, format!("the save is for region `{region}`, which is not a playable region of this game"));
        let backup = Some(backup_path(path)).filter(|b| b.is_file());
        Self::Invalid { path: path.to_path_buf(), error, backup }
    }
}

impl std::error::Error for StoreError {}

/// Where a loaded game came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Start {
    /// There was no save: a new game.
    NewGame,
    /// The save file was read.
    Loaded,
    /// There was no full save, so the old quest-only save became the quest log of a new game.
    MigratedQuests,
}

#[derive(Debug, Clone)]
pub struct Loaded {
    pub game: SaveGame,
    pub start: Start,
}

/// The copy kept after a successful load: `game.txt` becomes `game.txt.bak`.
pub fn backup_path(path: &Path) -> PathBuf {
    sibling(path, ".bak")
}

fn temp_path(path: &Path) -> PathBuf {
    sibling(path, ".tmp")
}

fn sibling(path: &Path, suffix: &str) -> PathBuf {
    let mut name = path.file_name().map(|n| n.to_os_string()).unwrap_or_default();
    name.push(suffix);
    path.with_file_name(name)
}

/// Loads the save at `path`. With no file there, `legacy_quests` (the pre-M6 quest save) is tried
/// next, and with neither a new game is returned. A file that exists but cannot be used is an
/// error and is left exactly as it was.
pub fn load(path: &Path, legacy_quests: Option<&Path>, ctx: &SaveContext) -> Result<Loaded, StoreError> {
    let text = match std::fs::read_to_string(path) {
        Ok(text) => text,
        Err(error) if error.kind() == ErrorKind::NotFound => return load_without_save(legacy_quests, ctx),
        Err(error) => return Err(StoreError::Read { path: path.to_path_buf(), error }),
    };
    match SaveGame::from_text(&text, ctx) {
        Ok(game) => Ok(Loaded { game, start: Start::Loaded }),
        Err(error) => {
            let backup = Some(backup_path(path)).filter(|b| b.is_file());
            Err(StoreError::Invalid { path: path.to_path_buf(), error, backup })
        }
    }
}

fn load_without_save(legacy_quests: Option<&Path>, ctx: &SaveContext) -> Result<Loaded, StoreError> {
    let mut game = SaveGame::new_game(ctx.level);
    let Some(legacy) = legacy_quests else {
        return Ok(Loaded { game, start: Start::NewGame });
    };
    match quest_store::load(legacy, ctx.quests).map_err(StoreError::LegacyQuests)? {
        Some(log) => {
            game.quests = log;
            Ok(Loaded { game, start: Start::MigratedQuests })
        }
        None => Ok(Loaded { game, start: Start::NewGame }),
    }
}

/// Writes the save, creating the folder if needed, via a temporary file and a rename.
pub fn save(path: &Path, game: &SaveGame) -> Result<(), StoreError> {
    let write_error = |error| StoreError::Write { path: path.to_path_buf(), error };
    if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
        std::fs::create_dir_all(parent).map_err(write_error)?;
    }
    let temp = temp_path(path);
    write_flushed(&temp, game.to_text().as_bytes()).map_err(|error| {
        let _ = std::fs::remove_file(&temp);
        write_error(error)
    })?;
    if let Err(error) = std::fs::rename(&temp, path) {
        let _ = std::fs::remove_file(&temp);
        return Err(write_error(error));
    }
    Ok(())
}

fn write_flushed(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    let mut file = std::fs::File::create(path)?;
    file.write_all(bytes)?;
    file.sync_all()
}

/// Copies the save at `path` to its backup (`<name>.bak`), through a temporary file so the
/// backup is never half-written. Call it after a successful load. A missing save is not an
/// error (there is nothing to keep).
pub fn back_up(path: &Path) -> std::io::Result<()> {
    let bytes = match std::fs::read(path) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(error),
    };
    let backup = backup_path(path);
    let temp = temp_path(&backup);
    write_flushed(&temp, &bytes)?;
    std::fs::rename(&temp, &backup).inspect_err(|_| {
        let _ = std::fs::remove_file(&temp);
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::equipment::{EquipSlot, Equipment};
    use crate::inventory::{Inventory, ItemId};
    use crate::quests::{QuestEvent, QuestLog};
    use crate::village::{builtin, World};
    use std::sync::atomic::{AtomicU32, Ordering};

    fn scratch() -> PathBuf {
        static NEXT: AtomicU32 = AtomicU32::new(0);
        let dir = std::env::temp_dir().join(format!("ozbound-save-{}-{}", std::process::id(), NEXT.fetch_add(1, Ordering::Relaxed)));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn world() -> World {
        builtin().expect("the shipped world loads")
    }

    fn ctx(world: &World) -> SaveContext<'_> {
        SaveContext { level: &world.level, quests: &world.content.quests, bestiary: &world.bestiary }
    }

    fn played(world: &World) -> SaveGame {
        let mut game = SaveGame::new_game(&world.level);
        game.bag = Inventory::from_stacks(20, [(ItemId::Timber, 3), (ItemId::WalkingStaff, 1)]).unwrap();
        game.equipment = Equipment::from_items([(EquipSlot::Charm, ItemId::CopperCharm)]).unwrap();
        game.quests.start(&world.content.quests, "moonpetals").unwrap();
        game.quests.record(&world.content.quests, &QuestEvent::Gathered { item: ItemId::Moonpetal, quantity: 2 });
        game
    }

    #[test]
    fn an_unknown_region_is_reported_as_an_invalid_save_naming_it() {
        let error = StoreError::unknown_region(Path::new("/nonexistent/game.txt"), "NOWHERE");
        let text = error.to_string();
        assert!(text.contains("invalid save") && text.contains("`NOWHERE`") && text.contains("still in that file"), "{text}");
    }

    #[test]
    fn missing_file_is_a_new_game() {
        let (dir, world) = (scratch(), world());
        let loaded = load(&dir.join("game.txt"), None, &ctx(&world)).unwrap();
        assert_eq!(loaded.start, Start::NewGame);
        assert_eq!(loaded.game, SaveGame::new_game(&world.level));
    }

    #[test]
    fn save_then_load_round_trips() {
        let (dir, world) = (scratch(), world());
        let path = dir.join("game.txt");
        let game = played(&world);
        save(&path, &game).unwrap();
        let loaded = load(&path, None, &ctx(&world)).unwrap();
        assert_eq!(loaded.start, Start::Loaded);
        assert_eq!(loaded.game, game);
    }

    #[test]
    fn save_creates_missing_folders_and_leaves_no_temp_file() {
        let (dir, world) = (scratch(), world());
        let path = dir.join("deep").join("er").join("game.txt");
        save(&path, &played(&world)).unwrap();
        assert!(path.exists());
        assert!(!temp_path(&path).exists());
    }

    #[test]
    fn saving_again_replaces_the_old_file() {
        let (dir, world) = (scratch(), world());
        let path = dir.join("game.txt");
        save(&path, &SaveGame::new_game(&world.level)).unwrap();
        let game = played(&world);
        save(&path, &game).unwrap();
        assert_eq!(load(&path, None, &ctx(&world)).unwrap().game, game);
    }

    #[test]
    fn a_stale_temp_file_from_a_crash_is_ignored_and_replaced() {
        let (dir, world) = (scratch(), world());
        let path = dir.join("game.txt");
        let game = played(&world);
        save(&path, &game).unwrap();
        std::fs::write(temp_path(&path), "ozbound-save 1\nhalf a wri").unwrap();
        assert_eq!(load(&path, None, &ctx(&world)).unwrap().game, game, "the temp file is never read");
        let newer = SaveGame::new_game(&world.level);
        save(&path, &newer).unwrap();
        assert_eq!(load(&path, None, &ctx(&world)).unwrap().game, newer);
        assert!(!temp_path(&path).exists());
    }

    #[test]
    fn an_invalid_file_is_reported_with_line_and_recovery_advice_and_left_untouched() {
        let (dir, world) = (scratch(), world());
        let path = dir.join("game.txt");
        let good = played(&world).to_text();
        let garbage = good.replace("bag timber 3", "bag rubies 3");
        std::fs::write(&path, &garbage).unwrap();
        let error = load(&path, None, &ctx(&world)).unwrap_err();
        assert!(matches!(error, StoreError::Invalid { backup: None, .. }), "{error}");
        let message = error.to_string();
        assert!(message.contains("game.txt") && message.contains("line ") && message.contains("rubies"), "{message}");
        assert!(message.contains("move the file aside"), "{message}");
        assert!(!message.contains(".bak"), "no backup exists yet: {message}");
        assert_eq!(std::fs::read_to_string(&path).unwrap(), garbage, "the bad file is never rewritten");
    }

    #[test]
    fn the_error_points_at_the_backup_when_there_is_one() {
        let (dir, world) = (scratch(), world());
        let path = dir.join("game.txt");
        let game = played(&world);
        save(&path, &game).unwrap();
        back_up(&path).unwrap();
        std::fs::write(&path, "garbage").unwrap();
        let error = load(&path, None, &ctx(&world)).unwrap_err();
        assert!(error.to_string().contains("game.txt.bak"), "{error}");
        // The backup itself loads, so the player can copy it back.
        assert_eq!(load(&backup_path(&path), None, &ctx(&world)).unwrap().game, game);
    }

    #[test]
    fn a_truncated_file_is_invalid_not_a_smaller_game() {
        let (dir, world) = (scratch(), world());
        let path = dir.join("game.txt");
        let text = played(&world).to_text();
        let cut = &text[..text.len() / 2];
        std::fs::write(&path, cut).unwrap();
        let error = load(&path, None, &ctx(&world)).unwrap_err();
        assert!(matches!(error, StoreError::Invalid { .. }), "{error}");
        assert_eq!(std::fs::read_to_string(&path).unwrap(), cut);
    }

    #[test]
    fn a_newer_format_is_reported_and_left_alone() {
        let (dir, world) = (scratch(), world());
        let path = dir.join("game.txt");
        let text = played(&world).to_text().replacen("ozbound-save 2", "ozbound-save 9", 1);
        std::fs::write(&path, &text).unwrap();
        let error = load(&path, None, &ctx(&world)).unwrap_err();
        assert!(error.to_string().contains("format 9"), "{error}");
        assert_eq!(std::fs::read_to_string(&path).unwrap(), text);
    }

    #[test]
    fn an_unreadable_path_is_a_read_error() {
        let (dir, world) = (scratch(), world());
        // A directory where the file should be cannot be read as text.
        let error = load(&dir, None, &ctx(&world)).unwrap_err();
        assert!(matches!(error, StoreError::Read { .. }), "{error}");
    }

    #[test]
    fn a_write_failure_is_reported_and_keeps_the_old_save() {
        let (dir, world) = (scratch(), world());
        let blocker = dir.join("file");
        std::fs::write(&blocker, "x").unwrap();
        // The "folder" is actually a file, so the save cannot be created under it.
        let error = save(&blocker.join("game.txt"), &played(&world)).unwrap_err();
        assert!(matches!(error, StoreError::Write { .. }), "{error}");

        // A target that cannot be replaced (a directory) fails and leaves no temp file behind.
        let target = dir.join("target");
        std::fs::create_dir_all(target.join("inner")).unwrap();
        let error = save(&target, &played(&world)).unwrap_err();
        assert!(matches!(error, StoreError::Write { .. }), "{error}");
        assert!(!temp_path(&target).exists());
        assert!(target.join("inner").exists());
    }

    // ---- the pre-M6 quest save ----

    fn legacy_log(world: &World) -> QuestLog {
        let mut log = QuestLog::new();
        log.start(&world.content.quests, "moonpetals").unwrap();
        log.record(&world.content.quests, &QuestEvent::Gathered { item: ItemId::Moonpetal, quantity: 2 });
        log
    }

    #[test]
    fn an_old_quest_save_becomes_the_quest_log_of_a_new_game() {
        let (dir, world) = (scratch(), world());
        let legacy = dir.join("quests.txt");
        quest_store::save(&legacy, &legacy_log(&world)).unwrap();
        let loaded = load(&dir.join("game.txt"), Some(&legacy), &ctx(&world)).unwrap();
        assert_eq!(loaded.start, Start::MigratedQuests);
        assert_eq!(loaded.game.quests, legacy_log(&world));
        let mut expected = SaveGame::new_game(&world.level);
        expected.quests = legacy_log(&world);
        assert_eq!(loaded.game, expected, "everything but quests is a fresh start");
        assert!(legacy.exists(), "the old file is left in place");
    }

    #[test]
    fn a_full_save_wins_over_the_old_quest_save() {
        let (dir, world) = (scratch(), world());
        let (path, legacy) = (dir.join("game.txt"), dir.join("quests.txt"));
        quest_store::save(&legacy, &legacy_log(&world)).unwrap();
        let game = SaveGame::new_game(&world.level);
        save(&path, &game).unwrap();
        let loaded = load(&path, Some(&legacy), &ctx(&world)).unwrap();
        assert_eq!(loaded.start, Start::Loaded);
        assert_eq!(loaded.game, game);
    }

    #[test]
    fn no_save_and_no_old_save_is_a_new_game() {
        let (dir, world) = (scratch(), world());
        let loaded = load(&dir.join("game.txt"), Some(&dir.join("quests.txt")), &ctx(&world)).unwrap();
        assert_eq!(loaded.start, Start::NewGame);
    }

    #[test]
    fn an_invalid_old_quest_save_is_reported_and_left_untouched() {
        let (dir, world) = (scratch(), world());
        let legacy = dir.join("quests.txt");
        let garbage = "ozbound-quests 1\nquest nonexistent active 0 0\n";
        std::fs::write(&legacy, garbage).unwrap();
        let error = load(&dir.join("game.txt"), Some(&legacy), &ctx(&world)).unwrap_err();
        assert!(matches!(error, StoreError::LegacyQuests(_)), "{error}");
        assert!(error.to_string().contains("quests.txt"), "{error}");
        assert_eq!(std::fs::read_to_string(&legacy).unwrap(), garbage);
    }

    // ---- the backup ----

    #[test]
    fn back_up_copies_the_save_and_leaves_no_temp_file() {
        let (dir, world) = (scratch(), world());
        let path = dir.join("game.txt");
        save(&path, &played(&world)).unwrap();
        back_up(&path).unwrap();
        let bak = backup_path(&path);
        assert_eq!(std::fs::read(&bak).unwrap(), std::fs::read(&path).unwrap());
        assert!(!temp_path(&bak).exists());
        assert_eq!(bak.file_name().unwrap(), "game.txt.bak");
    }

    #[test]
    fn backing_up_nothing_is_fine_and_a_later_backup_replaces_the_earlier_one() {
        let (dir, world) = (scratch(), world());
        let path = dir.join("game.txt");
        back_up(&path).unwrap();
        assert!(!backup_path(&path).exists());
        save(&path, &SaveGame::new_game(&world.level)).unwrap();
        back_up(&path).unwrap();
        let newer = played(&world);
        save(&path, &newer).unwrap();
        back_up(&path).unwrap();
        assert_eq!(load(&backup_path(&path), None, &ctx(&world)).unwrap().game, newer);
    }
}
