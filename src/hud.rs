//! Heads-up display: quest tracker, bag contents, the latest message, the hover label, and
//! the journal panel (J) that lists active and completed quests.

use bevy::prelude::*;
use ozbound::{
    journal::{tracker, Journal},
    pack::{bag_line, equipment_line},
};

use crate::components::*;

type HoverLabelQuery<'w, 's> =
    Query<'w, 's, (&'static mut Text, &'static mut BackgroundColor), (With<HoverHud>, Without<HudText>)>;

pub fn update_hud(
    progress: Res<GameProgress>,
    content: Res<GameContent>,
    hover: Res<HoverText>,
    mut hud: Query<&mut Text, (With<HudText>, Without<HoverHud>)>,
    mut hover_label: HoverLabelQuery,
) {
    if let Ok((mut label, mut backing)) = hover_label.get_single_mut() {
        label.0 = hover.0.clone();
        // Only draw the backing panel while there is something to say.
        backing.0 = if hover.0.is_empty() { Color::NONE } else { Color::srgba(0.0, 0.0, 0.0, 0.55) };
    }
    let Ok(mut text) = hud.get_single_mut() else { return };

    let journal = if progress.journal.is_empty() { "A quiet meadow awaits." } else { &progress.journal };

    text.0 = format!(
        "OZBOUND - A FAIRYTALE RPG\nLeft-click: walk / interact   Right-click: menu   J: journal   C: craft & gear   Middle-drag or arrows: camera   Scroll: zoom   Esc: close panel / quit\n{}\n{}\n{}\n{}",
        tracker(&content.0.quests, &progress.log),
        bag_line(&progress.inventory),
        equipment_line(&progress.equipment),
        journal,
    );
}

/// J shows or hides the journal; while it is shown the text follows the quest log.
pub fn update_journal_panel(
    keys: Res<ButtonInput<KeyCode>>,
    progress: Res<GameProgress>,
    content: Res<GameContent>,
    mut panel: Query<(&mut Text, &mut Visibility), With<JournalPanel>>,
) {
    let Ok((mut text, mut visibility)) = panel.get_single_mut() else { return };
    if keys.just_pressed(KeyCode::KeyJ) {
        *visibility = if *visibility == Visibility::Hidden { Visibility::Inherited } else { Visibility::Hidden };
    }
    if *visibility != Visibility::Hidden {
        text.0 = format!("JOURNAL (J to close)\n\n{}", Journal::build(&content.0.quests, &progress.log).render());
    }
}
