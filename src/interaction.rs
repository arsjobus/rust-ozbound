/// Euclidean interaction range check shared by NPCs and world objects.
pub fn within_interaction_range(actor: (f32, f32), target: (f32, f32), range: f32) -> bool {
    if !range.is_finite() || range < 0.0 || !actor.0.is_finite() || !actor.1.is_finite()
        || !target.0.is_finite() || !target.1.is_finite() { return false; }
    let dx = actor.0 - target.0;
    let dy = actor.1 - target.1;
    dx.mul_add(dx, dy * dy) <= range * range
}

/// Picks the closest target within `range` of `actor`; `None` when nothing is in range.
/// Targets are `(id, position)` pairs; ties go to the earliest target in iteration order.
pub fn nearest_in_range<I>(actor: (f32, f32), targets: I, range: f32) -> Option<usize>
where
    I: IntoIterator<Item = (usize, (f32, f32))>,
{
    let mut best: Option<(usize, f32)> = None;
    for (id, pos) in targets {
        if !within_interaction_range(actor, pos, range) { continue; }
        let (dx, dy) = (actor.0 - pos.0, actor.1 - pos.1);
        let dist_sq = dx.mul_add(dx, dy * dy);
        if best.map_or(true, |(_, d)| dist_sq < d) { best = Some((id, dist_sq)); }
    }
    best.map(|(id, _)| id)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn includes_boundary_and_excludes_outside() {
        assert!(within_interaction_range((0.0, 0.0), (3.0, 4.0), 5.0));
        assert!(!within_interaction_range((0.0, 0.0), (3.0, 4.1), 5.0));
    }
    #[test]
    fn rejects_invalid_ranges_and_coordinates() {
        assert!(!within_interaction_range((0.0, 0.0), (0.0, 0.0), -1.0));
        assert!(!within_interaction_range((f32::NAN, 0.0), (0.0, 0.0), 1.0));
    }
    #[test]
    fn nearest_in_range_picks_closest_and_ignores_far_targets() {
        let targets = [(7, (50.0, 0.0)), (8, (10.0, 0.0)), (9, (500.0, 0.0))];
        assert_eq!(nearest_in_range((0.0, 0.0), targets, 60.0), Some(8));
        assert_eq!(nearest_in_range((0.0, 0.0), targets, 5.0), None);
    }
    #[test]
    fn nearest_in_range_breaks_ties_by_order_and_handles_empty_or_invalid() {
        let tie = [(1, (3.0, 0.0)), (2, (-3.0, 0.0))];
        assert_eq!(nearest_in_range((0.0, 0.0), tie, 5.0), Some(1));
        assert_eq!(nearest_in_range((0.0, 0.0), Vec::new(), 5.0), None);
        assert_eq!(nearest_in_range((0.0, 0.0), [(1, (f32::NAN, 0.0))], 5.0), None);
    }
}
