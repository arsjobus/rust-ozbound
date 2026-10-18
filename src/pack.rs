//! The crafting-and-gear panel as plain data: which rows it shows, what each does, and the
//! text for the bag and equipment lines.
//!
//! Pure Rust (no Bevy). The Bevy layer draws `rows` and calls `apply` when one is chosen, so
//! every rule (what can be crafted, equipped, or taken off, and what the player is told)
//! lives here and is tested without a window.
//!
//! Locked recipes (a skill requirement is not met) stay in the list, greyed out with what they
//! need, so the player can see what to work toward; crafting teaches skill XP and a level-up
//! is added to the message.
//!
//! Row order is fixed: recipes in file order, then one "Equip" row per equippable item in
//! the bag (item order), then one "Remove" row per equipped item (slot order). Rows that
//! cannot be used right now are marked `enabled: false` but can still be chosen; choosing
//! one changes nothing and says why.

use crate::crafting::{RecipeBook, RecipeStatus};
use crate::equipment::{EquipSlot, Equipment};
use crate::inventory::{Inventory, ItemId};
use crate::skills::{SkillRequirement, Skills};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PackAction {
    /// Make one batch of the recipe with this id.
    Craft(String),
    /// Equip one of this item from the bag.
    Equip(ItemId),
    /// Take the item in this slot off.
    Unequip(EquipSlot),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PackRow {
    pub action: PackAction,
    pub text: String,
    /// False when choosing the row would fail right now (it then explains why).
    pub enabled: bool,
}

/// Every row of the panel for the current bag and equipment.
pub fn rows(book: &RecipeBook, bag: &Inventory, gear: &Equipment, skills: &Skills) -> Vec<PackRow> {
    let mut out = Vec::new();
    for recipe in book.all() {
        let (note, enabled) = match recipe.status(bag, skills) {
            RecipeStatus::Ready => (String::new(), true),
            RecipeStatus::Locked => {
                let needed: Vec<String> = recipe.unmet_skills(skills).iter().map(SkillRequirement::describe).collect();
                (format!(" - needs {}", needed.join(" and ")), false)
            }
            RecipeStatus::NoRoom => (" - bag full".to_string(), false),
            RecipeStatus::MissingItems => {
                let short: Vec<String> = recipe
                    .shortfalls(bag)
                    .iter()
                    .map(|s| format!("{} more {}", s.needed - s.have, s.item.label()))
                    .collect();
                (format!(" - need {}", short.join(", ")), false)
            }
        };
        out.push(PackRow {
            action: PackAction::Craft(recipe.id.clone()),
            text: format!("Make {} ({}){}", recipe.name.to_lowercase(), recipe.describe_inputs(), note),
            enabled,
        });
    }
    for item in bag.stacks().keys() {
        if let Some(slot) = item.equip_slot() {
            out.push(PackRow {
                action: PackAction::Equip(*item),
                text: format!("Equip {} ({})", item.label(), slot.label().to_lowercase()),
                enabled: true,
            });
        }
    }
    for (slot, item) in gear.iter() {
        let full = bag.can_add(item, 1).is_err();
        out.push(PackRow {
            action: PackAction::Unequip(slot),
            text: format!("Remove {} ({}){}", item.label(), slot.label().to_lowercase(), if full { " - bag full" } else { "" }),
            enabled: !full,
        });
    }
    out
}

/// Does the action and returns the message to show the player. On failure nothing changes.
pub fn apply(
    action: &PackAction,
    book: &RecipeBook,
    bag: &mut Inventory,
    gear: &mut Equipment,
    skills: &mut Skills,
) -> String {
    match action {
        PackAction::Craft(id) => match book.craft(id, bag, skills) {
            Ok(done) => {
                let mut message = format!("You made {} {}.", done.made.quantity, done.made.item.label());
                if let Some(level_up) = done.xp.and_then(|gain| gain.message()) {
                    message.push(' ');
                    message.push_str(&level_up);
                }
                message
            }
            Err(error) => error.message(),
        },
        PackAction::Equip(item) => match gear.equip(*item, bag) {
            Ok(None) => format!("You equip the {}.", item.label()),
            Ok(Some(old)) => format!("You equip the {} and put the {} back in your bag.", item.label(), old.label()),
            Err(error) => error.message(),
        },
        PackAction::Unequip(slot) => match gear.unequip(*slot, bag) {
            Ok(item) => format!("You remove the {}.", item.label()),
            Err(error) => error.message(),
        },
    }
}

/// "Bag 5/20: timber x3, moonpetal x2" (slots used / capacity), or "Bag 0/20: empty".
pub fn bag_line(bag: &Inventory) -> String {
    let items: Vec<String> = bag.stacks().iter().map(|(item, n)| format!("{} x{}", item.label(), n)).collect();
    let list = if items.is_empty() { "empty".to_string() } else { items.join(", ") };
    format!("Bag {}/{}: {}", bag.used_slots(), bag.capacity(), list)
}

/// "Equipped: hand: walking staff, charm: copper charm", or "Equipped: nothing".
pub fn equipment_line(gear: &Equipment) -> String {
    let worn: Vec<String> = gear.iter().map(|(slot, item)| format!("{}: {}", slot.label().to_lowercase(), item.label())).collect();
    if worn.is_empty() { "Equipped: nothing".to_string() } else { format!("Equipped: {}", worn.join(", ")) }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn book() -> RecipeBook {
        RecipeBook::builtin().unwrap()
    }

    /// A character who meets every skill requirement.
    fn max() -> Skills {
        Skills::at_level(crate::skills::MAX_LEVEL)
    }

    fn bag(items: &[(ItemId, u32)], capacity: usize) -> Inventory {
        let mut bag = Inventory::new(capacity);
        for (item, n) in items {
            bag.add(*item, *n).unwrap();
        }
        bag
    }

    #[test]
    fn empty_bag_lists_every_recipe_as_unavailable_with_what_is_missing() {
        let rows = rows(&book(), &Inventory::new(5), &Equipment::new(), &max());
        assert_eq!(rows.len(), 3, "no gear rows without gear");
        assert!(rows.iter().all(|r| !r.enabled));
        assert_eq!(rows[0].text, "Make walking staff (3 timber) - need 3 more timber");
        assert_eq!(rows[1].text, "Make copper charm (2 copper ore + 1 timber) - need 2 more copper ore, 1 more timber");
        assert_eq!(rows[2].action, PackAction::Craft("mint_tea".into()));
    }

    #[test]
    fn a_craftable_recipe_is_enabled_with_no_note() {
        let rows = rows(&book(), &bag(&[(ItemId::Timber, 3)], 5), &Equipment::new(), &max());
        assert!(rows[0].enabled);
        assert_eq!(rows[0].text, "Make walking staff (3 timber)");
        assert!(!rows[1].enabled);
    }

    #[test]
    fn a_recipe_with_no_room_says_the_bag_is_full() {
        let rows = rows(&book(), &bag(&[(ItemId::Timber, 4), (ItemId::Moonpetal, 1)], 2), &Equipment::new(), &max());
        assert!(!rows[0].enabled);
        assert_eq!(rows[0].text, "Make walking staff (3 timber) - bag full");
    }

    #[test]
    fn gear_rows_follow_recipes_in_item_then_slot_order() {
        let mut b = bag(&[(ItemId::CopperCharm, 1), (ItemId::WalkingStaff, 1), (ItemId::Timber, 1)], 6);
        let mut gear = Equipment::new();
        gear.equip(ItemId::WalkingStaff, &mut b).unwrap();
        let rows = rows(&book(), &b, &gear, &max());
        let tail: Vec<_> = rows[3..].iter().map(|r| (r.action.clone(), r.text.as_str())).collect();
        assert_eq!(
            tail,
            vec![
                (PackAction::Equip(ItemId::CopperCharm), "Equip copper charm (charm)"),
                (PackAction::Unequip(EquipSlot::Hand), "Remove walking staff (hand)"),
            ]
        );
        assert!(rows[3..].iter().all(|r| r.enabled));
    }

    #[test]
    fn ordinary_items_never_get_an_equip_row() {
        let rows = rows(&book(), &bag(&[(ItemId::Timber, 2), (ItemId::Moonpetal, 1)], 4), &Equipment::new(), &max());
        assert!(rows.iter().all(|r| !matches!(r.action, PackAction::Equip(_))));
    }

    #[test]
    fn remove_row_is_disabled_with_a_full_bag() {
        let mut b = bag(&[(ItemId::CopperCharm, 1)], 1);
        let mut gear = Equipment::new();
        gear.equip(ItemId::CopperCharm, &mut b).unwrap();
        b.add(ItemId::Timber, 1).unwrap();
        let rows = rows(&book(), &b, &gear, &max());
        let remove = rows.last().unwrap();
        assert_eq!(remove.text, "Remove copper charm (charm) - bag full");
        assert!(!remove.enabled);
    }

    #[test]
    fn applying_craft_equip_and_unequip_changes_state_and_reports() {
        let book = book();
        let mut b = bag(&[(ItemId::Timber, 3)], 4);
        let mut gear = Equipment::new();
        assert_eq!(apply(&PackAction::Craft("walking_staff".into()), &book, &mut b, &mut gear, &mut max()), "You made 1 walking staff.");
        assert_eq!(b.count(ItemId::WalkingStaff), 1);
        assert_eq!(apply(&PackAction::Equip(ItemId::WalkingStaff), &book, &mut b, &mut gear, &mut max()), "You equip the walking staff.");
        assert_eq!(gear.get(EquipSlot::Hand), Some(ItemId::WalkingStaff));
        assert_eq!(apply(&PackAction::Unequip(EquipSlot::Hand), &book, &mut b, &mut gear, &mut max()), "You remove the walking staff.");
        assert_eq!(b.count(ItemId::WalkingStaff), 1);
        assert!(gear.is_empty());
    }

    #[test]
    fn applying_a_swap_mentions_the_returned_item() {
        let book = book();
        let mut b = bag(&[(ItemId::WalkingStaff, 2)], 3);
        let mut gear = Equipment::new();
        apply(&PackAction::Equip(ItemId::WalkingStaff), &book, &mut b, &mut gear, &mut max());
        let msg = apply(&PackAction::Equip(ItemId::WalkingStaff), &book, &mut b, &mut gear, &mut max());
        assert_eq!(msg, "You equip the walking staff and put the walking staff back in your bag.");
    }

    #[test]
    fn failed_actions_explain_and_change_nothing() {
        let book = book();
        let mut b = bag(&[(ItemId::Timber, 1)], 2);
        let mut gear = Equipment::new();
        let before = (b.clone(), gear.clone());
        let msgs = [
            apply(&PackAction::Craft("walking_staff".into()), &book, &mut b, &mut gear, &mut max()),
            apply(&PackAction::Craft("nope".into()), &book, &mut b, &mut gear, &mut max()),
            apply(&PackAction::Equip(ItemId::CopperCharm), &book, &mut b, &mut gear, &mut max()),
            apply(&PackAction::Equip(ItemId::Timber), &book, &mut b, &mut gear, &mut max()),
            apply(&PackAction::Unequip(EquipSlot::Head), &book, &mut b, &mut gear, &mut max()),
        ];
        assert_eq!(msgs[0], "You still need 2 more timber.");
        assert!(msgs[1].contains("do not know"));
        assert_eq!(msgs[2], "You are not carrying a copper charm.");
        assert_eq!(msgs[3], "You cannot equip the timber.");
        assert_eq!(msgs[4], "Nothing is equipped in your head slot.");
        assert_eq!((b, gear), before);
    }

    #[test]
    fn every_row_is_consistent_with_what_apply_does() {
        // An enabled row must succeed; a disabled row must fail without changing anything,
        // including skills. Checked for a newcomer (locked recipes) and a master.
        let book = book();
        let mut b = bag(&[(ItemId::Timber, 3), (ItemId::CopperCharm, 1), (ItemId::Meadowmint, 1)], 4);
        let mut gear = Equipment::new();
        gear.equip(ItemId::CopperCharm, &mut b).unwrap();
        b.add(ItemId::WalkingStaff, 1).unwrap();
        for skills in [Skills::new(), Skills::at_level(3), max()] {
            for row in rows(&book, &b, &gear, &skills) {
                let (mut b2, mut g2, mut s2) = (b.clone(), gear.clone(), skills.clone());
                let message = apply(&row.action, &book, &mut b2, &mut g2, &mut s2);
                let changed = b2 != b || g2 != gear;
                assert_eq!(changed, row.enabled, "{row:?} -> {message}");
                if !row.enabled {
                    assert_eq!(s2, skills, "a refused row must not teach anything: {row:?}");
                }
            }
        }
    }

    #[test]
    fn a_locked_recipe_says_what_it_needs_and_stays_locked_with_every_ingredient() {
        let b = bag(&[(ItemId::CopperOre, 2), (ItemId::Timber, 1)], 4);
        let rows = rows(&book(), &b, &Equipment::new(), &Skills::new());
        assert_eq!(rows[1].text, "Make copper charm (2 copper ore + 1 timber) - needs Woodcraft level 3");
        assert!(!rows[1].enabled);
        // Skills are checked before ingredients, so an empty bag shows the skill, not the shortfall.
        let rows = rows_for_empty_bag();
        assert_eq!(rows[1].text, "Make copper charm (2 copper ore + 1 timber) - needs Woodcraft level 3");
        assert_eq!(rows[2].text, "Make mint tea (2 meadowmint + 1 honeyberry) - needs Herbalism level 2");
        // Meeting the level unlocks it.
        let unlocked = super::rows(&book(), &b, &Equipment::new(), &Skills::at_level(3));
        assert!(unlocked[1].enabled);
        assert_eq!(unlocked[1].text, "Make copper charm (2 copper ore + 1 timber)");
    }

    fn rows_for_empty_bag() -> Vec<PackRow> {
        rows(&book(), &Inventory::new(5), &Equipment::new(), &Skills::new())
    }

    #[test]
    fn crafting_a_locked_recipe_is_refused_and_changes_nothing() {
        let book = book();
        let mut b = bag(&[(ItemId::CopperOre, 2), (ItemId::Timber, 1)], 4);
        let mut gear = Equipment::new();
        let mut skills = Skills::new();
        let before = (b.clone(), skills.clone());
        let message = apply(&PackAction::Craft("copper_charm".into()), &book, &mut b, &mut gear, &mut skills);
        assert_eq!(message, "You need Woodcraft level 3 to make that.");
        assert_eq!((b, skills), before);
    }

    #[test]
    fn crafting_teaches_xp_and_announces_a_level_up() {
        let book = book();
        let mut b = bag(&[(ItemId::Timber, 3)], 4);
        let mut gear = Equipment::new();
        // The staff teaches 25 XP; a character 10 XP short of level 2 crosses the line.
        let mut skills = Skills::new();
        skills.add_xp(crate::skills::SkillId::Woodcraft, 10);
        assert_eq!(skills.level(crate::skills::SkillId::Woodcraft), 1);
        let message = apply(&PackAction::Craft("walking_staff".into()), &book, &mut b, &mut gear, &mut skills);
        assert_eq!(message, "You made 1 walking staff. Your Woodcraft level is now 2.");
        assert_eq!(skills.xp(crate::skills::SkillId::Woodcraft), 35);
    }

    #[test]
    fn crafting_without_a_level_up_does_not_mention_xp() {
        let book = book();
        let mut b = bag(&[(ItemId::Timber, 3)], 4);
        let mut skills = Skills::new();
        let message = apply(&PackAction::Craft("walking_staff".into()), &book, &mut b, &mut Equipment::new(), &mut skills);
        assert_eq!(message, "You made 1 walking staff.");
        assert_eq!(skills.xp(crate::skills::SkillId::Woodcraft), 25);
    }

    #[test]
    fn bag_and_equipment_lines_read_clearly() {
        assert_eq!(bag_line(&Inventory::new(20)), "Bag 0/20: empty");
        let b = bag(&[(ItemId::Timber, 3), (ItemId::Moonpetal, 2), (ItemId::CopperOre, 150)], 20);
        assert_eq!(bag_line(&b), "Bag 4/20: moonpetal x2, timber x3, copper ore x150");
        assert_eq!(equipment_line(&Equipment::new()), "Equipped: nothing");
        let mut b2 = bag(&[(ItemId::WalkingStaff, 1), (ItemId::CopperCharm, 1)], 4);
        let mut gear = Equipment::new();
        gear.equip(ItemId::CopperCharm, &mut b2).unwrap();
        gear.equip(ItemId::WalkingStaff, &mut b2).unwrap();
        assert_eq!(equipment_line(&gear), "Equipped: hand: walking staff, charm: copper charm");
    }
}
