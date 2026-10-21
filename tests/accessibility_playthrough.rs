//! End-to-end checks of the M7 acceptance criteria through the public API (no Bevy):
//!
//! - everything the mouse can do is reachable from the keyboard: every walkable tile can be
//!   reached with WASD steps, and every person, plant, tree, vein, sign, creature, and moonpetal
//!   can be selected with Tab and reached on foot;
//! - no shipped dialogue step, crafting row, or journal page overflows its panel at the smallest
//!   supported window and the largest text size;
//! - a whole session of pause-menu changes works from the keyboard alone and survives a restart.

use std::collections::{HashSet, VecDeque};

use ozbound::chat::{ChatLog, Kind as ChatKind};
use ozbound::equipment::Equipment;
use ozbound::interaction::{in_reach, InteractionKind};
use ozbound::inventory::{Inventory, ItemId};
use ozbound::journal::Journal;
use ozbound::pack;
use ozbound::pathfinding::plan_route;
use ozbound::pause::{controls_help, on_escape, EscapeAction, Key, Open, Outcome, Page, PauseMenu, Row};
use ozbound::quests::QuestLog;
use ozbound::settings::{Settings, TextSize};
use ozbound::skills::Skills;
use ozbound::targeting::{cycle, rank, step_direction, step_to, Candidate, Dir};
use ozbound::text_fit::{
    chars_per_line, dialogue_height, CHAT_LINES, CHAT_PANEL, CHAT_PX, fit_scale, lines_per_page, logical_size, pack_height, paginate, DIALOGUE_PANEL, DOCK_PANEL, JOURNAL_PX,
    pause_height, MARGIN, MIN_LOGICAL, MIN_WINDOW, PACK_PANEL, PAUSE_PANEL,
};
use ozbound::village::{builtin, World};
use ozbound::world::TilePos;

fn world() -> World {
    builtin().expect("the shipped world loads")
}

/// Everything in the level the player can use, as the Bevy layer lists it.
fn candidates(world: &World) -> Vec<Candidate<usize>> {
    let level = &world.level;
    let mut all: Vec<(InteractionKind, TilePos, String)> = Vec::new();
    for (_, tile, name) in level.npc_list() {
        all.push((InteractionKind::TalkTo, tile, name.to_string()));
    }
    for tile in &level.moonpetals {
        all.push((InteractionKind::Pick, *tile, "moonpetal".into()));
    }
    for node in &level.nodes {
        all.push((node.kind.interaction(), node.tile, node.kind.label().to_string()));
    }
    for sign in &level.signs {
        all.push((InteractionKind::Read, sign.tile, "signpost".into()));
    }
    for creature in &level.creatures {
        all.push((InteractionKind::Approach, creature.tile, creature.id.clone()));
    }
    all.into_iter()
        .enumerate()
        .map(|(id, (kind, tile, name))| Candidate { id, tile, approach: kind.approach(), label: format!("{} {name}", kind.verb()) })
        .collect()
}

#[test]
fn wasd_steps_reach_every_tile_the_mouse_can() {
    let world = world();
    let map = &world.level.map;
    let start = world.level.player_start;
    let mut seen: HashSet<TilePos> = HashSet::from([start]);
    let mut queue = VecDeque::from([start]);
    while let Some(here) = queue.pop_front() {
        for dir in [Dir::Forward, Dir::Back, Dir::Left, Dir::Right] {
            // Any camera angle gives the same four steps, just assigned to different keys.
            let delta = step_direction(0.0, dir);
            if let Ok(next) = step_to(map, here, delta) {
                if seen.insert(next) {
                    queue.push_back(next);
                }
            }
        }
    }
    let mut mouse_reach = 0;
    for y in 0..map.height() as i32 {
        for x in 0..map.width() as i32 {
            let tile = TilePos::new(x, y);
            let by_mouse = map.is_reachable(start, tile);
            assert_eq!(seen.contains(&tile), by_mouse, "{tile:?}: keyboard and mouse must agree on what is reachable");
            mouse_reach += usize::from(by_mouse);
        }
    }
    assert!(mouse_reach > 200, "the village is a real place to walk around");
}

#[test]
fn every_camera_angle_still_reaches_everywhere_with_four_keys() {
    let world = world();
    let map = &world.level.map;
    let start = world.level.player_start;
    for quarter in 0..4 {
        let yaw = quarter as f32 * std::f32::consts::FRAC_PI_2 + 0.2;
        let mut seen: HashSet<TilePos> = HashSet::from([start]);
        let mut queue = VecDeque::from([start]);
        while let Some(here) = queue.pop_front() {
            for dir in [Dir::Forward, Dir::Back, Dir::Left, Dir::Right] {
                if let Ok(next) = step_to(map, here, step_direction(yaw, dir)) {
                    if seen.insert(next) {
                        queue.push_back(next);
                    }
                }
            }
        }
        let expected = (0..map.height() as i32)
            .flat_map(|y| (0..map.width() as i32).map(move |x| TilePos::new(x, y)))
            .filter(|tile| map.is_reachable(start, *tile))
            .count();
        assert_eq!(seen.len(), expected, "yaw quarter {quarter}");
    }
}

#[test]
fn tab_offers_every_person_plant_tree_sign_creature_and_petal_and_each_can_be_used() {
    let world = world();
    let list = candidates(&world);
    assert!(list.len() > 20, "Bluebrook has plenty to visit ({} things)", list.len());
    let start = world.level.player_start;
    let ranked = rank(&world.level.map, start, &list);
    assert_eq!(ranked.len(), list.len(), "nothing the mouse could use is missing from the Tab list");
    assert!(ranked.windows(2).all(|pair| pair[0].steps <= pair[1].steps), "nearest first");

    // Walk to each one along the planned route and confirm the player ends up in reach.
    for item in &ranked {
        let candidate = &list[item.id];
        let route = plan_route(&world.level.map, start, candidate.tile, candidate.approach).expect("the ranked target has a route");
        let end = route.last().copied().unwrap_or(start);
        assert!(in_reach(end, candidate.tile, candidate.approach), "{}: the walk must end in reach", candidate.label);
        assert_eq!(route.len(), item.steps);
    }

    // Tab visits each exactly once in a lap.
    let mut current = None;
    let mut visited = HashSet::new();
    for _ in 0..ranked.len() {
        current = cycle(&ranked, current, true);
        assert!(visited.insert(current.unwrap()), "Tab repeated a target before finishing the lap");
    }
    assert_eq!(visited.len(), list.len());
}

#[test]
fn the_tab_list_still_works_after_walking_somewhere_else() {
    let world = world();
    let list = candidates(&world);
    let nearest_to_start = rank(&world.level.map, world.level.player_start, &list);
    let far = list.iter().find(|c| c.id == nearest_to_start.last().unwrap().id).unwrap();
    let route = plan_route(&world.level.map, world.level.player_start, far.tile, far.approach).unwrap();
    let there = *route.last().unwrap_or(&world.level.player_start);
    let from_there = rank(&world.level.map, there, &list);
    assert_eq!(from_there.len(), list.len());
    assert_eq!(from_there[0].id, far.id, "the thing you just walked to is now the nearest");
    assert_eq!(from_there[0].steps, 0);
}

#[test]
fn every_dialogue_step_fits_its_panel_at_the_smallest_window() {
    let world = world();
    let limit = MIN_LOGICAL.1 - DIALOGUE_PANEL.offset - MARGIN;
    let mut checked = 0;
    for conversation in world.content.dialogue.iter() {
        for node in &conversation.nodes {
            let choices: Vec<String> = node.choices.iter().map(|c| c.text.clone()).collect();
            assert!(choices.len() <= 9, "{} / {}: choices beyond the ninth cannot be picked with a number key", conversation.id, node.id);
            let height = dialogue_height(&node.text, &choices);
            assert!(
                height <= limit,
                "{} / {}: the dialogue panel would be {height:.0}px tall but only {limit:.0}px fit (shorten the text or split the node)",
                conversation.id,
                node.id
            );
            checked += 1;
        }
    }
    assert!(checked > 20, "the shipped conversations were actually checked ({checked})");
}

#[test]
fn the_crafting_panel_fits_even_with_every_item_in_the_bag_and_gear_worn() {
    let world = world();
    let mut bag = Inventory::default();
    for item in ItemId::ALL {
        let _ = bag.add(item, 1);
    }
    let mut gear = Equipment::new();
    for item in ItemId::ALL {
        if item.equip_slot().is_some() {
            let _ = gear.equip(item, &mut bag);
        }
    }
    for _ in 0..2 {
        // Once with gear worn, once with the same gear back in the bag (more Equip rows).
        let skills = Skills::default();
        let rows: Vec<String> = pack::rows(&world.recipes, &bag, &gear, &skills).into_iter().map(|r| r.text).collect();
        assert!(rows.len() <= 9, "rows beyond the ninth cannot be picked with a number key ({} rows)", rows.len());
        let info = format!("{}\n{}\n{}", pack::bag_line(&bag), pack::equipment_line(&gear), skills.summary_line());
        let height = pack_height(&info, &rows);
        let limit = MIN_LOGICAL.1 - PACK_PANEL.offset - MARGIN;
        assert!(height <= limit, "the crafting panel would be {height:.0}px tall but only {limit:.0}px fit ({} rows)", rows.len());
        let worn: Vec<_> = gear.iter().map(|(slot, _)| slot).collect();
        for slot in worn {
            let _ = gear.unequip(slot, &mut bag);
        }
    }
}

#[test]
fn the_journal_pages_fit_and_lose_nothing() {
    let world = world();
    let mut log = QuestLog::new();
    for quest in world.content.quests.iter() {
        let _ = log.start(&world.content.quests, &quest.id);
    }
    let text = Journal::build(&world.content.quests, &log).render();
    let width = chars_per_line(DOCK_PANEL.inner_width(), JOURNAL_PX);
    let per_page = lines_per_page(DOCK_PANEL.inner_height(MIN_LOGICAL.1), JOURNAL_PX);
    let pages = paginate(&text, width, per_page);
    assert!(!pages.is_empty());
    let mut words = 0;
    for page in &pages {
        assert!(page.lines().count() <= per_page);
        assert!(page.lines().all(|line| line.chars().count() <= width), "no line is wider than the panel");
        words += page.split_whitespace().count();
    }
    assert_eq!(words, text.split_whitespace().count(), "paging drops no words");
    // A journal with every quest running stays a handful of pages, so it is easy to read through.
    assert!(pages.len() <= 6, "{} pages", pages.len());
}

#[test]
fn long_messages_wrap_inside_the_chat_area_at_the_smallest_window() {
    // A message can carry a gathering result, a quest update, and a level-up together.
    let message = "You gathered 1 from the tree (2 left). Quest updated: A Staff for the Bridgewarden - gather timber (3 / 3). Your Woodcraft level is now 4.";
    let width = chars_per_line(CHAT_PANEL.inner_width(), CHAT_PX);
    let mut log = ChatLog::new();
    log.push(ChatKind::Game, message);
    let view = log.view(width, CHAT_LINES);
    assert!(view.iter().all(|line| line.text.chars().count() <= width));
    assert!(view.len() >= 2, "it really does need wrapping at this width");
    let shown: String = view.iter().map(|line| line.text.trim()).collect::<Vec<_>>().join(" ");
    assert_eq!(shown.split_whitespace().collect::<Vec<_>>(), message.split_whitespace().collect::<Vec<_>>(), "all of the message stays visible");
}

#[test]
fn a_whole_session_of_settings_changes_works_from_the_keyboard_alone_and_survives_a_restart() {
    let mut settings = Settings::default();
    let mut open = Open::default();
    let mut menu = PauseMenu::new();

    // Esc with nothing open pauses.
    let action = on_escape(open);
    assert_eq!(action, EscapeAction::OpenPause);
    open = open.after(action);
    assert!(open.pause);

    // Down, Enter: Settings. Then Left x3 on Volume, Down, Left: ambient off, Down, Right: Large,
    // Down, Enter: overlay on.
    let script = [
        (Key::Down, Outcome::Nothing),
        (Key::Confirm, Outcome::Nothing),
        (Key::Left, Outcome::SettingsChanged),
        (Key::Left, Outcome::SettingsChanged),
        (Key::Left, Outcome::SettingsChanged),
        (Key::Down, Outcome::Nothing),
        (Key::Left, Outcome::SettingsChanged),
        (Key::Down, Outcome::Nothing),
        (Key::Right, Outcome::SettingsChanged),
        (Key::Down, Outcome::Nothing),
        (Key::Confirm, Outcome::SettingsChanged),
    ];
    for (key, expected) in script {
        assert_eq!(menu.press(key, &mut settings), expected, "{key:?}");
    }
    assert_eq!(menu.page(), Page::Settings);
    assert_eq!(settings.volume(), 4);
    assert!(!settings.ambient);
    assert_eq!(settings.text_size, TextSize::Large);
    assert!(settings.show_perf);

    // Esc steps back to the first page, and Esc again resumes.
    assert_eq!(menu.press(Key::Back, &mut settings), Outcome::Nothing);
    assert_eq!(menu.selected_row(), Row::OpenSettings);
    assert_eq!(menu.press(Key::Back, &mut settings), Outcome::Resume);

    // A restart reads the same settings back.
    let reloaded = Settings::from_text(&settings.to_text()).expect("what we wrote loads");
    assert_eq!(reloaded, settings);
    assert_eq!(reloaded.ambient_gain(), 0.0, "ambient sound stays off");
}

#[test]
fn the_largest_text_size_is_reduced_not_clipped_in_a_small_window() {
    let mut settings = Settings::default();
    settings.text_size = TextSize::Larger;
    let scale = fit_scale(settings.requested_ui_scale(), MIN_WINDOW.0, MIN_WINDOW.1);
    assert!((1.0..1.5).contains(&scale));
    let (logical_width, logical_height) = logical_size(MIN_WINDOW.0, MIN_WINDOW.1, scale);
    for panel in [DIALOGUE_PANEL, PACK_PANEL, DOCK_PANEL, CHAT_PANEL] {
        assert!(panel.width + 2.0 * MARGIN <= logical_width + 0.01, "{panel:?}");
        assert!(panel.offset + MARGIN < logical_height);
    }
}

#[test]
fn every_pause_menu_page_fits_at_the_smallest_window() {
    let settings = Settings::default();
    let limit = MIN_LOGICAL.1 - PAUSE_PANEL.offset - MARGIN;
    for page in [Page::Main, Page::Settings, Page::Controls] {
        let mut menu = PauseMenu::new();
        let mut scratch = settings;
        // Navigate there with the keyboard, the way a player would.
        match page {
            Page::Main => {}
            Page::Settings => {
                menu.press(Key::Down, &mut scratch);
                menu.press(Key::Confirm, &mut scratch);
            }
            Page::Controls => {
                menu.press(Key::Down, &mut scratch);
                menu.press(Key::Down, &mut scratch);
                menu.press(Key::Confirm, &mut scratch);
            }
        }
        assert_eq!(menu.page(), page);
        let rows: Vec<String> = menu.lines(&settings).into_iter().map(|line| line.text).collect();
        let body = menu.controls_text();
        let height = pause_height(body.as_deref(), &rows, menu.hint());
        assert!(height <= limit, "{page:?}: the pause menu would be {height:.0}px tall but only {limit:.0}px fit");
    }
    assert_eq!(controls_help().lines().count(), ozbound::pause::CONTROLS.len());
}
