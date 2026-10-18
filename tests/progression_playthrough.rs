//! End-to-end check of the M4 acceptance criteria, through the public API alone: skills grow
//! from real gathering, crafting and walking; levels unlock a recipe and quest options; and
//! the number of actions needed is fixed and bounded.

use ozbound::crafting::RecipeBook;
use ozbound::dialogue::{DialogueBook, DialogueSession, QuestContext, DEFAULT_DIALOGUE};
use ozbound::gathering::{GatheringNode, ResourceKind};
use ozbound::inventory::{Inventory, ItemId};
use ozbound::equipment::Equipment;
use ozbound::pack::{self, PackAction};
use ozbound::quests::{QuestBook, QuestEvent, QuestLog, QuestStatus, DEFAULT_QUESTS};
use ozbound::skills::{self, SkillId, Skills};

struct Game {
    quests: QuestBook,
    dialogue: DialogueBook,
    recipes: RecipeBook,
    log: QuestLog,
    bag: Inventory,
    gear: Equipment,
    skills: Skills,
}

impl Game {
    fn new() -> Self {
        let quests = QuestBook::parse(DEFAULT_QUESTS).unwrap();
        let dialogue = DialogueBook::parse(DEFAULT_DIALOGUE).unwrap();
        let recipes = RecipeBook::builtin().unwrap();
        dialogue.check_against(&quests).unwrap();
        skills::check_gates(&recipes, &dialogue).expect("shipped gates are reachable");
        Self { quests, dialogue, recipes, log: QuestLog::new(), bag: Inventory::new(30), gear: Equipment::new(), skills: Skills::new() }
    }

    /// One gather action the way the game does it: bag, quest event, then skill XP.
    fn gather(&mut self, kind: ResourceKind) -> Option<skills::XpGain> {
        let mut node = GatheringNode::new(kind, 1, 1);
        let got = node.gather(&mut self.bag).expect("room in the bag");
        self.log.record(&self.quests, &QuestEvent::Gathered { item: kind.item(), quantity: got });
        self.skills.gathered(kind)
    }

    /// Talks to the guide and picks the named choices in order; returns the opening text and
    /// the choices offered at the start.
    fn talk(&mut self, picks: &[&str]) -> (String, Vec<String>) {
        let mut ctx = QuestContext { quests: &self.quests, log: &mut self.log, inventory: &mut self.bag, skills: &self.skills };
        let (mut session, _) = DialogueSession::begin(&self.dialogue, "guide", &mut ctx).unwrap();
        let view = session.view(&self.dialogue, &ctx).unwrap();
        for text in picks {
            let now = session.view(&self.dialogue, &ctx).unwrap();
            let index = now.choices.iter().position(|c| c == text).unwrap_or_else(|| panic!("`{text}` not offered: {:?}", now.choices));
            session.choose(&self.dialogue, index, &mut ctx).unwrap();
        }
        session.close();
        (view.text, view.choices)
    }

    fn craft(&mut self, id: &str) -> String {
        pack::apply(&PackAction::Craft(id.into()), &self.recipes, &mut self.bag, &mut self.gear, &mut self.skills)
    }

    fn charm_row(&self) -> String {
        pack::rows(&self.recipes, &self.bag, &self.gear, &self.skills)[1].text.clone()
    }
}

#[test]
fn a_newcomer_starts_at_level_one_with_the_charm_locked() {
    let mut game = Game::new();
    assert_eq!(game.skills, Skills::new());
    game.bag.add(ItemId::CopperOre, 2).unwrap();
    game.bag.add(ItemId::Timber, 1).unwrap();
    assert!(game.charm_row().ends_with("needs Woodcraft level 3"), "{}", game.charm_row());
    assert_eq!(game.craft("copper_charm"), "You need Woodcraft level 3 to make that.");
    assert_eq!(game.bag.count(ItemId::CopperOre), 2, "nothing was spent");
}

#[test]
fn chopping_and_crafting_unlock_the_charm_after_a_fixed_amount_of_work() {
    let mut game = Game::new();
    let mut chops = 0;
    let mut crafted_staff = false;
    let mut level_ups = Vec::new();
    while game.skills.level(SkillId::Woodcraft) < 3 {
        if let Some(gain) = game.gather(ResourceKind::Tree) {
            chops += 1;
            level_ups.extend(gain.message());
        }
        // Spend timber on a staff as soon as there is enough: crafting teaches too.
        if game.bag.count(ItemId::Timber) >= 3 && !crafted_staff {
            let message = game.craft("walking_staff");
            level_ups.extend(message.split(". ").nth(1).map(String::from));
            crafted_staff = true;
        }
        assert!(chops < 100, "progress must not stall");
    }
    // 3 chops (24 XP) + staff (25 XP) + more chops at 8 XP each reach 120 XP.
    assert_eq!(chops, 12, "same actions, same result, every run");
    assert!(level_ups.iter().any(|m| m.contains("Woodcraft level is now 2")), "{level_ups:?}");
    assert!(level_ups.iter().any(|m| m.contains("Woodcraft level is now 3")), "{level_ups:?}");
    game.bag.add(ItemId::CopperOre, 2).unwrap();
    assert!(!game.charm_row().contains("needs"), "{}", game.charm_row());
    assert_eq!(game.craft("copper_charm"), "You made 1 copper charm.");
}

#[test]
fn herbalism_unlocks_tea_and_the_apothecary_quest_option_in_order() {
    let mut game = Game::new();
    // Finish the first quest (Foraging reaches level 2 on the way: 3 petals x 10 XP).
    game.talk(&["I can gather some.", "I'll be back soon."]);
    assert_eq!(game.log.status("moonpetals"), QuestStatus::Active);
    for _ in 0..3 {
        game.gather(ResourceKind::Moonpetal);
    }
    assert_eq!(game.skills.level(SkillId::Foraging), 2);
    game.talk(&[]);
    assert_eq!(game.log.status("moonpetals"), QuestStatus::Completed);

    // A novice herbalist is told to practise; the quest is not offered.
    let (_, choices) = game.talk(&[]);
    assert!(choices.iter().any(|c| c == "Is there anything else I can do?"));
    let (text, _) = game.talk(&["Is there anything else I can do?"]);
    assert_eq!(text, game.talk(&[]).0, "asking leads nowhere new yet");
    assert_eq!(game.log.status("apothecary_mint"), QuestStatus::NotStarted);
    assert!(game.quests.get("apothecary_mint").is_some());

    // Five meadowmint gathers (6 XP each) reach Herbalism 2.
    for _ in 0..5 {
        game.gather(ResourceKind::HerbPatch);
    }
    assert_eq!(game.skills.level(SkillId::Herbalism), 2);
    game.bag.add(ItemId::Honeyberry, 1).unwrap();
    assert!(game.craft("mint_tea").starts_with("You made 1 mint tea."));

    // Now the offer exists and can be accepted.
    game.talk(&["Is there anything else I can do?", "I'll gather some.", "I'll be back soon."]);
    assert_eq!(game.log.status("apothecary_mint"), QuestStatus::Active);
    for _ in 0..2 {
        game.gather(ResourceKind::HerbPatch);
    }
    assert_eq!(game.log.status("apothecary_mint"), QuestStatus::ReadyToTurnIn);
    game.talk(&["Glad to help."]);
    assert_eq!(game.log.status("apothecary_mint"), QuestStatus::Completed);
    assert_eq!(game.bag.count(ItemId::MintTea), 2, "the crafted tea plus the quest reward");
}

#[test]
fn walking_teaches_wayfinding_and_opens_the_exploring_option() {
    let mut game = Game::new();
    game.talk(&["I can gather some.", "I'll be back soon."]);
    for _ in 0..3 {
        game.gather(ResourceKind::Moonpetal);
    }
    game.talk(&[]);
    let explore = "Is there anywhere worth exploring?";
    let (_, before) = game.talk(&[]);
    assert!(!before.iter().any(|c| c == explore));
    // 30 XP at one point per 10 tiles.
    for _ in 0..299 {
        game.skills.walked(1);
    }
    assert_eq!(game.skills.level(SkillId::Wayfinding), 1);
    game.skills.walked(1);
    assert_eq!(game.skills.level(SkillId::Wayfinding), 2);
    let (_, after) = game.talk(&[]);
    assert!(after.iter().any(|c| c == explore), "{after:?}");
    game.talk(&[explore, "Thank you."]);
}

#[test]
fn easy_sources_run_dry_so_late_levels_need_the_gated_recipes() {
    let mut game = Game::new();
    for _ in 0..1000 {
        game.gather(ResourceKind::Tree);
        game.bag = Inventory::new(30);
    }
    // Trees alone stop at Woodcraft 4; the charm (needs 3, teaches to 6) carries it on.
    assert_eq!(game.skills.level(SkillId::Woodcraft), 4);
    let before = game.skills.clone();
    game.gather(ResourceKind::Tree);
    assert_eq!(game.skills, before, "chopping teaches nothing more");
    game.bag.add(ItemId::CopperOre, 2).unwrap();
    game.bag.add(ItemId::Timber, 1).unwrap();
    let message = game.craft("copper_charm");
    assert!(message.starts_with("You made 1 copper charm."), "{message}");
    assert!(game.skills.xp(SkillId::Woodcraft) > before.xp(SkillId::Woodcraft));
}
