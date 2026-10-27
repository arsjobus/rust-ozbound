//! Original low-poly models, assembled from a handful of flat-shaded primitives.
//! Everything is generated in code, so there are no external assets to license.
//!
//! Props that have a library model (`ozbound::model_library::MODELS`: trees, copper veins, herb
//! patches, berry bushes, moonpetals, signposts, waymarks, the post and rail of a fence, thickets,
//! bridges, garden beds and their crops, vine rows, and the walls, timbers, floor and roof of a
//! house) are
//! spawned as *instances* of `assets/library/<id>.obj`: [`load_models`] reads those files at
//! start-up, generates any that are missing from the code-built models below (never overwriting
//! one that exists), and each `spawn_*` for such a prop places an instance of the file. The code-built
//! `build_*` functions remain as the source of a generated file and as the fallback when a file
//! cannot be used (see `model_library.rs` for the rules).
//!
//! Scale: one world unit is one tile. A character stands about 1.8 units tall.

use std::collections::HashMap;
use std::f32::consts::TAU;
use std::path::PathBuf;

use bevy::prelude::*;
use bevy::render::mesh::{Indices, PrimitiveTopology};
use bevy::render::render_asset::RenderAssetUsages;
use ozbound::appearance::{Appearance, Rgb};
use ozbound::model_library;
use ozbound::obj::{self, ParsedModel};

use crate::components::Limb;

/// Gives a mesh hard-edged facets: every triangle gets its own vertices and normal.
pub fn flat(mut mesh: Mesh) -> Mesh {
    mesh.duplicate_vertices();
    mesh.compute_flat_normals();
    mesh
}

fn material(materials: &mut Assets<StandardMaterial>, r: f32, g: f32, b: f32) -> Handle<StandardMaterial> {
    materials.add(StandardMaterial {
        base_color: Color::srgb(r, g, b),
        perceptual_roughness: 1.0,
        reflectance: 0.05,
        ..default()
    })
}

fn shade_of(materials: &mut Assets<StandardMaterial>, color: Rgb) -> Handle<StandardMaterial> {
    material(materials, f32::from(color[0]) / 255.0, f32::from(color[1]) / 255.0, f32::from(color[2]) / 255.0)
}

/// Builds the 3D look of a character from the same `Appearance` its portrait is drawn from.
fn look_from(materials: &mut Assets<StandardMaterial>, a: &Appearance) -> Look {
    Look {
        skin: shade_of(materials, a.skin),
        tunic: shade_of(materials, a.tunic),
        legs: shade_of(materials, a.legs),
        hat: shade_of(materials, a.hat),
        wide_brim: a.wide_brim,
    }
}

/// Colours and hat style of one character.
#[derive(Clone)]
pub struct Look {
    pub skin: Handle<StandardMaterial>,
    pub tunic: Handle<StandardMaterial>,
    pub legs: Handle<StandardMaterial>,
    pub hat: Handle<StandardMaterial>,
    pub wide_brim: bool,
}

/// Shared mesh and material handles, built once at startup.
#[derive(Resource)]
pub struct Kit {
    pub grass: [Handle<StandardMaterial>; 3],
    pub dark_grass: Handle<StandardMaterial>,
    pub path: [Handle<StandardMaterial>; 2],
    pub water: [Handle<StandardMaterial>; 2],

    pub trunk: Handle<Mesh>,
    pub crowns: [Handle<Mesh>; 3],
    pub bark: Handle<StandardMaterial>,
    pub plank: Handle<StandardMaterial>,
    pub leaves: [Handle<StandardMaterial>; 3],

    pub rock: Handle<Mesh>,
    pub stone: [Handle<StandardMaterial>; 2],
    pub fleck: Handle<Mesh>,
    pub copper: Handle<StandardMaterial>,

    pub stem: Handle<Mesh>,
    pub bloom: Handle<Mesh>,
    pub petal: Handle<Mesh>,
    pub stem_green: Handle<StandardMaterial>,
    pub berry: Handle<Mesh>,
    pub berry_red: Handle<StandardMaterial>,
    pub bloom_glow: Handle<StandardMaterial>,
    pub petal_glow: Handle<StandardMaterial>,

    pub roof_cone: Handle<Mesh>,
    pub pile: Handle<Mesh>,
    pub wall: Handle<StandardMaterial>,
    pub roofs: [Handle<StandardMaterial>; 2],
    pub soil: Handle<StandardMaterial>,
    pub cabbage: Handle<StandardMaterial>,
    pub grape: Handle<StandardMaterial>,
    pub thicket: [Handle<StandardMaterial>; 2],

    pub cube: Handle<Mesh>,
    pub hat_cone: Handle<Mesh>,
    pub brim: Handle<Mesh>,
    pub dark: Handle<StandardMaterial>,
    pub traveller: Look,
    pub guide: Look,
    /// One look per named appearance (`Appearance::KEYS`), for the villagers.
    pub npc_looks: Vec<(&'static str, Look)>,
    /// The library models that props are spawned from (empty until [`load_models`] has run).
    pub instances: Instances,
}

impl Kit {
    pub fn new(meshes: &mut Assets<Mesh>, materials: &mut Assets<StandardMaterial>) -> Self {
        Self {
            grass: [
                material(materials, 0.33, 0.55, 0.28),
                material(materials, 0.38, 0.60, 0.30),
                material(materials, 0.29, 0.50, 0.27),
            ],
            dark_grass: material(materials, 0.20, 0.38, 0.22),
            path: [material(materials, 0.74, 0.64, 0.45), material(materials, 0.69, 0.59, 0.42)],
            water: [material(materials, 0.18, 0.42, 0.72), material(materials, 0.22, 0.48, 0.78)],

            trunk: meshes.add(flat(Cylinder::new(0.13, 0.8).mesh().resolution(6).into())),
            crowns: [
                meshes.add(flat(Cone::new(0.80, 0.95).mesh().resolution(6).into())),
                meshes.add(flat(Cone::new(0.62, 0.85).mesh().resolution(6).into())),
                meshes.add(flat(Cone::new(0.42, 0.75).mesh().resolution(6).into())),
            ],
            bark: material(materials, 0.38, 0.26, 0.15),
            plank: material(materials, 0.66, 0.50, 0.30),
            leaves: [
                material(materials, 0.16, 0.42, 0.20),
                material(materials, 0.20, 0.48, 0.22),
                material(materials, 0.13, 0.36, 0.22),
            ],

            rock: meshes.add(flat(Sphere::new(0.5).mesh().ico(0).expect("ico(0) is valid"))),
            stone: [material(materials, 0.50, 0.50, 0.53), material(materials, 0.42, 0.42, 0.46)],
            fleck: meshes.add(Cuboid::new(0.16, 0.12, 0.14)),
            copper: material(materials, 0.85, 0.48, 0.24),

            stem: meshes.add(flat(Cylinder::new(0.025, 0.3).mesh().resolution(5).into())),
            bloom: meshes.add(flat(Sphere::new(0.09).mesh().ico(1).expect("ico(1) is valid"))),
            petal: meshes.add(Cuboid::new(0.17, 0.02, 0.07)),
            stem_green: material(materials, 0.25, 0.55, 0.28),
            berry: meshes.add(flat(Sphere::new(0.07).mesh().ico(1).expect("ico(1) is valid"))),
            berry_red: material(materials, 0.78, 0.16, 0.30),
            bloom_glow: materials.add(StandardMaterial {
                base_color: Color::srgb(0.80, 0.74, 0.98),
                emissive: LinearRgba::rgb(0.55, 0.45, 1.1),
                perceptual_roughness: 0.6,
                ..default()
            }),
            petal_glow: materials.add(StandardMaterial {
                base_color: Color::srgb(0.90, 0.86, 1.0),
                emissive: LinearRgba::rgb(0.35, 0.30, 0.8),
                perceptual_roughness: 0.6,
                ..default()
            }),

            roof_cone: meshes.add(flat(Cone::new(std::f32::consts::FRAC_1_SQRT_2, 1.0).mesh().resolution(4).into())),
            pile: meshes.add(flat(Cylinder::new(0.06, 0.5).mesh().resolution(6).into())),
            wall: material(materials, 0.93, 0.89, 0.80),
            roofs: [material(materials, 0.22, 0.34, 0.62), material(materials, 0.62, 0.28, 0.22)],
            soil: material(materials, 0.28, 0.19, 0.12),
            cabbage: material(materials, 0.55, 0.72, 0.35),
            grape: material(materials, 0.40, 0.18, 0.48),
            thicket: [material(materials, 0.10, 0.26, 0.15), material(materials, 0.13, 0.31, 0.17)],

            cube: meshes.add(Cuboid::new(1.0, 1.0, 1.0)),
            hat_cone: meshes.add(flat(Cone::new(0.21, 0.32).mesh().resolution(8).into())),
            brim: meshes.add(flat(Cylinder::new(0.34, 0.04).mesh().resolution(8).into())),
            dark: material(materials, 0.12, 0.10, 0.10),
            traveller: look_from(materials, &Appearance::from_key("traveller").expect("known appearance")),
            guide: look_from(materials, &Appearance::from_key("guide").expect("known appearance")),
            npc_looks: Appearance::KEYS
                .iter()
                .map(|key| (*key, look_from(materials, &Appearance::from_key(key).expect("listed appearance"))))
                .collect(),
            instances: Instances::default(),
        }
    }
}

fn part(mesh: &Handle<Mesh>, material: &Handle<StandardMaterial>, transform: Transform) -> impl Bundle {
    (Mesh3d(mesh.clone()), MeshMaterial3d(material.clone()), transform)
}

fn pivot(transform: Transform) -> impl Bundle {
    (transform, Visibility::default())
}

/// A tree: trunk plus three stacked cones. Returns `(root, crown)`; hiding `crown` leaves a stump.
fn build_tree(commands: &mut Commands, kit: &Kit, at: Vec3, yaw: f32, scale: f32, leaf: usize) -> (Entity, Entity) {
    let root = commands.spawn(pivot(Transform::from_translation(at).with_rotation(Quat::from_rotation_y(yaw)).with_scale(Vec3::splat(scale)))).id();
    let trunk = commands.spawn(part(&kit.trunk, &kit.bark, Transform::from_xyz(0.0, 0.4, 0.0))).id();
    let crown = commands.spawn((pivot(Transform::IDENTITY), Name::new("crown"))).id();
    let leaves = &kit.leaves[leaf % kit.leaves.len()];
    let layers = [(0, 1.10), (1, 1.65), (2, 2.15)];
    for (i, y) in layers {
        let cone = commands.spawn(part(&kit.crowns[i], leaves, Transform::from_xyz(0.0, y, 0.0).with_rotation(Quat::from_rotation_y(i as f32 * 0.5)))).id();
        commands.entity(crown).add_child(cone);
    }
    commands.entity(root).add_children(&[trunk, crown]);
    (root, crown)
}

/// A patch of meadowmint: a bare soil patch (always visible, so the player can see where it
/// regrows) with a tuft of leafy stems. Returns `(root, plants)`; hiding `plants` leaves the soil.
fn build_herb_patch(commands: &mut Commands, kit: &Kit, at: Vec3, yaw: f32) -> (Entity, Entity) {
    let root = commands.spawn(pivot(Transform::from_translation(at).with_rotation(Quat::from_rotation_y(yaw)))).id();
    let soil = commands.spawn(part(&kit.cube, &kit.path[1], Transform::from_xyz(0.0, 0.02, 0.0).with_scale(Vec3::new(0.7, 0.04, 0.7)))).id();
    let plants = commands.spawn((pivot(Transform::IDENTITY), Name::new("plants"))).id();
    // (x, z, lean about x, lean about z, height scale)
    let tuft = [(0.0, 0.0, 0.0, 0.0, 1.3), (0.18, 0.05, 0.0, -0.3, 1.0), (-0.17, 0.08, 0.1, 0.3, 1.1), (0.05, -0.2, -0.3, 0.1, 0.9), (-0.08, -0.15, -0.2, -0.1, 1.0)];
    for (x, z, lean_x, lean_z, tall) in tuft {
        let stem = commands
            .spawn(part(&kit.stem, &kit.stem_green, Transform::from_xyz(x, 0.15 * tall + 0.04, z).with_rotation(Quat::from_euler(EulerRot::XYZ, lean_x, 0.0, lean_z)).with_scale(Vec3::new(1.4, tall, 1.4))))
            .id();
        let leaf = commands
            .spawn(part(&kit.petal, &kit.leaves[1], Transform::from_xyz(x, 0.3 * tall + 0.04, z).with_rotation(Quat::from_euler(EulerRot::XYZ, lean_x, x * 9.0, lean_z))))
            .id();
        commands.entity(plants).add_children(&[stem, leaf]);
    }
    commands.entity(root).add_children(&[soil, plants]);
    (root, plants)
}

/// A honeyberry bush: a low leafy mound dotted with berries. Returns `(root, berries)`;
/// hiding `berries` leaves the bare bush.
fn build_berry_bush(commands: &mut Commands, kit: &Kit, at: Vec3, yaw: f32) -> (Entity, Entity) {
    let root = commands.spawn(pivot(Transform::from_translation(at).with_rotation(Quat::from_rotation_y(yaw)))).id();
    let mound = commands.spawn(part(&kit.rock, &kit.leaves[1], Transform::from_xyz(0.0, 0.28, 0.0).with_scale(Vec3::new(1.05, 0.62, 0.95)))).id();
    let berries = commands.spawn((pivot(Transform::IDENTITY), Name::new("berries"))).id();
    let dots = [(0.25, 0.45, 0.22), (-0.28, 0.40, 0.18), (0.05, 0.56, -0.12), (-0.12, 0.34, -0.34), (0.32, 0.30, -0.18), (-0.02, 0.38, 0.38)];
    for (x, y, z) in dots {
        let berry = commands.spawn(part(&kit.berry, &kit.berry_red, Transform::from_xyz(x, y, z))).id();
        commands.entity(berries).add_child(berry);
    }
    commands.entity(root).add_children(&[mound, berries]);
    (root, berries)
}

/// One tile of fence: a post in the middle with two rails reaching toward each linked
/// neighbour, so a row of fence tiles reads as one continuous fence. `rails` are unit map
/// offsets `(dx, dy)` from [`ozbound::scene::FenceLinks::rails`]; map `+y` is world `-z`.
fn build_fence(commands: &mut Commands, kit: &Kit, at: Vec3, rails: &[(i32, i32)]) -> Entity {
    let root = commands.spawn(pivot(Transform::from_translation(at))).id();
    let post = commands.spawn(part(&kit.cube, &kit.bark, Transform::from_xyz(0.0, 0.42, 0.0).with_scale(Vec3::new(0.14, 0.84, 0.14)))).id();
    commands.entity(root).add_child(post);
    for &(dx, dy) in rails {
        // Half a tile long, so rails from two neighbouring tiles meet at the tile edge.
        let (along_x, sign) = if dx != 0 { (true, dx as f32) } else { (false, -(dy as f32)) };
        for height in [0.34, 0.64] {
            let (offset, size) = if along_x {
                (Vec3::new(sign * 0.25, height, 0.0), Vec3::new(0.5, 0.07, 0.05))
            } else {
                (Vec3::new(0.0, height, sign * 0.25), Vec3::new(0.05, 0.07, 0.5))
            };
            let rail = commands.spawn(part(&kit.cube, &kit.plank, Transform::from_translation(offset).with_scale(size))).id();
            commands.entity(root).add_child(rail);
        }
    }
    root
}

/// A signpost: a wooden post with a painted board, turned a little so no two look identical.
fn build_signpost(commands: &mut Commands, kit: &Kit, at: Vec3, yaw: f32) -> Entity {
    let root = commands.spawn(pivot(Transform::from_translation(at).with_rotation(Quat::from_rotation_y(yaw)))).id();
    let post = commands.spawn(part(&kit.cube, &kit.bark, Transform::from_xyz(0.0, 0.5, 0.0).with_scale(Vec3::new(0.10, 1.0, 0.10)))).id();
    let board = commands.spawn(part(&kit.cube, &kit.plank, Transform::from_xyz(0.0, 0.92, 0.07).with_scale(Vec3::new(0.72, 0.38, 0.06)))).id();
    // Dark strokes standing in for painted words.
    let line_a = commands.spawn(part(&kit.cube, &kit.dark, Transform::from_xyz(0.0, 0.98, 0.105).with_scale(Vec3::new(0.50, 0.04, 0.01)))).id();
    let line_b = commands.spawn(part(&kit.cube, &kit.dark, Transform::from_xyz(-0.06, 0.88, 0.105).with_scale(Vec3::new(0.38, 0.04, 0.01)))).id();
    commands.entity(root).add_children(&[post, board, line_a, line_b]);
    root
}

/// A boulder made of two squashed icospheres. With `copper`, orange flecks stud the surface.
/// Returns `(root, flecks)`; the flecks entity exists only when `copper` is true.
fn build_rock(commands: &mut Commands, kit: &Kit, at: Vec3, yaw: f32, scale: f32, shade: usize, copper: bool) -> (Entity, Option<Entity>) {
    let stone = &kit.stone[shade % kit.stone.len()];
    let root = commands.spawn(pivot(Transform::from_translation(at).with_rotation(Quat::from_rotation_y(yaw)).with_scale(Vec3::splat(scale)))).id();
    let big = commands.spawn(part(&kit.rock, stone, Transform::from_xyz(0.0, 0.28, 0.0).with_scale(Vec3::new(1.55, 1.05, 1.35)))).id();
    let small = commands.spawn(part(&kit.rock, stone, Transform::from_xyz(0.28, 0.16, -0.22).with_rotation(Quat::from_rotation_y(0.9)).with_scale(Vec3::new(0.8, 0.6, 0.7)))).id();
    commands.entity(root).add_children(&[big, small]);
    let flecks = copper.then(|| {
        let group = commands.spawn((pivot(Transform::IDENTITY), Name::new("flecks"))).id();
        let spots = [(-0.30, 0.50, 0.12, 0.4), (0.05, 0.60, 0.22, 1.1), (0.34, 0.34, 0.30, 2.0), (-0.12, 0.30, 0.46, 0.2)];
        for (x, y, z, turn) in spots {
            let fleck = commands.spawn(part(&kit.fleck, &kit.copper, Transform::from_xyz(x, y, z).with_rotation(Quat::from_euler(EulerRot::YXZ, turn, 0.5, 0.3)))).id();
            commands.entity(group).add_child(fleck);
        }
        commands.entity(root).add_child(group);
        group
    });
    (root, flecks)
}

/// A glowing moonpetal flower: stem, blossom, and a ring of six flat petals.
fn build_moonpetal(commands: &mut Commands, kit: &Kit, at: Vec3) -> Entity {
    let root = commands.spawn(pivot(Transform::from_translation(at))).id();
    let stem = commands.spawn(part(&kit.stem, &kit.stem_green, Transform::from_xyz(0.0, 0.15, 0.0))).id();
    let bloom = commands.spawn(part(&kit.bloom, &kit.bloom_glow, Transform::from_xyz(0.0, 0.34, 0.0))).id();
    commands.entity(root).add_children(&[stem, bloom]);
    for i in 0..6 {
        let angle = i as f32 * TAU / 6.0;
        let ring = Quat::from_rotation_y(angle);
        let petal = commands.spawn(part(
            &kit.petal,
            &kit.petal_glow,
            Transform::from_translation(ring * Vec3::new(0.11, 0.33, 0.0)).with_rotation(ring * Quat::from_rotation_z(0.45)),
        )).id();
        commands.entity(root).add_child(petal);
    }
    root
}

/// A blocky traveller built from cubes, with swinging arms and legs. The model faces +Z.
pub fn spawn_humanoid(commands: &mut Commands, kit: &Kit, look: &Look, at: Vec3, yaw: f32) -> Entity {
    let root = commands.spawn(pivot(Transform::from_translation(at).with_rotation(Quat::from_rotation_y(yaw)))).id();
    let cube = |size: Vec3, y: f32| Transform::from_xyz(0.0, y, 0.0).with_scale(size);

    let torso = commands.spawn(part(&kit.cube, &look.tunic, Transform::from_xyz(0.0, 0.895, 0.0).with_scale(Vec3::new(0.50, 0.55, 0.28)))).id();
    let head = commands.spawn(part(&kit.cube, &look.skin, Transform::from_xyz(0.0, 1.35, 0.0).with_scale(Vec3::splat(0.36)))).id();
    let eye_left = commands.spawn(part(&kit.cube, &kit.dark, Transform::from_xyz(-0.08, 1.38, 0.18).with_scale(Vec3::splat(0.05)))).id();
    let eye_right = commands.spawn(part(&kit.cube, &kit.dark, Transform::from_xyz(0.08, 1.38, 0.18).with_scale(Vec3::splat(0.05)))).id();
    let cap = commands.spawn(part(&kit.hat_cone, &look.hat, Transform::from_xyz(0.0, 1.69, 0.0))).id();
    commands.entity(root).add_children(&[torso, head, eye_left, eye_right, cap]);
    if look.wide_brim {
        let brim = commands.spawn(part(&kit.brim, &look.hat, Transform::from_xyz(0.0, 1.54, 0.0))).id();
        commands.entity(root).add_child(brim);
    }

    // Limbs hang from pivots at the hip and shoulder so rotating the pivot swings the limb.
    let limbs = [
        (-0.12, 0.62, 0.62, Vec3::new(0.20, 0.62, 0.22), &look.legs, 1.0, false),
        (0.12, 0.62, 0.62, Vec3::new(0.20, 0.62, 0.22), &look.legs, -1.0, false),
        (-0.34, 1.13, 0.50, Vec3::new(0.14, 0.50, 0.16), &look.tunic, -1.0, true),
        (0.34, 1.13, 0.50, Vec3::new(0.14, 0.50, 0.16), &look.tunic, 1.0, true),
    ];
    for (x, y, length, size, material, swing, is_arm) in limbs {
        let hinge = commands.spawn((pivot(Transform::from_xyz(x, y, 0.0)), Limb { owner: root, swing, is_arm, side: x.signum() })).id();
        let shape = commands.spawn(part(&kit.cube, material, cube(size, -length / 2.0))).id();
        commands.entity(hinge).add_child(shape);
        commands.entity(root).add_child(hinge);
    }
    root
}

impl Kit {
    /// The look for a named appearance, falling back to the traveller's.
    pub fn look_for(&self, key: &str) -> &Look {
        self.npc_looks.iter().find(|(k, _)| *k == key).map_or(&self.traveller, |(_, look)| look)
    }
}

/// A standing stone with a faintly glowing cap: the marker for a named place.
fn build_waymark(commands: &mut Commands, kit: &Kit, at: Vec3, yaw: f32) -> Entity {
    let root = commands.spawn(pivot(Transform::from_translation(at).with_rotation(Quat::from_rotation_y(yaw)))).id();
    let stone = commands.spawn(part(&kit.cube, &kit.stone[1], Transform::from_xyz(0.0, 0.5, 0.0).with_scale(Vec3::new(0.30, 1.0, 0.22)))).id();
    let cap = commands.spawn(part(&kit.bloom, &kit.bloom_glow, Transform::from_xyz(0.0, 1.1, 0.0))).id();
    commands.entity(root).add_children(&[stone, cap]);
    root
}

/// A creature standing at `at`. Each bestiary id has its own small model; an id without one gets
/// a plain furry lump, so new bestiary entries still show up in the world. Returns the root.
pub fn spawn_creature(commands: &mut Commands, kit: &Kit, id: &str, at: Vec3, yaw: f32) -> Entity {
    let root = commands.spawn(pivot(Transform::from_translation(at).with_rotation(Quat::from_rotation_y(yaw)))).id();
    let mut children = Vec::new();
    let mut add = |mesh: &Handle<Mesh>, material: &Handle<StandardMaterial>, transform: Transform| {
        children.push(commands.spawn(part(mesh, material, transform)).id());
    };
    match id {
        "meadow_hare" => {
            add(&kit.rock, &kit.plank, Transform::from_xyz(0.0, 0.22, 0.0).with_scale(Vec3::new(0.34, 0.28, 0.5)));
            add(&kit.rock, &kit.plank, Transform::from_xyz(0.0, 0.34, -0.26).with_scale(Vec3::splat(0.24)));
            add(&kit.cube, &kit.plank, Transform::from_xyz(-0.06, 0.58, -0.27).with_rotation(Quat::from_rotation_x(-0.2)).with_scale(Vec3::new(0.05, 0.26, 0.04)));
            add(&kit.cube, &kit.plank, Transform::from_xyz(0.06, 0.58, -0.27).with_rotation(Quat::from_rotation_x(-0.2)).with_scale(Vec3::new(0.05, 0.26, 0.04)));
            add(&kit.rock, &kit.stone[0], Transform::from_xyz(0.0, 0.22, 0.28).with_scale(Vec3::splat(0.12)));
            add(&kit.berry, &kit.dark, Transform::from_xyz(-0.07, 0.38, -0.40).with_scale(Vec3::splat(0.6)));
            add(&kit.berry, &kit.dark, Transform::from_xyz(0.07, 0.38, -0.40).with_scale(Vec3::splat(0.6)));
        }
        "pond_heron" => {
            add(&kit.cube, &kit.dark, Transform::from_xyz(-0.05, 0.35, 0.0).with_scale(Vec3::new(0.03, 0.7, 0.03)));
            add(&kit.cube, &kit.dark, Transform::from_xyz(0.05, 0.35, 0.0).with_scale(Vec3::new(0.03, 0.7, 0.03)));
            add(&kit.rock, &kit.stone[0], Transform::from_xyz(0.0, 0.85, 0.0).with_scale(Vec3::new(0.30, 0.34, 0.55)));
            add(&kit.cube, &kit.stone[0], Transform::from_xyz(0.0, 1.15, -0.22).with_scale(Vec3::new(0.06, 0.50, 0.06)));
            add(&kit.rock, &kit.stone[0], Transform::from_xyz(0.0, 1.42, -0.27).with_scale(Vec3::splat(0.16)));
            add(&kit.cube, &kit.plank, Transform::from_xyz(0.0, 1.41, -0.46).with_scale(Vec3::new(0.04, 0.04, 0.28)));
        }
        "lantern_moth" => {
            add(&kit.cube, &kit.dark, Transform::from_xyz(0.0, 0.0, 0.0).with_scale(Vec3::new(0.06, 0.06, 0.26)));
            add(&kit.cube, &kit.petal_glow, Transform::from_xyz(-0.26, 0.04, 0.0).with_rotation(Quat::from_rotation_z(0.35)).with_scale(Vec3::new(0.46, 0.02, 0.34)));
            add(&kit.cube, &kit.petal_glow, Transform::from_xyz(0.26, 0.04, 0.0).with_rotation(Quat::from_rotation_z(-0.35)).with_scale(Vec3::new(0.46, 0.02, 0.34)));
        }
        _ => add(&kit.rock, &kit.plank, Transform::from_xyz(0.0, 0.2, 0.0).with_scale(Vec3::new(0.45, 0.35, 0.5))),
    }
    commands.entity(root).add_children(&children);
    root
}

/// A thicket: a tangle of dark leafy mounds that fills its tile, with nothing sticking out of it.
/// It marks a tree tile that cannot be chopped, so it never looks like the tall pine of a
/// harvestable tree. (An earlier version had bare brown branches poking out of the top; they
/// read as sticks in a bush and were removed.) The darker mounds are the model's body and the
/// lighter ones its `shade_b` piece; the tile swaps the two colours (`shade`).
pub fn spawn_thicket(commands: &mut Commands, kit: &Kit, at: Vec3, yaw: f32, scale: f32, shade: usize) -> Entity {
    let transform = Transform::from_translation(at).with_rotation(Quat::from_rotation_y(yaw)).with_scale(Vec3::splat(scale));
    let recolor = [(obj::BODY, &kit.thicket[shade % 2]), ("shade_b", &kit.thicket[(shade + 1) % 2])];
    match kit.instances.spawn(commands, "thicket", transform, &recolor) {
        Some(placed) => placed.root,
        None => build_thicket(commands, kit, at, yaw, scale, shade),
    }
}

fn build_thicket(commands: &mut Commands, kit: &Kit, at: Vec3, yaw: f32, scale: f32, shade: usize) -> Entity {
    let root = commands.spawn(pivot(Transform::from_translation(at).with_rotation(Quat::from_rotation_y(yaw)).with_scale(Vec3::splat(scale)))).id();
    let leaves = [&kit.thicket[shade % 2], &kit.thicket[(shade + 1) % 2]];
    let lighter = commands.spawn((pivot(Transform::IDENTITY), Name::new("shade_b"))).id();
    commands.entity(root).add_child(lighter);
    // (x, z, width, height, which leaf shade)
    let mounds = [(0.0, 0.0, 1.0, 0.95, 0), (0.28, 0.22, 0.7, 0.75, 1), (-0.27, 0.2, 0.72, 0.7, 1), (0.22, -0.28, 0.68, 0.8, 1), (-0.25, -0.26, 0.74, 0.65, 0), (0.0, 0.34, 0.5, 0.55, 0)];
    for (x, z, size, tall, which) in mounds {
        let mound = commands.spawn(part(&kit.rock, leaves[which], Transform::from_xyz(x, 0.3 * tall + 0.05, z).with_scale(Vec3::new(size, tall, size)))).id();
        commands.entity(if which == 0 { root } else { lighter }).add_child(mound);
    }
    root
}

/// One tile of wooden bridge. The deck runs along east-west, or north-south when `along_x` is
/// false. Rails stand on both water-facing sides; end posts stand only where the bridge
/// starts (`first`) or finishes (`last`), so a long bridge has one post at each end.
pub fn spawn_bridge_tile(commands: &mut Commands, kit: &Kit, at: Vec3, along_x: bool, first: bool, last: bool) -> Entity {
    if !(kit.instances.has("bridge_deck") && kit.instances.has("bridge_end")) {
        return build_bridge_tile(commands, kit, at, along_x, first, last);
    }
    // Everything is modelled along local x, then the whole bridge tile is turned for north-south.
    let turn = if along_x { 0.0 } else { std::f32::consts::FRAC_PI_2 };
    let root = commands.spawn(pivot(Transform::from_translation(at).with_rotation(Quat::from_rotation_y(turn)))).id();
    if let Some(deck) = kit.instances.spawn(commands, "bridge_deck", Transform::IDENTITY, &[]) {
        commands.entity(root).add_child(deck.root);
    }
    // The end posts are modelled at the `+x` end; half a turn puts the pair at the `-x` end.
    for (wanted, yaw) in [(first, std::f32::consts::PI), (last, 0.0)] {
        if wanted {
            if let Some(end) = kit.instances.spawn(commands, "bridge_end", Transform::from_rotation(Quat::from_rotation_y(yaw)), &[]) {
                commands.entity(root).add_child(end.root);
            }
        }
    }
    root
}

/// The deck of a bridge tile with its rails and piles (no end posts), along local x.
fn build_bridge_deck(commands: &mut Commands, kit: &Kit, at: Vec3) -> Entity {
    let root = commands.spawn(pivot(Transform::from_translation(at))).id();
    let deck = commands.spawn(part(&kit.cube, &kit.plank, Transform::from_xyz(0.0, 0.03, 0.0).with_scale(Vec3::new(1.0, 0.1, 0.94)))).id();
    commands.entity(root).add_child(deck);
    // Plank seams: thin dark strips across the deck.
    for i in 0..5 {
        let x = -0.4 + 0.2 * i as f32;
        let seam = commands.spawn(part(&kit.cube, &kit.bark, Transform::from_xyz(x, 0.085, 0.0).with_scale(Vec3::new(0.03, 0.01, 0.94)))).id();
        commands.entity(root).add_child(seam);
    }
    for side in [-1.0f32, 1.0] {
        let z = side * 0.44;
        let rail = commands.spawn(part(&kit.cube, &kit.bark, Transform::from_xyz(0.0, 0.62, z).with_scale(Vec3::new(1.0, 0.07, 0.07)))).id();
        let low = commands.spawn(part(&kit.cube, &kit.plank, Transform::from_xyz(0.0, 0.36, z).with_scale(Vec3::new(1.0, 0.05, 0.05)))).id();
        let mid_post = commands.spawn(part(&kit.cube, &kit.bark, Transform::from_xyz(0.0, 0.34, z).with_scale(Vec3::new(0.09, 0.62, 0.09)))).id();
        commands.entity(root).add_children(&[rail, low, mid_post]);
        // Piles going down into the river under each side.
        let pile = commands.spawn(part(&kit.pile, &kit.bark, Transform::from_xyz(0.0, -0.22, side * 0.38))).id();
        commands.entity(root).add_child(pile);
    }
    root
}

/// The two end posts (one on each rail, with a cap) at the `+x` end of a bridge tile.
fn build_bridge_end(commands: &mut Commands, kit: &Kit, at: Vec3) -> Entity {
    let root = commands.spawn(pivot(Transform::from_translation(at))).id();
    for side in [-1.0f32, 1.0] {
        let (x, z) = (0.46, side * 0.44);
        let post = commands.spawn(part(&kit.cube, &kit.bark, Transform::from_xyz(x, 0.4, z).with_scale(Vec3::new(0.13, 0.78, 0.13)))).id();
        let cap = commands.spawn(part(&kit.cube, &kit.plank, Transform::from_xyz(x, 0.8, z).with_scale(Vec3::new(0.19, 0.05, 0.19)))).id();
        commands.entity(root).add_children(&[post, cap]);
    }
    root
}

/// The code-built bridge tile: the deck, and an end-post pair at each end that is wanted.
fn build_bridge_tile(commands: &mut Commands, kit: &Kit, at: Vec3, along_x: bool, first: bool, last: bool) -> Entity {
    let turn = if along_x { 0.0 } else { std::f32::consts::FRAC_PI_2 };
    let root = commands.spawn(pivot(Transform::from_translation(at).with_rotation(Quat::from_rotation_y(turn)))).id();
    let deck = build_bridge_deck(commands, kit, Vec3::ZERO);
    commands.entity(root).add_child(deck);
    for (wanted, yaw) in [(first, std::f32::consts::PI), (last, 0.0)] {
        if wanted {
            let end = build_bridge_end(commands, kit, Vec3::ZERO);
            commands.entity(end).insert(Transform::from_rotation(Quat::from_rotation_y(yaw)));
            commands.entity(root).add_child(end);
        }
    }
    root
}

/// A raised garden bed: a wooden frame round dark soil, planted with one of three crops. The
/// bed and each crop are separate library models, so a crop can be reshaped on its own.
pub fn spawn_garden_bed(commands: &mut Commands, kit: &Kit, at: Vec3, yaw: f32, crop: usize) -> Entity {
    let crop_id = CROPS[crop % 3];
    if !(kit.instances.has("garden_bed") && kit.instances.has(crop_id)) {
        return build_garden_bed(commands, kit, at, yaw, crop);
    }
    let root = commands.spawn(pivot(Transform::from_translation(at).with_rotation(Quat::from_rotation_y(yaw)))).id();
    for id in ["garden_bed", crop_id] {
        if let Some(placed) = kit.instances.spawn(commands, id, Transform::IDENTITY, &[]) {
            commands.entity(root).add_child(placed.root);
        }
    }
    root
}

/// The library model of each crop, in the order `crop % 3` picks them.
const CROPS: [&str; 3] = ["crop_cabbage", "crop_tomato", "crop_pumpkin"];

/// The frame and soil of a garden bed.
fn build_garden_frame(commands: &mut Commands, kit: &Kit, at: Vec3) -> Entity {
    let root = commands.spawn(pivot(Transform::from_translation(at))).id();
    let soil = commands.spawn(part(&kit.cube, &kit.soil, Transform::from_xyz(0.0, 0.1, 0.0).with_scale(Vec3::new(0.86, 0.2, 0.86)))).id();
    commands.entity(root).add_child(soil);
    for (x, z, sx, sz) in [(0.0, 0.45, 0.96, 0.08), (0.0, -0.45, 0.96, 0.08), (0.45, 0.0, 0.08, 0.86), (-0.45, 0.0, 0.08, 0.86)] {
        let board = commands.spawn(part(&kit.cube, &kit.plank, Transform::from_xyz(x, 0.14, z).with_scale(Vec3::new(sx, 0.28, sz)))).id();
        commands.entity(root).add_child(board);
    }
    root
}

/// Nine plants of one crop (0 cabbages, 1 tomato bushes with fruit, 2 pumpkins), in the bed.
fn build_crop(commands: &mut Commands, kit: &Kit, at: Vec3, crop: usize) -> Entity {
    let root = commands.spawn(pivot(Transform::from_translation(at))).id();
    for row in 0..3 {
        for col in 0..3 {
            let x = -0.26 + 0.26 * col as f32;
            let z = -0.26 + 0.26 * row as f32;
            let (mesh, material, size, lift) = match crop % 3 {
                0 => (&kit.rock, &kit.cabbage, Vec3::new(0.26, 0.2, 0.26), 0.3), // cabbages
                1 => (&kit.rock, &kit.leaves[1], Vec3::new(0.2, 0.34, 0.2), 0.36), // tomato bushes
                _ => (&kit.rock, &kit.copper, Vec3::new(0.24, 0.18, 0.24), 0.29), // pumpkins
            };
            let plant = commands.spawn(part(mesh, material, Transform::from_xyz(x, lift, z).with_scale(size))).id();
            commands.entity(root).add_child(plant);
            if crop % 3 == 1 && (row + col) % 2 == 0 {
                let fruit = commands.spawn(part(&kit.berry, &kit.berry_red, Transform::from_xyz(x + 0.05, lift + 0.12, z + 0.05))).id();
                commands.entity(root).add_child(fruit);
            }
        }
    }
    root
}

/// The code-built garden bed: frame, soil, and the crop.
fn build_garden_bed(commands: &mut Commands, kit: &Kit, at: Vec3, yaw: f32, crop: usize) -> Entity {
    let root = commands.spawn(pivot(Transform::from_translation(at).with_rotation(Quat::from_rotation_y(yaw)))).id();
    let frame = build_garden_frame(commands, kit, Vec3::ZERO);
    let plants = build_crop(commands, kit, Vec3::ZERO, crop);
    commands.entity(root).add_children(&[frame, plants]);
    root
}

/// One tile of a vineyard row: a trellis (two posts and two wires) hung with leafy vine and
/// bunches of grapes. The row runs east-west, or north-south when `along_x` is false. The model
/// holds four bunches (`bunch0` to `bunch3`); the first `bunches` (at most four) are kept.
pub fn spawn_vine_row(commands: &mut Commands, kit: &Kit, at: Vec3, along_x: bool, bunches: usize) -> Entity {
    let turn = if along_x { 0.0 } else { std::f32::consts::FRAC_PI_2 };
    let transform = Transform::from_translation(at).with_rotation(Quat::from_rotation_y(turn));
    match kit.instances.spawn(commands, "vine_row", transform, &[]) {
        Some(placed) => {
            for (i, name) in VINE_BUNCHES.iter().enumerate() {
                if i >= bunches {
                    commands.entity(placed.toggle(name)).despawn_recursive();
                }
            }
            placed.root
        }
        None => build_vine_row(commands, kit, at, along_x, bunches),
    }
}

/// The holders of the grape bunches of a vine row, in the order they are kept.
const VINE_BUNCHES: [&str; 4] = ["bunch0", "bunch1", "bunch2", "bunch3"];

fn build_vine_row(commands: &mut Commands, kit: &Kit, at: Vec3, along_x: bool, bunches: usize) -> Entity {
    let turn = if along_x { 0.0 } else { std::f32::consts::FRAC_PI_2 };
    let root = commands.spawn(pivot(Transform::from_translation(at).with_rotation(Quat::from_rotation_y(turn)))).id();
    for x in [-0.46, 0.46] {
        let post = commands.spawn(part(&kit.cube, &kit.bark, Transform::from_xyz(x, 0.5, 0.0).with_scale(Vec3::new(0.09, 1.0, 0.09)))).id();
        commands.entity(root).add_child(post);
    }
    for height in [0.55, 0.95] {
        let wire = commands.spawn(part(&kit.cube, &kit.dark, Transform::from_xyz(0.0, height, 0.0).with_scale(Vec3::new(1.0, 0.02, 0.02)))).id();
        commands.entity(root).add_child(wire);
    }
    // Leafy clumps along the trellis.
    for (i, x) in [-0.32f32, -0.1, 0.12, 0.34].into_iter().enumerate() {
        let shade = &kit.leaves[i % 3];
        let tall = 0.5 + 0.1 * (i % 2) as f32;
        let clump = commands.spawn(part(&kit.rock, shade, Transform::from_xyz(x, 0.62 + 0.05 * tall, 0.0).with_scale(Vec3::new(0.32, tall, 0.42)))).id();
        commands.entity(root).add_child(clump);
    }
    // Grape bunches hanging below the leaves, on both sides, one holder each.
    for (i, name) in VINE_BUNCHES.iter().enumerate().take(bunches.min(4)) {
        let x = -0.3 + 0.2 * i as f32;
        let z = if i % 2 == 0 { 0.17 } else { -0.17 };
        let holder = commands.spawn((pivot(Transform::IDENTITY), Name::new(*name))).id();
        let bunch = commands.spawn(part(&kit.berry, &kit.grape, Transform::from_xyz(x, 0.4, z).with_scale(Vec3::new(1.1, 1.5, 1.1)))).id();
        commands.entity(holder).add_child(bunch);
        commands.entity(root).add_child(holder);
    }
    root
}

/// A walk-in house. Returns `(walls, roof)`: the walls (with the doorway frame and floor) and
/// the roof are separate roots so the game can lower the walls and hide the roof while the
/// player is inside. The shape comes from `HouseSpawn::shell_boxes` (thin walls down the middle
/// of the ring tiles, an open one-tile doorway, one floor), which is tested so that no two
/// boxes share a face: that is what stops the walls z-fighting once the roof is off.
///
/// Every box of the shell is an instance of one of three unit-box models (`house_wall`,
/// `house_timber`, `house_plank`) stretched to the box, and the roof is an instance of
/// `house_roof` stretched over the house, so a house of any size is made of the same four files.
pub fn spawn_house(commands: &mut Commands, kit: &Kit, map: &ozbound::world::WorldMap, house: &ozbound::homestead::HouseSpawn, roof_variant: usize) -> (Entity, Entity) {
    use ozbound::homestead::{ShellPart, WALL_HEIGHT};
    if !HOUSE_MODELS.iter().all(|id| kit.instances.has(id)) {
        return build_house(commands, kit, map, house, roof_variant);
    }
    let center = house_center(map, house);
    let walls = commands.spawn(pivot(Transform::from_translation(center))).id();
    for shell in house.shell_boxes() {
        let id = match shell.part {
            ShellPart::Wall | ShellPart::Lintel => "house_wall",
            ShellPart::Cap | ShellPart::Post => "house_timber",
            ShellPart::Floor | ShellPart::Threshold => "house_plank",
        };
        let transform = Transform::from_translation(Vec3::from_array(shell.center)).with_scale(Vec3::from_array(shell.size));
        if let Some(piece) = kit.instances.spawn(commands, id, transform, &[]) {
            commands.entity(walls).add_child(piece.root);
        }
    }
    // The roof model is one tile wide and one tile tall; stretched to the roof's footprint and
    // to the rise this house gets, it is the hip roof that used to be built in code. It starts
    // just above the timber cap on the wall tops.
    let (roof_x, roof_z) = house.roof_size();
    let scale = Vec3::new(roof_x, house_rise(house), roof_z);
    let transform = Transform::from_translation(center + Vec3::Y * (WALL_HEIGHT + 0.05)).with_scale(scale);
    let roof = kit.instances.spawn(commands, "house_roof", transform, &[(obj::BODY, &kit.roofs[roof_variant % kit.roofs.len()])]).map_or_else(|| commands.spawn(pivot(transform)).id(), |placed| placed.root);
    (walls, roof)
}

/// The library models a house is made of.
const HOUSE_MODELS: [&str; 4] = ["house_wall", "house_timber", "house_plank", "house_roof"];

/// The middle of a house's footprint, at ground level.
fn house_center(map: &ozbound::world::WorldMap, house: &ozbound::homestead::HouseSpawn) -> Vec3 {
    use ozbound::world::TilePos;
    let corner_a = map.tile_center_3d(house.origin, 0.0);
    let corner_b = map.tile_center_3d(TilePos::new(house.origin.x + house.width - 1, house.origin.y + house.height - 1), 0.0);
    Vec3::new((corner_a[0] + corner_b[0]) / 2.0, 0.0, (corner_a[2] + corner_b[2]) / 2.0)
}

/// How tall the roof of a bigger house stands.
fn house_rise(house: &ozbound::homestead::HouseSpawn) -> f32 {
    1.0 + 0.1 * house.height.min(house.width) as f32
}

/// A unit box (one tile on a side, centred) in `material`: the model a house's walls, timbers and
/// floors are stretched from.
fn build_unit_box(commands: &mut Commands, kit: &Kit, at: Vec3, material: &Handle<StandardMaterial>) -> Entity {
    let root = commands.spawn(pivot(Transform::from_translation(at))).id();
    let piece = commands.spawn(part(&kit.cube, material, Transform::IDENTITY)).id();
    commands.entity(root).add_child(piece);
    root
}

/// The roof of a house at rise one: a four-sided cone one tile across at the base and one tile
/// tall, turned to a hip roof, standing on the origin.
fn build_house_roof(commands: &mut Commands, kit: &Kit, at: Vec3) -> Entity {
    let root = commands.spawn(pivot(Transform::from_translation(at))).id();
    let cone = commands.spawn(part(&kit.roof_cone, &kit.roofs[0], Transform::from_xyz(0.0, 0.5, 0.0).with_rotation(Quat::from_rotation_y(std::f32::consts::FRAC_PI_4)))).id();
    commands.entity(root).add_child(cone);
    root
}

/// The code-built house, from primitives (the fallback when its models are not loaded).
fn build_house(commands: &mut Commands, kit: &Kit, map: &ozbound::world::WorldMap, house: &ozbound::homestead::HouseSpawn, roof_variant: usize) -> (Entity, Entity) {
    use ozbound::homestead::{ShellPart, WALL_HEIGHT};
    let center = house_center(map, house);

    let walls = commands.spawn(pivot(Transform::from_translation(center))).id();
    for shell in house.shell_boxes() {
        let material = match shell.part {
            ShellPart::Wall | ShellPart::Lintel => &kit.wall,
            ShellPart::Cap | ShellPart::Post => &kit.bark,
            ShellPart::Floor | ShellPart::Threshold => &kit.plank,
        };
        let piece = commands.spawn(part(&kit.cube, material, Transform::from_translation(Vec3::from_array(shell.center)).with_scale(Vec3::from_array(shell.size)))).id();
        commands.entity(walls).add_child(piece);
    }

    // A hip roof over the walls with an overhang, in its own root so it can be hidden. It starts
    // just above the timber cap on the wall tops.
    let (roof_x, roof_z) = house.roof_size();
    let roof = commands.spawn(pivot(Transform::from_translation(center + Vec3::Y * (WALL_HEIGHT + 0.05)).with_scale(Vec3::new(roof_x, 1.0, roof_z)))).id();
    // The cone's base sits at the bottom of its own height, so lift it by half the rise.
    let rise = house_rise(house);
    let cone = commands
        .spawn(part(&kit.roof_cone, &kit.roofs[roof_variant % kit.roofs.len()], Transform::from_xyz(0.0, 0.5 * rise, 0.0).with_rotation(Quat::from_rotation_y(std::f32::consts::FRAC_PI_4)).with_scale(Vec3::new(1.0, rise, 1.0))))
        .id();
    commands.entity(roof).add_child(cone);
    (walls, roof)
}

// ---------------------------------------------------------------------------------------------
// Props spawned from library models
// ---------------------------------------------------------------------------------------------

/// One mesh of a loaded model and the piece of the model it belongs to.
struct InstancePart {
    /// `body`, or the name of a piece that is shown and hidden (`crown`).
    sub_model: String,
    mesh: Handle<Mesh>,
    material: Handle<StandardMaterial>,
}

/// Every library model that loaded, as meshes and materials ready to be placed again and again.
#[derive(Default)]
pub struct Instances {
    models: HashMap<&'static str, Vec<InstancePart>>,
}

/// What placing one instance made.
pub struct Placed {
    pub root: Entity,
    /// The entity that holds each hideable piece of the model, by name.
    toggles: HashMap<String, Entity>,
}

impl Placed {
    /// The holder of the piece called `name`. Every model that can be placed has all the pieces
    /// its spec asks for, so a caller that names one of them always gets it.
    pub fn toggle(&self, name: &str) -> Entity {
        self.toggles.get(name).copied().expect("a placed model has every piece its spec lists")
    }
}

/// A flat-coloured mesh from the triangles of one part of a model file.
fn mesh_of(part: &obj::ParsedPart) -> Mesh {
    Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default())
        .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, part.positions.clone())
        .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, part.normals.clone())
        .with_inserted_indices(Indices::U32(part.indices.clone()))
}

fn material_of(part: &obj::ParsedPart) -> StandardMaterial {
    let [r, g, b, a] = part.color;
    let [er, eg, eb] = part.emissive;
    let glows = part.emissive.iter().any(|e| *e > 0.0);
    StandardMaterial {
        base_color: Color::srgba(r, g, b, a),
        emissive: LinearRgba::rgb(er, eg, eb),
        perceptual_roughness: if glows { 0.6 } else { 1.0 },
        reflectance: if glows { 0.5 } else { 0.05 },
        alpha_mode: if a < 1.0 { AlphaMode::Blend } else { AlphaMode::Opaque },
        ..default()
    }
}

impl Instances {
    /// Turns loaded models into meshes and materials. Equal colours share one material.
    pub fn new(models: &[(&'static str, ParsedModel)], meshes: &mut Assets<Mesh>, materials: &mut Assets<StandardMaterial>) -> Self {
        let mut out = HashMap::new();
        for (id, model) in models {
            let mut shared: HashMap<[u32; 7], Handle<StandardMaterial>> = HashMap::new();
            let parts = model
                .parts
                .iter()
                .map(|part| {
                    let [r, g, b, a] = part.color;
                    let key = [r.to_bits(), g.to_bits(), b.to_bits(), a.to_bits(), part.emissive[0].to_bits(), part.emissive[1].to_bits(), part.emissive[2].to_bits()];
                    let material = shared.entry(key).or_insert_with(|| materials.add(material_of(part))).clone();
                    InstancePart { sub_model: part.sub_model().to_string(), mesh: meshes.add(mesh_of(part)), material }
                })
                .collect();
            out.insert(*id, parts);
        }
        Self { models: out }
    }

    pub fn has(&self, id: &str) -> bool {
        self.models.contains_key(id)
    }

    /// Places an instance of the model `id` with `transform`, or `None` when that model is not
    /// loaded. Parts of a piece named in `recolor` use the given material instead of the file's
    /// (a tree's crown takes its leaf colour from the tile); pieces the spec hides get a holder
    /// entity each, named after the piece.
    pub fn spawn(&self, commands: &mut Commands, id: &str, transform: Transform, recolor: &[(&str, &Handle<StandardMaterial>)]) -> Option<Placed> {
        let parts = self.models.get(id)?;
        let toggles = model_library::spec(id)?.toggles;
        let root = commands.spawn(pivot(transform)).id();
        let mut holders: HashMap<String, Entity> = HashMap::new();
        for name in toggles {
            let holder = commands.spawn((pivot(Transform::IDENTITY), Name::new(*name))).id();
            commands.entity(root).add_child(holder);
            holders.insert((*name).to_string(), holder);
        }
        for p in parts {
            let material = recolor.iter().find(|(name, _)| *name == p.sub_model).map_or(&p.material, |(_, m)| *m);
            let entity = commands.spawn(part(&p.mesh, material, Transform::IDENTITY)).id();
            commands.entity(holders.get(&p.sub_model).copied().unwrap_or(root)).add_child(entity);
        }
        Some(Placed { root, toggles: holders })
    }
}

/// A tree. Returns `(root, crown)`; hiding `crown` leaves a stump.
pub fn spawn_tree(commands: &mut Commands, kit: &Kit, at: Vec3, yaw: f32, scale: f32, leaf: usize) -> (Entity, Entity) {
    let transform = Transform::from_translation(at).with_rotation(Quat::from_rotation_y(yaw)).with_scale(Vec3::splat(scale));
    match kit.instances.spawn(commands, "tree", transform, &[("crown", &kit.leaves[leaf % kit.leaves.len()])]) {
        Some(placed) => (placed.root, placed.toggle("crown")),
        None => build_tree(commands, kit, at, yaw, scale, leaf),
    }
}

/// A patch of meadowmint. Returns `(root, plants)`; hiding `plants` leaves the soil.
pub fn spawn_herb_patch(commands: &mut Commands, kit: &Kit, at: Vec3, yaw: f32) -> (Entity, Entity) {
    let transform = Transform::from_translation(at).with_rotation(Quat::from_rotation_y(yaw));
    match kit.instances.spawn(commands, "herb_patch", transform, &[]) {
        Some(placed) => (placed.root, placed.toggle("plants")),
        None => build_herb_patch(commands, kit, at, yaw),
    }
}

/// A honeyberry bush. Returns `(root, berries)`; hiding `berries` leaves the bare bush.
pub fn spawn_berry_bush(commands: &mut Commands, kit: &Kit, at: Vec3, yaw: f32) -> (Entity, Entity) {
    let transform = Transform::from_translation(at).with_rotation(Quat::from_rotation_y(yaw));
    match kit.instances.spawn(commands, "berry_bush", transform, &[]) {
        Some(placed) => (placed.root, placed.toggle("berries")),
        None => build_berry_bush(commands, kit, at, yaw),
    }
}

/// A boulder; with `copper`, orange flecks stud it. Returns `(root, flecks)`, the flecks entity
/// only when `copper` is true. A plain boulder is the same model without its flecks, in the
/// stone shade the tile picks.
pub fn spawn_rock(commands: &mut Commands, kit: &Kit, at: Vec3, yaw: f32, scale: f32, shade: usize, copper: bool) -> (Entity, Option<Entity>) {
    let transform = Transform::from_translation(at).with_rotation(Quat::from_rotation_y(yaw)).with_scale(Vec3::splat(scale));
    let stone = &kit.stone[shade % kit.stone.len()];
    match kit.instances.spawn(commands, "copper_vein", transform, &[(obj::BODY, stone)]) {
        Some(placed) => {
            let flecks = placed.toggle("flecks");
            if copper {
                (placed.root, Some(flecks))
            } else {
                commands.entity(flecks).despawn_recursive();
                (placed.root, None)
            }
        }
        None => build_rock(commands, kit, at, yaw, scale, shade, copper),
    }
}

/// A glowing moonpetal flower.
pub fn spawn_moonpetal(commands: &mut Commands, kit: &Kit, at: Vec3) -> Entity {
    match kit.instances.spawn(commands, "moonpetal", Transform::from_translation(at), &[]) {
        Some(placed) => placed.root,
        None => build_moonpetal(commands, kit, at),
    }
}

/// A signpost: a wooden post with a painted board.
pub fn spawn_signpost(commands: &mut Commands, kit: &Kit, at: Vec3, yaw: f32) -> Entity {
    match kit.instances.spawn(commands, "signpost", Transform::from_translation(at).with_rotation(Quat::from_rotation_y(yaw)), &[]) {
        Some(placed) => placed.root,
        None => build_signpost(commands, kit, at, yaw),
    }
}

/// A standing stone with a faintly glowing cap: the marker for a named place.
pub fn spawn_waymark(commands: &mut Commands, kit: &Kit, at: Vec3, yaw: f32) -> Entity {
    match kit.instances.spawn(commands, "waymark", Transform::from_translation(at).with_rotation(Quat::from_rotation_y(yaw)), &[]) {
        Some(placed) => placed.root,
        None => build_waymark(commands, kit, at, yaw),
    }
}

/// One tile of fence, built from the two modular pieces: a post in the middle and a rail toward
/// each linked neighbour. `rails` are unit map offsets `(dx, dy)` from
/// [`ozbound::scene::FenceLinks::rails`]; map `+y` is world `-z`. A rail piece reaches half a tile
/// toward `+x`, so it is turned to face the neighbour and the rails of two neighbouring tiles
/// meet at the tile edge.
pub fn spawn_fence(commands: &mut Commands, kit: &Kit, at: Vec3, rails: &[(i32, i32)]) -> Entity {
    if !(kit.instances.has("fence_post") && kit.instances.has("fence_rail")) {
        return build_fence(commands, kit, at, rails);
    }
    let root = commands.spawn(pivot(Transform::from_translation(at))).id();
    if let Some(post) = kit.instances.spawn(commands, "fence_post", Transform::IDENTITY, &[]) {
        commands.entity(root).add_child(post.root);
    }
    for &(dx, dy) in rails {
        // +x turned about +y by `yaw` points along (cos, -sin) in the (x, z) plane.
        let yaw = match (dx.signum(), dy.signum()) {
            (1, _) => 0.0,
            (-1, _) => std::f32::consts::PI,
            (_, 1) => std::f32::consts::FRAC_PI_2,
            _ => -std::f32::consts::FRAC_PI_2,
        };
        if let Some(rail) = kit.instances.spawn(commands, "fence_rail", Transform::from_rotation(Quat::from_rotation_y(yaw)), &[]) {
            commands.entity(root).add_child(rail.root);
        }
    }
    root
}

/// The post of a fence tile, at the tile's middle.
fn build_fence_post(commands: &mut Commands, kit: &Kit, at: Vec3) -> Entity {
    let root = commands.spawn(pivot(Transform::from_translation(at))).id();
    let post = commands.spawn(part(&kit.cube, &kit.bark, Transform::from_xyz(0.0, 0.42, 0.0).with_scale(Vec3::new(0.14, 0.84, 0.14)))).id();
    commands.entity(root).add_child(post);
    root
}

/// The rail of a fence tile toward `+x`: two bars half a tile long.
fn build_fence_rail(commands: &mut Commands, kit: &Kit, at: Vec3) -> Entity {
    let root = commands.spawn(pivot(Transform::from_translation(at))).id();
    for height in [0.34, 0.64] {
        let bar = commands.spawn(part(&kit.cube, &kit.plank, Transform::from_xyz(0.25, height, 0.0).with_scale(Vec3::new(0.5, 0.07, 0.05)))).id();
        commands.entity(root).add_child(bar);
    }
    root
}

// ---- reading the triangles of a model that is standing in the world ----

/// One Bevy mesh as an export part (triangle lists only; anything else is skipped). `label` is the
/// name of the piece it belongs to, empty for the body.
pub fn mesh_part(mesh: &Mesh, transform: Mat4, material: Option<&StandardMaterial>, label: &str) -> Option<obj::Part> {
    use bevy::render::mesh::VertexAttributeValues;
    if mesh.primitive_topology() != PrimitiveTopology::TriangleList {
        return None;
    }
    let float3 = |attribute| match mesh.attribute(attribute) {
        Some(VertexAttributeValues::Float32x3(values)) => Some(values.clone()),
        _ => None,
    };
    let positions = float3(Mesh::ATTRIBUTE_POSITION)?;
    let normals = float3(Mesh::ATTRIBUTE_NORMAL).filter(|n| n.len() == positions.len()).unwrap_or_default();
    let indices: Vec<u32> = match mesh.indices() {
        Some(Indices::U32(list)) => list.clone(),
        Some(Indices::U16(list)) => list.iter().map(|i| u32::from(*i)).collect(),
        None => (0..positions.len() as u32).collect(),
    };
    let (color, emissive) = material.map_or(([0.8, 0.8, 0.8, 1.0], [0.0; 3]), |m| {
        let c = m.base_color.to_srgba();
        ([c.red, c.green, c.blue, c.alpha], [m.emissive.red, m.emissive.green, m.emissive.blue])
    });
    Some(obj::Part { positions, normals, indices, transform: transform.to_cols_array(), color, emissive, label: label.to_string() })
}

/// The meshes under `root` as export parts, in the model's own frame (`root`'s own transform is
/// the frame, so it is not applied), in drawing order. A named entity labels itself and
/// everything below it until a nearer name replaces it.
fn parts_under(world: &World, root: Entity) -> Vec<obj::Part> {
    let (meshes, materials) = (world.resource::<Assets<Mesh>>(), world.resource::<Assets<StandardMaterial>>());
    let mut out = Vec::new();
    let mut stack = vec![(root, Mat4::IDENTITY, String::new())];
    while let Some((entity, parent, label)) = stack.pop() {
        let matrix = if entity == root { Mat4::IDENTITY } else { parent * world.get::<Transform>(entity).map_or(Mat4::IDENTITY, |t| t.compute_matrix()) };
        let label = world.get::<Name>(entity).map_or(label, |name| name.as_str().to_string());
        if let Some(mesh) = world.get::<Mesh3d>(entity) {
            let material = world.get::<MeshMaterial3d<StandardMaterial>>(entity).and_then(|m| materials.get(&m.0));
            if let Some(part) = meshes.get(&mesh.0).and_then(|m| mesh_part(m, matrix, material, &label)) {
                out.push(part);
            }
        }
        if let Some(children) = world.get::<Children>(entity) {
            // Reversed so the first child is read first.
            stack.extend(children.iter().rev().map(|child| (*child, matrix, label.clone())));
        }
    }
    out
}

/// Builds the code-made model of `id` once at the origin (unturned, scale one) and reads its
/// triangles back, then removes it. `None` for an id with no code-made model.
fn built_parts(world: &mut World, id: &str) -> Option<Vec<obj::Part>> {
    let root = world.resource_scope::<Kit, _>(|world, kit| {
        let root = {
            let mut commands = world.commands();
            let at = Vec3::ZERO;
            match id {
                "tree" => build_tree(&mut commands, &kit, at, 0.0, 1.0, 0).0,
                "copper_vein" => build_rock(&mut commands, &kit, at, 0.0, 1.0, 0, true).0,
                "herb_patch" => build_herb_patch(&mut commands, &kit, at, 0.0).0,
                "berry_bush" => build_berry_bush(&mut commands, &kit, at, 0.0).0,
                "moonpetal" => build_moonpetal(&mut commands, &kit, at),
                "signpost" => build_signpost(&mut commands, &kit, at, 0.0),
                "waymark" => build_waymark(&mut commands, &kit, at, 0.0),
                "fence_post" => build_fence_post(&mut commands, &kit, at),
                "fence_rail" => build_fence_rail(&mut commands, &kit, at),
                "thicket" => build_thicket(&mut commands, &kit, at, 0.0, 1.0, 0),
                "bridge_deck" => build_bridge_deck(&mut commands, &kit, at),
                "bridge_end" => build_bridge_end(&mut commands, &kit, at),
                "garden_bed" => build_garden_frame(&mut commands, &kit, at),
                "crop_cabbage" => build_crop(&mut commands, &kit, at, 0),
                "crop_tomato" => build_crop(&mut commands, &kit, at, 1),
                "crop_pumpkin" => build_crop(&mut commands, &kit, at, 2),
                "vine_row" => build_vine_row(&mut commands, &kit, at, true, 4),
                "house_wall" => build_unit_box(&mut commands, &kit, at, &kit.wall),
                "house_timber" => build_unit_box(&mut commands, &kit, at, &kit.bark),
                "house_plank" => build_unit_box(&mut commands, &kit, at, &kit.plank),
                "house_roof" => build_house_roof(&mut commands, &kit, at),
                _ => return None,
            }
        };
        world.flush();
        Some(root)
    })?;
    let parts = parts_under(world, root);
    world.commands().entity(root).despawn_recursive();
    world.flush();
    Some(parts)
}

/// Where the library lives: `OZBOUND_LIBRARY` or `assets/library` (the same place the editor
/// reads and writes).
fn library_dir() -> PathBuf {
    std::env::var_os("OZBOUND_LIBRARY").map_or_else(|| PathBuf::from(ozbound::asset_file::LIBRARY_DIR), PathBuf::from)
}

/// Startup, after the kit exists and before the world is built: loads every library model,
/// generates the ones that have no file from their code-made models, and makes them the models
/// the props are spawned from. Nothing here stops the game: a model that cannot be used is
/// reported on the console and its prop keeps the code-made model for this run.
pub fn load_models(world: &mut World) {
    let dir = library_dir();
    let scan = model_library::scan(&dir);
    if scan.directory_missing() {
        eprintln!("note: {} not found; props use their built-in models", dir.display());
        return;
    }
    for (id, why) in &scan.problems {
        eprintln!("model {id}: {why}; using the built-in model (the file is left as it is)");
    }
    let mut models = scan.found;
    for id in scan.missing {
        let Some(parts) = built_parts(world, id) else {
            eprintln!("model {id}: no built-in model to generate it from");
            continue;
        };
        match model_library::write_generated(&dir, id, &parts) {
            Ok((path, model)) => {
                eprintln!("model {id}: no file, so generated {} ({} triangles)", path.display(), model.triangles());
                models.push((id, model));
            }
            Err(error) => eprintln!("model {id}: could not generate it: {error}; using the built-in model"),
        }
    }
    let instances = world.resource_scope::<Assets<Mesh>, _>(|world, mut meshes| {
        world.resource_scope::<Assets<StandardMaterial>, _>(|_, mut materials| Instances::new(&models, &mut meshes, &mut materials))
    });
    world.resource_mut::<Kit>().instances = instances;
}

#[cfg(test)]
mod tests {
    use super::*;

    fn world_with_kit() -> World {
        let mut world = World::new();
        let mut meshes = Assets::<Mesh>::default();
        let mut materials = Assets::<StandardMaterial>::default();
        let kit = Kit::new(&mut meshes, &mut materials);
        world.insert_resource(meshes);
        world.insert_resource(materials);
        world.insert_resource(kit);
        world
    }

    #[test]
    fn every_library_model_can_be_generated_from_its_code_built_version_and_then_loads() {
        let dir = std::env::temp_dir().join(format!("ozbound-gen-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let mut world = world_with_kit();
        let mut models = Vec::new();
        for spec in &model_library::MODELS {
            let parts = built_parts(&mut world, spec.id).unwrap_or_else(|| panic!("{} has a built-in model", spec.id));
            let (_, model) = model_library::write_generated(&dir, spec.id, &parts).unwrap_or_else(|e| panic!("{}: {e}", spec.id));
            assert!(model.triangles() > 0, "{}", spec.id);
            models.push((spec.id, model));
        }
        // A second scan finds every file and nothing to generate.
        let scan = model_library::scan(&dir);
        assert!(scan.missing.is_empty() && scan.problems.is_empty(), "{:?}", scan.problems);
        assert_eq!(scan.found.len(), model_library::MODELS.len());
        // The scratch models were removed from the world again.
        assert_eq!(world.query::<&Transform>().iter(&world).count(), 0);
        // And they turn into instances that can be placed.
        let (mut meshes, mut materials) = (Assets::<Mesh>::default(), Assets::<StandardMaterial>::default());
        let instances = Instances::new(&models, &mut meshes, &mut materials);
        assert!(model_library::MODELS.iter().all(|m| instances.has(m.id)));
        let mut commands_world = World::new();
        let placed = {
            let mut commands = commands_world.commands();
            instances.spawn(&mut commands, "tree", Transform::IDENTITY, &[])
        };
        commands_world.flush();
        let placed = placed.expect("the tree is loaded");
        assert!(commands_world.get::<Children>(placed.toggle("crown")).is_some_and(|c| c.len() == 3), "three cones under the crown");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Writes any model file missing from `assets/library` (never overwriting one). Run it with
    /// `cargo test --bin ozbound -- --ignored write_missing_library_models` after adding a model.
    #[test]
    #[ignore]
    fn write_missing_library_models() {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(ozbound::asset_file::LIBRARY_DIR);
        let mut world = world_with_kit();
        for id in model_library::scan(&dir).missing {
            let parts = built_parts(&mut world, id).unwrap();
            let (path, _) = model_library::write_generated(&dir, id, &parts).unwrap();
            println!("wrote {}", path.display());
        }
    }

    // ---- instanced props are the same shape as the code-built ones ----

    /// A world whose kit places every library model as an instance of its generated file.
    fn world_with_models(name: &str) -> World {
        let dir = std::env::temp_dir().join(format!("ozbound-inst-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let mut world = world_with_kit();
        let mut models = Vec::new();
        for spec in &model_library::MODELS {
            let parts = built_parts(&mut world, spec.id).unwrap_or_else(|| panic!("{} has a built-in model", spec.id));
            let (_, model) = model_library::write_generated(&dir, spec.id, &parts).unwrap_or_else(|e| panic!("{}: {e}", spec.id));
            models.push((spec.id, model));
        }
        let instances = world.resource_scope::<Assets<Mesh>, _>(|world, mut meshes| {
            world.resource_scope::<Assets<StandardMaterial>, _>(|_, mut materials| Instances::new(&models, &mut meshes, &mut materials))
        });
        world.resource_mut::<Kit>().instances = instances;
        let _ = std::fs::remove_dir_all(&dir);
        world
    }

    /// What a model looks like, ignoring how it was put together: the triangles of each colour and
    /// the box that holds all of it (in thousandths).
    #[derive(Debug, PartialEq, Eq)]
    struct Shape {
        triangles_by_colour: std::collections::BTreeMap<[i32; 4], usize>,
        low: [i32; 3],
        high: [i32; 3],
    }

    fn shape_of(parts: &[obj::Part], frame: Mat4) -> Shape {
        let thousandths = |v: f32| (v * 1000.0).round() as i32;
        let (mut low, mut high) = ([f32::MAX; 3], [f32::MIN; 3]);
        let mut triangles_by_colour = std::collections::BTreeMap::new();
        for part in parts {
            let matrix = frame * Mat4::from_cols_array(&part.transform);
            for vertex in &part.positions {
                let at = matrix.transform_point3(Vec3::from_array(*vertex));
                for k in 0..3 {
                    low[k] = low[k].min(at[k]);
                    high[k] = high[k].max(at[k]);
                }
            }
            *triangles_by_colour.entry(part.color.map(thousandths)).or_insert(0) += part.indices.len() / 3;
        }
        Shape { triangles_by_colour, low: low.map(thousandths), high: high.map(thousandths) }
    }

    /// Spawns with `spawn`, reads the triangles under every root it returns, and removes them.
    fn shapes(world: &mut World, spawn: impl Fn(&mut Commands, &Kit) -> Vec<Entity>) -> Vec<Shape> {
        let roots = world.resource_scope::<Kit, _>(|world, kit| {
            let roots = spawn(&mut world.commands(), &kit);
            world.flush();
            roots
        });
        let out = roots.iter().map(|root| shape_of(&parts_under(world, *root), world.get::<Transform>(*root).map_or(Mat4::IDENTITY, |t| t.compute_matrix()))).collect();
        for root in roots {
            world.commands().entity(root).despawn_recursive();
        }
        world.flush();
        out
    }

    fn assert_same_shapes(what: &str, spawn: impl Fn(&mut Commands, &Kit) -> Vec<Entity>) {
        let built = shapes(&mut world_with_kit(), &spawn);
        let instanced = shapes(&mut world_with_models("same"), &spawn);
        assert_eq!(built, instanced, "{what}: the instanced model must look like the code-built one");
        assert!(built.iter().all(|s| !s.triangles_by_colour.is_empty()), "{what} is not empty");
    }

    #[test]
    fn a_thicket_looks_the_same_instanced_and_takes_its_two_shades_from_the_tile() {
        for shade in 0..3 {
            assert_same_shapes("thicket", |c, k| vec![spawn_thicket(c, k, Vec3::new(3.0, 0.5, 2.0), 0.7, 1.1, shade)]);
        }
        // The two shades really do swap: the first mound is drawn in a different leaf colour.
        let mut world = world_with_models("shades");
        let first_colour = |world: &mut World, shade| {
            let root = world.resource_scope::<Kit, _>(|world, kit| {
                let root = spawn_thicket(&mut world.commands(), &kit, Vec3::ZERO, 0.0, 1.0, shade);
                world.flush();
                root
            });
            let colour = parts_under(world, root)[0].color;
            world.commands().entity(root).despawn_recursive();
            world.flush();
            colour
        };
        assert_ne!(first_colour(&mut world, 0), first_colour(&mut world, 1));
        assert_eq!(first_colour(&mut world, 0), first_colour(&mut world, 2));
    }

    #[test]
    fn every_bridge_tile_looks_the_same_instanced_with_end_posts_only_where_wanted() {
        for along_x in [true, false] {
            for (first, last) in [(false, false), (true, false), (false, true), (true, true)] {
                assert_same_shapes("bridge tile", |c, k| vec![spawn_bridge_tile(c, k, Vec3::new(1.0, 0.0, 4.0), along_x, first, last)]);
            }
        }
        let mut world = world_with_models("bridge");
        let triangles = |world: &mut World, first, last| shapes(world, |c, k| vec![spawn_bridge_tile(c, k, Vec3::ZERO, true, first, last)]).remove(0).triangles_by_colour.values().sum::<usize>();
        let (none, one, both) = (triangles(&mut world, false, false), triangles(&mut world, true, false), triangles(&mut world, true, true));
        assert!(none < one && one < both, "{none} {one} {both}");
        assert_eq!(both - one, one - none, "each end adds the same posts");
    }

    #[test]
    fn a_garden_bed_looks_the_same_instanced_for_each_crop() {
        for crop in 0..4 {
            assert_same_shapes("garden bed", |c, k| vec![spawn_garden_bed(c, k, Vec3::new(2.0, 0.1, 2.0), 0.05, crop)]);
        }
    }

    #[test]
    fn a_vine_row_keeps_only_the_bunches_asked_for() {
        for bunches in 0..=6 {
            for along_x in [true, false] {
                assert_same_shapes("vine row", |c, k| vec![spawn_vine_row(c, k, Vec3::new(1.0, 0.0, 1.0), along_x, bunches)]);
            }
        }
        let mut world = world_with_models("vines");
        let grapes = |world: &mut World, bunches| {
            let grape = world.resource::<Kit>().grape.clone();
            let colour = world.resource::<Assets<StandardMaterial>>().get(&grape).map(|m| m.base_color.to_srgba()).unwrap();
            let key = [colour.red, colour.green, colour.blue, colour.alpha].map(|v| (v * 1000.0).round() as i32);
            shapes(world, |c, k| vec![spawn_vine_row(c, k, Vec3::ZERO, true, bunches)]).remove(0).triangles_by_colour.get(&key).copied().unwrap_or(0)
        };
        let per_bunch = grapes(&mut world, 1);
        assert!(per_bunch > 0);
        assert_eq!(grapes(&mut world, 0), 0);
        assert_eq!(grapes(&mut world, 3), 3 * per_bunch);
        assert_eq!(grapes(&mut world, 9), 4 * per_bunch, "never more than the four the model holds");
    }

    #[test]
    fn a_house_of_any_size_looks_the_same_instanced_and_its_roof_follows_its_variant() {
        use ozbound::homestead::{HouseSpawn, Side};
        use ozbound::world::{TilePos, WorldMap};
        let map = WorldMap::from_ascii(&format!("{}\n", ".".repeat(16)).repeat(16)).unwrap();
        for (w, h) in [(3, 3), (4, 4), (5, 4), (7, 9)] {
            for door in Side::ALL {
                let house = HouseSpawn { id: "h".into(), origin: TilePos::new(2, 3), width: w, height: h, door, name: "h".into(), ground: Vec::new() };
                for variant in 0..2 {
                    assert_same_shapes("house", |c, k| {
                        let (walls, roof) = spawn_house(c, k, &map, &house, variant);
                        vec![walls, roof]
                    });
                }
            }
        }
        // The two roof variants differ in colour only.
        let house = HouseSpawn { id: "h".into(), origin: TilePos::new(2, 3), width: 5, height: 5, door: Side::South, name: "h".into(), ground: Vec::new() };
        let mut world = world_with_models("roofs");
        let roofs: Vec<_> = (0..2).map(|variant| shapes(&mut world, |c, k| vec![spawn_house(c, k, &map, &house, variant).1]).remove(0)).collect();
        assert_ne!(roofs[0].triangles_by_colour, roofs[1].triangles_by_colour);
        assert_eq!((roofs[0].low, roofs[0].high), (roofs[1].low, roofs[1].high));
    }

    #[test]
    fn the_hide_and_lower_systems_still_find_the_house_roots() {
        // The house roof is hidden with `Visibility` and the walls are lowered with `scale.y`, so
        // both roots must carry those components even when they are instances.
        use ozbound::homestead::{HouseSpawn, Side};
        use ozbound::world::{TilePos, WorldMap};
        let map = WorldMap::from_ascii(&format!("{}\n", ".".repeat(12)).repeat(12)).unwrap();
        let house = HouseSpawn { id: "h".into(), origin: TilePos::new(2, 2), width: 4, height: 4, door: Side::South, name: "h".into(), ground: Vec::new() };
        let mut world = world_with_models("roots");
        let (walls, roof) = world.resource_scope::<Kit, _>(|world, kit| {
            let roots = spawn_house(&mut world.commands(), &kit, &map, &house, 0);
            world.flush();
            roots
        });
        assert!(world.get::<Visibility>(roof).is_some() && world.get::<Transform>(roof).is_some());
        assert!(world.get::<Transform>(walls).is_some_and(|t| t.scale == Vec3::ONE), "the walls start at full height");
        assert!(world.get::<Children>(walls).is_some_and(|c| c.len() == house.shell_boxes().len()), "one instance per box of the shell");
    }

    #[test]
    fn the_generated_pieces_match_the_shipped_files_in_shape() {
        let mut world = world_with_kit();
        for (id, groups) in [("tree", 4), ("herb_patch", 11), ("berry_bush", 7), ("copper_vein", 6), ("moonpetal", 8)] {
            let parts = built_parts(&mut world, id).unwrap();
            let shipped = ozbound::obj::load_obj(std::path::Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/assets/library")), id).unwrap();
            assert_eq!(parts.len(), groups, "{id}");
            assert_eq!(shipped.parts.len(), groups, "{id}");
            let generated: Vec<&str> = parts.iter().map(|p| if p.label.is_empty() { "body" } else { p.label.as_str() }).collect();
            let on_disk: Vec<&str> = shipped.parts.iter().map(|p| p.sub_model()).collect();
            assert_eq!(generated, on_disk, "{id}: same pieces in the same order");
        }
    }
}
