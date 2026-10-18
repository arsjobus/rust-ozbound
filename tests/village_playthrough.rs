//! End-to-end check of the M5 acceptance paths: every shipped Hollowmere quest is offered,
//! advanced by real world events, and handed in using only the public API (no Bevy). It also
//! walks the real routes on the real map, so a place or creature that cannot be reached
//! on foot fails here.

use ozbound::bestiary::{approach, Encounter, FieldNotes};
use ozbound::gathering::GatheringNode;
use ozbound::inventory::{Inventory, ItemId};
use ozbound::level::GUIDE_ID;
use ozbound::pathfinding::{plan_route, Approach};
use ozbound::quests::{QuestEvent, QuestLog, QuestStatus};
use ozbound::skills::{SkillId, Skills};
use ozbound::talk::Talk;
use ozbound::village::{builtin, World};
use ozbound::world::TilePos;

struct Game {
    world: World,
    log: QuestLog,
    bag: Inventory,
    skills: Skills,
    notes: FieldNotes,
    here: TilePos,
}

impl Game {
    fn new() -> Self {
        let world = builtin().expect("the shipped world loads");
        let here = world.level.player_start;
        Self { world, log: QuestLog::new(), bag: Inventory::default(), skills: Skills::default(), notes: FieldNotes::new(), here }
    }

    /// Opens a conversation, picks the choices whose text contains each fragment in turn, and
    /// returns the first thing said. Panics (naming the fragment) if a choice is not offered.
    fn talk(&mut self, npc: &str, picks: &[&str]) -> String {
        let (mut talk, _) = Talk::start(&self.world.content, &mut self.log, &mut self.bag, &self.skills, npc).expect("they have something to say");
        let first = talk.view().text.clone();
        for fragment in picks {
            let index = talk
                .view()
                .choices
                .iter()
                .position(|c| c.contains(fragment))
                .unwrap_or_else(|| panic!("no choice containing {fragment:?} in {:?}", talk.view().choices));
            talk.choose(&self.world.content, &mut self.log, &mut self.bag, &self.skills, index);
        }
        first
    }

    /// Walks to `target` along the real planned route and counts the steps for Wayfinding.
    fn walk_to(&mut self, target: TilePos, approach: Approach) {
        let route = plan_route(&self.world.level.map, self.here, target, approach).unwrap_or_else(|| panic!("no route to {target:?}"));
        self.skills.walked(route.len() as u32);
        self.here = *route.last().unwrap_or(&self.here);
    }

    /// Gathers `n` of an item from the nearest-listed node of that kind, as the game does.
    fn gather(&mut self, item: ItemId, n: u32) {
        for _ in 0..n {
            let spawn = self.world.level.nodes.iter().find(|s| s.kind.item() == item).expect("the world has such a node");
            let mut node = GatheringNode::new(spawn.kind, spawn.actions, spawn.yield_per_action);
            let got = node.gather(&mut self.bag).expect("the bag has room");
            self.log.record(&self.world.content.quests, &QuestEvent::Gathered { item, quantity: got });
        }
    }

    /// Walks the real map back and forth (start to the standing stone and back) until
    /// Wayfinding reaches level 2, and returns how many tiles that took.
    fn learn_the_roads(&mut self) -> u32 {
        let stone = self.world.level.places.iter().find(|p| p.id == "standing_stone").unwrap().tile;
        let start = self.world.level.player_start;
        let mut tiles = 0;
        while self.skills.level(SkillId::Wayfinding) < 2 {
            for target in [stone, start] {
                let route = plan_route(&self.world.level.map, self.here, target, Approach::OnTile).expect("route");
                tiles += route.len() as u32;
                self.skills.walked(route.len() as u32);
                self.here = *route.last().unwrap();
            }
            assert!(tiles < 2_000, "Wayfinding 2 should not take this long");
        }
        tiles
    }

    fn pick_moonpetals(&mut self, n: u32) {
        for _ in 0..n {
            self.bag.add(ItemId::Moonpetal, 1).unwrap();
            self.log.record(&self.world.content.quests, &QuestEvent::Gathered { item: ItemId::Moonpetal, quantity: 1 });
        }
    }

    fn status(&self, quest: &str) -> QuestStatus {
        self.log.status(quest)
    }

    fn finish_moonpetals(&mut self) {
        self.talk(GUIDE_ID, &["I can gather some"]);
        self.pick_moonpetals(3);
        self.talk(GUIDE_ID, &["Glad to help"]);
        assert_eq!(self.status("moonpetals"), QuestStatus::Completed);
    }
}

#[test]
fn moonpetals_for_the_apothecary_still_works_in_hollowmere() {
    let mut game = Game::new();
    game.finish_moonpetals();
    assert_eq!(game.bag.count(ItemId::Moonpetal), 0);
    assert_eq!(game.bag.count(ItemId::Timber), 2);
}

#[test]
fn the_shy_hare_befriend_then_talk() {
    let mut game = Game::new();
    // Before the guide's quest the apothecary has no errand.
    let hello = game.talk("apothecary", &["anything I can do"]);
    assert!(hello.contains("Pell"));
    assert_eq!(game.status("shy_hare"), QuestStatus::NotStarted);
    game.finish_moonpetals();

    game.talk("apothecary", &["anything I can do", "I will see what I can do"]);
    assert_eq!(game.status("shy_hare"), QuestStatus::Active);

    // Meeting the hare without a berry only makes it curious, and does not finish the quest.
    let hare = game.world.level.creatures.iter().find(|c| c.id == "meadow_hare").unwrap().tile;
    game.walk_to(hare, Approach::Adjacent);
    let result = approach(&game.world.bestiary, &mut game.notes, &mut game.bag, "meadow_hare").unwrap();
    assert!(matches!(result, Encounter::Watched { .. }));
    assert_eq!(game.log.current_objective("shy_hare"), Some((0, 0)));

    game.gather(ItemId::Honeyberry, 1);
    let result = approach(&game.world.bestiary, &mut game.notes, &mut game.bag, "meadow_hare").unwrap();
    assert!(matches!(result, Encounter::Befriended { .. }));
    game.log.record(&game.world.content.quests, &QuestEvent::Befriended { creature: "meadow_hare".into() });
    assert_eq!(game.log.current_objective("shy_hare"), Some((1, 0)));

    game.talk("apothecary", &["Glad to help"]);
    assert_eq!(game.status("shy_hare"), QuestStatus::Completed);
    assert_eq!(game.bag.count(ItemId::MintTea), 1);
    // The hare's gift petal was not "gathered", so it did not progress any collect objective.
    assert_eq!(game.bag.count(ItemId::Moonpetal), 1);
}

#[test]
fn a_staff_for_the_bridgewarden_craft_then_hand_in() {
    let mut game = Game::new();
    game.talk("ferrywoman", &["What do you need", "I will make you a staff"]);
    assert_eq!(game.status("bridge_staff"), QuestStatus::Active);

    // Handing in early does nothing; crafting before accepting would not have counted either.
    game.talk("ferrywoman", &["I am working on it"]);
    assert_eq!(game.status("bridge_staff"), QuestStatus::Active);

    game.gather(ItemId::Timber, 3);
    let crafted = game.world.recipes.craft("walking_staff", &mut game.bag, &mut game.skills).expect("three timber make a staff");
    game.log.record(&game.world.content.quests, &QuestEvent::Crafted { item: crafted.made.item, quantity: crafted.made.quantity });
    assert_eq!(game.status("bridge_staff"), QuestStatus::ReadyToTurnIn);

    game.talk("ferrywoman", &["Glad to help"]);
    assert_eq!(game.status("bridge_staff"), QuestStatus::Completed);
    assert_eq!(game.bag.count(ItemId::WalkingStaff), 0);
    assert_eq!(game.bag.count(ItemId::CopperOre), 2);
}

#[test]
fn a_missing_staff_delays_the_hand_in_without_losing_progress() {
    let mut game = Game::new();
    game.talk("ferrywoman", &["What do you need", "I will make you a staff"]);
    game.gather(ItemId::Timber, 3);
    let crafted = game.world.recipes.craft("walking_staff", &mut game.bag, &mut game.skills).unwrap();
    game.log.record(&game.world.content.quests, &QuestEvent::Crafted { item: crafted.made.item, quantity: 1 });
    assert_eq!(game.status("bridge_staff"), QuestStatus::ReadyToTurnIn);

    // The staff is no longer in the bag: Tamsin asks about it and nothing changes.
    game.bag.remove(ItemId::WalkingStaff, 1).unwrap();
    let before = game.bag.clone();
    let said = game.talk("ferrywoman", &["I will check"]);
    assert!(said.contains("still have it"), "{said}");
    assert_eq!(game.status("bridge_staff"), QuestStatus::ReadyToTurnIn);
    assert_eq!(game.bag, before);

    // With the staff back, the same conversation hands it in.
    game.bag.add(ItemId::WalkingStaff, 1).unwrap();
    game.talk("ferrywoman", &["Glad to help"]);
    assert_eq!(game.status("bridge_staff"), QuestStatus::Completed);
}

#[test]
fn waymarks_of_hollowmere_needs_wayfinding_then_three_visits() {
    let mut game = Game::new();
    // New travellers are asked to learn the roads first.
    game.talk("surveyor", &["help with the map", "I will."]);
    assert_eq!(game.status("waymarks"), QuestStatus::NotStarted);
    assert_eq!(game.skills.level(SkillId::Wayfinding), 1);

    // Learning the roads takes real walking: about 300 tiles (roughly a minute and a half).
    let tiles = game.learn_the_roads();
    assert!((300..=420).contains(&tiles), "Wayfinding 2 took {tiles} tiles");
    let places: Vec<_> = game.world.level.places.iter().map(|p| (p.id.clone(), p.tile)).collect();
    let surveyor = game.world.level.npcs.iter().find(|n| n.id == "surveyor").unwrap().tile;
    game.walk_to(surveyor, Approach::Adjacent);

    game.talk("surveyor", &["help with the map", "I will walk them"]);
    assert_eq!(game.status("waymarks"), QuestStatus::Active);

    // Visits count in order: reaching the lookout first does not skip the others.
    let order = ["standing_stone", "heron_landing", "lookout"];
    let tile_of = |id: &str| places.iter().find(|(p, _)| p == id).unwrap().1;
    game.walk_to(tile_of("lookout"), Approach::OnTile);
    game.log.record(&game.world.content.quests, &QuestEvent::Visited { place: "lookout".into() });
    assert_eq!(game.log.current_objective("waymarks"), Some((0, 0)));
    for place in order {
        game.walk_to(tile_of(place), Approach::OnTile);
        game.log.record(&game.world.content.quests, &QuestEvent::Visited { place: place.into() });
    }
    assert_eq!(game.log.current_objective("waymarks"), Some((3, 0)));

    game.walk_to(surveyor, Approach::Adjacent);
    game.talk("surveyor", &["Glad to help"]);
    assert_eq!(game.status("waymarks"), QuestStatus::Completed);
    assert_eq!(game.bag.count(ItemId::Honeyberry), 2);
}

#[test]
fn every_npc_place_and_creature_can_be_reached_on_foot_from_the_start() {
    let game = Game::new();
    let start = game.world.level.player_start;
    for (id, tile, _) in game.world.level.npc_list() {
        assert!(plan_route(&game.world.level.map, start, tile, Approach::Adjacent).is_some(), "cannot reach {id}");
    }
    for place in &game.world.level.places {
        assert!(plan_route(&game.world.level.map, start, place.tile, Approach::OnTile).is_some(), "cannot reach {}", place.id);
    }
    for creature in &game.world.level.creatures {
        assert!(plan_route(&game.world.level.map, start, creature.tile, Approach::Adjacent).is_some(), "cannot reach {}", creature.id);
    }
}

#[test]
fn every_village_quest_can_be_completed_in_one_session() {
    let mut game = Game::new();
    // Teach Wayfinding through the same walking the other quests need.
    game.finish_moonpetals();
    game.talk("ferrywoman", &["What do you need", "I will make you a staff"]);
    game.gather(ItemId::Timber, 3);
    let staff = game.world.recipes.craft("walking_staff", &mut game.bag, &mut game.skills).unwrap();
    game.log.record(&game.world.content.quests, &QuestEvent::Crafted { item: staff.made.item, quantity: 1 });
    game.talk("ferrywoman", &["Glad to help"]);

    game.talk("apothecary", &["anything I can do", "I will see what I can do"]);
    game.gather(ItemId::Honeyberry, 1);
    approach(&game.world.bestiary, &mut game.notes, &mut game.bag, "meadow_hare").unwrap();
    game.log.record(&game.world.content.quests, &QuestEvent::Befriended { creature: "meadow_hare".into() });
    game.talk("apothecary", &["Glad to help"]);

    game.learn_the_roads();
    game.talk("surveyor", &["help with the map", "I will walk them"]);
    for place in ["standing_stone", "heron_landing", "lookout"] {
        game.log.record(&game.world.content.quests, &QuestEvent::Visited { place: place.into() });
    }
    game.talk("surveyor", &["Glad to help"]);

    for quest in ["moonpetals", "bridge_staff", "shy_hare", "waymarks"] {
        assert_eq!(game.status(quest), QuestStatus::Completed, "{quest}");
    }
}
