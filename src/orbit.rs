//! Orbit-camera math for the third-person view. Pure Rust: the Bevy layer reads input,
//! calls these methods, and places the camera at `focus + offset()`.
//!
//! Yaw 0 puts the camera south of the focus looking north (`-z`), the default view of the map.

use std::f32::consts::TAU;

/// Lowest pitch (radians above the horizon), roughly 20 degrees.
pub const MIN_PITCH: f32 = 0.35;
/// Highest pitch, roughly 83 degrees: nearly top-down without flipping over.
pub const MAX_PITCH: f32 = 1.45;
pub const MIN_DISTANCE: f32 = 5.0;
pub const MAX_DISTANCE: f32 = 28.0;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct OrbitCamera {
    yaw: f32,
    pitch: f32,
    distance: f32,
}

impl Default for OrbitCamera {
    fn default() -> Self {
        Self { yaw: 0.0, pitch: 0.9, distance: 14.0 }
    }
}

impl OrbitCamera {
    /// Builds a camera, clamping pitch and distance. Non-finite values fall back to the default.
    pub fn new(yaw: f32, pitch: f32, distance: f32) -> Self {
        let d = Self::default();
        Self {
            yaw: if yaw.is_finite() { yaw.rem_euclid(TAU) } else { d.yaw },
            pitch: if pitch.is_finite() { pitch.clamp(MIN_PITCH, MAX_PITCH) } else { d.pitch },
            distance: if distance.is_finite() { distance.clamp(MIN_DISTANCE, MAX_DISTANCE) } else { d.distance },
        }
    }

    pub fn yaw(&self) -> f32 { self.yaw }
    pub fn pitch(&self) -> f32 { self.pitch }
    pub fn distance(&self) -> f32 { self.distance }

    /// Turns the camera. Yaw wraps around; pitch is clamped. Non-finite deltas are ignored.
    pub fn rotate(&mut self, delta_yaw: f32, delta_pitch: f32) {
        if delta_yaw.is_finite() {
            self.yaw = (self.yaw + delta_yaw).rem_euclid(TAU);
        }
        if delta_pitch.is_finite() {
            self.pitch = (self.pitch + delta_pitch).clamp(MIN_PITCH, MAX_PITCH);
        }
    }

    /// Moves the camera closer (negative) or farther (positive), within the allowed range.
    pub fn zoom(&mut self, delta: f32) {
        if delta.is_finite() {
            self.distance = (self.distance + delta).clamp(MIN_DISTANCE, MAX_DISTANCE);
        }
    }

    /// Camera position relative to the focus point. Its length is always `distance()`.
    pub fn offset(&self) -> [f32; 3] {
        let flat = self.distance * self.pitch.cos();
        [flat * self.yaw.sin(), self.distance * self.pitch.sin(), flat * self.yaw.cos()]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn length(v: [f32; 3]) -> f32 { (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt() }

    #[test]
    fn default_view_sits_south_and_above_looking_north() {
        let o = OrbitCamera::default().offset();
        assert!(o[0].abs() < 1.0e-5, "no sideways offset at yaw 0");
        assert!(o[1] > 0.0, "above the ground");
        assert!(o[2] > 0.0, "south of the focus, so it looks toward -z (north)");
    }

    #[test]
    fn offset_length_equals_distance_for_any_angles() {
        for (yaw, pitch, dist) in [(0.0, 0.5, 6.0), (2.0, 1.2, 20.0), (5.5, 0.4, 9.0)] {
            let cam = OrbitCamera::new(yaw, pitch, dist);
            assert!((length(cam.offset()) - cam.distance()).abs() < 1.0e-4);
        }
    }

    #[test]
    fn quarter_turn_moves_the_camera_to_the_east() {
        let mut cam = OrbitCamera::default();
        cam.rotate(std::f32::consts::FRAC_PI_2, 0.0);
        let o = cam.offset();
        assert!(o[0] > 1.0 && o[2].abs() < 1.0e-3);
    }

    #[test]
    fn pitch_and_distance_are_clamped() {
        let mut cam = OrbitCamera::default();
        cam.rotate(0.0, 10.0);
        assert_eq!(cam.pitch(), MAX_PITCH);
        cam.rotate(0.0, -10.0);
        assert_eq!(cam.pitch(), MIN_PITCH);
        cam.zoom(1000.0);
        assert_eq!(cam.distance(), MAX_DISTANCE);
        cam.zoom(-1000.0);
        assert_eq!(cam.distance(), MIN_DISTANCE);
    }

    #[test]
    fn yaw_wraps_in_both_directions() {
        let mut cam = OrbitCamera::default();
        cam.rotate(-0.1, 0.0);
        assert!(cam.yaw() > 6.0 && cam.yaw() < TAU);
        cam.rotate(0.2, 0.0);
        assert!(cam.yaw() > 0.0 && cam.yaw() < 0.2);
    }

    #[test]
    fn non_finite_input_is_ignored_or_defaulted() {
        let mut cam = OrbitCamera::default();
        let before = cam;
        cam.rotate(f32::NAN, f32::INFINITY);
        cam.zoom(f32::NAN);
        assert_eq!(cam, before);
        assert_eq!(OrbitCamera::new(f32::NAN, f32::NAN, f32::NAN), OrbitCamera::default());
    }
}
