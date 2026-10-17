//! Loading the quest and dialogue content files, with the same rules as the level file.
//!
//! Pure Rust (no Bevy). Each file is read from a path; a custom path (the caller passes it
//! in, usually from an environment variable) must load. The default path is used when the
//! file exists, and when it is simply absent the identical compiled-in copy is used instead
//! so the game starts from any directory. A file that exists but is invalid is never
//! replaced by the built-in copy: the error names the file and the line to fix.

use std::fmt::Display;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};

use crate::dialogue::{DialogueBook, DEFAULT_DIALOGUE};
use crate::quests::{QuestBook, DEFAULT_QUESTS};

/// Where the game looks for its quest file, relative to the working directory.
pub const QUESTS_PATH: &str = "assets/quests/meadow.quest";
/// Where the game looks for its dialogue file, relative to the working directory.
pub const DIALOGUE_PATH: &str = "assets/dialogue/meadow.dialogue";

/// Validated quest and dialogue content that agree with each other.
#[derive(Debug, Clone)]
pub struct Content {
    pub quests: QuestBook,
    pub dialogue: DialogueBook,
}

/// Where to read each file from.
#[derive(Debug, Clone, Copy)]
pub struct Sources<'a> {
    pub quests_custom: Option<&'a Path>,
    pub dialogue_custom: Option<&'a Path>,
    pub quests_default: &'a Path,
    pub dialogue_default: &'a Path,
}

impl Sources<'_> {
    /// The shipped default paths and no custom files.
    pub fn defaults() -> Sources<'static> {
        Sources {
            quests_custom: None,
            dialogue_custom: None,
            quests_default: Path::new(QUESTS_PATH),
            dialogue_default: Path::new(DIALOGUE_PATH),
        }
    }
}

/// The loaded content plus plain-language notes (for example "using the built-in quests").
#[derive(Debug, Clone)]
pub struct Loaded {
    pub content: Content,
    pub notes: Vec<String>,
}

impl Content {
    /// The copies compiled into the game. They are checked by tests, so this cannot fail in a
    /// released build; the error is returned rather than hidden anyway.
    pub fn builtin() -> Result<Self, String> {
        let quests = QuestBook::parse(DEFAULT_QUESTS).map_err(|e| format!("built-in quests are invalid: {e}"))?;
        let dialogue =
            DialogueBook::parse(DEFAULT_DIALOGUE).map_err(|e| format!("built-in dialogue is invalid: {e}"))?;
        dialogue
            .check_against(&quests)
            .map_err(|e| format!("built-in dialogue does not match the built-in quests: {e}"))?;
        Ok(Self { quests, dialogue })
    }
}

/// Reads one text file under the custom/default/built-in rules above.
/// Returns the text, the label to use in error messages, and a note when the built-in copy was used.
fn read_source(
    what: &str,
    custom: Option<&Path>,
    default: &Path,
    builtin: &'static str,
) -> Result<(String, String, Option<String>), String> {
    if let Some(path) = custom {
        let text = std::fs::read_to_string(path)
            .map_err(|e| format!("could not read {what} file {}: {e}", path.display()))?;
        return Ok((text, path.display().to_string(), None));
    }
    match std::fs::read_to_string(default) {
        Ok(text) => Ok((text, default.display().to_string(), None)),
        Err(e) if e.kind() == ErrorKind::NotFound => Ok((
            builtin.to_string(),
            format!("built-in {what}s"),
            Some(format!("note: {} not found; using the built-in {what}s", default.display())),
        )),
        Err(e) => Err(format!("could not read {what} file {}: {e}", default.display())),
    }
}

fn invalid(what: &str, label: &str, error: impl Display) -> String {
    if label.starts_with("built-in") {
        format!("{label} are invalid: {error}")
    } else {
        format!("invalid {what} file {label}: {error}")
    }
}

/// Loads, parses, and cross-checks both content files. Any problem is a plain message that
/// says which file (and line) is at fault.
pub fn load(sources: &Sources) -> Result<Loaded, String> {
    let mut notes = Vec::new();

    let (quest_text, quest_label, note) =
        read_source("quest", sources.quests_custom, sources.quests_default, DEFAULT_QUESTS)?;
    notes.extend(note);
    let quests = QuestBook::parse(&quest_text).map_err(|e| invalid("quest", &quest_label, e))?;

    let (dialogue_text, dialogue_label, note) =
        read_source("dialogue", sources.dialogue_custom, sources.dialogue_default, DEFAULT_DIALOGUE)?;
    notes.extend(note);
    let dialogue = DialogueBook::parse(&dialogue_text).map_err(|e| invalid("dialogue", &dialogue_label, e))?;
    dialogue.check_against(&quests).map_err(|e| {
        format!("dialogue file {dialogue_label} does not match the quests in {quest_label}: {e}")
    })?;

    Ok(Loaded { content: Content { quests, dialogue }, notes })
}

/// Convenience for callers that only have the two optional custom paths.
pub fn load_with_custom(quests: Option<PathBuf>, dialogue: Option<PathBuf>) -> Result<Loaded, String> {
    let defaults = Sources::defaults();
    load(&Sources {
        quests_custom: quests.as_deref(),
        dialogue_custom: dialogue.as_deref(),
        ..defaults
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU32, Ordering};

    /// A fresh scratch directory per test.
    fn scratch() -> PathBuf {
        static NEXT: AtomicU32 = AtomicU32::new(0);
        let dir = std::env::temp_dir().join(format!(
            "ozbound-content-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn missing_defaults(dir: &Path) -> (PathBuf, PathBuf) {
        (dir.join("none.quest"), dir.join("none.dialogue"))
    }

    #[test]
    fn builtin_content_is_valid_and_consistent() {
        let content = Content::builtin().unwrap();
        assert!(content.quests.get("moonpetals").is_some());
        assert!(content.dialogue.conversation_for_npc("guide").is_some());
    }

    #[test]
    fn missing_default_files_fall_back_to_builtin_with_notes() {
        let dir = scratch();
        let (q, d) = missing_defaults(&dir);
        let sources = Sources { quests_custom: None, dialogue_custom: None, quests_default: &q, dialogue_default: &d };
        let loaded = load(&sources).unwrap();
        assert_eq!(loaded.notes.len(), 2);
        assert!(loaded.notes[0].contains("built-in quests"));
        assert!(loaded.notes[1].contains("built-in dialogues"));
        assert!(loaded.content.quests.get("moonpetals").is_some());
    }

    #[test]
    fn shipped_default_paths_load_without_notes() {
        // Tests run with the package root as the working directory.
        let loaded = load(&Sources::defaults()).unwrap();
        assert!(loaded.notes.is_empty(), "{:?}", loaded.notes);
    }

    #[test]
    fn existing_default_file_is_used_over_builtin() {
        let dir = scratch();
        let (q, d) = missing_defaults(&dir);
        std::fs::write(&q, "[quest tea]\ntitle = Tea\nsummary = Brew tea.\nobjective = collect timber 1\n").unwrap();
        let sources = Sources { quests_custom: None, dialogue_custom: None, quests_default: &q, dialogue_default: &d };
        // The built-in dialogue still mentions the `moonpetals` quest, so the pair is rejected:
        // proof the on-disk quest file was the one read.
        let error = load(&sources).unwrap_err();
        assert!(error.contains("does not match"), "{error}");
        assert!(error.contains("none.dialogue") || error.contains("built-in dialogues"), "{error}");
    }

    #[test]
    fn invalid_default_file_is_not_replaced_by_builtin() {
        let dir = scratch();
        let (q, d) = missing_defaults(&dir);
        std::fs::write(&q, "[quest broken]\nbogus line\n").unwrap();
        let sources = Sources { quests_custom: None, dialogue_custom: None, quests_default: &q, dialogue_default: &d };
        let error = load(&sources).unwrap_err();
        assert!(error.starts_with("invalid quest file"), "{error}");
        assert!(error.contains("none.quest"), "{error}");
        assert!(error.contains("line 2"), "{error}");
    }

    #[test]
    fn custom_paths_must_exist_and_report_their_name() {
        let dir = scratch();
        let (q, d) = missing_defaults(&dir);
        let custom = dir.join("custom.quest");
        let sources = Sources { quests_custom: Some(&custom), dialogue_custom: None, quests_default: &q, dialogue_default: &d };
        let error = load(&sources).unwrap_err();
        assert!(error.starts_with("could not read quest file"), "{error}");
        assert!(error.contains("custom.quest"), "{error}");
    }

    #[test]
    fn invalid_custom_dialogue_reports_file_and_line() {
        let dir = scratch();
        let (q, d) = missing_defaults(&dir);
        let custom = dir.join("custom.dialogue");
        std::fs::write(&custom, "[conversation a]\nnpc = guide\nwhat = is this\n").unwrap();
        let sources = Sources { quests_custom: None, dialogue_custom: Some(&custom), quests_default: &q, dialogue_default: &d };
        let error = load(&sources).unwrap_err();
        assert!(error.starts_with("invalid dialogue file"), "{error}");
        assert!(error.contains("custom.dialogue"), "{error}");
        assert!(error.contains("line 3"), "{error}");
    }

    #[test]
    fn dialogue_naming_an_unknown_quest_is_rejected() {
        let dir = scratch();
        let (q, d) = missing_defaults(&dir);
        let custom = dir.join("custom.dialogue");
        let text = DEFAULT_DIALOGUE.replace("start moonpetals", "start nonexistent");
        assert_ne!(text, DEFAULT_DIALOGUE);
        std::fs::write(&custom, text).unwrap();
        let sources = Sources { quests_custom: None, dialogue_custom: Some(&custom), quests_default: &q, dialogue_default: &d };
        let error = load(&sources).unwrap_err();
        assert!(error.contains("does not match"), "{error}");
        assert!(error.contains("nonexistent"), "{error}");
    }
}
