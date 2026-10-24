//! Stitchable terrain meshes: the ground of one scene as a handful of merged meshes instead of
//! one entity per tile.
//!
//! Pure Rust (no Bevy): [`build_terrain`] returns plain vertex and index buffers, and the Bevy
//! layer turns them into `Mesh` assets. A scene's terrain is split into one *part* per ground
//! material, so the look is the same as when every tile was its own slab.
//!
//! # Stitching
//!
//! Vertices are written in the map's own frame (`WorldMap::tile_center_3d`), which for the open
//! world is the frame of the whole world, so two neighbouring scenes put their shared edge on
//! exactly the same coordinates. Where two tiles of different height meet, the **higher tile
//! owns the wall between them**: it draws a wall down to the lower tile's surface, and the lower
//! tile draws nothing there. Whichever scene the higher tile belongs to draws it, so a seam
//! between two scenes never has a gap and never has the wall twice, whatever order the scenes
//! load in. A tile's side that faces nothing at all (the edge of the world) is drawn down to the
//! bottom of the slab, so the world never looks hollow. Tiles of equal height draw no wall.
//!
//! What lies beyond a scene's own tiles is asked of a `neighbour` function (the true tile of
//! the neighbouring scene, loaded or not), because the map the game holds has placeholders where
//! scenes are not loaded.

use crate::scene;
use crate::world::{Tile, TilePos, WorldMap};

/// How thick a ground slab is: a wall with nothing beside it reaches this far below the surface.
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

/// How a tile is drawn as ground: its material and the height of its surface. `bridge` is true
/// for a path tile that crosses water (the river keeps flowing under the deck).
pub fn tile_style(tile: Tile, pos: TilePos, bridge: bool) -> (TerrainMaterial, f32) {
    match tile {
        Tile::Grass | Tile::Rock | Tile::Fence | Tile::Sign | Tile::Wall | Tile::Door | Tile::Floor | Tile::Garden | Tile::Vine => {
            (TerrainMaterial::Grass(scene::tile_variant(pos, 1, 3)), 0.0)
        }
        Tile::Tree => (TerrainMaterial::DarkGrass, 0.0),
        Tile::Path if bridge => (TerrainMaterial::Water(scene::tile_variant(pos, 3, 2)), -0.2),
        Tile::Path => (TerrainMaterial::Path(scene::tile_variant(pos, 2, 2)), -0.02),
        Tile::Water => (TerrainMaterial::Water(scene::tile_variant(pos, 3, 2)), -0.2),
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
    fn quad(&mut self, corners: [[f32; 3]; 4], normal: [f32; 3]) {
        let first = self.positions.len() as u32;
        self.positions.extend(corners);
        self.normals.extend([normal; 4]);
        self.indices.extend([first, first + 1, first + 2, first, first + 2, first + 3]);
    }

    /// Number of quads (each tile top and each wall is one).
    pub fn quad_count(&self) -> usize {
        self.positions.len() / 4
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

    pub fn quad_count(&self) -> usize {
        self.parts.iter().map(|(_, p)| p.quad_count()).sum()
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
    for y in rect.y0..rect.y1 {
        for x in rect.x0..rect.x1 {
            let pos = TilePos::new(x, y);
            let Ok(tile) = map.tile_at(pos) else { continue };
            let (material, top) = tile_style(tile, pos, bridges(pos));
            let slot = match parts.iter().position(|(m, _)| *m == material) {
                Some(slot) => slot,
                None => {
                    parts.push((material, TerrainPart::default()));
                    parts.len() - 1
                }
            };
            let part = &mut parts[slot].1;
            let [cx, _, cz] = map.tile_center_3d(pos, top);
            let (x0, x1, z0, z1) = (cx - 0.5, cx + 0.5, cz - 0.5, cz + 0.5);
            // Top face, normal up. Corners run counter-clockwise seen from above (+z is south).
            part.quad([[x0, top, z1], [x1, top, z1], [x1, top, z0], [x0, top, z0]], [0.0, 1.0, 0.0]);
            // Walls: toward +x, -x, +z (south), -z (north). Map +y is -z.
            for (dx, dy) in [(1, 0), (-1, 0), (0, -1), (0, 1)] {
                let other = TilePos::new(x + dx, y + dy);
                let beside = if inside(other) {
                    map.tile_at(other).ok().map(|t| (t, bridges(other)))
                } else {
                    neighbour(other)
                };
                let floor = top - SLAB_THICKNESS;
                let down_to = match beside {
                    Some((t, bridge)) => {
                        let other_top = tile_style(t, other, bridge).1;
                        // Equal or higher beside us: the neighbour draws it (or nothing shows).
                        if other_top >= top {
                            continue;
                        }
                        other_top.max(floor)
                    }
                    None => floor,
                };
                let (normal, corners) = match (dx, dy) {
                    (1, 0) => ([1.0, 0.0, 0.0], [[x1, top, z1], [x1, down_to, z1], [x1, down_to, z0], [x1, top, z0]]),
                    (-1, 0) => ([-1.0, 0.0, 0.0], [[x0, top, z0], [x0, down_to, z0], [x0, down_to, z1], [x0, top, z1]]),
                    (0, -1) => ([0.0, 0.0, 1.0], [[x0, top, z1], [x0, down_to, z1], [x1, down_to, z1], [x1, top, z1]]),
                    _ => ([0.0, 0.0, -1.0], [[x1, top, z0], [x1, down_to, z0], [x0, down_to, z0], [x0, top, z0]]),
                };
                part.quad(corners, normal);
            }
        }
    }
    parts.sort_by_key(|(m, _)| TerrainMaterial::ALL.iter().position(|a| a == m).unwrap_or(usize::MAX));
    TerrainMesh { scene: scene_id.to_string(), parts }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

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

    /// Every quad as four rounded corners plus its normal, grouped by material, sorted.
    fn quads(mesh: &TerrainMesh) -> BTreeMap<TerrainMaterial, Vec<String>> {
        let mut out: BTreeMap<TerrainMaterial, Vec<String>> = BTreeMap::new();
        for (material, part) in &mesh.parts {
            let list = out.entry(*material).or_default();
            for q in 0..part.quad_count() {
                let corners: Vec<String> = part.positions[q * 4..q * 4 + 4].iter().map(|p| format!("{:.3},{:.3},{:.3}", p[0], p[1], p[2])).collect();
                list.push(format!("{} n{:?}", corners.join(" "), part.normals[q * 4]));
            }
            list.sort();
        }
        out
    }

    fn tops(mesh: &TerrainMesh) -> usize {
        mesh.parts.iter().map(|(_, p)| (0..p.quad_count()).filter(|q| p.normals[q * 4] == [0.0, 1.0, 0.0]).count()).sum()
    }

    #[test]
    fn every_tile_gets_exactly_one_top_face() {
        let m = map("..~~\n.==.\nT##.\n");
        let mesh = build_terrain("A", &m, whole(&m), &no_bridges, &nothing);
        assert_eq!(tops(&mesh), 12);
    }

    #[test]
    fn a_flat_meadow_is_walled_only_at_its_outer_edge() {
        let m = map("...\n...\n...\n");
        let mesh = build_terrain("A", &m, whole(&m), &no_bridges, &nothing);
        // 9 tops plus the 12 outer sides of the 3 x 3 block.
        assert_eq!(mesh.quad_count(), 9 + 12);
    }

    #[test]
    fn the_higher_tile_owns_the_wall_to_lower_water() {
        let m = map(".~\n");
        let mesh = build_terrain("A", &m, whole(&m), &no_bridges, &nothing);
        let grass = mesh.parts.iter().find(|(mat, _)| matches!(mat, TerrainMaterial::Grass(_))).unwrap();
        let water = mesh.parts.iter().find(|(mat, _)| matches!(mat, TerrainMaterial::Water(_))).unwrap();
        // Grass: top + three outer walls + the wall toward the water; water: top + three outer walls.
        assert_eq!(grass.1.quad_count(), 5);
        assert_eq!(water.1.quad_count(), 4);
        // The wall toward the water only drops as far as the water surface (0.2), not the slab bottom.
        let lowest = grass.1.positions.iter().map(|p| p[1]).fold(f32::MAX, f32::min);
        assert!((lowest - -0.5).abs() < 1e-6, "outer walls still reach the slab bottom");
        let toward_water: Vec<&[f32; 3]> = grass.1.positions.iter().zip(&grass.1.normals).filter(|(_, n)| **n == [1.0, 0.0, 0.0]).map(|(p, _)| p).collect();
        assert!(toward_water.iter().all(|p| p[1] >= -0.2 - 1e-6));
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
    fn triangles_face_the_way_their_normal_says() {
        let m = map(".~\n~.\n");
        let mesh = build_terrain("A", &m, whole(&m), &no_bridges, &nothing);
        for (_, part) in &mesh.parts {
            for tri in part.indices.chunks(3) {
                let [a, b, c] = [tri[0], tri[1], tri[2]].map(|i| part.positions[i as usize]);
                let (u, v) = ([b[0] - a[0], b[1] - a[1], b[2] - a[2]], [c[0] - a[0], c[1] - a[1], c[2] - a[2]]);
                let cross = [u[1] * v[2] - u[2] * v[1], u[2] * v[0] - u[0] * v[2], u[0] * v[1] - u[1] * v[0]];
                let n = part.normals[tri[0] as usize];
                let dot = cross[0] * n[0] + cross[1] * n[1] + cross[2] * n[2];
                assert!(dot > 0.0, "triangle {a:?} {b:?} {c:?} faces away from its normal {n:?}");
            }
        }
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
    /// exactly the quads that one scene covering both would make.
    fn stitched_equals_whole(rows: &str, split: i32) {
        let m = map(rows);
        let (w, h) = (m.width() as i32, m.height() as i32);
        let at = |p: TilePos| m.tile_at(p).ok().map(|t| (t, false));
        let left = Rect { x0: 0, y0: 0, x1: split, y1: h };
        let right = Rect { x0: split, y0: 0, x1: w, y1: h };
        let a = build_terrain("A", &m, left, &no_bridges, &at);
        let b = build_terrain("B", &m, right, &no_bridges, &at);
        let both = build_terrain("AB", &m, whole(&m), &no_bridges, &nothing);
        let mut joined = quads(&a);
        for (material, list) in quads(&b) {
            joined.entry(material).or_default().extend(list);
        }
        for list in joined.values_mut() {
            list.sort();
        }
        assert_eq!(joined, quads(&both));
    }

    #[test]
    fn two_scenes_side_by_side_make_exactly_the_quads_of_one_scene() {
        let rows = "..~~..\n.==~T.\n#.~~..\n..=~~.\n";
        for split in 1..6 {
            stitched_equals_whole(rows, split);
        }
    }

    #[test]
    fn a_wall_on_a_seam_is_drawn_once_by_the_higher_side_whichever_side_that_is() {
        // Grass on the left of the seam and water on the right, then the other way round.
        stitched_equals_whole(".~\n.~\n", 1);
        stitched_equals_whole("~.\n~.\n", 1);
    }

    #[test]
    fn a_scene_beside_nothing_draws_a_full_height_wall() {
        let m = map("..\n");
        let right = Rect { x0: 1, y0: 0, x1: 2, y1: 1 };
        let alone = build_terrain("B", &m, right, &no_bridges, &nothing);
        let beside_grass = build_terrain("B", &m, right, &no_bridges, &|p| m.tile_at(p).ok().map(|t| (t, false)));
        assert_eq!(alone.quad_count(), 5, "top plus four walls");
        assert_eq!(beside_grass.quad_count(), 4, "the wall toward an equal-height neighbour is not drawn");
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
        assert_eq!(tops(&mesh), 2);
    }
}
