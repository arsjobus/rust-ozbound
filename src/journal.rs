//! The quest journal: a plain-data view of the quest log, listing active and completed quests.
//!
//! Pure Rust (no Bevy). `Journal::build` turns the quest definitions and the player's
//! `QuestLog` into entries a UI can draw; `Journal::render` gives the same thing as text.
//! Quests appear in the order they were started. Objectives the player has not reached yet
//! are not listed, so the journal never spoils what comes next.

use crate::quests::{QuestBook, QuestLog, QuestStatus};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JournalStep {
    pub text: String,
    pub done: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JournalEntry {
    pub quest: String,
    pub title: String,
    pub summary: String,
    pub status: QuestStatus,
    pub steps: Vec<JournalStep>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Journal {
    /// Quests in progress, including ones ready to hand in.
    pub active: Vec<JournalEntry>,
    pub completed: Vec<JournalEntry>,
}

impl Journal {
    pub fn build(book: &QuestBook, log: &QuestLog) -> Self {
        let mut journal = Self::default();
        for id in log.started() {
            let Some(def) = book.get(id) else { continue };
            let status = log.status(id);
            let steps = match status {
                QuestStatus::NotStarted => continue,
                QuestStatus::Active => {
                    let (current, count) = log.current_objective(id).unwrap_or((0, 0));
                    def.objectives
                        .iter()
                        .enumerate()
                        .take(current + 1)
                        .map(|(i, objective)| {
                            let mut text = objective.describe();
                            if i == current && objective.needed() > 1 {
                                text.push_str(&format!(" ({count}/{})", objective.needed()));
                            }
                            JournalStep {
                                text,
                                done: i < current,
                            }
                        })
                        .collect()
                }
                QuestStatus::ReadyToTurnIn | QuestStatus::Completed => {
                    let mut steps: Vec<JournalStep> = def
                        .objectives
                        .iter()
                        .map(|o| JournalStep {
                            text: o.describe(),
                            done: true,
                        })
                        .collect();
                    if status == QuestStatus::ReadyToTurnIn {
                        steps.push(JournalStep {
                            text: "Hand in the quest".into(),
                            done: false,
                        });
                    }
                    steps
                }
            };
            let entry = JournalEntry {
                quest: def.id.clone(),
                title: def.title.clone(),
                summary: def.summary.clone(),
                status,
                steps,
            };
            if status == QuestStatus::Completed {
                journal.completed.push(entry);
            } else {
                journal.active.push(entry);
            }
        }
        journal
    }

    pub fn is_empty(&self) -> bool {
        self.active.is_empty() && self.completed.is_empty()
    }

    /// The journal as plain text, e.g. for a text panel.
    pub fn render(&self) -> String {
        if self.is_empty() {
            return "Your journal is empty. Talk to people to find something to do.".to_string();
        }
        let mut out = String::new();
        for (heading, entries) in [
            ("Active quests", &self.active),
            ("Completed quests", &self.completed),
        ] {
            if entries.is_empty() {
                continue;
            }
            if !out.is_empty() {
                out.push('\n');
            }
            out.push_str(heading);
            out.push('\n');
            for entry in entries {
                out.push_str(&format!("- {}\n", entry.title));
                if !entry.summary.is_empty() {
                    out.push_str(&format!("    {}\n", entry.summary));
                }
                for step in &entry.steps {
                    out.push_str(&format!(
                        "    [{}] {}\n",
                        if step.done { "x" } else { " " },
                        step.text
                    ));
                }
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::inventory::{Inventory, ItemId};
    use crate::quests::QuestEvent;

    fn book() -> QuestBook {
        QuestBook::parse(
            "[quest herbs]\ntitle = Herbs\nsummary = Gather herbs.\nobjective = talk village_guide\nobjective = collect moonpetal 2\nobjective = talk village_guide\n\
             [quest planks]\ntitle = Planks\nrequires = herbs\nobjective = collect timber 1\n",
        )
        .unwrap()
    }

    fn texts(entry: &JournalEntry) -> Vec<(&str, bool)> {
        entry
            .steps
            .iter()
            .map(|s| (s.text.as_str(), s.done))
            .collect()
    }

    #[test]
    fn empty_log_gives_an_empty_journal() {
        let j = Journal::build(&book(), &QuestLog::new());
        assert!(j.is_empty());
        assert!(j.render().contains("journal is empty"));
    }

    #[test]
    fn active_quests_show_finished_steps_and_the_current_one_but_not_future_ones() {
        let book = book();
        let mut log = QuestLog::new();
        log.start(&book, "herbs").unwrap();
        let j = Journal::build(&book, &log);
        assert_eq!((j.active.len(), j.completed.len()), (1, 0));
        assert_eq!(texts(&j.active[0]), vec![("Talk to village guide", false)]);

        log.record(
            &book,
            &QuestEvent::SpokeTo {
                npc: "village_guide".into(),
            },
        );
        log.record(
            &book,
            &QuestEvent::Gathered {
                item: ItemId::Moonpetal,
                quantity: 1,
            },
        );
        let j = Journal::build(&book, &log);
        assert_eq!(
            texts(&j.active[0]),
            vec![
                ("Talk to village guide", true),
                ("Collect moonpetal (1/2)", false)
            ]
        );
        assert_eq!(j.active[0].title, "Herbs");
        assert_eq!(j.active[0].status, QuestStatus::Active);
    }

    #[test]
    fn ready_quests_stay_active_with_a_hand_in_step_and_completed_ones_move_down() {
        let book = QuestBook::parse("[quest a]\ntitle = A\nsummary = S\nobjective = collect timber 1\nreward = copper_ore 1\n[quest b]\ntitle = B\nobjective = talk x\n").unwrap();
        let mut log = QuestLog::new();
        log.start(&book, "a").unwrap();
        log.start(&book, "b").unwrap();
        log.record(
            &book,
            &QuestEvent::Gathered {
                item: ItemId::Timber,
                quantity: 1,
            },
        );
        let j = Journal::build(&book, &log);
        assert_eq!(j.active.len(), 2);
        assert_eq!(j.active[0].status, QuestStatus::ReadyToTurnIn);
        assert_eq!(
            texts(&j.active[0]),
            vec![("Collect timber", true), ("Hand in the quest", false)]
        );

        let mut bag = Inventory::default();
        log.complete(&book, "a", &mut bag).unwrap();
        let j = Journal::build(&book, &log);
        assert_eq!((j.active.len(), j.completed.len()), (1, 1));
        assert_eq!(j.active[0].quest, "b");
        assert_eq!(j.completed[0].quest, "a");
        assert_eq!(texts(&j.completed[0]), vec![("Collect timber", true)]);
    }

    #[test]
    fn render_lists_sections_summaries_and_checkboxes() {
        let book = book();
        let mut log = QuestLog::new();
        log.start(&book, "herbs").unwrap();
        log.record(
            &book,
            &QuestEvent::SpokeTo {
                npc: "village_guide".into(),
            },
        );
        let text = Journal::build(&book, &log).render();
        assert_eq!(
            text,
            "Active quests\n- Herbs\n    Gather herbs.\n    [x] Talk to village guide\n    [ ] Collect moonpetal (0/2)\n"
        );
    }

    #[test]
    fn render_separates_active_and_completed_sections() {
        let book = QuestBook::parse(
            "[quest a]\ntitle = A\nobjective = talk x\n[quest b]\ntitle = B\nobjective = talk y\n",
        )
        .unwrap();
        let mut log = QuestLog::new();
        log.start(&book, "a").unwrap();
        log.start(&book, "b").unwrap();
        log.record(&book, &QuestEvent::SpokeTo { npc: "x".into() });
        log.complete(&book, "a", &mut Inventory::default()).unwrap();
        let text = Journal::build(&book, &log).render();
        assert_eq!(
            text,
            "Active quests\n- B\n    [ ] Talk to y\n\nCompleted quests\n- A\n    [x] Talk to x\n"
        );
    }

    #[test]
    fn quests_missing_from_the_book_are_skipped_not_fatal() {
        let book = book();
        let mut log = QuestLog::new();
        log.start(&book, "herbs").unwrap();
        let smaller =
            QuestBook::parse("[quest planks]\ntitle = Planks\nobjective = collect timber 1\n")
                .unwrap();
        assert!(Journal::build(&smaller, &log).is_empty());
    }
}
