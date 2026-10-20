//! Emotes: short, purely visual gestures such as waving. Pure Rust (no Bevy).
//!
//! An [`Emote`] knows its chat command, how long it lasts, what the chat says when it starts,
//! and the [`Pose`] a character should hold `elapsed` seconds into it. The Bevy layer only
//! applies the pose to a character's limbs, so the same emotes can later be played by NPCs
//! (the NPC behaviour milestone) without any new rules.
//!
//! Rules:
//!
//! - An emote changes nothing about the game: no quest, skill, bag, or save state.
//! - A character can start one only while standing still and not working
//!   ([`can_start`]); walking away or starting to work ends it at once.
//! - Every pose starts and ends at rest (arms down), so an emote never snaps.
//! - Limb angles are in radians. `pitch` swings a limb forward (negative) or back (positive)
//!   and `roll` raises it sideways, positive meaning *away from the body*. Index 0 is the limb
//!   on the character's -x side and index 1 the limb on its +x side.

use std::f32::consts::TAU;

/// Seconds an emote takes to ease in and out of its pose.
const EASE_SECONDS: f32 = 0.3;

/// The most any limb angle can reach, in radians (just short of straight up).
pub const MAX_ANGLE: f32 = 3.0;
/// The highest a character is ever lifted off the ground, in world units.
pub const MAX_HOP: f32 = 0.2;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Emote {
    Wave,
    Dance,
    Cheer,
}

/// How one limb is held: a forward/back swing and a sideways raise.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct LimbPose {
    pub pitch: f32,
    pub roll: f32,
}

/// The whole body at one moment of an emote.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Pose {
    pub arms: [LimbPose; 2],
    pub legs: [LimbPose; 2],
    /// How far the body is lifted off the ground (never negative).
    pub hop: f32,
}

impl Pose {
    /// Arms and legs hanging, feet on the ground.
    pub const REST: Pose = Pose { arms: [LimbPose { pitch: 0.0, roll: 0.0 }; 2], legs: [LimbPose { pitch: 0.0, roll: 0.0 }; 2], hop: 0.0 };
}

impl Emote {
    /// Every emote, in the order `/help` lists them.
    pub const ALL: [Emote; 3] = [Self::Wave, Self::Dance, Self::Cheer];

    /// The chat command without its slash: `wave` for `/wave`.
    pub fn command(self) -> &'static str {
        match self {
            Self::Wave => "wave",
            Self::Dance => "dance",
            Self::Cheer => "cheer",
        }
    }

    /// The emote for a command word (no slash), ignoring case.
    pub fn from_command(word: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|emote| emote.command().eq_ignore_ascii_case(word))
    }

    /// How long the emote lasts, in seconds.
    pub fn duration(self) -> f32 {
        match self {
            Self::Wave => 2.4,
            Self::Dance => 3.6,
            Self::Cheer => 2.0,
        }
    }

    /// The line the chat shows when the player does it.
    pub fn message(self) -> &'static str {
        match self {
            Self::Wave => "You wave.",
            Self::Dance => "You dance.",
            Self::Cheer => "You cheer!",
        }
    }

    /// The pose `elapsed` seconds in. Before the start, after the end, or for a time that is
    /// not a number the character is at rest.
    pub fn pose(self, elapsed: f32) -> Pose {
        let blend = ease(elapsed, self.duration());
        if blend == 0.0 {
            return Pose::REST;
        }
        let limb = |pitch: f32, roll: f32| LimbPose { pitch: pitch * blend, roll: roll * blend };
        match self {
            // One arm raised high, waving from side to side.
            Self::Wave => {
                let sway = (elapsed * TAU * 2.2).sin() * 0.35;
                Pose { arms: [LimbPose::default(), limb(0.0, 2.6 + sway)], legs: [LimbPose::default(); 2], hop: 0.0 }
            }
            // Both arms up and pumping, with small hops.
            Self::Cheer => {
                let pump = (elapsed * TAU * 3.0).sin();
                Pose {
                    arms: [limb(0.25 * pump, 2.7 + 0.2 * pump), limb(-0.25 * pump, 2.7 + 0.2 * pump)],
                    legs: [LimbPose::default(); 2],
                    hop: blend * MAX_HOP * 0.7 * (elapsed * TAU * 1.5).sin().abs(),
                }
            }
            // Arms swinging in opposite directions, feet stepping, a light bounce.
            Self::Dance => {
                let beat = (elapsed * TAU * 1.4).sin();
                Pose {
                    arms: [limb(0.9 * beat, 0.55), limb(-0.9 * beat, 0.55)],
                    legs: [limb(0.55 * beat, 0.0), limb(-0.55 * beat, 0.0)],
                    hop: blend * MAX_HOP * 0.3 * (elapsed * TAU * 2.8).sin().abs(),
                }
            }
        }
    }
}

/// 0 outside the emote, rising smoothly to 1 over [`EASE_SECONDS`], holding, and falling back
/// to 0 over the last [`EASE_SECONDS`].
fn ease(elapsed: f32, duration: f32) -> f32 {
    if !elapsed.is_finite() || elapsed <= 0.0 || elapsed >= duration {
        return 0.0;
    }
    let edge = (elapsed / EASE_SECONDS).min((duration - elapsed) / EASE_SECONDS).clamp(0.0, 1.0);
    edge * edge * (3.0 - 2.0 * edge)
}

/// An emote in progress.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ActiveEmote {
    emote: Emote,
    elapsed: f32,
}

impl ActiveEmote {
    pub fn start(emote: Emote) -> Self {
        Self { emote, elapsed: 0.0 }
    }

    pub fn emote(&self) -> Emote {
        self.emote
    }

    pub fn elapsed(&self) -> f32 {
        self.elapsed
    }

    /// Moves time forward. A time step that is negative or not a number changes nothing.
    pub fn tick(&mut self, seconds: f32) {
        if seconds.is_finite() && seconds > 0.0 {
            self.elapsed += seconds;
        }
    }

    pub fn is_finished(&self) -> bool {
        self.elapsed >= self.emote.duration()
    }

    pub fn pose(&self) -> Pose {
        self.emote.pose(self.elapsed)
    }
}

/// Whether a character in this state may start an emote, or why not (as a chat line).
pub fn can_start(moving: bool, working: bool) -> Result<(), &'static str> {
    if moving {
        Err("You can't do that while you are walking.")
    } else if working {
        Err("Finish what you are doing first.")
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn samples(emote: Emote) -> impl Iterator<Item = f32> {
        let steps = (emote.duration() * 100.0) as usize;
        (0..=steps + 20).map(|i| i as f32 * 0.01)
    }

    #[test]
    fn commands_are_unique_lowercase_words_that_round_trip_in_any_case() {
        let mut seen = std::collections::HashSet::new();
        for emote in Emote::ALL {
            let word = emote.command();
            assert!(!word.is_empty() && word.chars().all(|c| c.is_ascii_lowercase()), "{word}");
            assert!(seen.insert(word), "duplicate command {word}");
            assert_eq!(Emote::from_command(word), Some(emote));
            assert_eq!(Emote::from_command(&word.to_uppercase()), Some(emote));
        }
        assert_eq!(Emote::from_command("Wave"), Some(Emote::Wave));
        assert_eq!(Emote::from_command("jump"), None);
        assert_eq!(Emote::from_command(""), None);
        assert_eq!(Emote::from_command("/wave"), None, "the slash is not part of the word");
    }

    #[test]
    fn every_emote_has_a_positive_duration_and_a_message() {
        for emote in Emote::ALL {
            assert!(emote.duration() > 0.5 && emote.duration() < 10.0, "{emote:?} should be brief");
            assert!(emote.message().starts_with("You "), "{}", emote.message());
            assert!(emote.message().ends_with(['.', '!']));
        }
    }

    #[test]
    fn every_pose_starts_and_ends_at_rest() {
        for emote in Emote::ALL {
            assert_eq!(emote.pose(0.0), Pose::REST, "{emote:?} start");
            assert_eq!(emote.pose(emote.duration()), Pose::REST, "{emote:?} end");
            assert_eq!(emote.pose(emote.duration() + 5.0), Pose::REST, "{emote:?} long after");
            assert_eq!(emote.pose(-1.0), Pose::REST, "{emote:?} before");
        }
    }

    #[test]
    fn poses_ease_in_instead_of_snapping() {
        for emote in Emote::ALL {
            let early = emote.pose(0.05);
            let held = emote.pose(emote.duration() / 2.0);
            let lift = |p: Pose| p.arms.iter().chain(p.legs.iter()).map(|l| l.roll.abs() + l.pitch.abs()).sum::<f32>() + p.hop;
            assert!(lift(early) < lift(held) || lift(held) == 0.0, "{emote:?}");
            assert!(lift(early) < 1.0, "{emote:?} should start gently");
        }
    }

    #[test]
    fn bad_times_give_rest_not_garbage() {
        for emote in Emote::ALL {
            for t in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY, -0.0, f32::MAX] {
                assert_eq!(emote.pose(t), Pose::REST, "{emote:?} at {t}");
            }
        }
    }

    #[test]
    fn angles_and_hops_stay_in_bounds_for_the_whole_emote() {
        for emote in Emote::ALL {
            for t in samples(emote) {
                let pose = emote.pose(t);
                for limb in pose.arms.iter().chain(pose.legs.iter()) {
                    assert!(limb.pitch.is_finite() && limb.roll.is_finite());
                    assert!(limb.pitch.abs() <= MAX_ANGLE && limb.roll.abs() <= MAX_ANGLE, "{emote:?} at {t}: {limb:?}");
                }
                assert!((0.0..=MAX_HOP).contains(&pose.hop), "{emote:?} at {t}: hop {}", pose.hop);
            }
        }
    }

    #[test]
    fn wave_raises_one_arm_cheer_raises_both_and_dance_moves_legs() {
        let mid = |e: Emote| e.pose(e.duration() / 2.0);
        let wave = mid(Emote::Wave);
        assert!(wave.arms[1].roll > 2.0, "the waving arm is above the shoulder");
        assert_eq!(wave.arms[0], LimbPose::default(), "the other arm stays down");
        assert_eq!(wave.hop, 0.0);

        let cheer = mid(Emote::Cheer);
        assert!(cheer.arms[0].roll > 2.0 && cheer.arms[1].roll > 2.0);

        let moved = samples(Emote::Dance).any(|t| Emote::Dance.pose(t).legs[0].pitch.abs() > 0.2);
        assert!(moved, "dancing steps with the legs");
        let still = samples(Emote::Wave).all(|t| Emote::Wave.pose(t).legs == [LimbPose::default(); 2]);
        assert!(still, "waving keeps the feet still");
    }

    #[test]
    fn dancing_swings_the_arms_in_opposite_directions() {
        let t = Emote::Dance.duration() / 3.0;
        let pose = Emote::Dance.pose(t);
        assert!(pose.arms[0].pitch * pose.arms[1].pitch < 0.0, "{pose:?}");
        assert!(pose.legs[0].pitch * pose.legs[1].pitch < 0.0, "{pose:?}");
    }

    #[test]
    fn poses_are_deterministic() {
        for emote in Emote::ALL {
            for t in samples(emote).step_by(7) {
                assert_eq!(emote.pose(t), emote.pose(t));
            }
        }
    }

    #[test]
    fn an_active_emote_finishes_once_its_time_is_up() {
        let mut active = ActiveEmote::start(Emote::Cheer);
        assert_eq!(active.emote(), Emote::Cheer);
        assert!(!active.is_finished());
        assert_eq!(active.pose(), Pose::REST, "the first frame is at rest");
        active.tick(1.0);
        assert!(!active.is_finished());
        assert_ne!(active.pose(), Pose::REST);
        active.tick(Emote::Cheer.duration() - 1.0);
        assert!(active.is_finished());
        assert_eq!(active.pose(), Pose::REST);
    }

    #[test]
    fn bad_time_steps_change_nothing() {
        let mut active = ActiveEmote::start(Emote::Wave);
        for bad in [f32::NAN, -1.0, 0.0, f32::INFINITY, f32::NEG_INFINITY] {
            active.tick(bad);
            assert_eq!(active.elapsed(), 0.0, "{bad}");
        }
        active.tick(0.5);
        assert_eq!(active.elapsed(), 0.5);
    }

    #[test]
    fn emotes_need_a_character_who_is_standing_still_and_idle() {
        assert_eq!(can_start(false, false), Ok(()));
        assert!(can_start(true, false).unwrap_err().contains("walking"));
        assert!(can_start(false, true).unwrap_err().contains("doing"));
        assert!(can_start(true, true).is_err());
    }
}
