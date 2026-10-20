//! The side panels on the right of the screen (bag, skills, quest book) as plain data: which
//! one is open, and the text each one shows. Pure Rust (no Bevy).
//!
//! Rules:
//!
//! - One side panel is open at a time, like the tabs of a classic MMORPG interface. Opening
//!   one closes the one that was open; opening the open one again closes it. That keeps the
//!   right-hand column from ever overflowing a small window.
//! - Each panel is paged by the Bevy layer (`text_fit::paginate`), so nothing is cut off.
//! - The panels only show state; nothing here changes the bag, gear, or skills.

use crate::equipment::{EquipSlot, Equipment};
use crate::inventory::Inventory;
use crate::skills::{SkillId, Skills};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Tab {
    Bag,
    Skills,
    Quests,
}

impl Tab {
    /// Toolbar order.
    pub const ALL: [Tab; 3] = [Self::Bag, Self::Skills, Self::Quests];

    pub fn label(self) -> &'static str {
        match self {
            Self::Bag => "Bag",
            Self::Skills => "Skills",
            Self::Quests => "Quests",
        }
    }

    /// The key that toggles the panel.
    pub fn hotkey(self) -> &'static str {
        match self {
            Self::Bag => "B",
            Self::Skills => "K",
            Self::Quests => "J",
        }
    }

    /// The text on the toolbar button: `Bag (B)`.
    pub fn button_text(self) -> String {
        format!("{} ({})", self.label(), self.hotkey())
    }

    /// The heading of the open panel.
    pub fn title(self) -> &'static str {
        match self {
            Self::Bag => "BAG",
            Self::Skills => "SKILLS",
            Self::Quests => "QUEST BOOK",
        }
    }
}

/// Which side panel is open, if any.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Dock {
    open: Option<Tab>,
}

impl Dock {
    pub fn current(&self) -> Option<Tab> {
        self.open
    }

    pub fn is_open(&self) -> bool {
        self.open.is_some()
    }

    /// Opens `tab`, or closes it when it is already the open one. Returns whether it is now
    /// open.
    pub fn toggle(&mut self, tab: Tab) -> bool {
        self.open = if self.open == Some(tab) { None } else { Some(tab) };
        self.open == Some(tab)
    }

    pub fn close(&mut self) {
        self.open = None;
    }
}

/// The body of the bag panel: slots used, what is carried, and what is worn.
pub fn bag_text(bag: &Inventory, gear: &Equipment) -> String {
    let mut text = format!("Slots used: {} of {}\n\nCarrying\n", bag.used_slots(), bag.capacity());
    if bag.stacks().is_empty() {
        text.push_str("  nothing yet\n");
    } else {
        for (item, count) in bag.stacks() {
            text.push_str(&format!("  {} x{}\n", item.label(), count));
        }
    }
    text.push_str("\nEquipped\n");
    for slot in EquipSlot::ALL {
        let worn = gear.get(slot).map_or("nothing", |item| item.label());
        text.push_str(&format!("  {}: {}\n", slot.label(), worn));
    }
    text.trim_end().to_string()
}

/// The body of the skills panel: each skill's level and progress, with what it is for.
pub fn skills_text(skills: &Skills) -> String {
    let mut parts = Vec::new();
    for skill in SkillId::ALL {
        let progress = match skills.progress(skill) {
            Some((have, need)) => format!("{have}/{need} xp to next level"),
            None => "top level".to_string(),
        };
        parts.push(format!("{}: level {} ({progress})\n  {}", skill.label(), skills.level(skill), skill.description()));
    }
    parts.join("\n\n")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::inventory::ItemId;

    #[test]
    fn a_new_dock_is_closed() {
        let dock = Dock::default();
        assert!(!dock.is_open());
        assert_eq!(dock.current(), None);
    }

    #[test]
    fn toggling_opens_switches_and_closes() {
        let mut dock = Dock::default();
        assert!(dock.toggle(Tab::Bag));
        assert_eq!(dock.current(), Some(Tab::Bag));
        assert!(dock.toggle(Tab::Skills), "opening another panel switches to it");
        assert_eq!(dock.current(), Some(Tab::Skills));
        assert!(!dock.toggle(Tab::Skills), "toggling the open panel closes it");
        assert!(!dock.is_open());
        assert!(dock.toggle(Tab::Quests));
        dock.close();
        assert!(!dock.is_open());
        dock.close();
        assert!(!dock.is_open(), "closing twice is harmless");
    }

    #[test]
    fn only_one_panel_is_ever_open() {
        let mut dock = Dock::default();
        for tab in Tab::ALL.into_iter().cycle().take(30) {
            dock.toggle(tab);
            let open: Vec<Tab> = Tab::ALL.into_iter().filter(|t| dock.current() == Some(*t)).collect();
            assert!(open.len() <= 1);
        }
    }

    #[test]
    fn every_tab_has_a_distinct_label_hotkey_and_title() {
        let mut labels = std::collections::HashSet::new();
        let mut keys = std::collections::HashSet::new();
        for tab in Tab::ALL {
            assert!(labels.insert(tab.label()));
            assert!(keys.insert(tab.hotkey()));
            assert_eq!(tab.hotkey().len(), 1);
            assert!(!tab.title().is_empty());
            assert!(tab.button_text().contains(tab.label()) && tab.button_text().contains(tab.hotkey()));
        }
        assert_eq!(Tab::Bag.hotkey(), "B", "the roadmap asks for B to open the bag");
        assert_eq!(Tab::Quests.hotkey(), "J", "the journal keeps its old key");
    }

    #[test]
    fn an_empty_bag_says_so_and_lists_every_slot_as_empty() {
        let text = bag_text(&Inventory::new(20), &Equipment::new());
        assert!(text.starts_with("Slots used: 0 of 20"), "{text}");
        assert!(text.contains("nothing yet"));
        for slot in EquipSlot::ALL {
            assert!(text.contains(&format!("{}: nothing", slot.label())), "{text}");
        }
    }

    #[test]
    fn the_bag_lists_items_counts_slots_and_gear() {
        let mut bag = Inventory::new(20);
        bag.add(ItemId::Moonpetal, 3).unwrap();
        bag.add(ItemId::Timber, 150).unwrap(); // two slots
        bag.add(ItemId::WalkingStaff, 1).unwrap();
        let mut gear = Equipment::new();
        gear.equip(ItemId::WalkingStaff, &mut bag).unwrap();
        let text = bag_text(&bag, &gear);
        assert!(text.starts_with(&format!("Slots used: {} of 20", bag.used_slots())), "{text}");
        assert!(text.contains("moonpetal x3"), "{text}");
        assert!(text.contains("timber x150"), "{text}");
        assert!(!text.contains("walking staff x"), "an equipped item has left the bag: {text}");
        assert!(text.contains("Hand: walking staff"), "{text}");
        assert!(!text.contains("nothing yet"));
        assert!(!text.ends_with('\n'));
    }

    #[test]
    fn the_bag_text_changes_nothing_and_is_deterministic() {
        let mut bag = Inventory::new(20);
        bag.add(ItemId::Honeyberry, 4).unwrap();
        let gear = Equipment::new();
        assert_eq!(bag_text(&bag, &gear), bag_text(&bag, &gear));
        assert_eq!(bag.count(ItemId::Honeyberry), 4);
    }

    #[test]
    fn skills_text_lists_every_skill_with_level_progress_and_purpose() {
        let text = skills_text(&Skills::new());
        for skill in SkillId::ALL {
            assert!(text.contains(skill.label()), "{text}");
            assert!(text.contains(skill.description()), "{text}");
        }
        assert!(text.contains("level 1"));
        assert!(text.contains("0/30 xp to next level"), "{text}");
    }

    #[test]
    fn skills_text_follows_progress_and_marks_the_top_level() {
        let mut skills = Skills::new();
        skills.add_xp(SkillId::Woodcraft, 40);
        let text = skills_text(&skills);
        assert!(text.contains("Woodcraft: level 2 (10/90 xp to next level)"), "{text}");
        let maxed = skills_text(&Skills::at_level(10));
        assert_eq!(maxed.matches("top level").count(), SkillId::ALL.len(), "{maxed}");
        assert!(!maxed.contains("xp to next"));
    }
}
