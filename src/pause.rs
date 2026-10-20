//! The pause menu as a pure state machine, plus the rule for what Esc does.
//!
//! The Bevy layer (`pause_ui.rs`) turns key presses and clicks into [`Key`]s and draws
//! [`PauseMenu::lines`]; everything about which row is selected, what a key changes, and when
//! the game should resume or quit is decided here and tested without a window. The whole menu
//! works from the keyboard alone (Up/Down, Left/Right, Enter, Esc), and a click on a row is the
//! same as selecting it and pressing Enter, so the mouse never reaches anything the keyboard
//! cannot.
//!
//! # Rules
//!
//! - Up and Down move the selection and wrap round.
//! - Enter on a row does its main action; Left and Right change a value row (volume, text size)
//!   or toggle it; on other rows they do nothing.
//! - Esc (`Key::Back`) steps back one page, and from the first page resumes the game, so
//!   holding Esc always ends up back in the game and never traps the player.
//! - Quitting is its own row and never a side effect of another key.

use crate::settings::{Settings, MAX_VOLUME};

/// A key press the menu understands.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Key {
    Up,
    Down,
    Left,
    Right,
    Confirm,
    Back,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Page {
    Main,
    Settings,
    Controls,
}

/// One selectable row.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Row {
    Resume,
    OpenSettings,
    OpenControls,
    Quit,
    Volume,
    Ambient,
    TextSize,
    Perf,
    Back,
}

const MAIN_ROWS: [Row; 4] = [Row::Resume, Row::OpenSettings, Row::OpenControls, Row::Quit];
const SETTINGS_ROWS: [Row; 5] = [Row::Volume, Row::Ambient, Row::TextSize, Row::Perf, Row::Back];
const CONTROLS_ROWS: [Row; 1] = [Row::Back];

impl Page {
    pub fn rows(self) -> &'static [Row] {
        match self {
            Self::Main => &MAIN_ROWS,
            Self::Settings => &SETTINGS_ROWS,
            Self::Controls => &CONTROLS_ROWS,
        }
    }

    pub fn title(self) -> &'static str {
        match self {
            Self::Main => "PAUSED",
            Self::Settings => "SETTINGS",
            Self::Controls => "CONTROLS",
        }
    }
}

/// What a key press asks the game to do.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    Nothing,
    /// Close the menu and carry on playing.
    Resume,
    /// Exit the game (the game saves on exit).
    Quit,
    /// A setting changed: apply it and write the settings file.
    SettingsChanged,
}

/// One drawn line of the menu.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Line {
    pub text: String,
    pub selected: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PauseMenu {
    page: Page,
    selected: usize,
}

impl Default for PauseMenu {
    fn default() -> Self {
        Self::new()
    }
}

impl PauseMenu {
    /// A menu on its first page with Resume selected.
    pub fn new() -> Self {
        Self { page: Page::Main, selected: 0 }
    }

    pub fn page(&self) -> Page {
        self.page
    }

    pub fn selected(&self) -> usize {
        self.selected
    }

    pub fn selected_row(&self) -> Row {
        self.page.rows()[self.selected]
    }

    fn go_to(&mut self, page: Page, select: Row) {
        self.page = page;
        self.selected = page.rows().iter().position(|row| *row == select).unwrap_or(0);
    }

    /// Applies one key press. Settings are changed in place; the outcome says what else the game
    /// must do.
    pub fn press(&mut self, key: Key, settings: &mut Settings) -> Outcome {
        let count = self.page.rows().len();
        match key {
            Key::Up => {
                self.selected = (self.selected + count - 1) % count;
                Outcome::Nothing
            }
            Key::Down => {
                self.selected = (self.selected + 1) % count;
                Outcome::Nothing
            }
            Key::Back => match self.page {
                Page::Main => Outcome::Resume,
                Page::Settings => {
                    self.go_to(Page::Main, Row::OpenSettings);
                    Outcome::Nothing
                }
                Page::Controls => {
                    self.go_to(Page::Main, Row::OpenControls);
                    Outcome::Nothing
                }
            },
            Key::Confirm => match self.selected_row() {
                Row::Resume => Outcome::Resume,
                Row::Quit => Outcome::Quit,
                Row::OpenSettings => {
                    self.go_to(Page::Settings, Row::Volume);
                    Outcome::Nothing
                }
                Row::OpenControls => {
                    self.go_to(Page::Controls, Row::Back);
                    Outcome::Nothing
                }
                Row::Back => self.press(Key::Back, settings),
                // Enter on a value row steps it forward; the volume wraps to silent after the top.
                Row::Volume => {
                    if !settings.volume_up() {
                        settings.set_volume(0);
                    }
                    Outcome::SettingsChanged
                }
                Row::Ambient => {
                    settings.ambient = !settings.ambient;
                    Outcome::SettingsChanged
                }
                Row::TextSize => {
                    settings.text_size = settings.text_size.next();
                    Outcome::SettingsChanged
                }
                Row::Perf => {
                    settings.show_perf = !settings.show_perf;
                    Outcome::SettingsChanged
                }
            },
            Key::Left | Key::Right => {
                let forward = key == Key::Right;
                match self.selected_row() {
                    Row::Volume => {
                        let changed = if forward { settings.volume_up() } else { settings.volume_down() };
                        if changed { Outcome::SettingsChanged } else { Outcome::Nothing }
                    }
                    Row::Ambient => {
                        settings.ambient = !settings.ambient;
                        Outcome::SettingsChanged
                    }
                    Row::TextSize => {
                        settings.text_size = if forward { settings.text_size.next() } else { settings.text_size.previous() };
                        Outcome::SettingsChanged
                    }
                    Row::Perf => {
                        settings.show_perf = !settings.show_perf;
                        Outcome::SettingsChanged
                    }
                    _ => Outcome::Nothing,
                }
            }
        }
    }

    /// A click on row `index`: selects it and presses Enter. A click outside the rows does
    /// nothing.
    pub fn click(&mut self, index: usize, settings: &mut Settings) -> Outcome {
        if index >= self.page.rows().len() {
            return Outcome::Nothing;
        }
        self.selected = index;
        self.press(Key::Confirm, settings)
    }

    /// The text of every row for the current settings.
    pub fn lines(&self, settings: &Settings) -> Vec<Line> {
        self.page
            .rows()
            .iter()
            .enumerate()
            .map(|(index, row)| Line { text: row_text(*row, settings), selected: index == self.selected })
            .collect()
    }

    /// The one-line hint under the menu.
    pub fn hint(&self) -> &'static str {
        match self.page {
            Page::Main | Page::Controls => "Up/Down: choose   Enter: select   Esc: back",
            Page::Settings => "Up/Down: choose   Left/Right: change   Enter: change   Esc: back",
        }
    }

    /// The controls help, shown on the controls page: one `keys: what it does` line per control.
    pub fn controls_text(&self) -> Option<String> {
        (self.page == Page::Controls).then(controls_help)
    }
}

fn row_text(row: Row, settings: &Settings) -> String {
    let on_off = |on: bool| if on { "on" } else { "off" };
    match row {
        Row::Resume => "Resume".into(),
        Row::OpenSettings => "Settings".into(),
        Row::OpenControls => "Controls".into(),
        Row::Quit => "Quit game (progress is saved)".into(),
        Row::Volume => {
            let filled = usize::from(settings.volume());
            let bar = format!("{}{}", "#".repeat(filled), "-".repeat(usize::from(MAX_VOLUME) - filled));
            format!("Volume: [{bar}] {} / {MAX_VOLUME}", settings.volume())
        }
        Row::Ambient => format!("Ambient sound: {}", on_off(settings.ambient)),
        Row::TextSize => format!("Text size: {}", settings.text_size.label()),
        Row::Perf => format!("Frame-time overlay (F3): {}", on_off(settings.show_perf)),
        Row::Back => "Back".into(),
    }
}

/// Every control, grouped, as plain text. Shown on the controls page; it is also the single
/// list the README and smoke-test checklist are checked against.
pub fn controls_help() -> String {
    CONTROLS.iter().map(|(keys, what)| format!("{keys}: {what}")).collect::<Vec<_>>().join("\n")
}

/// `(keys, what it does)` for every control. Keep this in step with the README.
pub const CONTROLS: [(&str, &str); 15] = [
    ("Left-click", "walk, or go to an object and use it"),
    ("Right-click", "action menu for what is under the cursor"),
    ("W A S D", "step relative to the camera (hold to walk)"),
    ("Tab / Shift+Tab", "select next / previous thing to visit"),
    ("Enter or Space", "go to the selected thing and use it"),
    ("X", "examine the selected thing"),
    ("1-9", "pick a numbered row (talk, crafting)"),
    ("B / K / J", "open bag / skills / quest book"),
    ("T or /", "chat: Enter sends, /help lists commands"),
    ("PageUp / PageDown", "scroll chat, or turn panel pages"),
    ("C", "crafting and gear"),
    ("Arrow keys / middle-drag", "turn the camera"),
    ("Scroll wheel or + / -", "zoom the camera"),
    ("F3", "frame-time overlay on or off"),
    ("Esc", "close a panel; with none open, pause"),
];

/// Which panels are open right now.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Open {
    pub pause: bool,
    pub context_menu: bool,
    pub dialogue: bool,
    pub crafting: bool,
    /// One of the side panels (bag, skills, quest book) is open.
    pub side_panel: bool,
    /// The chat box is open for typing.
    pub chat: bool,
}

/// What Esc does.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EscapeAction {
    /// The pause menu is open: Esc is a `Key::Back` for it.
    PauseBack,
    CloseContextMenu,
    CloseDialogue,
    CloseCrafting,
    CloseSidePanel,
    /// The chat box is open: Esc cancels what was being typed.
    CloseChat,
    /// Nothing is open: pause the game.
    OpenPause,
}

/// Esc closes the most urgent thing first: the pause menu (it sits on top of everything), then
/// the right-click menu, the chat box (you are mid-sentence), a conversation, the crafting
/// panel, a side panel, and with nothing open it pauses. Esc never quits by itself.
pub fn on_escape(open: Open) -> EscapeAction {
    if open.pause {
        EscapeAction::PauseBack
    } else if open.context_menu {
        EscapeAction::CloseContextMenu
    } else if open.chat {
        EscapeAction::CloseChat
    } else if open.dialogue {
        EscapeAction::CloseDialogue
    } else if open.crafting {
        EscapeAction::CloseCrafting
    } else if open.side_panel {
        EscapeAction::CloseSidePanel
    } else {
        EscapeAction::OpenPause
    }
}

impl Open {
    /// The panels left open after `action` has been carried out. `PauseBack` is handled by the
    /// pause menu itself (see [`PauseMenu::press`]) and is treated here as closing it, which is
    /// what pressing Back on its first page does.
    pub fn after(mut self, action: EscapeAction) -> Self {
        match action {
            EscapeAction::PauseBack => self.pause = false,
            EscapeAction::CloseContextMenu => self.context_menu = false,
            EscapeAction::CloseDialogue => self.dialogue = false,
            EscapeAction::CloseCrafting => self.crafting = false,
            EscapeAction::CloseSidePanel => self.side_panel = false,
            EscapeAction::CloseChat => self.chat = false,
            EscapeAction::OpenPause => self.pause = true,
        }
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn menu_and_settings() -> (PauseMenu, Settings) {
        (PauseMenu::new(), Settings::default())
    }

    fn go_to_settings(menu: &mut PauseMenu, settings: &mut Settings) {
        menu.press(Key::Down, settings);
        assert_eq!(menu.press(Key::Confirm, settings), Outcome::Nothing);
        assert_eq!(menu.page(), Page::Settings);
    }

    #[test]
    fn a_new_menu_starts_on_resume() {
        let (menu, settings) = menu_and_settings();
        assert_eq!(menu.page(), Page::Main);
        assert_eq!(menu.selected_row(), Row::Resume);
        let lines = menu.lines(&settings);
        assert!(lines[0].selected && lines.iter().filter(|l| l.selected).count() == 1);
    }

    #[test]
    fn up_and_down_wrap_round_every_page() {
        for page in [Page::Main, Page::Settings, Page::Controls] {
            let (mut menu, mut settings) = menu_and_settings();
            menu.page = page;
            let count = page.rows().len();
            menu.press(Key::Up, &mut settings);
            assert_eq!(menu.selected(), count - 1, "{page:?}: up from the top wraps to the bottom");
            menu.press(Key::Down, &mut settings);
            assert_eq!(menu.selected(), 0, "{page:?}: down from the bottom wraps to the top");
            for _ in 0..count {
                menu.press(Key::Down, &mut settings);
            }
            assert_eq!(menu.selected(), 0, "{page:?}: a full lap comes back");
        }
    }

    #[test]
    fn resume_quit_and_back_do_what_they_say() {
        let (mut menu, mut settings) = menu_and_settings();
        assert_eq!(menu.press(Key::Confirm, &mut settings), Outcome::Resume);
        assert_eq!(menu.press(Key::Back, &mut settings), Outcome::Resume, "Esc on the first page resumes");
        menu.press(Key::Up, &mut settings);
        assert_eq!(menu.selected_row(), Row::Quit);
        assert_eq!(menu.press(Key::Confirm, &mut settings), Outcome::Quit);
        // Quit is only ever a deliberate Enter on its own row.
        let mut other = PauseMenu::new();
        for key in [Key::Left, Key::Right, Key::Back, Key::Down, Key::Up] {
            assert_ne!(other.press(key, &mut settings), Outcome::Quit, "{key:?}");
        }
    }

    #[test]
    fn settings_and_controls_open_and_close_with_the_selection_kept_sensible() {
        let (mut menu, mut settings) = menu_and_settings();
        go_to_settings(&mut menu, &mut settings);
        assert_eq!(menu.selected_row(), Row::Volume);
        assert_eq!(menu.press(Key::Back, &mut settings), Outcome::Nothing);
        assert_eq!((menu.page(), menu.selected_row()), (Page::Main, Row::OpenSettings), "back lands on the row it came from");

        menu.press(Key::Down, &mut settings);
        assert_eq!(menu.selected_row(), Row::OpenControls);
        menu.press(Key::Confirm, &mut settings);
        assert_eq!(menu.page(), Page::Controls);
        assert!(menu.controls_text().is_some());
        assert_eq!(menu.press(Key::Confirm, &mut settings), Outcome::Nothing, "the Back row goes back");
        assert_eq!((menu.page(), menu.selected_row()), (Page::Main, Row::OpenControls));
        assert!(menu.controls_text().is_none());
    }

    #[test]
    fn left_and_right_change_volume_by_one_and_stop_at_the_ends() {
        let (mut menu, mut settings) = menu_and_settings();
        go_to_settings(&mut menu, &mut settings);
        assert_eq!(menu.press(Key::Right, &mut settings), Outcome::SettingsChanged);
        assert_eq!(settings.volume(), 8);
        assert_eq!(menu.press(Key::Left, &mut settings), Outcome::SettingsChanged);
        assert_eq!(settings.volume(), 7);
        settings.set_volume(MAX_VOLUME);
        assert_eq!(menu.press(Key::Right, &mut settings), Outcome::Nothing, "no wrap with the arrows");
        assert_eq!(settings.volume(), MAX_VOLUME);
        settings.set_volume(0);
        assert_eq!(menu.press(Key::Left, &mut settings), Outcome::Nothing);
        assert_eq!(settings.volume(), 0);
    }

    #[test]
    fn enter_on_volume_steps_up_and_wraps_to_silent_so_a_click_can_reach_every_value() {
        let (mut menu, mut settings) = menu_and_settings();
        go_to_settings(&mut menu, &mut settings);
        settings.set_volume(9);
        assert_eq!(menu.press(Key::Confirm, &mut settings), Outcome::SettingsChanged);
        assert_eq!(settings.volume(), MAX_VOLUME);
        assert_eq!(menu.press(Key::Confirm, &mut settings), Outcome::SettingsChanged);
        assert_eq!(settings.volume(), 0, "past the top comes back to silent");
        let mut seen = std::collections::BTreeSet::new();
        for _ in 0..=MAX_VOLUME {
            menu.press(Key::Confirm, &mut settings);
            seen.insert(settings.volume());
        }
        assert_eq!(seen.len(), usize::from(MAX_VOLUME) + 1, "every value is reachable with Enter alone");
    }

    #[test]
    fn toggles_and_text_size_change_with_either_arrow_or_enter() {
        let (mut menu, mut settings) = menu_and_settings();
        go_to_settings(&mut menu, &mut settings);
        menu.press(Key::Down, &mut settings);
        assert_eq!(menu.selected_row(), Row::Ambient);
        assert_eq!(menu.press(Key::Left, &mut settings), Outcome::SettingsChanged);
        assert!(!settings.ambient);
        assert_eq!(menu.press(Key::Confirm, &mut settings), Outcome::SettingsChanged);
        assert!(settings.ambient);

        menu.press(Key::Down, &mut settings);
        assert_eq!(menu.selected_row(), Row::TextSize);
        menu.press(Key::Right, &mut settings);
        assert_eq!(settings.text_size.label(), "Large");
        menu.press(Key::Left, &mut settings);
        menu.press(Key::Left, &mut settings);
        assert_eq!(settings.text_size.label(), "Larger", "left wraps from the smallest to the largest");
        menu.press(Key::Confirm, &mut settings);
        assert_eq!(settings.text_size.label(), "Normal");

        menu.press(Key::Down, &mut settings);
        assert_eq!(menu.selected_row(), Row::Perf);
        assert_eq!(menu.press(Key::Right, &mut settings), Outcome::SettingsChanged);
        assert!(settings.show_perf);
    }

    #[test]
    fn arrows_do_nothing_on_rows_that_are_not_values() {
        let (mut menu, mut settings) = menu_and_settings();
        let before = settings;
        for key in [Key::Left, Key::Right] {
            assert_eq!(menu.press(key, &mut settings), Outcome::Nothing);
        }
        assert_eq!(settings, before);
        go_to_settings(&mut menu, &mut settings);
        menu.press(Key::Up, &mut settings);
        assert_eq!(menu.selected_row(), Row::Back);
        assert_eq!(menu.press(Key::Right, &mut settings), Outcome::Nothing);
        assert_eq!(menu.page(), Page::Settings);
    }

    #[test]
    fn a_click_selects_and_presses_enter_and_ignores_outside_rows() {
        let (mut menu, mut settings) = menu_and_settings();
        assert_eq!(menu.click(3, &mut settings), Outcome::Quit);
        assert_eq!(menu.click(1, &mut settings), Outcome::Nothing);
        assert_eq!(menu.page(), Page::Settings);
        assert_eq!(menu.click(1, &mut settings), Outcome::SettingsChanged, "second row of settings is Ambient");
        assert!(!settings.ambient);
        let before = (menu.clone(), settings);
        assert_eq!(menu.click(99, &mut settings), Outcome::Nothing);
        assert_eq!((menu, settings), before, "a click outside the rows changes nothing");
    }

    #[test]
    fn lines_show_current_values_and_exactly_one_selection() {
        let (mut menu, mut settings) = menu_and_settings();
        go_to_settings(&mut menu, &mut settings);
        let lines = menu.lines(&settings);
        assert_eq!(lines.len(), 5);
        assert!(lines[0].text.contains("[#######---] 7 / 10"), "{}", lines[0].text);
        assert!(lines[1].text.ends_with("on") && lines[2].text.ends_with("Normal") && lines[3].text.ends_with("off"));
        assert_eq!(lines.iter().filter(|l| l.selected).count(), 1);
        settings.set_volume(0);
        settings.ambient = false;
        let lines = menu.lines(&settings);
        assert!(lines[0].text.contains("[----------] 0 / 10") && lines[1].text.ends_with("off"));
        settings.set_volume(MAX_VOLUME);
        assert!(menu.lines(&settings)[0].text.contains("[##########] 10 / 10"));
    }

    #[test]
    fn every_key_on_every_page_and_row_leaves_a_valid_state() {
        let keys = [Key::Up, Key::Down, Key::Left, Key::Right, Key::Confirm, Key::Back];
        for page in [Page::Main, Page::Settings, Page::Controls] {
            for start in 0..page.rows().len() {
                for key in keys {
                    let mut menu = PauseMenu { page, selected: start };
                    let mut settings = Settings::default();
                    menu.press(key, &mut settings);
                    assert!(menu.selected() < menu.page().rows().len(), "{page:?} row {start} {key:?}");
                    assert!(settings.volume() <= MAX_VOLUME);
                }
            }
        }
    }

    #[test]
    fn repeated_back_always_resumes_from_anywhere() {
        for page in [Page::Main, Page::Settings, Page::Controls] {
            let mut menu = PauseMenu { page, selected: 0 };
            let mut settings = Settings::default();
            let presses = (1..=3).find(|_| menu.press(Key::Back, &mut settings) == Outcome::Resume);
            assert!(presses.is_some(), "{page:?}: Esc must get back to the game within three presses");
        }
    }

    #[test]
    fn every_page_title_hint_and_row_is_present_and_every_page_can_go_back() {
        let settings = Settings::default();
        for page in [Page::Main, Page::Settings, Page::Controls] {
            assert!(!page.title().is_empty());
            let menu = PauseMenu { page, selected: 0 };
            assert!(menu.hint().contains("Esc"));
            assert!(!menu.lines(&settings).is_empty());
            assert!(page == Page::Main || page.rows().contains(&Row::Back), "{page:?} has a visible way back");
        }
    }

    #[test]
    fn the_controls_list_has_no_empty_or_duplicate_entries() {
        let mut keys = std::collections::HashSet::new();
        for (key, what) in CONTROLS {
            assert!(!key.is_empty() && !what.is_empty());
            assert!(keys.insert(key), "{key} is listed twice");
        }
        let text = controls_help();
        assert_eq!(text.lines().count(), CONTROLS.len());
        for line in text.lines() {
            assert!(line.chars().count() <= 56, "{line:?} is too long to stay on one line in the pause panel");
        }
        for essential in ["Tab", "Enter", "Esc", "W A S D", "B / K / J", "C", "T or /", "PageUp", "F3"] {
            assert!(text.contains(essential), "the help must mention {essential}");
        }
    }

    #[test]
    fn escape_closes_the_most_urgent_thing_first() {
        let all = Open { pause: true, context_menu: true, dialogue: true, crafting: true, side_panel: true, chat: true };
        let mut open = all;
        let mut order = Vec::new();
        for _ in 0..7 {
            let action = on_escape(open);
            order.push(action);
            open = open.after(action);
        }
        use EscapeAction::*;
        assert_eq!(order[..6], [PauseBack, CloseContextMenu, CloseChat, CloseDialogue, CloseCrafting, CloseSidePanel]);
        assert_eq!(order[6], OpenPause, "with nothing open, Esc pauses");
    }

    #[test]
    fn escape_never_quits_and_always_gets_back_to_a_calm_state() {
        // Every combination of open panels: Esc always does exactly one thing, never leaves a
        // panel it did not name, and within seven presses reaches "paused with nothing else open".
        for bits in 0u8..64 {
            let start = Open {
                pause: bits & 1 != 0,
                context_menu: bits & 2 != 0,
                dialogue: bits & 4 != 0,
                crafting: bits & 8 != 0,
                side_panel: bits & 16 != 0,
                chat: bits & 32 != 0,
            };
            let mut open = start;
            let mut presses = 0;
            while open != Open::default() && presses < 7 {
                let action = on_escape(open);
                let next = open.after(action);
                assert_ne!(next, open, "{start:?}: {action:?} must change something");
                open = next;
                presses += 1;
            }
            assert!(presses <= 6, "{start:?}: six panels at most, so six presses at most");
            // And one more press from a clean screen pauses the game.
            assert_eq!(on_escape(Open::default()), EscapeAction::OpenPause);
        }
    }

    #[test]
    fn opening_pause_then_pressing_escape_resumes() {
        let open = Open::default().after(on_escape(Open::default()));
        assert!(open.pause);
        assert_eq!(on_escape(open), EscapeAction::PauseBack);
        let mut menu = PauseMenu::new();
        let mut settings = Settings::default();
        assert_eq!(menu.press(Key::Back, &mut settings), Outcome::Resume);
        assert_eq!(open.after(EscapeAction::PauseBack), Open::default());
    }
}
