//! End-to-end check of the M3 acceptance criterion: inventory rules are deterministic and
//! covered without rendering. A player forages and gathers, hits a full bag, makes room,
//! crafts every shipped recipe, and equips the results, through the public API alone.

use ozbound::crafting::{CraftError, RecipeBook};
use ozbound::equipment::{EquipSlot, Equipment};
use ozbound::gathering::{GatherError, GatheringNode, ResourceKind};
use ozbound::inventory::{Inventory, InventoryError, ItemId};
use ozbound::level::Level;
use ozbound::pack::{self, PackAction};
use ozbound::scene::DEFAULT_LEVEL;

struct Game {
    book: RecipeBook,
    bag: Inventory,
    gear: Equipment,
}

impl Game {
    fn new(slots: usize) -> Self {
        Self { book: RecipeBook::builtin().unwrap(), bag: Inventory::new(slots), gear: Equipment::new() }
    }
    fn gather_all(&mut self, node: &mut GatheringNode) -> u32 {
        let mut total = 0;
        while let Ok(n) = node.gather(&mut self.bag) {
            total += n;
        }
        total
    }
    fn act(&mut self, action: PackAction) -> String {
        pack::apply(&action, &self.book, &mut self.bag, &mut self.gear)
    }
}

#[test]
fn forage_craft_and_equip_from_nothing() {
    let mut game = Game::new(6);

    // Gather the ingredients with real nodes.
    let mut herbs = GatheringNode::new(ResourceKind::HerbPatch, 3, 1);
    let mut berries = GatheringNode::new(ResourceKind::BerryBush, 3, 1);
    let mut tree = GatheringNode::new(ResourceKind::Tree, 3, 1);
    let mut ore = GatheringNode::new(ResourceKind::CopperVein, 2, 1);
    assert_eq!(game.gather_all(&mut herbs), 3);
    assert_eq!(game.gather_all(&mut berries), 3);
    assert_eq!(game.gather_all(&mut tree), 3);
    assert_eq!(game.gather_all(&mut ore), 2);
    assert!(herbs.is_depleted() && berries.is_depleted() && tree.is_depleted() && ore.is_depleted());
    assert_eq!(game.bag.used_slots(), 4);

    // Nothing is craftable by accident: each craft uses exactly its inputs.
    assert_eq!(game.act(PackAction::Craft("mint_tea".into())), "You made 1 mint tea.");
    assert_eq!((game.bag.count(ItemId::Meadowmint), game.bag.count(ItemId::Honeyberry)), (1, 2));
    assert_eq!(game.act(PackAction::Craft("walking_staff".into())), "You made 1 walking staff.");
    assert_eq!(game.bag.count(ItemId::Timber), 0);
    assert_eq!(game.act(PackAction::Craft("copper_charm".into())), "You still need 1 more timber.");
    assert_eq!(game.bag.count(ItemId::CopperOre), 2, "a failed craft keeps its inputs");

    // Equip the staff; it leaves the bag.
    assert_eq!(game.act(PackAction::Equip(ItemId::WalkingStaff)), "You equip the walking staff.");
    assert_eq!(game.gear.get(EquipSlot::Hand), Some(ItemId::WalkingStaff));
    assert_eq!(game.bag.count(ItemId::WalkingStaff), 0);
    assert_eq!(pack::equipment_line(&game.gear), "Equipped: hand: walking staff");
}

#[test]
fn full_bag_and_depleted_node_rules_hold_together() {
    // A one-slot bag holding timber is full for everything except more timber.
    let mut game = Game::new(1);
    game.bag.add(ItemId::Timber, 1).unwrap();

    // A full bag never consumes a node...
    let mut herbs = GatheringNode::new(ResourceKind::HerbPatch, 2, 1);
    assert_eq!(herbs.gather(&mut game.bag), Err(GatherError::Inventory(InventoryError::Full)));
    assert_eq!(herbs.remaining_actions(), 2);

    // ...but the open stack still takes more of its own item.
    let mut tree = GatheringNode::new(ResourceKind::Tree, 5, 1);
    assert_eq!(tree.gather(&mut game.bag), Ok(1));
    assert_eq!(game.bag.count(ItemId::Timber), 2);

    // A spent node says so, whatever the bag looks like, and comes back after a restock.
    let mut spent = GatheringNode::new(ResourceKind::BerryBush, 1, 1);
    let mut roomy = Inventory::new(3);
    spent.gather(&mut roomy).unwrap();
    assert_eq!(spent.gather(&mut Inventory::new(0)), Err(GatherError::Depleted));
    assert!(spent.restock());
    assert_eq!(spent.gather(&mut roomy), Ok(1));
    assert_eq!(roomy.count(ItemId::Honeyberry), 2);
}

#[test]
fn crafting_frees_slots_it_needs_but_never_loses_items_when_it_cannot() {
    let mut game = Game::new(2);
    game.bag.add(ItemId::Timber, 4).unwrap();
    game.bag.add(ItemId::Moonpetal, 1).unwrap();
    // 4 timber leaves 1 after the craft, so no slot is freed and the staff has no room.
    assert_eq!(game.act(PackAction::Craft("walking_staff".into())), CraftError::NoRoom.message());
    assert_eq!((game.bag.count(ItemId::Timber), game.bag.count(ItemId::WalkingStaff)), (4, 0));
    // Dropping a stack elsewhere (here: spending the petal) makes room and the same craft works.
    game.bag.remove(ItemId::Moonpetal, 1).unwrap();
    assert_eq!(game.act(PackAction::Craft("walking_staff".into())), "You made 1 walking staff.");
    assert_eq!((game.bag.count(ItemId::Timber), game.bag.count(ItemId::WalkingStaff)), (1, 1));
}

#[test]
fn removing_gear_needs_room_and_swapping_does_not() {
    let mut game = Game::new(2);
    game.bag.add(ItemId::WalkingStaff, 2).unwrap();
    game.act(PackAction::Equip(ItemId::WalkingStaff));
    game.bag.add(ItemId::Timber, 1).unwrap();
    assert!(game.bag.is_full());
    // Taking it off with no room is refused and the staff stays in hand.
    assert_eq!(game.act(PackAction::Unequip(EquipSlot::Hand)), "Your inventory is full. Make room first.");
    assert_eq!(game.gear.get(EquipSlot::Hand), Some(ItemId::WalkingStaff));
    // Swapping works in a full bag because equipping frees the slot the old item takes.
    assert!(game.act(PackAction::Equip(ItemId::WalkingStaff)).contains("put the walking staff back"));
    assert_eq!(game.bag.count(ItemId::WalkingStaff), 1);
}

#[test]
fn every_shipped_recipe_is_reachable_from_items_the_meadow_provides() {
    let level = Level::parse(DEFAULT_LEVEL).unwrap();
    let provides = |item: ItemId| level.nodes.iter().any(|n| n.kind.item() == item && n.actions * n.yield_per_action > 0);
    let book = RecipeBook::builtin().unwrap();
    for recipe in book.all() {
        for input in &recipe.inputs {
            assert!(provides(input.item), "{}: nothing in the meadow gives {}", recipe.id, input.item.label());
            // A single node's full supply must cover a single craft only if no other node does;
            // here we just require the meadow's total supply to cover one craft.
            let supply: u32 = level
                .nodes
                .iter()
                .filter(|n| n.kind.item() == input.item)
                .map(|n| n.actions * n.yield_per_action)
                .sum();
            assert!(supply >= input.quantity, "{}: only {supply} {} in the meadow", recipe.id, input.item.label());
        }
    }
}

#[test]
fn the_whole_meadow_supply_can_be_carried_and_crafted_in_a_default_bag() {
    // Gather every node in the level once, then make one of everything.
    let level = Level::parse(DEFAULT_LEVEL).unwrap();
    let mut game = Game::new(20);
    for spawn in &level.nodes {
        let mut node = GatheringNode::new(spawn.kind, spawn.actions, spawn.yield_per_action);
        game.gather_all(&mut node);
    }
    let ids: Vec<String> = game.book.all().iter().map(|r| r.id.clone()).collect();
    for id in ids {
        let msg = game.act(PackAction::Craft(id.clone()));
        assert!(msg.starts_with("You made"), "{id}: {msg}");
    }
    for (item, slot) in [(ItemId::WalkingStaff, EquipSlot::Hand), (ItemId::CopperCharm, EquipSlot::Charm)] {
        game.act(PackAction::Equip(item));
        assert_eq!(game.gear.get(slot), Some(item));
    }
    assert_eq!(game.bag.count(ItemId::MintTea), 1);
}
