//! Stitchable, triangulated terrain: the ground of one scene as a handful of merged meshes
//! instead of one entity per tile.
//!
//! Pure Rust (no Bevy): [`build_terrain`] returns plain vertex and index buffers, and the Bevy
//! layer turns them into `Mesh` assets. A scene's terrain is split into one *part* per ground
//! material, so the look is the same as when every tile was its own slab.
//!
//! # Triangles and heights
//!
//! Every tile is two triangles over its four corners, and a corner's height comes from
//! [`crate::heightfield::vertex_height`]: rolling hills that stay flat under paths, water, and
//! buildings (see that module for the rule). Triangles are flat-shaded, so each carries its own
//! normal and the ground reads as faceted low-poly. The diagonal alternates by tile parity
//! (`heightfield::splits_south_west_to_north_east`), so hills do not lean one way. Tiles keep the
//! material of their kind (grass shade, path, water), as before.
//!
//! # Stitching
//!
//! Vertices are written in the map's own frame (`WorldMap::tile_center_3d`), which for the open
//! world is the frame of the whole world, so two neighbouring scenes put their shared edge on
//! exactly the same coordinates. A corner's height is a pure function of the tiles around it,
//! asked of a `neighbour` function (the true tile of the neighbouring scene, loaded or not,
//! because the map the game holds has placeholders where scenes are not loaded), so both scenes
//! compute the same height for every corner on the seam and the surface has no cracks and no
//! steps, whichever order scenes load in. Nothing is drawn between two tiles that both exist; a
//! tile's side that faces nothing at all (the edge of the world) gets a skirt down below the
//! lowest corner of that edge, so the world never looks hollow.

use crate::heightfield::{splits_south_west_to_north_east, vertex_height};
use crate::scene;
use crate::world::{Tile, TilePos, WorldMap};

/// How far a skirt at the edge of the world reaches below the lowest corner of that edge.
pub const SLAB_THICKNESS: f32 = 0.5;

/// Which ground material a tile uses; the index picks the shade (see `scene::tile_variant`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum TerrainMaterial {
    Grass(usize),
    DarkGrass,
    Path(usize),
    Water(usize),
}

impl TerrainMaterial {
    /// Every material a map can use, in a fixed order (parts come out in this order).
    pub const ALL: [TerrainMaterial; 9] = [
        Self::Grass(0),
        Self::Grass(1),
        Self::Grass(2),
        Self::DarkGrass,
        Self::Path(0),
        Self::Path(1),
        Self::Water(0),
        Self::Water(1),
        Self::Water(2),
    ];
}

/// The ground material of a tile. `bridge` is true for a path tile that crosses water (the
/// river keeps flowing under the deck).
pub fn tile_material(tile: Tile, pos: TilePos, bridge: bool) -> TerrainMaterial {
    match tile {
        Tile::Grass | Tile::Rock | Tile::Fence | Tile::Sign | Tile::Wall | Tile::Door | Tile::Floor | Tile::Garden | Tile::Vine => TerrainMaterial::Grass(scene::tile_variant(pos, 1, 3)),
        Tile::Tree => TerrainMaterial::DarkGrass,
        Tile::Path if bridge => TerrainMaterial::Water(scene::tile_variant(pos, 3, 2)),
        Tile::Path => TerrainMaterial::Path(scene::tile_variant(pos, 2, 2)),
        Tile::Water => TerrainMaterial::Water(scene::tile_variant(pos, 3, 2)),
    }
}

/// The merged geometry of one material. Triangles are counter-clockwise seen from outside.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct TerrainPart {
    pub positions: Vec<[f32; 3]>,
    pub normals: Vec<[f32; 3]>,
    pub indices: Vec<u32>,
}

impl TerrainPart {
    /// One flat-shaded triangle: three corners (counter-clockwise from outside) and the normal
    /// they make.
    fn triangle(&mut self, corners: [[f32; 3]; 3]) {
        let [a, b, c] = corners;
        let (u, v) = ([b[0] - a[0], b[1] - a[1], b[2] - a[2]], [c[0] - a[0], c[1] - a[1], c[2] - a[2]]);
        let cross = [u[1] * v[2] - u[2] * v[1], u[2] * v[0] - u[0] * v[2], u[0] * v[1] - u[1] * v[0]];
        let length = (cross[0] * cross[0] + cross[1] * cross[1] + cross[2] * cross[2]).sqrt();
        let normal = if length > 1.0e-9 { [cross[0] / length, cross[1] / length, cross[2] / length] } else { [0.0, 1.0, 0.0] };
        let first = self.positions.len() as u32;
        self.positions.extend(corners);
        self.normals.extend([normal; 3]);
        self.indices.extend([first, first + 1, first + 2]);
    }

    /// A skirt: two triangles over four corners with one shared normal.
    fn wall(&mut self, corners: [[f32; 3]; 4], normal: [f32; 3]) {
        let first = self.positions.len() as u32;
        self.positions.extend(corners);
        self.normals.extend([normal; 4]);
        self.indices.extend([first, first + 1, first + 2, first, first + 2, first + 3]);
    }

    /// Number of triangles (two per tile, plus the skirts at the edge of the world).
    pub fn triangle_count(&self) -> usize {
        self.indices.len() / 3
    }
}

/// The terrain of one scene: which scene it belongs to, and its parts.
#[derive(Debug, Clone, PartialEq)]
pub struct TerrainMesh {
    /// The location id of the scene (chunk or region) this ground belongs to.
    pub scene: String,
    /// One entry per material that is used at all, in [`TerrainMaterial::ALL`] order.
    pub parts: Vec<(TerrainMaterial, TerrainPart)>,
}

impl TerrainMesh {
    pub fn part(&self, material: TerrainMaterial) -> Option<&TerrainPart> {
        self.parts.iter().find(|(m, _)| *m == material).map(|(_, p)| p)
    }

    pub fn triangle_count(&self) -> usize {
        self.parts.iter().map(|(_, p)| p.triangle_count()).sum()
    }

    pub fn is_empty(&self) -> bool {
        self.parts.is_empty()
    }
}

/// A rectangle of tiles: columns `x0..x1`, rows `y0..y1`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rect {
    pub x0: i32,
    pub y0: i32,
    pub x1: i32,
    pub y1: i32,
}

/// What stands beside a scene's own tiles, as far as the ground is concerned: the tile and
/// whether it is a bridge deck, or `None` where there is nothing (the edge of the world).
pub type Neighbour<'a> = &'a dyn Fn(TilePos) -> Option<(Tile, bool)>;

/// Builds the terrain of the tiles in `rect` of `map`. `bridges` says which path tiles are
/// bridge decks (inside the rectangle). Tiles outside `rect` are asked of `neighbour`.
pub fn build_terrain(scene_id: &str, map: &WorldMap, rect: Rect, bridges: &dyn Fn(TilePos) -> bool, neighbour: Neighbour) -> TerrainMesh {
    let mut parts: Vec<(TerrainMaterial, TerrainPart)> = Vec::new();
    let inside = |p: TilePos| p.x >= rect.x0 && p.x < rect.x1 && p.y >= rect.y0 && p.y < rect.y1;
    let lookup = |p: TilePos| -> Option<(Tile, bool)> {
        if inside(p) {
            map.tile_at(p).ok().map(|t| (t, bridges(p)))
        } else {
            neighbour(p)
        }
    };
    for y in rect.y0..rect.y1 {
        for x in rect.x0..rect.x1 {
            let pos = TilePos::new(x, y);
            let Ok(tile) = map.tile_at(pos) else { continue };
            let material = tile_material(tile, pos, bridges(pos));
            let slot = match parts.iter().position(|(m, _)| *m == material) {
                Some(slot) => slot,
                None => {
                    parts.push((material, TerrainPart::default()));
                    parts.len() - 1
                }
            };
            let part = &mut parts[slot].1;
            let [cx, _, cz] = map.tile_center_3d(pos, 0.0);
            let (x0, x1, z0, z1) = (cx - 0.5, cx + 0.5, cz - 0.5, cz + 0.5);
            let h = |i: i32, j: i32| vertex_height(&lookup, i, j);
            let (h_sw, h_se, h_ne, h_nw) = (h(x, y), h(x + 1, y), h(x + 1, y + 1), h(x, y + 1));
            // Corners run counter-clockwise seen from above (+z is south, so map north is -z).
            let (sw, se, ne, nw) = ([x0, h_sw, z1], [x1, h_se, z1], [x1, h_ne, z0], [x0, h_nw, z0]);
            if splits_south_west_to_north_east(pos) {
                part.triangle([sw, se, ne]);
                part.triangle([sw, ne, nw]);
            } else {
                part.triangle([sw, se, nw]);
                part.triangle([se, ne, nw]);
            }
            // A skirt on each side that faces nothing at all (the edge of the world).
            for (dx, dy) in [(1, 0), (-1, 0), (0, -1), (0, 1)] {
                if lookup(TilePos::new(x + dx, y + dy)).is_some() {
                    continue;
                }
                let (normal, top_a, top_b) = match (dx, dy) {
                    (1, 0) => ([1.0, 0.0, 0.0], h_se, h_ne),
                    (-1, 0) => ([-1.0, 0.0, 0.0], h_nw, h_sw),
                    (0, -1) => ([0.0, 0.0, 1.0], h_sw, h_se),
                    _ => ([0.0, 0.0, -1.0], h_ne, h_nw),
                };
                let d = top_a.min(top_b) - SLAB_THICKNESS;
                let corners = match (dx, dy) {
                    (1, 0) => [[x1, top_a, z1], [x1, d, z1], [x1, d, z0], [x1, top_b, z0]],
                    (-1, 0) => [[x0, top_a, z0], [x0, d, z0], [x0, d, z1], [x0, top_b, z1]],
                    (0, -1) => [[x0, top_a, z1], [x0, d, z1], [x1, d, z1], [x1, top_b, z1]],
                    _ => [[x1, top_a, z0], [x1, d, z0], [x0, d, z0], [x0, top_b, z0]],
                };
                part.wall(corners, normal);
            }
        }
    }
    parts.sort_by_key(|(m, _)| TerrainMaterial::ALL.iter().position(|a| a == m).unwrap_or(usize::MAX));
    TerrainMesh { scene: scene_id.to_string(), parts }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::heightfield::{vertex_height, WATER_LEVEL};
    use std::collections::{BTreeMap, BTreeSet};

    fn map(rows: &str) -> WorldMap {
        WorldMap::from_ascii(rows).unwrap()
    }

    fn no_bridges(_: TilePos) -> bool {
        false
    }

    fn whole(m: &WorldMap) -> Rect {
        Rect { x0: 0, y0: 0, x1: m.width() as i32, y1: m.height() as i32 }
    }

    fn nothing(_: TilePos) -> Option<(Tile, bool)> {
        None
    }

    fn rounded(p: &[f32; 3]) -> String {
        format!("{:.3},{:.3},{:.3}", p[0], p[1], p[2])
    }

    /// Every triangle as three rounded corners, grouped by material, sorted.
    fn triangles(mesh: &TerrainMesh) -> BTreeMap<TerrainMaterial, Vec<String>> {
        let mut out: BTreeMap<TerrainMaterial, Vec<String>> = BTreeMap::new();
        for (material, part) in &mesh.parts {
            let list = out.entry(*material).or_default();
            for tri in part.indices.chunks(3) {
                list.push(tri.iter().map(|i| rounded(&part.positions[*i as usize])).collect::<Vec<_>>().join(" "));
            }
            list.sort();
        }
        out
    }

    fn faces_up(part: &TerrainPart, tri: &[u32]) -> bool {
        part.normals[tri[0] as usize][1] > 0.0
    }

    fn tops(mesh: &TerrainMesh) -> usize {
        mesh.parts.iter().map(|(_, p)| p.indices.chunks(3).filter(|t| faces_up(p, t)).count()).sum()
    }

    #[test]
    fn every_tile_is_exactly_two_upward_triangles() {
        let m = map("..~~\n.==.\nT##.\n");
        let mesh = build_terrain("A", &m, whole(&m), &no_bridges, &nothing);
        assert_eq!(tops(&mesh), 24);
    }

    #[test]
    fn a_meadow_is_skirted_only_at_its_outer_edge() {
        let m = map("...\n...\n...\n");
        let mesh = build_terrain("A", &m, whole(&m), &no_bridges, &nothing);
        // 9 tiles of two triangles, plus 12 outer sides of two triangles each.
        assert_eq!(mesh.triangle_count(), 18 + 24);
    }

    #[test]
    fn the_surface_is_one_height_at_every_shared_corner() {
        let m = map("..~~..\n.==~T.\n#.~~..\n..=~~.\n");
        let mesh = build_terrain("A", &m, whole(&m), &no_bridges, &nothing);
        let mut heights: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
        for (_, part) in &mesh.parts {
            for tri in part.indices.chunks(3).filter(|t| faces_up(part, t)) {
                for i in tri {
                    let p = part.positions[*i as usize];
                    heights.entry(format!("{:.3},{:.3}", p[0], p[2])).or_default().insert(format!("{:.4}", p[1]));
                }
            }
        }
        assert_eq!(heights.len(), 7 * 5, "one corner per (columns + 1) x (rows + 1)");
        assert!(heights.values().all(|ys| ys.len() == 1), "a crack: {:?}", heights.iter().find(|(_, ys)| ys.len() > 1));
    }

    #[test]
    fn the_ground_rolls_where_nothing_is_built_and_is_flat_under_water_and_paths() {
        let grass = map("................\n................\n................\n................\n................\n................\n................\n................\n");
        let mesh = build_terrain("A", &grass, whole(&grass), &no_bridges, &nothing);
        let ys: Vec<f32> = mesh.parts.iter().flat_map(|(_, p)| p.indices.chunks(3).filter(|t| faces_up(p, t)).flat_map(|t| t.iter().map(|i| p.positions[*i as usize][1])).collect::<Vec<_>>()).collect();
        let (low, high) = ys.iter().fold((f32::MAX, f32::MIN), |(lo, hi), v| (lo.min(*v), hi.max(*v)));
        assert!(high - low > 0.1, "open grass is not flat ({low}..{high})");
        let lake = map("~~~\n~~~\n~~~\n");
        let water = build_terrain("A", &lake, whole(&lake), &no_bridges, &nothing);
        let tops: Vec<f32> = water.parts.iter().flat_map(|(_, p)| p.indices.chunks(3).filter(|t| faces_up(p, t)).flat_map(|t| t.iter().map(|i| p.positions[*i as usize][1])).collect::<Vec<_>>()).collect();
        assert!(tops.iter().all(|y| (*y - WATER_LEVEL).abs() < 1e-6), "water lies at its level");
    }

    #[test]
    fn output_is_deterministic_and_parts_follow_the_fixed_material_order() {
        let m = map("..~~\n.==.\nT##.\n");
        let a = build_terrain("A", &m, whole(&m), &no_bridges, &nothing);
        let b = build_terrain("A", &m, whole(&m), &no_bridges, &nothing);
        assert_eq!(a, b);
        let order: Vec<usize> = a.parts.iter().map(|(mat, _)| TerrainMaterial::ALL.iter().position(|x| x == mat).unwrap()).collect();
        assert!(order.windows(2).all(|w| w[0] < w[1]), "{order:?}");
    }

    #[test]
    fn indices_stay_inside_the_vertex_list_and_form_whole_triangles() {
        let m = map("..~~\n.==.\nT##.\n");
        let mesh = build_terrain("A", &m, whole(&m), &no_bridges, &nothing);
        for (_, part) in &mesh.parts {
            assert_eq!(part.positions.len(), part.normals.len());
            assert_eq!(part.indices.len() % 3, 0);
            assert!(part.indices.iter().all(|i| (*i as usize) < part.positions.len()));
        }
    }

    #[test]
    fn triangles_face_the_way_their_normal_says_and_normals_are_unit_length() {
        let m = map(".~\n~.\n..\n");
        let mesh = build_terrain("A", &m, whole(&m), &no_bridges, &nothing);
        for (_, part) in &mesh.parts {
            for tri in part.indices.chunks(3) {
                let [a, b, c] = [tri[0], tri[1], tri[2]].map(|i| part.positions[i as usize]);
                let (u, v) = ([b[0] - a[0], b[1] - a[1], b[2] - a[2]], [c[0] - a[0], c[1] - a[1], c[2] - a[2]]);
                let cross = [u[1] * v[2] - u[2] * v[1], u[2] * v[0] - u[0] * v[2], u[0] * v[1] - u[1] * v[0]];
                let n = part.normals[tri[0] as usize];
                assert!((n[0] * n[0] + n[1] * n[1] + n[2] * n[2] - 1.0).abs() < 1e-4, "{n:?}");
                let dot = cross[0] * n[0] + cross[1] * n[1] + cross[2] * n[2];
                assert!(dot > 0.0, "triangle {a:?} {b:?} {c:?} faces away from its normal {n:?}");
            }
        }
    }

    #[test]
    fn a_tile_top_is_flat_shaded_with_its_own_slope() {
        let m = map("....\n....\n....\n....\n");
        let mesh = build_terrain("A", &m, whole(&m), &no_bridges, &nothing);
        let tilted = mesh.parts.iter().flat_map(|(_, p)| p.indices.chunks(3).filter(|t| faces_up(p, t)).map(|t| p.normals[t[0] as usize])).filter(|n| n[1] < 0.9999).count();
        assert!(tilted > 0, "hills tilt some triangles away from straight up");
    }

    #[test]
    fn a_bridge_deck_is_drawn_as_water_height_ground() {
        let m = map("~=~\n");
        let flat = build_terrain("A", &m, whole(&m), &no_bridges, &nothing);
        let bridged = build_terrain("A", &m, whole(&m), &|p| p == TilePos::new(1, 0), &nothing);
        assert!(flat.parts.iter().any(|(mat, _)| matches!(mat, TerrainMaterial::Path(_))));
        assert!(!bridged.parts.iter().any(|(mat, _)| matches!(mat, TerrainMaterial::Path(_))));
    }

    /// The seam rule: two scenes built separately, each asking the other for its tiles, make
    /// exactly the triangles that one scene covering both would make.
    fn stitched_equals_whole(rows: &str, split: i32) {
        let m = map(rows);
        let (w, h) = (m.width() as i32, m.height() as i32);
        let at = |p: TilePos| m.tile_at(p).ok().map(|t| (t, false));
        let left = Rect { x0: 0, y0: 0, x1: split, y1: h };
        let right = Rect { x0: split, y0: 0, x1: w, y1: h };
        let a = build_terrain("A", &m, left, &no_bridges, &at);
        let b = build_terrain("B", &m, right, &no_bridges, &at);
        let both = build_terrain("AB", &m, whole(&m), &no_bridges, &nothing);
        let mut joined = triangles(&a);
        for (material, list) in triangles(&b) {
            joined.entry(material).or_default().extend(list);
        }
        for list in joined.values_mut() {
            list.sort();
        }
        assert_eq!(joined, triangles(&both));
    }

    #[test]
    fn two_scenes_side_by_side_make_exactly_the_triangles_of_one_scene() {
        let rows = "..~~..\n.==~T.\n#.~~..\n..=~~.\n";
        for split in 1..6 {
            stitched_equals_whole(rows, split);
        }
    }

    #[test]
    fn a_seam_between_water_and_grass_has_no_crack_whichever_side_is_which() {
        stitched_equals_whole(".~\n.~\n", 1);
        stitched_equals_whole("~.\n~.\n", 1);
        stitched_equals_whole("....\n....\n....\n", 2);
    }

    #[test]
    fn corners_on_a_seam_have_the_height_the_world_gives_them() {
        let rows = "..~~..\n.==~T.\n#.~~..\n..=~~.\n";
        let m = map(rows);
        let at = |p: TilePos| m.tile_at(p).ok().map(|t| (t, false));
        let left = build_terrain("A", &m, Rect { x0: 0, y0: 0, x1: 3, y1: 4 }, &no_bridges, &at);
        for (_, part) in &left.parts {
            for tri in part.indices.chunks(3).filter(|t| faces_up(part, t)) {
                for i in tri {
                    let p = part.positions[*i as usize];
                    // Back to tile corners: world x = i - width / 2, z = -(j - height / 2).
                    let (ci, cj) = ((p[0] + 3.0).round() as i32, (-p[2] + 2.0).round() as i32);
                    assert!((p[1] - vertex_height(&at, ci, cj)).abs() < 1e-6, "corner ({ci}, {cj})");
                }
            }
        }
    }

    #[test]
    fn a_scene_beside_nothing_gets_skirts_and_beside_a_neighbour_does_not() {
        let m = map("..\n");
        let right = Rect { x0: 1, y0: 0, x1: 2, y1: 1 };
        let alone = build_terrain("B", &m, right, &no_bridges, &nothing);
        let beside_grass = build_terrain("B", &m, right, &no_bridges, &|p| m.tile_at(p).ok().map(|t| (t, false)));
        assert_eq!(alone.triangle_count(), 2 + 4 * 2, "top plus four skirts");
        assert_eq!(beside_grass.triangle_count(), 2 + 3 * 2, "no skirt toward a neighbour that exists");
    }

    #[test]
    fn skirts_reach_below_the_lowest_corner_of_their_edge() {
        let m = map("~.\n");
        let mesh = build_terrain("A", &m, whole(&m), &no_bridges, &nothing);
        for (_, part) in &mesh.parts {
            for tri in part.indices.chunks(3).filter(|t| !faces_up(part, t)) {
                for i in tri {
                    assert!(part.positions[*i as usize][1] >= WATER_LEVEL - SLAB_THICKNESS - 1e-6);
                }
            }
        }
        let lowest = mesh.parts.iter().flat_map(|(_, p)| p.positions.iter().map(|v| v[1])).fold(f32::MAX, f32::min);
        assert!((lowest - (WATER_LEVEL - SLAB_THICKNESS)).abs() < 1e-5, "the world is not hollow: {lowest}");
    }

    #[test]
    fn the_scene_name_is_carried() {
        let m = map(".\n");
        assert_eq!(build_terrain("YELLOW_BRICK_ROAD_EAST", &m, whole(&m), &no_bridges, &nothing).scene, "YELLOW_BRICK_ROAD_EAST");
    }

    #[test]
    fn an_empty_rectangle_makes_an_empty_mesh() {
        let m = map(".\n");
        assert!(build_terrain("A", &m, Rect { x0: 0, y0: 0, x1: 0, y1: 0 }, &no_bridges, &nothing).is_empty());
    }

    #[test]
    fn a_rectangle_partly_off_the_map_only_covers_the_map() {
        let m = map("..\n");
        let mesh = build_terrain("A", &m, Rect { x0: -3, y0: -3, x1: 5, y1: 5 }, &no_bridges, &nothing);
        assert_eq!(tops(&mesh), 4);
    }
}
