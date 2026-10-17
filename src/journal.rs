//! The quest journal: a plain-data view of the quest log, listing active and completed quests.
//!
//! Pure Rust (no Bevy). `Journal::build` turns the quest definitions and the player's
//! `QuestLog` into entries a UI can draw; `Journal::render` gives the same thing as text.
//! Quests appear in the order they were started. Objectives the player has not reached yet
//! are not listed, so the journal never spoils what comes next.

use crate::dialogue::{EffectFailure, Outcome};
use crate::quests::{CompleteError, QuestBook, QuestLog, QuestStatus, QuestUpdate, StartError};

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

/// The quest's title for messages, falling back to its id for a quest missing from the book.
fn title_of<'a>(book: &'a QuestBook, id: &'a str) -> &'a str {
    book.get(id).map_or(id, |def| def.title.as_str())
}

/// One plain-language line for a quest state change, for the journal message line.
pub fn describe_update(book: &QuestBook, update: &QuestUpdate) -> String {
    let step = |quest: &str, objective: usize| {
        book.get(quest)
            .and_then(|def| def.objectives.get(objective))
            .map_or_else(|| "Objective".to_string(), |o| o.describe())
    };
    match update {
        QuestUpdate::Started { quest } => format!("New quest: {}.", title_of(book, quest)),
        QuestUpdate::Progress { quest, objective, count, needed } => {
            format!("{}: {} ({count}/{needed})", title_of(book, quest), step(quest, *objective))
        }
        QuestUpdate::ObjectiveDone { quest, objective } => {
            format!("{}: {} - done.", title_of(book, quest), step(quest, *objective))
        }
        QuestUpdate::ReadyToTurnIn { quest } => {
            format!("{} is ready to hand in.", title_of(book, quest))
        }
        QuestUpdate::Completed { quest, rewards } => {
            let mut text = format!("Quest complete: {}.", title_of(book, quest));
            if !rewards.is_empty() {
                let list: Vec<String> =
                    rewards.iter().map(|r| format!("{} {}", r.quantity, r.item.label())).collect();
                text.push_str(&format!(" You received {}.", list.join(", ")));
            }
            text
        }
    }
}

/// One plain-language line for an effect that could not be applied.
pub fn describe_failure(book: &QuestBook, failure: &EffectFailure) -> String {
    match failure {
        EffectFailure::Start { quest, error } => match error {
            StartError::AlreadyStarted(_) => format!("{} is already under way.", title_of(book, quest)),
            StartError::PrerequisiteNotMet(_) => {
                format!("You are not ready to start {} yet.", title_of(book, quest))
            }
            StartError::UnknownQuest(_) => "That quest cannot be started.".to_string(),
        },
        EffectFailure::Complete { quest, error } => match error {
            CompleteError::InventoryFull => {
                format!("Your bag is too full to receive the reward for {}.", title_of(book, quest))
            }
            CompleteError::MissingItems(item) => {
                format!("You do not have the {} needed for {}.", item.label(), title_of(book, quest))
            }
            CompleteError::NotReady(_) => format!("{} is not ready to hand in.", title_of(book, quest)),
            CompleteError::UnknownQuest(_) => "That quest cannot be completed.".to_string(),
        },
    }
}

/// Everything worth telling the player about a dialogue step: quest changes, then failures.
pub fn outcome_messages(book: &QuestBook, outcome: &Outcome) -> Vec<String> {
    outcome
        .updates
        .iter()
        .map(|u| describe_update(book, u))
        .chain(outcome.failures.iter().map(|f| describe_failure(book, f)))
        .collect()
}

/// A one-line quest tracker for the HUD: the first active quest and its next step.
pub fn tracker(book: &QuestBook, log: &QuestLog) -> String {
    let journal = Journal::build(book, log);
    match journal.active.first() {
        Some(entry) => match entry.steps.iter().find(|s| !s.done) {
            Some(step) => format!("Quest: {} - {}", entry.title, step.text),
            None => format!("Quest: {}", entry.title),
        },
        None => "No active quests.".to_string(),
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

#[cfg(test)]
mod message_tests {
    use super::*;
    use crate::inventory::{Inventory, ItemId};
    use crate::quests::{ItemAmount, QuestEvent, DEFAULT_QUESTS};

    fn book() -> QuestBook {
        QuestBook::parse(DEFAULT_QUESTS).unwrap()
    }

    fn gathered(n: u32) -> QuestEvent {
        QuestEvent::Gathered { item: ItemId::Moonpetal, quantity: n }
    }

    #[test]
    fn updates_read_as_plain_sentences() {
        let book = book();
        let mut log = QuestLog::new();
        log.start(&book, "moonpetals").unwrap();
        let progress = log.record(&book, &gathered(1));
        assert_eq!(
            describe_update(&book, &QuestUpdate::Started { quest: "moonpetals".into() }),
            "New quest: Moonpetals for the Apothecary."
        );
        assert_eq!(describe_update(&book, &progress[0]), "Moonpetals for the Apothecary: Collect moonpetal (1/3)");
        let done = log.record(&book, &gathered(2));
        let texts: Vec<String> = done.iter().map(|u| describe_update(&book, u)).collect();
        assert!(texts.contains(&"Moonpetals for the Apothecary: Collect moonpetal - done.".to_string()), "{texts:?}");
        assert!(texts.contains(&"Moonpetals for the Apothecary is ready to hand in.".to_string()), "{texts:?}");
    }

    #[test]
    fn completion_lists_rewards_and_handles_none() {
        let book = book();
        let with = QuestUpdate::Completed {
            quest: "moonpetals".into(),
            rewards: vec![ItemAmount { item: ItemId::Timber, quantity: 2 }],
        };
        assert_eq!(
            describe_update(&book, &with),
            "Quest complete: Moonpetals for the Apothecary. You received 2 timber."
        );
        let without = QuestUpdate::Completed { quest: "moonpetals".into(), rewards: vec![] };
        assert_eq!(describe_update(&book, &without), "Quest complete: Moonpetals for the Apothecary.");
    }

    #[test]
    fn unknown_quests_fall_back_to_their_id() {
        let book = book();
        assert_eq!(describe_update(&book, &QuestUpdate::Started { quest: "ghost".into() }), "New quest: ghost.");
        let progress = QuestUpdate::Progress { quest: "ghost".into(), objective: 4, count: 1, needed: 2 };
        assert_eq!(describe_update(&book, &progress), "ghost: Objective (1/2)");
    }

    #[test]
    fn failures_explain_themselves() {
        let book = book();
        let full = EffectFailure::Complete { quest: "moonpetals".into(), error: CompleteError::InventoryFull };
        assert_eq!(
            describe_failure(&book, &full),
            "Your bag is too full to receive the reward for Moonpetals for the Apothecary."
        );
        let missing = EffectFailure::Complete {
            quest: "moonpetals".into(),
            error: CompleteError::MissingItems(ItemId::Moonpetal),
        };
        assert!(describe_failure(&book, &missing).starts_with("You do not have the moonpetal needed"));
        let again = EffectFailure::Start {
            quest: "moonpetals".into(),
            error: StartError::AlreadyStarted(QuestStatus::Active),
        };
        assert_eq!(describe_failure(&book, &again), "Moonpetals for the Apothecary is already under way.");
    }

    #[test]
    fn outcome_messages_list_updates_before_failures() {
        let book = book();
        let outcome = Outcome {
            updates: vec![QuestUpdate::Started { quest: "moonpetals".into() }],
            failures: vec![EffectFailure::Complete { quest: "moonpetals".into(), error: CompleteError::InventoryFull }],
        };
        let lines = outcome_messages(&book, &outcome);
        assert_eq!(lines.len(), 2);
        assert!(lines[0].starts_with("New quest"));
        assert!(lines[1].starts_with("Your bag is too full"));
        assert!(outcome_messages(&book, &Outcome::default()).is_empty());
    }

    #[test]
    fn tracker_follows_the_quest_through_every_stage() {
        let book = book();
        let mut log = QuestLog::new();
        assert_eq!(tracker(&book, &log), "No active quests.");
        log.start(&book, "moonpetals").unwrap();
        assert_eq!(tracker(&book, &log), "Quest: Moonpetals for the Apothecary - Collect moonpetal (0/3)");
        log.record(&book, &gathered(2));
        assert_eq!(tracker(&book, &log), "Quest: Moonpetals for the Apothecary - Collect moonpetal (2/3)");
        log.record(&book, &gathered(1));
        assert_eq!(tracker(&book, &log), "Quest: Moonpetals for the Apothecary - Hand in the quest");
        let mut bag = Inventory::default();
        bag.add(ItemId::Moonpetal, 3).unwrap();
        log.complete(&book, "moonpetals", &mut bag).unwrap();
        assert_eq!(tracker(&book, &log), "No active quests.");
    }
}
