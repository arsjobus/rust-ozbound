//! Quest definitions, prerequisites, objectives, rewards, and the player's quest log.
//!
//! Pure Rust (no Bevy). Definitions are validated content (`QuestBook`, usually parsed from
//! `assets/quests/*.quest`); the `QuestLog` is the player's state and is changed only through
//! methods that either do everything or change nothing.
//!
//! Rules, in one place:
//! - A quest can be started once, and only after every prerequisite is `Completed`.
//! - Objectives are done in order; only the current one advances.
//! - `Collect` counts items *gathered* (`QuestEvent::Gathered`) after the objective became
//!   current. Surplus from one event is not carried over, and quest rewards never count as
//!   gathering. Items already in the bag do not count.
//! - `Craft` counts items *crafted* (`QuestEvent::Crafted`) the same way: only crafts made
//!   while the objective is current count, and items already in the bag do not.
//! - `Visit` is done when the player reaches a named place (`QuestEvent::Visited`); places
//!   are listed in the level file. Walking there earlier does not count.
//! - `Befriend` is done by winning over a named creature (`QuestEvent::Befriended`); see
//!   `bestiary.rs`. Encounters are optional and never harm anything.
//! - When the last objective is done the quest is `ReadyToTurnIn`. `complete` then takes the
//!   `takes` items, grants the `reward` items, and marks it `Completed`, all or nothing: if the
//!   bag lacks the items or has no room for the rewards, nothing changes and the quest stays
//!   ready, so the player can make room and try again.

use std::collections::{BTreeMap, BTreeSet};

use crate::inventory::{Inventory, ItemId};
use crate::textdata::{check_id, parse_positive, read_lines, ContentError, Item};

/// The quest file compiled into the game, identical to `assets/quests/meadow.quest`.
pub const DEFAULT_QUESTS: &str = include_str!("../assets/quests/meadow.quest");

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ItemAmount {
    pub item: ItemId,
    pub quantity: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Objective {
    /// Speak to an NPC (see `QuestEvent::SpokeTo`).
    TalkTo { npc: String },
    /// Gather `count` of an item.
    Collect { item: ItemId, count: u32 },
    /// Craft `count` of an item (see `QuestEvent::Crafted`).
    Craft { item: ItemId, count: u32 },
    /// Reach a named place from the level's `[places]` section.
    Visit { place: String },
    /// Win the trust of a named creature from the bestiary.
    Befriend { creature: String },
}

impl Objective {
    /// How many steps the objective needs: `count` for collecting and crafting, 1 otherwise.
    pub fn needed(&self) -> u32 {
        match self {
            Self::TalkTo { .. } | Self::Visit { .. } | Self::Befriend { .. } => 1,
            Self::Collect { count, .. } | Self::Craft { count, .. } => *count,
        }
    }

    /// Journal wording, e.g. "Collect moonpetal" or "Talk to village guide".
    pub fn describe(&self) -> String {
        match self {
            Self::TalkTo { npc } => format!("Talk to {}", npc.replace('_', " ")),
            Self::Collect { item, .. } => format!("Collect {}", item.label()),
            Self::Craft { item, .. } => format!("Craft {}", item.label()),
            Self::Visit { place } => format!("Visit {}", place.replace('_', " ")),
            Self::Befriend { creature } => format!("Befriend {}", creature.replace('_', " ")),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QuestDef {
    pub id: String,
    pub title: String,
    pub summary: String,
    pub prerequisites: Vec<String>,
    pub objectives: Vec<Objective>,
    /// Handed over when the quest is completed.
    pub takes: Vec<ItemAmount>,
    /// Given when the quest is completed.
    pub rewards: Vec<ItemAmount>,
    /// Line of the `[quest ...]` header when parsed from a file; 0 when built in code.
    pub line: usize,
}

/// All quest definitions, validated: unique ids, known prerequisites without cycles, at least
/// one objective, positive amounts, and no item listed twice in one `takes`/`reward` list.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct QuestBook {
    quests: Vec<QuestDef>,
}

impl QuestBook {
    pub fn new(quests: Vec<QuestDef>) -> Result<Self, ContentError> {
        validate(&quests)?;
        Ok(Self { quests })
    }

    pub fn get(&self, id: &str) -> Option<&QuestDef> {
        self.quests.iter().find(|q| q.id == id)
    }

    pub fn iter(&self) -> impl Iterator<Item = &QuestDef> {
        self.quests.iter()
    }

    pub fn len(&self) -> usize {
        self.quests.len()
    }

    pub fn is_empty(&self) -> bool {
        self.quests.is_empty()
    }

    /// Parses and validates a `.quest` file (format documented in `assets/quests/meadow.quest`).
    pub fn parse(text: &str) -> Result<Self, ContentError> {
        let mut quests: Vec<QuestDef> = Vec::new();
        let mut seen_title: Vec<bool> = Vec::new();
        let mut seen_summary: Vec<bool> = Vec::new();
        for line in read_lines(text)? {
            let n = line.number;
            match line.item {
                Item::Section { kind, arg } => {
                    if kind != "quest" {
                        return Err(ContentError::at(
                            n,
                            format!("unknown section `[{kind}]` (expected `[quest <id>]`)"),
                        ));
                    }
                    check_id(n, "quest id", &arg)?;
                    quests.push(QuestDef {
                        id: arg,
                        title: String::new(),
                        summary: String::new(),
                        prerequisites: Vec::new(),
                        objectives: Vec::new(),
                        takes: Vec::new(),
                        rewards: Vec::new(),
                        line: n,
                    });
                    seen_title.push(false);
                    seen_summary.push(false);
                }
                Item::Field { key, value } => {
                    let Some(quest) = quests.last_mut() else {
                        return Err(ContentError::at(
                            n,
                            format!("`{key}` must be inside a `[quest <id>]` section"),
                        ));
                    };
                    let last = seen_title.len() - 1;
                    match key.as_str() {
                        "title" => {
                            if std::mem::replace(&mut seen_title[last], true) {
                                return Err(ContentError::at(n, "`title` is given twice"));
                            }
                            quest.title = value;
                        }
                        "summary" => {
                            if std::mem::replace(&mut seen_summary[last], true) {
                                return Err(ContentError::at(n, "`summary` is given twice"));
                            }
                            quest.summary = value;
                        }
                        "requires" => {
                            for id in value.split(',').map(str::trim) {
                                check_id(n, "prerequisite quest id", id)?;
                                quest.prerequisites.push(id.to_string());
                            }
                        }
                        "objective" => quest.objectives.push(parse_objective(n, &value)?),
                        "takes" => quest.takes.push(parse_amount(n, &value)?),
                        "reward" => quest.rewards.push(parse_amount(n, &value)?),
                        other => {
                            return Err(ContentError::at(
                                n,
                                format!("unknown quest field `{other}`"),
                            ))
                        }
                    }
                }
            }
        }
        Self::new(quests)
    }
}

fn parse_objective(line: usize, value: &str) -> Result<Objective, ContentError> {
    let words: Vec<&str> = value.split_whitespace().collect();
    match words.as_slice() {
        ["talk", npc] => {
            check_id(line, "npc id", npc)?;
            Ok(Objective::TalkTo {
                npc: (*npc).to_string(),
            })
        }
        ["collect", item, count] => Ok(Objective::Collect {
            item: parse_item(line, item)?,
            count: parse_positive(line, "the collect amount", count)?,
        }),
        ["craft", item, count] => Ok(Objective::Craft {
            item: parse_item(line, item)?,
            count: parse_positive(line, "the craft amount", count)?,
        }),
        ["visit", place] => {
            check_id(line, "place id", place)?;
            Ok(Objective::Visit {
                place: (*place).to_string(),
            })
        }
        ["befriend", creature] => {
            check_id(line, "creature id", creature)?;
            Ok(Objective::Befriend {
                creature: (*creature).to_string(),
            })
        }
        _ => Err(ContentError::at(
            line,
            format!(
                "objective `{value}` must be `talk <npc>`, `collect <item> <amount>`, `craft <item> <amount>`, `visit <place>` or `befriend <creature>`"
            ),
        )),
    }
}

fn parse_item(line: usize, key: &str) -> Result<ItemId, ContentError> {
    ItemId::from_key(key).ok_or_else(|| ContentError::at(line, format!("unknown item `{key}`")))
}

fn parse_amount(line: usize, value: &str) -> Result<ItemAmount, ContentError> {
    let words: Vec<&str> = value.split_whitespace().collect();
    match words.as_slice() {
        [item, quantity] => Ok(ItemAmount {
            item: parse_item(line, item)?,
            quantity: parse_positive(line, "the item amount", quantity)?,
        }),
        _ => Err(ContentError::at(
            line,
            format!("`{value}` must be `<item> <amount>`"),
        )),
    }
}

fn validate(quests: &[QuestDef]) -> Result<(), ContentError> {
    let mut ids: BTreeSet<&str> = BTreeSet::new();
    for q in quests {
        if !ids.insert(q.id.as_str()) {
            return Err(ContentError::at(
                q.line,
                format!("quest `{}` is defined twice", q.id),
            ));
        }
    }
    for q in quests {
        let here = |msg: String| ContentError::at(q.line, format!("quest `{}`: {msg}", q.id));
        if q.title.trim().is_empty() {
            return Err(here("needs a `title`".into()));
        }
        if q.objectives.is_empty() {
            return Err(here("needs at least one `objective`".into()));
        }
        for o in &q.objectives {
            if o.needed() == 0 {
                return Err(here("has an objective with a zero amount".into()));
            }
        }
        for (what, list) in [("takes", &q.takes), ("reward", &q.rewards)] {
            let mut seen = BTreeSet::new();
            for a in list {
                if a.quantity == 0 {
                    return Err(here(format!("`{what}` has a zero amount")));
                }
                if !seen.insert(a.item) {
                    return Err(here(format!(
                        "`{what}` lists {} twice; combine them into one amount",
                        a.item.key()
                    )));
                }
            }
        }
        for p in &q.prerequisites {
            if p == &q.id {
                return Err(here("cannot require itself".into()));
            }
            if !ids.contains(p.as_str()) {
                return Err(here(format!("requires unknown quest `{p}`")));
            }
        }
    }
    // Prerequisite cycles would make quests that can never start.
    let mut state: BTreeMap<&str, bool> = BTreeMap::new(); // false = visiting, true = done
    for q in quests {
        visit(q, quests, &mut state)?;
    }
    Ok(())
}

fn visit<'a>(
    q: &'a QuestDef,
    all: &'a [QuestDef],
    state: &mut BTreeMap<&'a str, bool>,
) -> Result<(), ContentError> {
    match state.get(q.id.as_str()) {
        Some(true) => return Ok(()),
        Some(false) => {
            return Err(ContentError::at(
                q.line,
                format!("quest `{}` is part of a prerequisite cycle", q.id),
            ));
        }
        None => {}
    }
    state.insert(q.id.as_str(), false);
    for p in &q.prerequisites {
        if let Some(next) = all.iter().find(|x| &x.id == p) {
            visit(next, all, state)?;
        }
    }
    state.insert(q.id.as_str(), true);
    Ok(())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QuestStatus {
    NotStarted,
    Active,
    /// All objectives are done; the quest is waiting to be handed in.
    ReadyToTurnIn,
    Completed,
}

impl QuestStatus {
    /// Stable key used in dialogue conditions and saves.
    pub fn key(self) -> &'static str {
        match self {
            Self::NotStarted => "not_started",
            Self::Active => "active",
            Self::ReadyToTurnIn => "ready",
            Self::Completed => "completed",
        }
    }

    pub fn from_key(key: &str) -> Option<Self> {
        [
            Self::NotStarted,
            Self::Active,
            Self::ReadyToTurnIn,
            Self::Completed,
        ]
        .into_iter()
        .find(|s| s.key() == key)
    }
}

/// Something that happened in the world that quests may care about.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum QuestEvent {
    Gathered { item: ItemId, quantity: u32 },
    SpokeTo { npc: String },
    /// Something was made at the crafting panel.
    Crafted { item: ItemId, quantity: u32 },
    /// The player reached a named place.
    Visited { place: String },
    /// A creature accepted the player's offering.
    Befriended { creature: String },
}

/// What a state change means for the player; the UI turns these into journal messages.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum QuestUpdate {
    Started {
        quest: String,
    },
    /// A counted objective moved forward but is not finished.
    Progress {
        quest: String,
        objective: usize,
        count: u32,
        needed: u32,
    },
    ObjectiveDone {
        quest: String,
        objective: usize,
    },
    ReadyToTurnIn {
        quest: String,
    },
    Completed {
        quest: String,
        rewards: Vec<ItemAmount>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StartError {
    UnknownQuest(String),
    /// Already active, ready, or completed.
    AlreadyStarted(QuestStatus),
    /// The named prerequisite is not completed yet.
    PrerequisiteNotMet(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CompleteError {
    UnknownQuest(String),
    NotReady(QuestStatus),
    /// The bag does not hold the items the quest takes.
    MissingItems(ItemId),
    /// The rewards do not fit in the bag.
    InventoryFull,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Stage {
    Active { objective: usize, count: u32 },
    Ready,
    Completed,
}

/// The player's quests, in the order they were started.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct QuestLog {
    pub(crate) entries: Vec<(String, Stage)>,
}

impl QuestLog {
    pub fn new() -> Self {
        Self::default()
    }

    pub(crate) fn stage(&self, id: &str) -> Option<Stage> {
        self.entries.iter().find(|(q, _)| q == id).map(|(_, s)| *s)
    }

    fn stage_mut(&mut self, id: &str) -> Option<&mut Stage> {
        self.entries
            .iter_mut()
            .find(|(q, _)| q == id)
            .map(|(_, s)| s)
    }

    pub fn status(&self, id: &str) -> QuestStatus {
        match self.stage(id) {
            None => QuestStatus::NotStarted,
            Some(Stage::Active { .. }) => QuestStatus::Active,
            Some(Stage::Ready) => QuestStatus::ReadyToTurnIn,
            Some(Stage::Completed) => QuestStatus::Completed,
        }
    }

    /// Index and progress count of the objective currently being worked on, if the quest is active.
    pub fn current_objective(&self, id: &str) -> Option<(usize, u32)> {
        match self.stage(id) {
            Some(Stage::Active { objective, count }) => Some((objective, count)),
            _ => None,
        }
    }

    /// Ids of quests that were started, in start order.
    pub fn started(&self) -> impl Iterator<Item = &str> {
        self.entries.iter().map(|(id, _)| id.as_str())
    }

    pub fn can_start(&self, book: &QuestBook, id: &str) -> Result<(), StartError> {
        let def = book
            .get(id)
            .ok_or_else(|| StartError::UnknownQuest(id.to_string()))?;
        let status = self.status(id);
        if status != QuestStatus::NotStarted {
            return Err(StartError::AlreadyStarted(status));
        }
        match def
            .prerequisites
            .iter()
            .find(|p| self.status(p) != QuestStatus::Completed)
        {
            Some(missing) => Err(StartError::PrerequisiteNotMet(missing.clone())),
            None => Ok(()),
        }
    }

    pub fn start(&mut self, book: &QuestBook, id: &str) -> Result<(), StartError> {
        self.can_start(book, id)?;
        self.entries.push((
            id.to_string(),
            Stage::Active {
                objective: 0,
                count: 0,
            },
        ));
        Ok(())
    }

    /// Feeds a world event to every active quest and reports what changed.
    pub fn record(&mut self, book: &QuestBook, event: &QuestEvent) -> Vec<QuestUpdate> {
        let mut updates = Vec::new();
        if matches!(
            event,
            QuestEvent::Gathered { quantity: 0, .. } | QuestEvent::Crafted { quantity: 0, .. }
        ) {
            return updates;
        }
        for (id, stage) in &mut self.entries {
            let Stage::Active { objective, count } = *stage else {
                continue;
            };
            let Some(def) = book.get(id) else { continue };
            let Some(current) = def.objectives.get(objective) else {
                continue;
            };
            let gained = match (current, event) {
                (Objective::TalkTo { npc }, QuestEvent::SpokeTo { npc: spoke }) if npc == spoke => {
                    1
                }
                (
                    Objective::Collect { item, .. },
                    QuestEvent::Gathered {
                        item: got,
                        quantity,
                    },
                ) if item == got => *quantity,
                (
                    Objective::Craft { item, .. },
                    QuestEvent::Crafted {
                        item: made,
                        quantity,
                    },
                ) if item == made => *quantity,
                (Objective::Visit { place }, QuestEvent::Visited { place: reached })
                    if place == reached =>
                {
                    1
                }
                (
                    Objective::Befriend { creature },
                    QuestEvent::Befriended { creature: won },
                ) if creature == won => 1,
                _ => continue,
            };
            let needed = current.needed();
            let count = count.saturating_add(gained).min(needed);
            if count < needed {
                *stage = Stage::Active { objective, count };
                updates.push(QuestUpdate::Progress {
                    quest: id.clone(),
                    objective,
                    count,
                    needed,
                });
                continue;
            }
            updates.push(QuestUpdate::ObjectiveDone {
                quest: id.clone(),
                objective,
            });
            if objective + 1 < def.objectives.len() {
                *stage = Stage::Active {
                    objective: objective + 1,
                    count: 0,
                };
            } else {
                *stage = Stage::Ready;
                updates.push(QuestUpdate::ReadyToTurnIn { quest: id.clone() });
            }
        }
        updates
    }

    /// Hands in a ready quest: takes the quest's items, grants its rewards, and marks it completed.
    /// All or nothing: on any error neither the log nor the bag changes.
    pub fn complete(
        &mut self,
        book: &QuestBook,
        id: &str,
        inventory: &mut Inventory,
    ) -> Result<Vec<ItemAmount>, CompleteError> {
        let def = book
            .get(id)
            .ok_or_else(|| CompleteError::UnknownQuest(id.to_string()))?;
        let status = self.status(id);
        if status != QuestStatus::ReadyToTurnIn {
            return Err(CompleteError::NotReady(status));
        }
        let mut trial = inventory.clone();
        for take in &def.takes {
            trial
                .remove(take.item, take.quantity)
                .map_err(|_| CompleteError::MissingItems(take.item))?;
        }
        for reward in &def.rewards {
            trial
                .add(reward.item, reward.quantity)
                .map_err(|_| CompleteError::InventoryFull)?;
        }
        *inventory = trial;
        if let Some(stage) = self.stage_mut(id) {
            *stage = Stage::Completed;
        }
        Ok(def.rewards.clone())
    }

    /// Whether `complete` would succeed right now (changes nothing).
    pub fn can_complete(&self, book: &QuestBook, id: &str, inventory: &Inventory) -> bool {
        self.clone()
            .complete(book, id, &mut inventory.clone())
            .is_ok()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const CHAIN: &str = "\
# a three-quest chain
[quest herbs]
title = Herbs
summary = Gather herbs.
objective = talk guide
objective = collect moonpetal 2
takes = moonpetal 2
reward = timber 1

[quest planks]
title = Planks
requires = herbs
objective = collect timber 3
reward = copper_ore 1

[quest finale]
title = Finale
requires = herbs, planks
objective = talk guide
";

    fn book() -> QuestBook {
        QuestBook::parse(CHAIN).unwrap()
    }

    fn gathered(item: ItemId, quantity: u32) -> QuestEvent {
        QuestEvent::Gathered { item, quantity }
    }

    fn spoke(npc: &str) -> QuestEvent {
        QuestEvent::SpokeTo { npc: npc.into() }
    }

    #[test]
    fn parses_every_field() {
        let book = book();
        assert_eq!(book.len(), 3);
        let herbs = book.get("herbs").unwrap();
        assert_eq!(herbs.title, "Herbs");
        assert_eq!(herbs.summary, "Gather herbs.");
        assert_eq!(
            herbs.objectives[0],
            Objective::TalkTo {
                npc: "guide".into()
            }
        );
        assert_eq!(
            herbs.objectives[1],
            Objective::Collect {
                item: ItemId::Moonpetal,
                count: 2
            }
        );
        assert_eq!(
            herbs.takes,
            vec![ItemAmount {
                item: ItemId::Moonpetal,
                quantity: 2
            }]
        );
        assert_eq!(
            herbs.rewards,
            vec![ItemAmount {
                item: ItemId::Timber,
                quantity: 1
            }]
        );
        assert_eq!(herbs.line, 2);
        assert_eq!(
            book.get("finale").unwrap().prerequisites,
            vec!["herbs", "planks"]
        );
        assert!(book.get("nope").is_none());
    }

    #[test]
    fn shipped_quests_are_valid() {
        let book = QuestBook::parse(DEFAULT_QUESTS).unwrap();
        let q = book.get("moonpetals").unwrap();
        assert_eq!(
            q.objectives,
            vec![Objective::Collect {
                item: ItemId::Moonpetal,
                count: 3
            }]
        );
        let disk = std::fs::read_to_string(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/assets/quests/meadow.quest"
        ))
        .unwrap();
        assert_eq!(disk, DEFAULT_QUESTS);
    }

    fn parse_err(text: &str) -> ContentError {
        QuestBook::parse(text).unwrap_err()
    }

    #[test]
    fn invalid_content_reports_the_line_and_the_problem() {
        let base = "[quest a]\ntitle = A\nobjective = talk x\n";
        assert!(QuestBook::parse(base).is_ok());

        let e = parse_err("title = x\n");
        assert_eq!(
            (e.line, e.message.contains("inside a `[quest")),
            (Some(1), true)
        );
        let e = parse_err("[item a]\n");
        assert!(e.message.contains("unknown section"));
        let e = parse_err("[quest Bad Id]\n");
        assert!(e.message.contains("quest id"));
        let e = parse_err(&format!("{base}colour = red\n"));
        assert_eq!(
            (e.line, e.message.contains("unknown quest field")),
            (Some(4), true)
        );
        let e = parse_err(&format!("{base}title = again\n"));
        assert_eq!((e.line, e.message.contains("twice")), (Some(4), true));
        let e = parse_err(&format!("{base}summary = s\nsummary = t\n"));
        assert_eq!(e.line, Some(5));
        let e = parse_err(&format!("{base}objective = dance\n"));
        assert!(e.message.contains("talk <npc>"));
        let e = parse_err(&format!("{base}objective = collect moonpetal 0\n"));
        assert!(e.message.contains("above zero"));
        let e = parse_err(&format!("{base}objective = collect gold 2\n"));
        assert!(e.message.contains("unknown item `gold`"));
        let e = parse_err(&format!("{base}reward = timber\n"));
        assert!(e.message.contains("<item> <amount>"));
        let e = parse_err(&format!("{base}reward = timber 1\nreward = timber 2\n"));
        assert!(e.message.contains("twice"));
        let e = parse_err(&format!("{base}requires = a, \n"));
        assert!(e.message.contains("prerequisite quest id"));
    }

    #[test]
    fn invalid_definitions_are_rejected() {
        let e = parse_err("[quest a]\nobjective = talk x\n");
        assert!(e.message.contains("needs a `title`"));
        let e = parse_err("[quest a]\ntitle = A\n");
        assert_eq!(
            (e.line, e.message.contains("at least one `objective`")),
            (Some(1), true)
        );
        let e = parse_err(
            "[quest a]\ntitle = A\nobjective = talk x\n[quest a]\ntitle = B\nobjective = talk y\n",
        );
        assert_eq!(
            (e.line, e.message.contains("defined twice")),
            (Some(4), true)
        );
        let e = parse_err("[quest a]\ntitle = A\nobjective = talk x\nrequires = ghost\n");
        assert!(e.message.contains("unknown quest `ghost`"));
        let e = parse_err("[quest a]\ntitle = A\nobjective = talk x\nrequires = a\n");
        assert!(e.message.contains("cannot require itself"));
        let cycle = "[quest a]\ntitle = A\nobjective = talk x\nrequires = b\n[quest b]\ntitle = B\nobjective = talk x\nrequires = a\n";
        assert!(parse_err(cycle).message.contains("cycle"));
        let long_cycle = "[quest a]\ntitle = A\nobjective = talk x\nrequires = c\n[quest b]\ntitle = B\nobjective = talk x\nrequires = a\n[quest c]\ntitle = C\nobjective = talk x\nrequires = b\n";
        assert!(parse_err(long_cycle).message.contains("cycle"));
    }

    #[test]
    fn built_in_code_content_is_validated_too() {
        let def = |id: &str| QuestDef {
            id: id.into(),
            title: "T".into(),
            summary: String::new(),
            prerequisites: vec![],
            objectives: vec![Objective::Collect {
                item: ItemId::Timber,
                count: 0,
            }],
            takes: vec![],
            rewards: vec![],
            line: 0,
        };
        let e = QuestBook::new(vec![def("a")]).unwrap_err();
        assert_eq!(e.line, None);
        assert!(e.message.contains("zero amount"));
        let mut zero_reward = def("b");
        zero_reward.objectives = vec![Objective::TalkTo { npc: "x".into() }];
        zero_reward.rewards = vec![ItemAmount {
            item: ItemId::Timber,
            quantity: 0,
        }];
        assert!(QuestBook::new(vec![zero_reward])
            .unwrap_err()
            .message
            .contains("zero amount"));
        assert!(QuestBook::new(vec![]).unwrap().is_empty());
    }

    #[test]
    fn starting_requires_prerequisites_and_happens_once() {
        let book = book();
        let mut log = QuestLog::new();
        assert_eq!(log.status("herbs"), QuestStatus::NotStarted);
        assert_eq!(
            log.start(&book, "nope"),
            Err(StartError::UnknownQuest("nope".into()))
        );
        assert_eq!(
            log.start(&book, "planks"),
            Err(StartError::PrerequisiteNotMet("herbs".into()))
        );
        assert_eq!(log.status("planks"), QuestStatus::NotStarted);
        log.start(&book, "herbs").unwrap();
        assert_eq!(log.status("herbs"), QuestStatus::Active);
        assert_eq!(
            log.start(&book, "herbs"),
            Err(StartError::AlreadyStarted(QuestStatus::Active))
        );
        // An active (not completed) prerequisite still blocks.
        assert_eq!(
            log.can_start(&book, "planks"),
            Err(StartError::PrerequisiteNotMet("herbs".into()))
        );
        assert_eq!(log.started().collect::<Vec<_>>(), vec!["herbs"]);
    }

    #[test]
    fn objectives_advance_in_order_and_ignore_unrelated_events() {
        let book = book();
        let mut log = QuestLog::new();
        log.start(&book, "herbs").unwrap();
        // Gathering before the talk objective is current counts for nothing.
        assert!(log
            .record(&book, &gathered(ItemId::Moonpetal, 2))
            .is_empty());
        assert!(log.record(&book, &spoke("someone_else")).is_empty());
        assert_eq!(log.current_objective("herbs"), Some((0, 0)));

        let up = log.record(&book, &spoke("guide"));
        assert_eq!(
            up,
            vec![QuestUpdate::ObjectiveDone {
                quest: "herbs".into(),
                objective: 0
            }]
        );
        assert_eq!(log.current_objective("herbs"), Some((1, 0)));

        assert!(log.record(&book, &gathered(ItemId::Timber, 5)).is_empty());
        assert!(log
            .record(&book, &gathered(ItemId::Moonpetal, 0))
            .is_empty());
        let up = log.record(&book, &gathered(ItemId::Moonpetal, 1));
        assert_eq!(
            up,
            vec![QuestUpdate::Progress {
                quest: "herbs".into(),
                objective: 1,
                count: 1,
                needed: 2
            }]
        );
        let up = log.record(&book, &gathered(ItemId::Moonpetal, 1));
        assert_eq!(
            up,
            vec![
                QuestUpdate::ObjectiveDone {
                    quest: "herbs".into(),
                    objective: 1
                },
                QuestUpdate::ReadyToTurnIn {
                    quest: "herbs".into()
                },
            ]
        );
        assert_eq!(log.status("herbs"), QuestStatus::ReadyToTurnIn);
        // Nothing more to record once ready.
        assert!(log
            .record(&book, &gathered(ItemId::Moonpetal, 1))
            .is_empty());
    }

    #[test]
    fn surplus_is_capped_and_not_carried_over() {
        let book = book();
        let mut log = QuestLog::new();
        log.start(&book, "herbs").unwrap();
        log.record(&book, &spoke("guide"));
        log.record(&book, &gathered(ItemId::Moonpetal, u32::MAX));
        assert_eq!(log.status("herbs"), QuestStatus::ReadyToTurnIn);
    }

    #[test]
    fn one_event_can_advance_several_quests() {
        let text = "[quest a]\ntitle = A\nobjective = collect timber 1\n[quest b]\ntitle = B\nobjective = collect timber 2\n";
        let book = QuestBook::parse(text).unwrap();
        let mut log = QuestLog::new();
        log.start(&book, "a").unwrap();
        log.start(&book, "b").unwrap();
        let up = log.record(&book, &gathered(ItemId::Timber, 1));
        assert_eq!(log.status("a"), QuestStatus::ReadyToTurnIn);
        assert_eq!(log.current_objective("b"), Some((0, 1)));
        assert_eq!(up.len(), 3);
    }

    fn ready_herbs(book: &QuestBook) -> QuestLog {
        let mut log = QuestLog::new();
        log.start(book, "herbs").unwrap();
        log.record(book, &spoke("guide"));
        log.record(book, &gathered(ItemId::Moonpetal, 2));
        log
    }

    #[test]
    fn completing_takes_items_grants_rewards_and_unlocks_followups() {
        let book = book();
        let mut log = ready_herbs(&book);
        let mut bag = Inventory::new(4);
        bag.add(ItemId::Moonpetal, 2).unwrap();
        assert!(log.can_complete(&book, "herbs", &bag));
        let rewards = log.complete(&book, "herbs", &mut bag).unwrap();
        assert_eq!(
            rewards,
            vec![ItemAmount {
                item: ItemId::Timber,
                quantity: 1
            }]
        );
        assert_eq!(bag.count(ItemId::Moonpetal), 0);
        assert_eq!(bag.count(ItemId::Timber), 1);
        assert_eq!(log.status("herbs"), QuestStatus::Completed);
        assert_eq!(
            log.complete(&book, "herbs", &mut bag),
            Err(CompleteError::NotReady(QuestStatus::Completed))
        );
        assert_eq!(bag.count(ItemId::Timber), 1, "no second reward");
        log.start(&book, "planks").unwrap();
        assert_eq!(
            log.start(&book, "finale"),
            Err(StartError::PrerequisiteNotMet("planks".into()))
        );
    }

    #[test]
    fn failed_completion_changes_nothing() {
        let book = book();
        let mut log = ready_herbs(&book);
        let before = log.clone();

        // The bag does not hold the petals the quest takes.
        let mut empty = Inventory::new(4);
        assert_eq!(
            log.complete(&book, "herbs", &mut empty),
            Err(CompleteError::MissingItems(ItemId::Moonpetal))
        );
        assert!(!log.can_complete(&book, "herbs", &empty));
        assert_eq!(log, before);
        assert_eq!(
            log.complete(&book, "nope", &mut empty),
            Err(CompleteError::UnknownQuest("nope".into()))
        );

        // Handing over the petals frees their slot, so a one-slot bag still fits the reward.
        let mut tight = Inventory::new(1);
        tight.add(ItemId::Moonpetal, 2).unwrap();
        assert!(log.can_complete(&book, "herbs", &tight));
    }

    #[test]
    fn rewards_that_do_not_fit_leave_everything_unchanged_until_there_is_room() {
        let book =
            QuestBook::parse("[quest r]\ntitle = R\nobjective = talk x\nreward = timber 1\n")
                .unwrap();
        let mut log = QuestLog::new();
        log.start(&book, "r").unwrap();
        log.record(&book, &spoke("x"));

        let mut full = Inventory::new(1);
        full.add(ItemId::CopperOre, 1).unwrap();
        let before = (log.clone(), full.clone());
        assert_eq!(
            log.complete(&book, "r", &mut full),
            Err(CompleteError::InventoryFull)
        );
        assert_eq!((log.clone(), full.clone()), before);
        assert_eq!(log.status("r"), QuestStatus::ReadyToTurnIn);

        full.remove(ItemId::CopperOre, 1).unwrap();
        assert!(log.complete(&book, "r", &mut full).is_ok());
        assert_eq!(full.count(ItemId::Timber), 1);
    }

    #[test]
    fn cannot_complete_before_ready() {
        let book = book();
        let mut log = QuestLog::new();
        let mut bag = Inventory::default();
        assert_eq!(
            log.complete(&book, "herbs", &mut bag),
            Err(CompleteError::NotReady(QuestStatus::NotStarted))
        );
        log.start(&book, "herbs").unwrap();
        assert_eq!(
            log.complete(&book, "herbs", &mut bag),
            Err(CompleteError::NotReady(QuestStatus::Active))
        );
    }

    #[test]
    fn status_keys_round_trip() {
        for s in [
            QuestStatus::NotStarted,
            QuestStatus::Active,
            QuestStatus::ReadyToTurnIn,
            QuestStatus::Completed,
        ] {
            assert_eq!(QuestStatus::from_key(s.key()), Some(s));
        }
        assert_eq!(QuestStatus::from_key("done"), None);
    }

    #[test]
    fn objective_descriptions() {
        assert_eq!(
            Objective::TalkTo {
                npc: "village_guide".into()
            }
            .describe(),
            "Talk to village guide"
        );
        assert_eq!(
            Objective::Collect {
                item: ItemId::CopperOre,
                count: 2
            }
            .describe(),
            "Collect copper ore"
        );
        assert_eq!(
            Objective::Collect {
                item: ItemId::CopperOre,
                count: 2
            }
            .needed(),
            2
        );
    }
}

#[cfg(test)]
mod place_craft_befriend_tests {
    use super::*;

    const ERRANDS: &str = "\
[quest errands]
title = Errands
objective = craft walking_staff 2
objective = visit old_mill
objective = befriend meadow_hare
objective = talk miller
takes = walking_staff 2
reward = timber 1
";

    fn book() -> QuestBook {
        QuestBook::parse(ERRANDS).unwrap()
    }

    fn started() -> QuestLog {
        let mut log = QuestLog::new();
        log.start(&book(), "errands").unwrap();
        log
    }

    fn crafted(item: ItemId, quantity: u32) -> QuestEvent {
        QuestEvent::Crafted { item, quantity }
    }

    fn visited(place: &str) -> QuestEvent {
        QuestEvent::Visited {
            place: place.into(),
        }
    }

    fn befriended(creature: &str) -> QuestEvent {
        QuestEvent::Befriended {
            creature: creature.into(),
        }
    }

    #[test]
    fn parses_craft_visit_and_befriend_objectives() {
        let quest = book().get("errands").unwrap().clone();
        assert_eq!(
            quest.objectives,
            vec![
                Objective::Craft {
                    item: ItemId::WalkingStaff,
                    count: 2
                },
                Objective::Visit {
                    place: "old_mill".into()
                },
                Objective::Befriend {
                    creature: "meadow_hare".into()
                },
                Objective::TalkTo {
                    npc: "miller".into()
                },
            ]
        );
        assert_eq!(quest.objectives[0].needed(), 2);
        assert_eq!(quest.objectives[1].needed(), 1);
        assert_eq!(quest.objectives[2].needed(), 1);
    }

    #[test]
    fn journal_wording_names_the_target() {
        let quest = book().get("errands").unwrap().clone();
        assert_eq!(quest.objectives[0].describe(), "Craft walking staff");
        assert_eq!(quest.objectives[1].describe(), "Visit old mill");
        assert_eq!(quest.objectives[2].describe(), "Befriend meadow hare");
    }

    #[test]
    fn malformed_new_objectives_name_their_line() {
        for (text, fragment) in [
            ("objective = craft walking_staff", "must be"),
            ("objective = craft walking_staff 0", "craft amount"),
            ("objective = craft pebble 1", "unknown item"),
            ("objective = visit", "must be"),
            ("objective = visit Old-Mill", "place id"),
            ("objective = befriend", "must be"),
            ("objective = befriend two words", "must be"),
        ] {
            let source = format!("[quest q]\ntitle = Q\n{text}\n");
            let error = QuestBook::parse(&source).unwrap_err();
            assert_eq!(error.line, Some(3), "{text}");
            assert!(error.to_string().contains(fragment), "{text}: {error}");
        }
    }

    #[test]
    fn crafting_counts_only_while_the_objective_is_current() {
        let book = book();
        let mut early = QuestLog::new();
        // Crafting before the quest starts does nothing.
        assert!(early
            .record(&book, &crafted(ItemId::WalkingStaff, 1))
            .is_empty());

        let mut log = started();
        let updates = log.record(&book, &crafted(ItemId::WalkingStaff, 1));
        assert_eq!(
            updates,
            vec![QuestUpdate::Progress {
                quest: "errands".into(),
                objective: 0,
                count: 1,
                needed: 2
            }]
        );
        // The wrong item and a zero quantity are ignored.
        assert!(log.record(&book, &crafted(ItemId::CopperCharm, 1)).is_empty());
        assert!(log.record(&book, &crafted(ItemId::WalkingStaff, 0)).is_empty());
        // Gathering the item is not crafting it.
        assert!(log
            .record(
                &book,
                &QuestEvent::Gathered {
                    item: ItemId::WalkingStaff,
                    quantity: 1
                }
            )
            .is_empty());
        let updates = log.record(&book, &crafted(ItemId::WalkingStaff, 5));
        assert_eq!(
            updates[0],
            QuestUpdate::ObjectiveDone {
                quest: "errands".into(),
                objective: 0
            }
        );
        assert_eq!(log.current_objective("errands"), Some((1, 0)));
    }

    #[test]
    fn places_and_creatures_only_count_when_current_and_named() {
        let book = book();
        let mut log = started();
        // Out of order events are ignored: the mill is not the current objective yet.
        assert!(log.record(&book, &visited("old_mill")).is_empty());
        assert!(log.record(&book, &befriended("meadow_hare")).is_empty());
        log.record(&book, &crafted(ItemId::WalkingStaff, 2));

        assert!(log.record(&book, &visited("pond")).is_empty());
        let updates = log.record(&book, &visited("old_mill"));
        assert_eq!(
            updates,
            vec![QuestUpdate::ObjectiveDone {
                quest: "errands".into(),
                objective: 1
            }]
        );
        assert!(log.record(&book, &visited("old_mill")).is_empty());

        assert!(log.record(&book, &befriended("pond_heron")).is_empty());
        log.record(&book, &befriended("meadow_hare"));
        assert_eq!(log.current_objective("errands"), Some((3, 0)));
        let updates = log.record(
            &book,
            &QuestEvent::SpokeTo {
                npc: "miller".into(),
            },
        );
        assert_eq!(
            updates.last(),
            Some(&QuestUpdate::ReadyToTurnIn {
                quest: "errands".into()
            })
        );
        assert_eq!(log.status("errands"), QuestStatus::ReadyToTurnIn);
    }

    #[test]
    fn crafted_items_are_handed_in_like_any_other() {
        let book = book();
        let mut log = started();
        log.record(&book, &crafted(ItemId::WalkingStaff, 2));
        log.record(&book, &visited("old_mill"));
        log.record(&book, &befriended("meadow_hare"));
        log.record(
            &book,
            &QuestEvent::SpokeTo {
                npc: "miller".into(),
            },
        );
        // Ready, but the two staffs must actually be in the bag.
        let mut bag = Inventory::default();
        assert_eq!(
            log.complete(&book, "errands", &mut bag),
            Err(CompleteError::MissingItems(ItemId::WalkingStaff))
        );
        bag.add(ItemId::WalkingStaff, 2).unwrap();
        let rewards = log.complete(&book, "errands", &mut bag).unwrap();
        assert_eq!(rewards.len(), 1);
        assert_eq!(bag.count(ItemId::WalkingStaff), 0);
        assert_eq!(bag.count(ItemId::Timber), 1);
    }
}
