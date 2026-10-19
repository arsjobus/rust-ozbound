//! Keyboard-only play: choosing something to visit with Tab, and stepping with WASD. Pure Rust.
//!
//! - [`rank`] orders everything the player could use by how far they would have to walk, using
//!   the same route planner the mouse uses, so a target the keyboard offers is always one a
//!   click could reach, and nothing the planner cannot reach is offered.
//! - [`cycle`] moves the selection forward or back through that list and wraps round.
//! - [`step_direction`] turns W/A/S/D into a one-tile step relative to where the camera looks,
//!   and [`step_to`] checks the step against the same walkability rule as everything else.

use crate::pathfinding::{plan_route, Approach};
use crate::world::{TilePos, WorldMap};

/// Something that may be selected: an id the caller understands, where it stands, how it is
/// approached, and what it is called ("Chop down tree").
#[derive(Debug, Clone, PartialEq)]
pub struct Candidate<T> {
    pub id: T,
    pub tile: TilePos,
    pub approach: Approach,
    pub label: String,
}

/// A reachable candidate and how many steps the walk to it takes (0 means "already in reach").
#[derive(Debug, Clone, PartialEq)]
pub struct Ranked<T> {
    pub id: T,
    pub steps: usize,
    pub label: String,
}

/// Everything reachable from `from`, nearest first. Ties keep the order the candidates were
/// given in, so the list is stable from one press to the next. Unreachable candidates are left
/// out.
pub fn rank<T: Copy>(map: &WorldMap, from: TilePos, candidates: &[Candidate<T>]) -> Vec<Ranked<T>> {
    let mut ranked: Vec<Ranked<T>> = candidates
        .iter()
        .filter_map(|c| {
            plan_route(map, from, c.tile, c.approach).map(|route| Ranked { id: c.id, steps: route.len(), label: c.label.clone() })
        })
        .collect();
    // `sort_by_key` is stable, which is what keeps ties in the given order.
    ranked.sort_by_key(|r| r.steps);
    ranked
}

/// The selection after pressing Tab (`forward`) or Shift+Tab. With nothing selected, or a
/// selection that is no longer in the list, Tab picks the nearest and Shift+Tab the farthest.
/// The list wraps. `None` only when the list is empty.
pub fn cycle<T: Copy + PartialEq>(ranked: &[Ranked<T>], current: Option<T>, forward: bool) -> Option<T> {
    if ranked.is_empty() {
        return None;
    }
    let position = current.and_then(|id| ranked.iter().position(|r| r.id == id));
    let index = match (position, forward) {
        (None, true) => 0,
        (None, false) => ranked.len() - 1,
        (Some(i), true) => (i + 1) % ranked.len(),
        (Some(i), false) => (i + ranked.len() - 1) % ranked.len(),
    };
    Some(ranked[index].id)
}

/// The text shown for the selection, for example `Talk to Village Guide (3 steps) - Enter to go`.
pub fn describe<T>(item: &Ranked<T>) -> String {
    let distance = match item.steps {
        0 => "here".to_string(),
        1 => "1 step".to_string(),
        n => format!("{n} steps"),
    };
    format!("{} ({distance}) - Enter to go, X to examine", item.label)
}

/// A direction on the keyboard, relative to the camera.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Dir {
    Forward,
    Back,
    Left,
    Right,
}

/// The tile offset `(dx, dy)` for pressing `dir` when the camera has orbit yaw `yaw` (see
/// `orbit.rs`: yaw 0 looks north, which is +y on the map). The camera's view direction is
/// rounded to the nearest of the four map directions, so W always moves roughly "up the screen".
pub fn step_direction(yaw: f32, dir: Dir) -> (i32, i32) {
    let yaw = if yaw.is_finite() { yaw } else { 0.0 };
    // Forward on the map for this yaw: the camera sits at (sin yaw, cos yaw) around the focus
    // (world x, world z), so it looks along (-sin, -cos); map y is world -z.
    let (fx, fy) = (-yaw.sin(), yaw.cos());
    let forward = if fx.abs() > fy.abs() { (fx.signum() as i32, 0) } else { (0, fy.signum() as i32) };
    let right = (forward.1, -forward.0);
    match dir {
        Dir::Forward => forward,
        Dir::Back => (-forward.0, -forward.1),
        Dir::Right => right,
        Dir::Left => (-right.0, -right.1),
    }
}

/// Why a keyboard step did not happen.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Blocked {
    /// The tile beyond is off the map or not walkable (water, rock, a tree, a fence, a sign).
    Tile,
}

/// The tile one step from `from` by `delta`, if the player may stand there.
pub fn step_to(map: &WorldMap, from: TilePos, delta: (i32, i32)) -> Result<TilePos, Blocked> {
    let to = TilePos::new(from.x.saturating_add(delta.0), from.y.saturating_add(delta.1));
    if map.can_walk_to(to) { Ok(to) } else { Err(Blocked::Tile) }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scene::meadow_level;
    use std::f32::consts::{FRAC_PI_2, PI, TAU};

    fn t(x: i32, y: i32) -> TilePos {
        TilePos::new(x, y)
    }

    fn open_map() -> WorldMap {
        WorldMap::from_ascii("T.....\n......\n..~...\n......").unwrap()
    }

    fn cand(id: u32, tile: TilePos, approach: Approach) -> Candidate<u32> {
        Candidate { id, tile, approach, label: format!("thing {id}") }
    }

    #[test]
    fn nearest_first_with_ties_in_the_given_order() {
        let map = WorldMap::from_ascii("......\n......\n......").unwrap();
        let from = t(0, 0);
        let list = [
            cand(1, t(4, 0), Approach::OnTile),
            cand(2, t(1, 0), Approach::OnTile),
            cand(3, t(0, 1), Approach::OnTile),
            cand(4, t(2, 0), Approach::OnTile),
        ];
        let ranked = rank(&map, from, &list);
        let ids: Vec<u32> = ranked.iter().map(|r| r.id).collect();
        assert_eq!(ids, [2, 3, 4, 1], "2 and 3 are both one step away, so they keep their given order");
        assert_eq!(ranked[0].steps, 1);
        assert_eq!(ranked[3].steps, 4);
    }

    #[test]
    fn unreachable_candidates_are_left_out_and_in_reach_ones_cost_nothing() {
        let map = WorldMap::from_ascii("..#..\n..#..\n..#..").unwrap();
        let list = [cand(1, t(4, 1), Approach::OnTile), cand(2, t(1, 1), Approach::OnTile), cand(3, t(0, 0), Approach::OnTile)];
        let ranked = rank(&map, t(0, 0), &list);
        let ids: Vec<u32> = ranked.iter().map(|r| r.id).collect();
        assert_eq!(ids, [3, 2], "the far side of the wall is not offered");
        assert_eq!(ranked[0].steps, 0, "already standing on it");
    }

    #[test]
    fn blocked_targets_are_approached_from_beside() {
        let map = open_map();
        let tree = cand(7, t(0, 3), Approach::Adjacent);
        let ranked = rank(&map, t(5, 0), &[tree]);
        assert_eq!(ranked.len(), 1);
        assert!(ranked[0].steps >= 1);
        let enclosed = WorldMap::from_ascii("#T#\nTTT\n#T#").unwrap();
        assert!(rank(&enclosed, t(0, 0), &[cand(1, t(1, 1), Approach::Adjacent)]).is_empty());
    }

    #[test]
    fn an_empty_list_ranks_to_nothing() {
        let none: Vec<Candidate<u32>> = Vec::new();
        assert!(rank(&open_map(), t(1, 1), &none).is_empty());
    }

    #[test]
    fn cycling_goes_forward_and_back_and_wraps() {
        let ranked: Vec<Ranked<u32>> = (1..=3).map(|id| Ranked { id, steps: id as usize, label: String::new() }).collect();
        assert_eq!(cycle(&ranked, None, true), Some(1), "Tab with nothing selected picks the nearest");
        assert_eq!(cycle(&ranked, None, false), Some(3), "Shift+Tab with nothing selected picks the farthest");
        assert_eq!(cycle(&ranked, Some(1), true), Some(2));
        assert_eq!(cycle(&ranked, Some(3), true), Some(1), "wraps forward");
        assert_eq!(cycle(&ranked, Some(1), false), Some(3), "wraps back");
        assert_eq!(cycle(&ranked, Some(2), false), Some(1));
    }

    #[test]
    fn a_stale_selection_starts_again_and_an_empty_list_selects_nothing() {
        let ranked = vec![Ranked { id: 5_u32, steps: 1, label: String::new() }, Ranked { id: 6, steps: 2, label: String::new() }];
        assert_eq!(cycle(&ranked, Some(99), true), Some(5));
        assert_eq!(cycle(&ranked, Some(99), false), Some(6));
        let none: Vec<Ranked<u32>> = Vec::new();
        assert_eq!(cycle(&none, Some(5), true), None);
        assert_eq!(cycle(&none, None, false), None);
        let one = vec![Ranked { id: 1_u32, steps: 0, label: String::new() }];
        assert_eq!(cycle(&one, Some(1), true), Some(1), "a single choice stays selected");
        assert_eq!(cycle(&one, Some(1), false), Some(1));
    }

    #[test]
    fn a_full_lap_of_tab_visits_every_target_exactly_once() {
        let ranked: Vec<Ranked<u32>> = (0..12).map(|id| Ranked { id, steps: id as usize, label: String::new() }).collect();
        let mut seen = Vec::new();
        let mut current = None;
        for _ in 0..ranked.len() {
            current = cycle(&ranked, current, true);
            seen.push(current.unwrap());
        }
        let mut sorted = seen.clone();
        sorted.sort_unstable();
        sorted.dedup();
        assert_eq!(sorted.len(), ranked.len());
        assert_eq!(cycle(&ranked, current, true), Some(0), "the next press starts the lap again");
    }

    #[test]
    fn descriptions_say_how_far_and_what_to_press() {
        let item = |steps| Ranked { id: 1_u32, steps, label: "Talk to Village Guide".into() };
        assert!(describe(&item(0)).contains("(here)"));
        assert!(describe(&item(1)).contains("(1 step)"));
        assert!(describe(&item(7)).contains("(7 steps)"));
        let text = describe(&item(3));
        assert!(text.starts_with("Talk to Village Guide") && text.contains("Enter") && text.contains('X'), "{text}");
    }

    #[test]
    fn wasd_follows_the_camera_in_quarter_turns() {
        // Yaw 0: the camera looks north, so forward is +y and right is +x.
        assert_eq!(step_direction(0.0, Dir::Forward), (0, 1));
        assert_eq!(step_direction(0.0, Dir::Back), (0, -1));
        assert_eq!(step_direction(0.0, Dir::Right), (1, 0));
        assert_eq!(step_direction(0.0, Dir::Left), (-1, 0));
        // A quarter turn (the camera now sits to the east and looks west): forward is -x.
        assert_eq!(step_direction(FRAC_PI_2, Dir::Forward), (-1, 0));
        assert_eq!(step_direction(FRAC_PI_2, Dir::Right), (0, 1));
        // Half a turn: everything is reversed.
        assert_eq!(step_direction(PI, Dir::Forward), (0, -1));
        assert_eq!(step_direction(PI, Dir::Right), (-1, 0));
        // Three quarters: forward is +x.
        assert_eq!(step_direction(3.0 * FRAC_PI_2, Dir::Forward), (1, 0));
        assert_eq!(step_direction(3.0 * FRAC_PI_2, Dir::Right), (0, -1));
    }

    #[test]
    fn the_four_keys_are_always_four_different_unit_steps_and_opposites_cancel() {
        let mut yaw = -TAU;
        while yaw <= 2.0 * TAU {
            let steps: Vec<(i32, i32)> = [Dir::Forward, Dir::Back, Dir::Left, Dir::Right].iter().map(|d| step_direction(yaw, *d)).collect();
            for (dx, dy) in &steps {
                assert_eq!(dx.abs() + dy.abs(), 1, "yaw {yaw}: {steps:?}");
            }
            let mut unique = steps.clone();
            unique.sort_unstable();
            unique.dedup();
            assert_eq!(unique.len(), 4, "yaw {yaw}: {steps:?}");
            assert_eq!((steps[0].0 + steps[1].0, steps[0].1 + steps[1].1), (0, 0));
            assert_eq!((steps[2].0 + steps[3].0, steps[2].1 + steps[3].1), (0, 0));
            yaw += 0.07;
        }
    }

    #[test]
    fn slightly_turned_cameras_keep_the_nearest_direction() {
        assert_eq!(step_direction(0.3, Dir::Forward), (0, 1));
        assert_eq!(step_direction(-0.3, Dir::Forward), (0, 1));
        assert_eq!(step_direction(FRAC_PI_2 + 0.3, Dir::Forward), (-1, 0));
        assert_eq!(step_direction(f32::NAN, Dir::Forward), (0, 1), "a bad yaw acts like the default view");
    }

    #[test]
    fn steps_stop_at_blockers_and_the_map_edge() {
        let map = open_map();
        // The map text lists the top row first, so (0, 3) is the tree in the corner.
        assert_eq!(step_to(&map, t(1, 3), (-1, 0)), Err(Blocked::Tile), "a tree blocks");
        assert_eq!(step_to(&map, t(2, 0), (0, 1)), Err(Blocked::Tile), "water blocks");
        assert_eq!(step_to(&map, t(0, 0), (-1, 0)), Err(Blocked::Tile), "the edge blocks");
        assert_eq!(step_to(&map, t(0, 0), (0, -1)), Err(Blocked::Tile));
        assert_eq!(step_to(&map, t(1, 1), (0, 1)), Ok(t(1, 2)));
        assert_eq!(step_to(&map, t(i32::MAX, 0), (1, 0)), Err(Blocked::Tile), "no overflow at extreme coordinates");
    }

    #[test]
    fn steps_agree_with_the_walkability_rule_everywhere_in_the_meadow() {
        let level = meadow_level().expect("the built-in meadow is valid");
        let map = &level.map;
        for y in 0..map.height() as i32 {
            for x in 0..map.width() as i32 {
                for delta in [(0, 1), (0, -1), (1, 0), (-1, 0)] {
                    let from = t(x, y);
                    let to = t(x + delta.0, y + delta.1);
                    assert_eq!(step_to(map, from, delta).is_ok(), map.can_walk_to(to), "{from:?} by {delta:?}");
                }
            }
        }
    }
}
