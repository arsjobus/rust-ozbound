//! Heads-up display: the quest tracker and hover label (top-left), the toolbar (top-right), and
//! the side panels it opens: bag (B), skills (K), and quest book (J), one at a time.
//!
//! Every block of text is wrapped to the room it has (`ozbound::text_fit`), so nothing runs off
//! the edge of the window at any supported size, and each side panel is split into pages
//! (PageUp and PageDown turn them) so a long list is never cut off. Messages are not shown
//! here any more: they go to the chat area (`chat_ui.rs`).

use bevy::prelude::*;
use bevy::window::PrimaryWindow;
use ozbound::{
    journal::{tracker, Journal},
    side_panel::{bag_text, skills_text, Tab},
    text_fit::{chars_per_line, lines_per_page, logical_size, paginate, toolbar_width, wrap_text, DOCK_PANEL, HUD_PX, JOURNAL_PX, MARGIN},
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
    dock: Res<DockState>,
    ui_scale: Res<UiScale>,
    windows: Query<&Window, With<PrimaryWindow>>,
    mut hud: Query<&mut Text, (With<HudText>, Without<HoverHud>)>,
    mut hover_label: HoverLabelQuery,
) {
    let (logical_width, _) = logical_window(&windows, &ui_scale);
    // The top-right corner holds the toolbar, or the open side panel below it, so the text on
    // the left keeps clear of whichever is wider.
    let labels: Vec<String> = Tab::ALL.iter().map(|tab| tab.button_text()).collect();
    let right = if dock.dock.is_open() { DOCK_PANEL.width } else { toolbar_width(&labels) };
    let room = logical_width - right - 3.0 * MARGIN;
    if let Ok((mut label, mut backing)) = hover_label.get_single_mut() {
        // The label has 10px of padding each side.
        label.0 = wrap_text(&hover.0, chars_per_line(room - 20.0, 22.0));
        // Only draw the backing panel while there is something to say.
        backing.0 = if hover.0.is_empty() { Color::NONE } else { Color::srgba(0.0, 0.0, 0.0, 0.55) };
    }
    let Ok(mut text) = hud.get_single_mut() else { return };
    // The tracker has 8px of padding each side.
    text.0 = wrap_text(&tracker(&content.0.quests, &progress.log), chars_per_line(room - 16.0, HUD_PX));
}

/// B, K and J open or close the bag, skills, and quest book; PageUp and PageDown turn the
/// pages of whichever is open.
pub fn dock_input(keys: Res<ButtonInput<KeyCode>>, chat: Res<ChatState>, mut dock: ResMut<DockState>) {
    if chat.keys_captured {
        return;
    }
    for (key, tab) in [(KeyCode::KeyB, Tab::Bag), (KeyCode::KeyK, Tab::Skills), (KeyCode::KeyJ, Tab::Quests)] {
        if keys.just_pressed(key) {
            dock.toggle(tab);
            return;
        }
    }
    if dock.dock.is_open() {
        if keys.just_pressed(KeyCode::PageDown) {
            dock.page += 1;
        }
        if keys.just_pressed(KeyCode::PageUp) {
            dock.page = dock.page.saturating_sub(1);
        }
    }
}

/// Clicking a toolbar button opens or closes its side panel.
pub fn toolbar_input(buttons: Query<(&Interaction, &ToolbarButton), Changed<Interaction>>, mut dock: ResMut<DockState>) {
    for (interaction, button) in &buttons {
        if *interaction == Interaction::Pressed {
            dock.toggle(button.0);
        }
    }
}

/// Lights the button of the open panel and the one under the cursor.
pub fn sync_toolbar(dock: Res<DockState>, mut buttons: Query<(&Interaction, &ToolbarButton, &mut BackgroundColor)>) {
    for (interaction, button, mut background) in &mut buttons {
        let open = dock.dock.current() == Some(button.0);
        let color = match (open, *interaction) {
            (true, _) => Color::srgba(0.25, 0.32, 0.5, 0.92),
            (false, Interaction::Hovered | Interaction::Pressed) => Color::srgba(0.18, 0.18, 0.26, 0.9),
            (false, Interaction::None) => Color::srgba(0.05, 0.05, 0.08, 0.78),
        };
        if background.0 != color {
            background.0 = color;
        }
    }
}

/// Shows the current page of the open side panel; the text follows the game state.
pub fn update_dock_panel(
    mut dock: ResMut<DockState>,
    progress: Res<GameProgress>,
    content: Res<GameContent>,
    wildlife: Res<Wildlife>,
    ui_scale: Res<UiScale>,
    windows: Query<&Window, With<PrimaryWindow>>,
    mut panel: Query<(&mut Text, &mut Visibility), With<DockPanel>>,
) {
    let Ok((mut text, mut visibility)) = panel.get_single_mut() else { return };
    let Some(tab) = dock.dock.current() else {
        if *visibility != Visibility::Hidden {
            *visibility = Visibility::Hidden;
        }
        return;
    };
    if *visibility == Visibility::Hidden {
        *visibility = Visibility::Inherited;
    }
    let body = match tab {
        Tab::Bag => bag_text(&progress.inventory, &progress.equipment),
        Tab::Skills => skills_text(&progress.skills),
        Tab::Quests => {
            let notes = wildlife.notes.render(&wildlife.bestiary);
            let notes = if notes.is_empty() { notes } else { format!("\n\n{notes}") };
            format!("{}{notes}", Journal::build(&content.0.quests, &progress.log).render())
        }
    };

    let (_, logical_height) = logical_window(&windows, &ui_scale);
    // The title, a blank line, a blank line, and the page footer take four lines of the panel.
    let per_page = lines_per_page(DOCK_PANEL.inner_height(logical_height), JOURNAL_PX).saturating_sub(4).max(1);
    let pages = paginate(&body, chars_per_line(DOCK_PANEL.inner_width(), JOURNAL_PX), per_page);
    let page = dock.page.min(pages.len() - 1);
    if page != dock.page {
        dock.page = page;
    }
    let footer = if pages.len() > 1 { format!("Page {}/{}  PageUp/PageDown to turn", page + 1, pages.len()) } else { String::new() };
    text.0 = format!("{} ({} to close)\n\n{}\n\n{footer}", tab.title(), tab.hotkey(), pages[page]);
}
