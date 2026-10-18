use std::collections::BTreeMap;

use crate::equipment::EquipSlot;

/// Stable, data-oriented item identifiers. Display names belong in the UI layer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ItemId {
    Moonpetal,
    Timber,
    CopperOre,
    /// A wild herb gathered from meadow herb patches.
    Meadowmint,
    /// A berry picked from wild bushes.
    Honeyberry,
    /// Crafted: a drink made from herbs and berries.
    MintTea,
    /// Crafted equipment for the `Hand` slot.
    WalkingStaff,
    /// Crafted equipment for the `Charm` slot.
    CopperCharm,
}

/// How many items fit in one inventory slot unless an item says otherwise.
pub const DEFAULT_STACK: u32 = 99;

impl ItemId {
    /// Every item, in a fixed order (used for key lookup, tests, and listings).
    pub const ALL: [ItemId; 8] = [
        Self::Moonpetal,
        Self::Timber,
        Self::CopperOre,
        Self::Meadowmint,
        Self::Honeyberry,
        Self::MintTea,
        Self::WalkingStaff,
        Self::CopperCharm,
    ];

    /// Stable lower-case key used in data files and saves. Never change an existing key.
    pub fn key(self) -> &'static str {
        match self {
            Self::Moonpetal => "moonpetal",
            Self::Timber => "timber",
            Self::CopperOre => "copper_ore",
            Self::Meadowmint => "meadowmint",
            Self::Honeyberry => "honeyberry",
            Self::MintTea => "mint_tea",
            Self::WalkingStaff => "walking_staff",
            Self::CopperCharm => "copper_charm",
        }
    }

    /// Reverse of [`ItemId::key`]; unknown keys give `None`.
    pub fn from_key(key: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|item| item.key() == key)
    }

    /// Plain-language name for journal and quest text.
    pub fn label(self) -> &'static str {
        match self {
            Self::Moonpetal => "moonpetal",
            Self::Timber => "timber",
            Self::CopperOre => "copper ore",
            Self::Meadowmint => "meadowmint",
            Self::Honeyberry => "honeyberry",
            Self::MintTea => "mint tea",
            Self::WalkingStaff => "walking staff",
            Self::CopperCharm => "copper charm",
        }
    }

    /// How many of this item share one slot. Equipment is never stacked.
    pub fn max_stack(self) -> u32 {
        if self.equip_slot().is_some() { 1 } else { DEFAULT_STACK }
    }

    /// The equipment slot this item is worn or held in, or `None` for ordinary items.
    pub fn equip_slot(self) -> Option<EquipSlot> {
        match self {
            Self::WalkingStaff => Some(EquipSlot::Hand),
            Self::CopperCharm => Some(EquipSlot::Charm),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InventoryError {
    ZeroQuantity,
    Full,
    NotEnoughItems,
}

/// Slots needed to hold `count` of `item`.
fn slots_for(item: ItemId, count: u32) -> usize {
    count.div_ceil(item.max_stack()) as usize
}

/// A bag with a fixed number of slots. One slot holds up to [`ItemId::max_stack`] of a single
/// item, so 150 timber take two slots while a walking staff always takes one slot each.
///
/// Rules (all checked before anything changes, so a failed call never alters the bag):
/// - `add` succeeds only if the slots needed after the add still fit the capacity. Adding to
///   a partly filled stack uses its spare room first.
/// - `remove` needs the full quantity on hand and frees slots as stacks empty.
/// - Quantities of zero are rejected.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Inventory {
    capacity: usize,
    /// Total on hand per item; the slot layout follows from the totals.
    stacks: BTreeMap<ItemId, u32>,
}

impl Inventory {
    pub fn new(capacity: usize) -> Self {
        Self { capacity, stacks: BTreeMap::new() }
    }

    /// Rebuilds a bag from saved totals (see `save.rs`). Every total must be positive and the
    /// slots they need must fit `capacity`; the same item listed twice is added up like any
    /// other pickup. Nothing is returned on failure, so a bad save never yields a bag that
    /// play could not have produced.
    pub fn from_stacks(capacity: usize, stacks: impl IntoIterator<Item = (ItemId, u32)>) -> Result<Self, InventoryError> {
        let mut bag = Self::new(capacity);
        for (item, quantity) in stacks {
            bag.add(item, quantity)?;
        }
        Ok(bag)
    }

    pub fn capacity(&self) -> usize { self.capacity }
    /// Slots in use (a stack of 150 timber counts as two).
    pub fn used_slots(&self) -> usize { self.stacks.iter().map(|(item, count)| slots_for(*item, *count)).sum() }
    pub fn free_slots(&self) -> usize { self.capacity.saturating_sub(self.used_slots()) }
    /// True when no slot is free. A full bag can still take more of an item whose last stack has room.
    pub fn is_full(&self) -> bool { self.used_slots() >= self.capacity }
    pub fn count(&self, item: ItemId) -> u32 { self.stacks.get(&item).copied().unwrap_or(0) }
    /// Total on hand per item.
    pub fn stacks(&self) -> &BTreeMap<ItemId, u32> { &self.stacks }

    /// Whether `add(item, quantity)` would succeed, without changing anything.
    pub fn can_add(&self, item: ItemId, quantity: u32) -> Result<(), InventoryError> {
        if quantity == 0 { return Err(InventoryError::ZeroQuantity); }
        let current = self.count(item);
        let new_total = current.checked_add(quantity).ok_or(InventoryError::Full)?;
        let needed = self.used_slots() - slots_for(item, current) + slots_for(item, new_total);
        if needed > self.capacity { Err(InventoryError::Full) } else { Ok(()) }
    }

    pub fn add(&mut self, item: ItemId, quantity: u32) -> Result<(), InventoryError> {
        self.can_add(item, quantity)?;
        *self.stacks.entry(item).or_insert(0) += quantity;
        Ok(())
    }

    pub fn remove(&mut self, item: ItemId, quantity: u32) -> Result<(), InventoryError> {
        if quantity == 0 { return Err(InventoryError::ZeroQuantity); }
        let current = self.count(item);
        if current < quantity { return Err(InventoryError::NotEnoughItems); }
        if current == quantity { self.stacks.remove(&item); }
        else { self.stacks.insert(item, current - quantity); }
        Ok(())
    }
}

impl Default for Inventory {
    fn default() -> Self { Self::new(20) }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn from_stacks_rebuilds_a_bag_and_adds_repeated_items() {
        let bag = Inventory::from_stacks(5, [(ItemId::Timber, 150), (ItemId::Moonpetal, 2), (ItemId::Moonpetal, 1)]).unwrap();
        assert_eq!(bag.count(ItemId::Timber), 150);
        assert_eq!(bag.count(ItemId::Moonpetal), 3);
        assert_eq!(bag.used_slots(), 3);
        assert_eq!(bag.capacity(), 5);
    }

    #[test]
    fn from_stacks_refuses_overfull_and_zero_totals() {
        assert_eq!(Inventory::from_stacks(1, [(ItemId::Timber, 1), (ItemId::CopperOre, 1)]), Err(InventoryError::Full));
        assert_eq!(Inventory::from_stacks(1, [(ItemId::Timber, 100)]), Err(InventoryError::Full));
        assert_eq!(Inventory::from_stacks(3, [(ItemId::Timber, 0)]), Err(InventoryError::ZeroQuantity));
        assert_eq!(Inventory::from_stacks(0, []).unwrap().used_slots(), 0);
    }

    #[test]
    fn stacks_same_item_in_one_slot() {
        let mut bag = Inventory::new(1);
        bag.add(ItemId::Moonpetal, 2).unwrap();
        bag.add(ItemId::Moonpetal, 4).unwrap();
        assert_eq!(bag.count(ItemId::Moonpetal), 6);
        assert_eq!(bag.used_slots(), 1);
    }

    #[test]
    fn refuses_new_item_when_capacity_is_full() {
        let mut bag = Inventory::new(1);
        bag.add(ItemId::Timber, 1).unwrap();
        assert_eq!(bag.add(ItemId::CopperOre, 1), Err(InventoryError::Full));
        assert_eq!(bag.count(ItemId::CopperOre), 0);
    }

    #[test]
    fn removing_last_item_frees_slot() {
        let mut bag = Inventory::new(1);
        bag.add(ItemId::Timber, 2).unwrap();
        bag.remove(ItemId::Timber, 2).unwrap();
        assert_eq!(bag.used_slots(), 0);
        bag.add(ItemId::CopperOre, 1).unwrap();
    }

    #[test]
    fn invalid_quantities_and_shortages_are_rejected_without_mutation() {
        let mut bag = Inventory::new(2);
        assert_eq!(bag.add(ItemId::Moonpetal, 0), Err(InventoryError::ZeroQuantity));
        bag.add(ItemId::Moonpetal, 2).unwrap();
        assert_eq!(bag.remove(ItemId::Moonpetal, 0), Err(InventoryError::ZeroQuantity));
        assert_eq!(bag.remove(ItemId::Moonpetal, 3), Err(InventoryError::NotEnoughItems));
        assert_eq!(bag.count(ItemId::Moonpetal), 2);
    }

    #[test]
    fn item_keys_round_trip_and_reject_unknowns() {
        for item in ItemId::ALL {
            assert_eq!(ItemId::from_key(item.key()), Some(item));
        }
        assert_eq!(ItemId::from_key("Moonpetal"), None);
        assert_eq!(ItemId::from_key(""), None);
        assert_eq!(ItemId::CopperOre.label(), "copper ore");
    }

    #[test]
    fn every_item_has_a_unique_key_and_label_and_is_listed_once() {
        let mut keys: Vec<_> = ItemId::ALL.iter().map(|i| i.key()).collect();
        let mut labels: Vec<_> = ItemId::ALL.iter().map(|i| i.label()).collect();
        keys.sort_unstable();
        labels.sort_unstable();
        keys.dedup();
        labels.dedup();
        assert_eq!(keys.len(), ItemId::ALL.len());
        assert_eq!(labels.len(), ItemId::ALL.len());
        for key in keys {
            assert!(crate::textdata::is_valid_id(key), "{key} must be usable in data files");
        }
    }

    #[test]
    fn existing_item_keys_never_change() {
        // Saved games and content files rely on these exact strings.
        assert_eq!(ItemId::Moonpetal.key(), "moonpetal");
        assert_eq!(ItemId::Timber.key(), "timber");
        assert_eq!(ItemId::CopperOre.key(), "copper_ore");
    }

    #[test]
    fn equipment_is_never_stackable_and_other_items_stack() {
        for item in ItemId::ALL {
            if item.equip_slot().is_some() {
                assert_eq!(item.max_stack(), 1, "{item:?}");
            } else {
                assert_eq!(item.max_stack(), DEFAULT_STACK, "{item:?}");
            }
        }
    }

    #[test]
    fn a_full_stack_spills_into_a_new_slot() {
        let mut bag = Inventory::new(2);
        bag.add(ItemId::Timber, DEFAULT_STACK).unwrap();
        assert_eq!(bag.used_slots(), 1);
        bag.add(ItemId::Timber, 1).unwrap();
        assert_eq!(bag.used_slots(), 2);
        assert_eq!(bag.count(ItemId::Timber), DEFAULT_STACK + 1);
    }

    #[test]
    fn spare_room_in_the_last_stack_is_used_before_a_new_slot() {
        let mut bag = Inventory::new(1);
        bag.add(ItemId::Timber, 50).unwrap();
        bag.add(ItemId::Timber, 49).unwrap();
        assert_eq!(bag.used_slots(), 1);
        assert!(bag.is_full());
        assert_eq!(bag.add(ItemId::Timber, 1), Err(InventoryError::Full), "slot is full and no slot is spare");
        assert_eq!(bag.count(ItemId::Timber), 99);
    }

    #[test]
    fn a_big_add_that_needs_too_many_slots_changes_nothing() {
        let mut bag = Inventory::new(2);
        bag.add(ItemId::Timber, 10).unwrap();
        assert_eq!(bag.add(ItemId::Timber, 200), Err(InventoryError::Full), "needs 3 slots");
        assert_eq!(bag.count(ItemId::Timber), 10);
        assert_eq!(bag.used_slots(), 1);
        bag.add(ItemId::Timber, 188).unwrap();
        assert_eq!(bag.used_slots(), 2, "exactly two slots is fine");
    }

    #[test]
    fn equipment_takes_one_slot_each() {
        let mut bag = Inventory::new(2);
        bag.add(ItemId::WalkingStaff, 1).unwrap();
        bag.add(ItemId::WalkingStaff, 1).unwrap();
        assert_eq!(bag.used_slots(), 2);
        assert_eq!(bag.add(ItemId::WalkingStaff, 1), Err(InventoryError::Full));
        assert_eq!(bag.add(ItemId::Timber, 1), Err(InventoryError::Full));
        assert_eq!(bag.count(ItemId::WalkingStaff), 2);
    }

    #[test]
    fn removing_part_of_a_spilled_stack_frees_a_slot() {
        let mut bag = Inventory::new(3);
        bag.add(ItemId::Timber, 150).unwrap();
        assert_eq!(bag.used_slots(), 2);
        bag.remove(ItemId::Timber, 60).unwrap();
        assert_eq!(bag.count(ItemId::Timber), 90);
        assert_eq!(bag.used_slots(), 1);
        assert_eq!(bag.free_slots(), 2);
    }

    #[test]
    fn can_add_matches_add_and_never_mutates() {
        let mut bag = Inventory::new(1);
        bag.add(ItemId::Moonpetal, 1).unwrap();
        let before = bag.clone();
        assert_eq!(bag.can_add(ItemId::Moonpetal, 5), Ok(()));
        assert_eq!(bag.can_add(ItemId::Timber, 1), Err(InventoryError::Full));
        assert_eq!(bag.can_add(ItemId::Timber, 0), Err(InventoryError::ZeroQuantity));
        assert_eq!(bag, before);
    }

    #[test]
    fn overflowing_quantities_are_refused_not_wrapped() {
        // u32::MAX / 99 is about 43.4 million slots, so this bag really can hold the pile.
        let mut bag = Inventory::new(50_000_000);
        bag.add(ItemId::Timber, u32::MAX - 1).unwrap();
        let before = bag.clone();
        assert_eq!(bag.can_add(ItemId::Timber, 2), Err(InventoryError::Full));
        assert_eq!(bag.add(ItemId::Timber, 2), Err(InventoryError::Full));
        assert_eq!(bag, before);
    }

    #[test]
    fn a_zero_slot_bag_takes_nothing() {
        let mut bag = Inventory::new(0);
        assert!(bag.is_full());
        assert_eq!(bag.add(ItemId::Moonpetal, 1), Err(InventoryError::Full));
        assert_eq!(bag.free_slots(), 0);
    }
}
