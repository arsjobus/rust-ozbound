//! Mouse picking in 3D. Pure Rust: the Bevy layer turns the cursor into a ray, and this
//! module decides what that ray is pointing at (an object or a ground tile).
//!
//! Coordinates are 3D world space: x east, y up, z south (north is `-z`; see
//! [`WorldMap::tile_center_3d`](crate::world::WorldMap::tile_center_3d)).

use crate::world::{TilePos, WorldMap};

pub type Vec3f = [f32; 3];

/// Where a ray meets the ground plane. `distance` is in multiples of the ray direction.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GroundHit {
    pub distance: f32,
    pub x: f32,
    pub z: f32,
}

/// An axis-aligned box that can be clicked, tagged with the caller's own id.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PickBox<T> {
    pub id: T,
    pub min: Vec3f,
    pub max: Vec3f,
}

/// What the cursor is over.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Pick<T> {
    Nothing,
    Ground(GroundHit),
    Object { id: T, distance: f32 },
}

fn finite(v: Vec3f) -> bool {
    v.iter().all(|c| c.is_finite())
}

/// Intersects a ray with the horizontal plane `y = ground_y`.
/// Rays that point away from, or run parallel to, the plane do not hit it.
pub fn ray_ground_hit(origin: Vec3f, dir: Vec3f, ground_y: f32) -> Option<GroundHit> {
    if !finite(origin) || !finite(dir) || !ground_y.is_finite() || dir[1].abs() < 1.0e-6 {
        return None;
    }
    let distance = (ground_y - origin[1]) / dir[1];
    if distance < 0.0 {
        return None;
    }
    Some(GroundHit {
        distance,
        x: origin[0] + dir[0] * distance,
        z: origin[2] + dir[2] * distance,
    })
}

/// Slab test against an axis-aligned box. Returns the distance (in multiples of `dir`) at
/// which the ray enters the box, or `0.0` when the origin is already inside it.
pub fn ray_aabb(origin: Vec3f, dir: Vec3f, min: Vec3f, max: Vec3f) -> Option<f32> {
    if !finite(origin) || !finite(dir) || !finite(min) || !finite(max) {
        return None;
    }
    if (0..3).any(|i| min[i] > max[i]) {
        return None;
    }
    let (mut t_enter, mut t_exit) = (0.0_f32, f32::INFINITY);
    for i in 0..3 {
        if dir[i].abs() < 1.0e-8 {
            // Parallel to this slab: the origin must already lie between its planes.
            if origin[i] < min[i] || origin[i] > max[i] {
                return None;
            }
        } else {
            let inv = 1.0 / dir[i];
            let (mut t1, mut t2) = ((min[i] - origin[i]) * inv, (max[i] - origin[i]) * inv);
            if t1 > t2 {
                std::mem::swap(&mut t1, &mut t2);
            }
            t_enter = t_enter.max(t1);
            t_exit = t_exit.min(t2);
            if t_enter > t_exit {
                return None;
            }
        }
    }
    Some(t_enter)
}

/// Decides what a ray points at. The nearest clickable box wins unless the ground is
/// strictly nearer; with no box hit, the ground hit (if any) is returned. Ties between
/// boxes go to the earliest one in iteration order.
pub fn pick<T: Copy>(
    origin: Vec3f,
    dir: Vec3f,
    ground_y: f32,
    boxes: impl IntoIterator<Item = PickBox<T>>,
) -> Pick<T> {
    let ground = ray_ground_hit(origin, dir, ground_y);
    let mut best: Option<(T, f32)> = None;
    for b in boxes {
        if let Some(distance) = ray_aabb(origin, dir, b.min, b.max) {
            if best.is_none_or(|(_, d)| distance < d) {
                best = Some((b.id, distance));
            }
        }
    }
    match (best, ground) {
        (Some((id, distance)), Some(g)) if distance <= g.distance => Pick::Object { id, distance },
        (Some((id, distance)), None) => Pick::Object { id, distance },
        (_, Some(g)) => Pick::Ground(g),
        (None, None) => Pick::Nothing,
    }
}

/// The map tile under a ground point, or `None` when the point is off the map.
pub fn ground_to_tile(map: &WorldMap, x: f32, z: f32) -> Option<TilePos> {
    map.world_to_tile((x, -z)).filter(|pos| map.tile_at(*pos).is_ok())
}

#[cfg(test)]
mod tests {
    use super::*;

    const UNIT_BOX_MIN: Vec3f = [-0.5, 0.0, -0.5];
    const UNIT_BOX_MAX: Vec3f = [0.5, 2.0, 0.5];

    fn approx(a: f32, b: f32) -> bool {
        (a - b).abs() < 1.0e-4
    }

    #[test]
    fn ray_straight_down_hits_the_ground_below() {
        let hit = ray_ground_hit([2.0, 5.0, 3.0], [0.0, -1.0, 0.0], 0.0).unwrap();
        assert!(approx(hit.x, 2.0) && approx(hit.z, 3.0) && approx(hit.distance, 5.0));
    }

    #[test]
    fn oblique_ray_lands_where_geometry_says() {
        let hit = ray_ground_hit([0.0, 4.0, 0.0], [1.0, -1.0, 0.0], 0.0).unwrap();
        assert!(approx(hit.x, 4.0) && approx(hit.z, 0.0));
        let raised = ray_ground_hit([0.0, 4.0, 0.0], [0.0, -1.0, 0.0], 1.0).unwrap();
        assert!(approx(raised.distance, 3.0));
    }

    #[test]
    fn rays_that_cannot_reach_the_ground_miss() {
        assert_eq!(ray_ground_hit([0.0, 4.0, 0.0], [0.0, 1.0, 0.0], 0.0), None, "pointing up");
        assert_eq!(ray_ground_hit([0.0, 4.0, 0.0], [1.0, 0.0, 0.0], 0.0), None, "parallel");
        assert_eq!(ray_ground_hit([f32::NAN, 4.0, 0.0], [0.0, -1.0, 0.0], 0.0), None);
        assert_eq!(ray_ground_hit([0.0, 4.0, 0.0], [0.0, -1.0, 0.0], f32::INFINITY), None);
    }

    #[test]
    fn aabb_front_face_hit_distance() {
        let t = ray_aabb([0.0, 1.0, -5.0], [0.0, 0.0, 1.0], UNIT_BOX_MIN, UNIT_BOX_MAX).unwrap();
        assert!(approx(t, 4.5));
    }

    #[test]
    fn aabb_misses_and_behind_and_parallel_outside() {
        let origin = [0.0, 1.0, -5.0];
        assert_eq!(ray_aabb(origin, [1.0, 0.0, 1.0], UNIT_BOX_MIN, UNIT_BOX_MAX), None, "passes beside");
        assert_eq!(ray_aabb(origin, [0.0, 0.0, -1.0], UNIT_BOX_MIN, UNIT_BOX_MAX), None, "box behind");
        assert_eq!(ray_aabb([3.0, 1.0, -5.0], [0.0, 0.0, 1.0], UNIT_BOX_MIN, UNIT_BOX_MAX), None, "parallel, outside");
    }

    #[test]
    fn aabb_origin_inside_returns_zero_and_bad_input_is_rejected() {
        assert_eq!(ray_aabb([0.0, 1.0, 0.0], [0.0, 0.0, 1.0], UNIT_BOX_MIN, UNIT_BOX_MAX), Some(0.0));
        assert_eq!(ray_aabb([f32::NAN, 1.0, 0.0], [0.0, 0.0, 1.0], UNIT_BOX_MIN, UNIT_BOX_MAX), None);
        assert_eq!(ray_aabb([0.0, 1.0, -5.0], [0.0, 0.0, 1.0], UNIT_BOX_MAX, UNIT_BOX_MIN), None, "inverted box");
    }

    fn boxed(id: u32, x: f32) -> PickBox<u32> {
        PickBox { id, min: [x - 0.5, 0.0, -0.5], max: [x + 0.5, 2.0, 0.5] }
    }

    #[test]
    fn object_in_front_of_the_ground_point_wins() {
        // Camera up and behind, looking at the box's base: the ray enters the box before the ground.
        let pick = pick([0.0, 5.0, 5.0], [0.0, -1.0, -1.0], 0.0, [boxed(7, 0.0)]);
        assert!(matches!(pick, Pick::Object { id: 7, .. }), "{pick:?}");
    }

    #[test]
    fn ground_wins_when_the_ray_passes_beside_objects() {
        let pick = pick([5.0, 5.0, 5.0], [0.0, -1.0, 0.0], 0.0, [boxed(7, 0.0)]);
        assert!(matches!(pick, Pick::Ground(g) if approx(g.x, 5.0) && approx(g.z, 5.0)), "{pick:?}");
    }

    #[test]
    fn nearest_of_two_objects_wins_and_ties_go_to_the_first() {
        let near_first = pick([0.0, 1.0, 9.0], [0.0, 0.0, -1.0], 0.0, [boxed(1, 0.0), boxed(2, 0.0)]);
        assert!(matches!(near_first, Pick::Object { id: 1, .. }));
        let behind = PickBox { id: 3, min: [-0.5, 0.0, -4.5], max: [0.5, 2.0, -3.5] };
        let front = PickBox { id: 4, min: [-0.5, 0.0, 3.5], max: [0.5, 2.0, 4.5] };
        let nearest = pick([0.0, 1.0, 9.0], [0.0, 0.0, -1.0], 0.0, [behind, front]);
        assert!(matches!(nearest, Pick::Object { id: 4, .. }));
    }

    #[test]
    fn object_is_still_picked_when_the_ray_never_meets_the_ground() {
        let level = pick([0.0, 1.0, 9.0], [0.0, 0.0, -1.0], 0.0, [boxed(5, 0.0)]);
        assert!(matches!(level, Pick::Object { id: 5, .. }));
    }

    #[test]
    fn sky_clicks_pick_nothing() {
        let none: Pick<u32> = pick([0.0, 5.0, 0.0], [0.0, 1.0, 0.0], 0.0, []);
        assert_eq!(none, Pick::Nothing);
    }

    #[test]
    fn ground_points_map_back_to_tiles_and_off_map_is_none() {
        let map = crate::scene::meadow_map().unwrap();
        for y in 0..map.height() as i32 {
            for x in 0..map.width() as i32 {
                let pos = TilePos::new(x, y);
                let c = map.tile_center_3d(pos, 0.0);
                assert_eq!(ground_to_tile(&map, c[0], c[2]), Some(pos));
            }
        }
        assert_eq!(ground_to_tile(&map, 1.0e4, 0.0), None);
        assert_eq!(ground_to_tile(&map, 0.0, -1.0e4), None);
        assert_eq!(ground_to_tile(&map, f32::NAN, 0.0), None);
    }
}
