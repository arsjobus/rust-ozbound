//! Heads-up display: quest tracker, bag contents, the latest message, the hover label, and
//! the journal panel (J) that lists active and completed quests, a page at a time.
//!
//! Every block of text is wrapped to the room it has (`ozbound::text_fit`), so nothing runs off
//! the edge of the window at any supported size, and the journal is split into pages (PageUp and
//! PageDown turn them) so a long journal is never cut off.

use bevy::prelude::*;
use bevy::window::PrimaryWindow;
use ozbound::{
    journal::{tracker, Journal},
    pack::{bag_line, equipment_line},
    text_fit::{chars_per_line, lines_per_page, logical_size, paginate, wrap_text, HUD_PX, JOURNAL_PANEL, JOURNAL_PX, MARGIN},
};

use crate::components::*;

type HoverLabelQuery<'w, 's> =
    Query<'w, 's, (&'static mut Text, &'static mut BackgroundColor), (With<HoverHud>, Without<HudText>)>;

/// The interface size in logical pixels, or a sensible default before the window exists.
fn logical_window(windows: &Query<&Window, With<PrimaryWindow>>, ui_scale: &UiScale) -> (f32, f32) {
    windows.get_single().map_or((1100.0, 720.0), |window| logical_size(window.width(), window.height(), ui_scale.0))
}

pub fn update_hud(
    progress: Res<GameProgress>,
    content: Res<GameContent>,
    hover: Res<HoverText>,
    journal: Res<JournalState>,
    ui_scale: Res<UiScale>,
    windows: Query<&Window, With<PrimaryWindow>>,
    mut hud: Query<&mut Text, (With<HudText>, Without<HoverHud>)>,
    mut hover_label: HoverLabelQuery,
) {
    let (logical_width, _) = logical_window(&windows, &ui_scale);
    if let Ok((mut label, mut backing)) = hover_label.get_single_mut() {
        // The label has 10px of padding each side and sits 10px from the edge.
        label.0 = wrap_text(&hover.0, chars_per_line(logical_width - 2.0 * MARGIN - 20.0, 22.0));
        // Only draw the backing panel while there is something to say.
        backing.0 = if hover.0.is_empty() { Color::NONE } else { Color::srgba(0.0, 0.0, 0.0, 0.55) };
    }
    let Ok(mut text) = hud.get_single_mut() else { return };

    let message = if progress.journal.is_empty() { "A quiet meadow awaits." } else { &progress.journal };

    // The block has 8px of padding each side; keep clear of the journal panel when it is open.
    let reserved = if journal.open { JOURNAL_PANEL.width + 3.0 * MARGIN } else { 2.0 * MARGIN };
    let width = chars_per_line(logical_width - reserved - 16.0, HUD_PX);
    let full = format!(
        "OZBOUND - A FAIRYTALE RPG\nEsc: menu   Tab: choose   Enter: go   J: journal   C: craft & gear\n{}\n{}\n{}\n{}\n{}",
        tracker(&content.0.quests, &progress.log),
        bag_line(&progress.inventory),
        equipment_line(&progress.equipment),
        progress.skills.summary_line(),
        message,
    );
    text.0 = wrap_text(&full, width);
}

/// J opens or closes the journal; PageUp and PageDown turn its pages while it is open.
pub fn journal_input(keys: Res<ButtonInput<KeyCode>>, mut journal: ResMut<JournalState>) {
    if keys.just_pressed(KeyCode::KeyJ) {
        journal.open = !journal.open;
        journal.page = 0;
    } else if journal.open {
        if keys.just_pressed(KeyCode::PageDown) {
            journal.page += 1;
        }
        if keys.just_pressed(KeyCode::PageUp) {
            journal.page = journal.page.saturating_sub(1);
        }
    }
}

/// Shows the current page of the journal while it is open; the text follows the quest log.
pub fn update_journal_panel(
    mut journal: ResMut<JournalState>,
    progress: Res<GameProgress>,
    content: Res<GameContent>,
    wildlife: Res<Wildlife>,
    ui_scale: Res<UiScale>,
    windows: Query<&Window, With<PrimaryWindow>>,
    mut panel: Query<(&mut Text, &mut Visibility), With<JournalPanel>>,
) {
    let Ok((mut text, mut visibility)) = panel.get_single_mut() else { return };
    if !journal.open {
        if *visibility != Visibility::Hidden {
            *visibility = Visibility::Hidden;
        }
        return;
    }
    if *visibility == Visibility::Hidden {
        *visibility = Visibility::Inherited;
    }
    let notes = wildlife.notes.render(&wildlife.bestiary);
    let notes = if notes.is_empty() { notes } else { format!("\n\n{notes}") };
    let body = format!("{}{notes}", Journal::build(&content.0.quests, &progress.log).render());

    let (_, logical_height) = logical_window(&windows, &ui_scale);
    // The title, a blank line, a blank line, and the page footer take four lines of the panel.
    let per_page = lines_per_page(JOURNAL_PANEL.inner_height(logical_height), JOURNAL_PX).saturating_sub(4).max(1);
    let pages = paginate(&body, chars_per_line(JOURNAL_PANEL.inner_width(), JOURNAL_PX), per_page);
    let page = journal.page.min(pages.len() - 1);
    if page != journal.page {
        journal.page = page;
    }
    let footer = if pages.len() > 1 { format!("Page {}/{}  PageUp/PageDown to turn", page + 1, pages.len()) } else { String::new() };
    text.0 = format!("JOURNAL (J to close)\n\n{}\n\n{footer}", pages[page]);
}
