//! The chat area as plain data: the message log, the text-entry box, and the slash commands.
//! Pure Rust (no Bevy); the Bevy layer draws `ChatLog::view` and feeds typed characters in.
//!
//! What goes in the log:
//!
//! - **Game** lines: everything the game used to print in the HUD (gathering, quest updates,
//!   refusals such as "You can't walk there.").
//! - **Examine** lines: what an object looks like when the player examines it.
//! - **Player** lines: what the player typed, shown as `You: ...`.
//! - **Emote** lines: `You wave.` and friends.
//! - **System** lines: answers to chat commands (`/help`, unknown commands).
//!
//! Rules:
//!
//! - The log keeps the newest [`MAX_MESSAGES`] messages; older ones are dropped. Blank
//!   messages are ignored.
//! - The newest message is always shown unless the player has scrolled back; a new message
//!   brings the view back to the newest.
//! - Typed text is limited to [`MAX_INPUT_CHARS`] printable characters. Control characters
//!   (tabs, newlines, escapes) are refused, so a line is always one line.
//! - Text that starts with `/` is a command, anything else is spoken. A command that is not
//!   known, or a known emote given extra words, is answered with a System line and does
//!   nothing else. Nothing a player types ever changes game state.

use std::collections::VecDeque;

use crate::emote::Emote;
use crate::text_fit::wrap;

/// How many messages the log keeps.
pub const MAX_MESSAGES: usize = 200;
/// How many characters the text-entry box accepts.
pub const MAX_INPUT_CHARS: usize = 100;
/// How many messages one scroll step moves.
pub const SCROLL_STEP: usize = 3;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Kind {
    Game,
    Examine,
    Player,
    Emote,
    System,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Message {
    pub kind: Kind,
    pub text: String,
}

impl Message {
    /// The text as shown: the player's own words get a `You: ` prefix.
    pub fn display(&self) -> String {
        match self.kind {
            Kind::Player => format!("You: {}", self.text),
            _ => self.text.clone(),
        }
    }
}

/// One wrapped line of the visible log.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Line {
    pub kind: Kind,
    pub text: String,
}

#[derive(Debug, Clone, Default)]
pub struct ChatLog {
    messages: VecDeque<Message>,
    /// How many of the newest messages are scrolled out of view.
    newer_hidden: usize,
}

impl ChatLog {
    pub fn new() -> Self {
        Self::default()
    }

    /// Adds a message (trimmed) and shows the newest again. Blank text is ignored and gives
    /// `false`.
    pub fn push(&mut self, kind: Kind, text: impl Into<String>) -> bool {
        let text: String = text.into();
        let text = text.trim();
        if text.is_empty() {
            return false;
        }
        self.messages.push_back(Message { kind, text: text.to_string() });
        while self.messages.len() > MAX_MESSAGES {
            self.messages.pop_front();
        }
        self.newer_hidden = 0;
        true
    }

    pub fn len(&self) -> usize {
        self.messages.len()
    }

    pub fn is_empty(&self) -> bool {
        self.messages.is_empty()
    }

    pub fn messages(&self) -> impl Iterator<Item = &Message> {
        self.messages.iter()
    }

    /// Scrolls toward older messages, never past the oldest one.
    pub fn scroll_back(&mut self, messages: usize) {
        self.newer_hidden = (self.newer_hidden + messages).min(self.messages.len().saturating_sub(1));
    }

    /// Scrolls toward newer messages, never past the newest one.
    pub fn scroll_forward(&mut self, messages: usize) {
        self.newer_hidden = self.newer_hidden.saturating_sub(messages);
    }

    pub fn is_scrolled_back(&self) -> bool {
        self.newer_hidden > 0
    }

    /// How many newer messages are out of view.
    pub fn newer_hidden(&self) -> usize {
        self.newer_hidden
    }

    /// The last `max_lines` lines of the visible log, each at most `width` characters. A
    /// message that wraps keeps its first line flush left and indents the rest by two spaces,
    /// so one message is easy to tell from the next. When the view is cut, the oldest visible
    /// message may show only its last lines.
    pub fn view(&self, width: usize, max_lines: usize) -> Vec<Line> {
        let width = width.max(3);
        let end = self.messages.len() - self.newer_hidden;
        let mut newest_first: Vec<Line> = Vec::new();
        'collect: for message in self.messages.iter().take(end).rev() {
            let wrapped = wrap(&message.display(), width - 2);
            for (index, line) in wrapped.into_iter().enumerate().rev() {
                if newest_first.len() == max_lines {
                    break 'collect;
                }
                let text = if index == 0 { line } else { format!("  {line}") };
                newest_first.push(Line { kind: message.kind, text });
            }
            if newest_first.len() == max_lines {
                break;
            }
        }
        newest_first.reverse();
        newest_first
    }
}

/// What a submitted line means.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Submission {
    /// Plain speech.
    Say(String),
    Emote(Emote),
    Help,
    /// `/edit`: turns the world editor on or off (see `editor.rs`).
    Edit,
    /// A command that did not work, with the System line that explains why.
    Error(String),
}

/// The text of `/help`.
pub fn help_text() -> String {
    let commands: Vec<String> = Emote::ALL.iter().map(|emote| format!("/{}", emote.command())).collect();
    format!("Commands: {}, /edit (world editor), /help. Anything else you type is said aloud.", commands.join(", "))
}

/// Reads a line the player typed. Blank text is `None`.
pub fn parse(text: &str) -> Option<Submission> {
    let text = text.trim();
    if text.is_empty() {
        return None;
    }
    let Some(command) = text.strip_prefix('/') else {
        return Some(Submission::Say(text.to_string()));
    };
    let mut words = command.split_whitespace();
    let Some(word) = words.next() else {
        return Some(Submission::Error("Type a command after the slash, such as /wave. /help lists them.".into()));
    };
    let extra = words.next().is_some();
    let word = word.to_lowercase();
    if word == "help" {
        return Some(if extra { Submission::Error("/help takes no extra words.".into()) } else { Submission::Help });
    }
    if word == "edit" {
        return Some(if extra { Submission::Error("/edit takes no extra words.".into()) } else { Submission::Edit });
    }
    match Emote::from_command(&word) {
        Some(_) if extra => Some(Submission::Error(format!("/{word} takes no extra words."))),
        Some(emote) => Some(Submission::Emote(emote)),
        None => Some(Submission::Error(format!("Unknown command /{word}. Type /help to see the commands."))),
    }
}

/// Writes the log lines a submission causes and says which emote, if any, should now play.
/// `emote_allowed` is the answer of [`crate::emote::can_start`] for the player right now.
pub fn handle(log: &mut ChatLog, submission: Submission, emote_allowed: Result<(), &'static str>) -> Option<Emote> {
    match submission {
        Submission::Say(text) => {
            log.push(Kind::Player, text);
            None
        }
        Submission::Emote(emote) => match emote_allowed {
            Ok(()) => {
                log.push(Kind::Emote, emote.message());
                Some(emote)
            }
            Err(reason) => {
                log.push(Kind::System, reason);
                None
            }
        },
        Submission::Help => {
            log.push(Kind::System, help_text());
            None
        }
        // The editor logs its own lines (it knows whether it was switched on or off).
        Submission::Edit => None,
        Submission::Error(message) => {
            log.push(Kind::System, message);
            None
        }
    }
}

/// The text-entry box: closed, or open with what has been typed so far.
#[derive(Debug, Clone, Default)]
pub struct ChatInput {
    open: bool,
    text: String,
}

impl ChatInput {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn is_open(&self) -> bool {
        self.open
    }

    pub fn text(&self) -> &str {
        &self.text
    }

    /// Opens the box empty.
    pub fn open(&mut self) {
        self.open = true;
        self.text.clear();
    }

    /// Opens the box already holding `prefill` (for example `/`), cut to the limit.
    pub fn open_with(&mut self, prefill: &str) {
        self.open();
        for c in prefill.chars() {
            self.type_char(c);
        }
    }

    /// Closes the box and forgets what was typed.
    pub fn close(&mut self) {
        self.open = false;
        self.text.clear();
    }

    /// Adds one character. Refused (`false`) when the box is closed, the character is a
    /// control character, or the box is full.
    pub fn type_char(&mut self, c: char) -> bool {
        if !self.open || c.is_control() || self.text.chars().count() >= MAX_INPUT_CHARS {
            return false;
        }
        self.text.push(c);
        true
    }

    /// Removes the last character, if any.
    pub fn backspace(&mut self) -> bool {
        self.open && self.text.pop().is_some()
    }

    /// Closes the box and reads what was typed. `None` when nothing but spaces was typed
    /// (or the box was closed), so Enter on an empty box just closes it.
    pub fn submit(&mut self) -> Option<Submission> {
        if !self.open {
            return None;
        }
        let parsed = parse(&self.text);
        self.close();
        parsed
    }

    /// The box as one line of at most `width` characters: `> ` and the text with a cursor. A
    /// long text shows its end, after `...` (or alone, in a very narrow box), so what is being
    /// typed is always visible.
    pub fn display(&self, width: usize) -> String {
        let width = width.max(6);
        let room = width - 3; // "> " and the cursor
        let count = self.text.chars().count();
        let shown: String = if count <= room {
            self.text.clone()
        } else if room > 4 {
            let tail: String = self.text.chars().skip(count - (room - 3)).collect();
            format!("...{tail}")
        } else {
            // Too narrow for an ellipsis and a useful tail: just show the end.
            self.text.chars().skip(count - room).collect()
        };
        format!("> {shown}_")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn log_of(lines: &[(Kind, &str)]) -> ChatLog {
        let mut log = ChatLog::new();
        for (kind, text) in lines {
            log.push(*kind, *text);
        }
        log
    }

    fn texts(lines: &[Line]) -> Vec<&str> {
        lines.iter().map(|l| l.text.as_str()).collect()
    }

    #[test]
    fn a_new_log_is_empty_and_blank_messages_are_ignored() {
        let mut log = ChatLog::new();
        assert!(log.is_empty());
        assert!(!log.push(Kind::Game, ""));
        assert!(!log.push(Kind::Game, "   \n "));
        assert!(log.is_empty());
        assert!(log.push(Kind::Game, "  hello  "));
        assert_eq!(log.messages().next().unwrap().text, "hello", "messages are trimmed");
        assert!(log.view(40, 5).len() == 1);
        assert!(ChatLog::new().view(40, 5).is_empty());
    }

    #[test]
    fn the_log_keeps_only_the_newest_messages() {
        let mut log = ChatLog::new();
        for i in 0..MAX_MESSAGES + 25 {
            log.push(Kind::Game, format!("message {i}"));
        }
        assert_eq!(log.len(), MAX_MESSAGES);
        assert_eq!(log.messages().next().unwrap().text, "message 25");
        assert_eq!(log.messages().last().unwrap().text, format!("message {}", MAX_MESSAGES + 24));
    }

    #[test]
    fn the_view_shows_the_newest_lines_in_order() {
        let log = log_of(&[(Kind::Game, "one"), (Kind::Examine, "two"), (Kind::Game, "three"), (Kind::Game, "four")]);
        let view = log.view(30, 3);
        assert_eq!(texts(&view), ["two", "three", "four"]);
        assert_eq!(view[0].kind, Kind::Examine);
        assert_eq!(texts(&log.view(30, 10)), ["one", "two", "three", "four"]);
        assert!(log.view(30, 0).is_empty());
    }

    #[test]
    fn player_lines_are_prefixed_and_other_lines_are_not() {
        let log = log_of(&[(Kind::Player, "hello"), (Kind::Emote, "You wave.")]);
        assert_eq!(texts(&log.view(30, 5)), ["You: hello", "You wave."]);
    }

    #[test]
    fn long_messages_wrap_with_an_indent_and_never_exceed_the_width() {
        let log = log_of(&[(Kind::Game, "You gathered 1 from the tree (3 left). Your Woodcraft level is now 2."), (Kind::Game, "short")]);
        for width in [8, 12, 20, 33, 60] {
            let view = log.view(width, 50);
            for line in &view {
                assert!(line.text.chars().count() <= width, "width {width}: {:?}", line.text);
            }
            let joined: String = view.iter().map(|l| l.text.trim()).collect::<Vec<_>>().join("");
            assert_eq!(joined.replace(' ', ""), "Yougathered1fromthetree(3left).YourWoodcraftlevelisnow2.short".replace(' ', ""), "width {width}: nothing is lost");
            assert_eq!(view.last().unwrap().text, "short");
        }
        let view = log.view(30, 50);
        assert!(!view[0].text.starts_with(' '));
        assert!(view[1].text.starts_with("  "), "continuation lines are indented");
    }

    #[test]
    fn a_cut_view_keeps_the_end_of_the_oldest_message() {
        let log = log_of(&[(Kind::Game, "alpha beta gamma delta epsilon zeta"), (Kind::Game, "last")]);
        let full = log.view(14, 50);
        assert!(full.len() > 3);
        let cut = log.view(14, 3);
        assert_eq!(texts(&cut), texts(&full[full.len() - 3..]));
    }

    #[test]
    fn scrolling_back_hides_newer_messages_and_stops_at_the_ends() {
        let mut log = log_of(&[(Kind::Game, "a"), (Kind::Game, "b"), (Kind::Game, "c"), (Kind::Game, "d")]);
        assert!(!log.is_scrolled_back());
        log.scroll_back(2);
        assert!(log.is_scrolled_back());
        assert_eq!(log.newer_hidden(), 2);
        assert_eq!(texts(&log.view(20, 5)), ["a", "b"]);
        log.scroll_back(100);
        assert_eq!(texts(&log.view(20, 5)), ["a"], "the oldest message always stays in view");
        log.scroll_forward(1);
        assert_eq!(texts(&log.view(20, 5)), ["a", "b"]);
        log.scroll_forward(100);
        assert!(!log.is_scrolled_back());
        assert_eq!(texts(&log.view(20, 5)), ["a", "b", "c", "d"]);
        ChatLog::new().scroll_back(5); // an empty log has nothing to scroll
    }

    #[test]
    fn a_new_message_returns_to_the_newest() {
        let mut log = log_of(&[(Kind::Game, "a"), (Kind::Game, "b"), (Kind::Game, "c")]);
        log.scroll_back(2);
        log.push(Kind::Game, "d");
        assert!(!log.is_scrolled_back());
        assert_eq!(log.view(20, 1)[0].text, "d");
    }

    #[test]
    fn plain_text_is_speech_and_blank_text_is_nothing() {
        assert_eq!(parse("hello there"), Some(Submission::Say("hello there".into())));
        assert_eq!(parse("  hi  "), Some(Submission::Say("hi".into())));
        assert_eq!(parse(""), None);
        assert_eq!(parse("    "), None);
        assert_eq!(parse("what about 1/2?"), Some(Submission::Say("what about 1/2?".into())), "a slash inside speech is speech");
    }

    #[test]
    fn edit_is_a_command_in_any_case_and_takes_no_words() {
        assert_eq!(parse("/edit"), Some(Submission::Edit));
        assert_eq!(parse("  /EDIT "), Some(Submission::Edit));
        assert!(matches!(parse("/edit now"), Some(Submission::Error(m)) if m.contains("no extra words")));
        let mut log = ChatLog::new();
        assert_eq!(handle(&mut log, Submission::Edit, Ok(())), None);
        assert_eq!(log.len(), 0);
        assert!(help_text().contains("/edit"));
    }

    #[test]
    fn every_emote_command_is_parsed_in_any_case() {
        for emote in Emote::ALL {
            let word = emote.command();
            assert_eq!(parse(&format!("/{word}")), Some(Submission::Emote(emote)));
            assert_eq!(parse(&format!("  /{}  ", word.to_uppercase())), Some(Submission::Emote(emote)));
        }
        assert_eq!(parse("/Help"), Some(Submission::Help));
    }

    #[test]
    fn bad_commands_give_a_clear_error_and_nothing_else() {
        let Some(Submission::Error(message)) = parse("/jump") else { panic!("unknown command") };
        assert!(message.contains("/jump") && message.contains("/help"), "{message}");
        let Some(Submission::Error(message)) = parse("/WAVE hello") else { panic!("extra words") };
        assert!(message.contains("/wave") && message.contains("no extra words"), "{message}");
        let Some(Submission::Error(message)) = parse("/help me") else { panic!("help with words") };
        assert!(message.contains("/help"), "{message}");
        let Some(Submission::Error(message)) = parse("/") else { panic!("lone slash") };
        assert!(message.contains("slash"), "{message}");
        assert!(matches!(parse("/   "), Some(Submission::Error(_))));
        assert!(matches!(parse("//wave"), Some(Submission::Error(_))), "a doubled slash is not a command");
    }

    #[test]
    fn help_lists_every_command() {
        let help = help_text();
        for emote in Emote::ALL {
            assert!(help.contains(&format!("/{}", emote.command())), "{help}");
        }
        assert!(help.contains("/help"));
    }

    #[test]
    fn handling_speech_help_and_errors_only_writes_to_the_log() {
        let mut log = ChatLog::new();
        assert_eq!(handle(&mut log, Submission::Say("hi".into()), Ok(())), None);
        assert_eq!(handle(&mut log, Submission::Help, Ok(())), None);
        assert_eq!(handle(&mut log, Submission::Error("nope".into()), Ok(())), None);
        let kinds: Vec<Kind> = log.messages().map(|m| m.kind).collect();
        assert_eq!(kinds, [Kind::Player, Kind::System, Kind::System]);
    }

    #[test]
    fn an_allowed_emote_is_logged_and_started() {
        let mut log = ChatLog::new();
        assert_eq!(handle(&mut log, Submission::Emote(Emote::Dance), Ok(())), Some(Emote::Dance));
        let message = log.messages().next().unwrap();
        assert_eq!((message.kind, message.text.as_str()), (Kind::Emote, "You dance."));
    }

    #[test]
    fn a_refused_emote_says_why_and_does_not_start() {
        let mut log = ChatLog::new();
        assert_eq!(handle(&mut log, Submission::Emote(Emote::Wave), Err("You can't do that while you are walking.")), None);
        let message = log.messages().next().unwrap();
        assert_eq!(message.kind, Kind::System);
        assert!(message.text.contains("walking"));
        assert_eq!(log.len(), 1, "no 'You wave.' line for an emote that did not happen");
    }

    #[test]
    fn a_closed_box_takes_nothing() {
        let mut input = ChatInput::new();
        assert!(!input.is_open());
        assert!(!input.type_char('a'));
        assert!(!input.backspace());
        assert_eq!(input.submit(), None);
        assert_eq!(input.text(), "");
    }

    #[test]
    fn typing_backspace_and_submit_work() {
        let mut input = ChatInput::new();
        input.open();
        assert!(input.is_open());
        for c in "hellp".chars() {
            assert!(input.type_char(c));
        }
        assert!(input.backspace());
        assert!(input.type_char('o'));
        assert_eq!(input.text(), "hello");
        assert_eq!(input.submit(), Some(Submission::Say("hello".into())));
        assert!(!input.is_open(), "sending closes the box");
        assert_eq!(input.text(), "");
    }

    #[test]
    fn a_slash_prefill_starts_a_command() {
        let mut input = ChatInput::new();
        input.open_with("/");
        assert_eq!(input.text(), "/");
        for c in "wave".chars() {
            input.type_char(c);
        }
        assert_eq!(input.submit(), Some(Submission::Emote(Emote::Wave)));
    }

    #[test]
    fn control_characters_are_refused_and_spaces_are_kept() {
        let mut input = ChatInput::new();
        input.open();
        for c in ['\n', '\r', '\t', '\u{1b}', '\u{7f}', '\0'] {
            assert!(!input.type_char(c), "{c:?}");
        }
        assert!(input.type_char(' '));
        assert!(input.type_char('é'));
        assert!(input.type_char('日'));
        assert_eq!(input.text(), " é日");
    }

    #[test]
    fn the_box_is_limited_and_never_loses_what_fits() {
        let mut input = ChatInput::new();
        input.open();
        for _ in 0..MAX_INPUT_CHARS {
            assert!(input.type_char('x'));
        }
        assert!(!input.type_char('y'), "full");
        assert_eq!(input.text().chars().count(), MAX_INPUT_CHARS);
        assert!(input.backspace());
        assert!(input.type_char('y'));
        let mut long = ChatInput::new();
        long.open_with(&"z".repeat(500));
        assert_eq!(long.text().chars().count(), MAX_INPUT_CHARS);
    }

    #[test]
    fn closing_and_reopening_starts_clean_and_blank_input_just_closes() {
        let mut input = ChatInput::new();
        input.open();
        input.type_char('a');
        input.close();
        assert!(!input.is_open());
        input.open();
        assert_eq!(input.text(), "", "an abandoned line is not kept");
        for c in "   ".chars() {
            input.type_char(c);
        }
        assert_eq!(input.submit(), None);
        assert!(!input.is_open());
    }

    #[test]
    fn the_display_line_always_fits_and_shows_the_end_of_long_text() {
        let mut input = ChatInput::new();
        input.open();
        assert_eq!(input.display(20), "> _");
        for c in "hello".chars() {
            input.type_char(c);
        }
        assert_eq!(input.display(20), "> hello_");
        let mut long = ChatInput::new();
        long.open_with("0123456789012345678901234567890123456789");
        for width in [6, 7, 10, 20, 45, 200] {
            let line = long.display(width);
            assert!(line.chars().count() <= width.max(6), "width {width}: {line:?}");
            assert!(line.starts_with("> "));
            assert!(line.ends_with('_'));
            assert!(line.contains("789"), "width {width}: the end stays visible: {line:?}");
        }
        assert!(long.display(20).starts_with("> ..."), "{}", long.display(20));
    }
}
