//! Countdown used to bring spent resource nodes back. Pure Rust: the Bevy layer feeds it
//! frame time and reacts when it reports that the wait is over.

/// A one-shot countdown. [`RespawnTimer::tick`] reports completion exactly once.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RespawnTimer {
    remaining: f32,
    fired: bool,
}

impl RespawnTimer {
    /// Starts a countdown of `seconds`. A negative or non-finite duration counts as zero,
    /// so a bad value can never leave a node spent forever.
    pub fn new(seconds: f32) -> Self {
        let remaining = if seconds.is_finite() && seconds > 0.0 { seconds } else { 0.0 };
        Self { remaining, fired: false }
    }

    /// Seconds left before the node returns.
    pub fn remaining(&self) -> f32 {
        self.remaining
    }

    /// Whether the completion has already been reported.
    pub fn is_finished(&self) -> bool {
        self.fired
    }

    /// Advances the countdown by `dt` seconds. Returns `true` on the one tick that reaches
    /// zero (or on the first tick of a zero-length countdown), and `false` otherwise.
    /// A negative or non-finite `dt` is ignored.
    pub fn tick(&mut self, dt: f32) -> bool {
        if self.fired {
            return false;
        }
        if dt.is_finite() && dt > 0.0 {
            self.remaining = (self.remaining - dt).max(0.0);
        }
        if self.remaining <= 0.0 && (dt.is_finite() && dt >= 0.0) {
            self.fired = true;
            return true;
        }
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn counts_down_and_fires_once() {
        let mut timer = RespawnTimer::new(2.0);
        assert!(!timer.tick(0.5));
        assert!((timer.remaining() - 1.5).abs() < 1.0e-6);
        assert!(!timer.tick(1.0));
        assert!(timer.tick(0.5), "reaches zero exactly");
        assert!(timer.is_finished());
        assert!(!timer.tick(1.0), "never fires twice");
    }

    #[test]
    fn a_big_step_past_zero_still_fires_once_and_clamps() {
        let mut timer = RespawnTimer::new(1.0);
        assert!(timer.tick(10.0));
        assert_eq!(timer.remaining(), 0.0);
        assert!(!timer.tick(10.0));
    }

    #[test]
    fn zero_length_timer_fires_on_the_first_tick() {
        let mut timer = RespawnTimer::new(0.0);
        assert!(!timer.is_finished());
        assert!(timer.tick(0.016));
        assert!(!timer.tick(0.016));
    }

    #[test]
    fn invalid_durations_count_as_zero() {
        for bad in [-5.0, f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
            let timer = RespawnTimer::new(bad);
            assert_eq!(timer.remaining(), 0.0, "{bad}");
        }
    }

    #[test]
    fn invalid_time_steps_do_not_advance_or_fire() {
        let mut timer = RespawnTimer::new(1.0);
        for bad in [-1.0, f32::NAN, f32::INFINITY] {
            assert!(!timer.tick(bad), "{bad}");
        }
        assert_eq!(timer.remaining(), 1.0);
        assert!(!timer.is_finished());
    }

    #[test]
    fn zero_step_does_not_advance_a_running_timer() {
        let mut timer = RespawnTimer::new(1.0);
        assert!(!timer.tick(0.0));
        assert_eq!(timer.remaining(), 1.0);
    }
}
