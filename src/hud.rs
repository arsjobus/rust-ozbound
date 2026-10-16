//! Heads-up display: quest objective, bag contents, journal line, and the hover label.

use bevy::prelude::*;
use ozbound::inventory::ItemId;

use crate::components::*;

type HoverLabelQuery<'w, 's> =
    Query<'w, 's, (&'static mut Text, &'static mut BackgroundColor), (With<HoverHud>, Without<HudText>)>;

pub fn update_hud(
    progress: Res<GameProgress>,
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

    let objective = if progress.quest.is_complete() {
        "Objective: Return to the guide"
    } else if progress.spoke_to_guide {
        "Objective: Gather 3 moonpetals"
    } else {
        "Objective: Talk to the village guide"
    };
    let journal = if progress.journal.is_empty() { "Journal: A quiet meadow awaits." } else { &progress.journal };

    text.0 = format!(
        "OZBOUND - A FAIRYTALE RPG\nLeft-click: walk / interact   Middle-drag or arrows: camera   Scroll: zoom   Esc: quit\n{}\n{}\nBag: {} moonpetals | {} timber | {} copper ore\n{}",
        objective,
        progress.quest.progress_text(),
        progress.inventory.count(ItemId::Moonpetal),
        progress.inventory.count(ItemId::Timber),
        progress.inventory.count(ItemId::CopperOre),
        journal,
    );
}
