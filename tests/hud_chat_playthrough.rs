//! M8: the slimmed HUD, the chat area, emotes, the side panels (bag, skills, quest book), and
//! signs read in the dialogue panel. Everything here runs on the pure rules (no window), the
//! same way the other playthrough tests do.
//!
//! What is covered:
//!
//! - A whole chat session: speech, emotes, refusals, unknown commands, `/help`, scrolling, and
//!   the log's size limit.
//! - Esc closes typing first, then the rest, and never quits.
//! - The chat area, toolbar, and side panels fit the smallest window at every text size, and
//!   the dialogue panel stands clear of the chat area.
//! - The bag and skills panels fit and lose nothing even with every item and skill maxed.
//! - Every shipped sign fits the dialogue panel and closes with its single choice.
//! - Every hotkey the toolbar shows is listed in the controls.

use ozbound::chat::{handle, help_text, parse, ChatInput, ChatLog, Kind, Submission, MAX_INPUT_CHARS, MAX_MESSAGES};
use ozbound::emote::{can_start, ActiveEmote, Emote, Pose};
use ozbound::equipment::Equipment;
use ozbound::inventory::{Inventory, ItemId};
use ozbound::pause::{controls_help, on_escape, EscapeAction, Open};
use ozbound::quests::QuestLog;
use ozbound::settings::TextSize;
use ozbound::side_panel::{bag_text, skills_text, Dock, Tab};
use ozbound::skills::Skills;
use ozbound::talk::{Talk, READING_DONE};
use ozbound::text_fit::{
    chars_per_line, dialogue_height, fit_scale, lines_per_page, logical_size, paginate, toolbar_width, CHAT_HEIGHT, CHAT_LINES, CHAT_PANEL, CHAT_PX,
    DIALOGUE_PANEL, DOCK_PANEL, JOURNAL_PX, MARGIN, MIN_LOGICAL, MIN_WINDOW, TOOLBAR_BUTTON_HEIGHT,
};
use ozbound::village::builtin;

/// Types `text` into the box and sends it, as the keyboard would.
fn say(log: &mut ChatLog, text: &str, free: Result<(), &'static str>) -> Option<Emote> {
    let mut input = ChatInput::new();
    input.open();
    for c in text.chars() {
        input.type_char(c);
    }
    input.submit().and_then(|submission| handle(log, submission, free))
}

fn lines(log: &ChatLog) -> Vec<(Kind, String)> {
    log.messages().map(|m| (m.kind, m.display())).collect()
}

#[test]
fn a_chat_session_from_greeting_to_dancing() {
    let mut log = ChatLog::new();
    log.push(Kind::Game, "You gathered 1 from the tree (2 left).");
    log.push(Kind::Examine, "A sturdy oak. It could be felled for timber.");

    assert_eq!(say(&mut log, "hello!", Ok(())), None);
    assert_eq!(say(&mut log, "/wave", Ok(())), Some(Emote::Wave));
    assert_eq!(say(&mut log, "/DANCE", Ok(())), Some(Emote::Dance));
    assert_eq!(say(&mut log, "/cheer now", Ok(())), None, "extra words are refused");
    assert_eq!(say(&mut log, "/fly", Ok(())), None);
    assert_eq!(say(&mut log, "/help", Ok(())), None);
    assert_eq!(say(&mut log, "   ", Ok(())), None, "blank input sends nothing");

    let log_lines = lines(&log);
    assert_eq!(log_lines.len(), 2 + 6, "one line for each non-blank send: {log_lines:#?}");
    assert_eq!(log_lines[2], (Kind::Player, "You: hello!".to_string()));
    assert_eq!(log_lines[3], (Kind::Emote, "You wave.".to_string()));
    assert_eq!(log_lines[4], (Kind::Emote, "You dance.".to_string()));
    assert_eq!(log_lines[5].0, Kind::System);
    assert!(log_lines[5].1.contains("no extra words"));
    assert!(log_lines[6].1.contains("/fly"));
    assert_eq!(log_lines[7].1, help_text());
}

#[test]
fn emotes_are_refused_while_walking_or_working_and_the_chat_says_why() {
    let mut log = ChatLog::new();
    assert_eq!(say(&mut log, "/wave", can_start(true, false)), None);
    assert_eq!(say(&mut log, "/dance", can_start(false, true)), None);
    let all = lines(&log);
    assert!(all.iter().all(|(kind, _)| *kind == Kind::System), "{all:?}");
    assert!(all[0].1.contains("walking"));
    assert!(!all.iter().any(|(_, text)| text.starts_with("You wave") || text.starts_with("You dance")), "nothing was performed");
    // Once free, the same command works.
    assert_eq!(say(&mut log, "/wave", can_start(false, false)), Some(Emote::Wave));
}

#[test]
fn a_played_emote_runs_its_course_and_ends_at_rest() {
    for emote in Emote::ALL {
        let mut active = ActiveEmote::start(emote);
        let mut frames = 0;
        let mut moved = false;
        while !active.is_finished() {
            active.tick(1.0 / 60.0);
            moved |= active.pose() != Pose::REST;
            frames += 1;
            assert!(frames < 60 * 10, "{emote:?} must end");
        }
        assert!(moved, "{emote:?} did something visible");
        assert_eq!(active.pose(), Pose::REST, "{emote:?} ends at rest");
        assert!((frames as f32 / 60.0 - emote.duration()).abs() < 0.1);
    }
}

#[test]
fn nothing_typed_can_change_the_game() {
    // Chat only ever writes to the log: bag, skills, and quests are untouched by any line.
    let mut bag = Inventory::new(20);
    bag.add(ItemId::Timber, 3).unwrap();
    let before = (bag.count(ItemId::Timber), QuestLog::new().to_text(), Skills::new().summary_line());
    let mut log = ChatLog::new();
    for text in ["/wave", "/give timber 99", "/level 10", "/quest complete", "give me everything", "/help", "//", "/"] {
        say(&mut log, text, Ok(()));
    }
    let after = (bag.count(ItemId::Timber), QuestLog::new().to_text(), Skills::new().summary_line());
    assert_eq!(before, after);
}

#[test]
fn pasted_junk_is_cleaned_to_one_short_printable_line() {
    let mut input = ChatInput::new();
    input.open();
    for c in "line one\nline two\t\u{1b}[31m red\r\n".chars().chain("x".repeat(500).chars()) {
        input.type_char(c);
    }
    assert!(input.text().chars().count() <= MAX_INPUT_CHARS);
    assert!(input.text().chars().all(|c| !c.is_control()));
    let Some(Submission::Say(said)) = input.submit() else { panic!("it is speech") };
    assert!(!said.contains('\n'));
}

#[test]
fn a_long_game_keeps_the_newest_messages_and_scrolling_reaches_every_kept_one() {
    let mut log = ChatLog::new();
    for i in 0..MAX_MESSAGES * 2 {
        log.push(Kind::Game, format!("event {i}"));
    }
    assert_eq!(log.len(), MAX_MESSAGES);
    let width = chars_per_line(CHAT_PANEL.inner_width(), CHAT_PX);
    let newest = log.view(width, CHAT_LINES);
    assert_eq!(newest.last().unwrap().text, format!("event {}", MAX_MESSAGES * 2 - 1));

    // Page back until the oldest kept message shows.
    let mut steps = 0;
    while log.view(width, CHAT_LINES).first().map(|l| l.text.as_str()) != Some(&format!("event {}", MAX_MESSAGES)) {
        log.scroll_back(ozbound::chat::SCROLL_STEP);
        steps += 1;
        assert!(steps < MAX_MESSAGES, "scrolling must reach the oldest message");
        if log.newer_hidden() + 1 >= log.len() {
            break;
        }
    }
    assert!(log.view(width, CHAT_LINES).iter().any(|l| l.text == format!("event {MAX_MESSAGES}")));
    // A new message snaps back to the newest.
    log.push(Kind::Game, "fresh");
    assert_eq!(log.view(width, 1)[0].text, "fresh");
}

#[test]
fn escape_cancels_typing_before_anything_else_and_never_quits() {
    let typing_over_panels = Open { chat: true, side_panel: true, dialogue: true, ..Open::default() };
    assert_eq!(on_escape(typing_over_panels), EscapeAction::CloseChat);
    let after = typing_over_panels.after(EscapeAction::CloseChat);
    assert!(!after.chat && after.dialogue && after.side_panel, "only the chat box closed");
    assert_eq!(on_escape(after), EscapeAction::CloseDialogue);
    assert_eq!(on_escape(after.after(EscapeAction::CloseDialogue)), EscapeAction::CloseSidePanel);
    assert_eq!(on_escape(Open::default()), EscapeAction::OpenPause);
}

#[test]
fn the_dock_shows_one_panel_at_a_time_and_every_panel_has_a_way_to_close() {
    let mut dock = Dock::default();
    for tab in Tab::ALL {
        assert!(dock.toggle(tab), "{tab:?} opens");
        assert_eq!(dock.current(), Some(tab));
        assert!(on_escape(Open { side_panel: dock.is_open(), ..Open::default() }) == EscapeAction::CloseSidePanel);
    }
    assert!(!dock.toggle(Tab::Quests), "the same key closes it");
    assert!(!dock.is_open());
}

#[test]
fn the_toolbar_hotkeys_are_all_in_the_controls_help() {
    let help = controls_help();
    for tab in Tab::ALL {
        assert!(help.contains(tab.hotkey()), "{} ({}) is not in the controls: {help}", tab.label(), tab.hotkey());
    }
    assert!(help.contains("T or /"), "{help}");
    assert!(help.contains("PageUp"), "{help}");
}

#[test]
fn chat_toolbar_and_panels_fit_every_window_and_text_size() {
    let labels: Vec<String> = Tab::ALL.iter().map(|tab| tab.button_text()).collect();
    for scale in [1.0, 1.25, 1.5] {
        let (logical_width, logical_height) = logical_size(MIN_WINDOW.0, MIN_WINDOW.1, fit_scale(scale, MIN_WINDOW.0, MIN_WINDOW.1));
        assert!(CHAT_PANEL.width + 2.0 * MARGIN <= logical_width, "chat is too wide at scale {scale}");
        assert!(CHAT_PANEL.offset + CHAT_HEIGHT < logical_height, "chat is too tall at scale {scale}");
        assert!(toolbar_width(&labels) + 2.0 * MARGIN <= logical_width, "toolbar is too wide at scale {scale}");
        assert!(DOCK_PANEL.width + 2.0 * MARGIN <= logical_width, "dock is too wide at scale {scale}");
        // The tracker keeps some room beside the toolbar and the open dock.
        for right in [toolbar_width(&labels), DOCK_PANEL.width] {
            assert!(logical_width - right - 3.0 * MARGIN >= 220.0, "tracker would be squeezed at scale {scale}: {}", logical_width - right);
        }
        assert!(DOCK_PANEL.offset >= MARGIN + TOOLBAR_BUTTON_HEIGHT, "the dock hangs below the toolbar");
    }
    let (_, logical_height) = logical_size(MIN_WINDOW.0, MIN_WINDOW.1, 1.0);
    assert!((logical_height - MIN_LOGICAL.1).abs() < 1.0 || logical_height >= MIN_LOGICAL.1);
}

#[test]
fn the_dialogue_panel_stands_clear_of_the_chat_area() {
    assert!(DIALOGUE_PANEL.offset >= CHAT_PANEL.offset + CHAT_HEIGHT, "the two would overlap");
}

#[test]
fn every_shipped_conversation_and_sign_still_fits_above_the_chat_at_the_smallest_window() {
    let world = builtin().expect("the built-in world loads");
    let limit = MIN_LOGICAL.1 - DIALOGUE_PANEL.offset - MARGIN;
    for conversation in world.content.dialogue.iter() {
        for node in &conversation.nodes {
            let choices: Vec<String> = node.choices.iter().map(|c| c.text.clone()).collect();
            let height = dialogue_height(&node.text, &choices);
            assert!(height <= limit, "{} / {} needs {height:.0}px, only {limit:.0}px fit above the chat", conversation.id, node.id);
        }
    }
    assert!(!world.level.signs.is_empty(), "the world has signs to check");
    for sign in &world.level.signs {
        let reading = Talk::reading("Signpost", &sign.text);
        let height = dialogue_height(&reading.view().text, &reading.view().choices);
        assert!(height <= limit, "the sign at {:?} needs {height:.0}px, only {limit:.0}px fit", sign.tile);
    }
}

#[test]
fn reading_a_sign_ends_with_its_single_choice_and_changes_nothing() {
    let world = builtin().unwrap();
    let sign = world.level.signs.first().expect("a sign");
    let mut reading = Talk::reading("Signpost", &sign.text);
    assert_eq!(reading.view().choices, [READING_DONE]);
    assert_eq!(reading.view().text, sign.text);
    let (mut log, mut bag, skills) = (QuestLog::new(), Inventory::new(20), Skills::new());
    let before = log.to_text();
    let (messages, open) = reading.choose(&world.content, &mut log, &mut bag, &skills, 0);
    assert!(messages.is_empty() && !open);
    assert_eq!(log.to_text(), before);
    assert!(bag.stacks().is_empty());
}

#[test]
fn the_bag_and_skills_panels_fit_and_lose_nothing_with_everything_owned() {
    let mut bag = Inventory::new(20);
    for item in [ItemId::Moonpetal, ItemId::Timber, ItemId::CopperOre, ItemId::Meadowmint, ItemId::Honeyberry, ItemId::MintTea] {
        bag.add(item, 40).unwrap();
    }
    bag.add(ItemId::WalkingStaff, 1).unwrap();
    bag.add(ItemId::CopperCharm, 1).unwrap();
    let mut gear = Equipment::new();
    gear.equip(ItemId::WalkingStaff, &mut bag).unwrap();
    gear.equip(ItemId::CopperCharm, &mut bag).unwrap();

    let width = chars_per_line(DOCK_PANEL.inner_width(), JOURNAL_PX);
    let per_page = lines_per_page(DOCK_PANEL.inner_height(MIN_LOGICAL.1), JOURNAL_PX).saturating_sub(4).max(1);
    for (name, body) in [("bag", bag_text(&bag, &gear)), ("skills", skills_text(&Skills::at_level(10))), ("skills (new)", skills_text(&Skills::new()))] {
        let pages = paginate(&body, width, per_page);
        assert!(pages.len() <= 4, "{name} needs {} pages", pages.len());
        for page in &pages {
            assert!(page.lines().count() <= per_page, "{name}: a page is too tall");
            assert!(page.lines().all(|line| line.chars().count() <= width), "{name}: a line is too wide");
        }
        let kept: Vec<&str> = pages.iter().flat_map(|p| p.split_whitespace()).collect();
        let original: Vec<&str> = body.split_whitespace().collect();
        assert_eq!(kept, original, "{name}: paging lost or reordered text");
    }
}

#[test]
fn the_text_size_setting_still_leaves_chat_and_toolbar_readable() {
    // The largest size is reduced to fit small windows, never clipped.
    let requested = {
        let mut settings = ozbound::settings::Settings::default();
        settings.text_size = TextSize::Larger;
        settings.requested_ui_scale()
    };
    let scale = fit_scale(requested, MIN_WINDOW.0, MIN_WINDOW.1);
    let (logical_width, _) = logical_size(MIN_WINDOW.0, MIN_WINDOW.1, scale);
    assert!(chars_per_line(CHAT_PANEL.inner_width(), CHAT_PX) >= 30, "chat lines stay a readable length");
    assert!(CHAT_PANEL.width + 2.0 * MARGIN <= logical_width);
    assert_eq!(parse("/help"), Some(Submission::Help));
}
