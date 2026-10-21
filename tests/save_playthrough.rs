//! End-to-end check of the M6 acceptance criteria using only the public API (no Bevy): a play
//! session is saved, the program "restarts", and the world, bag, gear, skills, quests, and
//! creatures are exactly as they were; a corrupt save is refused without being touched; the old
//! quest-only save is carried over once; and a save from another map is refused.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};

use ozbound::bestiary::{approach, Encounter, FieldNotes};
use ozbound::equipment::{EquipSlot, Equipment};
use ozbound::gathering::{GatheringNode, ResourceKind};
use ozbound::inventory::{Inventory, ItemId};
use ozbound::level::{Level, NodeSpawn};
use ozbound::pathfinding::{plan_route, Approach};
use ozbound::quests::{QuestEvent, QuestLog, QuestStatus};
use ozbound::save::{SaveContext, SaveGame, WorldChanges};
use ozbound::save_store::{self, Start, StoreError};
use ozbound::scene;
use ozbound::skills::{SkillId, Skills};
use ozbound::village::{builtin, World};
use ozbound::world::TilePos;
use ozbound::quest_store;

fn scratch() -> PathBuf {
    static NEXT: AtomicU32 = AtomicU32::new(0);
    let dir = std::env::temp_dir().join(format!("ozbound-playthrough-{}-{}", std::process::id(), NEXT.fetch_add(1, Ordering::Relaxed)));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn ctx(world: &World) -> SaveContext<'_> {
    SaveContext { level: &world.level, quests: &world.content.quests, bestiary: &world.bestiary }
}

/// What the Bevy layer holds while playing, built from a save exactly as `setup` and `main` do.
struct Session<'a> {
    world: &'a World,
    here: TilePos,
    bag: Inventory,
    gear: Equipment,
    skills: Skills,
    log: QuestLog,
    notes: FieldNotes,
    petals: Vec<TilePos>,
    nodes: Vec<(NodeSpawn, GatheringNode)>,
}

impl<'a> Session<'a> {
    fn from_save(world: &'a World, game: &SaveGame) -> Self {
        let level = &world.level;
        let nodes = level
            .nodes
            .iter()
            .map(|spawn| {
                let mut node = GatheringNode::new(spawn.kind, spawn.actions, spawn.yield_per_action);
                if let Some(left) = game.changes.node_remaining(spawn.tile) {
                    assert!(node.set_remaining(left));
                }
                (*spawn, node)
            })
            .collect();
        Self {
            world,
            here: game.player,
            bag: game.bag.clone(),
            gear: game.equipment.clone(),
            skills: game.skills.clone(),
            log: game.quests.clone(),
            notes: game.notes.clone(),
            petals: level.moonpetals.iter().copied().filter(|t| !game.changes.is_picked(*t)).collect(),
            nodes,
        }
    }

    fn snapshot(&self) -> SaveGame {
        let level = &self.world.level;
        let mut game = SaveGame::new_game(level);
        game.player = self.here;
        game.bag = self.bag.clone();
        game.equipment = self.gear.clone();
        game.skills = self.skills.clone();
        game.quests = self.log.clone();
        game.notes = self.notes.clone();
        let state: Vec<(TilePos, u32)> = self.nodes.iter().map(|(spawn, node)| (spawn.tile, node.remaining_actions())).collect();
        game.changes = WorldChanges::capture(level, &self.petals, &state);
        game
    }

    fn walk_to(&mut self, target: TilePos, how: Approach) {
        let route = plan_route(&self.world.level.map, self.here, target, how).unwrap_or_else(|| panic!("no route to {target:?}"));
        self.skills.walked(route.len() as u32);
        self.here = *route.last().unwrap_or(&self.here);
    }

    fn pick_petal(&mut self, tile: TilePos) {
        self.walk_to(tile, Approach::OnTile);
        self.petals.retain(|t| *t != tile);
        self.bag.add(ItemId::Moonpetal, 1).unwrap();
        self.log.record(&self.world.content.quests, &QuestEvent::Gathered { item: ItemId::Moonpetal, quantity: 1 });
        self.skills.gathered(ResourceKind::Moonpetal);
    }

    /// Gathers from the node at `tile` up to `times` times, stopping when it is spent.
    fn gather(&mut self, tile: TilePos, times: u32) {
        let beside = if self.nodes.iter().any(|(s, _)| s.tile == tile && matches!(s.kind, ResourceKind::HerbPatch | ResourceKind::BerryBush)) {
            Approach::OnTile
        } else {
            Approach::Adjacent
        };
        self.walk_to(tile, beside);
        let index = self.nodes.iter().position(|(s, _)| s.tile == tile).expect("a node there");
        for _ in 0..times {
            let kind = self.nodes[index].0.kind;
            if let Ok(got) = self.nodes[index].1.gather(&mut self.bag) {
                self.log.record(&self.world.content.quests, &QuestEvent::Gathered { item: kind.item(), quantity: got });
                self.skills.gathered(kind);
            }
        }
    }

    fn node(&self, tile: TilePos) -> &GatheringNode {
        &self.nodes.iter().find(|(s, _)| s.tile == tile).expect("a node there").1
    }
}

fn first(world: &World, kind: ResourceKind) -> NodeSpawn {
    *world.level.nodes.iter().find(|n| n.kind == kind).expect("the world has this node kind")
}

fn nth(world: &World, kind: ResourceKind, n: usize) -> NodeSpawn {
    *world.level.nodes.iter().filter(|s| s.kind == kind).nth(n).expect("the world has enough nodes of this kind")
}

#[test]
fn a_session_survives_a_restart_and_play_carries_on() {
    let world = builtin().expect("the shipped world loads");
    let dir = scratch();
    let path = dir.join("saves").join("game.txt");
    let level = &world.level;

    // ---- first session: a new game ----
    let loaded = save_store::load(&path, None, &ctx(&world)).unwrap();
    assert_eq!(loaded.start, Start::NewGame);
    let mut one = Session::from_save(&world, &loaded.game);
    assert_eq!(one.here, level.player_start);

    let quests = &world.content.quests;
    one.log.start(quests, "moonpetals").unwrap();
    for tile in level.moonpetals.iter().take(3).copied().collect::<Vec<_>>() {
        one.pick_petal(tile);
    }
    assert_eq!(one.log.status("moonpetals"), QuestStatus::ReadyToTurnIn);

    let tree = first(&world, ResourceKind::Tree);
    let other_tree = nth(&world, ResourceKind::Tree, 1);
    let herb = first(&world, ResourceKind::HerbPatch);
    one.gather(tree.tile, tree.actions + 5); // chopped until spent
    assert!(one.node(tree.tile).is_depleted());
    one.gather(herb.tile, 1); // used once
    assert!(one.node(herb.tile).remaining_actions() < herb.actions);

    one.bag.add(ItemId::WalkingStaff, 1).unwrap();
    one.gear.equip(ItemId::WalkingStaff, &mut one.bag).unwrap();

    let hare = world.level.creatures.iter().find(|c| c.id == "meadow_hare").expect("the hare lives in Bluebrook");
    one.bag.add(ItemId::Honeyberry, 1).unwrap();
    one.walk_to(hare.tile, Approach::Adjacent);
    let met = approach(&world.bestiary, &mut one.notes, &mut one.bag, "meadow_hare").unwrap();
    assert!(matches!(met, Encounter::Befriended { .. }), "{met:?}");

    let first_session = one.snapshot();
    save_store::save(&path, &first_session).unwrap();
    drop(one); // the program quits

    // ---- second session: everything is as it was ----
    let loaded = save_store::load(&path, None, &ctx(&world)).unwrap();
    assert_eq!(loaded.start, Start::Loaded);
    assert_eq!(loaded.game, first_session);
    let mut two = Session::from_save(&world, &loaded.game);

    assert_eq!(two.here, first_session.player);
    assert_ne!(two.here, level.player_start, "the player was not sent back to the start");
    assert_eq!(two.petals.len(), level.moonpetals.len() - 3, "picked petals stay picked");
    assert!(level.moonpetals.iter().skip(3).all(|t| two.petals.contains(t)));
    assert!(two.node(tree.tile).is_depleted(), "a spent tree is still spent");
    assert_eq!(two.node(other_tree.tile).remaining_actions(), other_tree.actions, "untouched nodes are untouched");
    assert_eq!(two.node(herb.tile).remaining_actions(), first_session.changes.node_remaining(herb.tile).unwrap());
    assert_eq!(two.bag, first_session.bag);
    assert_eq!(two.gear.get(EquipSlot::Hand), Some(ItemId::WalkingStaff));
    assert!(two.notes.is_befriended("meadow_hare"));
    assert_eq!(two.skills, first_session.skills);
    assert!(two.skills.level(SkillId::Foraging) >= 1);
    assert_eq!(two.log.status("moonpetals"), QuestStatus::ReadyToTurnIn);

    // The spent tree grows back as in a live game, and the reloaded session carries on from there.
    let index = two.nodes.iter().position(|(s, _)| s.tile == tree.tile).unwrap();
    assert!(two.nodes[index].1.restock());
    let timber_before = two.bag.count(ItemId::Timber);
    let petals_before = two.bag.count(ItemId::Moonpetal);
    two.log.complete(quests, "moonpetals", &mut two.bag).expect("the hand-in works after a reload");
    assert_eq!(two.log.status("moonpetals"), QuestStatus::Completed);
    assert!(two.bag.count(ItemId::Timber) > timber_before, "the reward arrived");
    assert_eq!(two.bag.count(ItemId::Moonpetal), petals_before - 3, "the three petals were taken");

    let second_session = two.snapshot();
    assert_ne!(second_session, first_session);
    save_store::save(&path, &second_session).unwrap();
    let third = save_store::load(&path, None, &ctx(&world)).unwrap();
    assert_eq!(third.game, second_session);
    assert_eq!(third.game.quests.status("moonpetals"), QuestStatus::Completed);
    assert!(third.game.changes.node_remaining(tree.tile).is_none(), "the regrown tree is no longer listed");
}

#[test]
fn saving_the_same_game_twice_writes_the_same_bytes() {
    let world = builtin().unwrap();
    let dir = scratch();
    let mut session = Session::from_save(&world, &SaveGame::new_game(&world.level));
    session.pick_petal(world.level.moonpetals[0]);
    session.gather(first(&world, ResourceKind::Tree).tile, 2);
    let (a, b) = (dir.join("a.txt"), dir.join("b.txt"));
    save_store::save(&a, &session.snapshot()).unwrap();
    save_store::save(&b, &session.snapshot()).unwrap();
    assert_eq!(std::fs::read(&a).unwrap(), std::fs::read(&b).unwrap());
}

#[test]
fn a_corrupt_save_is_refused_untouched_and_the_backup_recovers_it() {
    let world = builtin().unwrap();
    let dir = scratch();
    let path = dir.join("game.txt");

    let mut session = Session::from_save(&world, &SaveGame::new_game(&world.level));
    session.pick_petal(world.level.moonpetals[0]);
    let good = session.snapshot();
    save_store::save(&path, &good).unwrap();
    // The game keeps a copy after a successful start.
    assert_eq!(save_store::load(&path, None, &ctx(&world)).unwrap().start, Start::Loaded);
    save_store::back_up(&path).unwrap();

    // Something damages the save: half the file is gone.
    let text = std::fs::read_to_string(&path).unwrap();
    let damaged = text[..text.len() * 2 / 3].to_string();
    std::fs::write(&path, &damaged).unwrap();

    let error = save_store::load(&path, None, &ctx(&world)).unwrap_err();
    let StoreError::Invalid { backup, .. } = &error else { panic!("expected an invalid-save error, got {error}") };
    assert_eq!(backup.as_deref(), Some(save_store::backup_path(&path).as_path()));
    let message = error.to_string();
    assert!(message.contains("game.txt") && message.contains("line ") && message.contains("game.txt.bak"), "{message}");
    assert_eq!(std::fs::read_to_string(&path).unwrap(), damaged, "the damaged file was not overwritten");

    // Recovering is a plain file copy, and the game then loads the backed-up progress.
    std::fs::copy(save_store::backup_path(&path), &path).unwrap();
    assert_eq!(save_store::load(&path, None, &ctx(&world)).unwrap().game, good);
}

#[test]
fn every_kind_of_damage_to_one_line_is_caught_and_never_loads_partly() {
    let world = builtin().unwrap();
    let mut session = Session::from_save(&world, &SaveGame::new_game(&world.level));
    session.pick_petal(world.level.moonpetals[0]);
    session.gather(first(&world, ResourceKind::Tree).tile, 1);
    session.bag.add(ItemId::WalkingStaff, 1).unwrap();
    session.gear.equip(ItemId::WalkingStaff, &mut session.bag).unwrap();
    session.notes = FieldNotes::from_ids(["meadow_hare"], ["meadow_hare"]).unwrap();
    let text = session.snapshot().to_text();

    let lines: Vec<&str> = text.lines().collect();
    let original = session.snapshot();
    let mut caught = 0;
    for (index, line) in lines.iter().enumerate().skip(1) {
        // Dropping a line must fail or lose what it described. Only lines that say "the default"
        // (a zero, or the normal bag size) may be dropped without any change.
        let without: String = lines.iter().enumerate().filter(|(i, _)| *i != index).map(|(_, l)| format!("{l}\n")).collect();
        let redundant = line.starts_with("bag_capacity") || line.ends_with(" 0") && !line.starts_with("node");
        match SaveGame::from_text(&without, &ctx(&world)) {
            Err(_) => caught += 1,
            Ok(game) if !redundant => assert_ne!(game, original, "dropping `{line}` must lose something"),
            Ok(_) => {}
        }
        // Garbling the line must never load the damaged game.
        let garbled: String = lines.iter().enumerate().map(|(i, l)| if i == index { format!("{l} ???\n") } else { format!("{l}\n") }).collect();
        assert!(SaveGame::from_text(&garbled, &ctx(&world)).is_err(), "garbled `{line}` was accepted");
    }
    assert!(caught >= 3, "world, player, and end are required (caught {caught})");
}

#[test]
fn the_old_quest_save_is_carried_over_once() {
    let world = builtin().unwrap();
    let dir = scratch();
    let (path, legacy) = (dir.join("game.txt"), dir.join("quests.txt"));

    let mut old = QuestLog::new();
    old.start(&world.content.quests, "moonpetals").unwrap();
    old.record(&world.content.quests, &QuestEvent::Gathered { item: ItemId::Moonpetal, quantity: 2 });
    quest_store::save(&legacy, &old).unwrap();

    let migrated = save_store::load(&path, Some(&legacy), &ctx(&world)).unwrap();
    assert_eq!(migrated.start, Start::MigratedQuests);
    assert_eq!(migrated.game.quests, old);
    save_store::save(&path, &migrated.game).unwrap();

    // From now on the full save is the truth, even if the old file changes or is deleted.
    quest_store::save(&legacy, &QuestLog::new()).unwrap();
    let again = save_store::load(&path, Some(&legacy), &ctx(&world)).unwrap();
    assert_eq!(again.start, Start::Loaded);
    assert_eq!(again.game.quests, old);
    std::fs::remove_file(&legacy).unwrap();
    assert_eq!(save_store::load(&path, Some(&legacy), &ctx(&world)).unwrap().game.quests, old);
}

#[test]
fn a_save_from_another_map_is_refused() {
    let world = builtin().unwrap();
    let dir = scratch();
    let path = dir.join("game.txt");
    save_store::save(&path, &SaveGame::new_game(&world.level)).unwrap();

    let meadow = Level::parse(scene::DEFAULT_LEVEL).expect("the tutorial meadow parses");
    let other = SaveContext { level: &meadow, quests: &world.content.quests, bestiary: &world.bestiary };
    let error = save_store::load(&path, None, &other).unwrap_err();
    let message = error.to_string();
    assert!(message.contains("different world") && message.contains("line 2"), "{message}");
    assert!(Path::new(&path).exists());
}

#[test]
fn a_new_game_on_either_shipped_map_round_trips() {
    let world = builtin().unwrap();
    let meadow = Level::parse(scene::DEFAULT_LEVEL).unwrap();
    for level in [&world.level, &meadow] {
        let context = SaveContext { level, quests: &world.content.quests, bestiary: &world.bestiary };
        let game = SaveGame::new_game(level);
        assert_eq!(SaveGame::from_text(&game.to_text(), &context).unwrap(), game);
    }
}
