use std::collections::BTreeMap;

/// Stable, data-oriented item identifiers. Display names belong in the UI layer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ItemId {
    Moonpetal,
    Timber,
    CopperOre,
}

impl ItemId {
    /// Stable lower-case key used in data files and saves. Never change an existing key.
    pub fn key(self) -> &'static str {
        match self {
            Self::Moonpetal => "moonpetal",
            Self::Timber => "timber",
            Self::CopperOre => "copper_ore",
        }
    }

    /// Reverse of [`ItemId::key`]; unknown keys give `None`.
    pub fn from_key(key: &str) -> Option<Self> {
        [Self::Moonpetal, Self::Timber, Self::CopperOre].into_iter().find(|item| item.key() == key)
    }

    /// Plain-language name for journal and quest text.
    pub fn label(self) -> &'static str {
        match self {
            Self::Moonpetal => "moonpetal",
            Self::Timber => "timber",
            Self::CopperOre => "copper ore",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InventoryError {
    ZeroQuantity,
    Full,
    NotEnoughItems,
}

/// Inventory capacity counts occupied slots, not total item quantity.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Inventory {
    capacity: usize,
    stacks: BTreeMap<ItemId, u32>,
}

impl Inventory {
    pub fn new(capacity: usize) -> Self {
        Self { capacity, stacks: BTreeMap::new() }
    }

    pub fn capacity(&self) -> usize { self.capacity }
    pub fn used_slots(&self) -> usize { self.stacks.len() }
    pub fn is_full(&self) -> bool { self.used_slots() >= self.capacity }
    pub fn count(&self, item: ItemId) -> u32 { self.stacks.get(&item).copied().unwrap_or(0) }
    pub fn stacks(&self) -> &BTreeMap<ItemId, u32> { &self.stacks }

    pub fn add(&mut self, item: ItemId, quantity: u32) -> Result<(), InventoryError> {
        if quantity == 0 { return Err(InventoryError::ZeroQuantity); }
        if !self.stacks.contains_key(&item) && self.is_full() { return Err(InventoryError::Full); }
        let current = self.count(item);
        let new_total = current.checked_add(quantity).ok_or(InventoryError::Full)?;
        self.stacks.insert(item, new_total);
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
        for item in [ItemId::Moonpetal, ItemId::Timber, ItemId::CopperOre] {
            assert_eq!(ItemId::from_key(item.key()), Some(item));
        }
        assert_eq!(ItemId::from_key("Moonpetal"), None);
        assert_eq!(ItemId::from_key(""), None);
        assert_eq!(ItemId::CopperOre.label(), "copper ore");
    }
}
