//! Player settings as validated values and a small versioned text format, independent of
//! rendering and of file I/O (`settings_store.rs` reads and writes the file safely).
//!
//! Settings are not part of a save: they belong to the player's machine, so they live in their
//! own file and are never mixed into `saves/game.txt`.
//!
//! # Format, version 1
//!
//! The first entry is the header; the rest are `key = value` lines in any order. Blank lines
//! and lines starting with `#` are ignored, and CRLF line endings and a leading BOM are
//! accepted. Keys are case-insensitive. A missing key means "the default", so a file written by
//! an older version still loads. An unknown key, a repeated key, or a value that is not allowed
//! is an error with its line number, never silently ignored, so a typo cannot hide.
//!
//! ```text
//! ozbound-settings 1
//! volume = 7          # 0 (silent) to 10
//! ambient = on        # on or off: the wind-and-birdsong loop
//! text_size = normal  # normal, large, or larger
//! show_perf = off     # on or off: the frame-time overlay (F3)
//! ```
//!
//! # Changing the format
//!
//! Bump [`FORMAT_VERSION`] when a key changes meaning, keep reading every older version, never
//! reuse a retired key for something else, and add a test that loads a saved sample of each
//! version.

use crate::textdata::ContentError;

/// The version written by [`Settings::to_text`].
pub const FORMAT_VERSION: u32 = 1;
/// The loudest master volume step.
pub const MAX_VOLUME: u8 = 10;
/// The header keyword.
pub const HEADER: &str = "ozbound-settings";

/// How large the interface text is. The Bevy layer turns this into a UI scale; the layout rules
/// in `text_fit.rs` shrink it when the window is too small for the panels, so text never clips.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TextSize {
    #[default]
    Normal,
    Large,
    Larger,
}

impl TextSize {
    pub const ALL: [TextSize; 3] = [TextSize::Normal, TextSize::Large, TextSize::Larger];

    /// The UI scale this size asks for (before the window-size limit is applied).
    pub fn scale(self) -> f32 {
        match self {
            Self::Normal => 1.0,
            Self::Large => 1.25,
            Self::Larger => 1.5,
        }
    }

    /// The value written to the settings file.
    pub fn key(self) -> &'static str {
        match self {
            Self::Normal => "normal",
            Self::Large => "large",
            Self::Larger => "larger",
        }
    }

    /// The name shown in the menu.
    pub fn label(self) -> &'static str {
        match self {
            Self::Normal => "Normal",
            Self::Large => "Large",
            Self::Larger => "Larger",
        }
    }

    pub fn from_key(key: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|size| size.key().eq_ignore_ascii_case(key))
    }

    /// The next size, wrapping from the largest back to the smallest.
    pub fn next(self) -> Self {
        match self {
            Self::Normal => Self::Large,
            Self::Large => Self::Larger,
            Self::Larger => Self::Normal,
        }
    }

    /// The previous size, wrapping from the smallest to the largest.
    pub fn previous(self) -> Self {
        match self {
            Self::Normal => Self::Larger,
            Self::Large => Self::Normal,
            Self::Larger => Self::Large,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Settings {
    /// Master volume, 0 to [`MAX_VOLUME`].
    volume: u8,
    /// Whether the ambient loop plays at all.
    pub ambient: bool,
    pub text_size: TextSize,
    /// Whether the frame-time overlay is shown.
    pub show_perf: bool,
}

impl Default for Settings {
    fn default() -> Self {
        // Volume 7 with the ambient loop on is the 0.35 gain the game always used.
        Self { volume: 7, ambient: true, text_size: TextSize::Normal, show_perf: false }
    }
}

impl Settings {
    pub fn volume(&self) -> u8 {
        self.volume
    }

    /// Sets the volume, clamping to the allowed range.
    pub fn set_volume(&mut self, volume: u8) {
        self.volume = volume.min(MAX_VOLUME);
    }

    /// One step louder. Returns whether anything changed (false at the top).
    pub fn volume_up(&mut self) -> bool {
        let before = self.volume;
        self.set_volume(self.volume.saturating_add(1));
        self.volume != before
    }

    /// One step quieter. Returns whether anything changed (false at the bottom).
    pub fn volume_down(&mut self) -> bool {
        let before = self.volume;
        self.volume = self.volume.saturating_sub(1);
        self.volume != before
    }

    /// The linear gain for the ambient loop: silent when it is switched off or the volume is 0,
    /// otherwise proportional to the volume, topping out at 0.5 (the loop is already quiet by
    /// design, so the top step is not painful).
    pub fn ambient_gain(&self) -> f32 {
        if !self.ambient {
            return 0.0;
        }
        f32::from(self.volume) / f32::from(MAX_VOLUME) * 0.5
    }

    /// The UI scale this text size asks for.
    pub fn requested_ui_scale(&self) -> f32 {
        self.text_size.scale()
    }

    /// The settings as text in the current format (see the module docs).
    pub fn to_text(&self) -> String {
        let on_off = |on: bool| if on { "on" } else { "off" };
        format!(
            "{HEADER} {FORMAT_VERSION}\nvolume = {}\nambient = {}\ntext_size = {}\nshow_perf = {}\n",
            self.volume,
            on_off(self.ambient),
            self.text_size.key(),
            on_off(self.show_perf),
        )
    }

    /// Reads settings text. Keys that are missing keep their defaults; anything wrong is an
    /// error that names its line, and nothing loads partly.
    pub fn from_text(text: &str) -> Result<Self, ContentError> {
        let mut entries = text
            .trim_start_matches('\u{feff}')
            .lines()
            .enumerate()
            .map(|(index, raw)| (index + 1, raw.trim()))
            .filter(|(_, line)| !line.is_empty() && !line.starts_with('#'));

        let Some((header_line, header)) = entries.next() else {
            return Err(ContentError::at(0, format!("the settings file is empty (expected the header \"{HEADER} {FORMAT_VERSION}\")")));
        };
        check_header(header_line, header)?;

        let mut settings = Settings::default();
        let mut seen: Vec<&'static str> = Vec::new();
        for (number, line) in entries {
            let Some((key, value)) = line.split_once('=') else {
                return Err(ContentError::at(number, format!("expected \"key = value\", found \"{line}\"")));
            };
            let (key, value) = (key.trim().to_ascii_lowercase(), value.trim());
            // Allow a trailing comment on a value line.
            let value = value.split('#').next().unwrap_or("").trim();
            let name = KEYS
                .iter()
                .copied()
                .find(|known| *known == key)
                .ok_or_else(|| ContentError::at(number, format!("unknown setting \"{key}\" (known settings: {})", KEYS.join(", "))))?;
            if seen.contains(&name) {
                return Err(ContentError::at(number, format!("\"{name}\" is set more than once")));
            }
            seen.push(name);
            match name {
                "volume" => {
                    let volume: u8 = value
                        .parse()
                        .ok()
                        .filter(|v| *v <= MAX_VOLUME)
                        .ok_or_else(|| ContentError::at(number, format!("volume must be a whole number from 0 to {MAX_VOLUME}, not \"{value}\"")))?;
                    settings.volume = volume;
                }
                "ambient" => settings.ambient = parse_bool(number, name, value)?,
                "show_perf" => settings.show_perf = parse_bool(number, name, value)?,
                "text_size" => {
                    settings.text_size = TextSize::from_key(value).ok_or_else(|| {
                        let allowed: Vec<&str> = TextSize::ALL.iter().map(|size| size.key()).collect();
                        ContentError::at(number, format!("text_size must be one of {}, not \"{value}\"", allowed.join(", ")))
                    })?;
                }
                _ => unreachable!("every key in KEYS is handled above"),
            }
        }
        Ok(settings)
    }
}

const KEYS: [&str; 4] = ["volume", "ambient", "text_size", "show_perf"];

fn check_header(line: usize, header: &str) -> Result<(), ContentError> {
    let mut words = header.split_whitespace();
    let (keyword, version, extra) = (words.next(), words.next(), words.next());
    if keyword != Some(HEADER) || extra.is_some() {
        return Err(ContentError::at(line, format!("this is not a settings file (the first line must be \"{HEADER} {FORMAT_VERSION}\")")));
    }
    let version: u32 = version
        .and_then(|v| v.parse().ok())
        .filter(|v| *v >= 1)
        .ok_or_else(|| ContentError::at(line, format!("the settings version must be a whole number from 1 (found \"{header}\")")))?;
    if version > FORMAT_VERSION {
        return Err(ContentError::at(
            line,
            format!("these settings were written by a newer version of the game (format {version}; this one reads up to {FORMAT_VERSION})"),
        ));
    }
    Ok(())
}

fn parse_bool(line: usize, name: &str, value: &str) -> Result<bool, ContentError> {
    match value.to_ascii_lowercase().as_str() {
        "on" | "true" | "yes" => Ok(true),
        "off" | "false" | "no" => Ok(false),
        _ => Err(ContentError::at(line, format!("{name} must be on or off, not \"{value}\""))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn busy() -> Settings {
        let mut settings = Settings::default();
        settings.set_volume(3);
        settings.ambient = false;
        settings.text_size = TextSize::Larger;
        settings.show_perf = true;
        settings
    }

    #[test]
    fn defaults_match_the_gain_the_game_always_used() {
        let settings = Settings::default();
        assert_eq!(settings.volume(), 7);
        assert!((settings.ambient_gain() - 0.35).abs() < 1.0e-6);
        assert_eq!(settings.text_size, TextSize::Normal);
        assert!(!settings.show_perf);
        assert!((settings.requested_ui_scale() - 1.0).abs() < f32::EPSILON);
    }

    #[test]
    fn volume_is_clamped_and_steps_report_change() {
        let mut settings = Settings::default();
        settings.set_volume(200);
        assert_eq!(settings.volume(), MAX_VOLUME);
        assert!(!settings.volume_up(), "already at the top");
        assert_eq!(settings.volume(), MAX_VOLUME);
        settings.set_volume(0);
        assert!(!settings.volume_down(), "already at the bottom");
        assert!(settings.volume_up());
        assert_eq!(settings.volume(), 1);
    }

    #[test]
    fn ambient_gain_is_silent_when_off_or_muted_and_rises_with_volume() {
        let mut settings = Settings::default();
        settings.set_volume(0);
        assert_eq!(settings.ambient_gain(), 0.0);
        settings.set_volume(MAX_VOLUME);
        assert!((settings.ambient_gain() - 0.5).abs() < 1.0e-6);
        settings.ambient = false;
        assert_eq!(settings.ambient_gain(), 0.0);
        let mut last = -1.0;
        settings.ambient = true;
        for volume in 0..=MAX_VOLUME {
            settings.set_volume(volume);
            assert!(settings.ambient_gain() > last, "gain rises with every step");
            last = settings.ambient_gain();
        }
    }

    #[test]
    fn text_sizes_cycle_both_ways_and_get_larger() {
        let mut size = TextSize::Normal;
        for _ in 0..3 {
            assert_eq!(size.next().previous(), size);
            size = size.next();
        }
        assert_eq!(size, TextSize::Normal, "three steps wrap all the way round");
        assert!(TextSize::Normal.scale() < TextSize::Large.scale() && TextSize::Large.scale() < TextSize::Larger.scale());
        assert_eq!(TextSize::Normal.scale(), 1.0, "normal is never scaled");
        for size in TextSize::ALL {
            assert_eq!(TextSize::from_key(size.key()), Some(size));
            assert_eq!(TextSize::from_key(&size.key().to_uppercase()), Some(size));
        }
        assert_eq!(TextSize::from_key("huge"), None);
    }

    #[test]
    fn text_round_trips_and_is_deterministic() {
        for settings in [Settings::default(), busy()] {
            let text = settings.to_text();
            assert_eq!(Settings::from_text(&text).unwrap(), settings);
            assert_eq!(text, settings.to_text());
        }
        assert!(Settings::default().to_text().starts_with("ozbound-settings 1\n"));
    }

    #[test]
    fn missing_keys_keep_their_defaults() {
        let loaded = Settings::from_text("ozbound-settings 1\nvolume = 2\n").unwrap();
        assert_eq!(loaded.volume(), 2);
        assert_eq!(Settings { volume: 7, ..loaded }, Settings::default());
        assert_eq!(Settings::from_text("ozbound-settings 1").unwrap(), Settings::default());
    }

    #[test]
    fn comments_blank_lines_crlf_bom_case_and_trailing_comments_are_accepted() {
        let text = "\u{feff}# my settings\r\n\r\nOzbound-Settings 1\r\nVOLUME = 4   # quiet\r\n  Ambient=Off\r\ntext_size = LARGE\r\nshow_perf = yes\r\n";
        // The header keyword is case-sensitive on purpose: only the keys and values are lenient.
        assert!(Settings::from_text(text).is_err());
        let text = text.replace("Ozbound-Settings", "ozbound-settings");
        let loaded = Settings::from_text(&text).unwrap();
        assert_eq!(loaded.volume(), 4);
        assert!(!loaded.ambient);
        assert_eq!(loaded.text_size, TextSize::Large);
        assert!(loaded.show_perf);
    }

    #[test]
    fn every_error_names_its_line() {
        let cases: [(&str, usize, &str); 10] = [
            ("ozbound-settings 1\nvolume = 11\n", 2, "0 to 10"),
            ("ozbound-settings 1\nvolume = -1\n", 2, "whole number"),
            ("ozbound-settings 1\nvolume = loud\n", 2, "whole number"),
            ("ozbound-settings 1\nambient = maybe\n", 2, "on or off"),
            ("ozbound-settings 1\n\ntext_size = huge\n", 3, "normal, large, larger"),
            ("ozbound-settings 1\nshow_perf = 1\n", 2, "on or off"),
            ("ozbound-settings 1\nvolume 5\n", 2, "key = value"),
            ("ozbound-settings 1\nbrightness = 3\n", 2, "unknown setting"),
            ("ozbound-settings 1\nvolume = 3\nvolume = 4\n", 3, "more than once"),
            ("# note\nvolume = 3\n", 2, "not a settings file"),
        ];
        for (text, line, words) in cases {
            let error = Settings::from_text(text).expect_err(text);
            assert_eq!(error.line, Some(line), "{text:?}: {error}");
            assert!(error.message.contains(words), "{text:?}: {error}");
        }
    }

    #[test]
    fn unknown_key_errors_list_the_valid_ones() {
        let error = Settings::from_text("ozbound-settings 1\nvolum = 3\n").unwrap_err();
        for key in KEYS {
            assert!(error.message.contains(key), "{error}");
        }
    }

    #[test]
    fn headers_are_checked() {
        assert!(Settings::from_text("").unwrap_err().message.contains("empty"));
        assert!(Settings::from_text("# only a comment\n").unwrap_err().message.contains("empty"));
        for bad in ["ozbound-settings", "ozbound-settings x", "ozbound-settings 0", "ozbound-settings 1 extra", "ozbound-save 1"] {
            let error = Settings::from_text(bad).unwrap_err();
            assert_eq!(error.line, Some(1), "{bad}: {error}");
        }
        let newer = Settings::from_text("ozbound-settings 2\nvolume = 3\n").unwrap_err();
        assert!(newer.message.contains("newer version"), "{newer}");
    }

    #[test]
    fn a_bad_file_never_loads_partly() {
        let text = "ozbound-settings 1\nvolume = 2\nambient = off\ntext_size = huge\n";
        assert!(Settings::from_text(text).is_err());
    }

    #[test]
    fn version_one_sample_keeps_loading() {
        // A saved sample of format 1: if this stops loading, the format changed without a bump.
        let sample = "ozbound-settings 1\nvolume = 9\nambient = on\ntext_size = large\nshow_perf = on\n";
        let loaded = Settings::from_text(sample).unwrap();
        assert_eq!(loaded.volume(), 9);
        assert_eq!(loaded.text_size, TextSize::Large);
        assert!(loaded.ambient && loaded.show_perf);
    }
}
