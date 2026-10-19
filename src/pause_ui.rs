//! The pause menu and Esc: which panel Esc closes, the menu's keyboard and mouse input, the
//! panel itself, and stopping the game clock while it is open. What each key does comes from the
//! tested `ozbound::pause`; this module only connects it to Bevy.
//!
//! While the menu is open the game clock is stopped (`Time<Virtual>`), every gameplay input
//! system stands aside (they are gated on [`running`] in `main.rs`), and the ambient sound keeps
//! playing so a volume change can be heard. Esc is handled here and nowhere else.

use bevy::prelude::*;
use ozbound::{
    pause::{on_escape, EscapeAction, Key, Open, Outcome, PauseMenu},
    text_fit::{chars_per_line, wrap_text, CONTROLS_PX, HINT_PX, MENU_PX, PAUSE_PANEL, ROW_PAD, ROW_PAD_X, SPEAKER_PX},
};

use crate::components::*;

const ROW_IDLE: Color = Color::srgba(0.0, 0.0, 0.0, 0.0);
const ROW_SELECTED: Color = Color::srgba(0.25, 0.30, 0.45, 0.95);
const ROW_HOVER: Color = Color::srgba(0.35, 0.30, 0.10, 0.9);

/// Run condition: true while the game is not paused.
pub fn running(pause: Res<Pause>) -> bool {
    pause.0.is_none()
}

/// Esc: with the pause menu open it steps back (and resumes from the first page); otherwise it
/// closes the most urgent open panel, and with nothing open it pauses. While the menu is open
/// the arrow keys, Enter or Space, and clicks on its rows drive it too.
pub fn pause_input(
    keys: Res<ButtonInput<KeyCode>>,
    rows: Query<(&Interaction, &PauseRow), Changed<Interaction>>,
    mut pause: ResMut<Pause>,
    mut settings: ResMut<UserSettings>,
    mut menu: ResMut<ContextMenuState>,
    mut talk: ResMut<TalkState>,
    mut pack: ResMut<PackState>,
    mut journal: ResMut<JournalState>,
    mut clock: ResMut<Time<Virtual>>,
    mut exit: EventWriter<AppExit>,
) {
    if pause.0.is_some() {
        let key = if keys.just_pressed(KeyCode::Escape) {
            Some(Key::Back)
        } else if keys.just_pressed(KeyCode::ArrowUp) {
            Some(Key::Up)
        } else if keys.just_pressed(KeyCode::ArrowDown) {
            Some(Key::Down)
        } else if keys.just_pressed(KeyCode::ArrowLeft) {
            Some(Key::Left)
        } else if keys.just_pressed(KeyCode::ArrowRight) {
            Some(Key::Right)
        } else if keys.any_just_pressed([KeyCode::Enter, KeyCode::NumpadEnter, KeyCode::Space]) {
            Some(Key::Confirm)
        } else {
            None
        };
        let clicked = rows.iter().find(|(interaction, _)| **interaction == Interaction::Pressed).map(|(_, row)| row.0);
        if key.is_none() && clicked.is_none() {
            return;
        }
        // Only now touch the resources mutably, so an idle menu is not rebuilt every frame.
        let Some(state) = pause.0.as_mut() else { return };
        let outcome = match (key, clicked) {
            (Some(key), _) => state.press(key, &mut settings.0),
            (None, Some(index)) => state.click(index, &mut settings.0),
            (None, None) => Outcome::Nothing,
        };
        match outcome {
            Outcome::Resume => {
                pause.0 = None;
                clock.unpause();
            }
            Outcome::Quit => {
                exit.send(AppExit::Success);
            }
            Outcome::Nothing | Outcome::SettingsChanged => {}
        }
        return;
    }

    if !keys.just_pressed(KeyCode::Escape) {
        return;
    }
    let open = Open { pause: false, context_menu: menu.0.is_some(), dialogue: talk.0.is_some(), crafting: pack.0, journal: journal.open };
    match on_escape(open) {
        EscapeAction::CloseContextMenu => menu.0 = None,
        EscapeAction::CloseDialogue => talk.0 = None,
        EscapeAction::CloseCrafting => pack.0 = false,
        EscapeAction::CloseJournal => journal.open = false,
        EscapeAction::OpenPause => {
            pause.0 = Some(PauseMenu::new());
            clock.pause();
        }
        // The menu is not open on this path, so there is nothing to step back from.
        EscapeAction::PauseBack => {}
    }
}

/// Shows or hides the panel, rebuilds its rows when the menu or a setting changes, and
/// highlights the row under the cursor.
pub fn sync_pause_ui(
    mut commands: Commands,
    pause: Res<Pause>,
    settings: Res<UserSettings>,
    mut panels: Query<(Entity, &mut Visibility), With<PausePanel>>,
    mut rows: Query<(&Interaction, &PauseRow, &mut BackgroundColor)>,
) {
    let Ok((panel, mut visibility)) = panels.get_single_mut() else { return };

    if pause.is_changed() || settings.is_changed() {
        commands.entity(panel).despawn_descendants();
        match &pause.0 {
            Some(menu) => {
                *visibility = Visibility::Inherited;
                let inner = PAUSE_PANEL.inner_width();
                let lines = menu.lines(&settings.0);
                commands.entity(panel).with_children(|panel| {
                    panel.spawn((Text::new(menu.page().title()), TextFont { font_size: SPEAKER_PX, ..default() }, TextColor(Color::srgb(1.0, 0.92, 0.5))));
                    if let Some(body) = menu.controls_text() {
                        panel.spawn((
                            Text::new(wrap_text(&body, chars_per_line(inner, CONTROLS_PX))),
                            TextFont { font_size: CONTROLS_PX, ..default() },
                            TextColor(Color::srgb(0.85, 0.9, 0.95)),
                        ));
                    }
                    for (index, line) in lines.iter().enumerate() {
                        let background = if line.selected { ROW_SELECTED } else { ROW_IDLE };
                        panel
                            .spawn((
                                Button,
                                Node { padding: UiRect::axes(Val::Px(ROW_PAD_X), Val::Px(ROW_PAD)), ..default() },
                                BackgroundColor(background),
                                PauseRow(index),
                            ))
                            .with_children(|row| {
                                let marker = if line.selected { "> " } else { "  " };
                                let width = chars_per_line(inner - 2.0 * ROW_PAD_X, MENU_PX);
                                row.spawn((
                                    Text::new(wrap_text(&format!("{marker}{}", line.text), width)),
                                    TextFont { font_size: MENU_PX, ..default() },
                                    TextColor(Color::WHITE),
                                ));
                            });
                    }
                    panel.spawn((
                        Text::new(wrap_text(menu.hint(), chars_per_line(inner, HINT_PX))),
                        TextFont { font_size: HINT_PX, ..default() },
                        TextColor(Color::srgb(0.65, 0.7, 0.78)),
                    ));
                });
            }
            None => *visibility = Visibility::Hidden,
        }
    }

    let selected = pause.0.as_ref().map(PauseMenu::selected);
    for (interaction, row, mut background) in &mut rows {
        background.0 = if *interaction != Interaction::None {
            ROW_HOVER
        } else if Some(row.0) == selected {
            ROW_SELECTED
        } else {
            ROW_IDLE
        };
    }
}
