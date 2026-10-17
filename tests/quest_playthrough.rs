//! End-to-end check of the M2 acceptance criterion: a complete short quest (the shipped
//! "Moonpetals for the Apothecary") is started, advanced, saved and restored, completed, and
//! verified through the public API alone, without launching Bevy.

use ozbound::dialogue::{DialogueBook, DialogueSession, QuestContext, DEFAULT_DIALOGUE};
use ozbound::gathering::{GatheringNode, ResourceKind};
use ozbound::inventory::{Inventory, ItemId};
use ozbound::journal::Journal;
use ozbound::quests::{QuestBook, QuestEvent, QuestLog, QuestStatus, QuestUpdate, DEFAULT_QUESTS};

struct Game {
    quests: QuestBook,
    dialogue: DialogueBook,
    log: QuestLog,
    bag: Inventory,
}

impl Game {
    fn new(bag_slots: usize) -> Self {
        let quests = QuestBook::parse(DEFAULT_QUESTS).expect("shipped quests parse");
        let dialogue = DialogueBook::parse(DEFAULT_DIALOGUE).expect("shipped dialogue parses");
        dialogue
            .check_against(&quests)
            .expect("dialogue only names real quests");
        Self {
            quests,
            dialogue,
            log: QuestLog::new(),
            bag: Inventory::new(bag_slots),
        }
    }

    /// Talks to the guide, picks the given choices in order, and returns every update seen
    /// plus the text shown first.
    fn talk(&mut self, picks: &[usize]) -> (String, Vec<QuestUpdate>) {
        let mut ctx = QuestContext {
            quests: &self.quests,
            log: &mut self.log,
            inventory: &mut self.bag,
        };
        let (mut session, outcome) =
            DialogueSession::begin(&self.dialogue, "guide", &mut ctx).unwrap();
        assert!(outcome.failures.is_empty(), "{:?}", outcome.failures);
        let mut updates = outcome.updates;
        let first = session.view(&self.dialogue, &ctx).unwrap().text;
        for &pick in picks {
            let out = session.choose(&self.dialogue, pick, &mut ctx).unwrap();
            assert!(out.failures.is_empty(), "{:?}", out.failures);
            updates.extend(out.updates);
        }
        session.close();
        (first, updates)
    }

    /// Picks a moonpetal the way the game does: the bag first, then the quest event.
    fn pick_moonpetal(&mut self) {
        let mut node = GatheringNode::new(ResourceKind::Moonpetal, 1, 1);
        let got = node.gather(&mut self.bag).expect("room in the bag");
        self.log.record(
            &self.quests,
            &QuestEvent::Gathered {
                item: ItemId::Moonpetal,
                quantity: got,
            },
        );
    }

    fn journal(&self) -> Journal {
        Journal::build(&self.quests, &self.log)
    }
}

#[test]
fn the_moonpetal_quest_can_be_played_start_to_finish() {
    let mut game = Game::new(20);
    assert!(game.journal().is_empty());

    // Meet the guide and accept.
    let (first, updates) = game.talk(&[0, 0]);
    assert!(first.starts_with("Welcome to the meadow"));
    assert_eq!(
        updates,
        vec![QuestUpdate::Started {
            quest: "moonpetals".into()
        }]
    );
    assert_eq!(game.log.status("moonpetals"), QuestStatus::Active);
    assert!(game
        .journal()
        .render()
        .contains("[ ] Collect moonpetal (0/3)"));

    // Talking again just reminds the player.
    let (first, updates) = game.talk(&[0]);
    assert!(first.starts_with("Three moonpetals, please"));
    assert!(updates.is_empty());

    // Two petals, then quit and come back: progress survives a save/load round trip.
    game.pick_moonpetal();
    game.pick_moonpetal();
    assert_eq!(game.log.current_objective("moonpetals"), Some((0, 2)));
    let saved = game.log.to_text();
    game.log = QuestLog::from_text(&saved, &game.quests).expect("own save loads");
    assert!(game.journal().render().contains("Collect moonpetal (2/3)"));

    // Handing in early does nothing: the quest is not ready.
    let (first, _) = game.talk(&[0]);
    assert!(first.starts_with("Three moonpetals, please"));
    assert_eq!(game.bag.count(ItemId::Timber), 0);

    // The third petal makes it ready.
    game.pick_moonpetal();
    assert_eq!(game.log.status("moonpetals"), QuestStatus::ReadyToTurnIn);
    assert_eq!(game.journal().active[0].status, QuestStatus::ReadyToTurnIn);

    // Hand in: petals go, timber arrives, the quest completes exactly once.
    let (first, updates) = game.talk(&[0]);
    assert!(first.starts_with("These are lovely"));
    assert!(
        matches!(updates.as_slice(), [QuestUpdate::Completed { quest, rewards }] if quest == "moonpetals" && rewards.len() == 1)
    );
    assert_eq!(game.bag.count(ItemId::Moonpetal), 0);
    assert_eq!(game.bag.count(ItemId::Timber), 2);
    assert_eq!(game.log.status("moonpetals"), QuestStatus::Completed);
    let journal = game.journal();
    assert!(journal.active.is_empty());
    assert_eq!(journal.completed.len(), 1);
    assert!(journal
        .render()
        .contains("Completed quests\n- Moonpetals for the Apothecary"));

    // The guide now only thanks the player, and nothing is paid twice.
    let (first, updates) = game.talk(&[0]);
    assert!(first.starts_with("Thank you again"));
    assert!(updates.is_empty());
    assert_eq!(game.bag.count(ItemId::Timber), 2);

    // The finished state also survives a save.
    let reloaded = QuestLog::from_text(&game.log.to_text(), &game.quests).unwrap();
    assert_eq!(reloaded.status("moonpetals"), QuestStatus::Completed);
}

#[test]
fn a_full_bag_delays_the_reward_without_losing_anything() {
    let mut game = Game::new(2);
    game.talk(&[0, 0]);
    game.bag.add(ItemId::CopperOre, 1).unwrap();
    for _ in 0..3 {
        game.pick_moonpetal();
    }
    // A spare petal keeps the petal slot occupied after the hand-in, so timber cannot fit.
    game.bag.add(ItemId::Moonpetal, 1).unwrap();

    let before = (game.log.clone(), game.bag.clone());
    let (first, updates) = game.talk(&[0]);
    assert!(first.starts_with("I can't take the moonpetals"));
    assert!(updates.is_empty());
    assert_eq!(
        (game.log.clone(), game.bag.clone()),
        before,
        "nothing was taken or given"
    );
    assert_eq!(game.log.status("moonpetals"), QuestStatus::ReadyToTurnIn);

    // Make room and try again.
    game.bag.remove(ItemId::CopperOre, 1).unwrap();
    let (first, updates) = game.talk(&[0]);
    assert!(first.starts_with("These are lovely"));
    assert_eq!(updates.len(), 1);
    assert_eq!(
        game.bag.count(ItemId::Moonpetal),
        1,
        "only the three the quest takes are removed"
    );
    assert_eq!(game.bag.count(ItemId::Timber), 2);
}

#[test]
fn declining_the_offer_starts_nothing() {
    let mut game = Game::new(20);
    let (_, updates) = game.talk(&[1]);
    assert!(updates.is_empty());
    assert_eq!(game.log.status("moonpetals"), QuestStatus::NotStarted);
    assert!(game.journal().is_empty());
    // Petals gathered with no quest do not count toward it later.
    game.pick_moonpetal();
    game.talk(&[0, 0]);
    assert_eq!(game.log.current_objective("moonpetals"), Some((0, 0)));
}
