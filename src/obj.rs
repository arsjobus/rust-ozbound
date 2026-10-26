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
//!   A part with a label is written as `g <label>` (`crown`, `plants`, ...), so a game can find
//!   the pieces it shows and hides (a felled tree's crown); unlabelled parts are `g part_N`.
//! * Positions (`v`), normals (`vn`) when the part has them, and triangles (`f v//vn ...`) with
//!   1-based indices that run across the whole file.
//! * No texture coordinates: the game's models are flat-coloured.
//! * The `.mtl` file holds one `newmtl` per distinct colour with its diffuse colour `Kd`
//!   (sRGB, as modelling tools expect), `Ke` (emitted light, linear) when the part glows, and `d`
//!   (opacity) when not fully opaque.
//!
//! # Reading it back
//!
//! [`parse_obj`] reads that same dialect (and ordinary OBJ files from other tools: `v/vt/vn`
//! faces, negative indices, quads and larger polygons, missing normals) into a [`ParsedModel`]:
//! one [`ParsedPart`] per group and material, with its own vertex list, so the game can turn each
//! into one mesh. Writing then reading a model gives the same triangles back.

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
    /// Emitted light as linear red, green, blue (zero for a part that does not glow).
    pub emissive: [f32; 3],
    /// What the part is called in the file (`crown`); empty for `part_N`.
    pub label: String,
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

/// The material a colour (and glow) is written under: the same colour is the same entry.
fn material_name(color: [f32; 4], emissive: [f32; 3]) -> String {
    if emissive.iter().any(|e| *e > 0.0) {
        let hundredths = |e: f32| (e.max(0.0) * 100.0).round() as u32;
        format!("color_{}_ke{}_{}_{}", hex(color), hundredths(emissive[0]), hundredths(emissive[1]), hundredths(emissive[2]))
    } else {
        format!("color_{}", hex(color))
    }
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
    let mut materials: BTreeMap<String, ([f32; 4], [f32; 3])> = BTreeMap::new();
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
        if part.transform.iter().chain(part.positions.iter().flatten()).chain(part.normals.iter().flatten()).chain(part.color.iter()).chain(part.emissive.iter()).any(|v| !v.is_finite()) {
            return Err(format!("{label} of {name}: a number is not finite"));
        }
        let material = material_name(part.color, part.emissive);
        materials.insert(material.clone(), (part.color, part.emissive));
        if part.label.is_empty() {
            let _ = writeln!(obj, "g part_{number_of_part}");
        } else {
            let _ = writeln!(obj, "g {}", safe_name(&part.label));
        }
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
    for (material, (color, emissive)) in &materials {
        let _ = writeln!(mtl, "newmtl {material}");
        let _ = writeln!(mtl, "Kd {} {} {}", number(color[0].clamp(0.0, 1.0)), number(color[1].clamp(0.0, 1.0)), number(color[2].clamp(0.0, 1.0)));
        if emissive.iter().any(|e| *e > 0.0) {
            let _ = writeln!(mtl, "Ke {} {} {}", number(emissive[0].max(0.0)), number(emissive[1].max(0.0)), number(emissive[2].max(0.0)));
        }
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

// ---------------------------------------------------------------------------------------------
// Reading
// ---------------------------------------------------------------------------------------------

/// The most vertices and triangles a model file may hold: a guard against a file that would
/// fill memory, far above any model of this game (the biggest has a few thousand).
pub const MAX_PARSED_VERTICES: usize = 200_000;
pub const MAX_PARSED_TRIANGLES: usize = 200_000;

/// The colour of a part that names no material.
pub const DEFAULT_COLOR: [f32; 4] = [0.8, 0.8, 0.8, 1.0];

/// One group of a model file with one material, as a mesh of its own.
#[derive(Debug, Clone, PartialEq)]
pub struct ParsedPart {
    /// The group's name in the file (`crown`, `part_3`).
    pub name: String,
    pub positions: Vec<[f32; 3]>,
    /// One per position.
    pub normals: Vec<[f32; 3]>,
    /// Three per triangle, indexing `positions`.
    pub indices: Vec<u32>,
    pub color: [f32; 4],
    pub emissive: [f32; 3],
}

/// The sub-model every unnamed or `part_N` group belongs to.
pub const BODY: &str = "body";

impl ParsedPart {
    /// Which piece of the model this part belongs to: the group name up to its first `.`, with
    /// `part_N` (and an empty name) meaning [`BODY`]. A game shows and hides whole sub-models.
    pub fn sub_model(&self) -> &str {
        let base = self.name.split('.').next().unwrap_or("");
        let numbered = base.strip_prefix("part_").is_some_and(|n| !n.is_empty() && n.bytes().all(|b| b.is_ascii_digit()));
        if base.is_empty() || numbered {
            BODY
        } else {
            base
        }
    }

    pub fn triangles(&self) -> usize {
        self.indices.len() / 3
    }
}

/// A model read from an OBJ file (and its MTL).
#[derive(Debug, Clone, PartialEq)]
pub struct ParsedModel {
    /// The `o` name, or empty.
    pub name: String,
    pub parts: Vec<ParsedPart>,
}

impl ParsedModel {
    pub fn triangles(&self) -> usize {
        self.parts.iter().map(ParsedPart::triangles).sum()
    }

    pub fn vertices(&self) -> usize {
        self.parts.iter().map(|p| p.positions.len()).sum()
    }

    /// The distinct sub-model names, in order of first appearance.
    pub fn sub_models(&self) -> Vec<&str> {
        let mut names: Vec<&str> = Vec::new();
        for part in &self.parts {
            if !names.contains(&part.sub_model()) {
                names.push(part.sub_model());
            }
        }
        names
    }

    /// True when some part belongs to the sub-model `name`.
    pub fn has_sub_model(&self, name: &str) -> bool {
        self.parts.iter().any(|p| p.sub_model() == name)
    }
}

fn float(word: &str, line: usize) -> Result<f32, String> {
    match word.parse::<f32>() {
        Ok(v) if v.is_finite() => Ok(v),
        _ => Err(format!("line {line}: `{word}` is not a finite number")),
    }
}

fn floats<const N: usize>(words: &[&str], line: usize, what: &str) -> Result<[f32; N], String> {
    if words.len() < N {
        return Err(format!("line {line}: `{what}` needs {N} numbers, found {}", words.len()));
    }
    let mut out = [0.0; N];
    for (slot, word) in out.iter_mut().zip(words) {
        *slot = float(word, line)?;
    }
    Ok(out)
}

/// What an MTL file says about each material: sRGB diffuse colour with opacity, and emitted light.
fn parse_mtl(text: &str) -> Result<BTreeMap<String, ([f32; 4], [f32; 3])>, String> {
    let mut out: BTreeMap<String, ([f32; 4], [f32; 3])> = BTreeMap::new();
    let mut current: Option<String> = None;
    for (index, raw) in text.trim_start_matches('\u{feff}').lines().enumerate() {
        let line = index + 1;
        let content = raw.split('#').next().unwrap_or("").trim();
        let words: Vec<&str> = content.split_whitespace().collect();
        let Some((keyword, rest)) = words.split_first() else { continue };
        match *keyword {
            "newmtl" => {
                let name = rest.join(" ");
                if name.is_empty() {
                    return Err(format!("line {line}: `newmtl` needs a name"));
                }
                out.insert(name.clone(), (DEFAULT_COLOR, [0.0; 3]));
                current = Some(name);
            }
            "Kd" | "d" | "Tr" | "Ke" => {
                let Some(name) = &current else { return Err(format!("line {line}: `{keyword}` comes before any `newmtl`")) };
                let entry = out.get_mut(name).expect("the current material was inserted");
                match *keyword {
                    "Kd" => {
                        let [r, g, b] = floats::<3>(rest, line, "Kd")?;
                        entry.0 = [r.clamp(0.0, 1.0), g.clamp(0.0, 1.0), b.clamp(0.0, 1.0), entry.0[3]];
                    }
                    "d" => entry.0[3] = floats::<1>(rest, line, "d")?[0].clamp(0.0, 1.0),
                    "Tr" => entry.0[3] = (1.0 - floats::<1>(rest, line, "Tr")?[0]).clamp(0.0, 1.0),
                    _ => {
                        let [r, g, b] = floats::<3>(rest, line, "Ke")?;
                        entry.1 = [r.max(0.0), g.max(0.0), b.max(0.0)];
                    }
                }
            }
            // Everything else (Ka, Ks, Ns, illum, map_*) does not matter to flat-coloured models.
            _ => {}
        }
    }
    Ok(out)
}

/// One face corner: 0-based position index, optional 0-based normal index.
type Corner = (usize, Option<usize>);

fn corner(word: &str, positions: usize, normals: usize, line: usize) -> Result<Corner, String> {
    let fields: Vec<&str> = word.split('/').collect();
    if fields.len() > 3 {
        return Err(format!("line {line}: `{word}` has too many `/` fields"));
    }
    let resolve = |text: &str, count: usize, what: &str| -> Result<usize, String> {
        let n: i64 = text.parse().map_err(|_| format!("line {line}: `{text}` is not a whole {what} index"))?;
        let at = if n > 0 { n - 1 } else if n < 0 { count as i64 + n } else { -1 };
        if at < 0 || at as usize >= count {
            return Err(format!("line {line}: {what} index {n} is outside the {count} defined so far"));
        }
        Ok(at as usize)
    };
    let v = resolve(fields[0], positions, "vertex")?;
    // fields[1] is the texture coordinate, which flat-coloured models do not use.
    let n = match fields.get(2) {
        Some(text) if !text.is_empty() => Some(resolve(text, normals, "normal")?),
        _ => None,
    };
    Ok((v, n))
}

struct PartBuilder {
    name: String,
    color: [f32; 4],
    emissive: [f32; 3],
    positions: Vec<[f32; 3]>,
    normals: Vec<[f32; 3]>,
    indices: Vec<u32>,
    shared: std::collections::HashMap<(usize, usize), u32>,
}

impl PartBuilder {
    fn new(name: &str, color: [f32; 4], emissive: [f32; 3]) -> Self {
        Self { name: name.to_string(), color, emissive, positions: Vec::new(), normals: Vec::new(), indices: Vec::new(), shared: std::collections::HashMap::new() }
    }

    fn finish(self) -> Option<ParsedPart> {
        (!self.indices.is_empty()).then_some(ParsedPart { name: self.name, positions: self.positions, normals: self.normals, indices: self.indices, color: self.color, emissive: self.emissive })
    }
}

fn face_normal(a: [f32; 3], b: [f32; 3], c: [f32; 3]) -> [f32; 3] {
    let (u, v) = ([b[0] - a[0], b[1] - a[1], b[2] - a[2]], [c[0] - a[0], c[1] - a[1], c[2] - a[2]]);
    let n = [u[1] * v[2] - u[2] * v[1], u[2] * v[0] - u[0] * v[2], u[0] * v[1] - u[1] * v[0]];
    let length = (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt();
    if length > 1.0e-12 {
        [n[0] / length, n[1] / length, n[2] / length]
    } else {
        [0.0, 1.0, 0.0]
    }
}

/// Reads an OBJ file, with the text of its MTL file when there is one.
///
/// A new part starts at every `g` and whenever `usemtl` changes the material of a part that
/// already has faces; parts without triangles are dropped. Faces of any size are cut into
/// triangles; a face corner with no normal gets its triangle's flat normal. When `mtl` is given,
/// every material a face uses must be defined in it; without one, parts are plain grey.
///
/// Refused, naming the line, for: a number that is not finite, an index outside what has been
/// defined, a face with fewer than three corners, and a file with no triangles or more vertices or
/// triangles than [`MAX_PARSED_VERTICES`] / [`MAX_PARSED_TRIANGLES`].
pub fn parse_obj(obj: &str, mtl: Option<&str>) -> Result<ParsedModel, String> {
    let materials = match mtl {
        Some(text) => Some(parse_mtl(text).map_err(|e| format!("material file: {e}"))?),
        None => None,
    };
    let (mut positions, mut normals): (Vec<[f32; 3]>, Vec<[f32; 3]>) = (Vec::new(), Vec::new());
    let mut model_name = String::new();
    let mut finished: Vec<ParsedPart> = Vec::new();
    let mut current = PartBuilder::new("", DEFAULT_COLOR, [0.0; 3]);
    let (mut vertices, mut triangles) = (0usize, 0usize);
    for (index, raw) in obj.trim_start_matches('\u{feff}').lines().enumerate() {
        let line = index + 1;
        let content = raw.split('#').next().unwrap_or("").trim();
        let words: Vec<&str> = content.split_whitespace().collect();
        let Some((keyword, rest)) = words.split_first() else { continue };
        match *keyword {
            "v" => {
                positions.push(floats::<3>(rest, line, "v")?);
                if positions.len() > MAX_PARSED_VERTICES {
                    return Err(format!("line {line}: more than {MAX_PARSED_VERTICES} vertices"));
                }
            }
            "vn" => normals.push(floats::<3>(rest, line, "vn")?),
            "o" => {
                if model_name.is_empty() {
                    model_name = rest.join(" ");
                }
            }
            "g" => {
                let name = rest.first().copied().unwrap_or("");
                let next = PartBuilder::new(name, current.color, current.emissive);
                finished.extend(std::mem::replace(&mut current, next).finish());
            }
            "usemtl" => {
                let name = rest.join(" ");
                let (color, emissive) = match &materials {
                    Some(table) => *table.get(&name).ok_or_else(|| format!("line {line}: material `{name}` is not defined in the material file"))?,
                    None => (DEFAULT_COLOR, [0.0; 3]),
                };
                if current.indices.is_empty() {
                    current.color = color;
                    current.emissive = emissive;
                } else if (current.color, current.emissive) != (color, emissive) {
                    let next = PartBuilder::new(&current.name, color, emissive);
                    finished.extend(std::mem::replace(&mut current, next).finish());
                }
            }
            "f" => {
                if rest.len() < 3 {
                    return Err(format!("line {line}: a face needs at least three corners, found {}", rest.len()));
                }
                let corners = rest.iter().map(|w| corner(w, positions.len(), normals.len(), line)).collect::<Result<Vec<_>, _>>()?;
                let with_normals = corners.iter().all(|(_, n)| n.is_some());
                for i in 1..corners.len() - 1 {
                    let tri = [corners[0], corners[i], corners[i + 1]];
                    let flat = face_normal(positions[tri[0].0], positions[tri[1].0], positions[tri[2].0]);
                    for (v, n) in tri {
                        let slot = if with_normals {
                            let n = n.expect("every corner has a normal");
                            *current.shared.entry((v, n)).or_insert_with(|| {
                                current.positions.push(positions[v]);
                                current.normals.push(normals[n]);
                                vertices += 1;
                                (current.positions.len() - 1) as u32
                            })
                        } else {
                            current.positions.push(positions[v]);
                            current.normals.push(flat);
                            vertices += 1;
                            (current.positions.len() - 1) as u32
                        };
                        current.indices.push(slot);
                    }
                    triangles += 1;
                    if triangles > MAX_PARSED_TRIANGLES || vertices > MAX_PARSED_VERTICES * 3 {
                        return Err(format!("line {line}: more than {MAX_PARSED_TRIANGLES} triangles"));
                    }
                }
            }
            // Texture coordinates, smoothing groups, `mtllib`, lines and points: not used.
            _ => {}
        }
    }
    finished.extend(current.finish());
    if finished.is_empty() {
        return Err("the file has no triangles".to_string());
    }
    Ok(ParsedModel { name: model_name, parts: finished })
}

/// Reads `<dir>/<name>.obj` and its `<name>.mtl` (the `.mtl` may be absent: parts are grey).
pub fn load_obj(dir: &Path, name: &str) -> Result<ParsedModel, String> {
    let name = safe_name(name);
    let obj_path = dir.join(format!("{name}.obj"));
    let obj = std::fs::read_to_string(&obj_path).map_err(|error| format!("cannot read {}: {error}", obj_path.display()))?;
    let mtl_path = dir.join(format!("{name}.mtl"));
    let mtl = match std::fs::read_to_string(&mtl_path) {
        Ok(text) => Some(text),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
        Err(error) => return Err(format!("cannot read {}: {error}", mtl_path.display())),
    };
    parse_obj(&obj, mtl.as_deref()).map_err(|error| format!("{}: {error}", obj_path.display()))
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
            emissive: [0.0; 3],
            label: String::new(),
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
    // ---- reading ----

    fn labelled(label: &str, color: [f32; 4]) -> Part {
        Part { label: label.to_string(), ..triangle(color) }
    }

    #[test]
    fn a_written_model_reads_back_with_the_same_triangles_colours_and_labels() {
        let mut moved = labelled("crown", [0.1, 0.5, 0.2, 1.0]);
        moved.transform = translation(0.0, 2.0, 0.0);
        let mut glowing = triangle([0.9, 0.86, 1.0, 1.0]);
        glowing.emissive = [0.35, 0.30, 0.8];
        let written = build_obj("tree", &[triangle([0.4, 0.3, 0.1, 1.0]), moved, glowing]).unwrap();
        let read = parse_obj(&written.obj, Some(&written.mtl)).unwrap();
        assert_eq!(read.name, "tree");
        assert_eq!((read.parts.len(), read.triangles(), read.vertices()), (3, 3, 9));
        assert_eq!(read.parts.iter().map(|p| p.name.as_str()).collect::<Vec<_>>(), ["part_0", "crown", "part_2"]);
        assert_eq!(read.parts.iter().map(ParsedPart::sub_model).collect::<Vec<_>>(), ["body", "crown", "body"]);
        assert_eq!(read.sub_models(), ["body", "crown"]);
        assert!(read.has_sub_model("crown") && !read.has_sub_model("flecks"));
        assert_eq!(read.parts[1].positions[2], [0.0, 3.0, 0.0], "the part's transform was applied by the writer");
        let close = |a: f32, b: f32| (a - b).abs() < 0.01;
        assert!(read.parts[1].color.iter().zip([0.1, 0.5, 0.2, 1.0]).all(|(a, b)| close(*a, b)));
        assert_eq!(read.parts[0].emissive, [0.0; 3]);
        assert!(read.parts[2].emissive.iter().zip([0.35, 0.30, 0.8]).all(|(a, b)| close(*a, b)), "{:?}", read.parts[2].emissive);
        assert_eq!(read.parts[0].normals, vec![[0.0, 0.0, 1.0]; 3]);
    }

    #[test]
    fn writing_a_read_model_again_is_stable() {
        let first = build_obj("a", &[labelled("crown", [0.2, 0.4, 0.6, 1.0])]).unwrap();
        let read = parse_obj(&first.obj, Some(&first.mtl)).unwrap();
        let parts: Vec<Part> = read
            .parts
            .iter()
            .map(|p| Part { positions: p.positions.clone(), normals: p.normals.clone(), indices: p.indices.clone(), transform: IDENTITY, color: p.color, emissive: p.emissive, label: p.name.clone() })
            .collect();
        assert_eq!(build_obj("a", &parts).unwrap(), first);
    }

    #[test]
    fn labels_are_made_safe_and_blank_ones_stay_numbered() {
        let file = build_obj("m", &[labelled("my crown!", [1.0; 4]), triangle([1.0; 4])]).unwrap();
        assert_eq!(lines(&file.obj, "g "), ["g my_crown_", "g part_1"]);
    }

    #[test]
    fn sub_model_names_ignore_dots_and_numbered_groups_are_the_body() {
        let part = |name: &str| ParsedPart { name: name.into(), positions: vec![], normals: vec![], indices: vec![], color: [1.0; 4], emissive: [0.0; 3] };
        assert_eq!(part("part_12").sub_model(), "body");
        assert_eq!(part("").sub_model(), "body");
        assert_eq!(part("crown.002").sub_model(), "crown");
        assert_eq!(part("part_x").sub_model(), "part_x");
        assert_eq!(part("part_").sub_model(), "part_");
    }

    const QUAD: &str = "o quad\nv 0 0 0\nv 1 0 0\nv 1 1 0\nv 0 1 0\nusemtl red\nf 1 2 3 4\n";
    const RED: &str = "newmtl red\nKd 1 0 0\n";

    #[test]
    fn polygons_are_cut_into_triangles_and_get_flat_normals_when_the_file_has_none() {
        let model = parse_obj(QUAD, Some(RED)).unwrap();
        assert_eq!((model.parts.len(), model.triangles()), (1, 2));
        assert_eq!(model.parts[0].color, [1.0, 0.0, 0.0, 1.0]);
        assert_eq!(model.parts[0].normals.len(), model.parts[0].positions.len());
        assert!(model.parts[0].normals.iter().all(|n| *n == [0.0, 0.0, 1.0]));
        assert!(model.parts[0].indices.iter().all(|i| (*i as usize) < model.parts[0].positions.len()));
    }

    #[test]
    fn every_face_style_and_negative_indices_are_read() {
        let text = "v 0 0 0\nv 1 0 0\nv 0 1 0\nvt 0 0\nvn 0 0 1\nf 1/1/1 2/1/1 3/1/1\nf 1//1 2//1 3//1\nf -3 -2 -1\n";
        let model = parse_obj(text, None).unwrap();
        assert_eq!(model.triangles(), 3);
        assert_eq!(model.parts[0].color, DEFAULT_COLOR, "no material file means grey");
        // The first two faces share their three vertices; the third has no normals of its own.
        assert_eq!(model.parts[0].positions.len(), 6);
    }

    #[test]
    fn a_new_part_starts_at_each_group_and_each_material_change_and_empty_ones_are_dropped() {
        let text = "v 0 0 0\nv 1 0 0\nv 0 1 0\ng a\nusemtl red\nf 1 2 3\nusemtl blue\nf 1 2 3\ng empty\ng b\nf 1 2 3\n";
        let model = parse_obj(text, Some("newmtl red\nKd 1 0 0\nnewmtl blue\nKd 0 0 1\n")).unwrap();
        assert_eq!(model.parts.iter().map(|p| p.name.as_str()).collect::<Vec<_>>(), ["a", "a", "b"]);
        assert_eq!(model.parts[1].color, [0.0, 0.0, 1.0, 1.0]);
        assert_eq!(model.parts[2].color, [0.0, 0.0, 1.0, 1.0], "a group keeps the material in force");
    }

    #[test]
    fn mtl_opacity_and_comments_and_transparency_are_read() {
        let mtl = "# comment\nnewmtl a\nKd 0.5 0.5 0.5 # trailing\nd 0.25\nnewmtl b\nKd 1 1 1\nTr 0.5\nillum 2\nNs 10\n";
        let text = "v 0 0 0\nv 1 0 0\nv 0 1 0\nusemtl a\nf 1 2 3\nusemtl b\nf 1 2 3\n";
        let model = parse_obj(text, Some(mtl)).unwrap();
        assert_eq!(model.parts[0].color[3], 0.25);
        assert_eq!(model.parts[1].color[3], 0.5);
    }

    #[test]
    fn bad_files_are_refused_with_the_line() {
        let faces = "v 0 0 0\nv 1 0 0\nv 0 1 0\n";
        let err = |text: &str, mtl: Option<&str>| parse_obj(text, mtl).unwrap_err();
        assert!(err(&format!("{faces}f 1 2 9\n"), None).contains("line 4") && err(&format!("{faces}f 1 2 9\n"), None).contains("outside"));
        assert!(err(&format!("{faces}f 1 2\n"), None).contains("at least three"));
        assert!(err(&format!("{faces}f 0 1 2\n"), None).contains("outside"));
        assert!(err("v 0 nan 0\n", None).contains("line 1") && err("v 0 nan 0\n", None).contains("finite"));
        assert!(err("v 0 0\n", None).contains("needs 3 numbers"));
        assert!(err(&format!("{faces}f 1 2 x\n"), None).contains("not a whole"));
        assert!(err(&format!("{faces}f 1//1 2//1 3//1\n"), None).contains("normal index"));
        assert!(err(&format!("{faces}usemtl gold\nf 1 2 3\n"), Some(RED)).contains("`gold` is not defined"));
        assert!(err(faces, None).contains("no triangles"));
        assert!(err("", None).contains("no triangles"));
        assert!(err(&format!("{faces}f 1/2/3/4 2 3\n"), None).contains("too many"));
        assert!(err(faces, Some("Kd 1 1 1\n")).contains("before any `newmtl`"));
        assert!(err(faces, Some("newmtl a\nKd 1 1\n")).contains("material file: line 2"));
    }

    #[test]
    fn a_file_with_a_byte_order_mark_and_windows_line_ends_reads() {
        let text = "\u{feff}v 0 0 0\r\nv 1 0 0\r\nv 0 1 0\r\nf 1 2 3\r\n";
        assert_eq!(parse_obj(text, None).unwrap().triangles(), 1);
    }

    #[test]
    fn load_obj_reads_a_saved_model_and_names_the_file_when_it_fails() {
        let dir = std::env::temp_dir().join(format!("ozbound-obj-{}-{}", std::process::id(), line!()));
        let _ = std::fs::remove_dir_all(&dir);
        let file = build_obj("thing", &[labelled("crown", [0.3, 0.3, 0.3, 1.0])]).unwrap();
        save_obj(&dir, "thing", &file).unwrap();
        let model = load_obj(&dir, "thing").unwrap();
        assert_eq!(model.parts[0].sub_model(), "crown");
        let missing = load_obj(&dir, "nothing").unwrap_err();
        assert!(missing.contains("nothing.obj"), "{missing}");
        std::fs::write(dir.join("broken.obj"), "v 1 2\n").unwrap();
        let broken = load_obj(&dir, "broken").unwrap_err();
        assert!(broken.contains("broken.obj") && broken.contains("line 1"), "{broken}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn an_unreadable_material_file_is_not_the_same_as_a_missing_one() {
        // A missing .mtl is allowed (grey parts); a directory where the .mtl should be is an error.
        let dir = std::env::temp_dir().join(format!("ozbound-obj-{}-{}", std::process::id(), line!()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("x.mtl")).unwrap();
        std::fs::write(dir.join("x.obj"), "v 0 0 0\nv 1 0 0\nv 0 1 0\nf 1 2 3\n").unwrap();
        assert!(load_obj(&dir, "x").unwrap_err().contains("x.mtl"));
        std::fs::remove_dir_all(dir.join("x.mtl")).unwrap();
        assert_eq!(load_obj(&dir, "x").unwrap().parts[0].color, DEFAULT_COLOR);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
