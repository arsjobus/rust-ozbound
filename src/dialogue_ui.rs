//! The dialogue panel: draws the open conversation and turns a click or a number key into a
//! choice. What is said, which choices exist, and what each one changes all come from the
//! tested `ozbound::talk::Talk`; this module only connects it to Bevy.
//!
//! While a conversation is open it owns the mouse (`pointer` and `menu` stand aside), so a
//! click on a choice never also walks. Esc closes it (see `exit_on_escape` in `main.rs`) and
//! so does walking away, so the player can never be trapped.

use bevy::prelude::*;
use ozbound::text_fit::{
    chars_per_line, wrap_text, CHOICE_PX, DIALOGUE_PANEL, DIALOGUE_PX, PORTRAIT, PORTRAIT_GAP, ROW_PAD, ROW_PAD_X, SPEAKER_PX,
};

use crate::components::*;

const ROW_IDLE: Color = Color::srgba(0.0, 0.0, 0.0, 0.0);
const ROW_HOVER: Color = Color::srgba(0.35, 0.30, 0.10, 0.9);

const DIGITS: [KeyCode; 9] = [
    KeyCode::Digit1,
    KeyCode::Digit2,
    KeyCode::Digit3,
    KeyCode::Digit4,
    KeyCode::Digit5,
    KeyCode::Digit6,
    KeyCode::Digit7,
    KeyCode::Digit8,
    KeyCode::Digit9,
];

/// Picks a choice when a row is clicked or its number key (1-9) is pressed.
pub fn dialogue_input(
    keys: Res<ButtonInput<KeyCode>>,
    rows: Query<(&Interaction, &ChoiceRow), Changed<Interaction>>,
    content: Res<GameContent>,
    chat: Res<ChatState>,
    mut progress: ResMut<GameProgress>,
    mut players: Query<ProgressMut, With<Player>>,
    mut talk: ResMut<TalkState>,
) {
    let Some(active) = talk.0.as_mut() else { return };
    // Number keys type into the chat box while it is open instead of picking a row.
    let from_key = if chat.keys_captured { None } else { DIGITS.iter().position(|key| keys.just_pressed(*key)) };
    let from_click = rows.iter().find(|(interaction, _)| **interaction == Interaction::Pressed).map(|(_, row)| row.0);
    let Some(index) = from_key.or(from_click) else { return };
    if index >= active.view().choices.len() {
        return;
    }
    let Ok(mut mine) = players.get_single_mut() else { return };
    let (messages, open) = active.choose(&content.0, &mut mine.log.0, &mut mine.inventory.0, &mine.skills.0, index);
    if !messages.is_empty() {
        progress.notice = messages.join(" ");
    }
    if !open {
        talk.0 = None;
    }
}

/// Rebuilds the panel when the conversation changes and highlights the row under the cursor.
pub fn sync_dialogue_ui(
    mut commands: Commands,
    talk: Res<TalkState>,
    portrait: Res<TalkPortrait>,
    mut panels: Query<(Entity, &mut Visibility), With<DialoguePanel>>,
    mut rows: Query<(&Interaction, &mut BackgroundColor), With<ChoiceRow>>,
) {
    let Ok((panel, mut visibility)) = panels.get_single_mut() else { return };

    if talk.is_changed() {
        commands.entity(panel).despawn_descendants();
        match &talk.0 {
            Some(active) => {
                *visibility = Visibility::Inherited;
                let view = active.view();
                // The text is wrapped to the room it has, so the panel's height is what the tests measured.
                let words_width = chars_per_line(DIALOGUE_PANEL.inner_width() - PORTRAIT - PORTRAIT_GAP, DIALOGUE_PX);
                let choice_width = chars_per_line(DIALOGUE_PANEL.inner_width() - 2.0 * ROW_PAD_X, CHOICE_PX);
                commands.entity(panel).with_children(|panel| {
                    // Portrait on the left, speaker and words on the right.
                    panel.spawn(Node { flex_direction: FlexDirection::Row, column_gap: Val::Px(PORTRAIT_GAP), ..default() }).with_children(|row| {
                        if let Some(image) = portrait.0.clone() {
                            row.spawn((ImageNode::new(image), Node { width: Val::Px(PORTRAIT), height: Val::Px(PORTRAIT), flex_shrink: 0.0, ..default() }));
                        }
                        row.spawn(Node { flex_direction: FlexDirection::Column, row_gap: Val::Px(4.0), flex_shrink: 1.0, ..default() }).with_children(|words| {
                            words.spawn((
                                Text::new(view.speaker.clone()),
                                TextFont { font_size: SPEAKER_PX, ..default() },
                                TextColor(Color::srgb(1.0, 0.92, 0.5)),
                            ));
                            words.spawn((
                                Text::new(wrap_text(&view.text, words_width)),
                                TextFont { font_size: DIALOGUE_PX, ..default() },
                                TextColor(Color::WHITE),
                                Node { margin: UiRect::bottom(Val::Px(8.0)), ..default() },
                            ));
                        });
                    });
                    for (index, choice) in view.choices.iter().enumerate() {
                        panel
                            .spawn((
                                Button,
                                Node { padding: UiRect::axes(Val::Px(ROW_PAD_X), Val::Px(ROW_PAD)), ..default() },
                                BackgroundColor(ROW_IDLE),
                                ChoiceRow(index),
                            ))
                            .with_children(|row| {
                                row.spawn((
                                    Text::new(wrap_text(&format!("{}. {}", index + 1, choice), choice_width)),
                                    TextFont { font_size: CHOICE_PX, ..default() },
                                    TextColor(Color::srgb(0.85, 0.95, 1.0)),
                                ));
                            });
                    }
                });
            }
            None => *visibility = Visibility::Hidden,
        }
    }

    for (interaction, mut background) in &mut rows {
        background.0 = if *interaction == Interaction::None { ROW_IDLE } else { ROW_HOVER };
    }
}
