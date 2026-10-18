//! The bestiary: gentle creatures of the countryside and the optional, non-lethal
//! encounters with them.
//!
//! Pure Rust (no Bevy). Definitions are validated content (`Bestiary`, parsed from
//! `assets/bestiary/*.bestiary`); what the player has met is a [`FieldNotes`] value that is
//! changed only through [`approach`], which either does everything or changes nothing.
//!
//! Rules, in one place:
//! - There is no combat. Nothing in this module can hurt a creature or the player, and no
//!   creature is ever required: they are optional, apart from quests that ask for one.
//! - Approaching a creature the first time marks it *seen*.
//! - If the bag holds the creature's `likes` item(s) *and* the creature's `gift` fits, the
//!   offering is taken, the gift is handed over, and the creature is *befriended*. A
//!   creature can be befriended once; approaching it again only says it is settled.
//! - If the bag holds the offering but the gift would not fit, nothing is taken and the
//!   player is told to make room (so a full bag never loses an offering).
//! - Otherwise the creature just watches and its `hint` says what it might like.

use std::collections::BTreeSet;

use crate::inventory::{Inventory, ItemId};
use crate::quests::ItemAmount;
use crate::textdata::{check_id, parse_positive, read_lines, ContentError, Item};

/// The bestiary file compiled into the game, identical to `assets/bestiary/hollowmere.bestiary`.
pub const DEFAULT_BESTIARY: &str = include_str!("../assets/bestiary/hollowmere.bestiary");
/// Where the game looks for the bestiary on disk, relative to the working directory.
pub const BESTIARY_PATH: &str = "assets/bestiary/hollowmere.bestiary";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Creature {
    pub id: String,
    /// Shown above the text and in the field notes, e.g. "Meadow hare".
    pub name: String,
    /// What the player sees when they examine it.
    pub description: String,
    /// What it takes to win it over.
    pub likes: ItemAmount,
    /// Handed over when it is befriended (optional).
    pub gift: Option<ItemAmount>,
    /// Said when approached without the offering.
    pub hint: String,
    /// Said when it is befriended.
    pub thanks: String,
    /// Said when approached again after being befriended.
    pub settled: String,
    /// Line of the `[creature ...]` header when parsed from a file; 0 when built in code.
    pub line: usize,
}

/// All creature definitions, validated: unique ids and every text field filled in.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Bestiary {
    creatures: Vec<Creature>,
}

impl Bestiary {
    pub fn new(creatures: Vec<Creature>) -> Result<Self, ContentError> {
        let mut seen = BTreeSet::new();
        for c in &creatures {
            if !seen.insert(c.id.as_str()) {
                return Err(ContentError::at(c.line, format!("creature `{}` is defined twice", c.id)));
            }
            let here = |msg: &str| ContentError::at(c.line, format!("creature `{}`: {msg}", c.id));
            for (what, text) in [
                ("name", &c.name),
                ("description", &c.description),
                ("hint", &c.hint),
                ("thanks", &c.thanks),
                ("settled", &c.settled),
            ] {
                if text.trim().is_empty() {
                    return Err(here(&format!("needs a `{what}`")));
                }
            }
        }
        Ok(Self { creatures })
    }

    pub fn get(&self, id: &str) -> Option<&Creature> {
        self.creatures.iter().find(|c| c.id == id)
    }

    pub fn iter(&self) -> impl Iterator<Item = &Creature> {
        self.creatures.iter()
    }

    pub fn len(&self) -> usize {
        self.creatures.len()
    }

    pub fn is_empty(&self) -> bool {
        self.creatures.is_empty()
    }

    /// Parses and validate a `.bestiary` file (format documented in `hollowmere.bestiary`).
    pub fn parse(text: &str) -> Result<Self, ContentError> {
        struct Draft {
            creature: Creature,
            likes_line: usize,
        }
        let mut drafts: Vec<Draft> = Vec::new();
        let mut given: Vec<Vec<String>> = Vec::new();
        for line in read_lines(text)? {
            let n = line.number;
            match line.item {
                Item::Section { kind, arg } => {
                    if kind != "creature" {
                        return Err(ContentError::at(n, format!("unknown section `[{kind}]` (expected `[creature <id>]`)")));
                    }
                    check_id(n, "creature id", &arg)?;
                    drafts.push(Draft {
                        creature: Creature {
                            id: arg,
                            name: String::new(),
                            description: String::new(),
                            likes: ItemAmount { item: ItemId::Honeyberry, quantity: 0 },
                            gift: None,
                            hint: String::new(),
                            thanks: String::new(),
                            settled: String::new(),
                            line: n,
                        },
                        likes_line: 0,
                    });
                    given.push(Vec::new());
                }
                Item::Field { key, value } => {
                    let Some(draft) = drafts.last_mut() else {
                        return Err(ContentError::at(n, format!("`{key}` must be inside a `[creature <id>]` section")));
                    };
                    let seen = given.last_mut().expect("one list per draft");
                    if seen.contains(&key) {
                        return Err(ContentError::at(n, format!("`{key}` is given twice")));
                    }
                    let c = &mut draft.creature;
                    match key.as_str() {
                        "name" => c.name = value,
                        "description" => c.description = value,
                        "hint" => c.hint = value,
                        "thanks" => c.thanks = value,
                        "settled" => c.settled = value,
                        "likes" => {
                            c.likes = parse_amount(n, &value)?;
                            draft.likes_line = n;
                        }
                        "gift" => c.gift = Some(parse_amount(n, &value)?),
                        other => return Err(ContentError::at(n, format!("unknown creature field `{other}`"))),
                    }
                    seen.push(key);
                }
            }
        }
        for draft in &drafts {
            if draft.likes_line == 0 {
                return Err(ContentError::at(
                    draft.creature.line,
                    format!("creature `{}`: needs a `likes = <item> <amount>` line", draft.creature.id),
                ));
            }
        }
        Self::new(drafts.into_iter().map(|d| d.creature).collect())
    }
}

fn parse_amount(line: usize, value: &str) -> Result<ItemAmount, ContentError> {
    let words: Vec<&str> = value.split_whitespace().collect();
    match words.as_slice() {
        [item, quantity] => Ok(ItemAmount {
            item: ItemId::from_key(item).ok_or_else(|| ContentError::at(line, format!("unknown item `{item}`")))?,
            quantity: parse_positive(line, "the item amount", quantity)?,
        }),
        _ => Err(ContentError::at(line, format!("`{value}` must be `<item> <amount>`"))),
    }
}

/// What the player has met so far. Kept in memory for now; saving it belongs to M6.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct FieldNotes {
    seen: BTreeSet<String>,
    befriended: BTreeSet<String>,
}

impl FieldNotes {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn has_seen(&self, id: &str) -> bool {
        self.seen.contains(id)
    }

    pub fn is_befriended(&self, id: &str) -> bool {
        self.befriended.contains(id)
    }

    /// Ids of befriended creatures, in alphabetical order.
    pub fn befriended(&self) -> impl Iterator<Item = &str> {
        self.befriended.iter().map(String::as_str)
    }

    /// The field notes as journal text: one line per creature met, in bestiary order.
    /// Empty when nothing has been met.
    pub fn render(&self, book: &Bestiary) -> String {
        let mut lines = Vec::new();
        for c in book.iter().filter(|c| self.seen.contains(&c.id)) {
            let state = if self.befriended.contains(&c.id) { "befriended" } else { "met" };
            lines.push(format!("{} ({state})", c.name));
        }
        if lines.is_empty() {
            String::new()
        } else {
            format!("Field notes: {}", lines.join(", "))
        }
    }
}

/// What happened when the player approached a creature.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Encounter {
    /// The creature watched and nothing changed except that it is now *seen*. `text` is its hint.
    Watched { text: String },
    /// The offering was taken and the gift handed over.
    Befriended { creature: String, taken: ItemAmount, gift: Option<ItemAmount>, text: String },
    /// Already befriended earlier.
    Settled { text: String },
    /// The player has the offering, but the gift would not fit; nothing was taken.
    NeedsRoom { text: String },
}

impl Encounter {
    /// The sentence to show the player.
    pub fn text(&self) -> &str {
        match self {
            Self::Watched { text } | Self::Settled { text } | Self::NeedsRoom { text } => text,
            Self::Befriended { text, .. } => text,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EncounterError {
    UnknownCreature(String),
}

/// The player approaches a creature. All or nothing: the bag and notes only change when the
/// result is [`Encounter::Befriended`] (or the creature is marked seen, which is harmless).
pub fn approach(
    book: &Bestiary,
    notes: &mut FieldNotes,
    inventory: &mut Inventory,
    id: &str,
) -> Result<Encounter, EncounterError> {
    let creature = book.get(id).ok_or_else(|| EncounterError::UnknownCreature(id.to_string()))?;
    notes.seen.insert(creature.id.clone());
    if notes.befriended.contains(id) {
        return Ok(Encounter::Settled { text: creature.settled.clone() });
    }
    if inventory.count(creature.likes.item) < creature.likes.quantity {
        return Ok(Encounter::Watched { text: creature.hint.clone() });
    }
    // Work on a copy: taking the offering may free the very slot the gift needs.
    let mut trial = inventory.clone();
    let taken = trial.remove(creature.likes.item, creature.likes.quantity).is_ok();
    let fits = taken && creature.gift.is_none_or_fits(&mut trial);
    if !fits {
        return Ok(Encounter::NeedsRoom {
            text: format!("{} seems willing, but your bag has no room for what it would give you. Make some space first.", creature.name),
        });
    }
    *inventory = trial;
    notes.befriended.insert(creature.id.clone());
    Ok(Encounter::Befriended {
        creature: creature.id.clone(),
        taken: creature.likes,
        gift: creature.gift,
        text: creature.thanks.clone(),
    })
}

/// Small helper so `approach` reads top to bottom: adds an optional gift to a bag.
trait GiftFits {
    fn is_none_or_fits(&self, bag: &mut Inventory) -> bool;
}

impl GiftFits for Option<ItemAmount> {
    fn is_none_or_fits(&self, bag: &mut Inventory) -> bool {
        match self {
            None => true,
            Some(gift) => bag.add(gift.item, gift.quantity).is_ok(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = "\
# two creatures
[creature hare]
name = Meadow hare
description = A long-eared hare sitting very still in the clover.
likes = honeyberry 1
gift = moonpetal 1
hint = The hare sniffs the air. It seems to like honeyberries.
thanks = The hare nibbles the berry and nudges a glowing petal toward you.
settled = The hare blinks at you sleepily.

[creature heron]
name = Pond heron
description = A grey heron standing in the shallows.
likes = meadowmint 2
hint = The heron eyes your hands, then looks away.
thanks = The heron accepts the mint and flaps off with a croak.
settled = The heron stands on one leg.
";

    fn book() -> Bestiary {
        Bestiary::parse(SAMPLE).unwrap()
    }

    fn bag(items: &[(ItemId, u32)]) -> Inventory {
        let mut bag = Inventory::new(20);
        for (item, n) in items {
            bag.add(*item, *n).unwrap();
        }
        bag
    }

    #[test]
    fn parses_every_field() {
        let book = book();
        assert_eq!(book.len(), 2);
        let hare = book.get("hare").unwrap();
        assert_eq!(hare.name, "Meadow hare");
        assert_eq!(hare.likes, ItemAmount { item: ItemId::Honeyberry, quantity: 1 });
        assert_eq!(hare.gift, Some(ItemAmount { item: ItemId::Moonpetal, quantity: 1 }));
        assert_eq!(book.get("heron").unwrap().gift, None);
        assert!(book.get("fox").is_none());
        assert!(!book.is_empty());
    }

    #[test]
    fn the_shipped_bestiary_is_valid() {
        let book = Bestiary::parse(DEFAULT_BESTIARY).unwrap();
        assert!(book.len() >= 3);
        let path = concat!(env!("CARGO_MANIFEST_DIR"), "/assets/bestiary/hollowmere.bestiary");
        assert_eq!(std::fs::read_to_string(path).unwrap(), DEFAULT_BESTIARY);
    }

    #[test]
    fn invalid_files_report_the_line() {
        let cases: [(&str, usize, &str); 9] = [
            ("[monster x]\n", 1, "unknown section"),
            ("name = Hare\n", 1, "inside a `[creature"),
            ("[creature Bad-Id]\n", 1, "creature id"),
            ("[creature a]\nname = A\nname = B\n", 3, "given twice"),
            ("[creature a]\nlikes = pebble 1\n", 2, "unknown item"),
            ("[creature a]\nlikes = honeyberry 0\n", 2, "item amount"),
            ("[creature a]\nlikes = honeyberry\n", 2, "<item> <amount>"),
            ("[creature a]\nwings = 2\n", 2, "unknown creature field"),
            ("[creature a]\nname = A\n", 1, "needs a `likes"),
        ];
        for (text, line, fragment) in cases {
            let error = Bestiary::parse(text).unwrap_err();
            assert_eq!(error.line, Some(line), "{text:?}");
            assert!(error.to_string().contains(fragment), "{text:?}: {error}");
        }
    }

    #[test]
    fn missing_text_and_duplicate_ids_are_rejected() {
        let missing = SAMPLE.replace("hint = The heron eyes your hands, then looks away.\n", "");
        let error = Bestiary::parse(&missing).unwrap_err();
        assert!(error.to_string().contains("needs a `hint`"), "{error}");
        let doubled = format!("{SAMPLE}\n{}", SAMPLE.split("[creature heron]").next().unwrap());
        let error = Bestiary::parse(&doubled).unwrap_err();
        assert!(error.to_string().contains("defined twice"), "{error}");
    }

    #[test]
    fn approaching_without_the_offering_only_marks_it_seen() {
        let book = book();
        let mut notes = FieldNotes::new();
        let mut bag = bag(&[(ItemId::Timber, 3)]);
        let before = bag.clone();
        let result = approach(&book, &mut notes, &mut bag, "hare").unwrap();
        assert_eq!(result, Encounter::Watched { text: book.get("hare").unwrap().hint.clone() });
        assert_eq!(bag, before);
        assert!(notes.has_seen("hare"));
        assert!(!notes.is_befriended("hare"));
        assert!(!notes.has_seen("heron"));
    }

    #[test]
    fn too_few_of_the_offering_is_not_enough() {
        let book = book();
        let mut notes = FieldNotes::new();
        let mut bag = bag(&[(ItemId::Meadowmint, 1)]);
        let result = approach(&book, &mut notes, &mut bag, "heron").unwrap();
        assert!(matches!(result, Encounter::Watched { .. }));
        assert_eq!(bag.count(ItemId::Meadowmint), 1);
    }

    #[test]
    fn the_right_offering_befriends_and_pays_the_gift() {
        let book = book();
        let mut notes = FieldNotes::new();
        let mut bag = bag(&[(ItemId::Honeyberry, 2)]);
        let result = approach(&book, &mut notes, &mut bag, "hare").unwrap();
        assert_eq!(
            result,
            Encounter::Befriended {
                creature: "hare".into(),
                taken: ItemAmount { item: ItemId::Honeyberry, quantity: 1 },
                gift: Some(ItemAmount { item: ItemId::Moonpetal, quantity: 1 }),
                text: book.get("hare").unwrap().thanks.clone(),
            }
        );
        assert_eq!(bag.count(ItemId::Honeyberry), 1);
        assert_eq!(bag.count(ItemId::Moonpetal), 1);
        assert!(notes.is_befriended("hare"));
        assert_eq!(notes.befriended().collect::<Vec<_>>(), vec!["hare"]);
    }

    #[test]
    fn a_creature_with_no_gift_just_takes_the_offering() {
        let book = book();
        let mut notes = FieldNotes::new();
        let mut bag = bag(&[(ItemId::Meadowmint, 2)]);
        let result = approach(&book, &mut notes, &mut bag, "heron").unwrap();
        assert!(matches!(result, Encounter::Befriended { gift: None, .. }));
        assert_eq!(bag.count(ItemId::Meadowmint), 0);
    }

    #[test]
    fn a_creature_is_only_befriended_once() {
        let book = book();
        let mut notes = FieldNotes::new();
        let mut bag = bag(&[(ItemId::Honeyberry, 3)]);
        approach(&book, &mut notes, &mut bag, "hare").unwrap();
        let after_first = bag.clone();
        let again = approach(&book, &mut notes, &mut bag, "hare").unwrap();
        assert_eq!(again, Encounter::Settled { text: book.get("hare").unwrap().settled.clone() });
        assert_eq!(bag, after_first, "the second visit must not take another offering");
    }

    #[test]
    fn the_slot_freed_by_the_offering_can_hold_the_gift() {
        let book = book();
        let mut notes = FieldNotes::new();
        // One slot, holding the only berry: taking it frees the slot the petal needs.
        let mut tight = Inventory::new(1);
        tight.add(ItemId::Honeyberry, 1).unwrap();
        let result = approach(&book, &mut notes, &mut tight, "hare").unwrap();
        assert!(matches!(result, Encounter::Befriended { .. }));
        assert_eq!(tight.count(ItemId::Honeyberry), 0);
        assert_eq!(tight.count(ItemId::Moonpetal), 1);
    }

    #[test]
    fn a_gift_that_does_not_fit_changes_nothing() {
        let book = book();
        let mut notes = FieldNotes::new();
        // Two slots, both stay occupied after one berry is taken (a second berry remains).
        let mut full = Inventory::new(2);
        full.add(ItemId::Honeyberry, 2).unwrap();
        full.add(ItemId::Timber, 1).unwrap();
        let before = full.clone();
        let result = approach(&book, &mut notes, &mut full, "hare").unwrap();
        assert!(matches!(result, Encounter::NeedsRoom { .. }));
        assert!(result.text().contains("Meadow hare"));
        assert_eq!(full, before, "the offering must not be taken when the gift cannot be given");
        assert!(!notes.is_befriended("hare"));
        assert!(notes.has_seen("hare"));

        // Making room (dropping the timber) lets the same approach succeed.
        full.remove(ItemId::Timber, 1).unwrap();
        let result = approach(&book, &mut notes, &mut full, "hare").unwrap();
        assert!(matches!(result, Encounter::Befriended { .. }));
        assert_eq!(full.count(ItemId::Honeyberry), 1);
        assert_eq!(full.count(ItemId::Moonpetal), 1);
    }

    #[test]
    fn a_gift_too_big_for_any_bag_is_refused_without_taking_the_offering() {
        let big = Bestiary::parse(&SAMPLE.replace("gift = moonpetal 1", "gift = timber 300")).unwrap();
        let mut notes = FieldNotes::new();
        let mut bag = Inventory::new(2);
        bag.add(ItemId::Honeyberry, 1).unwrap();
        let before = bag.clone();
        let result = approach(&big, &mut notes, &mut bag, "hare").unwrap();
        assert!(matches!(result, Encounter::NeedsRoom { .. }));
        assert_eq!(bag, before);
        assert!(!notes.is_befriended("hare"));
    }

    #[test]
    fn unknown_creatures_are_an_error_and_change_nothing() {
        let book = book();
        let mut notes = FieldNotes::new();
        let mut bag = bag(&[(ItemId::Honeyberry, 1)]);
        let before = bag.clone();
        assert_eq!(
            approach(&book, &mut notes, &mut bag, "dragon"),
            Err(EncounterError::UnknownCreature("dragon".into()))
        );
        assert_eq!(bag, before);
        assert_eq!(notes, FieldNotes::new());
    }

    #[test]
    fn field_notes_list_creatures_in_bestiary_order() {
        let book = book();
        let mut notes = FieldNotes::new();
        assert_eq!(notes.render(&book), "");
        let mut bag = bag(&[(ItemId::Meadowmint, 2)]);
        approach(&book, &mut notes, &mut bag, "heron").unwrap();
        approach(&book, &mut notes, &mut bag, "hare").unwrap();
        assert_eq!(notes.render(&book), "Field notes: Meadow hare (met), Pond heron (befriended)");
    }
}
