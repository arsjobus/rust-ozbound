//! Right-click context menu, as in classic MMORPGs: what entries to offer for whatever is
//! under the cursor, where the menu sits on screen, and which row a click lands on.
//! Pure Rust (screen positions are plain `(x, y)` pixel pairs, origin top-left).

use crate::interaction::{hover_text, InteractionKind};
use crate::text_fit::GLYPH_WIDTH_EM;

/// What choosing a menu row does.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MenuAction {
    /// The object's main action (chop down, mine, pick, talk to).
    Interact,
    /// Show a short description in the journal; the player does not move.
    Examine,
    /// Walk to the ground tile that was under the cursor.
    WalkHere,
    /// Close the menu and do nothing.
    Cancel,
    /// Editor: select the object under the cursor.
    Select,
    /// Editor: move the selected object to the ground tile under the cursor.
    MoveHere,
    /// Editor: remove the object under the cursor from the scene.
    Remove,
    /// Editor: place the chosen library asset on the ground tile under the cursor.
    PlaceHere,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MenuEntry {
    pub action: MenuAction,
    pub label: String,
}

impl MenuEntry {
    fn new(action: MenuAction, label: impl Into<String>) -> Self {
        Self { action, label: label.into() }
    }
}

/// Builds the rows for a right-click. `object` is the thing under the cursor, if any;
/// `ground_walkable` says whether the ground tile under the cursor can be walked to.
/// Order matches the classic layout: main action, Examine, Walk here, Cancel. Cancel is
/// always present and always last, so the menu can never be empty.
pub fn build_menu(object: Option<(InteractionKind, &str)>, ground_walkable: bool) -> Vec<MenuEntry> {
    let mut entries = Vec::with_capacity(4);
    if let Some((kind, name)) = object {
        entries.push(MenuEntry::new(MenuAction::Interact, hover_text(kind, name)));
        entries.push(MenuEntry::new(MenuAction::Examine, format!("Examine {name}")));
    }
    if ground_walkable {
        entries.push(MenuEntry::new(MenuAction::WalkHere, "Walk here"));
    }
    entries.push(MenuEntry::new(MenuAction::Cancel, "Cancel"));
    entries
}

/// The rows of the extended right-click menu while the editor is on. `object` is the name of
/// the editable object under the cursor; `selected` is the name of the selected object (if
/// any); `ground_free` says whether the tile under the cursor is open ground; `asset` is the
/// library asset the Place tool would place. Cancel is always present and always last.
pub fn build_edit_menu(object: Option<&str>, selected: Option<&str>, ground_free: bool, asset: Option<&str>) -> Vec<MenuEntry> {
    let mut entries = Vec::with_capacity(5);
    if let Some(name) = object {
        entries.push(MenuEntry::new(MenuAction::Select, format!("Select {name}")));
    }
    if let (Some(name), true) = (selected, ground_free) {
        entries.push(MenuEntry::new(MenuAction::MoveHere, format!("Move {name} here")));
    }
    if let (Some(name), true) = (asset, ground_free) {
        entries.push(MenuEntry::new(MenuAction::PlaceHere, format!("Place {name} here")));
    }
    if let Some(name) = object {
        entries.push(MenuEntry::new(MenuAction::Remove, format!("Remove {name}")));
    }
    entries.push(MenuEntry::new(MenuAction::Cancel, "Cancel"));
    entries
}

/// Row geometry of one open menu, so the Bevy layout and the click test cannot disagree.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MenuMetrics {
    pub width: f32,
    pub row_height: f32,
}

/// Font size, row height, side padding, narrowest menu, and the gap kept to the window edge.
pub const MENU_FONT_PX: f32 = 17.0;
pub const MENU_ROW_HEIGHT: f32 = 26.0;
pub const MENU_PAD_X: f32 = 10.0;
pub const MENU_MIN_WIDTH: f32 = 150.0;
pub const MENU_WINDOW_MARGIN: f32 = 6.0;

impl MenuMetrics {
    /// Total panel size for `rows` rows.
    pub fn size(&self, rows: usize) -> (f32, f32) {
        (self.width, self.row_height * rows as f32)
    }

    /// How many characters of a label fit on one row.
    pub fn label_chars(&self) -> usize {
        let inner = (self.width - 2.0 * MENU_PAD_X) / (MENU_FONT_PX * GLYPH_WIDTH_EM);
        if inner.is_finite() && inner >= 1.0 {
            inner.floor() as usize
        } else {
            1
        }
    }
}

/// The geometry for a menu with these rows in an interface `window_width` logical pixels wide:
/// wide enough for the longest label, never narrower than [`MENU_MIN_WIDTH`], and never wider
/// than the window less a margin (labels that still do not fit are shortened by
/// [`fit_label`]). Every row is one line high, so a long label can never spill into the next.
pub fn menu_metrics(entries: &[MenuEntry], window_width: f32) -> MenuMetrics {
    let longest = entries.iter().map(|e| e.label.chars().count()).max().unwrap_or(0) as f32;
    let wanted = (longest * MENU_FONT_PX * GLYPH_WIDTH_EM + 2.0 * MENU_PAD_X).max(MENU_MIN_WIDTH);
    let room = window_width - 2.0 * MENU_WINDOW_MARGIN;
    let width = if room.is_finite() && room > 0.0 { wanted.min(room) } else { wanted };
    MenuMetrics { width, row_height: MENU_ROW_HEIGHT }
}

/// The label as drawn: unchanged when it fits in `max_chars`, otherwise cut and ended with
/// `...` so it still fits (at least one character is always kept).
pub fn fit_label(label: &str, max_chars: usize) -> String {
    let max_chars = max_chars.max(1);
    if label.chars().count() <= max_chars {
        return label.to_string();
    }
    if max_chars <= 3 {
        return label.chars().take(max_chars).collect();
    }
    let kept: String = label.chars().take(max_chars - 3).collect();
    format!("{}...", kept.trim_end())
}

/// Where to put the top-left corner of a menu opened at `cursor` so it stays fully inside
/// the window: it flips to the other side of the cursor, then clamps. A menu larger than the
/// window is pinned to the top-left corner.
pub fn place_menu(cursor: (f32, f32), size: (f32, f32), window: (f32, f32)) -> (f32, f32) {
    let place = |c: f32, extent: f32, limit: f32| -> f32 {
        if !c.is_finite() || !extent.is_finite() || !limit.is_finite() || extent >= limit {
            return 0.0;
        }
        let pos = if c + extent > limit { c - extent } else { c };
        pos.clamp(0.0, limit - extent)
    };
    (place(cursor.0, size.0, window.0), place(cursor.1, size.1, window.1))
}

/// Which row (0-based) the cursor is over, if any. The panel covers
/// `[origin, origin + size)`, so the right and bottom edges are outside it.
pub fn row_at(origin: (f32, f32), metrics: MenuMetrics, rows: usize, cursor: (f32, f32)) -> Option<usize> {
    if rows == 0 || !(metrics.row_height.is_finite() && metrics.row_height > 0.0) || !metrics.width.is_finite() {
        return None;
    }
    let (dx, dy) = (cursor.0 - origin.0, cursor.1 - origin.1);
    if !dx.is_finite() || !dy.is_finite() || dx < 0.0 || dy < 0.0 || dx >= metrics.width {
        return None;
    }
    let row = (dy / metrics.row_height) as usize;
    (row < rows).then_some(row)
}

#[cfg(test)]
mod tests {
    use super::*;

    const M: MenuMetrics = MenuMetrics { width: 100.0, row_height: 20.0 };

    fn rows(labels: &[&str]) -> Vec<MenuEntry> {
        labels.iter().map(|l| MenuEntry::new(MenuAction::Cancel, *l)).collect()
    }

    fn actions(entries: &[MenuEntry]) -> Vec<MenuAction> {
        entries.iter().map(|e| e.action).collect()
    }

    #[test]
    fn the_edit_menu_offers_what_applies_and_always_ends_with_cancel() {
        let all = build_edit_menu(Some("Mistress Pell"), Some("tree"), true, Some("Signpost"));
        assert_eq!(actions(&all), [MenuAction::Select, MenuAction::MoveHere, MenuAction::PlaceHere, MenuAction::Remove, MenuAction::Cancel]);
        assert_eq!(all[0].label, "Select Mistress Pell");
        assert_eq!(all[1].label, "Move tree here");
        assert_eq!(all[2].label, "Place Signpost here");
        assert_eq!(all[3].label, "Remove Mistress Pell");
        assert_eq!(actions(&build_edit_menu(None, Some("tree"), true, None)), [MenuAction::MoveHere, MenuAction::Cancel]);
        assert_eq!(actions(&build_edit_menu(None, Some("tree"), false, Some("Tree"))), [MenuAction::Cancel], "no ground, no move or place");
        assert_eq!(actions(&build_edit_menu(Some("tree"), None, false, None)), [MenuAction::Select, MenuAction::Remove, MenuAction::Cancel]);
        assert_eq!(actions(&build_edit_menu(None, None, true, None)), [MenuAction::Cancel]);
    }


    #[test]
    fn the_menu_is_as_wide_as_its_longest_label() {
        let short = menu_metrics(&rows(&["Cancel"]), 1000.0);
        assert_eq!(short.width, MENU_MIN_WIDTH, "never narrower than the minimum");
        let long = menu_metrics(&rows(&["Select Mistress Pell", "Place Honeyberry bush here", "Cancel"]), 1000.0);
        assert!(long.width > MENU_MIN_WIDTH);
        for entry in rows(&["Select Mistress Pell", "Place Honeyberry bush here", "Cancel"]) {
            assert_eq!(fit_label(&entry.label, long.label_chars()), entry.label, "nothing is cut when there is room");
        }
    }

    #[test]
    fn a_menu_never_outgrows_a_small_window() {
        let entries = rows(&["Move the Village Guide to this tile right here, please"]);
        for window in [200.0, 320.0, 800.0] {
            let m = menu_metrics(&entries, window);
            assert!(m.width <= window - 2.0 * MENU_WINDOW_MARGIN + 1e-3, "{window}: {}", m.width);
            let drawn = fit_label(&entries[0].label, m.label_chars());
            assert!(drawn.chars().count() <= m.label_chars());
            assert!(drawn.chars().count() as f32 * MENU_FONT_PX * GLYPH_WIDTH_EM + 2.0 * MENU_PAD_X <= m.width + 1e-3);
        }
    }

    #[test]
    fn long_labels_are_shortened_with_dots_and_short_ones_untouched() {
        assert_eq!(fit_label("Cancel", 6), "Cancel");
        assert_eq!(fit_label("Remove Mistress Pell", 12), "Remove Mi...");
        assert_eq!(fit_label("Remove Mistress Pell", 3), "Rem");
        assert_eq!(fit_label("Remove", 0), "R");
        assert_eq!(fit_label("", 5), "");
        assert_eq!(fit_label("abcdefgh ij", 9), "abcdef...", "no space before the dots");
    }

    #[test]
    fn degenerate_windows_still_give_a_usable_menu() {
        for window in [0.0, -5.0, f32::NAN, f32::INFINITY] {
            let m = menu_metrics(&rows(&["Cancel"]), window);
            assert!(m.width.is_finite() && m.width >= 1.0 && m.label_chars() >= 1, "{window}");
        }
        assert_eq!(menu_metrics(&[], 800.0).width, MENU_MIN_WIDTH);
    }

    #[test]
    fn every_shipped_menu_label_fits_its_row_in_the_smallest_window() {
        // The widest rows any menu can build: long people names and the longest asset names.
        let worst = build_edit_menu(Some("Mistress Pell"), Some("Honeyberry bush"), true, Some("Honeyberry bush"));
        let m = menu_metrics(&worst, 800.0);
        for entry in &worst {
            let drawn = fit_label(&entry.label, m.label_chars());
            assert_eq!(drawn, entry.label, "{} would be cut", entry.label);
        }
        assert!(m.size(worst.len()).0 <= 800.0 && m.size(worst.len()).1 <= 600.0);
    }

    #[test]
    fn object_menu_lists_action_examine_walk_cancel_in_order() {
        let menu = build_menu(Some((InteractionKind::ChopDown, "tree")), true);
        assert_eq!(actions(&menu), [MenuAction::Interact, MenuAction::Examine, MenuAction::WalkHere, MenuAction::Cancel]);
        assert_eq!(menu[0].label, "Chop down tree");
        assert_eq!(menu[1].label, "Examine tree");
        assert_eq!(menu[2].label, "Walk here");
        assert_eq!(menu[3].label, "Cancel");
    }

    #[test]
    fn ground_menu_offers_walk_here_and_cancel() {
        let menu = build_menu(None, true);
        assert_eq!(actions(&menu), [MenuAction::WalkHere, MenuAction::Cancel]);
    }

    #[test]
    fn blocked_ground_offers_only_cancel() {
        let menu = build_menu(None, false);
        assert_eq!(actions(&menu), [MenuAction::Cancel]);
    }

    #[test]
    fn walk_here_is_left_out_when_the_ground_is_blocked_but_an_object_is_there() {
        let menu = build_menu(Some((InteractionKind::Mine, "copper vein")), false);
        assert_eq!(actions(&menu), [MenuAction::Interact, MenuAction::Examine, MenuAction::Cancel]);
    }

    #[test]
    fn cancel_is_always_last_and_never_missing() {
        for object in [None, Some((InteractionKind::TalkTo, "Village Guide"))] {
            for walkable in [false, true] {
                let menu = build_menu(object, walkable);
                assert_eq!(menu.last().map(|e| e.action), Some(MenuAction::Cancel));
            }
        }
    }

    #[test]
    fn menu_size_scales_with_rows() {
        assert_eq!(M.size(3), (100.0, 60.0));
        assert_eq!(M.size(0), (100.0, 0.0));
    }

    #[test]
    fn menu_opens_at_the_cursor_when_there_is_room() {
        assert_eq!(place_menu((50.0, 60.0), (100.0, 80.0), (800.0, 600.0)), (50.0, 60.0));
    }

    #[test]
    fn menu_flips_to_the_left_and_up_near_the_far_edges() {
        let origin = place_menu((780.0, 590.0), (100.0, 80.0), (800.0, 600.0));
        assert_eq!(origin, (680.0, 510.0));
    }

    #[test]
    fn menu_exactly_fitting_does_not_flip() {
        assert_eq!(place_menu((700.0, 520.0), (100.0, 80.0), (800.0, 600.0)), (700.0, 520.0));
    }

    #[test]
    fn menu_never_leaves_the_window() {
        // Too tight to open right/down, and flipping would push it off the near edge,
        // so it clamps to the corner instead of going negative.
        let origin = place_menu((10.0, 10.0), (100.0, 80.0), (105.0, 85.0));
        assert_eq!(origin, (0.0, 0.0));
        // A cursor far outside the window still yields a menu pinned to the far edge.
        assert_eq!(place_menu((5000.0, 5000.0), (100.0, 80.0), (800.0, 600.0)), (700.0, 520.0));
        for cursor in [(-50.0, -50.0), (0.0, 0.0), (400.0, 300.0), (799.0, 599.0), (5000.0, 5000.0)] {
            let (x, y) = place_menu(cursor, (100.0, 80.0), (800.0, 600.0));
            assert!(x >= 0.0 && x + 100.0 <= 800.0 && y >= 0.0 && y + 80.0 <= 600.0, "{cursor:?} -> {x},{y}");
        }
    }

    #[test]
    fn oversized_or_invalid_input_pins_to_the_corner() {
        assert_eq!(place_menu((10.0, 10.0), (500.0, 500.0), (400.0, 400.0)), (0.0, 0.0));
        assert_eq!(place_menu((f32::NAN, 10.0), (50.0, 50.0), (400.0, 400.0)).0, 0.0);
    }

    #[test]
    fn row_hit_test_maps_pixels_to_rows() {
        let origin = (10.0, 10.0);
        assert_eq!(row_at(origin, M, 3, (10.0, 10.0)), Some(0), "top-left corner is inside");
        assert_eq!(row_at(origin, M, 3, (50.0, 29.9)), Some(0));
        assert_eq!(row_at(origin, M, 3, (50.0, 30.0)), Some(1), "boundary belongs to the next row");
        assert_eq!(row_at(origin, M, 3, (109.9, 69.9)), Some(2));
    }

    #[test]
    fn clicks_outside_the_panel_hit_nothing() {
        let origin = (10.0, 10.0);
        assert_eq!(row_at(origin, M, 3, (9.9, 20.0)), None, "left of it");
        assert_eq!(row_at(origin, M, 3, (50.0, 9.9)), None, "above it");
        assert_eq!(row_at(origin, M, 3, (110.0, 20.0)), None, "right edge is outside");
        assert_eq!(row_at(origin, M, 3, (50.0, 70.0)), None, "bottom edge is outside");
    }

    #[test]
    fn hit_test_rejects_degenerate_input() {
        let origin = (0.0, 0.0);
        assert_eq!(row_at(origin, M, 0, (5.0, 5.0)), None, "no rows");
        assert_eq!(row_at(origin, M, 2, (f32::NAN, 5.0)), None);
        assert_eq!(row_at(origin, MenuMetrics { width: 100.0, row_height: 0.0 }, 2, (5.0, 5.0)), None);
        assert_eq!(row_at(origin, MenuMetrics { width: f32::NAN, row_height: 20.0 }, 2, (5.0, 5.0)), None);
    }

    #[test]
    fn placement_and_hit_test_agree() {
        let size = M.size(4);
        let origin = place_menu((790.0, 590.0), size, (800.0, 600.0));
        let centre_of_last_row = (origin.0 + 50.0, origin.1 + 70.0);
        assert_eq!(row_at(origin, M, 4, centre_of_last_row), Some(3));
    }
}
