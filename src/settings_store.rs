//! Reads and writes the settings file (see `settings.rs` for the format).
//!
//! The rules match the save file's, scaled down for something that is easy to recreate:
//!
//! - A missing file is not an error: the player simply has the defaults.
//! - Writes go to `<name>.tmp`, are flushed, and are then renamed over the real file, so a crash
//!   mid-write cannot leave half a file.
//! - A file that exists but cannot be read or understood is reported with its path and line and
//!   is never overwritten (the Bevy layer switches saving of settings off for that session), so
//!   the player's hand edits are not lost to a typo.

use std::fmt;
use std::io::{ErrorKind, Write};
use std::path::{Path, PathBuf};

use crate::settings::Settings;
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
            Self::Read { path, error } => write!(f, "could not read settings {}: {error}", path.display()),
            Self::Invalid { path, error } => write!(
                f,
                "invalid settings {}: {error}. The file was left alone: fix that line, or delete the file to go back to the defaults",
                path.display()
            ),
            Self::Write { path, error } => write!(f, "could not write settings {}: {error}", path.display()),
        }
    }
}

impl std::error::Error for StoreError {}

/// The default place for the settings file (override with `OZBOUND_SETTINGS`).
pub const DEFAULT_PATH: &str = "saves/settings.txt";

/// Loads the settings at `path`. A missing file gives the defaults.
pub fn load(path: &Path) -> Result<Settings, StoreError> {
    let text = match std::fs::read_to_string(path) {
        Ok(text) => text,
        Err(error) if error.kind() == ErrorKind::NotFound => return Ok(Settings::default()),
        Err(error) => return Err(StoreError::Read { path: path.to_path_buf(), error }),
    };
    Settings::from_text(&text).map_err(|error| StoreError::Invalid { path: path.to_path_buf(), error })
}

/// Writes the settings, creating the folder if needed, through a temporary file and a rename.
pub fn save(path: &Path, settings: &Settings) -> Result<(), StoreError> {
    let write_error = |error| StoreError::Write { path: path.to_path_buf(), error };
    if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
        std::fs::create_dir_all(parent).map_err(write_error)?;
    }
    let temp = temp_path(path);
    let written = std::fs::File::create(&temp).and_then(|mut file| {
        file.write_all(settings.to_text().as_bytes())?;
        file.sync_all()
    });
    if let Err(error) = written {
        let _ = std::fs::remove_file(&temp);
        return Err(write_error(error));
    }
    if let Err(error) = std::fs::rename(&temp, path) {
        let _ = std::fs::remove_file(&temp);
        return Err(write_error(error));
    }
    Ok(())
}

/// `<path>.tmp`, next to the real file so the rename stays on one filesystem.
pub fn temp_path(path: &Path) -> PathBuf {
    let mut name = path.file_name().map(|n| n.to_os_string()).unwrap_or_default();
    name.push(".tmp");
    path.with_file_name(name)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::settings::TextSize;
    use std::sync::atomic::{AtomicUsize, Ordering};

    static NEXT: AtomicUsize = AtomicUsize::new(0);

    fn folder() -> PathBuf {
        let dir = std::env::temp_dir().join(format!("ozbound-settings-{}-{}", std::process::id(), NEXT.fetch_add(1, Ordering::Relaxed)));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn busy() -> Settings {
        let mut settings = Settings::default();
        settings.set_volume(2);
        settings.text_size = TextSize::Large;
        settings.show_perf = true;
        settings
    }

    #[test]
    fn a_missing_file_means_the_defaults() {
        let dir = folder();
        assert_eq!(load(&dir.join("nothing.txt")).unwrap(), Settings::default());
    }

    #[test]
    fn saving_then_loading_round_trips_and_creates_the_folder() {
        let dir = folder();
        let path = dir.join("deeper").join("settings.txt");
        save(&path, &busy()).unwrap();
        assert_eq!(load(&path).unwrap(), busy());
        assert!(!temp_path(&path).exists(), "no temporary file is left behind");
    }

    #[test]
    fn saving_again_replaces_the_old_settings() {
        let dir = folder();
        let path = dir.join("settings.txt");
        save(&path, &busy()).unwrap();
        save(&path, &Settings::default()).unwrap();
        assert_eq!(load(&path).unwrap(), Settings::default());
    }

    #[test]
    fn an_invalid_file_names_the_path_and_line_and_is_not_changed_by_loading() {
        let dir = folder();
        let path = dir.join("settings.txt");
        let bad = "ozbound-settings 1\nvolume = 99\n";
        std::fs::write(&path, bad).unwrap();
        let error = load(&path).unwrap_err();
        let text = error.to_string();
        assert!(matches!(error, StoreError::Invalid { .. }));
        assert!(text.contains("settings.txt") && text.contains("line 2") && text.contains("left alone"), "{text}");
        assert_eq!(std::fs::read_to_string(&path).unwrap(), bad);
    }

    #[test]
    fn a_stale_temporary_file_does_not_disturb_loading_or_saving() {
        let dir = folder();
        let path = dir.join("settings.txt");
        std::fs::write(temp_path(&path), "half a fi").unwrap();
        assert_eq!(load(&path).unwrap(), Settings::default());
        save(&path, &busy()).unwrap();
        assert_eq!(load(&path).unwrap(), busy());
        assert!(!temp_path(&path).exists());
    }

    #[test]
    fn a_path_that_cannot_be_read_is_a_read_error() {
        let dir = folder();
        // A directory where the file should be: reading it fails with something other than NotFound.
        let error = load(&dir).unwrap_err();
        assert!(matches!(error, StoreError::Read { .. }), "{error}");
        assert!(error.to_string().contains("could not read settings"));
    }

    #[test]
    fn a_failed_write_is_reported_and_leaves_no_temporary_file() {
        let dir = folder();
        // The folder for the file is really a file, so it cannot be created.
        let blocker = dir.join("blocker");
        std::fs::write(&blocker, "x").unwrap();
        let path = blocker.join("settings.txt");
        let error = save(&path, &busy()).unwrap_err();
        assert!(matches!(error, StoreError::Write { .. }), "{error}");
        assert!(error.to_string().contains("could not write settings"));
        // Renaming onto a directory fails too, and cleans up after itself.
        let target = dir.join("is_a_dir");
        std::fs::create_dir_all(&target).unwrap();
        assert!(save(&target, &busy()).is_err());
        assert!(!temp_path(&target).exists());
    }

    #[test]
    fn the_temporary_name_sits_beside_the_file() {
        assert_eq!(temp_path(Path::new("saves/settings.txt")), PathBuf::from("saves/settings.txt.tmp"));
        assert_eq!(temp_path(Path::new("settings.txt")), PathBuf::from("settings.txt.tmp"));
    }
}
