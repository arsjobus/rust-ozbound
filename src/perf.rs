//! Frame-time statistics for the performance overlay (F3) and the exit summary. Pure Rust: the
//! Bevy layer feeds in each frame's real (unpaused) duration and shows the text.
//!
//! [`FrameStats`] keeps a rolling window of recent frames for the live numbers and running
//! totals for the whole session, so a hitch that has already scrolled out of the window still
//! shows in the summary.

use std::collections::VecDeque;

/// A frame this long or longer is "slow" (below 30 fps).
pub const SLOW_FRAME_MS: f32 = 1000.0 / 30.0;
/// The frame budget for 60 fps.
pub const BUDGET_MS: f32 = 1000.0 / 60.0;
/// How many recent frames the live numbers cover (about two seconds at 60 fps).
pub const WINDOW: usize = 120;
/// Frames longer than this are not timings but stalls (the machine slept, a drag froze the
/// window); they are counted as one slow frame at this length so they cannot ruin the average.
pub const STALL_MS: f32 = 1000.0;

/// How the recent frames feel.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Rating {
    /// Averaging within the 60 fps budget.
    Smooth,
    /// Between 30 and 60 fps.
    Fair,
    /// Below 30 fps.
    Poor,
}

impl Rating {
    pub fn label(self) -> &'static str {
        match self {
            Self::Smooth => "smooth",
            Self::Fair => "fair",
            Self::Poor => "poor",
        }
    }
}

#[derive(Debug, Clone)]
pub struct FrameStats {
    recent: VecDeque<f32>,
    capacity: usize,
    frames: u64,
    slow_frames: u64,
    total_ms: f64,
    worst_ms: f32,
}

impl Default for FrameStats {
    fn default() -> Self {
        Self::new(WINDOW)
    }
}

impl FrameStats {
    /// Stats over a window of `window` recent frames (at least 1).
    pub fn new(window: usize) -> Self {
        let capacity = window.max(1);
        Self { recent: VecDeque::with_capacity(capacity), capacity, frames: 0, slow_frames: 0, total_ms: 0.0, worst_ms: 0.0 }
    }

    /// Records one frame that took `seconds`. Zero, negative, and non-finite durations are not
    /// frames and are ignored; stalls are capped at [`STALL_MS`].
    pub fn record(&mut self, seconds: f32) {
        if !seconds.is_finite() || seconds <= 0.0 {
            return;
        }
        let ms = (seconds * 1000.0).min(STALL_MS);
        if self.recent.len() == self.capacity {
            self.recent.pop_front();
        }
        self.recent.push_back(ms);
        self.frames += 1;
        self.total_ms += f64::from(ms);
        self.worst_ms = self.worst_ms.max(ms);
        if ms >= SLOW_FRAME_MS {
            self.slow_frames += 1;
        }
    }

    /// Frames recorded this session.
    pub fn frames(&self) -> u64 {
        self.frames
    }

    /// Slow frames (30 fps or worse) this session.
    pub fn slow_frames(&self) -> u64 {
        self.slow_frames
    }

    /// The slowest frame this session, in milliseconds.
    pub fn worst_ms(&self) -> f32 {
        self.worst_ms
    }

    /// Average frame time over the recent window.
    pub fn recent_avg_ms(&self) -> Option<f32> {
        (!self.recent.is_empty()).then(|| self.recent.iter().sum::<f32>() / self.recent.len() as f32)
    }

    /// Frames per second over the recent window.
    pub fn recent_fps(&self) -> Option<f32> {
        self.recent_avg_ms().map(|ms| 1000.0 / ms)
    }

    /// The slowest frame in the recent window.
    pub fn recent_worst_ms(&self) -> Option<f32> {
        self.recent.iter().copied().reduce(f32::max)
    }

    /// The frame time that `percent` percent of recent frames are at or under (nearest rank).
    /// `percentile_ms(99.0)` is the "1% low" frame time.
    pub fn recent_percentile_ms(&self, percent: f32) -> Option<f32> {
        if self.recent.is_empty() || !percent.is_finite() {
            return None;
        }
        let mut sorted: Vec<f32> = self.recent.iter().copied().collect();
        sorted.sort_by(f32::total_cmp);
        let rank = ((percent.clamp(0.0, 100.0) / 100.0) * sorted.len() as f32).ceil() as usize;
        Some(sorted[rank.clamp(1, sorted.len()) - 1])
    }

    /// How the recent frames rate, by their average.
    pub fn rating(&self) -> Option<Rating> {
        self.recent_avg_ms().map(|ms| {
            if ms <= BUDGET_MS * 1.05 {
                Rating::Smooth
            } else if ms <= SLOW_FRAME_MS {
                Rating::Fair
            } else {
                Rating::Poor
            }
        })
    }

    /// The overlay text, for example `60 fps (16.7 ms avg, 1% low 18.2 ms, worst 21.0 ms) smooth`.
    pub fn overlay_text(&self) -> String {
        match (self.recent_fps(), self.recent_avg_ms(), self.recent_percentile_ms(99.0), self.recent_worst_ms(), self.rating()) {
            (Some(fps), Some(avg), Some(low), Some(worst), Some(rating)) => {
                format!("{fps:.0} fps ({avg:.1} ms avg, 1% low {low:.1} ms, worst {worst:.1} ms) {}", rating.label())
            }
            _ => "measuring...".to_string(),
        }
    }

    /// The summary printed when the game exits, or `None` if no frame was recorded.
    pub fn summary(&self) -> Option<String> {
        if self.frames == 0 {
            return None;
        }
        let average = self.total_ms / self.frames as f64;
        let share = self.slow_frames as f64 * 100.0 / self.frames as f64;
        Some(format!(
            "performance: {} frames, {average:.1} ms average ({:.0} fps), worst {:.1} ms, {} slow frames ({share:.1}%)",
            self.frames,
            1000.0 / average,
            self.worst_ms,
            self.slow_frames,
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fed(frames: &[f32]) -> FrameStats {
        let mut stats = FrameStats::default();
        for seconds in frames {
            stats.record(*seconds);
        }
        stats
    }

    fn near(a: f32, b: f32) -> bool {
        (a - b).abs() < 0.01
    }

    #[test]
    fn nothing_is_known_before_the_first_frame() {
        let stats = FrameStats::default();
        assert_eq!(stats.frames(), 0);
        assert!(stats.recent_avg_ms().is_none() && stats.recent_fps().is_none() && stats.recent_worst_ms().is_none());
        assert!(stats.recent_percentile_ms(99.0).is_none() && stats.rating().is_none() && stats.summary().is_none());
        assert_eq!(stats.overlay_text(), "measuring...");
    }

    #[test]
    fn steady_sixty_fps_is_smooth() {
        let stats = fed(&[1.0 / 60.0; 100]);
        assert!(near(stats.recent_avg_ms().unwrap(), 16.667));
        assert!(near(stats.recent_fps().unwrap(), 60.0));
        assert_eq!(stats.rating(), Some(Rating::Smooth));
        assert_eq!(stats.slow_frames(), 0);
        assert!(stats.overlay_text().starts_with("60 fps (16.7 ms avg"), "{}", stats.overlay_text());
    }

    #[test]
    fn ratings_follow_the_frame_budget() {
        assert_eq!(fed(&[0.020; 50]).rating(), Some(Rating::Fair), "50 fps");
        assert_eq!(fed(&[0.033; 50]).rating(), Some(Rating::Fair), "30 fps is still fair");
        assert_eq!(fed(&[0.040; 50]).rating(), Some(Rating::Poor), "25 fps");
        assert_eq!(fed(&[0.008; 50]).rating(), Some(Rating::Smooth), "125 fps");
    }

    #[test]
    fn bad_durations_are_not_frames() {
        let mut stats = FrameStats::default();
        for bad in [0.0, -1.0, f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
            stats.record(bad);
        }
        assert_eq!(stats.frames(), 0);
        stats.record(0.016);
        assert_eq!(stats.frames(), 1);
    }

    #[test]
    fn the_window_forgets_old_frames_but_the_session_does_not() {
        let mut stats = FrameStats::new(4);
        stats.record(0.200); // a hitch
        for _ in 0..4 {
            stats.record(0.010);
        }
        assert!(near(stats.recent_worst_ms().unwrap(), 10.0), "the hitch has left the window");
        assert!(near(stats.worst_ms(), 200.0), "but the session remembers it");
        assert_eq!(stats.frames(), 5);
        assert_eq!(stats.slow_frames(), 1);
        assert!(stats.summary().unwrap().contains("worst 200.0 ms"));
    }

    #[test]
    fn a_window_of_zero_still_keeps_one_frame() {
        let mut stats = FrameStats::new(0);
        stats.record(0.01);
        stats.record(0.02);
        assert!(near(stats.recent_avg_ms().unwrap(), 20.0));
    }

    #[test]
    fn stalls_are_capped_so_they_cannot_wreck_the_numbers() {
        let mut stats = FrameStats::default();
        stats.record(3600.0);
        assert!(near(stats.worst_ms(), STALL_MS));
        assert_eq!(stats.slow_frames(), 1);
        assert!(stats.recent_avg_ms().unwrap().is_finite());
    }

    #[test]
    fn percentiles_use_nearest_rank_and_clamp() {
        let frames: Vec<f32> = (1..=100).map(|ms| ms as f32 / 1000.0).collect();
        let stats = fed(&frames);
        assert!(near(stats.recent_percentile_ms(50.0).unwrap(), 50.0));
        assert!(near(stats.recent_percentile_ms(99.0).unwrap(), 99.0));
        assert!(near(stats.recent_percentile_ms(100.0).unwrap(), 100.0));
        assert!(near(stats.recent_percentile_ms(0.0).unwrap(), 1.0), "never below the fastest frame");
        assert!(near(stats.recent_percentile_ms(250.0).unwrap(), 100.0), "clamped to 100");
        assert!(stats.recent_percentile_ms(f32::NAN).is_none());
        assert!(stats.recent_percentile_ms(99.0).unwrap() <= stats.recent_worst_ms().unwrap());
    }

    #[test]
    fn one_hitch_shows_in_the_one_percent_low_but_barely_in_the_average() {
        let mut frames = vec![1.0 / 60.0; 99];
        frames.push(0.120);
        let stats = fed(&frames);
        assert!(stats.recent_avg_ms().unwrap() < 19.0);
        assert!(near(stats.recent_worst_ms().unwrap(), 120.0));
        assert_eq!(stats.rating(), Some(Rating::Fair), "an average just over budget is fair, not smooth");
    }

    #[test]
    fn the_summary_reports_totals_and_the_slow_share() {
        let mut stats = FrameStats::default();
        for _ in 0..90 {
            stats.record(0.010);
        }
        for _ in 0..10 {
            stats.record(0.050);
        }
        let summary = stats.summary().unwrap();
        assert!(summary.contains("100 frames"), "{summary}");
        assert!(summary.contains("14.0 ms average"), "{summary}");
        assert!(summary.contains("10 slow frames (10.0%)"), "{summary}");
        assert!(summary.contains("worst 50.0 ms"), "{summary}");
    }

    #[test]
    fn overlay_text_names_every_number() {
        let text = fed(&[0.020; 10]).overlay_text();
        for part in ["50 fps", "20.0 ms avg", "1% low 20.0 ms", "worst 20.0 ms", "fair"] {
            assert!(text.contains(part), "{text}");
        }
    }
}
