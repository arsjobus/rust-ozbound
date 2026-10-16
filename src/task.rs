//! What the player is currently doing: walking a route, optionally to interact with a
//! target on arrival. Pure Rust and generic over the target id, so the Bevy layer can use
//! `Entity` while tests use plain integers.

use std::collections::VecDeque;

use crate::world::TilePos;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Task<T> {
    route: VecDeque<TilePos>,
    target: Option<T>,
}

impl<T> Default for Task<T> {
    fn default() -> Self {
        Self { route: VecDeque::new(), target: None }
    }
}

impl<T: Copy> Task<T> {
    pub fn idle() -> Self { Self::default() }

    /// Walk a route and stop.
    pub fn walk(route: Vec<TilePos>) -> Self {
        Self { route: route.into(), target: None }
    }

    /// Walk a route, then interact with `target`. An empty route means "already in position".
    pub fn interact(route: Vec<TilePos>, target: T) -> Self {
        Self { route: route.into(), target: Some(target) }
    }

    /// The tile the walker is heading to right now.
    pub fn next_step(&self) -> Option<TilePos> { self.route.front().copied() }

    /// Marks the current step as finished and returns the tile that was reached.
    pub fn complete_step(&mut self) -> Option<TilePos> { self.route.pop_front() }

    pub fn is_moving(&self) -> bool { !self.route.is_empty() }

    pub fn remaining_steps(&self) -> usize { self.route.len() }

    /// The interaction waiting at the end of the route, whether or not the walker has arrived.
    pub fn pending_target(&self) -> Option<T> { self.target }

    /// Hands out the target exactly once, and only after the whole route has been walked.
    pub fn take_ready_target(&mut self) -> Option<T> {
        if self.route.is_empty() { self.target.take() } else { None }
    }

    pub fn cancel(&mut self) {
        self.route.clear();
        self.target = None;
    }

    /// Where a freshly clicked route should be planned from: the tile being walked toward
    /// (so the walker finishes its stride instead of snapping back), else where it stands.
    pub fn planning_origin(&self, standing: TilePos) -> TilePos {
        self.next_step().unwrap_or(standing)
    }

    /// Builds the task that replaces this one after a new click. `route` must have been
    /// planned from [`Task::planning_origin`]; the step in progress is kept in front of it.
    pub fn rerouted(&self, route: Vec<TilePos>, target: Option<T>) -> Self {
        let mut full = VecDeque::with_capacity(route.len() + 1);
        if let Some(step) = self.next_step() {
            full.push_back(step);
        }
        full.extend(route);
        Self { route: full, target }
    }
}

/// Moves `pos` toward `dest` by at most `max_distance`. Returns the new position and whether
/// it reached `dest`. A negative or non-finite budget moves nothing.
pub fn step_toward(pos: (f32, f32), dest: (f32, f32), max_distance: f32) -> ((f32, f32), bool) {
    let (dx, dy) = (dest.0 - pos.0, dest.1 - pos.1);
    let dist = dx.hypot(dy);
    if !dist.is_finite() {
        return (pos, false);
    }
    if dist <= f32::EPSILON {
        return (dest, true);
    }
    if !max_distance.is_finite() || max_distance <= 0.0 {
        return (pos, false);
    }
    if max_distance >= dist {
        return (dest, true);
    }
    let k = max_distance / dist;
    ((pos.0 + dx * k, pos.1 + dy * k), false)
}

/// Yaw (radians about +Y) that makes a model facing +Z look along the horizontal direction
/// `(dx, dz)`. `None` for a zero or non-finite direction.
pub fn facing_yaw(dx: f32, dz: f32) -> Option<f32> {
    if !dx.is_finite() || !dz.is_finite() || (dx == 0.0 && dz == 0.0) {
        return None;
    }
    Some(dx.atan2(dz))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn t(x: i32, y: i32) -> TilePos { TilePos::new(x, y) }

    #[test]
    fn walking_consumes_the_route_one_step_at_a_time() {
        let mut task: Task<u32> = Task::walk(vec![t(1, 0), t(2, 0)]);
        assert!(task.is_moving());
        assert_eq!(task.next_step(), Some(t(1, 0)));
        assert_eq!(task.complete_step(), Some(t(1, 0)));
        assert_eq!(task.remaining_steps(), 1);
        assert_eq!(task.complete_step(), Some(t(2, 0)));
        assert!(!task.is_moving());
        assert_eq!(task.complete_step(), None);
    }

    #[test]
    fn interaction_is_only_ready_after_the_route_ends_and_only_once() {
        let mut task = Task::interact(vec![t(1, 0)], 42_u32);
        assert_eq!(task.pending_target(), Some(42));
        assert_eq!(task.take_ready_target(), None, "still walking");
        task.complete_step();
        assert_eq!(task.take_ready_target(), Some(42));
        assert_eq!(task.take_ready_target(), None, "consumed");
    }

    #[test]
    fn empty_route_means_interact_immediately() {
        let mut task = Task::interact(Vec::new(), 7_u32);
        assert!(!task.is_moving());
        assert_eq!(task.take_ready_target(), Some(7));
    }

    #[test]
    fn plain_walks_never_produce_a_target() {
        let mut task: Task<u32> = Task::walk(vec![t(1, 0)]);
        task.complete_step();
        assert_eq!(task.take_ready_target(), None);
    }

    #[test]
    fn cancel_clears_route_and_target() {
        let mut task = Task::interact(vec![t(1, 0), t(2, 0)], 1_u32);
        task.cancel();
        assert!(!task.is_moving());
        assert_eq!(task.pending_target(), None);
    }

    #[test]
    fn rerouting_finishes_the_step_in_progress_first() {
        let moving: Task<u32> = Task::walk(vec![t(1, 0), t(2, 0), t(3, 0)]);
        assert_eq!(moving.planning_origin(t(0, 0)), t(1, 0));
        let next = moving.rerouted(vec![t(1, 1), t(1, 2)], Some(9));
        assert_eq!(next.remaining_steps(), 3);
        assert_eq!(next.next_step(), Some(t(1, 0)), "keeps the stride that was underway");
        assert_eq!(next.pending_target(), Some(9));
    }

    #[test]
    fn rerouting_from_standing_still_uses_the_new_route_alone() {
        let idle: Task<u32> = Task::idle();
        assert_eq!(idle.planning_origin(t(4, 4)), t(4, 4));
        let next = idle.rerouted(vec![t(5, 4)], None);
        assert_eq!(next.remaining_steps(), 1);
        assert_eq!(next.next_step(), Some(t(5, 4)));
    }

    #[test]
    fn rerouting_with_an_empty_route_still_arrives_at_the_step_in_progress() {
        let moving: Task<u32> = Task::walk(vec![t(1, 0)]);
        let mut next = moving.rerouted(Vec::new(), Some(3));
        assert_eq!(next.take_ready_target(), None);
        next.complete_step();
        assert_eq!(next.take_ready_target(), Some(3));
    }

    #[test]
    fn step_toward_moves_partway_and_snaps_on_arrival() {
        let (p, arrived) = step_toward((0.0, 0.0), (4.0, 0.0), 1.0);
        assert!((p.0 - 1.0).abs() < 1.0e-6 && p.1 == 0.0 && !arrived);
        assert_eq!(step_toward((0.0, 0.0), (4.0, 0.0), 4.0), ((4.0, 0.0), true));
        assert_eq!(step_toward((0.0, 0.0), (3.0, 4.0), 100.0), ((3.0, 4.0), true), "no overshoot");
        assert_eq!(step_toward((2.0, 2.0), (2.0, 2.0), 0.0), ((2.0, 2.0), true), "already there");
    }

    #[test]
    fn step_toward_follows_the_diagonal() {
        let (p, arrived) = step_toward((0.0, 0.0), (3.0, 4.0), 2.5);
        assert!((p.0 - 1.5).abs() < 1.0e-5 && (p.1 - 2.0).abs() < 1.0e-5 && !arrived);
    }

    #[test]
    fn step_toward_ignores_bad_budgets_and_positions() {
        assert_eq!(step_toward((0.0, 0.0), (4.0, 0.0), -1.0), ((0.0, 0.0), false));
        assert_eq!(step_toward((0.0, 0.0), (4.0, 0.0), f32::NAN), ((0.0, 0.0), false));
        assert!(!step_toward((f32::NAN, 0.0), (4.0, 0.0), 1.0).1);
    }

    #[test]
    fn facing_yaw_points_along_the_direction_of_travel() {
        assert_eq!(facing_yaw(0.0, 1.0), Some(0.0), "+z is the model's default facing");
        assert!((facing_yaw(1.0, 0.0).unwrap() - std::f32::consts::FRAC_PI_2).abs() < 1.0e-6);
        assert!((facing_yaw(0.0, -1.0).unwrap().abs() - std::f32::consts::PI).abs() < 1.0e-6);
        assert_eq!(facing_yaw(0.0, 0.0), None);
        assert_eq!(facing_yaw(f32::NAN, 1.0), None);
    }
}
