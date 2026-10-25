//! Geometry export as Wavefront OBJ (with an MTL file for colours).
//!
//! OBJ is the most open and best supported exchange format for plain triangle meshes: it is
//! text, documented since the 1990s, and read by Blender, Maya, MeshLab, Godot, Unity, Unreal,
//! three.js and every other 3D tool, with no library needed to write it. (FBX is proprietary and
//! binary; glTF is also open but carries a scene description this game does not need yet.)
//!
//! Pure Rust (no Bevy): the Bevy layer gathers the triangles of a model, and [`build_obj`] turns
//! them into text. The result is deterministic, so the same model always exports the same bytes.
//!
//! # What the file says
//!
//! * Right-handed, **Y up**, one unit = one map tile, the model's own origin at its foot (the
//!   frame the game builds the model in: facing as spawned, scale 1).
//! * One `o` object for the model and one `g` group per mesh part, each with its `usemtl` colour.
//! * Positions (`v`), normals (`vn`) when the part has them, and triangles (`f v//vn ...`) with
//!   1-based indices that run across the whole file.
//! * No texture coordinates: the game's models are flat-coloured.
//! * The `.mtl` file holds one `newmtl` per distinct colour with its diffuse colour `Kd`
//!   (sRGB, as modelling tools expect) and `d` (opacity) when not fully opaque.

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::path::{Path, PathBuf};

/// A 4 x 4 transform, column-major (the layout of `Mat4::to_cols_array`).
pub type Matrix = [f32; 16];

/// The identity transform.
pub const IDENTITY: Matrix = [1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0];

/// One mesh of a model, with the transform that puts it in the model's frame.
#[derive(Debug, Clone, PartialEq)]
pub struct Part {
    pub positions: Vec<[f32; 3]>,
    /// One per position, or empty for none.
    pub normals: Vec<[f32; 3]>,
    /// Triangle corners, three per triangle, indexing `positions`.
    pub indices: Vec<u32>,
    pub transform: Matrix,
    /// Colour as sRGB red, green, blue, alpha, each `0..=1`.
    pub color: [f32; 4],
}

/// The text of an export, and what is in it.
#[derive(Debug, Clone, PartialEq)]
pub struct ObjFile {
    pub obj: String,
    pub mtl: String,
    pub vertices: usize,
    pub triangles: usize,
}

/// Turns a name into something safe in an OBJ/MTL line and a file name: letters, digits, `_`
/// and `-` stay, everything else becomes `_`, and an empty name becomes `model`.
pub fn safe_name(name: &str) -> String {
    let cleaned: String = name.chars().map(|c| if c.is_ascii_alphanumeric() || c == '_' || c == '-' { c } else { '_' }).collect();
    if cleaned.trim_matches('_').is_empty() {
        "model".to_string()
    } else {
        cleaned
    }
}

fn apply(m: &Matrix, p: [f32; 3]) -> [f32; 3] {
    [
        m[0] * p[0] + m[4] * p[1] + m[8] * p[2] + m[12],
        m[1] * p[0] + m[5] * p[1] + m[9] * p[2] + m[13],
        m[2] * p[0] + m[6] * p[1] + m[10] * p[2] + m[14],
    ]
}

/// The determinant of the upper 3 x 3, and the cofactor matrix (`det * inverse-transpose`) that
/// carries normals through a transform with any scale or mirroring.
fn normal_matrix(m: &Matrix) -> (f32, [[f32; 3]; 3]) {
    // Rows of the 3 x 3 (column-major storage: element (row, col) is m[col * 4 + row]).
    let a = |r: usize, c: usize| m[c * 4 + r];
    let cof = |r: usize, c: usize| {
        let (r1, r2) = ((r + 1) % 3, (r + 2) % 3);
        let (c1, c2) = ((c + 1) % 3, (c + 2) % 3);
        a(r1, c1) * a(r2, c2) - a(r1, c2) * a(r2, c1)
    };
    let det = a(0, 0) * cof(0, 0) + a(0, 1) * cof(0, 1) + a(0, 2) * cof(0, 2);
    let mut out = [[0.0; 3]; 3];
    for (r, row) in out.iter_mut().enumerate() {
        for (c, value) in row.iter_mut().enumerate() {
            *value = cof(r, c);
        }
    }
    (det, out)
}

fn number(value: f32) -> String {
    // Float noise from inverting a transform (a few millionths) is not geometry.
    let value = if value.abs() < 1.0e-5 { 0.0 } else { value };
    let text = format!("{value:.6}");
    let trimmed = text.trim_end_matches('0').trim_end_matches('.');
    // Never print "-0".
    if trimmed == "-0" || trimmed.is_empty() {
        "0".to_string()
    } else {
        trimmed.to_string()
    }
}

fn hex(color: [f32; 4]) -> String {
    let byte = |c: f32| (c.clamp(0.0, 1.0) * 255.0).round() as u8;
    format!("{:02x}{:02x}{:02x}{:02x}", byte(color[0]), byte(color[1]), byte(color[2]), byte(color[3]))
}

/// Builds the OBJ and MTL text for a model called `name` made of `parts`.
///
/// Refused with a reason when there is nothing to write, when a part's indices point outside its
/// positions or are not whole triangles, when normals do not match positions, or when any number
/// is not finite.
pub fn build_obj(name: &str, parts: &[Part]) -> Result<ObjFile, String> {
    let name = safe_name(name);
    let nonempty: Vec<(usize, &Part)> = parts.iter().enumerate().filter(|(_, p)| !p.indices.is_empty()).collect();
    if nonempty.is_empty() {
        return Err(format!("{name} has no geometry to export"));
    }
    // One material per distinct colour, named by its value so the same colour is one entry.
    let mut materials: BTreeMap<String, [f32; 4]> = BTreeMap::new();
    let mut obj = String::new();
    let _ = writeln!(obj, "# Ozbound model export: {name}");
    let _ = writeln!(obj, "# Wavefront OBJ, right-handed, Y up, 1 unit = 1 map tile, origin at the model's foot");
    let _ = writeln!(obj, "mtllib {name}.mtl");
    let _ = writeln!(obj, "o {name}");
    let (mut vertex_base, mut normal_base) = (0usize, 0usize);
    let (mut vertices, mut triangles) = (0usize, 0usize);
    for (number_of_part, (original, part)) in nonempty.iter().enumerate() {
        let label = format!("part {original}");
        if part.indices.len() % 3 != 0 {
            return Err(format!("{label} of {name}: its {} indices are not whole triangles", part.indices.len()));
        }
        if let Some(bad) = part.indices.iter().find(|i| **i as usize >= part.positions.len()) {
            return Err(format!("{label} of {name}: index {bad} is outside its {} positions", part.positions.len()));
        }
        if !part.normals.is_empty() && part.normals.len() != part.positions.len() {
            return Err(format!("{label} of {name}: {} normals for {} positions", part.normals.len(), part.positions.len()));
        }
        if part.transform.iter().chain(part.positions.iter().flatten()).chain(part.normals.iter().flatten()).chain(part.color.iter()).any(|v| !v.is_finite()) {
            return Err(format!("{label} of {name}: a number is not finite"));
        }
        let material = format!("color_{}", hex(part.color));
        materials.insert(material.clone(), part.color);
        let _ = writeln!(obj, "g part_{number_of_part}");
        let _ = writeln!(obj, "usemtl {material}");
        for p in &part.positions {
            let q = apply(&part.transform, *p);
            let _ = writeln!(obj, "v {} {} {}", number(q[0]), number(q[1]), number(q[2]));
        }
        let (det, cof) = normal_matrix(&part.transform);
        // A mirroring transform turns triangles inside out: reverse them to keep them facing out.
        let mirrored = det < 0.0;
        let has_normals = !part.normals.is_empty();
        for n in &part.normals {
            let mut v = [0.0f32; 3];
            for (r, row) in cof.iter().enumerate() {
                v[r] = row[0] * n[0] + row[1] * n[1] + row[2] * n[2];
            }
            let flip = if mirrored { -1.0 } else { 1.0 };
            let length = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt();
            let v = if length > 1.0e-12 { [flip * v[0] / length, flip * v[1] / length, flip * v[2] / length] } else { [0.0, 1.0, 0.0] };
            let _ = writeln!(obj, "vn {} {} {}", number(v[0]), number(v[1]), number(v[2]));
        }
        for tri in part.indices.chunks(3) {
            let corners: [u32; 3] = if mirrored { [tri[0], tri[2], tri[1]] } else { [tri[0], tri[1], tri[2]] };
            let face: Vec<String> = corners
                .iter()
                .map(|i| {
                    let v = vertex_base + *i as usize + 1;
                    if has_normals {
                        format!("{v}//{}", normal_base + *i as usize + 1)
                    } else {
                        v.to_string()
                    }
                })
                .collect();
            let _ = writeln!(obj, "f {}", face.join(" "));
        }
        vertex_base += part.positions.len();
        if has_normals {
            normal_base += part.normals.len();
        }
        vertices += part.positions.len();
        triangles += part.indices.len() / 3;
    }
    let mut mtl = String::new();
    let _ = writeln!(mtl, "# Ozbound model export: {name}");
    for (material, color) in &materials {
        let _ = writeln!(mtl, "newmtl {material}");
        let _ = writeln!(mtl, "Kd {} {} {}", number(color[0].clamp(0.0, 1.0)), number(color[1].clamp(0.0, 1.0)), number(color[2].clamp(0.0, 1.0)));
        if color[3] < 1.0 {
            let _ = writeln!(mtl, "d {}", number(color[3].clamp(0.0, 1.0)));
        }
        let _ = writeln!(mtl, "illum 1");
    }
    Ok(ObjFile { obj, mtl, vertices, triangles })
}

/// Writes `<name>.obj` and `<name>.mtl` into `dir` (created if needed), each through a temporary
/// file and a rename so a failure never leaves half a file. Returns the `.obj` path.
pub fn save_obj(dir: &Path, name: &str, file: &ObjFile) -> Result<PathBuf, String> {
    let name = safe_name(name);
    std::fs::create_dir_all(dir).map_err(|error| format!("cannot create {}: {error}", dir.display()))?;
    let write = |extension: &str, text: &str| -> Result<PathBuf, String> {
        use std::io::Write;
        let path = dir.join(format!("{name}.{extension}"));
        let temp = dir.join(format!("{name}.{extension}.tmp"));
        let result = (|| -> std::io::Result<()> {
            let mut handle = std::fs::File::create(&temp)?;
            handle.write_all(text.as_bytes())?;
            handle.sync_all()?;
            std::fs::rename(&temp, &path)
        })();
        result.map_err(|error| {
            let _ = std::fs::remove_file(&temp);
            format!("cannot save {}: {error}", path.display())
        })?;
        Ok(path)
    };
    write("mtl", &file.mtl)?;
    write("obj", &file.obj)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn triangle(color: [f32; 4]) -> Part {
        Part {
            positions: vec![[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]],
            normals: vec![[0.0, 0.0, 1.0]; 3],
            indices: vec![0, 1, 2],
            transform: IDENTITY,
            color,
        }
    }

    fn lines<'a>(text: &'a str, prefix: &str) -> Vec<&'a str> {
        text.lines().filter(|l| l.starts_with(prefix)).collect()
    }

    fn translation(x: f32, y: f32, z: f32) -> Matrix {
        let mut m = IDENTITY;
        m[12] = x;
        m[13] = y;
        m[14] = z;
        m
    }

    fn scale(x: f32, y: f32, z: f32) -> Matrix {
        let mut m = IDENTITY;
        m[0] = x;
        m[5] = y;
        m[10] = z;
        m
    }

    #[test]
    fn one_triangle_writes_the_expected_file() {
        let file = build_obj("Big Tree", &[triangle([1.0, 0.5, 0.0, 1.0])]).unwrap();
        assert_eq!((file.vertices, file.triangles), (3, 1));
        assert_eq!(
            file.obj,
            "# Ozbound model export: Big_Tree\n# Wavefront OBJ, right-handed, Y up, 1 unit = 1 map tile, origin at the model's foot\nmtllib Big_Tree.mtl\no Big_Tree\ng part_0\nusemtl color_ff8000ff\nv 0 0 0\nv 1 0 0\nv 0 1 0\nvn 0 0 1\nvn 0 0 1\nvn 0 0 1\nf 1//1 2//2 3//3\n"
        );
        assert_eq!(file.mtl, "# Ozbound model export: Big_Tree\nnewmtl color_ff8000ff\nKd 1 0.5 0\nillum 1\n");
    }

    #[test]
    fn indices_run_across_parts_and_each_part_keeps_its_group_and_colour() {
        let a = triangle([1.0, 0.0, 0.0, 1.0]);
        let mut b = triangle([0.0, 1.0, 0.0, 1.0]);
        b.transform = translation(5.0, 0.0, 0.0);
        let file = build_obj("two", &[a, b]).unwrap();
        assert_eq!(lines(&file.obj, "v ").len(), 6);
        assert_eq!(lines(&file.obj, "g "), ["g part_0", "g part_1"]);
        assert_eq!(lines(&file.obj, "usemtl "), ["usemtl color_ff0000ff", "usemtl color_00ff00ff"]);
        assert_eq!(lines(&file.obj, "f "), ["f 1//1 2//2 3//3", "f 4//4 5//5 6//6"]);
        assert!(file.obj.contains("v 5 0 0\nv 6 0 0\nv 5 1 0\n"), "the second part is moved by its transform");
        assert_eq!(lines(&file.mtl, "newmtl ").len(), 2);
    }

    #[test]
    fn equal_colours_share_one_material() {
        let file = build_obj("same", &[triangle([0.2, 0.4, 0.6, 1.0]), triangle([0.2, 0.4, 0.6, 1.0])]).unwrap();
        assert_eq!(lines(&file.mtl, "newmtl ").len(), 1);
        assert_eq!(lines(&file.obj, "usemtl ").len(), 2);
    }

    #[test]
    fn translucent_colours_write_their_opacity() {
        let file = build_obj("glass", &[triangle([0.1, 0.2, 0.3, 0.5])]).unwrap();
        assert!(file.mtl.contains("d 0.5\n"), "{}", file.mtl);
        assert!(!build_obj("solid", &[triangle([0.1, 0.2, 0.3, 1.0])]).unwrap().mtl.contains("\nd "));
    }

    #[test]
    fn parts_without_normals_write_plain_faces() {
        let mut part = triangle([1.0, 1.0, 1.0, 1.0]);
        part.normals.clear();
        let file = build_obj("flat", &[part]).unwrap();
        assert!(lines(&file.obj, "vn ").is_empty());
        assert_eq!(lines(&file.obj, "f "), ["f 1 2 3"]);
    }

    #[test]
    fn a_part_without_normals_does_not_shift_the_normal_numbers_of_the_next() {
        let mut bare = triangle([1.0, 0.0, 0.0, 1.0]);
        bare.normals.clear();
        let file = build_obj("mixed", &[bare, triangle([0.0, 1.0, 0.0, 1.0])]).unwrap();
        assert_eq!(lines(&file.obj, "f "), ["f 1 2 3", "f 4//1 5//2 6//3"]);
    }

    #[test]
    fn transforms_move_scale_and_carry_normals_correctly() {
        let mut part = triangle([1.0, 1.0, 1.0, 1.0]);
        part.positions = vec![[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]];
        part.normals = vec![[0.0, 1.0, 0.0]; 3];
        // Stretch y by 4: positions stretch, and a normal stays pointing along y with length one.
        part.transform = scale(1.0, 4.0, 1.0);
        let file = build_obj("tall", &[part]).unwrap();
        assert!(file.obj.contains("v 1 0 0\nv 0 4 0\nv 0 0 1\n"));
        assert!(file.obj.contains("vn 0 1 0\nvn 0 1 0\nvn 0 1 0\n"));
        // A slanted normal under a non-uniform scale follows the inverse-transpose rule.
        let mut slanted = triangle([1.0, 1.0, 1.0, 1.0]);
        slanted.normals = vec![[1.0, 1.0, 0.0]; 3];
        slanted.transform = scale(1.0, 4.0, 1.0);
        let out = build_obj("slanted", &[slanted]).unwrap();
        let normal = lines(&out.obj, "vn ")[0].split_whitespace().skip(1).map(|v| v.parse::<f32>().unwrap()).collect::<Vec<_>>();
        assert!(normal[0] > normal[1], "stretching y tilts the normal toward x: {normal:?}");
        assert!(((normal[0] * normal[0] + normal[1] * normal[1] + normal[2] * normal[2]) - 1.0).abs() < 1e-4);
    }

    #[test]
    fn a_mirroring_transform_keeps_triangles_facing_outward() {
        let mut part = triangle([1.0, 1.0, 1.0, 1.0]);
        part.transform = scale(-1.0, 1.0, 1.0);
        let file = build_obj("mirror", &[part]).unwrap();
        assert!(file.obj.contains("v -1 0 0\n"), "{}", file.obj);
        assert_eq!(lines(&file.obj, "f "), ["f 1//1 3//3 2//2"], "winding reversed");
        assert!(file.obj.contains("vn 0 0 1\n"), "the normal still points the way the face does");
    }

    #[test]
    fn numbers_are_short_stable_and_never_negative_zero() {
        assert_eq!(number(0.0), "0");
        assert_eq!(number(-0.0), "0");
        assert_eq!(number(-0.0000001), "0");
        assert_eq!(number(-0.000002), "0", "float noise is not written");
        assert_eq!(number(0.00002), "0.00002");
        assert_eq!(number(1.0), "1");
        assert_eq!(number(0.5), "0.5");
        assert_eq!(number(0.123456789), "0.123457");
        assert_eq!(number(-2.25), "-2.25");
    }

    #[test]
    fn bad_input_is_refused_with_the_part_named() {
        assert!(build_obj("x", &[]).unwrap_err().contains("no geometry"));
        let mut empty = triangle([1.0; 4]);
        empty.indices.clear();
        assert!(build_obj("x", &[empty]).unwrap_err().contains("no geometry"));
        let mut out_of_range = triangle([1.0; 4]);
        out_of_range.indices = vec![0, 1, 7];
        assert!(build_obj("x", &[out_of_range]).unwrap_err().contains("index 7"));
        let mut partial = triangle([1.0; 4]);
        partial.indices = vec![0, 1];
        assert!(build_obj("x", &[partial]).unwrap_err().contains("whole triangles"));
        let mut normals = triangle([1.0; 4]);
        normals.normals.pop();
        assert!(build_obj("x", &[normals]).unwrap_err().contains("normals"));
        let mut nan = triangle([1.0; 4]);
        nan.positions[1][0] = f32::NAN;
        assert!(build_obj("x", &[nan]).unwrap_err().contains("not finite"));
        let mut inf = triangle([1.0; 4]);
        inf.transform[12] = f32::INFINITY;
        assert!(build_obj("x", &[inf]).unwrap_err().contains("not finite"));
        let first = triangle([1.0; 4]);
        let mut second = triangle([1.0; 4]);
        second.indices = vec![0, 1, 9];
        assert!(build_obj("x", &[first, second]).unwrap_err().contains("part 1"), "the failing part is named");
    }

    #[test]
    fn output_is_deterministic() {
        let parts = [triangle([0.3, 0.6, 0.9, 1.0]), triangle([0.9, 0.6, 0.3, 1.0])];
        assert_eq!(build_obj("same", &parts).unwrap(), build_obj("same", &parts).unwrap());
    }

    #[test]
    fn names_are_made_safe() {
        assert_eq!(safe_name("copper vein"), "copper_vein");
        assert_eq!(safe_name("../../etc/passwd"), "______etc_passwd");
        assert_eq!(safe_name(""), "model");
        assert_eq!(safe_name("   "), "model");
        assert_eq!(safe_name("tree-2_b"), "tree-2_b");
        assert_eq!(safe_name("Mistress Pell\n"), "Mistress_Pell_");
    }

    #[test]
    fn every_face_index_points_at_a_written_vertex_and_normal() {
        let a = triangle([1.0, 0.0, 0.0, 1.0]);
        let mut b = triangle([0.0, 1.0, 0.0, 1.0]);
        b.indices = vec![2, 1, 0, 0, 1, 2];
        let file = build_obj("check", &[a, b]).unwrap();
        let (v, n) = (lines(&file.obj, "v ").len(), lines(&file.obj, "vn ").len());
        for face in lines(&file.obj, "f ") {
            for corner in face.split_whitespace().skip(1) {
                let (vi, ni) = corner.split_once("//").unwrap();
                assert!((1..=v).contains(&vi.parse::<usize>().unwrap()));
                assert!((1..=n).contains(&ni.parse::<usize>().unwrap()));
            }
        }
        assert_eq!(file.triangles, 3);
    }

    #[test]
    fn saving_writes_both_files_and_leaves_no_temporary() {
        let dir = std::env::temp_dir().join(format!("ozbound-obj-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let file = build_obj("big tree", &[triangle([0.5, 0.5, 0.5, 1.0])]).unwrap();
        let path = save_obj(&dir.join("library"), "big tree", &file).unwrap();
        assert_eq!(path.file_name().unwrap(), "big_tree.obj");
        assert_eq!(std::fs::read_to_string(&path).unwrap(), file.obj);
        assert_eq!(std::fs::read_to_string(dir.join("library").join("big_tree.mtl")).unwrap(), file.mtl);
        let leftovers: Vec<_> = std::fs::read_dir(dir.join("library")).unwrap().filter_map(|e| e.ok()).filter(|e| e.file_name().to_string_lossy().ends_with(".tmp")).collect();
        assert!(leftovers.is_empty());
        // Saving again replaces the files.
        let again = build_obj("big tree", &[triangle([0.1, 0.1, 0.1, 1.0])]).unwrap();
        save_obj(&dir.join("library"), "big tree", &again).unwrap();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), again.obj);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_failed_save_names_the_file_and_reports_the_reason() {
        let dir = std::env::temp_dir().join(format!("ozbound-obj-blocked-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        // A file where the directory should be.
        let blocker = dir.join("library");
        std::fs::write(&blocker, "not a directory").unwrap();
        let file = build_obj("tree", &[triangle([1.0; 4])]).unwrap();
        let error = save_obj(&blocker, "tree", &file).unwrap_err();
        assert!(error.contains("cannot"), "{error}");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
