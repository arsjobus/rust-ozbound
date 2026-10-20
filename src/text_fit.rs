//! Rules that keep interface text from being clipped: word wrapping, pagination, the limit on
//! the UI scale for a small window, and the geometry of each panel. Pure Rust.
//!
//! Bevy's default font is monospaced, so a line of `n` characters at font size `f` is `n * f *
//! GLYPH_WIDTH_EM` pixels wide. That makes wrapping exact enough to do ourselves: the panels are
//! given text that is already wrapped to their width, so what the tests measure is what is drawn.
//! [`GLYPH_WIDTH_EM`] is a little wider than the font really is, so a line can only come out
//! shorter than estimated, never longer.
//!
//! # Supported window sizes
//!
//! The smallest supported window is [`MIN_WINDOW`] (800 x 600). The interface is laid out in
//! "logical pixels" that are at least [`MIN_LOGICAL`] in size; [`fit_scale`] lowers a large text
//! size just enough that this holds, so at the smallest window normal text is never scaled and
//! larger sizes are reduced instead of overflowing.

/// The smallest window the game promises to lay out without clipping (width, height).
pub const MIN_WINDOW: (f32, f32) = (800.0, 600.0);
/// The smallest logical interface (window size divided by the UI scale) the panels fit in.
pub const MIN_LOGICAL: (f32, f32) = (680.0, 520.0);
/// The lowest UI scale ever used, however small the window gets.
pub const MIN_SCALE: f32 = 0.5;
/// Advance width of one character, as a fraction of the font size. The default font is 0.6; the
/// extra 3% means estimates err on the side of too wide.
pub const GLYPH_WIDTH_EM: f32 = 0.62;
/// Height of one line of text, as a fraction of the font size (generous on purpose).
pub const LINE_HEIGHT_EM: f32 = 1.3;
/// Space kept clear between a panel and the window edge.
pub const MARGIN: f32 = 10.0;

/// A panel's fixed geometry in logical pixels. `offset` is the distance from the window edge
/// the panel is anchored to (the bottom for the dialogue panel, the top for the others).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Panel {
    pub width: f32,
    pub padding: f32,
    pub offset: f32,
}

impl Panel {
    /// The width text may use inside the panel.
    pub fn inner_width(&self) -> f32 {
        self.width - 2.0 * self.padding
    }

    /// The height text may use inside the panel in a window `logical_height` tall.
    pub fn inner_height(&self, logical_height: f32) -> f32 {
        (logical_height - self.offset - MARGIN - 2.0 * self.padding).max(0.0)
    }
}

/// The chat area sits in the bottom-left corner (`offset` is its distance from the bottom).
pub const CHAT_PANEL: Panel = Panel { width: 440.0, padding: 8.0, offset: MARGIN };
/// Lines of the chat log that are visible at once (the input line is one more).
pub const CHAT_LINES: usize = 5;
pub const CHAT_PX: f32 = 15.0;
/// The chat panel's whole height: the log lines, the input line, and the padding.
pub const CHAT_HEIGHT: f32 = (CHAT_LINES as f32 + 1.0) * CHAT_PX * LINE_HEIGHT_EM + 2.0 * CHAT_PANEL.padding;

/// The toolbar of side-panel buttons in the top-right corner.
pub const TOOLBAR_PX: f32 = 16.0;
pub const TOOLBAR_BUTTON_HEIGHT: f32 = 30.0;
pub const TOOLBAR_BUTTON_PAD: f32 = 10.0;
pub const TOOLBAR_GAP: f32 = 6.0;

/// The dialogue panel stands just above the chat area, so the two never overlap.
pub const DIALOGUE_PANEL: Panel = Panel { width: 640.0, padding: 14.0, offset: CHAT_PANEL.offset + CHAT_HEIGHT + MARGIN };
pub const PACK_PANEL: Panel = Panel { width: 620.0, padding: 14.0, offset: 40.0 };
/// The side panels (bag, skills, quest book) hang just below the toolbar.
pub const DOCK_PANEL: Panel = Panel { width: 400.0, padding: 10.0, offset: MARGIN + TOOLBAR_BUTTON_HEIGHT + TOOLBAR_GAP };
pub const PAUSE_PANEL: Panel = Panel { width: 640.0, padding: 14.0, offset: 40.0 };

/// Font sizes, shared by the Bevy layer and the estimates below.
pub const HUD_PX: f32 = 19.0;
pub const SPEAKER_PX: f32 = 21.0;
pub const DIALOGUE_PX: f32 = 19.0;
pub const CHOICE_PX: f32 = 18.0;
pub const PACK_TITLE_PX: f32 = 21.0;
pub const PACK_INFO_PX: f32 = 17.0;
pub const PACK_ROW_PX: f32 = 18.0;
/// Text size inside the side panels.
pub const JOURNAL_PX: f32 = 17.0;
pub const MENU_PX: f32 = 20.0;
pub const CONTROLS_PX: f32 = 16.0;
pub const HINT_PX: f32 = 16.0;

/// Portrait size and the gap beside it in the dialogue panel.
pub const PORTRAIT: f32 = 96.0;
pub const PORTRAIT_GAP: f32 = 14.0;
/// Vertical padding inside a clickable row (each side) and the gap the panels put between rows.
pub const ROW_PAD: f32 = 4.0;
pub const ROW_GAP: f32 = 6.0;
/// Horizontal padding inside a clickable row (each side).
pub const ROW_PAD_X: f32 = 10.0;

/// How many characters of size `font_px` fit in `width_px` (at least 1).
pub fn chars_per_line(width_px: f32, font_px: f32) -> usize {
    if !width_px.is_finite() || !font_px.is_finite() || font_px <= 0.0 {
        return 1;
    }
    ((width_px / (font_px * GLYPH_WIDTH_EM)).floor().max(1.0)) as usize
}

/// How many lines of size `font_px` fit in `height_px` (at least 1).
pub fn lines_per_page(height_px: f32, font_px: f32) -> usize {
    if !height_px.is_finite() || !font_px.is_finite() || font_px <= 0.0 {
        return 1;
    }
    ((height_px / (font_px * LINE_HEIGHT_EM)).floor().max(1.0)) as usize
}

/// The height of `lines` lines of text of size `font_px`.
pub fn text_height(lines: usize, font_px: f32) -> f32 {
    lines as f32 * font_px * LINE_HEIGHT_EM
}

/// The width of the toolbar when its buttons carry these labels.
pub fn toolbar_width(labels: &[String]) -> f32 {
    let buttons: f32 = labels.iter().map(|label| label.chars().count() as f32 * TOOLBAR_PX * GLYPH_WIDTH_EM + 2.0 * TOOLBAR_BUTTON_PAD).sum();
    buttons + TOOLBAR_GAP * labels.len().saturating_sub(1) as f32
}

/// Wraps `text` to at most `width` characters per line. Existing line breaks are kept (an empty
/// line stays an empty line); words are split between lines at spaces, and a word longer than a
/// whole line is cut. Lines never end in a space. Empty text gives one empty line.
pub fn wrap(text: &str, width: usize) -> Vec<String> {
    let width = width.max(1);
    let mut out = Vec::new();
    for source in text.split('\n') {
        let source = source.trim_end_matches('\r');
        if source.trim().is_empty() {
            out.push(String::new());
            continue;
        }
        // Keep indentation on the first line of each source line.
        let indent: String = source.chars().take_while(|c| *c == ' ').collect();
        let indent = if indent.chars().count() < width { indent } else { String::new() };
        let mut line = indent.clone();
        let mut has_word = false;
        for word in source.trim_start_matches(' ').split(' ').filter(|w| !w.is_empty()) {
            let mut word: &str = word;
            loop {
                let used = line.chars().count();
                let need = word.chars().count() + usize::from(has_word);
                if used + need <= width {
                    if has_word {
                        line.push(' ');
                    }
                    line.push_str(word);
                    has_word = true;
                    break;
                }
                if has_word {
                    out.push(std::mem::take(&mut line));
                    line = String::new();
                    has_word = false;
                    continue;
                }
                // A word that cannot fit even on an empty line: cut it.
                let room = width.saturating_sub(used).max(1);
                let cut = word.char_indices().nth(room).map_or(word.len(), |(index, _)| index);
                if cut >= word.len() {
                    line.push_str(word);
                    has_word = true;
                    break;
                }
                line.push_str(&word[..cut]);
                out.push(std::mem::take(&mut line));
                word = &word[cut..];
            }
        }
        out.push(line);
    }
    out
}

/// [`wrap`], joined back into one string.
pub fn wrap_text(text: &str, width: usize) -> String {
    wrap(text, width).join("\n")
}

/// Splits `text` into pages of at most `lines_per_page` wrapped lines each. A page prefers to end
/// at a blank line (a paragraph break) when there is one near the bottom; pages never start or
/// end with blank lines, and no text is dropped. Always returns at least one page.
pub fn paginate(text: &str, width: usize, lines_per_page: usize) -> Vec<String> {
    let per_page = lines_per_page.max(1);
    let lines = wrap(text, width);
    let mut pages = Vec::new();
    let mut start = 0;
    while start < lines.len() {
        // Skip blank lines at the top of a page.
        if lines[start].is_empty() {
            start += 1;
            continue;
        }
        let mut end = (start + per_page).min(lines.len());
        if end < lines.len() {
            // Prefer a paragraph break within the last half of the page.
            let earliest = start + per_page - per_page.div_ceil(2);
            if let Some(blank) = (earliest.max(start + 1)..end).rev().find(|i| lines[*i].is_empty()) {
                end = blank;
            }
        }
        let mut page = &lines[start..end];
        while page.last().is_some_and(|l| l.is_empty()) {
            page = &page[..page.len() - 1];
        }
        pages.push(page.join("\n"));
        start = end;
    }
    if pages.is_empty() {
        pages.push(String::new());
    }
    pages
}

/// The UI scale to use: the one asked for, lowered if the window is too small to lay the panels
/// out at that scale, and never below [`MIN_SCALE`]. A bad request or window size gives 1.0.
pub fn fit_scale(requested: f32, window_width: f32, window_height: f32) -> f32 {
    if !requested.is_finite() || requested <= 0.0 || !window_width.is_finite() || !window_height.is_finite() || window_width <= 0.0 || window_height <= 0.0 {
        return 1.0;
    }
    let most = (window_width / MIN_LOGICAL.0).min(window_height / MIN_LOGICAL.1);
    requested.min(most).max(MIN_SCALE.min(requested))
}

/// The size of the interface in logical pixels for a window and a UI scale.
pub fn logical_size(window_width: f32, window_height: f32, scale: f32) -> (f32, f32) {
    let scale = if scale.is_finite() && scale > 0.0 { scale } else { 1.0 };
    (window_width / scale, window_height / scale)
}

/// The estimated height of the dialogue panel for one conversation step: the portrait row (as
/// tall as the portrait or the words, whichever is more) plus one row per choice.
pub fn dialogue_height(text: &str, choices: &[String]) -> f32 {
    let words_width = DIALOGUE_PANEL.inner_width() - PORTRAIT - PORTRAIT_GAP;
    let text_lines = wrap(text, chars_per_line(words_width, DIALOGUE_PX)).len();
    let words = text_height(1, SPEAKER_PX) + 4.0 + text_height(text_lines, DIALOGUE_PX) + 8.0;
    let mut total = words.max(PORTRAIT);
    let choice_width = DIALOGUE_PANEL.inner_width() - 2.0 * ROW_PAD_X;
    for (index, choice) in choices.iter().enumerate() {
        let lines = wrap(&format!("{}. {choice}", index + 1), chars_per_line(choice_width, CHOICE_PX)).len();
        total += ROW_GAP + text_height(lines, CHOICE_PX) + 2.0 * ROW_PAD;
    }
    total + 2.0 * DIALOGUE_PANEL.padding
}

/// The estimated height of the crafting-and-gear panel: title, three info lines, and the rows.
pub fn pack_height(info: &str, rows: &[String]) -> f32 {
    let info_lines = wrap(info, chars_per_line(PACK_PANEL.inner_width(), PACK_INFO_PX)).len();
    let mut total = text_height(1, PACK_TITLE_PX) + text_height(info_lines, PACK_INFO_PX) + 8.0 + 4.0 * (rows.len() as f32 + 2.0);
    let row_width = PACK_PANEL.inner_width() - 2.0 * ROW_PAD_X;
    for (index, row) in rows.iter().enumerate() {
        let lines = wrap(&format!("{}. {row}", index + 1), chars_per_line(row_width, PACK_ROW_PX)).len();
        total += text_height(lines, PACK_ROW_PX) + 2.0 * ROW_PAD;
    }
    total + 2.0 * PACK_PANEL.padding
}

/// The estimated height of the pause menu: title, an optional body (the controls list), the rows,
/// and the hint line.
pub fn pause_height(body: Option<&str>, rows: &[String], hint: &str) -> f32 {
    let inner = PAUSE_PANEL.inner_width();
    let mut total = text_height(1, SPEAKER_PX) + ROW_GAP;
    if let Some(body) = body {
        total += text_height(wrap(body, chars_per_line(inner, CONTROLS_PX)).len(), CONTROLS_PX) + ROW_GAP;
    }
    let row_width = inner - 2.0 * ROW_PAD_X;
    for row in rows {
        total += text_height(wrap(row, chars_per_line(row_width, MENU_PX)).len(), MENU_PX) + 2.0 * ROW_PAD + ROW_GAP;
    }
    total += text_height(wrap(hint, chars_per_line(inner, HINT_PX)).len(), HINT_PX) + ROW_GAP;
    total + 2.0 * PAUSE_PANEL.padding
}

#[cfg(test)]
mod tests {
    use super::*;

    fn widest(lines: &[String]) -> usize {
        lines.iter().map(|l| l.chars().count()).max().unwrap_or(0)
    }

    #[test]
    fn chars_and_lines_per_page_follow_the_font_and_never_reach_zero() {
        assert_eq!(chars_per_line(620.0, 18.0), (620.0_f32 / (18.0 * GLYPH_WIDTH_EM)).floor() as usize);
        assert!(chars_per_line(620.0, 24.0) < chars_per_line(620.0, 12.0), "bigger text fits fewer characters");
        for bad in [0.0, -5.0, f32::NAN, f32::INFINITY] {
            assert_eq!(chars_per_line(bad, 18.0), 1);
            assert_eq!(chars_per_line(100.0, bad), 1);
            assert_eq!(lines_per_page(bad, 18.0), 1);
            assert_eq!(lines_per_page(100.0, bad), 1);
        }
        assert_eq!(chars_per_line(1.0, 18.0), 1);
        assert!(lines_per_page(400.0, 17.0) > lines_per_page(400.0, 24.0));
        assert!((text_height(3, 10.0) - 3.0 * 10.0 * LINE_HEIGHT_EM).abs() < 1.0e-4);
    }

    #[test]
    fn wrap_breaks_at_spaces_and_respects_the_width() {
        let lines = wrap("the quick brown fox jumps over the lazy dog", 15);
        assert_eq!(lines, ["the quick brown", "fox jumps over", "the lazy dog"]);
        assert!(widest(&lines) <= 15);
        assert_eq!(wrap("exactly ten", 11), ["exactly ten"], "a line that just fits is kept whole");
        assert_eq!(wrap("exactly ten", 10), ["exactly", "ten"]);
    }

    #[test]
    fn wrap_keeps_existing_breaks_and_blank_lines() {
        assert_eq!(wrap("one\n\ntwo", 20), ["one", "", "two"]);
        assert_eq!(wrap("a\r\nb", 20), ["a", "b"]);
        assert_eq!(wrap("", 20), [""]);
        assert_eq!(wrap("   \n", 20), ["", ""]);
        assert_eq!(wrap("tail\n", 20), ["tail", ""]);
    }

    #[test]
    fn wrap_cuts_words_longer_than_a_line_and_never_loops() {
        let lines = wrap("abcdefghijklmnopqrstuvwxyz", 10);
        assert_eq!(lines, ["abcdefghij", "klmnopqrst", "uvwxyz"]);
        let mixed = wrap("hi abcdefghijklmnop ok", 8);
        assert!(widest(&mixed) <= 8, "{mixed:?}");
        assert_eq!(mixed.concat().replace(' ', ""), "hiabcdefghijklmnopok");
        assert_eq!(wrap("anything at all", 0), wrap("anything at all", 1), "width zero is treated as one");
        assert!(widest(&wrap("héllo wörld ünïcode", 6)) <= 6, "counts characters, not bytes");
    }

    #[test]
    fn wrap_collapses_runs_of_spaces_inside_a_line_but_keeps_the_indent() {
        assert_eq!(wrap("  indented   words  here", 40), ["  indented words here"]);
        let lines = wrap("  indented words that wrap onto a second line", 20);
        assert!(lines[0].starts_with("  ") && !lines[1].starts_with(' '), "{lines:?}");
        assert!(widest(&lines) <= 20);
        // An indent as wide as the line is dropped instead of leaving no room for text.
        assert!(widest(&wrap("          x", 5)) <= 5);
    }

    #[test]
    fn wrapped_lines_never_end_in_a_space_or_lose_a_word() {
        let text = "Bring me three moonpetals, said the apothecary, and I will pay you in good timber.  Mind the pond.";
        for width in 1..60 {
            let lines = wrap(text, width);
            assert!(widest(&lines) <= width, "width {width}: {lines:?}");
            assert!(lines.iter().all(|l| !l.ends_with(' ')), "width {width}");
            if width >= 12 {
                let words = |s: &str| s.split_whitespace().map(str::to_string).collect::<Vec<_>>();
                assert_eq!(words(&lines.join(" ")), words(text), "width {width}: no word lost or reordered");
            }
        }
    }

    #[test]
    fn wrap_text_joins_with_newlines() {
        assert_eq!(wrap_text("aaa bbb ccc", 7), "aaa bbb\nccc");
    }

    #[test]
    fn short_text_is_one_page() {
        assert_eq!(paginate("hello world", 40, 10), ["hello world"]);
        assert_eq!(paginate("", 40, 10), [""]);
        assert_eq!(paginate("\n\n", 40, 10), [""], "only blank lines is still one (empty) page");
    }

    #[test]
    fn pages_hold_at_most_the_page_height_and_lose_no_text() {
        let text = (1..=40).map(|n| format!("line number {n}")).collect::<Vec<_>>().join("\n");
        for per_page in [1, 2, 3, 7, 10, 39, 40, 100] {
            let pages = paginate(&text, 30, per_page);
            let mut joined = Vec::new();
            for page in &pages {
                let lines: Vec<&str> = page.split('\n').collect();
                assert!(lines.len() <= per_page, "per_page {per_page}: {} lines", lines.len());
                joined.extend(lines.iter().map(|l| l.to_string()));
            }
            let expected: Vec<String> = text.split('\n').map(str::to_string).collect();
            assert_eq!(joined, expected, "per_page {per_page}");
            assert_eq!(pages.len(), 40_usize.div_ceil(per_page), "per_page {per_page}");
        }
    }

    #[test]
    fn pages_break_at_paragraph_gaps_and_never_start_or_end_blank() {
        let text = "alpha one\nalpha two\nalpha three\n\nbeta one\nbeta two\nbeta three\n\ngamma one\ngamma two";
        let pages = paginate(text, 40, 5);
        assert_eq!(pages[0], "alpha one\nalpha two\nalpha three", "the page ends at the paragraph break");
        for page in &pages {
            assert!(!page.starts_with('\n') && !page.ends_with('\n'), "{page:?}");
            assert!(!page.is_empty());
        }
        let all: String = pages.join("\n");
        for word in ["alpha", "beta", "gamma"] {
            assert!(all.contains(word));
        }
    }

    #[test]
    fn a_page_without_a_nearby_paragraph_break_just_fills_up() {
        let text = (1..=9).map(|n| format!("row {n}")).collect::<Vec<_>>().join("\n");
        let pages = paginate(&text, 20, 4);
        assert_eq!(pages.len(), 3);
        assert_eq!(pages[0].lines().count(), 4);
    }

    #[test]
    fn pagination_wraps_before_it_pages() {
        let text = "word ".repeat(60);
        let pages = paginate(&text, 20, 3);
        for page in &pages {
            assert!(page.lines().count() <= 3);
            assert!(widest(&page.lines().map(str::to_string).collect::<Vec<_>>()) <= 20);
        }
        assert!(pages.len() > 1);
    }

    #[test]
    fn pagination_always_terminates_even_with_tiny_pages_and_odd_text() {
        for text in ["", "\n", "\n\n\nx", "x\n\n\n", "a\n\nb\n\nc", &"x ".repeat(100)] {
            for width in [1, 2, 5, 80] {
                for per_page in [0, 1, 2, 3, 50] {
                    let pages = paginate(text, width, per_page);
                    assert!(!pages.is_empty());
                    let words = |s: &str| s.split_whitespace().count();
                    assert_eq!(pages.iter().map(|p| words(p)).sum::<usize>(), words(&wrap(text, width).join(" ")), "{text:?} {width} {per_page}");
                }
            }
        }
    }

    #[test]
    fn normal_text_is_never_scaled_down_at_any_supported_window() {
        for (w, h) in [(800.0, 600.0), (1100.0, 720.0), (1280.0, 720.0), (1920.0, 1080.0), (800.0, 2000.0), (3000.0, 600.0)] {
            assert_eq!(fit_scale(1.0, w, h), 1.0, "{w}x{h}");
        }
    }

    #[test]
    fn larger_text_is_lowered_just_enough_for_a_small_window() {
        assert_eq!(fit_scale(1.5, 1920.0, 1080.0), 1.5, "plenty of room");
        let small = fit_scale(1.5, MIN_WINDOW.0, MIN_WINDOW.1);
        assert!((1.0..1.5).contains(&small), "{small}");
        let (lw, lh) = logical_size(MIN_WINDOW.0, MIN_WINDOW.1, small);
        assert!(lw >= MIN_LOGICAL.0 - 0.01 && lh >= MIN_LOGICAL.1 - 0.01, "{lw}x{lh}");
        // Every size at every window from the minimum up fits the logical minimum.
        for scale in [1.0, 1.25, 1.5] {
            for w in (800..=2000).step_by(100) {
                for h in (600..=1200).step_by(100) {
                    let used = fit_scale(scale, w as f32, h as f32);
                    let (lw, lh) = logical_size(w as f32, h as f32, used);
                    assert!(lw >= MIN_LOGICAL.0 - 0.01 && lh >= MIN_LOGICAL.1 - 0.01, "{scale} at {w}x{h}");
                    assert!(used <= scale + f32::EPSILON);
                }
            }
        }
    }

    #[test]
    fn fit_scale_is_bounded_and_ignores_bad_input() {
        assert!(fit_scale(1.5, 100.0, 100.0) >= MIN_SCALE - f32::EPSILON, "a tiny window never shrinks text to nothing");
        for bad in [f32::NAN, f32::INFINITY, 0.0, -1.0] {
            assert_eq!(fit_scale(bad, 1000.0, 800.0), 1.0);
            assert_eq!(fit_scale(1.0, bad, 800.0), 1.0);
            assert_eq!(fit_scale(1.0, 1000.0, bad), 1.0);
        }
        assert_eq!(logical_size(800.0, 600.0, 0.0), (800.0, 600.0));
        assert_eq!(logical_size(800.0, 600.0, 2.0), (400.0, 300.0));
    }

    #[test]
    fn the_panels_fit_across_the_smallest_logical_window() {
        let (w, h) = MIN_LOGICAL;
        for panel in [DIALOGUE_PANEL, PACK_PANEL, DOCK_PANEL, PAUSE_PANEL] {
            assert!(panel.width + 2.0 * MARGIN <= w, "{panel:?} is too wide for {w}");
            assert!(panel.inner_height(h) > 100.0, "{panel:?} has no room in {h}");
            assert!(panel.inner_width() > 200.0);
        }
        // The journal and dialogue panels may be open together with the HUD; the journal sits on
        // the right and the dialogue on the bottom centre, so check they do not overlap in height.
        assert!(DOCK_PANEL.offset + DOCK_PANEL.inner_height(h) + 2.0 * DOCK_PANEL.padding <= h);
    }

    #[test]
    fn height_estimates_grow_with_text_and_choices() {
        let short = dialogue_height("Hello.", &["Bye.".to_string()]);
        let long = dialogue_height(&"Many words in a long speech. ".repeat(20), &["Bye.".to_string()]);
        let choices = dialogue_height("Hello.", &["Bye.".to_string(), "Tell me more.".to_string(), "Where is the pond?".to_string()]);
        assert!(long > short && choices > short);
        assert!(short >= PORTRAIT + 2.0 * DIALOGUE_PANEL.padding, "never shorter than the portrait");
        let few = pack_height("bag", &["a".to_string()]);
        let many = pack_height("bag", &(0..8).map(|n| format!("row {n}")).collect::<Vec<_>>());
        assert!(many > few);
        let rows = vec!["Resume".to_string(), "Quit".to_string()];
        let plain = pause_height(None, &rows, "Esc: back");
        let with_body = pause_height(Some(&"controls line\n".repeat(10)), &rows, "Esc: back");
        assert!(with_body > plain && plain > 2.0 * PAUSE_PANEL.padding);
    }
}
