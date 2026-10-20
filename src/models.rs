//! Original low-poly models, assembled from a handful of flat-shaded primitives.
//! Everything is generated in code, so there are no external assets to license.
//!
//! Scale: one world unit is one tile. A character stands about 1.8 units tall.

use std::f32::consts::TAU;

use bevy::prelude::*;
use ozbound::appearance::{Appearance, Rgb};

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
pub struct Kit {
    pub slab: Handle<Mesh>,
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

    pub cube: Handle<Mesh>,
    pub hat_cone: Handle<Mesh>,
    pub brim: Handle<Mesh>,
    pub dark: Handle<StandardMaterial>,
    pub traveller: Look,
    pub guide: Look,
    /// One look per named appearance (`Appearance::KEYS`), for the villagers.
    pub npc_looks: Vec<(&'static str, Look)>,
}

impl Kit {
    pub fn new(meshes: &mut Assets<Mesh>, materials: &mut Assets<StandardMaterial>) -> Self {
        Self {
            slab: meshes.add(Cuboid::new(1.0, 0.5, 1.0)),
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
pub fn spawn_tree(commands: &mut Commands, kit: &Kit, at: Vec3, yaw: f32, scale: f32, leaf: usize) -> (Entity, Entity) {
    let root = commands.spawn(pivot(Transform::from_translation(at).with_rotation(Quat::from_rotation_y(yaw)).with_scale(Vec3::splat(scale)))).id();
    let trunk = commands.spawn(part(&kit.trunk, &kit.bark, Transform::from_xyz(0.0, 0.4, 0.0))).id();
    let crown = commands.spawn(pivot(Transform::IDENTITY)).id();
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
pub fn spawn_herb_patch(commands: &mut Commands, kit: &Kit, at: Vec3, yaw: f32) -> (Entity, Entity) {
    let root = commands.spawn(pivot(Transform::from_translation(at).with_rotation(Quat::from_rotation_y(yaw)))).id();
    let soil = commands.spawn(part(&kit.cube, &kit.path[1], Transform::from_xyz(0.0, 0.02, 0.0).with_scale(Vec3::new(0.7, 0.04, 0.7)))).id();
    let plants = commands.spawn(pivot(Transform::IDENTITY)).id();
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
pub fn spawn_berry_bush(commands: &mut Commands, kit: &Kit, at: Vec3, yaw: f32) -> (Entity, Entity) {
    let root = commands.spawn(pivot(Transform::from_translation(at).with_rotation(Quat::from_rotation_y(yaw)))).id();
    let mound = commands.spawn(part(&kit.rock, &kit.leaves[1], Transform::from_xyz(0.0, 0.28, 0.0).with_scale(Vec3::new(1.05, 0.62, 0.95)))).id();
    let berries = commands.spawn(pivot(Transform::IDENTITY)).id();
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
pub fn spawn_fence(commands: &mut Commands, kit: &Kit, at: Vec3, rails: &[(i32, i32)]) -> Entity {
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
pub fn spawn_signpost(commands: &mut Commands, kit: &Kit, at: Vec3, yaw: f32) -> Entity {
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
pub fn spawn_rock(commands: &mut Commands, kit: &Kit, at: Vec3, yaw: f32, scale: f32, shade: usize, copper: bool) -> (Entity, Option<Entity>) {
    let stone = &kit.stone[shade % kit.stone.len()];
    let root = commands.spawn(pivot(Transform::from_translation(at).with_rotation(Quat::from_rotation_y(yaw)).with_scale(Vec3::splat(scale)))).id();
    let big = commands.spawn(part(&kit.rock, stone, Transform::from_xyz(0.0, 0.28, 0.0).with_scale(Vec3::new(1.55, 1.05, 1.35)))).id();
    let small = commands.spawn(part(&kit.rock, stone, Transform::from_xyz(0.28, 0.16, -0.22).with_rotation(Quat::from_rotation_y(0.9)).with_scale(Vec3::new(0.8, 0.6, 0.7)))).id();
    commands.entity(root).add_children(&[big, small]);
    let flecks = copper.then(|| {
        let group = commands.spawn(pivot(Transform::IDENTITY)).id();
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
pub fn spawn_moonpetal(commands: &mut Commands, kit: &Kit, at: Vec3) -> Entity {
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
pub fn spawn_waymark(commands: &mut Commands, kit: &Kit, at: Vec3, yaw: f32) -> Entity {
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
