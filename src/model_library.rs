//! The models of the object library as OBJ/MTL files: which objects have one, how a file is
//! checked, and what to do when one is missing.
//!
//! Pure Rust (no Bevy). `assets/library/<id>.obj` (with `<id>.mtl`) is the model of a kind of
//! object. The game spawns every object in [`MODELS`] as an *instance* of its file instead of
//! assembling it from primitives in code. The rules, in one place:
//!
//! * **A missing file is generated.** When neither `<id>.obj` nor `<id>.mtl` exists, the game
//!   builds that object once from its code-built model, writes the pair (through temporary files
//!   and renames), and uses what it wrote. So a fresh checkout, or a deleted file, repairs
//!   itself, and every file can be opened and reshaped in a 3D tool afterwards.
//! * **An existing file is never overwritten.** A file that cannot be read or fails
//!   [`check_model`] is reported by name and line, and the object keeps its code-built model for
//!   that run; the file is left exactly as it is for its owner to fix. Half a pair (an `.mtl`
//!   without its `.obj`) counts as a problem for the same reason.
//! * **A directory that does not exist is not created.** Started from another directory, the game
//!   uses its code-built models and says so.
//! * **Parts are named by what they are.** A group called `crown`, `plants`, `berries` or
//!   `flecks` is a piece the game hides when the object is spent; every other group is the
//!   always-visible body (`part_N` groups, from older exports, are body too).

use std::path::{Path, PathBuf};

use crate::obj::{self, ParsedModel};

/// One object kind whose model is a library file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ModelSpec {
    /// The file stem: `assets/library/<id>.obj`.
    pub id: &'static str,
    /// Sub-models the game shows and hides; each must be present in the file.
    pub toggles: &'static [&'static str],
}

/// Every object kind that is spawned from a library file.
pub const MODELS: [ModelSpec; 9] = [
    ModelSpec { id: "tree", toggles: &["crown"] },
    ModelSpec { id: "copper_vein", toggles: &["flecks"] },
    ModelSpec { id: "herb_patch", toggles: &["plants"] },
    ModelSpec { id: "berry_bush", toggles: &["berries"] },
    ModelSpec { id: "moonpetal", toggles: &[] },
    ModelSpec { id: "signpost", toggles: &[] },
    ModelSpec { id: "waymark", toggles: &[] },
    // The two pieces a fence is built from, one tile at a time: a post at the tile's middle, and a
    // rail that reaches half a tile toward +x (the game turns it toward each linked neighbour).
    ModelSpec { id: "fence_post", toggles: &[] },
    ModelSpec { id: "fence_rail", toggles: &[] },
];

/// The spec of `id`, if it is one of [`MODELS`].
pub fn spec(id: &str) -> Option<&'static ModelSpec> {
    MODELS.iter().find(|m| m.id == id)
}

/// Why a parsed model cannot be used as `spec`'s object, or `Ok`.
pub fn check_model(spec: &ModelSpec, model: &ParsedModel) -> Result<(), String> {
    if !model.has_sub_model(obj::BODY) && spec.toggles.is_empty() {
        return Err(format!("{} has no parts", spec.id));
    }
    for toggle in spec.toggles {
        if !model.has_sub_model(toggle) {
            return Err(format!("{} needs a group called `{toggle}` (the piece that is hidden when it is spent), but the file has {}", spec.id, describe_groups(model)));
        }
    }
    if model.parts.iter().any(|p| p.positions.iter().chain(p.normals.iter()).flatten().any(|v| !v.is_finite())) {
        return Err(format!("{} has a number that is not finite", spec.id));
    }
    Ok(())
}

fn describe_groups(model: &ParsedModel) -> String {
    let names = model.sub_models();
    if names.is_empty() {
        "no groups".to_string()
    } else {
        format!("the groups {}", names.join(", "))
    }
}

/// What a look at the library folder found.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Scan {
    /// Models that loaded and passed their checks, in [`MODELS`] order.
    pub found: Vec<(&'static str, ParsedModel)>,
    /// Ids with no file at all: to be generated.
    pub missing: Vec<&'static str>,
    /// Ids whose files exist but cannot be used, each with the reason. These are not generated.
    pub problems: Vec<(&'static str, String)>,
}

/// Looks at `dir` for every model of [`MODELS`]. A directory that does not exist gives every id
/// as neither found nor missing: the caller should keep its code-built models (see
/// [`Scan::directory_missing`]).
pub fn scan(dir: &Path) -> Scan {
    let mut out = Scan::default();
    if !dir.is_dir() {
        return out;
    }
    for spec in &MODELS {
        let (obj_path, mtl_path) = (dir.join(format!("{}.obj", spec.id)), dir.join(format!("{}.mtl", spec.id)));
        match (obj_path.exists(), mtl_path.exists()) {
            (false, false) => out.missing.push(spec.id),
            (false, true) => out.problems.push((spec.id, format!("{} exists but {} does not; leaving it alone", mtl_path.display(), obj_path.display()))),
            _ => match obj::load_obj(dir, spec.id).and_then(|model| check_model(spec, &model).map(|()| model)) {
                Ok(model) => out.found.push((spec.id, model)),
                Err(reason) => out.problems.push((spec.id, reason)),
            },
        }
    }
    out
}

impl Scan {
    /// True when nothing at all was looked at (the folder is absent), as opposed to a folder with
    /// no models in it.
    pub fn directory_missing(&self) -> bool {
        self.found.is_empty() && self.missing.is_empty() && self.problems.is_empty()
    }
}

/// Writes the model of `id` into `dir` unless either of its files is already there. Returns the
/// path of the `.obj` written, and what was parsed back from the text that was written (so the
/// game uses exactly what is on disk).
pub fn write_generated(dir: &Path, id: &str, parts: &[obj::Part]) -> Result<(PathBuf, ParsedModel), String> {
    let spec = spec(id).ok_or_else(|| format!("`{id}` is not an object with a library model"))?;
    for extension in ["obj", "mtl"] {
        let path = dir.join(format!("{id}.{extension}"));
        if path.exists() {
            return Err(format!("{} already exists; not overwriting it", path.display()));
        }
    }
    let file = obj::build_obj(id, parts)?;
    let model = obj::parse_obj(&file.obj, Some(&file.mtl))?;
    check_model(spec, &model)?;
    let path = obj::save_obj(dir, id, &file)?;
    Ok((path, model))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::obj::{build_obj, Part, IDENTITY};

    fn part(label: &str) -> Part {
        Part {
            positions: vec![[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]],
            normals: vec![[0.0, 0.0, 1.0]; 3],
            indices: vec![0, 1, 2],
            transform: IDENTITY,
            color: [0.5, 0.5, 0.5, 1.0],
            emissive: [0.0; 3],
            label: label.to_string(),
        }
    }

    fn temp(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("ozbound-models-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn write(dir: &Path, id: &str, parts: &[Part]) {
        let file = build_obj(id, parts).unwrap();
        obj::save_obj(dir, id, &file).unwrap();
    }

    #[test]
    fn ids_are_unique_known_and_safe_file_names() {
        for (i, a) in MODELS.iter().enumerate() {
            assert_eq!(obj::safe_name(a.id), a.id, "{} must be a safe file name", a.id);
            assert!(MODELS.iter().skip(i + 1).all(|b| b.id != a.id), "{} is listed twice", a.id);
            assert_eq!(spec(a.id), Some(a));
        }
        assert_eq!(spec("castle"), None);
    }

    #[test]
    fn every_toggle_is_a_plain_group_name_that_is_not_the_body() {
        for model in &MODELS {
            for toggle in model.toggles {
                assert_eq!(obj::safe_name(toggle), *toggle);
                assert_ne!(*toggle, obj::BODY);
                assert!(!toggle.contains('.') && !toggle.starts_with("part_"));
            }
        }
    }

    #[test]
    fn a_model_must_have_each_hideable_piece() {
        let tree = spec("tree").unwrap();
        let with = obj::parse_obj(&build_obj("tree", &[part(""), part("crown")]).unwrap().obj, None).unwrap();
        assert!(check_model(tree, &with).is_ok());
        let without = obj::parse_obj(&build_obj("tree", &[part(""), part("")]).unwrap().obj, None).unwrap();
        let error = check_model(tree, &without).unwrap_err();
        assert!(error.contains("`crown`") && error.contains("body"), "{error}");
        assert!(check_model(spec("signpost").unwrap(), &without).is_ok());
    }

    #[test]
    fn a_folder_that_is_not_there_is_not_scanned_or_made() {
        let dir = std::env::temp_dir().join(format!("ozbound-models-{}-absent", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let scan = scan(&dir);
        assert!(scan.directory_missing());
        assert!(!dir.exists());
    }

    #[test]
    fn an_empty_folder_wants_every_model_generated() {
        let dir = temp("empty");
        let scan = scan(&dir);
        assert!(!scan.directory_missing());
        assert_eq!(scan.missing, MODELS.map(|m| m.id));
        assert!(scan.found.is_empty() && scan.problems.is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn good_files_load_and_bad_ones_are_reported_not_replaced() {
        let dir = temp("mixed");
        write(&dir, "signpost", &[part("")]);
        write(&dir, "tree", &[part("")]); // no crown
        std::fs::write(dir.join("moonpetal.obj"), "v 1 2\n").unwrap();
        std::fs::write(dir.join("waymark.mtl"), "newmtl a\n").unwrap(); // half a pair
        let scan = scan(&dir);
        assert_eq!(scan.found.iter().map(|(id, _)| *id).collect::<Vec<_>>(), ["signpost"]);
        let problems: Vec<(&str, &str)> = scan.problems.iter().map(|(id, why)| (*id, why.as_str())).collect();
        assert_eq!(problems.iter().map(|(id, _)| *id).collect::<Vec<_>>(), ["tree", "moonpetal", "waymark"]);
        assert!(problems[0].1.contains("crown"));
        assert!(problems[1].1.contains("moonpetal.obj") && problems[1].1.contains("line 1"));
        assert!(problems[2].1.contains("waymark.obj"));
        assert!(!scan.missing.contains(&"tree") && !scan.missing.contains(&"moonpetal") && !scan.missing.contains(&"waymark"));
        assert_eq!(std::fs::read_to_string(dir.join("moonpetal.obj")).unwrap(), "v 1 2\n", "a bad file is left exactly as it was");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn generating_writes_a_pair_once_and_gives_back_what_is_on_disk() {
        let dir = temp("generate");
        let (path, model) = write_generated(&dir, "tree", &[part(""), part("crown")]).unwrap();
        assert_eq!(path, dir.join("tree.obj"));
        assert!(dir.join("tree.mtl").exists());
        assert_eq!(model, obj::load_obj(&dir, "tree").unwrap());
        assert!(model.has_sub_model("crown"));
        // Now it is found, not missing.
        let scan = scan(&dir);
        assert_eq!(scan.found.iter().map(|(id, _)| *id).collect::<Vec<_>>(), ["tree"]);
        assert!(!scan.missing.contains(&"tree"));
        // A second generation refuses and changes nothing.
        let before = std::fs::read_to_string(dir.join("tree.obj")).unwrap();
        let error = write_generated(&dir, "tree", &[part(""), part("crown"), part("crown")]).unwrap_err();
        assert!(error.contains("already exists"), "{error}");
        assert_eq!(std::fs::read_to_string(dir.join("tree.obj")).unwrap(), before);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn generation_refuses_unknown_ids_empty_models_and_models_missing_a_piece() {
        let dir = temp("refuse");
        assert!(write_generated(&dir, "castle", &[part("")]).unwrap_err().contains("not an object"));
        assert!(write_generated(&dir, "signpost", &[]).unwrap_err().contains("no geometry"));
        assert!(write_generated(&dir, "tree", &[part("")]).unwrap_err().contains("crown"));
        assert!(!dir.join("tree.obj").exists() && !dir.join("signpost.obj").exists(), "nothing is written for a refused model");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_shipped_library_files_are_valid_models_with_their_pieces() {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join(crate::asset_file::LIBRARY_DIR);
        let scan = scan(&dir);
        assert!(scan.problems.is_empty(), "{:?}", scan.problems);
        // Every model is shipped, and each one loaded and passed its checks.
        assert_eq!(scan.found.len(), MODELS.len());
        assert!(scan.missing.is_empty(), "{:?} are not in assets/library (run the ignored `write_missing_library_models` test)", scan.missing);
        for (id, model) in &scan.found {
            assert!(model.triangles() > 0, "{id}");
        }
    }
}
