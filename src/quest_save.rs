//! Saving and loading the quest log as versioned plain text, independent of rendering.
//!
//! Pure Rust (no Bevy, no file I/O): `to_text` and `from_text` convert between a `QuestLog` and
//! a string. The full save (`save.rs`) writes its quest lines in this same format, and
//! `save_store.rs` decides where the text lives and writes it safely.
//!
//! Format, version 1 (one quest per line, in the order the quests were started):
//!
//! ```text
//! ozbound-quests 1
//! quest moonpetals active 1 2     # objective index, progress on that objective
//! quest apothecary_visit ready
//! quest first_steps completed
//! ```
//!
//! Loading checks the text against the quest definitions and refuses anything that could not
//! have been produced by playing: unknown quests, duplicates, out-of-range progress, or a
//! started quest whose prerequisites are not completed. Loading never partly succeeds.

use crate::quests::{QuestBook, QuestLog, QuestStatus, Stage};
use crate::textdata::{is_valid_id, ContentError};

/// The save format this build writes and reads.
pub const FORMAT_VERSION: u32 = 1;

const MAGIC: &str = "ozbound-quests";

impl QuestLog {
    /// Writes the log in the versioned text format.
    pub fn to_text(&self) -> String {
        let mut out = format!("{MAGIC} {FORMAT_VERSION}\n");
        for (id, stage) in &self.entries {
            match stage {
                Stage::Active { objective, count } => {
                    out.push_str(&format!("quest {id} active {objective} {count}\n"))
                }
                Stage::Ready => out.push_str(&format!("quest {id} ready\n")),
                Stage::Completed => out.push_str(&format!("quest {id} completed\n")),
            }
        }
        out
    }

    /// Reads a log written by [`QuestLog::to_text`], validating it against `book`.
    pub fn from_text(text: &str, book: &QuestBook) -> Result<Self, ContentError> {
        let mut lines = text
            .trim_start_matches('\u{feff}')
            .lines()
            .enumerate()
            .map(|(i, raw)| (i + 1, raw.trim()))
            .filter(|(_, line)| !line.is_empty() && !line.starts_with('#'));

        let Some((header_line, header)) = lines.next() else {
            return Err(ContentError::at(0, "the quest save is empty"));
        };
        let version = match header.split_whitespace().collect::<Vec<_>>().as_slice() {
            [magic, version] if *magic == MAGIC => version.parse::<u32>().map_err(|_| {
                ContentError::at(
                    header_line,
                    format!("`{version}` is not a format version number"),
                )
            })?,
            _ => {
                return Err(ContentError::at(
                    header_line,
                    format!("not a quest save: the first line must be `{MAGIC} <version>`"),
                ))
            }
        };
        if version == 0 {
            return Err(ContentError::at(
                header_line,
                "format version 0 does not exist",
            ));
        }
        if version > FORMAT_VERSION {
            return Err(ContentError::at(
                header_line,
                format!("this save uses format {version}, but this version of Ozbound reads up to format {FORMAT_VERSION}"),
            ));
        }

        let mut log = QuestLog::new();
        let mut line_of: Vec<usize> = Vec::new();
        for (number, line) in lines {
            let words: Vec<&str> = line.split_whitespace().collect();
            let (id, stage) = match words.as_slice() {
                ["quest", id, "active", objective, count] => {
                    let objective = objective.parse::<usize>().map_err(|_| {
                        ContentError::at(
                            number,
                            format!("`{objective}` is not an objective number"),
                        )
                    })?;
                    let count = count.parse::<u32>().map_err(|_| {
                        ContentError::at(number, format!("`{count}` is not a progress count"))
                    })?;
                    (*id, Stage::Active { objective, count })
                }
                ["quest", id, "ready"] => (*id, Stage::Ready),
                ["quest", id, "completed"] => (*id, Stage::Completed),
                ["quest", _, other, ..]
                    if QuestStatus::from_key(other).is_none() || *other == "not_started" =>
                {
                    return Err(ContentError::at(
                        number,
                        format!("unknown quest state `{other}` (use active, ready, or completed)"),
                    ));
                }
                _ => return Err(ContentError::at(number, format!("cannot read `{line}`"))),
            };
            if !is_valid_id(id) {
                return Err(ContentError::at(
                    number,
                    format!("`{id}` is not a valid quest id"),
                ));
            }
            let Some(def) = book.get(id) else {
                return Err(ContentError::at(number, format!("unknown quest `{id}`")));
            };
            if log.stage(id).is_some() {
                return Err(ContentError::at(
                    number,
                    format!("quest `{id}` appears twice"),
                ));
            }
            if let Stage::Active { objective, count } = stage {
                let Some(current) = def.objectives.get(objective) else {
                    return Err(ContentError::at(
                        number,
                        format!("quest `{id}` has {} objectives, so objective {objective} does not exist", def.objectives.len()),
                    ));
                };
                if count >= current.needed() {
                    return Err(ContentError::at(
                        number,
                        format!("quest `{id}` objective {objective} needs {}, so progress {count} means it should already be done", current.needed()),
                    ));
                }
            }
            log.entries.push((id.to_string(), stage));
            line_of.push(number);
        }

        // A quest can only have been started after its prerequisites were completed.
        for (index, (id, _)) in log.entries.iter().enumerate() {
            let Some(def) = book.get(id) else { continue };
            if let Some(missing) = def
                .prerequisites
                .iter()
                .find(|p| log.status(p) != QuestStatus::Completed)
            {
                return Err(ContentError::at(
                    line_of[index],
                    format!("quest `{id}` was started, but its prerequisite `{missing}` is not completed"),
                ));
            }
        }
        Ok(log)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::inventory::{Inventory, ItemId};
    use crate::quests::QuestEvent;

    fn book() -> QuestBook {
        QuestBook::parse(
            "[quest herbs]\ntitle = Herbs\nobjective = talk guide\nobjective = collect moonpetal 3\n\
             [quest planks]\ntitle = Planks\nrequires = herbs\nobjective = collect timber 1\n\
             [quest chat]\ntitle = Chat\nobjective = talk guide\n",
        )
        .unwrap()
    }

    fn err(text: &str) -> ContentError {
        QuestLog::from_text(text, &book()).unwrap_err()
    }

    #[test]
    fn empty_log_round_trips() {
        let log = QuestLog::new();
        let text = log.to_text();
        assert_eq!(text, "ozbound-quests 1\n");
        assert_eq!(QuestLog::from_text(&text, &book()).unwrap(), log);
    }

    #[test]
    fn every_stage_round_trips_in_start_order() {
        let book = book();
        let mut log = QuestLog::new();
        log.start(&book, "chat").unwrap();
        log.start(&book, "herbs").unwrap();
        log.record(
            &book,
            &QuestEvent::SpokeTo {
                npc: "guide".into(),
            },
        );
        log.record(
            &book,
            &QuestEvent::Gathered {
                item: ItemId::Moonpetal,
                quantity: 2,
            },
        );
        let text = log.to_text();
        assert_eq!(
            text,
            "ozbound-quests 1\nquest chat ready\nquest herbs active 1 2\n"
        );
        let loaded = QuestLog::from_text(&text, &book).unwrap();
        assert_eq!(loaded, log);

        log.record(
            &book,
            &QuestEvent::Gathered {
                item: ItemId::Moonpetal,
                quantity: 1,
            },
        );
        log.complete(&book, "chat", &mut Inventory::default())
            .unwrap();
        log.complete(&book, "herbs", &mut Inventory::default())
            .unwrap();
        log.start(&book, "planks").unwrap();
        let text = log.to_text();
        assert_eq!(text, "ozbound-quests 1\nquest chat completed\nquest herbs completed\nquest planks active 0 0\n");
        assert_eq!(QuestLog::from_text(&text, &book).unwrap(), log);
    }

    #[test]
    fn a_loaded_log_keeps_playing_where_it_left_off() {
        let book = book();
        let mut log = QuestLog::new();
        log.start(&book, "herbs").unwrap();
        log.record(
            &book,
            &QuestEvent::SpokeTo {
                npc: "guide".into(),
            },
        );
        log.record(
            &book,
            &QuestEvent::Gathered {
                item: ItemId::Moonpetal,
                quantity: 2,
            },
        );
        let mut loaded = QuestLog::from_text(&log.to_text(), &book).unwrap();
        let ups = loaded.record(
            &book,
            &QuestEvent::Gathered {
                item: ItemId::Moonpetal,
                quantity: 1,
            },
        );
        assert_eq!(loaded.status("herbs"), QuestStatus::ReadyToTurnIn);
        assert_eq!(ups.len(), 2);
    }

    #[test]
    fn comments_blank_lines_crlf_and_bom_are_tolerated() {
        let text = "\u{feff}# saved\r\nozbound-quests 1\r\n\r\n# progress\r\nquest chat ready\r\n";
        assert_eq!(
            QuestLog::from_text(text, &book()).unwrap().status("chat"),
            QuestStatus::ReadyToTurnIn
        );
    }

    #[test]
    fn bad_headers_and_versions_are_refused() {
        assert!(err("").message.contains("empty"));
        assert!(err("# only a comment\n").message.contains("empty"));
        let e = err("quests 1\n");
        assert_eq!(
            (e.line, e.message.contains("not a quest save")),
            (Some(1), true)
        );
        let e = err("ozbound-quests\n");
        assert!(e.message.contains("not a quest save"));
        let e = err("ozbound-quests one\n");
        assert!(e.message.contains("not a format version number"));
        let e = err("ozbound-quests 0\n");
        assert!(e.message.contains("does not exist"));
        let e = err("ozbound-quests 2\nquest chat ready\n");
        assert_eq!(e.line, Some(1));
        assert!(e.message.contains("format 2") && e.message.contains("up to format 1"));
    }

    #[test]
    fn corrupt_entries_are_refused_with_their_line() {
        let head = "ozbound-quests 1\n";
        let e = err(&format!("{head}quest chat\n"));
        assert_eq!((e.line, e.message.contains("cannot read")), (Some(2), true));
        let e = err(&format!("{head}something else\n"));
        assert!(e.message.contains("cannot read"));
        let e = err(&format!("{head}quest chat paused\n"));
        assert!(e.message.contains("unknown quest state `paused`"));
        let e = err(&format!("{head}quest chat not_started\n"));
        assert!(e.message.contains("unknown quest state `not_started`"));
        let e = err(&format!("{head}quest Chat ready\n"));
        assert!(e.message.contains("not a valid quest id"));
        let e = err(&format!("{head}quest ghost ready\n"));
        assert!(e.message.contains("unknown quest `ghost`"));
        let e = err(&format!("{head}quest chat ready\nquest chat completed\n"));
        assert_eq!((e.line, e.message.contains("twice")), (Some(3), true));
        let e = err(&format!("{head}quest herbs active x 0\n"));
        assert!(e.message.contains("not an objective number"));
        let e = err(&format!("{head}quest herbs active 0 -1\n"));
        assert!(e.message.contains("not a progress count"));
        let e = err(&format!("{head}quest herbs active 2 0\n"));
        assert!(e.message.contains("2 objectives") && e.message.contains("does not exist"));
        let e = err(&format!("{head}quest herbs active 1 3\n"));
        assert!(e.message.contains("should already be done"));
        let e = err(&format!("{head}quest chat active 0 1\n"));
        assert!(e.message.contains("should already be done"));
    }

    #[test]
    fn a_started_quest_must_have_its_prerequisites_completed() {
        let head = "ozbound-quests 1\n";
        let e = err(&format!("{head}quest planks active 0 0\n"));
        assert_eq!(e.line, Some(2));
        assert!(e.message.contains("prerequisite `herbs`"));
        let e = err(&format!(
            "{head}quest herbs ready\nquest planks active 0 0\n"
        ));
        assert!(e.message.contains("prerequisite `herbs`"));
        assert!(QuestLog::from_text(
            &format!("{head}quest herbs completed\nquest planks active 0 0\n"),
            &book()
        )
        .is_ok());
    }

    #[test]
    fn loading_never_partly_succeeds() {
        let text = "ozbound-quests 1\nquest chat ready\nquest ghost ready\n";
        assert!(QuestLog::from_text(text, &book()).is_err());
    }
}
