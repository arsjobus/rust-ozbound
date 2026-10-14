/// Euclidean interaction range check shared by NPCs and world objects.
pub fn within_interaction_range(actor: (f32, f32), target: (f32, f32), range: f32) -> bool {
    if !range.is_finite() || range < 0.0 || !actor.0.is_finite() || !actor.1.is_finite()
        || !target.0.is_finite() || !target.1.is_finite() { return false; }
    let dx = actor.0 - target.0;
    let dy = actor.1 - target.1;
    dx.mul_add(dx, dy * dy) <= range * range
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
}
