//! Data-driven dialogue: conversations made of nodes and choices, with conditions on quest and
//! bag state and effects that start or complete quests.
//!
//! Pure Rust (no Bevy). Conversations are validated content (`DialogueBook`, usually parsed
//! from `assets/dialogue/*.dialogue`). A `DialogueSession` walks one conversation; it holds
//! only the current node, and every call is given the book and the live `QuestContext`.
//!
//! Content rules enforced when loading, so a player can never get stuck:
//! - every conversation has an `entry` list whose last entry has no condition;
//! - every node has text and at least one choice with no condition (an always-available way on);
//! - every choice leads to a node that exists, or ends the conversation;
//! - every node is reachable from an entry, and from every node the player can reach the end
//!   using only unconditional choices.
//!
//! Playing a conversation: `begin` records that the player spoke to the NPC (so `talk`
//! objectives advance), picks the first entry whose conditions hold, and enters that node.
//! Entering a node runs its effects in order. An effect that cannot be applied (for example
//! completing a quest whose rewards do not fit) is reported in `Outcome::failures` and changes
//! nothing; use the `can_complete` condition to route such cases to a helpful node.

use std::collections::BTreeSet;

use crate::inventory::{Inventory, ItemId};
use crate::quests::{
    CompleteError, QuestBook, QuestEvent, QuestLog, QuestStatus, QuestUpdate, StartError,
};
use crate::textdata::{check_id, parse_positive, read_lines, ContentError, Item};

/// The dialogue file compiled into the game, identical to `assets/dialogue/meadow.dialogue`.
pub const DEFAULT_DIALOGUE: &str = include_str!("../assets/dialogue/meadow.dialogue");

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Condition {
    /// `quest <id> is <status>` (or `not` when `negate`).
    Quest {
        id: String,
        status: QuestStatus,
        negate: bool,
    },
    /// `has <item> <n>`: the bag holds at least this many.
    Has { item: ItemId, quantity: u32 },
    /// `can_complete <id>`: the quest is ready and its items and rewards fit the bag.
    CanComplete { id: String },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Effect {
    Start(String),
    Complete(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Target {
    Node(String),
    End,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Choice {
    pub text: String,
    pub target: Target,
    /// All must hold for the choice to be offered; empty means always offered.
    pub when: Vec<Condition>,
    pub line: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Node {
    pub id: String,
    /// Lines joined with `\n`.
    pub text: String,
    pub effects: Vec<Effect>,
    pub choices: Vec<Choice>,
    pub line: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    pub node: String,
    pub when: Vec<Condition>,
    pub line: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Conversation {
    pub id: String,
    /// The NPC spoken to; starting the conversation records `QuestEvent::SpokeTo` for it.
    pub npc: String,
    /// Name shown above the text.
    pub speaker: String,
    pub entries: Vec<Entry>,
    pub nodes: Vec<Node>,
    pub line: usize,
}

impl Conversation {
    pub fn node(&self, id: &str) -> Option<&Node> {
        self.nodes.iter().find(|n| n.id == id)
    }
}

/// All conversations, validated (see the module docs for the rules).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct DialogueBook {
    conversations: Vec<Conversation>,
}

impl DialogueBook {
    pub fn new(conversations: Vec<Conversation>) -> Result<Self, ContentError> {
        validate(&conversations)?;
        Ok(Self { conversations })
    }

    pub fn conversation(&self, id: &str) -> Option<&Conversation> {
        self.conversations.iter().find(|c| c.id == id)
    }

    /// The conversation held with an NPC, if any.
    pub fn conversation_for_npc(&self, npc: &str) -> Option<&Conversation> {
        self.conversations.iter().find(|c| c.npc == npc)
    }

    pub fn iter(&self) -> impl Iterator<Item = &Conversation> {
        self.conversations.iter()
    }

    pub fn len(&self) -> usize {
        self.conversations.len()
    }

    pub fn is_empty(&self) -> bool {
        self.conversations.is_empty()
    }

    /// Checks that every quest named in a condition or effect exists in `quests`.
    pub fn check_against(&self, quests: &QuestBook) -> Result<(), ContentError> {
        let known = |id: &str, line: usize| {
            if quests.get(id).is_some() {
                Ok(())
            } else {
                Err(ContentError::at(line, format!("unknown quest `{id}`")))
            }
        };
        let conds = |list: &[Condition], line: usize| -> Result<(), ContentError> {
            for c in list {
                match c {
                    Condition::Quest { id, .. } | Condition::CanComplete { id } => known(id, line)?,
                    Condition::Has { .. } => {}
                }
            }
            Ok(())
        };
        for conv in &self.conversations {
            for e in &conv.entries {
                conds(&e.when, e.line)?;
            }
            for node in &conv.nodes {
                for effect in &node.effects {
                    match effect {
                        Effect::Start(id) | Effect::Complete(id) => known(id, node.line)?,
                    }
                }
                for choice in &node.choices {
                    conds(&choice.when, choice.line)?;
                }
            }
        }
        Ok(())
    }

    /// Parses and validates a `.dialogue` file (format documented in
    /// `assets/dialogue/meadow.dialogue`).
    pub fn parse(text: &str) -> Result<Self, ContentError> {
        let mut convs: Vec<Conversation> = Vec::new();
        for line in read_lines(text)? {
            let n = line.number;
            match line.item {
                Item::Section { kind, arg } => match kind.as_str() {
                    "conversation" => {
                        check_id(n, "conversation id", &arg)?;
                        convs.push(Conversation {
                            id: arg,
                            npc: String::new(),
                            speaker: String::new(),
                            entries: Vec::new(),
                            nodes: Vec::new(),
                            line: n,
                        });
                    }
                    "node" => {
                        check_id(n, "node id", &arg)?;
                        let Some(conv) = convs.last_mut() else {
                            return Err(ContentError::at(
                                n,
                                "a `[node ...]` must come after a `[conversation ...]`",
                            ));
                        };
                        conv.nodes.push(Node {
                            id: arg,
                            text: String::new(),
                            effects: Vec::new(),
                            choices: Vec::new(),
                            line: n,
                        });
                    }
                    other => {
                        return Err(ContentError::at(
                            n,
                            format!("unknown section `[{other}]` (expected `[conversation <id>]` or `[node <id>]`)"),
                        ));
                    }
                },
                Item::Field { key, value } => {
                    let Some(conv) = convs.last_mut() else {
                        return Err(ContentError::at(
                            n,
                            format!("`{key}` must be inside a `[conversation <id>]` section"),
                        ));
                    };
                    if let Some(node) = conv.nodes.last_mut() {
                        match key.as_str() {
                            "text" => {
                                if !node.text.is_empty() {
                                    node.text.push('\n');
                                }
                                node.text.push_str(&value);
                            }
                            "effect" => node.effects.push(parse_effect(n, &value)?),
                            "choice" => node.choices.push(parse_choice(n, &value)?),
                            "npc" | "speaker" | "entry" => {
                                return Err(ContentError::at(
                                    n,
                                    format!("`{key}` belongs before the first `[node ...]`"),
                                ));
                            }
                            other => {
                                return Err(ContentError::at(
                                    n,
                                    format!("unknown node field `{other}`"),
                                ))
                            }
                        }
                    } else {
                        match key.as_str() {
                            "npc" => {
                                check_id(n, "npc id", &value)?;
                                if !conv.npc.is_empty() {
                                    return Err(ContentError::at(n, "`npc` is given twice"));
                                }
                                conv.npc = value;
                            }
                            "speaker" => {
                                if !conv.speaker.is_empty() {
                                    return Err(ContentError::at(n, "`speaker` is given twice"));
                                }
                                conv.speaker = value;
                            }
                            "entry" => {
                                let (node, when) = split_condition(n, &value)?;
                                check_id(n, "node id", node)?;
                                conv.entries.push(Entry {
                                    node: node.to_string(),
                                    when,
                                    line: n,
                                });
                            }
                            "text" | "effect" | "choice" => {
                                return Err(ContentError::at(
                                    n,
                                    format!("`{key}` must be inside a `[node ...]` section"),
                                ));
                            }
                            other => {
                                return Err(ContentError::at(
                                    n,
                                    format!("unknown conversation field `{other}`"),
                                ))
                            }
                        }
                    }
                }
            }
        }
        Self::new(convs)
    }
}

/// Splits `<head> [if <conditions>]` and parses the conditions.
fn split_condition(line: usize, value: &str) -> Result<(&str, Vec<Condition>), ContentError> {
    match value.split_once(" if ") {
        Some((head, conds)) => Ok((head.trim(), parse_conditions(line, conds)?)),
        None => Ok((value.trim(), Vec::new())),
    }
}

fn parse_choice(line: usize, value: &str) -> Result<Choice, ContentError> {
    let Some((text, rest)) = value.rsplit_once(" -> ") else {
        return Err(ContentError::at(
            line,
            "a choice must look like `choice = <text> -> <node or end>`",
        ));
    };
    let (target, when) = split_condition(line, rest)?;
    let target = if target == "end" {
        Target::End
    } else {
        check_id(line, "choice target", target)?;
        Target::Node(target.to_string())
    };
    Ok(Choice {
        text: text.trim().to_string(),
        target,
        when,
        line,
    })
}

fn parse_effect(line: usize, value: &str) -> Result<Effect, ContentError> {
    let words: Vec<&str> = value.split_whitespace().collect();
    match words.as_slice() {
        ["start", id] => {
            check_id(line, "quest id", id)?;
            Ok(Effect::Start((*id).to_string()))
        }
        ["complete", id] => {
            check_id(line, "quest id", id)?;
            Ok(Effect::Complete((*id).to_string()))
        }
        _ => Err(ContentError::at(
            line,
            format!("effect `{value}` must be `start <quest>` or `complete <quest>`"),
        )),
    }
}

fn parse_conditions(line: usize, text: &str) -> Result<Vec<Condition>, ContentError> {
    text.split(" and ")
        .map(|part| parse_condition(line, part.trim()))
        .collect()
}

fn parse_condition(line: usize, text: &str) -> Result<Condition, ContentError> {
    let words: Vec<&str> = text.split_whitespace().collect();
    match words.as_slice() {
        ["quest", id, verb @ ("is" | "not"), status] => {
            check_id(line, "quest id", id)?;
            let status = QuestStatus::from_key(status).ok_or_else(|| {
                ContentError::at(line, format!("unknown quest status `{status}` (use not_started, active, ready, or completed)"))
            })?;
            Ok(Condition::Quest { id: (*id).to_string(), status, negate: *verb == "not" })
        }
        ["has", item, quantity] => Ok(Condition::Has {
            item: ItemId::from_key(item).ok_or_else(|| ContentError::at(line, format!("unknown item `{item}`")))?,
            quantity: parse_positive(line, "the item amount", quantity)?,
        }),
        ["can_complete", id] => {
            check_id(line, "quest id", id)?;
            Ok(Condition::CanComplete { id: (*id).to_string() })
        }
        _ => Err(ContentError::at(
            line,
            format!("condition `{text}` must be `quest <id> is|not <status>`, `has <item> <n>`, or `can_complete <id>`"),
        )),
    }
}

fn validate(convs: &[Conversation]) -> Result<(), ContentError> {
    let mut ids = BTreeSet::new();
    let mut npcs = BTreeSet::new();
    for c in convs {
        if !ids.insert(c.id.as_str()) {
            return Err(ContentError::at(
                c.line,
                format!("conversation `{}` is defined twice", c.id),
            ));
        }
        let here =
            |msg: String| ContentError::at(c.line, format!("conversation `{}`: {msg}", c.id));
        if c.npc.is_empty() {
            return Err(here("needs an `npc`".into()));
        }
        if !npcs.insert(c.npc.as_str()) {
            return Err(here(format!("npc `{}` already has a conversation", c.npc)));
        }
        if c.speaker.trim().is_empty() {
            return Err(here("needs a `speaker`".into()));
        }
        validate_conversation(c)?;
    }
    Ok(())
}

fn validate_conversation(c: &Conversation) -> Result<(), ContentError> {
    let here = |line: usize, msg: String| {
        ContentError::at(line, format!("conversation `{}`: {msg}", c.id))
    };

    // Entries: each leads to a real node, and the last must always apply.
    let Some(last) = c.entries.last() else {
        return Err(here(c.line, "needs at least one `entry`".into()));
    };
    if !last.when.is_empty() {
        return Err(here(
            last.line,
            "the last `entry` must have no `if`, so every situation has somewhere to start".into(),
        ));
    }
    if let Some(shadow) = c
        .entries
        .iter()
        .position(|e| e.when.is_empty())
        .filter(|&i| i + 1 < c.entries.len())
    {
        return Err(here(
            c.entries[shadow].line,
            "an `entry` with no `if` hides the entries after it".into(),
        ));
    }
    for e in &c.entries {
        if c.node(&e.node).is_none() {
            return Err(here(
                e.line,
                format!("`entry` leads to unknown node `{}`", e.node),
            ));
        }
    }

    // Nodes: unique ids, text, an unconditional choice, and valid targets.
    let mut seen = BTreeSet::new();
    for node in &c.nodes {
        if !seen.insert(node.id.as_str()) {
            return Err(here(
                node.line,
                format!("node `{}` is defined twice", node.id),
            ));
        }
        if node.text.trim().is_empty() {
            return Err(here(node.line, format!("node `{}` needs `text`", node.id)));
        }
        if node.choices.is_empty() {
            return Err(here(
                node.line,
                format!("node `{}` needs at least one `choice`", node.id),
            ));
        }
        if node.choices.iter().all(|ch| !ch.when.is_empty()) {
            return Err(here(
                node.line,
                format!("node `{}` needs a `choice` with no `if`, or the player could be left with no way on", node.id),
            ));
        }
        for ch in &node.choices {
            if ch.text.is_empty() {
                return Err(here(ch.line, "a choice has no text".into()));
            }
            if let Target::Node(target) = &ch.target {
                if c.node(target).is_none() {
                    return Err(here(
                        ch.line,
                        format!("choice leads to unknown node `{target}`"),
                    ));
                }
            }
        }
    }

    // Reachability from the entries.
    let mut reached: BTreeSet<&str> = c.entries.iter().map(|e| e.node.as_str()).collect();
    let mut frontier: Vec<&str> = reached.iter().copied().collect();
    while let Some(id) = frontier.pop() {
        if let Some(node) = c.node(id) {
            for ch in &node.choices {
                if let Target::Node(t) = &ch.target {
                    if reached.insert(t.as_str()) {
                        frontier.push(t.as_str());
                    }
                }
            }
        }
    }
    if let Some(node) = c.nodes.iter().find(|n| !reached.contains(n.id.as_str())) {
        return Err(here(
            node.line,
            format!("node `{}` can never be reached from an `entry`", node.id),
        ));
    }

    // From every node there must be an unconditional way to the end.
    let mut can_end: BTreeSet<&str> = BTreeSet::new();
    loop {
        let before = can_end.len();
        for node in &c.nodes {
            let leaves = node.choices.iter().any(|ch| {
                ch.when.is_empty()
                    && match &ch.target {
                        Target::End => true,
                        Target::Node(t) => can_end.contains(t.as_str()),
                    }
            });
            if leaves {
                can_end.insert(node.id.as_str());
            }
        }
        if can_end.len() == before {
            break;
        }
    }
    if let Some(node) = c.nodes.iter().find(|n| !can_end.contains(n.id.as_str())) {
        return Err(here(
            node.line,
            format!(
                "node `{}` is a trap: its always-available choices never lead to the end",
                node.id
            ),
        ));
    }
    Ok(())
}

/// The live game state a conversation reads and changes.
pub struct QuestContext<'a> {
    pub quests: &'a QuestBook,
    pub log: &'a mut QuestLog,
    pub inventory: &'a mut Inventory,
}

impl QuestContext<'_> {
    fn holds(&self, conds: &[Condition]) -> bool {
        conds.iter().all(|c| match c {
            Condition::Quest { id, status, negate } => (self.log.status(id) == *status) != *negate,
            Condition::Has { item, quantity } => self.inventory.count(*item) >= *quantity,
            Condition::CanComplete { id } => self.log.can_complete(self.quests, id, self.inventory),
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EffectFailure {
    Start { quest: String, error: StartError },
    Complete { quest: String, error: CompleteError },
}

/// What happened when a node was entered.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Outcome {
    pub updates: Vec<QuestUpdate>,
    pub failures: Vec<EffectFailure>,
}

/// What the player sees at the current node.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DialogueView {
    pub speaker: String,
    pub text: String,
    /// Texts of the choices currently offered; `choose` takes an index into this list.
    pub choices: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DialogueError {
    UnknownConversation(String),
    /// The conversation has already ended.
    Ended,
    NoSuchChoice(usize),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DialogueSession {
    conversation: String,
    node: Option<String>,
}

impl DialogueSession {
    /// Starts a conversation: records that the NPC was spoken to, picks the entry, runs its effects.
    pub fn begin(
        book: &DialogueBook,
        conversation: &str,
        ctx: &mut QuestContext,
    ) -> Result<(Self, Outcome), DialogueError> {
        let conv = book
            .conversation(conversation)
            .ok_or_else(|| DialogueError::UnknownConversation(conversation.to_string()))?;
        let mut outcome = Outcome {
            updates: ctx.log.record(
                ctx.quests,
                &QuestEvent::SpokeTo {
                    npc: conv.npc.clone(),
                },
            ),
            failures: Vec::new(),
        };
        // Validation guarantees the last entry has no condition, so one always matches.
        let entry = conv
            .entries
            .iter()
            .find(|e| ctx.holds(&e.when))
            .unwrap_or(&conv.entries[conv.entries.len() - 1]);
        let mut session = Self {
            conversation: conv.id.clone(),
            node: None,
        };
        session.enter(conv, &entry.node, ctx, &mut outcome);
        Ok((session, outcome))
    }

    pub fn is_finished(&self) -> bool {
        self.node.is_none()
    }

    pub fn conversation(&self) -> &str {
        &self.conversation
    }

    /// The current text and offered choices, or `None` once the conversation has ended.
    pub fn view(&self, book: &DialogueBook, ctx: &QuestContext) -> Option<DialogueView> {
        let conv = book.conversation(&self.conversation)?;
        let node = conv.node(self.node.as_deref()?)?;
        Some(DialogueView {
            speaker: conv.speaker.clone(),
            text: node.text.clone(),
            choices: node
                .choices
                .iter()
                .filter(|c| ctx.holds(&c.when))
                .map(|c| c.text.clone())
                .collect(),
        })
    }

    /// Picks one of the offered choices (an index into `DialogueView::choices`).
    pub fn choose(
        &mut self,
        book: &DialogueBook,
        index: usize,
        ctx: &mut QuestContext,
    ) -> Result<Outcome, DialogueError> {
        let conv = book
            .conversation(&self.conversation)
            .ok_or_else(|| DialogueError::UnknownConversation(self.conversation.clone()))?;
        let current = self
            .node
            .as_deref()
            .and_then(|id| conv.node(id))
            .ok_or(DialogueError::Ended)?;
        let target = {
            let offered: Vec<&Choice> = current
                .choices
                .iter()
                .filter(|c| ctx.holds(&c.when))
                .collect();
            offered
                .get(index)
                .ok_or(DialogueError::NoSuchChoice(index))?
                .target
                .clone()
        };
        let mut outcome = Outcome::default();
        match target {
            Target::End => self.node = None,
            Target::Node(id) => self.enter(conv, &id, ctx, &mut outcome),
        }
        Ok(outcome)
    }

    /// Leaves the conversation immediately (for example when the player walks away).
    pub fn close(&mut self) {
        self.node = None;
    }

    fn enter(
        &mut self,
        conv: &Conversation,
        id: &str,
        ctx: &mut QuestContext,
        outcome: &mut Outcome,
    ) {
        self.node = Some(id.to_string());
        let Some(node) = conv.node(id) else { return };
        for effect in &node.effects {
            match effect {
                Effect::Start(quest) => match ctx.log.start(ctx.quests, quest) {
                    Ok(()) => outcome.updates.push(QuestUpdate::Started {
                        quest: quest.clone(),
                    }),
                    Err(error) => outcome.failures.push(EffectFailure::Start {
                        quest: quest.clone(),
                        error,
                    }),
                },
                Effect::Complete(quest) => match ctx.log.complete(ctx.quests, quest, ctx.inventory)
                {
                    Ok(rewards) => outcome.updates.push(QuestUpdate::Completed {
                        quest: quest.clone(),
                        rewards,
                    }),
                    Err(error) => outcome.failures.push(EffectFailure::Complete {
                        quest: quest.clone(),
                        error,
                    }),
                },
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::quests::ItemAmount;

    const QUESTS: &str = "\
[quest herbs]
title = Herbs
objective = talk guide
objective = collect moonpetal 1
takes = moonpetal 1
reward = timber 1

[quest sequel]
title = Sequel
requires = herbs
objective = talk guide
";

    const TALK: &str = "\
[conversation guide]
npc = guide
speaker = Guide
entry = done if quest herbs is completed
entry = ready if can_complete herbs
entry = full if quest herbs is ready
entry = busy if quest herbs is active
entry = hello

[node hello]
text = Hello.
text = Want work?
choice = Yes -> accept
choice = Sequel please -> sequel if quest herbs is completed
choice = No -> end

[node accept]
effect = start herbs
text = Good.
choice = Bye -> end

[node busy]
text = Keep going.
choice = Bye -> end
choice = Ask again -> busy if has moonpetal 1

[node ready]
effect = complete herbs
text = Thanks!
choice = Bye -> end

[node full]
text = No room.
choice = Bye -> end

[node done]
text = All done.
choice = Sequel please -> sequel
choice = Bye -> end

[node sequel]
effect = start sequel
text = One more.
choice = Bye -> end
";

    fn books() -> (QuestBook, DialogueBook) {
        let quests = QuestBook::parse(QUESTS).unwrap();
        let dialogue = DialogueBook::parse(TALK).unwrap();
        dialogue.check_against(&quests).unwrap();
        (quests, dialogue)
    }

    fn err(text: &str) -> ContentError {
        DialogueBook::parse(text).unwrap_err()
    }

    #[test]
    fn parses_conversations_nodes_choices_and_conditions() {
        let (_, d) = books();
        assert_eq!(d.len(), 1);
        let c = d.conversation("guide").unwrap();
        assert_eq!(
            (
                c.npc.as_str(),
                c.speaker.as_str(),
                c.entries.len(),
                c.nodes.len()
            ),
            ("guide", "Guide", 5, 7)
        );
        assert_eq!(d.conversation_for_npc("guide").unwrap().id, "guide");
        assert!(d.conversation_for_npc("nobody").is_none());
        assert_eq!(c.node("hello").unwrap().text, "Hello.\nWant work?");
        assert_eq!(
            c.node("accept").unwrap().effects,
            vec![Effect::Start("herbs".into())]
        );
        let choice = &c.node("hello").unwrap().choices[1];
        assert_eq!(choice.target, Target::Node("sequel".into()));
        assert_eq!(
            choice.when,
            vec![Condition::Quest {
                id: "herbs".into(),
                status: QuestStatus::Completed,
                negate: false
            }]
        );
        assert_eq!(c.node("hello").unwrap().choices[2].target, Target::End);
        assert_eq!(
            c.entries[1].when,
            vec![Condition::CanComplete { id: "herbs".into() }]
        );
    }

    #[test]
    fn choice_text_may_contain_arrows_and_the_word_if() {
        let text = "[conversation a]\nnpc = a\nspeaker = A\nentry = n\n[node n]\ntext = t\nchoice = Go -> there if you must -> end\n";
        let d = DialogueBook::parse(text).unwrap();
        let ch = &d.conversation("a").unwrap().node("n").unwrap().choices[0];
        assert_eq!(ch.text, "Go -> there if you must");
        assert_eq!(ch.target, Target::End);
        let text = "[conversation a]\nnpc = a\nspeaker = A\nentry = n\n[node n]\ntext = t\nchoice = Tell me if it works -> end\n";
        assert_eq!(
            DialogueBook::parse(text)
                .unwrap()
                .conversation("a")
                .unwrap()
                .node("n")
                .unwrap()
                .choices[0]
                .text,
            "Tell me if it works"
        );
    }

    #[test]
    fn conditions_can_be_combined_and_negated() {
        let text = "[conversation a]\nnpc = a\nspeaker = A\nentry = n\n[node n]\ntext = t\nchoice = Hm -> end if quest q not active and has timber 2\nchoice = Bye -> end\n";
        let book = DialogueBook::parse(text).unwrap();
        let ch = &book.conversation("a").unwrap().node("n").unwrap().choices[0];
        assert_eq!(
            ch.when,
            vec![
                Condition::Quest {
                    id: "q".into(),
                    status: QuestStatus::Active,
                    negate: true
                },
                Condition::Has {
                    item: ItemId::Timber,
                    quantity: 2
                },
            ]
        );
    }

    #[test]
    fn shipped_dialogue_is_valid_and_matches_the_shipped_quests() {
        let quests = QuestBook::parse(crate::quests::DEFAULT_QUESTS).unwrap();
        let d = DialogueBook::parse(DEFAULT_DIALOGUE).unwrap();
        d.check_against(&quests).unwrap();
        assert!(d.conversation_for_npc("guide").is_some());
        let disk = std::fs::read_to_string(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/assets/dialogue/meadow.dialogue"
        ))
        .unwrap();
        assert_eq!(disk, DEFAULT_DIALOGUE);
    }

    #[test]
    fn syntax_errors_name_the_line() {
        let head = "[conversation a]\nnpc = a\nspeaker = A\nentry = n\n";
        let ok = format!("{head}[node n]\ntext = t\nchoice = Bye -> end\n");
        assert!(DialogueBook::parse(&ok).is_ok());

        let e = err("npc = a\n");
        assert_eq!(
            (e.line, e.message.contains("inside a `[conversation")),
            (Some(1), true)
        );
        let e = err("[node n]\n");
        assert!(e.message.contains("after a `[conversation"));
        let e = err("[scene x]\n");
        assert!(e.message.contains("unknown section"));
        let e = err(&format!("{ok}colour = red\n"));
        assert_eq!(
            (e.line, e.message.contains("unknown node field")),
            (Some(8), true)
        );
        let e = err(&format!("{ok}npc = b\n"));
        assert!(e.message.contains("before the first `[node"));
        let e = err("[conversation a]\ntext = hi\n");
        assert!(e.message.contains("inside a `[node"));
        let e = err("[conversation a]\nmood = sad\n");
        assert!(e.message.contains("unknown conversation field"));
        let e = err("[conversation a]\nnpc = a\nnpc = b\n");
        assert_eq!(e.line, Some(3));
        let e = err("[conversation a]\nspeaker = A\nspeaker = B\n");
        assert_eq!(e.line, Some(3));
        let e = err(&format!("{head}[node n]\nchoice = no arrow\n"));
        assert_eq!(
            (e.line, e.message.contains("choice = <text> -> ")),
            (Some(6), true)
        );
        let e = err(&format!("{head}[node n]\nchoice = X -> Bad Id\n"));
        assert!(e.message.contains("choice target"));
        let e = err(&format!("{head}[node n]\neffect = dance q\n"));
        assert!(e.message.contains("start <quest>"));
        let e = err(&format!(
            "{head}[node n]\nchoice = X -> end if quest q is lost\n"
        ));
        assert!(e.message.contains("unknown quest status `lost`"));
        let e = err(&format!(
            "{head}[node n]\nchoice = X -> end if has gold 1\n"
        ));
        assert!(e.message.contains("unknown item `gold`"));
        let e = err(&format!(
            "{head}[node n]\nchoice = X -> end if has timber 0\n"
        ));
        assert!(e.message.contains("above zero"));
        let e = err(&format!("{head}[node n]\nchoice = X -> end if maybe\n"));
        assert!(e.message.contains("condition `maybe`"));
        let e = err("[conversation Bad]\n");
        assert!(e.message.contains("conversation id"));
    }

    #[test]
    fn invalid_structure_is_rejected() {
        let conv = |extra_head: &str, nodes: &str| {
            format!("[conversation a]\nnpc = a\nspeaker = A\n{extra_head}{nodes}")
        };
        let node = "[node n]\ntext = t\nchoice = Bye -> end\n";

        let e = err("[conversation a]\nspeaker = A\nentry = n\n");
        assert!(e.message.contains("needs an `npc`"));
        let e = err("[conversation a]\nnpc = a\nentry = n\n");
        assert!(e.message.contains("needs a `speaker`"));
        let e = err(&conv("", node));
        assert!(e.message.contains("at least one `entry`"));
        let e = err(&conv("entry = n if has timber 1\n", node));
        assert_eq!(
            (e.line, e.message.contains("last `entry` must have no `if`")),
            (Some(4), true)
        );
        let e = err(&conv("entry = n\nentry = n\n", node));
        assert!(e.message.contains("hides the entries after it"));
        let e = err(&conv("entry = ghost\n", node));
        assert!(e.message.contains("unknown node `ghost`"));
        let e = err(&conv("entry = n\n", &format!("{node}{node}")));
        assert!(e.message.contains("defined twice"));
        let e = err(&conv("entry = n\n", "[node n]\nchoice = Bye -> end\n"));
        assert!(e.message.contains("needs `text`"));
        let e = err(&conv("entry = n\n", "[node n]\ntext = t\n"));
        assert!(e.message.contains("at least one `choice`"));
        let e = err(&conv(
            "entry = n\n",
            "[node n]\ntext = t\nchoice = Bye -> end if has timber 1\n",
        ));
        assert!(e.message.contains("no `if`"));
        let e = err(&conv(
            "entry = n\n",
            "[node n]\ntext = t\nchoice = -> end\n",
        ));
        assert!(e.message.contains("choice = <text> -> "));
        let e = err(&conv(
            "entry = n\n",
            "[node n]\ntext = t\nchoice = Go -> missing\n",
        ));
        assert_eq!(
            (e.line, e.message.contains("unknown node `missing`")),
            (Some(7), true)
        );
        let e = err(&conv(
            "entry = n\n",
            &format!("{node}[node orphan]\ntext = t\nchoice = Bye -> end\n"),
        ));
        assert!(e.message.contains("can never be reached"));
        let trap =
            "[node n]\ntext = t\nchoice = Next -> m\n[node m]\ntext = t\nchoice = Back -> n\n";
        let e = err(&conv("entry = n\n", trap));
        assert!(e.message.contains("trap"));
        // A loop is fine as long as one always-available choice can leave.
        let ok_loop = "[node n]\ntext = t\nchoice = Next -> m\nchoice = Bye -> end\n[node m]\ntext = t\nchoice = Back -> n\n";
        assert!(DialogueBook::parse(&conv("entry = n\n", ok_loop)).is_ok());

        let two = format!("{}{}", conv("entry = n\n", node), conv("entry = n\n", node));
        assert!(err(&two).message.contains("defined twice"));
        let two_npcs = format!(
            "{}{}",
            conv("entry = n\n", node),
            conv("entry = n\n", node).replace("conversation a", "conversation b")
        );
        assert!(err(&two_npcs)
            .message
            .contains("already has a conversation"));
    }

    #[test]
    fn check_against_finds_unknown_quests_everywhere() {
        let quests = QuestBook::parse(QUESTS).unwrap();
        let base = |entry: &str, node_extra: &str, choice: &str| {
            format!("[conversation a]\nnpc = a\nspeaker = A\nentry = n if {entry}\nentry = n\n[node n]\ntext = t\n{node_extra}choice = Bye -> end{choice}\nchoice = Out -> end\n")
        };
        assert!(DialogueBook::parse(&base("has timber 1", "", ""))
            .unwrap()
            .check_against(&quests)
            .is_ok());
        let e = DialogueBook::parse(&base("can_complete ghost", "", ""))
            .unwrap()
            .check_against(&quests)
            .unwrap_err();
        assert!(e.message.contains("unknown quest `ghost`") && e.line.is_some());
        let e = DialogueBook::parse(&base("has timber 1", "effect = start ghost\n", ""))
            .unwrap()
            .check_against(&quests)
            .unwrap_err();
        assert!(e.message.contains("ghost"));
        let e = DialogueBook::parse(&base("has timber 1", "", " if quest ghost is active"))
            .unwrap()
            .check_against(&quests)
            .unwrap_err();
        assert!(e.message.contains("ghost"));
    }

    fn talk(
        d: &DialogueBook,
        q: &QuestBook,
        log: &mut QuestLog,
        bag: &mut Inventory,
    ) -> (DialogueSession, Outcome) {
        let mut ctx = QuestContext {
            quests: q,
            log,
            inventory: bag,
        };
        DialogueSession::begin(d, "guide", &mut ctx).unwrap()
    }

    fn view(
        s: &DialogueSession,
        d: &DialogueBook,
        q: &QuestBook,
        log: &mut QuestLog,
        bag: &mut Inventory,
    ) -> DialogueView {
        let ctx = QuestContext {
            quests: q,
            log,
            inventory: bag,
        };
        s.view(d, &ctx).unwrap()
    }

    #[test]
    fn a_short_quest_runs_start_to_finish_through_dialogue() {
        let (q, d) = books();
        let mut log = QuestLog::new();
        let mut bag = Inventory::new(4);

        // First visit: the offer. Accepting starts the quest.
        let (mut s, out) = talk(&d, &q, &mut log, &mut bag);
        assert!(out.updates.is_empty() && out.failures.is_empty());
        let v = view(&s, &d, &q, &mut log, &mut bag);
        assert_eq!(
            (v.speaker.as_str(), v.text.as_str()),
            ("Guide", "Hello.\nWant work?")
        );
        // "Sequel please" is hidden while herbs is not completed.
        assert_eq!(v.choices, vec!["Yes", "No"]);
        let out = s
            .choose(
                &d,
                0,
                &mut QuestContext {
                    quests: &q,
                    log: &mut log,
                    inventory: &mut bag,
                },
            )
            .unwrap();
        assert_eq!(
            out.updates,
            vec![QuestUpdate::Started {
                quest: "herbs".into()
            }]
        );
        assert_eq!(log.status("herbs"), QuestStatus::Active);
        let out = s
            .choose(
                &d,
                0,
                &mut QuestContext {
                    quests: &q,
                    log: &mut log,
                    inventory: &mut bag,
                },
            )
            .unwrap();
        assert!(out.updates.is_empty());
        assert!(s.is_finished());
        assert_eq!(
            s.choose(
                &d,
                0,
                &mut QuestContext {
                    quests: &q,
                    log: &mut log,
                    inventory: &mut bag
                }
            ),
            Err(DialogueError::Ended)
        );

        // Second visit: talking counts toward the `talk guide` objective, and the entry
        // is chosen after that, so the guide says the "busy" line.
        let (s, out) = talk(&d, &q, &mut log, &mut bag);
        assert_eq!(
            out.updates,
            vec![QuestUpdate::ObjectiveDone {
                quest: "herbs".into(),
                objective: 0
            }]
        );
        assert_eq!(view(&s, &d, &q, &mut log, &mut bag).text, "Keep going.");

        // Gather a petal; the quest becomes ready.
        bag.add(ItemId::Moonpetal, 1).unwrap();
        let ups = log.record(
            &q,
            &QuestEvent::Gathered {
                item: ItemId::Moonpetal,
                quantity: 1,
            },
        );
        assert!(ups.contains(&QuestUpdate::ReadyToTurnIn {
            quest: "herbs".into()
        }));

        // Third visit: hand-in. Items are taken, the reward arrives.
        let (mut s, out) = talk(&d, &q, &mut log, &mut bag);
        assert_eq!(
            out.updates,
            vec![QuestUpdate::Completed {
                quest: "herbs".into(),
                rewards: vec![ItemAmount {
                    item: ItemId::Timber,
                    quantity: 1
                }]
            }]
        );
        assert_eq!(
            (bag.count(ItemId::Moonpetal), bag.count(ItemId::Timber)),
            (0, 1)
        );
        assert_eq!(view(&s, &d, &q, &mut log, &mut bag).text, "Thanks!");
        s.close();
        assert!(
            s.is_finished()
                && s.view(
                    &d,
                    &QuestContext {
                        quests: &q,
                        log: &mut log,
                        inventory: &mut bag
                    }
                )
                .is_none()
        );

        // Fourth visit: finished-quest line, and the sequel (which needed `herbs`) can start.
        let (mut s, _) = talk(&d, &q, &mut log, &mut bag);
        assert_eq!(view(&s, &d, &q, &mut log, &mut bag).text, "All done.");
        let out = s
            .choose(
                &d,
                0,
                &mut QuestContext {
                    quests: &q,
                    log: &mut log,
                    inventory: &mut bag,
                },
            )
            .unwrap();
        assert_eq!(
            out.updates,
            vec![QuestUpdate::Started {
                quest: "sequel".into()
            }]
        );
    }

    #[test]
    fn a_full_bag_routes_to_the_helpful_node_and_changes_nothing() {
        let (q, d) = books();
        let mut log = QuestLog::new();
        let mut bag = Inventory::new(1);
        log.start(&q, "herbs").unwrap();
        log.record(
            &q,
            &QuestEvent::SpokeTo {
                npc: "guide".into(),
            },
        );
        log.record(
            &q,
            &QuestEvent::Gathered {
                item: ItemId::Moonpetal,
                quantity: 1,
            },
        );
        assert_eq!(log.status("herbs"), QuestStatus::ReadyToTurnIn);
        // Petals were never put in the bag (e.g. dropped): ready, but cannot be handed in.
        bag.add(ItemId::CopperOre, 1).unwrap();
        let before = (log.clone(), bag.clone());
        let (s, out) = talk(&d, &q, &mut log, &mut bag);
        assert!(out.failures.is_empty());
        assert_eq!(view(&s, &d, &q, &mut log, &mut bag).text, "No room.");
        assert_eq!((log, bag), before);
    }

    #[test]
    fn effects_that_cannot_apply_are_reported_not_hidden() {
        let q = QuestBook::parse("[quest r]\ntitle = R\nobjective = talk x\nreward = timber 1\n[quest s]\ntitle = S\nrequires = r\nobjective = talk x\n").unwrap();
        let text = "[conversation c]\nnpc = c\nspeaker = C\nentry = n\n[node n]\neffect = start s\neffect = complete r\ntext = t\nchoice = Bye -> end\n";
        let d = DialogueBook::parse(text).unwrap();
        d.check_against(&q).unwrap();
        let mut log = QuestLog::new();
        let mut bag = Inventory::default();
        let (_, out) = DialogueSession::begin(
            &d,
            "c",
            &mut QuestContext {
                quests: &q,
                log: &mut log,
                inventory: &mut bag,
            },
        )
        .unwrap();
        assert_eq!(
            out.failures,
            vec![
                EffectFailure::Start {
                    quest: "s".into(),
                    error: StartError::PrerequisiteNotMet("r".into())
                },
                EffectFailure::Complete {
                    quest: "r".into(),
                    error: CompleteError::NotReady(QuestStatus::NotStarted)
                },
            ]
        );
        assert!(out.updates.is_empty() && log.started().next().is_none());
    }

    #[test]
    fn choice_indexes_follow_the_offered_list_and_bad_indexes_are_rejected() {
        let (q, d) = books();
        let mut log = QuestLog::new();
        let mut bag = Inventory::new(4);
        let (mut s, _) = talk(&d, &q, &mut log, &mut bag);
        let mut ctx = QuestContext {
            quests: &q,
            log: &mut log,
            inventory: &mut bag,
        };
        assert_eq!(
            s.choose(&d, 2, &mut ctx),
            Err(DialogueError::NoSuchChoice(2))
        );
        assert!(!s.is_finished(), "a bad index changes nothing");
        // Index 1 is "No" (the hidden sequel choice does not take up an index).
        s.choose(&d, 1, &mut ctx).unwrap();
        assert!(s.is_finished());
        assert!(matches!(
            DialogueSession::begin(&d, "nobody", &mut ctx),
            Err(DialogueError::UnknownConversation(_))
        ));
    }

    #[test]
    fn conditional_choices_appear_when_their_condition_starts_to_hold() {
        let (q, d) = books();
        let mut log = QuestLog::new();
        let mut bag = Inventory::new(4);
        log.start(&q, "herbs").unwrap();
        let (s, _) = talk(&d, &q, &mut log, &mut bag);
        assert_eq!(view(&s, &d, &q, &mut log, &mut bag).choices, vec!["Bye"]);
        bag.add(ItemId::Moonpetal, 1).unwrap();
        assert_eq!(
            view(&s, &d, &q, &mut log, &mut bag).choices,
            vec!["Bye", "Ask again"]
        );
    }
}
