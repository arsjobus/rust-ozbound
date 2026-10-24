//! The chat area: a small log in the bottom-left corner that receives every game message, what
//! the player examines, what the player says, and emotes, plus a text-entry line.
//!
//! The rules live in `ozbound::chat` (log, input box, commands) and `ozbound::emote`; this file
//! reads the keyboard, starts emotes, and draws the panel.
//!
//! - **T** or **/** opens the box (`/` starts it with a slash). **Enter** sends, **Esc**
//!   cancels (decided in `pause_input` through `pause::on_escape`), **Backspace** edits.
//! - While the box is open every other key handler stands aside (`ChatState::keys_captured`),
//!   so typing `wasd` or `1` does not walk or pick a menu row.
//! - **PageUp** / **PageDown** scroll the log, unless a side panel is open, in which case they
//!   turn that panel's pages (see `hud::dock_input`).
//! - Systems never write to the chat directly for ordinary messages: they set
//!   `GameProgress::notice` and [`collect_notices`] moves it into the log.

use bevy::input::keyboard::{Key, KeyboardInput};
use bevy::input::ButtonState;
use bevy::prelude::*;
use bevy::ui::FocusPolicy;
use ozbound::{
    chat::{self, Kind, SCROLL_STEP},
    emote::{self, ActiveEmote},
    text_fit::{chars_per_line, CHAT_HEIGHT, CHAT_LINES, CHAT_PANEL, CHAT_PX, LINE_HEIGHT_EM},
};

use crate::components::*;

/// Holds the log lines (rebuilt whenever the chat changes).
#[derive(Component)]
pub struct ChatLogBox;

/// The single line under the log: the text being typed, or a hint.
#[derive(Component)]
pub struct ChatInputLine;

/// Builds the chat panel once at startup.
pub fn spawn_chat_panel(mut commands: Commands) {
    commands
        .spawn((
            Node {
                position_type: PositionType::Absolute,
                bottom: Val::Px(CHAT_PANEL.offset),
                left: Val::Px(ozbound::text_fit::MARGIN),
                width: Val::Px(CHAT_PANEL.width),
                height: Val::Px(CHAT_HEIGHT),
                padding: UiRect::all(Val::Px(CHAT_PANEL.padding)),
                flex_direction: FlexDirection::Column,
                justify_content: JustifyContent::SpaceBetween,
                ..default()
            },
            BackgroundColor(Color::srgba(0.03, 0.03, 0.06, 0.7)),
            Interaction::default(),
            FocusPolicy::Block,
            UiBlocker,
            ChatPanel,
        ))
        .with_children(|panel| {
            panel.spawn((
                Node {
                    height: Val::Px(CHAT_LINES as f32 * CHAT_PX * LINE_HEIGHT_EM),
                    flex_direction: FlexDirection::Column,
                    justify_content: JustifyContent::FlexEnd,
                    overflow: Overflow::clip(),
                    ..default()
                },
                ChatLogBox,
            ));
            panel.spawn((Text::new(""), TextFont { font_size: CHAT_PX, ..default() }, TextColor(Color::WHITE), ChatInputLine));
        });
}

/// Moves the pending game message into the chat log.
pub fn collect_notices(mut progress: ResMut<GameProgress>, mut chat: ResMut<ChatState>) {
    if progress.notice.is_empty() {
        return;
    }
    let notice = std::mem::take(&mut progress.notice);
    chat.log.push(Kind::Game, notice);
}

/// Reads typed characters while the box is open, opens it on T or /, sends on Enter, and
/// scrolls the log with PageUp / PageDown.
pub fn chat_input(
    mut events: EventReader<KeyboardInput>,
    keys: Res<ButtonInput<KeyCode>>,
    dock: Res<DockState>,
    mut chat: ResMut<ChatState>,
    mut players: Query<&mut Player>,
) {
    let was_open = chat.input.is_open();
    for event in events.read() {
        if event.state != ButtonState::Pressed {
            continue;
        }
        if !chat.input.is_open() {
            if event.repeat {
                continue;
            }
            match &event.logical_key {
                Key::Character(text) if text.as_str().eq_ignore_ascii_case("t") => chat.input.open(),
                Key::Character(text) if text.as_str() == "/" => chat.input.open_with("/"),
                _ => {}
            }
            continue;
        }
        match &event.logical_key {
            Key::Enter if !event.repeat => send(&mut chat, &mut players),
            Key::Backspace => {
                chat.input.backspace();
            }
            Key::Space => {
                chat.input.type_char(' ');
            }
            Key::Character(text) => {
                for c in text.chars() {
                    chat.input.type_char(c);
                }
            }
            _ => {}
        }
    }

    if !dock.dock.is_open() {
        if keys.just_pressed(KeyCode::PageUp) {
            chat.log.scroll_back(SCROLL_STEP);
        }
        if keys.just_pressed(KeyCode::PageDown) {
            chat.log.scroll_forward(SCROLL_STEP);
        }
    }

    // Other key handlers stand aside when the box was open at the start of the frame (so the
    // Enter that sent a line is not also "go to the selected thing") or is open now (so keys
    // typed in the same frame as the T that opened it are not also moves).
    let captured = was_open || chat.input.is_open();
    if chat.keys_captured != captured {
        chat.keys_captured = captured;
    }
}

/// Sends what was typed: speech goes to the log, a command runs, and an emote starts playing
/// if the player is free to do it.
fn send(chat: &mut ChatState, players: &mut Query<&mut Player>) {
    let Some(submission) = chat.input.submit() else { return };
    if submission == chat::Submission::Edit {
        chat.edit_requested = true;
    }
    let allowed = match players.get_single() {
        Ok(player) => emote::can_start(player.task.is_moving(), player.work.is_some()),
        Err(_) => Ok(()),
    };
    if let Some(emote) = chat::handle(&mut chat.log, submission, allowed) {
        if let Ok(mut player) = players.get_single_mut() {
            player.emote = Some(ActiveEmote::start(emote));
        }
    }
}

fn kind_color(kind: Kind) -> Color {
    match kind {
        Kind::Game => Color::srgb(0.95, 0.95, 0.95),
        Kind::Examine => Color::srgb(0.65, 0.85, 1.0),
        Kind::Player => Color::srgb(1.0, 0.93, 0.55),
        Kind::Emote => Color::srgb(0.75, 0.95, 0.7),
        Kind::System => Color::srgb(1.0, 0.7, 0.5),
    }
}

/// Redraws the log and the input line whenever the chat changes.
pub fn sync_chat_ui(
    mut commands: Commands,
    chat: Res<ChatState>,
    log_box: Query<Entity, With<ChatLogBox>>,
    mut input_line: Query<(&mut Text, &mut TextColor), With<ChatInputLine>>,
) {
    if !chat.is_changed() {
        return;
    }
    let width = chars_per_line(CHAT_PANEL.inner_width(), CHAT_PX);
    if let Ok(log_box) = log_box.get_single() {
        commands.entity(log_box).despawn_descendants().with_children(|lines| {
            for line in chat.log.view(width, CHAT_LINES) {
                lines.spawn((Text::new(line.text), TextFont { font_size: CHAT_PX, ..default() }, TextColor(kind_color(line.kind))));
            }
        });
    }
    if let Ok((mut text, mut color)) = input_line.get_single_mut() {
        let (line, tint) = if chat.input.is_open() {
            (chat.input.display(width), Color::WHITE)
        } else if chat.log.is_scrolled_back() {
            ("(older messages - PageDown for newer)".to_string(), Color::srgb(0.8, 0.8, 0.5))
        } else {
            ("T or / to chat".to_string(), Color::srgb(0.6, 0.6, 0.65))
        };
        text.0 = line;
        color.0 = tint;
    }
}
