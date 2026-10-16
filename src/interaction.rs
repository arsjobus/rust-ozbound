/// What a left-click on a world object does. The label shown while hovering is
/// "<verb> <name>", e.g. "Chop down tree".
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InteractionKind {
    TalkTo,
    ChopDown,
    Mine,
    Pick,
}

impl InteractionKind {
    pub fn verb(self) -> &'static str {
        match self {
            Self::TalkTo => "Talk to",
            Self::ChopDown => "Chop down",
            Self::Mine => "Mine",
            Self::Pick => "Pick",
        }
    }
}

impl InteractionKind {
    /// Short description shown when the player chooses "Examine".
    pub fn examine(self) -> &'static str {
        match self {
            Self::TalkTo => "A friendly villager who knows the meadow well.",
            Self::ChopDown => "A sturdy meadow tree. Its timber would be useful.",
            Self::Mine => "A rock streaked with copper.",
            Self::Pick => "A moonpetal, glowing faintly in the grass.",
        }
    }
}

/// Examine text for something that can currently be gathered from, or has run dry.
pub fn examine_text(kind: InteractionKind, spent: bool) -> String {
    if spent {
        format!("{} It has nothing left to give for now, but it will recover in time.", kind.examine())
    } else {
        kind.examine().to_string()
    }
}

/// Hover label for an object, e.g. `hover_text(InteractionKind::Mine, "copper vein")`.
pub fn hover_text(kind: InteractionKind, name: &str) -> String {
    format!("{} {}", kind.verb(), name)
}

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
        if best.is_none_or(|(_, d)| dist_sq < d) { best = Some((id, dist_sq)); }
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
    #[test]
    fn every_kind_has_distinct_non_empty_examine_text() {
        let kinds = [InteractionKind::TalkTo, InteractionKind::ChopDown, InteractionKind::Mine, InteractionKind::Pick];
        for (i, a) in kinds.iter().enumerate() {
            assert!(!a.examine().is_empty());
            for b in &kinds[i + 1..] {
                assert_ne!(a.examine(), b.examine());
            }
        }
    }
    #[test]
    fn spent_resources_say_they_will_recover() {
        let fresh = examine_text(InteractionKind::ChopDown, false);
        let spent = examine_text(InteractionKind::ChopDown, true);
        assert_eq!(fresh, InteractionKind::ChopDown.examine());
        assert!(spent.starts_with(&fresh));
        assert!(spent.contains("recover"));
    }
    #[test]
    fn hover_text_combines_verb_and_name() {
        assert_eq!(hover_text(InteractionKind::ChopDown, "tree"), "Chop down tree");
        assert_eq!(hover_text(InteractionKind::TalkTo, "Village Guide"), "Talk to Village Guide");
        assert_eq!(hover_text(InteractionKind::Mine, "copper vein"), "Mine copper vein");
        assert_eq!(hover_text(InteractionKind::Pick, "moonpetal"), "Pick moonpetal");
    }
}
