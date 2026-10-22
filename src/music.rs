//! Background music: one theme per area, played from the `bgm` folder.
//!
//! Pure Rust (no Bevy). Each region in `assets/world/locations.world` may name a track
//! (`music = bluebrook.ogg`) and a built-in theme (`theme = village`). [`choose`] picks what
//! to play, in this order:
//!
//! 1. the named file in the `bgm` folder (`assets/bgm`), if it exists and looks like audio
//!    in one of the supported formats: **ogg**, **wav**, or **mp3**;
//! 2. the area's built-in theme, a short loop generated in code (so the game ships no audio
//!    files and a missing track is never silence);
//! 3. silence, if the area names neither.
//!
//! A missing file is not an error. The reason is returned as a note for the console. The
//! file's first bytes are checked against the format's signature, because Bevy's audio
//! decoder panics on bytes it cannot decode: a stray text file renamed `theme.ogg` must be
//! refused here with a note instead of taking the game down.

use std::f32::consts::TAU;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};

use crate::ambient::{wav_bytes, SAMPLE_RATE};

/// The folder music files are read from, relative to the working directory.
pub const BGM_DIR: &str = "assets/bgm";

/// Supported file extensions (compared without regard to case).
pub const FORMATS: [&str; 3] = ["ogg", "wav", "mp3"];

/// Built-in theme ids.
pub const THEMES: [&str; 2] = ["village", "road"];

/// Refuse files larger than this. The whole file is read into memory, so a long recording
/// must be shortened or compressed (about one minute of CD-quality WAV is 10 MB).
pub const MAX_TRACK_BYTES: u64 = 64 * 1024 * 1024;

/// The loudest sample a built-in theme produces, out of `i16::MAX`. Music sits under the
/// ambient sound's character, not over it.
pub const THEME_PEAK: i16 = 7_000;

/// True for a plain file name (no folders, no leading dot) with a supported extension.
pub fn is_valid_track_name(name: &str) -> bool {
    if name.is_empty() || name.starts_with('.') || name.contains(['/', '\\', ':', '\0']) {
        return false;
    }
    let Some((stem, extension)) = name.rsplit_once('.') else { return false };
    !stem.is_empty() && FORMATS.iter().any(|f| extension.eq_ignore_ascii_case(f))
}

/// The format a track name claims, lower-case (`ogg`, `wav`, or `mp3`).
pub fn format_of(name: &str) -> Option<&'static str> {
    let extension = name.rsplit_once('.')?.1;
    FORMATS.iter().copied().find(|f| extension.eq_ignore_ascii_case(f))
}

/// True when `bytes` start the way a file of `format` does: `RIFF....WAVE` for wav, `OggS`
/// for ogg, and for mp3 either an `ID3` tag (what most encoders write first) or an MPEG audio
/// frame header (see [`is_mpeg_frame_header`]).
pub fn looks_like(format: &str, bytes: &[u8]) -> bool {
    match format {
        "wav" => bytes.len() >= 12 && &bytes[0..4] == b"RIFF" && &bytes[8..12] == b"WAVE",
        "ogg" => bytes.len() >= 4 && &bytes[0..4] == b"OggS",
        "mp3" => (bytes.len() >= 10 && &bytes[0..3] == b"ID3") || is_mpeg_frame_header(bytes),
        _ => false,
    }
}

/// True when the first four bytes are a plausible MPEG-1/2/2.5 Layer III frame header: eleven
/// set sync bits, a defined version, layer III, and bitrate and sample-rate fields that are
/// not the reserved values. A bare AAC (ADTS) stream also starts with sync bits but names
/// layer 0, so it is not taken for an mp3.
pub fn is_mpeg_frame_header(bytes: &[u8]) -> bool {
    if bytes.len() < 4 || bytes[0] != 0xFF || bytes[1] & 0xE0 != 0xE0 {
        return false;
    }
    let version = (bytes[1] >> 3) & 0b11;
    let layer = (bytes[1] >> 1) & 0b11;
    let bitrate = bytes[2] >> 4;
    let sample_rate = (bytes[2] >> 2) & 0b11;
    version != 0b01 && layer == 0b01 && bitrate != 0b1111 && sample_rate != 0b11
}

/// What to play for an area.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Track {
    /// A file from the bgm folder. `bytes` is the whole file.
    File { name: String, bytes: Vec<u8> },
    /// A generated theme as a WAV file.
    Builtin { theme: String, bytes: Vec<u8> },
    Silence,
}

impl Track {
    /// Short description for logs and tests.
    pub fn label(&self) -> String {
        match self {
            Track::File { name, .. } => format!("file {name}"),
            Track::Builtin { theme, .. } => format!("built-in {theme} theme"),
            Track::Silence => "silence".to_string(),
        }
    }
}

/// The result of [`choose`]: the track, and why a requested file was not used (if it was not).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Choice {
    pub track: Track,
    pub note: Option<String>,
}

/// Picks the music for an area. See the module docs for the order.
pub fn choose(dir: &Path, file: Option<&str>, theme: Option<&str>) -> Choice {
    let mut note = None;
    if let Some(name) = file {
        match read_track(dir, name) {
            Ok(bytes) => return Choice { track: Track::File { name: name.to_string(), bytes }, note: None },
            Err(reason) => note = Some(reason),
        }
    }
    if let Some(id) = theme {
        if let Some(bytes) = theme_wav(id) {
            return Choice { track: Track::Builtin { theme: id.to_string(), bytes }, note };
        }
        let unknown = format!("unknown built-in theme `{id}` (use {})", THEMES.join(" or "));
        note = Some(match note {
            Some(first) => format!("{first}; {unknown}"),
            None => unknown,
        });
    }
    Choice { track: Track::Silence, note }
}

fn read_track(dir: &Path, name: &str) -> Result<Vec<u8>, String> {
    let Some(format) = format_of(name).filter(|_| is_valid_track_name(name)) else {
        return Err(format!("`{name}` is not a valid track name (a plain file name ending in .ogg, .wav, or .mp3)"));
    };
    let path: PathBuf = dir.join(name);
    let length = match std::fs::metadata(&path) {
        Ok(meta) => meta.len(),
        Err(e) if e.kind() == ErrorKind::NotFound => {
            return Err(format!("{} was not found; using the built-in theme instead", path.display()));
        }
        Err(e) => return Err(format!("could not read {}: {e}", path.display())),
    };
    if length > MAX_TRACK_BYTES {
        return Err(format!("{} is {length} bytes; the limit is {MAX_TRACK_BYTES}", path.display()));
    }
    let bytes = std::fs::read(&path).map_err(|e| format!("could not read {}: {e}", path.display()))?;
    if !looks_like(format, &bytes) {
        return Err(format!("{} does not look like a .{format} file (wrong first bytes); not playing it", path.display()));
    }
    Ok(bytes)
}

/// One note of a built-in theme: `degree` indexes the theme's scale (negative values are
/// below it), `start` and `length` are in beats.
struct Note {
    degree: i32,
    start: f32,
    length: f32,
}

struct Theme {
    bpm: f32,
    beats: f32,
    /// Frequency of scale degree 0, and the scale's semitone steps above it.
    root_hz: f32,
    scale: &'static [i32],
    notes: Vec<Note>,
}

fn theme_named(id: &str) -> Option<Theme> {
    match id {
        // Warm and unhurried: a major-pentatonic tune over a slow arpeggio. 16 beats at 84 bpm.
        "village" => {
            let mut notes = Vec::new();
            // Arpeggio: one soft note every beat, climbing and falling on the scale.
            for (beat, degree) in [0, 2, 4, 2, 0, 2, 5, 4, 0, 2, 4, 7, 5, 4, 2, 1].into_iter().enumerate() {
                notes.push(Note { degree: degree - 5, start: beat as f32, length: 1.6 });
            }
            // Melody: long notes above, resting for the last beats so the loop joins cleanly.
            for (start, length, degree) in [(0.0, 2.0, 7), (2.0, 1.0, 9), (3.0, 1.0, 7), (4.0, 3.0, 10), (8.0, 2.0, 9), (10.0, 1.0, 7), (11.0, 2.5, 5)] {
                notes.push(Note { degree, start, length });
            }
            Some(Theme { bpm: 84.0, beats: 16.0, root_hz: 261.63, scale: &[0, 2, 4, 7, 9], notes })
        }
        // Open and a little wistful: a walking bass of fifths with a sparse melody. 16 beats at 66 bpm.
        "road" => {
            let mut notes = Vec::new();
            for (beat, degree) in [0, 4, 0, 4, 2, 4, 2, 4].into_iter().enumerate() {
                notes.push(Note { degree: degree - 10, start: beat as f32 * 2.0, length: 2.2 });
            }
            for (start, length, degree) in [(0.0, 3.0, 5), (4.0, 1.0, 6), (5.0, 1.0, 5), (6.0, 2.0, 3), (8.0, 3.0, 4), (12.0, 1.0, 3), (13.0, 2.0, 1)] {
                notes.push(Note { degree, start, length });
            }
            Some(Theme { bpm: 66.0, beats: 16.0, root_hz: 293.66, scale: &[0, 2, 4, 5, 7, 9, 10], notes })
        }
        _ => None,
    }
}

fn frequency(theme: &Theme, degree: i32) -> f32 {
    let steps = theme.scale.len() as i32;
    let octave = degree.div_euclid(steps);
    let within = degree.rem_euclid(steps) as usize;
    let semitones = octave * 12 + theme.scale[within];
    theme.root_hz * 2f32.powf(semitones as f32 / 12.0)
}

/// Samples of a built-in theme (one loop), each within `[-THEME_PEAK, THEME_PEAK]`, or `None`
/// for an unknown id. Deterministic: the same on every run.
pub fn theme_samples(id: &str) -> Option<Vec<i16>> {
    let theme = theme_named(id)?;
    let seconds = theme.beats * 60.0 / theme.bpm;
    let total = (seconds * SAMPLE_RATE as f32) as usize;
    let mut mix = vec![0.0f32; total];
    for note in &theme.notes {
        let hz = frequency(&theme, note.degree);
        let first = (note.start * 60.0 / theme.bpm * SAMPLE_RATE as f32) as usize;
        let count = (note.length * 60.0 / theme.bpm * SAMPLE_RATE as f32) as usize;
        for k in 0..count {
            let i = first + k;
            if i >= total {
                break;
            }
            let t = k as f32 / SAMPLE_RATE as f32;
            // A soft pluck: quick attack, exponential decay, and a touch of upper harmonics.
            let envelope = (t * 90.0).min(1.0) * (-t * 2.6).exp();
            let wave = (TAU * hz * t).sin() + 0.28 * (TAU * 2.0 * hz * t).sin() + 0.09 * (TAU * 3.0 * hz * t).sin();
            mix[i] += wave * envelope;
        }
    }
    // Normalise so the loudest moment sits at THEME_PEAK, and fade the very ends.
    let loudest = mix.iter().fold(0.0f32, |m, v| m.max(v.abs())).max(1e-6);
    let fade = (0.05 * SAMPLE_RATE as f32) as usize;
    Some(
        mix.iter()
            .enumerate()
            .map(|(i, v)| {
                let edge = i.min(total - 1 - i);
                let gain = if edge < fade { edge as f32 / fade as f32 } else { 1.0 };
                (v / loudest * gain * f32::from(THEME_PEAK)) as i16
            })
            .collect(),
    )
}

/// A built-in theme as a complete WAV file, or `None` for an unknown id.
pub fn theme_wav(id: &str) -> Option<Vec<u8>> {
    theme_samples(id).map(|samples| wav_bytes(&samples))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU32, Ordering};

    /// A fresh empty temp folder per test, removed when dropped.
    struct TempDir(PathBuf);
    impl TempDir {
        fn new() -> Self {
            static NEXT: AtomicU32 = AtomicU32::new(0);
            let dir = std::env::temp_dir().join(format!("ozbound-bgm-test-{}-{}", std::process::id(), NEXT.fetch_add(1, Ordering::SeqCst)));
            std::fs::create_dir_all(&dir).unwrap();
            Self(dir)
        }
        fn write(&self, name: &str, bytes: &[u8]) { std::fs::write(self.0.join(name), bytes).unwrap(); }
    }
    impl Drop for TempDir {
        fn drop(&mut self) { let _ = std::fs::remove_dir_all(&self.0); }
    }

    fn fake_wav() -> Vec<u8> { wav_bytes(&[0, 100, -100, 0]) }
    fn fake_ogg() -> Vec<u8> { b"OggS\0\x02rest-of-the-file".to_vec() }
    fn fake_mp3() -> Vec<u8> { b"ID3\x04\0\0\0\0\0\x22rest-of-the-file".to_vec() }
    /// A bare MPEG-1 Layer III frame header (128 kbit/s, 44.1 kHz), what an mp3 without a tag starts with.
    fn bare_mp3() -> Vec<u8> { vec![0xFF, 0xFB, 0x90, 0x64, 0, 0, 0, 0] }

    #[test]
    fn track_names_must_be_plain_files_in_a_supported_format() {
        for ok in ["theme.ogg", "Theme.OGG", "a.b.wav", "village-1.mp3", "x.WaV"] {
            assert!(is_valid_track_name(ok), "{ok}");
        }
        for bad in ["", "theme", "theme.mp4", ".ogg", ".hidden.ogg", "dir/theme.ogg", "dir\\theme.ogg", "../theme.ogg", "c:theme.ogg", "theme.ogg.txt", "theme.", "nul\0.ogg"] {
            assert!(!is_valid_track_name(bad), "{bad:?}");
        }
        assert_eq!(format_of("a.OGG"), Some("ogg"));
        assert_eq!(format_of("a.mp3"), Some("mp3"));
        assert_eq!(format_of("a.txt"), None);
        assert_eq!(format_of("noextension"), None);
    }

    #[test]
    fn signatures_are_checked_per_format() {
        assert!(looks_like("wav", &fake_wav()));
        assert!(looks_like("ogg", &fake_ogg()));
        assert!(looks_like("mp3", &fake_mp3()));
        assert!(!looks_like("wav", &fake_ogg()));
        assert!(!looks_like("ogg", &fake_wav()));
        assert!(!looks_like("mp3", &fake_wav()));
        assert!(!looks_like("ogg", b"Og"), "too short");
        assert!(!looks_like("wav", b"RIFF"), "too short");
        assert!(!looks_like("mp3", b""), "empty");
        assert!(!looks_like("mp3", b"ID3"), "an ID3 marker alone is too short to trust");
    }

    #[test]
    fn mp3_means_mp3_not_an_mp4_container() {
        // What encoders write: an ID3 tag first, or a bare MPEG Layer III frame.
        assert!(looks_like("mp3", &fake_mp3()));
        assert!(looks_like("mp3", &bare_mp3()));
        // An MPEG-2 and an MPEG-2.5 Layer III frame header are mp3 too.
        assert!(looks_like("mp3", &[0xFF, 0xF3, 0x90, 0x64]));
        assert!(looks_like("mp3", &[0xFF, 0xE3, 0x90, 0x64]));
        // Not mp3: an MP4/AAC container (`ftyp` box), a bare AAC stream (layer 0), a reserved
        // version, reserved bitrate and sample-rate fields, text, and wav.
        assert!(!looks_like("mp3", b"\0\0\0\x18ftypisom\0\0\0\0isomiso2"));
        assert!(!looks_like("mp3", &[0xFF, 0xF1, 0x50, 0x80]), "ADTS AAC");
        assert!(!looks_like("mp3", &[0xFF, 0xEB, 0x90, 0x64]), "reserved MPEG version");
        assert!(!looks_like("mp3", &[0xFF, 0xFB, 0xF0, 0x64]), "reserved bitrate");
        assert!(!looks_like("mp3", &[0xFF, 0xFB, 0x9C, 0x64]), "reserved sample rate");
        assert!(!looks_like("mp3", b"this is not audio at all"));
        assert!(!looks_like("mp3", &fake_wav()));
    }

    #[test]
    fn a_present_file_wins_over_the_builtin_theme() {
        let dir = TempDir::new();
        dir.write("bluebrook.ogg", &fake_ogg());
        let choice = choose(&dir.0, Some("bluebrook.ogg"), Some("village"));
        assert_eq!(choice.note, None);
        assert_eq!(choice.track, Track::File { name: "bluebrook.ogg".into(), bytes: fake_ogg() });
        // Each supported format is accepted.
        dir.write("a.wav", &fake_wav());
        dir.write("b.mp3", &fake_mp3());
        assert!(matches!(choose(&dir.0, Some("a.wav"), None).track, Track::File { .. }));
        assert!(matches!(choose(&dir.0, Some("b.mp3"), None).track, Track::File { .. }));
    }

    #[test]
    fn a_missing_file_falls_back_to_the_theme_with_a_note() {
        let dir = TempDir::new();
        let choice = choose(&dir.0, Some("nope.ogg"), Some("road"));
        assert!(matches!(&choice.track, Track::Builtin { theme, .. } if theme == "road"));
        let note = choice.note.expect("a note explains the fallback");
        assert!(note.contains("nope.ogg") && note.contains("not found"), "{note}");
        // A folder that does not exist at all behaves the same.
        let gone = dir.0.join("no-such-folder");
        assert!(matches!(choose(&gone, Some("a.ogg"), Some("village")).track, Track::Builtin { .. }));
    }

    #[test]
    fn a_file_that_is_not_audio_is_refused_not_played() {
        let dir = TempDir::new();
        dir.write("fake.ogg", b"this is plain text, not audio");
        dir.write("empty.wav", b"");
        dir.write("wrong.mp3", &fake_wav());
        for name in ["fake.ogg", "empty.wav", "wrong.mp3"] {
            let choice = choose(&dir.0, Some(name), Some("village"));
            assert!(matches!(choice.track, Track::Builtin { .. }), "{name}");
            assert!(choice.note.unwrap().contains("does not look like"), "{name}");
        }
        let choice = choose(&dir.0, Some("fake.ogg"), None);
        assert_eq!(choice.track, Track::Silence, "no theme to fall back on");
        assert!(choice.note.is_some());
    }

    #[test]
    fn bad_names_never_touch_the_filesystem() {
        let dir = TempDir::new();
        dir.write("secret.txt", b"RIFF....WAVE");
        for name in ["../secret.txt", "secret.txt", "sub/x.ogg", "x.mp4"] {
            let choice = choose(&dir.0, Some(name), None);
            assert_eq!(choice.track, Track::Silence, "{name}");
            assert!(choice.note.unwrap().contains("not a valid track name"), "{name}");
        }
    }

    #[test]
    fn nothing_requested_means_silence_and_an_unknown_theme_is_reported() {
        let dir = TempDir::new();
        assert_eq!(choose(&dir.0, None, None), Choice { track: Track::Silence, note: None });
        let choice = choose(&dir.0, None, Some("polka"));
        assert_eq!(choice.track, Track::Silence);
        assert!(choice.note.unwrap().contains("unknown built-in theme `polka`"));
        // Both problems are reported together.
        let both = choose(&dir.0, Some("gone.ogg"), Some("polka")).note.unwrap();
        assert!(both.contains("gone.ogg") && both.contains("polka"), "{both}");
    }

    #[test]
    fn a_folder_where_the_file_should_be_is_reported_not_crashed_on() {
        let dir = TempDir::new();
        std::fs::create_dir(dir.0.join("adir.ogg")).unwrap();
        let choice = choose(&dir.0, Some("adir.ogg"), Some("village"));
        assert!(matches!(choice.track, Track::Builtin { .. }));
        assert!(choice.note.is_some());
    }

    #[test]
    fn every_builtin_theme_is_a_valid_quiet_deterministic_loop() {
        for id in THEMES {
            let samples = theme_samples(id).expect(id);
            assert!(samples.len() > SAMPLE_RATE as usize * 10, "{id}: at least ten seconds");
            assert!(samples.len() < SAMPLE_RATE as usize * 60, "{id}: not a minute-long download");
            let loudest = samples.iter().map(|s| s.unsigned_abs()).max().unwrap();
            assert!(loudest > 3_000, "{id} is audible");
            assert!(loudest <= THEME_PEAK as u16, "{id} is quieter than clipping: {loudest}");
            assert_eq!(samples.first(), Some(&0), "{id}: starts silent so a repeat cannot click");
            assert_eq!(samples.last(), Some(&0), "{id}: ends silent");
            assert_eq!(samples, theme_samples(id).unwrap(), "{id}: deterministic");
            let wav = theme_wav(id).unwrap();
            assert!(looks_like("wav", &wav), "{id}: a real WAV header");
            assert_eq!(wav.len(), 44 + samples.len() * 2);
        }
    }

    #[test]
    fn the_themes_are_different_music() {
        let village = theme_samples("village").unwrap();
        let road = theme_samples("road").unwrap();
        assert_ne!(village.len(), road.len(), "different tempos give different lengths");
        let n = village.len().min(road.len());
        let differing = (0..n).filter(|i| village[*i] != road[*i]).count();
        assert!(differing > n / 2);
        assert_eq!(theme_samples("polka"), None);
        assert_eq!(theme_wav("polka"), None);
    }

    #[test]
    fn scale_degrees_climb_by_octaves_and_stay_in_tune() {
        let theme = theme_named("village").unwrap();
        let root = frequency(&theme, 0);
        assert!((root - 261.63).abs() < 0.01);
        // Five notes to the octave: degree 5 is exactly one octave up, degree -5 one down.
        assert!((frequency(&theme, 5) - 2.0 * root).abs() < 0.01);
        assert!((frequency(&theme, -5) - root / 2.0).abs() < 0.01);
        assert!(frequency(&theme, 1) > frequency(&theme, 0));
        assert!(frequency(&theme, -1) < frequency(&theme, 0));
    }

    #[test]
    fn track_labels_say_what_is_playing() {
        assert_eq!(Track::Silence.label(), "silence");
        assert_eq!(Track::File { name: "a.ogg".into(), bytes: vec![] }.label(), "file a.ogg");
        assert_eq!(Track::Builtin { theme: "road".into(), bytes: vec![] }.label(), "built-in road theme");
    }

    /// The point of the whole change: bytes that pass `looks_like` really play. Hands the shipped
    /// mp3 and a built-in theme to Bevy's own decoder (the one the game uses, which panics on
    /// anything it cannot read) and checks that real, non-silent samples come out.
    #[test]
    fn the_games_audio_decoder_plays_the_shipped_mp3_and_the_builtin_themes() {
        use bevy::audio::{AudioSource, Decodable};
        fn audible(bytes: Vec<u8>) -> usize {
            let source = AudioSource { bytes: bytes.into() };
            source.decoder().take(400_000).filter(|sample| *sample != 0).count()
        }
        let mp3 = std::fs::read(Path::new(BGM_DIR).join("bluebrook.mp3")).expect("assets/bgm/bluebrook.mp3 ships with the game");
        assert!(looks_like("mp3", &mp3));
        assert!(audible(mp3) > 1_000, "the mp3 decodes to sound");
        for id in THEMES {
            assert!(audible(theme_wav(id).unwrap()) > 1_000, "{id} theme decodes to sound");
        }
    }
}
