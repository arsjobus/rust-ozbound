//! The crafting-and-gear panel (C): draws `ozbound::pack::rows` and turns a click or a number
//! key into `ozbound::pack::apply`. What can be made, equipped, or removed, and what the
//! player is told, all come from the tested pure module; this file only connects it to Bevy.
//!
//! While the panel is open it owns the mouse (`pointer` and `menu` stand aside), so a click
//! on a row never also walks. C or Esc closes it (see `exit_on_escape` in `main.rs`), and a
//! conversation closes it, so the player can never be trapped.

use bevy::prelude::*;
use ozbound::{
    journal::describe_update,
    pack::{self, PackAction, PackRow},
    quests::QuestEvent,
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

/// C opens or closes the panel; while it is open a row is chosen by click or number key (1-9).
pub fn pack_input(
    keys: Res<ButtonInput<KeyCode>>,
    buttons: Query<(&Interaction, &PackRowButton), Changed<Interaction>>,
    recipes: Res<Recipes>,
    content: Res<GameContent>,
    talk: Res<TalkState>,
    mut progress: ResMut<GameProgress>,
    mut state: ResMut<PackState>,
) {
    // A conversation takes over the keyboard and mouse; the panel steps aside.
    if talk.0.is_some() {
        if state.0 {
            state.0 = false;
        }
        return;
    }
    if keys.just_pressed(KeyCode::KeyC) {
        state.0 = !state.0;
        return;
    }
    if !state.0 {
        return;
    }
    let from_key = DIGITS.iter().position(|key| keys.just_pressed(*key));
    let from_click = buttons.iter().find(|(interaction, _)| **interaction == Interaction::Pressed).map(|(_, row)| row.0);
    let Some(index) = from_key.or(from_click) else { return };

    let rows = pack::rows(&recipes.0, &progress.inventory, &progress.equipment, &progress.skills);
    let Some(row) = rows.get(index) else { return };
    let progress = &mut *progress;
    // A craft is reported to the quest log by what actually arrived in the bag.
    let crafted = match &row.action {
        PackAction::Craft(id) => recipes.0.get(id).map(|recipe| (recipe.output.item, progress.inventory.count(recipe.output.item))),
        _ => None,
    };
    let mut message = pack::apply(&row.action, &recipes.0, &mut progress.inventory, &mut progress.equipment, &mut progress.skills);
    if let Some((item, before)) = crafted {
        let made = progress.inventory.count(item).saturating_sub(before);
        if made > 0 {
            let updates = progress.log.record(&content.0.quests, &QuestEvent::Crafted { item, quantity: made });
            for update in &updates {
                message.push(' ');
                message.push_str(&describe_update(&content.0.quests, update));
            }
        }
    }
    progress.journal = message;
}

/// Shows or hides the panel, rebuilds its rows when the bag, gear, or open state changes, and
/// highlights the row under the cursor.
pub fn sync_pack_ui(
    mut commands: Commands,
    state: Res<PackState>,
    progress: Res<GameProgress>,
    recipes: Res<Recipes>,
    mut panels: Query<(Entity, &mut Visibility), With<PackPanel>>,
    mut rows: Query<(&Interaction, &mut BackgroundColor), With<PackRowButton>>,
) {
    let Ok((panel, mut visibility)) = panels.get_single_mut() else { return };

    if state.is_changed() || (state.0 && progress.is_changed()) {
        commands.entity(panel).despawn_descendants();
        if state.0 {
            *visibility = Visibility::Inherited;
            let listed: Vec<PackRow> = pack::rows(&recipes.0, &progress.inventory, &progress.equipment, &progress.skills);
            commands.entity(panel).with_children(|panel| {
                panel.spawn((
                    Text::new("CRAFTING & GEAR (C or Esc to close)"),
                    TextFont { font_size: 21.0, ..default() },
                    TextColor(Color::srgb(1.0, 0.92, 0.5)),
                ));
                panel.spawn((
                    Text::new(format!(
                        "{}\n{}\n{}",
                        pack::bag_line(&progress.inventory),
                        pack::equipment_line(&progress.equipment),
                        progress.skills.summary_line()
                    )),
                    TextFont { font_size: 17.0, ..default() },
                    TextColor(Color::srgb(0.8, 0.85, 0.9)),
                    Node { margin: UiRect::bottom(Val::Px(8.0)), ..default() },
                ));
                for (index, row) in listed.iter().enumerate() {
                    let number = if index < DIGITS.len() { format!("{}. ", index + 1) } else { "   ".to_string() };
                    let colour = if row.enabled { Color::srgb(0.85, 0.95, 1.0) } else { Color::srgb(0.55, 0.58, 0.62) };
                    panel
                        .spawn((
                            Button,
                            Node { padding: UiRect::axes(Val::Px(10.0), Val::Px(4.0)), ..default() },
                            BackgroundColor(ROW_IDLE),
                            PackRowButton(index),
                        ))
                        .with_children(|button| {
                            button.spawn((
                                Text::new(format!("{number}{}", row.text)),
                                TextFont { font_size: 18.0, ..default() },
                                TextColor(colour),
                            ));
                        });
                }
            });
        } else {
            *visibility = Visibility::Hidden;
        }
    }

    for (interaction, mut background) in &mut rows {
        background.0 = if *interaction == Interaction::None { ROW_IDLE } else { ROW_HOVER };
    }
}
