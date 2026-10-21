use crate::pathfinding::{is_orthogonally_adjacent, Approach};
use crate::world::TilePos;

/// What a left-click on a world object does. The label shown while hovering is
/// "<verb> <name>", e.g. "Chop down tree".
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InteractionKind {
    TalkTo,
    ChopDown,
    Mine,
    Pick,
    /// Gathering from a plant patch or bush that grows back (herbs, berries).
    Forage,
    Read,
    /// Walk up to a creature from the bestiary (see `bestiary::approach`).
    Approach,
}

impl InteractionKind {
    pub fn verb(self) -> &'static str {
        match self {
            Self::TalkTo => "Talk to",
            Self::ChopDown => "Chop down",
            Self::Mine => "Mine",
            Self::Pick => "Pick",
            Self::Forage => "Forage",
            Self::Read => "Read",
            Self::Approach => "Approach",
        }
    }

    /// Where the player must stand to use this kind of object. This is the single source of
    /// truth for interaction range: scenery that blocks movement (trees, ore, signposts) and
    /// people are used from a walkable tile right beside them; things lying on open ground
    /// (moonpetals, herb patches, berry bushes) are used by standing on them.
    pub fn approach(self) -> Approach {
        match self {
            Self::Pick | Self::Forage => Approach::OnTile,
            Self::TalkTo | Self::ChopDown | Self::Mine | Self::Read | Self::Approach => Approach::Adjacent,
        }
    }
}

/// True when an actor standing on `actor` is close enough to use a target on `target` the way
/// `approach` demands: on the same tile, or orthogonally beside it (diagonals do not count).
/// Interactions re-check this on arrival, so nothing can be used from further away than the
/// route planner would have walked to.
pub fn in_reach(actor: TilePos, target: TilePos, approach: Approach) -> bool {
    match approach {
        Approach::OnTile => actor == target,
        Approach::Adjacent => is_orthogonally_adjacent(actor, target),
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
            Self::Forage => "Wild plants that grow back each season. Worth gathering.",
            Self::Read => "A weathered wooden signpost with a few words painted on it.",
            Self::Approach => "A shy creature of the countryside. Approach gently, and perhaps bring something it likes.",
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
    use crate::scene::meadow_level;
    use crate::pathfinding::plan_route;

    const ALL_KINDS: [InteractionKind; 7] = [
        InteractionKind::TalkTo,
        InteractionKind::ChopDown,
        InteractionKind::Mine,
        InteractionKind::Pick,
        InteractionKind::Forage,
        InteractionKind::Read,
        InteractionKind::Approach,
    ];
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
        let kinds = ALL_KINDS;
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
        assert_eq!(hover_text(InteractionKind::Forage, "meadowmint patch"), "Forage meadowmint patch");
    }
    #[test]
    fn hover_text_for_signposts_and_creatures() {
        assert_eq!(hover_text(InteractionKind::Read, "signpost"), "Read signpost");
        assert_eq!(hover_text(InteractionKind::Approach, "Meadow hare"), "Approach Meadow hare");
        // Creatures stand on open ground, but are approached from beside, never trampled.
        assert_eq!(InteractionKind::Approach.approach(), Approach::Adjacent);
    }

    #[test]
    fn every_kind_has_a_verb_and_an_approach() {
        for kind in ALL_KINDS {
            assert!(!kind.verb().is_empty());
        }
        // Only things growing on open ground are used by standing on them.
        let on_tile: Vec<_> = ALL_KINDS.iter().filter(|k| k.approach() == Approach::OnTile).collect();
        assert_eq!(on_tile, vec![&InteractionKind::Pick, &InteractionKind::Forage]);
    }

    #[test]
    fn on_tile_reach_needs_the_exact_tile() {
        let t = TilePos::new(4, 4);
        assert!(in_reach(t, t, Approach::OnTile));
        assert!(!in_reach(TilePos::new(4, 5), t, Approach::OnTile));
    }

    #[test]
    fn adjacent_reach_is_orthogonal_only() {
        let t = TilePos::new(4, 4);
        for (dx, dy) in [(1, 0), (-1, 0), (0, 1), (0, -1)] {
            assert!(in_reach(TilePos::new(4 + dx, 4 + dy), t, Approach::Adjacent), "({dx},{dy})");
        }
        for (dx, dy) in [(1, 1), (-1, 1), (1, -1), (-1, -1), (2, 0), (0, -2), (0, 0)] {
            assert!(!in_reach(TilePos::new(4 + dx, 4 + dy), t, Approach::Adjacent), "({dx},{dy})");
        }
    }

    #[test]
    fn reach_is_symmetric_and_handles_far_away_tiles() {
        let a = TilePos::new(1, 1);
        let b = TilePos::new(1, 2);
        assert_eq!(in_reach(a, b, Approach::Adjacent), in_reach(b, a, Approach::Adjacent));
        assert!(!in_reach(TilePos::new(i32::MIN / 2, 0), TilePos::new(i32::MAX / 2, 0), Approach::Adjacent));
    }

    /// The planner and the range rule must agree: wherever a planned route ends, the actor is
    /// in reach, for every kind of object the meadow places.
    #[test]
    fn every_planned_route_ends_in_reach_of_its_target() {
        let level = meadow_level().expect("built-in meadow parses");
        let start = level.player_start;
        let ends_in_reach = |target: TilePos, kind: InteractionKind| {
            let approach = kind.approach();
            let route = plan_route(&level.map, start, target, approach)
                .unwrap_or_else(|| panic!("no route to {kind:?} at {target:?}"));
            assert!(in_reach(route.last().copied().unwrap_or(start), target, approach), "{kind:?} at {target:?}");
        };
        ends_in_reach(level.guide.expect("the meadow has a guide"), InteractionKind::TalkTo);
        for petal in &level.moonpetals { ends_in_reach(*petal, InteractionKind::Pick); }
        for node in &level.nodes { ends_in_reach(node.tile, node.kind.interaction()); }
        for sign in &level.signs { ends_in_reach(sign.tile, InteractionKind::Read); }
    }
}
