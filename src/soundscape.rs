//! Zone music that fades in and out as the player crosses zone boundaries.
//!
//! Pure Rust (no Bevy). A [`Mixer`] keeps one *voice* per track that is audible. The game says
//! which track the player's zone wants ([`Mixer::set_target`]) and calls [`Mixer::tick`] every
//! frame with the elapsed time; the mixer answers with which tracks to start, the gain of
//! every playing track, and which tracks have faded out completely and can be stopped.
//!
//! Rules:
//!
//! * The wanted track fades in over `fade_seconds` while every other track fades out over the
//!   same time, so crossing a boundary is a crossfade. Gains follow a quarter-sine curve of the
//!   fade progress, so two tracks crossfading keep roughly the same loudness (equal power)
//!   instead of dipping in the middle.
//! * Asking for the track that is already wanted changes nothing: it never restarts.
//! * Turning back mid-fade is smooth: each voice moves from where it is, up or down.
//! * A voice that has faded to nothing and is not wanted is dropped, and its key is returned
//!   once in [`Frame::stop`].
//! * A mixer made with [`Mixer::starting_with`] begins at full volume (the first track of a
//!   game does not fade in from silence).
//! * A bad time step (negative, NaN, infinite) is ignored. A fade time of zero or less means an
//!   instant swap.

use std::f32::consts::FRAC_PI_2;

/// How long, in seconds, a zone's music takes to fade in or out.
pub const FADE_SECONDS: f32 = 2.5;

#[derive(Debug, Clone, PartialEq)]
struct Voice<K> {
    key: K,
    /// Fade progress, 0 (silent) to 1 (full).
    level: f32,
    announced: bool,
}

/// What to do after a [`Mixer::tick`].
#[derive(Debug, Clone, PartialEq)]
pub struct Frame<K> {
    /// Tracks that became audible this tick and are not playing yet: start them (at the gain
    /// listed for them in `gains`).
    pub start: Vec<K>,
    /// The gain, 0 to 1, of every track that should be playing, including the new ones.
    pub gains: Vec<(K, f32)>,
    /// Tracks that have finished fading out: stop and remove them.
    pub stop: Vec<K>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Mixer<K> {
    voices: Vec<Voice<K>>,
    target: Option<K>,
    fade_seconds: f32,
}

/// The loudness for a fade progress (equal-power curve).
pub fn gain_at(level: f32) -> f32 {
    (level.clamp(0.0, 1.0) * FRAC_PI_2).sin()
}

impl<K: Clone + PartialEq> Mixer<K> {
    /// A silent mixer that wants nothing yet.
    pub fn new(fade_seconds: f32) -> Self {
        Self { voices: Vec::new(), target: None, fade_seconds }
    }

    /// A mixer that already plays `key` at full volume.
    pub fn starting_with(key: K, fade_seconds: f32) -> Self {
        Self { voices: vec![Voice { key: key.clone(), level: 1.0, announced: false }], target: Some(key), fade_seconds }
    }

    /// The track that should be playing now (`None` for silence).
    pub fn target(&self) -> Option<&K> {
        self.target.as_ref()
    }

    /// Changes the wanted track. The same track again changes nothing.
    pub fn set_target(&mut self, key: Option<K>) {
        if self.target == key {
            return;
        }
        if let Some(key) = &key {
            if !self.voices.iter().any(|v| v.key == *key) {
                self.voices.push(Voice { key: key.clone(), level: 0.0, announced: false });
            }
        }
        self.target = key;
    }

    /// The current gain of a track (0 when it is not playing).
    pub fn gain_of(&self, key: &K) -> f32 {
        self.voices.iter().find(|v| v.key == *key).map_or(0.0, |v| gain_at(v.level))
    }

    /// Number of tracks currently audible or fading.
    pub fn voice_count(&self) -> usize {
        self.voices.len()
    }

    /// Moves every fade forward by `dt` seconds.
    pub fn tick(&mut self, dt: f32) -> Frame<K> {
        if dt.is_finite() && dt > 0.0 {
            let step = if self.fade_seconds > 0.0 { dt / self.fade_seconds } else { 1.0 };
            for voice in &mut self.voices {
                let wanted = self.target.as_ref() == Some(&voice.key);
                voice.level = if wanted { (voice.level + step).min(1.0) } else { (voice.level - step).max(0.0) };
            }
        }
        let target = self.target.clone();
        let mut stop = Vec::new();
        self.voices.retain(|voice| {
            let keep = voice.level > 0.0 || target.as_ref() == Some(&voice.key);
            if !keep && voice.announced {
                stop.push(voice.key.clone());
            }
            keep
        });
        let mut start = Vec::new();
        for voice in &mut self.voices {
            if !voice.announced {
                voice.announced = true;
                start.push(voice.key.clone());
            }
        }
        let gains = self.voices.iter().map(|v| (v.key.clone(), gain_at(v.level))).collect();
        Frame { start, gains, stop }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const DT: f32 = 0.1;

    fn run(mixer: &mut Mixer<&'static str>, seconds: f32) -> Frame<&'static str> {
        let mut last = mixer.tick(0.0);
        let mut started = last.start.clone();
        let mut stopped = last.stop.clone();
        for _ in 0..(seconds / DT).round() as usize {
            last = mixer.tick(DT);
            started.extend(last.start.clone());
            stopped.extend(last.stop.clone());
        }
        last.start = started;
        last.stop = stopped;
        last
    }

    #[test]
    fn a_new_mixer_is_silent() {
        let mut mixer: Mixer<&str> = Mixer::new(2.0);
        let frame = mixer.tick(1.0);
        assert!(frame.gains.is_empty() && frame.start.is_empty() && frame.stop.is_empty());
    }

    #[test]
    fn the_first_track_of_a_game_starts_at_full_volume() {
        let mut mixer = Mixer::starting_with("a", 2.0);
        let frame = mixer.tick(0.0);
        assert_eq!(frame.start, ["a"]);
        assert_eq!(frame.gains, [("a", 1.0)]);
    }

    #[test]
    fn a_new_target_starts_silent_then_fades_in() {
        let mut mixer: Mixer<&str> = Mixer::new(2.0);
        mixer.set_target(Some("a"));
        let first = mixer.tick(0.0);
        assert_eq!(first.start, ["a"]);
        assert_eq!(first.gains, [("a", 0.0)]);
        let mut previous = 0.0;
        for _ in 0..20 {
            let gain = mixer.tick(DT).gains[0].1;
            assert!(gain >= previous, "never gets quieter while fading in");
            previous = gain;
        }
        assert!((previous - 1.0).abs() < 1e-5, "full volume after the fade time");
    }

    #[test]
    fn crossing_a_boundary_is_a_crossfade_that_ends_with_the_old_track_stopped() {
        let mut mixer = Mixer::starting_with("a", 2.0);
        mixer.tick(0.0);
        mixer.set_target(Some("b"));
        let mid = run(&mut mixer, 1.0);
        let a = mixer.gain_of(&"a");
        let b = mixer.gain_of(&"b");
        assert!(a > 0.0 && a < 1.0 && b > 0.0 && b < 1.0, "both audible half-way: {a} {b}");
        assert!((a * a + b * b - 1.0).abs() < 1e-4, "equal power: {a} {b}");
        assert_eq!(mid.start, ["b"]);
        let end = run(&mut mixer, 1.5);
        assert_eq!(end.stop, ["a"], "the old track is told to stop exactly once");
        assert_eq!(end.gains, [("b", 1.0)]);
        assert_eq!(mixer.voice_count(), 1);
    }

    #[test]
    fn the_same_target_again_does_not_restart_anything() {
        let mut mixer = Mixer::starting_with("a", 2.0);
        mixer.tick(0.0);
        mixer.set_target(Some("a"));
        let frame = mixer.tick(DT);
        assert!(frame.start.is_empty() && frame.stop.is_empty());
        assert_eq!(frame.gains, [("a", 1.0)]);
    }

    #[test]
    fn turning_back_mid_fade_moves_each_voice_smoothly_from_where_it_is() {
        let mut mixer = Mixer::starting_with("a", 2.0);
        mixer.tick(0.0);
        mixer.set_target(Some("b"));
        run(&mut mixer, 1.0);
        let (a_before, b_before) = (mixer.gain_of(&"a"), mixer.gain_of(&"b"));
        mixer.set_target(Some("a"));
        mixer.tick(DT);
        assert!(mixer.gain_of(&"a") > a_before, "a recovers from where it was");
        assert!(mixer.gain_of(&"b") < b_before, "b fades back out from where it was");
        let end = run(&mut mixer, 3.0);
        assert_eq!(end.gains, [("a", 1.0)]);
        assert!(end.stop.contains(&"b"));
        assert!(end.start.is_empty(), "a was never stopped, so it is never started again");
    }

    #[test]
    fn a_silent_zone_fades_everything_out() {
        let mut mixer = Mixer::starting_with("a", 1.0);
        mixer.tick(0.0);
        mixer.set_target(None);
        let end = run(&mut mixer, 1.5);
        assert_eq!(end.stop, ["a"]);
        assert!(end.gains.is_empty());
        assert_eq!(mixer.target(), None);
    }

    #[test]
    fn bad_time_steps_are_ignored() {
        let mut mixer = Mixer::starting_with("a", 2.0);
        mixer.tick(0.0);
        mixer.set_target(Some("b"));
        mixer.tick(0.0);
        for dt in [-1.0, f32::NAN, f32::INFINITY, f32::NEG_INFINITY, 0.0] {
            let frame = mixer.tick(dt);
            assert_eq!(frame.gains, [("a", 1.0), ("b", 0.0)], "dt = {dt}");
        }
    }

    #[test]
    fn a_zero_fade_time_swaps_at_once() {
        let mut mixer = Mixer::starting_with("a", 0.0);
        mixer.tick(0.0);
        mixer.set_target(Some("b"));
        let frame = mixer.tick(0.016);
        assert_eq!(frame.gains, [("b", 1.0)]);
        assert_eq!(frame.stop, ["a"]);
    }

    #[test]
    fn a_track_that_was_never_audible_is_not_reported_as_stopped() {
        let mut mixer: Mixer<&str> = Mixer::new(2.0);
        mixer.set_target(Some("a"));
        mixer.set_target(Some("b"));
        let frame = mixer.tick(DT);
        assert!(frame.stop.is_empty(), "a never got a tick as the target, so nothing was started");
        assert!(frame.start.contains(&"b"));
    }

    #[test]
    fn gains_stay_between_zero_and_one_through_a_busy_walk() {
        let mut mixer = Mixer::new(2.0);
        let route = ["a", "b", "a", "c", "c", "b"];
        for (i, key) in route.into_iter().enumerate() {
            mixer.set_target(Some(key));
            for _ in 0..(5 + i * 3) {
                for (_, gain) in mixer.tick(DT).gains {
                    assert!((0.0..=1.0).contains(&gain));
                }
            }
            assert!(mixer.voice_count() <= 3, "old tracks do not pile up");
        }
    }

    #[test]
    fn the_curve_is_silent_at_zero_full_at_one_and_clamped() {
        assert_eq!(gain_at(0.0), 0.0);
        assert!((gain_at(1.0) - 1.0).abs() < 1e-6);
        assert_eq!(gain_at(-3.0), 0.0);
        assert!((gain_at(9.0) - 1.0).abs() < 1e-6);
        assert!(gain_at(0.5) > 0.5, "louder than a straight line at the middle (equal power)");
    }
}
