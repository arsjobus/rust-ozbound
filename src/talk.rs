//! One conversation in progress, ready for a UI to draw and click through.
//!
//! Pure Rust (no Bevy). `Talk` wraps a `DialogueSession` and keeps the text and choices the
//! player currently sees, so a UI only has to draw `Talk::view` and call `Talk::choose` with the
//! index of the clicked choice. Every quest change a step causes is returned as plain-language
//! lines (see `journal::outcome_messages`) for the journal message.

use crate::content::Content;
use crate::dialogue::{DialogueSession, QuestContext};
pub use crate::dialogue::DialogueView;
use crate::inventory::Inventory;
use crate::journal::outcome_messages;
use crate::quests::QuestLog;
use crate::skills::Skills;

#[derive(Debug, Clone)]
pub struct Talk {
    session: DialogueSession,
    view: DialogueView,
}

impl Talk {
    /// Starts the conversation with the NPC named `npc` in the dialogue file. Talking to an NPC
    /// is recorded for `talk` objectives, and the opening node's effects run.
    pub fn start(
        content: &Content,
        log: &mut QuestLog,
        inventory: &mut Inventory,
        skills: &Skills,
        npc: &str,
    ) -> Result<(Self, Vec<String>), String> {
        let conversation = content
            .dialogue
            .conversation_for_npc(npc)
            .ok_or_else(|| "They have nothing to say right now.".to_string())?;
        let id = conversation.id.clone();
        let mut ctx = QuestContext { quests: &content.quests, log, inventory, skills };
        let (session, outcome) = DialogueSession::begin(&content.dialogue, &id, &mut ctx)
            .map_err(|e| format!("The conversation could not start: {e:?}"))?;
        let messages = outcome_messages(&content.quests, &outcome);
        let view = session
            .view(&content.dialogue, &ctx)
            .ok_or_else(|| "They have nothing to say right now.".to_string())?;
        Ok((Self { session, view }, messages))
    }

    /// What the player currently sees: speaker, text, and the choices offered.
    pub fn view(&self) -> &DialogueView {
        &self.view
    }

    /// Picks the `index`th offered choice. Returns the journal lines the step caused and
    /// whether the conversation is still open. An index that is not offered changes nothing.
    pub fn choose(
        &mut self,
        content: &Content,
        log: &mut QuestLog,
        inventory: &mut Inventory,
        skills: &Skills,
        index: usize,
    ) -> (Vec<String>, bool) {
        let mut ctx = QuestContext { quests: &content.quests, log, inventory, skills };
        match self.session.choose(&content.dialogue, index, &mut ctx) {
            Ok(outcome) => {
                let messages = outcome_messages(&content.quests, &outcome);
                match self.session.view(&content.dialogue, &ctx) {
                    Some(view) => {
                        self.view = view;
                        (messages, true)
                    }
                    None => (messages, false),
                }
            }
            Err(_) => (Vec::new(), !self.session.is_finished()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::inventory::ItemId;
    use crate::quests::{QuestEvent, QuestStatus};

    fn content() -> Content {
        Content::builtin().unwrap()
    }

    #[test]
    fn unknown_npcs_have_nothing_to_say() {
        let content = content();
        let error = Talk::start(&content, &mut QuestLog::new(), &mut Inventory::default(), &Skills::default(), "nobody").unwrap_err();
        assert_eq!(error, "They have nothing to say right now.");
    }

    #[test]
    fn first_meeting_offers_the_quest_and_accepting_starts_it() {
        let content = content();
        let (mut log, mut bag) = (QuestLog::new(), Inventory::default());
        let (mut talk, opening) = Talk::start(&content, &mut log, &mut bag, &Skills::default(), "guide").unwrap();
        assert!(opening.is_empty());
        assert_eq!(talk.view().speaker, "Village Guide");
        assert_eq!(talk.view().choices, ["I can gather some.", "Not right now."]);

        let (messages, open) = talk.choose(&content, &mut log, &mut bag, &Skills::default(), 0);
        assert!(open);
        assert_eq!(messages, ["New quest: Moonpetals for the Apothecary."]);
        assert_eq!(log.status("moonpetals"), QuestStatus::Active);
        assert!(talk.view().text.starts_with("Wonderful."));

        let (messages, open) = talk.choose(&content, &mut log, &mut bag, &Skills::default(), 0);
        assert!(messages.is_empty());
        assert!(!open, "the last choice ends the conversation");
    }

    #[test]
    fn declining_ends_the_talk_and_starts_nothing() {
        let content = content();
        let (mut log, mut bag) = (QuestLog::new(), Inventory::default());
        let (mut talk, _) = Talk::start(&content, &mut log, &mut bag, &Skills::default(), "guide").unwrap();
        let (messages, open) = talk.choose(&content, &mut log, &mut bag, &Skills::default(), 1);
        assert!(messages.is_empty());
        assert!(!open);
        assert_eq!(log.status("moonpetals"), QuestStatus::NotStarted);
    }

    #[test]
    fn a_choice_that_is_not_offered_changes_nothing() {
        let content = content();
        let (mut log, mut bag) = (QuestLog::new(), Inventory::default());
        let (mut talk, _) = Talk::start(&content, &mut log, &mut bag, &Skills::default(), "guide").unwrap();
        let before = talk.view().clone();
        let (messages, open) = talk.choose(&content, &mut log, &mut bag, &Skills::default(), 7);
        assert!(messages.is_empty());
        assert!(open);
        assert_eq!(talk.view(), &before);
        assert_eq!(log, QuestLog::new());
    }

    #[test]
    fn the_greeting_depends_on_quest_progress() {
        let content = content();
        let (mut log, mut bag) = (QuestLog::new(), Inventory::default());
        log.start(&content.quests, "moonpetals").unwrap();
        let (talk, _) = Talk::start(&content, &mut log, &mut bag, &Skills::default(), "guide").unwrap();
        assert!(talk.view().text.starts_with("Three moonpetals, please"));
    }

    #[test]
    fn handing_in_takes_petals_and_pays_once() {
        let content = content();
        let (mut log, mut bag) = (QuestLog::new(), Inventory::default());
        log.start(&content.quests, "moonpetals").unwrap();
        for _ in 0..3 {
            bag.add(ItemId::Moonpetal, 1).unwrap();
        }
        log.record(&content.quests, &QuestEvent::Gathered { item: ItemId::Moonpetal, quantity: 3 });

        let (mut talk, opening) = Talk::start(&content, &mut log, &mut bag, &Skills::default(), "guide").unwrap();
        assert_eq!(
            opening,
            ["Quest complete: Moonpetals for the Apothecary. You received 2 timber."]
        );
        assert_eq!(bag.count(ItemId::Moonpetal), 0);
        assert_eq!(bag.count(ItemId::Timber), 2);
        talk.choose(&content, &mut log, &mut bag, &Skills::default(), 0);

        let (talk, opening) = Talk::start(&content, &mut log, &mut bag, &Skills::default(), "guide").unwrap();
        assert!(opening.is_empty());
        assert!(talk.view().text.starts_with("Thank you again"));
        assert_eq!(bag.count(ItemId::Timber), 2);
    }

    #[test]
    fn talking_advances_a_talk_objective() {
        let quests = "[quest hello]\ntitle = Say hello\nsummary = Greet the guide.\nobjective = talk guide\n";
        let dialogue = "[conversation guide]\nnpc = guide\nspeaker = Guide\nentry = hi\n\n[node hi]\ntext = Hello.\nchoice = Bye. -> end\n";
        let quests = crate::quests::QuestBook::parse(quests).unwrap();
        let dialogue = crate::dialogue::DialogueBook::parse(dialogue).unwrap();
        dialogue.check_against(&quests).unwrap();
        let content = Content { quests, dialogue };
        let (mut log, mut bag) = (QuestLog::new(), Inventory::default());
        log.start(&content.quests, "hello").unwrap();
        let (_, messages) = Talk::start(&content, &mut log, &mut bag, &Skills::default(), "guide").unwrap();
        assert_eq!(
            messages,
            ["Say hello: Talk to guide - done.", "Say hello is ready to hand in."]
        );
        assert_eq!(log.status("hello"), QuestStatus::ReadyToTurnIn);
    }
}
