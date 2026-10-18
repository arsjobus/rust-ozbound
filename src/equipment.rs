//! Equipment slots: what the player holds or wears, separate from the bag.
//!
//! Pure Rust (no Bevy). Equipment items are never stacked (`ItemId::max_stack` is 1), and
//! each belongs to exactly one slot (`ItemId::equip_slot`). Equipping and unequipping move
//! items between the bag and a slot, all or nothing: a failed call changes neither.
//!
//! Rules, in one place:
//! - Only an item that names a slot can be equipped, and it must be in the bag.
//! - Equipping into an occupied slot swaps: the old item goes back to the bag.
//! - Unequipping needs a free bag slot; with a full bag the item stays equipped.
//! - Equipment has no effect on gameplay yet; stat and skill effects arrive with M4.

use std::collections::BTreeMap;

use crate::inventory::{Inventory, InventoryError, ItemId};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum EquipSlot {
    Head,
    Body,
    Feet,
    /// A tool or staff carried in the hand.
    Hand,
    Charm,
}

impl EquipSlot {
    pub const ALL: [EquipSlot; 5] = [Self::Head, Self::Body, Self::Feet, Self::Hand, Self::Charm];

    /// Stable lower-case key (for future saves). Never change an existing key.
    pub fn key(self) -> &'static str {
        match self {
            Self::Head => "head",
            Self::Body => "body",
            Self::Feet => "feet",
            Self::Hand => "hand",
            Self::Charm => "charm",
        }
    }

    pub fn from_key(key: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|slot| slot.key() == key)
    }

    /// Capitalised name for panels.
    pub fn label(self) -> &'static str {
        match self {
            Self::Head => "Head",
            Self::Body => "Body",
            Self::Feet => "Feet",
            Self::Hand => "Hand",
            Self::Charm => "Charm",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EquipError {
    /// The item has no equipment slot.
    NotEquippable(ItemId),
    /// The item is not in the bag.
    NotInBag(ItemId),
    /// Nothing is equipped in that slot.
    SlotEmpty(EquipSlot),
    /// There is no bag room for the item that would come off.
    BagFull,
    /// The item belongs in a different slot (only seen when restoring a save).
    WrongSlot(ItemId, EquipSlot),
    /// The slot is listed twice (only seen when restoring a save).
    SlotTaken(EquipSlot),
}

impl EquipError {
    /// A short plain-language message for the player.
    pub fn message(&self) -> String {
        match self {
            Self::NotEquippable(item) => format!("You cannot equip the {}.", item.label()),
            Self::NotInBag(item) => format!("You are not carrying a {}.", item.label()),
            Self::SlotEmpty(slot) => format!("Nothing is equipped in your {} slot.", slot.label().to_lowercase()),
            Self::BagFull => "Your inventory is full. Make room first.".to_string(),
            Self::WrongSlot(item, slot) => format!("The {} cannot go in the {} slot.", item.label(), slot.label().to_lowercase()),
            Self::SlotTaken(slot) => format!("The {} slot is listed twice.", slot.label().to_lowercase()),
        }
    }
}

/// What the player has equipped, one item per slot.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Equipment {
    slots: BTreeMap<EquipSlot, ItemId>,
}

impl Equipment {
    pub fn new() -> Self {
        Self::default()
    }

    /// Rebuilds equipment from saved `(slot, item)` pairs. Each item must belong in the slot it
    /// is listed under and no slot may appear twice. The bag is not involved: a save stores
    /// worn items separately from the bag.
    pub fn from_items(items: impl IntoIterator<Item = (EquipSlot, ItemId)>) -> Result<Self, EquipError> {
        let mut equipment = Self::new();
        for (slot, item) in items {
            match item.equip_slot() {
                None => return Err(EquipError::NotEquippable(item)),
                Some(right) if right != slot => return Err(EquipError::WrongSlot(item, slot)),
                Some(_) => {}
            }
            if equipment.slots.insert(slot, item).is_some() {
                return Err(EquipError::SlotTaken(slot));
            }
        }
        Ok(equipment)
    }

    pub fn get(&self, slot: EquipSlot) -> Option<ItemId> {
        self.slots.get(&slot).copied()
    }

    /// Equipped items in slot order.
    pub fn iter(&self) -> impl Iterator<Item = (EquipSlot, ItemId)> + '_ {
        self.slots.iter().map(|(slot, item)| (*slot, *item))
    }

    pub fn is_empty(&self) -> bool {
        self.slots.is_empty()
    }

    /// Moves one `item` from the bag into its slot. Returns the item that was swapped out
    /// to the bag, if the slot was occupied.
    pub fn equip(&mut self, item: ItemId, bag: &mut Inventory) -> Result<Option<ItemId>, EquipError> {
        let slot = item.equip_slot().ok_or(EquipError::NotEquippable(item))?;
        if bag.count(item) == 0 {
            return Err(EquipError::NotInBag(item));
        }
        // Work on a copy so a failure leaves the real bag untouched.
        let mut trial = bag.clone();
        trial.remove(item, 1).map_err(|_| EquipError::NotInBag(item))?;
        let swapped = self.get(slot);
        if let Some(old) = swapped {
            trial.add(old, 1).map_err(|e| match e {
                InventoryError::Full => EquipError::BagFull,
                _ => EquipError::NotInBag(old),
            })?;
        }
        *bag = trial;
        self.slots.insert(slot, item);
        Ok(swapped)
    }

    /// Moves the item in `slot` back into the bag. Fails, changing nothing, if the bag is full.
    pub fn unequip(&mut self, slot: EquipSlot, bag: &mut Inventory) -> Result<ItemId, EquipError> {
        let item = self.get(slot).ok_or(EquipError::SlotEmpty(slot))?;
        bag.add(item, 1).map_err(|_| EquipError::BagFull)?;
        self.slots.remove(&slot);
        Ok(item)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bag_with(items: &[(ItemId, u32)], capacity: usize) -> Inventory {
        let mut bag = Inventory::new(capacity);
        for (item, n) in items {
            bag.add(*item, *n).unwrap();
        }
        bag
    }

    #[test]
    fn from_items_rebuilds_worn_gear() {
        let gear = Equipment::from_items([(EquipSlot::Hand, ItemId::WalkingStaff), (EquipSlot::Charm, ItemId::CopperCharm)]).unwrap();
        assert_eq!(gear.get(EquipSlot::Hand), Some(ItemId::WalkingStaff));
        assert_eq!(gear.get(EquipSlot::Charm), Some(ItemId::CopperCharm));
        assert_eq!(Equipment::from_items([]).unwrap(), Equipment::new());
    }

    #[test]
    fn from_items_refuses_impossible_gear() {
        assert_eq!(Equipment::from_items([(EquipSlot::Hand, ItemId::Timber)]), Err(EquipError::NotEquippable(ItemId::Timber)));
        assert_eq!(
            Equipment::from_items([(EquipSlot::Head, ItemId::WalkingStaff)]),
            Err(EquipError::WrongSlot(ItemId::WalkingStaff, EquipSlot::Head))
        );
        assert_eq!(
            Equipment::from_items([(EquipSlot::Hand, ItemId::WalkingStaff), (EquipSlot::Hand, ItemId::WalkingStaff)]),
            Err(EquipError::SlotTaken(EquipSlot::Hand))
        );
        assert!(!EquipError::WrongSlot(ItemId::WalkingStaff, EquipSlot::Head).message().is_empty());
        assert!(!EquipError::SlotTaken(EquipSlot::Hand).message().is_empty());
    }

    #[test]
    fn slot_keys_round_trip_and_are_unique() {
        for slot in EquipSlot::ALL {
            assert_eq!(EquipSlot::from_key(slot.key()), Some(slot));
        }
        assert_eq!(EquipSlot::from_key("Hand"), None);
        assert_eq!(EquipSlot::from_key(""), None);
        assert_eq!(EquipSlot::Charm.label(), "Charm");
    }

    #[test]
    fn equipping_moves_one_item_from_bag_to_slot() {
        let mut bag = bag_with(&[(ItemId::WalkingStaff, 1), (ItemId::Timber, 3)], 4);
        let mut gear = Equipment::new();
        assert_eq!(gear.equip(ItemId::WalkingStaff, &mut bag), Ok(None));
        assert_eq!(gear.get(EquipSlot::Hand), Some(ItemId::WalkingStaff));
        assert_eq!(bag.count(ItemId::WalkingStaff), 0);
        assert_eq!(bag.count(ItemId::Timber), 3);
        assert!(!gear.is_empty());
    }

    #[test]
    fn items_without_a_slot_cannot_be_equipped() {
        let mut bag = bag_with(&[(ItemId::Timber, 1)], 2);
        let mut gear = Equipment::new();
        assert_eq!(gear.equip(ItemId::Timber, &mut bag), Err(EquipError::NotEquippable(ItemId::Timber)));
        assert_eq!(bag.count(ItemId::Timber), 1);
        assert!(gear.is_empty());
    }

    #[test]
    fn items_not_in_the_bag_cannot_be_equipped() {
        let mut bag = Inventory::new(2);
        let mut gear = Equipment::new();
        assert_eq!(gear.equip(ItemId::CopperCharm, &mut bag), Err(EquipError::NotInBag(ItemId::CopperCharm)));
        assert!(gear.is_empty());
    }

    #[test]
    fn equipping_into_an_occupied_slot_swaps_even_with_a_full_bag() {
        let mut bag = bag_with(&[(ItemId::WalkingStaff, 2)], 2);
        let mut gear = Equipment::new();
        gear.equip(ItemId::WalkingStaff, &mut bag).unwrap();
        // Bag: one staff (1 of 2 slots used). Fill the other slot.
        bag.add(ItemId::Timber, 1).unwrap();
        assert!(bag.is_full());
        // Swapping staff for staff frees and refills one slot, so it works.
        assert_eq!(gear.equip(ItemId::WalkingStaff, &mut bag), Ok(Some(ItemId::WalkingStaff)));
        assert_eq!(bag.count(ItemId::WalkingStaff), 1);
        assert_eq!(bag.count(ItemId::Timber), 1);
        assert_eq!(gear.get(EquipSlot::Hand), Some(ItemId::WalkingStaff));
    }

    #[test]
    fn different_slots_do_not_interfere() {
        let mut bag = bag_with(&[(ItemId::WalkingStaff, 1), (ItemId::CopperCharm, 1)], 4);
        let mut gear = Equipment::new();
        gear.equip(ItemId::WalkingStaff, &mut bag).unwrap();
        gear.equip(ItemId::CopperCharm, &mut bag).unwrap();
        let worn: Vec<_> = gear.iter().collect();
        assert_eq!(worn, vec![(EquipSlot::Hand, ItemId::WalkingStaff), (EquipSlot::Charm, ItemId::CopperCharm)]);
        assert_eq!(bag.used_slots(), 0);
    }

    #[test]
    fn unequipping_returns_the_item_to_the_bag() {
        let mut bag = bag_with(&[(ItemId::CopperCharm, 1)], 2);
        let mut gear = Equipment::new();
        gear.equip(ItemId::CopperCharm, &mut bag).unwrap();
        assert_eq!(gear.unequip(EquipSlot::Charm, &mut bag), Ok(ItemId::CopperCharm));
        assert_eq!(bag.count(ItemId::CopperCharm), 1);
        assert_eq!(gear.get(EquipSlot::Charm), None);
    }

    #[test]
    fn unequipping_with_a_full_bag_changes_nothing() {
        let mut bag = bag_with(&[(ItemId::CopperCharm, 1)], 1);
        let mut gear = Equipment::new();
        gear.equip(ItemId::CopperCharm, &mut bag).unwrap();
        bag.add(ItemId::Timber, 1).unwrap();
        let before = bag.clone();
        assert_eq!(gear.unequip(EquipSlot::Charm, &mut bag), Err(EquipError::BagFull));
        assert_eq!(gear.get(EquipSlot::Charm), Some(ItemId::CopperCharm));
        assert_eq!(bag, before);
    }

    #[test]
    fn unequipping_an_empty_slot_is_an_error() {
        let mut bag = Inventory::new(1);
        let mut gear = Equipment::new();
        assert_eq!(gear.unequip(EquipSlot::Head, &mut bag), Err(EquipError::SlotEmpty(EquipSlot::Head)));
    }

    #[test]
    fn equipping_never_fails_for_lack_of_room() {
        // Equipment does not stack, so taking one out of the bag always frees the slot the
        // swapped-out item needs. This is why `equip` has no room check of its own to fail.
        for item in ItemId::ALL.into_iter().filter(|i| i.equip_slot().is_some()) {
            let mut bag = Inventory::new(1);
            bag.add(item, 1).unwrap();
            let mut gear = Equipment::new();
            gear.equip(item, &mut bag).unwrap();
            bag.add(item, 1).unwrap();
            assert!(bag.is_full());
            assert_eq!(gear.equip(item, &mut bag), Ok(Some(item)), "{item:?}");
            assert_eq!(bag.count(item), 1);
        }
    }

    #[test]
    fn error_messages_are_plain_language() {
        assert_eq!(EquipError::NotEquippable(ItemId::Timber).message(), "You cannot equip the timber.");
        assert_eq!(EquipError::NotInBag(ItemId::CopperCharm).message(), "You are not carrying a copper charm.");
        assert_eq!(EquipError::SlotEmpty(EquipSlot::Hand).message(), "Nothing is equipped in your hand slot.");
        assert!(EquipError::BagFull.message().contains("full"));
    }
}
