use crate::interaction::InteractionKind;
use crate::inventory::{Inventory, InventoryError, ItemId};
use crate::world::{MapError, Tile, TilePos, WorldMap};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResourceKind { Moonpetal, Tree, CopperVein }

impl ResourceKind {
    pub fn item(self) -> ItemId {
        match self { Self::Moonpetal => ItemId::Moonpetal, Self::Tree => ItemId::Timber, Self::CopperVein => ItemId::CopperOre }
    }
}

impl ResourceKind {
    /// Lower-case name used in hover text and journal messages.
    pub fn label(self) -> &'static str {
        match self { Self::Moonpetal => "moonpetal", Self::Tree => "tree", Self::CopperVein => "copper vein" }
    }
    /// What clicking this resource does.
    pub fn interaction(self) -> InteractionKind {
        match self { Self::Moonpetal => InteractionKind::Pick, Self::Tree => InteractionKind::ChopDown, Self::CopperVein => InteractionKind::Mine }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GatherError { Depleted, Inventory(InventoryError) }

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GatheringNode { kind: ResourceKind, remaining: u32, yield_per_action: u32 }

impl GatheringNode {
    pub fn new(kind: ResourceKind, available_actions: u32, yield_per_action: u32) -> Self {
        Self { kind, remaining: available_actions, yield_per_action }
    }
    pub fn kind(&self) -> ResourceKind { self.kind }
    pub fn remaining_actions(&self) -> u32 { self.remaining }
    pub fn is_depleted(&self) -> bool { self.remaining == 0 }

    /// Adds resources to the inventory and only consumes a node on success.
    pub fn gather(&mut self, inventory: &mut Inventory) -> Result<u32, GatherError> {
        if self.is_depleted() { return Err(GatherError::Depleted); }
        if self.yield_per_action == 0 { return Err(GatherError::Depleted); }
        inventory.add(self.kind.item(), self.yield_per_action).map_err(GatherError::Inventory)?;
        self.remaining -= 1;
        Ok(self.yield_per_action)
    }
}

/// Whether an automatic "keep gathering" loop should continue after an attempt: only while
/// attempts succeed and the node still has something to give. A full inventory or a spent
/// node ends the loop.
pub fn should_keep_gathering(outcome: &Result<u32, GatherError>, node: &GatheringNode) -> bool {
    outcome.is_ok() && !node.is_depleted()
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PlacementError {
    /// The position is not on the map.
    OutOfBounds,
    /// The tile under the node does not suit this resource.
    WrongTile { expected: &'static str, found: Tile },
}

/// Checks that a node is placed on a tile that makes sense for it: trees on tree tiles,
/// copper veins on rock tiles, and moonpetals on walkable ground (so they can be reached).
pub fn validate_node_placement(map: &WorldMap, pos: TilePos, kind: ResourceKind) -> Result<(), PlacementError> {
    let found = map.tile_at(pos).map_err(|_: MapError| PlacementError::OutOfBounds)?;
    let (ok, expected) = match kind {
        ResourceKind::Tree => (found == Tile::Tree, "tree"),
        ResourceKind::CopperVein => (found == Tile::Rock, "rock"),
        ResourceKind::Moonpetal => (found.is_walkable(), "walkable ground"),
    };
    if ok { Ok(()) } else { Err(PlacementError::WrongTile { expected, found }) }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn successful_gathering_adds_yield_and_depletes_node() {
        let mut node = GatheringNode::new(ResourceKind::Tree, 2, 3);
        let mut bag = Inventory::new(4);
        assert_eq!(node.gather(&mut bag), Ok(3));
        assert_eq!(bag.count(ItemId::Timber), 3);
        assert_eq!(node.remaining_actions(), 1);
        node.gather(&mut bag).unwrap();
        assert!(node.is_depleted());
        assert_eq!(node.gather(&mut bag), Err(GatherError::Depleted));
    }
    #[test]
    fn failed_inventory_add_does_not_consume_resource() {
        let mut node = GatheringNode::new(ResourceKind::CopperVein, 1, 1);
        let mut bag = Inventory::new(0);
        assert_eq!(node.gather(&mut bag), Err(GatherError::Inventory(InventoryError::Full)));
        assert_eq!(node.remaining_actions(), 1);
        assert_eq!(bag.count(ItemId::CopperOre), 0);
    }
    #[test]
    fn zero_yield_is_not_a_successful_gather() {
        let mut node = GatheringNode::new(ResourceKind::Moonpetal, 1, 0);
        let mut bag = Inventory::default();
        assert_eq!(node.gather(&mut bag), Err(GatherError::Depleted));
        assert_eq!(node.remaining_actions(), 1);
    }
    #[test]
    fn node_placement_matches_tile_type() {
        let map = WorldMap::from_ascii("T#.~").unwrap();
        assert_eq!(validate_node_placement(&map, TilePos::new(0, 0), ResourceKind::Tree), Ok(()));
        assert_eq!(validate_node_placement(&map, TilePos::new(1, 0), ResourceKind::CopperVein), Ok(()));
        assert_eq!(validate_node_placement(&map, TilePos::new(2, 0), ResourceKind::Moonpetal), Ok(()));
        assert_eq!(
            validate_node_placement(&map, TilePos::new(2, 0), ResourceKind::Tree),
            Err(PlacementError::WrongTile { expected: "tree", found: Tile::Grass })
        );
        assert_eq!(
            validate_node_placement(&map, TilePos::new(3, 0), ResourceKind::Moonpetal),
            Err(PlacementError::WrongTile { expected: "walkable ground", found: Tile::Water })
        );
        assert_eq!(validate_node_placement(&map, TilePos::new(9, 9), ResourceKind::Tree), Err(PlacementError::OutOfBounds));
    }
    #[test]
    fn node_reports_its_kind() {
        assert_eq!(GatheringNode::new(ResourceKind::Tree, 1, 1).kind(), ResourceKind::Tree);
    }
    #[test]
    fn gathering_loop_continues_until_spent_or_blocked() {
        let mut bag = Inventory::new(4);
        let mut node = GatheringNode::new(ResourceKind::Tree, 2, 1);
        let first = node.gather(&mut bag);
        assert!(should_keep_gathering(&first, &node), "one action left");
        let second = node.gather(&mut bag);
        assert!(!should_keep_gathering(&second, &node), "just spent");
        let third = node.gather(&mut bag);
        assert!(!should_keep_gathering(&third, &node), "already depleted");

        let mut full = Inventory::new(0);
        let mut ore = GatheringNode::new(ResourceKind::CopperVein, 3, 1);
        let blocked = ore.gather(&mut full);
        assert!(!should_keep_gathering(&blocked, &ore), "full inventory stops the loop");
    }
    #[test]
    fn resources_know_their_label_and_click_action() {
        assert_eq!(ResourceKind::Tree.label(), "tree");
        assert_eq!(ResourceKind::CopperVein.interaction(), InteractionKind::Mine);
        assert_eq!(ResourceKind::Tree.interaction(), InteractionKind::ChopDown);
        assert_eq!(ResourceKind::Moonpetal.interaction(), InteractionKind::Pick);
    }
}
