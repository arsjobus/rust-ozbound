//! Startup: builds the 3D meadow, its props and characters, lights, camera, and HUD.

use std::collections::HashMap;
use std::f32::consts::TAU;

use bevy::image::ImageSampler;
use bevy::pbr::{CascadeShadowConfigBuilder, NotShadowCaster};
use bevy::prelude::*;
use bevy::render::{
    render_asset::RenderAssetUsages,
    render_resource::{Extent3d, TextureDimension, TextureFormat},
};
use ozbound::{
    ambient,
    appearance::{portrait_rgba, Appearance, PORTRAIT_SIZE},
    gathering::{validate_node_placement, GatheringNode, ResourceKind},
    interaction::InteractionKind,
    level::{NodeSpawn, SignSpawn},
    scene,
    task::Task,
    world::{Tile, TilePos, WorldMap},
};

use crate::camera_rig::Rig;
use crate::components::*;
use crate::models::{self, Kit};

/// Slabs are 0.5 thick; `top` is where the visible surface sits.
const SLAB_THICKNESS: f32 = 0.5;

fn vec3(v: [f32; 3]) -> Vec3 {
    Vec3::from_array(v)
}

/// Pick box centered on a tile: `half` wide in x and z, `height` tall.
fn pick_box(map: &WorldMap, tile: TilePos, half: f32, height: f32) -> ([f32; 3], [f32; 3]) {
    let c = map.tile_center_3d(tile, 0.0);
    ([c[0] - half, 0.0, c[2] - half], [c[0] + half, height, c[2] + half])
}

pub fn setup(
    mut commands: Commands,
    meadow: Res<Meadow>,
    level: Res<LevelData>,
    wildlife: Res<Wildlife>,
    mut portraits: ResMut<Portraits>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut images: ResMut<Assets<Image>>,
    mut sounds: ResMut<Assets<AudioSource>>,
) {
    let map = &meadow.0;
    let level = &level.0;
    let kit = Kit::new(&mut meshes, &mut materials);

    // Click markers: a yellow cross for walking, a red one for interacting.
    let marker = |color: Color| StandardMaterial { base_color: color, unlit: true, ..default() };
    commands.insert_resource(ClickAssets {
        bar: meshes.add(Cuboid::new(0.55, 0.03, 0.09)),
        walk: materials.add(marker(Color::srgb(1.0, 0.92, 0.2))),
        interact: materials.add(marker(Color::srgb(0.95, 0.15, 0.12))),
    });

    // Ground: one slab per tile, tinted from a small palette so the meadow isn't one flat colour.
    // Water sits lower than the grass, so its banks read as little cliffs.
    for y in 0..map.height() as i32 {
        for x in 0..map.width() as i32 {
            let pos = TilePos::new(x, y);
            let tile = map.tile_at(pos).expect("loop stays inside the map");
            let (material, top) = match tile {
                Tile::Grass | Tile::Rock | Tile::Fence | Tile::Sign => (kit.grass[scene::tile_variant(pos, 1, 3)].clone(), 0.0),
                Tile::Tree => (kit.dark_grass.clone(), 0.0),
                Tile::Path => (kit.path[scene::tile_variant(pos, 2, 2)].clone(), -0.02),
                Tile::Water => (kit.water[scene::tile_variant(pos, 3, 2)].clone(), -0.2),
            };
            commands.spawn((
                Mesh3d(kit.slab.clone()),
                MeshMaterial3d(material),
                Transform::from_translation(vec3(map.tile_center_3d(pos, top - SLAB_THICKNESS / 2.0))),
                NotShadowCaster,
            ));
        }
    }

    // Validate resource nodes once; scenery below attaches gameplay to the valid ones.
    let mut nodes: HashMap<TilePos, NodeSpawn> = HashMap::new();
    for node in level.nodes.iter().copied() {
        match validate_node_placement(map, node.tile, node.kind) {
            Ok(()) => {
                nodes.insert(node.tile, node);
            }
            Err(error) => error!("skipping {} at {:?}: {error:?}", node.kind.label(), node.tile),
        }
    }

    let signs: HashMap<TilePos, &SignSpawn> = level.signs.iter().map(|sign| (sign.tile, sign)).collect();

    // Trees, rocks, fences, and signposts stand on every blocking tile. Only the trees and
    // rocks listed as resource nodes, and the signposts listed in `[signs]`, can be clicked;
    // the rest is scenery that shuts the player in. Collision itself comes from the tile
    // map (`Tile::is_walkable`), never from these models.
    for y in 0..map.height() as i32 {
        for x in 0..map.width() as i32 {
            let pos = TilePos::new(x, y);
            let tile = map.tile_at(pos).expect("loop stays inside the map");
            let at = vec3(map.tile_center_3d(pos, 0.0));
            let yaw = scene::tile_noise(pos, 4) * TAU;
            match tile {
                Tile::Tree => {
                    let scale = 0.9 + scene::tile_noise(pos, 5) * 0.3;
                    let leaf = scene::tile_variant(pos, 6, 3);
                    let (root, crown) = models::spawn_tree(&mut commands, &kit, at, yaw, scale, leaf);
                    if let Some(node) = nodes.get(&pos).filter(|n| n.kind == ResourceKind::Tree) {
                        let (pick_min, pick_max) = pick_box(map, pos, 0.45, 2.5 * scale);
                        commands.entity(root).insert((
                            ResourceNode(GatheringNode::new(node.kind, node.actions, node.yield_per_action)),
                            SpentVisual(crown),
                            Interactable { kind: InteractionKind::ChopDown, name: "tree", tile: pos, approach: InteractionKind::ChopDown.approach(), pick_min, pick_max },
                        ));
                    }
                }
                Tile::Rock => {
                    let scale = 0.95 + scene::tile_noise(pos, 7) * 0.2;
                    let shade = scene::tile_variant(pos, 8, 2);
                    let node = nodes.get(&pos).filter(|n| n.kind == ResourceKind::CopperVein);
                    let (root, flecks) = models::spawn_rock(&mut commands, &kit, at, yaw, scale, shade, node.is_some());
                    if let (Some(node), Some(flecks)) = (node, flecks) {
                        let (pick_min, pick_max) = pick_box(map, pos, 0.5, 1.0);
                        commands.entity(root).insert((
                            ResourceNode(GatheringNode::new(node.kind, node.actions, node.yield_per_action)),
                            SpentVisual(flecks),
                            Interactable { kind: InteractionKind::Mine, name: "copper vein", tile: pos, approach: InteractionKind::Mine.approach(), pick_min, pick_max },
                        ));
                    }
                }
                Tile::Fence => {
                    let rails = scene::fence_links(map, pos).rails();
                    models::spawn_fence(&mut commands, &kit, at, &rails);
                }
                Tile::Sign => {
                    let post = models::spawn_signpost(&mut commands, &kit, at, (scene::tile_noise(pos, 9) - 0.5) * 0.5);
                    if let Some(sign) = signs.get(&pos) {
                        let (pick_min, pick_max) = pick_box(map, pos, 0.4, 1.2);
                        commands.entity(post).insert((
                            SignText(sign.text.clone()),
                            Interactable { kind: InteractionKind::Read, name: "signpost", tile: pos, approach: InteractionKind::Read.approach(), pick_min, pick_max },
                        ));
                    }
                }
                _ => {}
            }
        }
    }

    // Three moonpetals along the path. They are picked up by clicking them.
    for (i, tile) in level.moonpetals.iter().copied().enumerate() {
        if let Err(error) = validate_node_placement(map, tile, ResourceKind::Moonpetal) {
            error!("skipping moonpetal at {tile:?}: {error:?}");
            continue;
        }
        let at = vec3(map.tile_center_3d(tile, 0.12));
        let (pick_min, pick_max) = pick_box(map, tile, 0.4, 0.7);
        let flower = models::spawn_moonpetal(&mut commands, &kit, at);
        commands.entity(flower).insert((
            Moonpetal,
            Floaty { base_y: at.y, phase: i as f32 * 2.1 },
            Interactable { kind: InteractionKind::Pick, name: "moonpetal", tile, approach: InteractionKind::Pick.approach(), pick_min, pick_max },
        ));
    }

    // Herb patches and berry bushes grow on open ground and are used by standing on them.
    // Only valid, listed nodes become clickable; the crop (not the soil or bush) hides when spent.
    for spawn in level.nodes.iter().filter(|n| matches!(n.kind, ResourceKind::HerbPatch | ResourceKind::BerryBush)) {
        let pos = spawn.tile;
        if !nodes.contains_key(&pos) {
            continue;
        }
        let at = vec3(map.tile_center_3d(pos, 0.0));
        let yaw = scene::tile_noise(pos, 10) * TAU;
        let (root, crop, height) = match spawn.kind {
            ResourceKind::HerbPatch => {
                let (root, plants) = models::spawn_herb_patch(&mut commands, &kit, at, yaw);
                (root, plants, 0.5)
            }
            _ => {
                let (root, berries) = models::spawn_berry_bush(&mut commands, &kit, at, yaw);
                (root, berries, 0.8)
            }
        };
        let (pick_min, pick_max) = pick_box(map, pos, 0.42, height);
        let kind = InteractionKind::Forage;
        commands.entity(root).insert((
            ResourceNode(GatheringNode::new(spawn.kind, spawn.actions, spawn.yield_per_action)),
            SpentVisual(crop),
            Interactable { kind, name: spawn.kind.label(), tile: pos, approach: kind.approach(), pick_min, pick_max },
        ));
    }

    // The village guide faces the player's starting tile.
    let guide_at = vec3(map.tile_center_3d(level.guide, 0.0));
    let start_at = vec3(map.tile_center_3d(level.player_start, 0.0));
    let toward_start = ozbound::task::facing_yaw(start_at.x - guide_at.x, start_at.z - guide_at.z).unwrap_or(0.0);
    let guide = models::spawn_humanoid(&mut commands, &kit, &kit.guide, guide_at, toward_start);
    let (pick_min, pick_max) = pick_box(map, level.guide, 0.4, 1.95);
    commands.entity(guide).insert((
        Guide,
        Npc(ozbound::level::GUIDE_ID.to_string()),
        Interactable { kind: InteractionKind::TalkTo, name: "Village Guide", tile: level.guide, approach: InteractionKind::TalkTo.approach(), pick_min, pick_max },
    ));

    // Portraits for every person who can be talked to (the guide uses the guide's look).
    let portrait_for = |key: &str, images: &mut Assets<Image>| {
        let look = Appearance::from_key(key).or_else(|| Appearance::from_key("traveller")).expect("traveller exists");
        let mut image = Image::new(
            Extent3d { width: PORTRAIT_SIZE as u32, height: PORTRAIT_SIZE as u32, depth_or_array_layers: 1 },
            TextureDimension::D2,
            portrait_rgba(&look),
            TextureFormat::Rgba8UnormSrgb,
            RenderAssetUsages::RENDER_WORLD,
        );
        image.sampler = ImageSampler::nearest();
        images.add(image)
    };
    portraits.0.insert(ozbound::level::GUIDE_ID.to_string(), portrait_for("guide", &mut images));

    // The other villagers, each facing the player's starting tile like the guide does.
    // Interactable names are `&'static str`; the few names in the level file are leaked once
    // at startup, which is simpler than threading a lifetime through every system.
    for npc in &level.npcs {
        let at = vec3(map.tile_center_3d(npc.tile, 0.0));
        let toward = ozbound::task::facing_yaw(start_at.x - at.x, start_at.z - at.z).unwrap_or(0.0);
        let body = models::spawn_humanoid(&mut commands, &kit, kit.look_for(npc.look.key().unwrap_or("traveller")), at, toward);
        let (pick_min, pick_max) = pick_box(map, npc.tile, 0.4, 1.95);
        let name: &'static str = Box::leak(npc.name.clone().into_boxed_str());
        commands.entity(body).insert((
            Npc(npc.id.clone()),
            Interactable { kind: InteractionKind::TalkTo, name, tile: npc.tile, approach: InteractionKind::TalkTo.approach(), pick_min, pick_max },
        ));
        portraits.0.insert(npc.id.clone(), portrait_for(npc.look.key().unwrap_or("traveller"), &mut images));
    }

    // Named places are marked by a standing stone, so the player can see where to walk to.
    for place in &level.places {
        let at = vec3(map.tile_center_3d(place.tile, 0.0));
        models::spawn_waymark(&mut commands, &kit, at, scene::tile_noise(place.tile, 11) * TAU);
    }

    // Creatures stand on open ground and are approached from beside, never clicked through.
    for (i, spawn) in level.creatures.iter().enumerate() {
        let Some(creature) = wildlife.bestiary.get(&spawn.id) else { continue };
        let moth = spawn.id == "lantern_moth";
        let at = vec3(map.tile_center_3d(spawn.tile, if moth { 0.8 } else { 0.0 }));
        let toward = ozbound::task::facing_yaw(start_at.x - at.x, start_at.z - at.z).unwrap_or(0.0);
        let body = models::spawn_creature(&mut commands, &kit, &spawn.id, at, toward);
        let (pick_min, pick_max) = pick_box(map, spawn.tile, 0.45, if moth { 1.3 } else { 1.6 });
        let name: &'static str = Box::leak(creature.name.clone().into_boxed_str());
        commands.entity(body).insert((
            Creature(spawn.id.clone()),
            Interactable { kind: InteractionKind::Approach, name, tile: spawn.tile, approach: InteractionKind::Approach.approach(), pick_min, pick_max },
        ));
        if moth {
            commands.entity(body).insert(Floaty { base_y: at.y, phase: i as f32 * 1.7 });
        }
    }

    // Ambient sound: wind and distant birds, generated in code and looped quietly.
    let wav = ambient::meadow_loop_wav(24);
    let ambience = sounds.add(AudioSource { bytes: wav.into() });
    commands.spawn((AudioPlayer::new(ambience), PlaybackSettings::LOOP.with_volume(bevy::audio::Volume::new(0.35))));

    // The player.
    let player = models::spawn_humanoid(&mut commands, &kit, &kit.traveller, start_at, 0.0);
    commands.entity(player).insert(Player { tile: level.player_start, task: Task::idle(), work: None });

    // A wide dark floor under the meadow so the map edge doesn't float in the sky colour.
    commands.spawn((
        Mesh3d(meshes.add(Plane3d::default().mesh().size(240.0, 240.0))),
        MeshMaterial3d(materials.add(StandardMaterial { base_color: Color::srgb(0.16, 0.30, 0.20), perceptual_roughness: 1.0, ..default() })),
        Transform::from_xyz(0.0, -0.55, 0.0),
        NotShadowCaster,
    ));

    // Light: a warm sun with shadows plus soft sky fill.
    commands.insert_resource(AmbientLight { color: Color::srgb(0.80, 0.88, 1.0), brightness: 260.0 });
    commands.spawn((
        DirectionalLight { color: Color::srgb(1.0, 0.95, 0.85), illuminance: 11_000.0, shadows_enabled: true, ..default() },
        Transform::from_xyz(-9.0, 16.0, 7.0).looking_at(Vec3::ZERO, Vec3::Y),
        CascadeShadowConfigBuilder { maximum_distance: 60.0, first_cascade_far_bound: 18.0, ..default() }.build(),
    ));

    // Camera: starts south of the player looking north; `Rig` keeps it following afterwards.
    let rig = Rig::new(start_at + Vec3::Y * 0.9);
    commands.spawn((Camera3d::default(), rig.camera_transform(), MainCamera));
    commands.insert_resource(rig);

    // HUD.
    commands.spawn((
        Text::new("OZBOUND"),
        TextFont { font_size: 19.0, ..default() },
        TextColor(Color::WHITE),
        Node { position_type: PositionType::Absolute, top: Val::Px(10.0), left: Val::Px(10.0), padding: UiRect::all(Val::Px(8.0)), ..default() },
        BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.45)),
        HudText,
    ));
    commands.spawn((
        Text::new(""),
        TextFont { font_size: 22.0, ..default() },
        TextColor(Color::srgb(1.0, 0.95, 0.55)),
        Node { position_type: PositionType::Absolute, bottom: Val::Px(12.0), left: Val::Px(10.0), padding: UiRect::axes(Val::Px(10.0), Val::Px(4.0)), ..default() },
        BackgroundColor(Color::NONE),
        HoverHud,
    ));

    // Journal panel (toggled with J): `hud::update_journal_panel` fills in the text.
    commands.spawn((
        Text::new(""),
        TextFont { font_size: 17.0, ..default() },
        TextColor(Color::WHITE),
        Node { position_type: PositionType::Absolute, top: Val::Px(10.0), right: Val::Px(10.0), width: Val::Px(400.0), padding: UiRect::all(Val::Px(10.0)), ..default() },
        BackgroundColor(Color::srgba(0.05, 0.05, 0.08, 0.88)),
        Visibility::Hidden,
        JournalPanel,
    ));

    // Dialogue panel: hidden until a conversation opens; `dialogue_ui::sync_dialogue_ui` fills it in.
    commands.spawn((
        Node {
            position_type: PositionType::Absolute,
            left: Val::Percent(50.0),
            bottom: Val::Px(70.0),
            width: Val::Px(640.0),
            margin: UiRect::left(Val::Px(-320.0)),
            padding: UiRect::all(Val::Px(14.0)),
            flex_direction: FlexDirection::Column,
            row_gap: Val::Px(6.0),
            ..default()
        },
        BackgroundColor(Color::srgba(0.07, 0.06, 0.10, 0.94)),
        GlobalZIndex(9),
        Visibility::Hidden,
        DialoguePanel,
    ));

    // Crafting-and-gear panel (toggled with C): `pack_ui::sync_pack_ui` fills in the rows.
    commands.spawn((
        Node {
            position_type: PositionType::Absolute,
            left: Val::Percent(50.0),
            top: Val::Px(120.0),
            width: Val::Px(620.0),
            margin: UiRect::left(Val::Px(-310.0)),
            padding: UiRect::all(Val::Px(14.0)),
            flex_direction: FlexDirection::Column,
            row_gap: Val::Px(4.0),
            ..default()
        },
        BackgroundColor(Color::srgba(0.07, 0.06, 0.10, 0.94)),
        GlobalZIndex(9),
        Visibility::Hidden,
        PackPanel,
    ));

    // Right-click menu panel: hidden until opened; `menu::sync_menu_ui` fills in the rows.
    commands.spawn((
        Node { position_type: PositionType::Absolute, left: Val::Px(0.0), top: Val::Px(0.0), flex_direction: FlexDirection::Column, ..default() },
        BackgroundColor(Color::srgba(0.07, 0.07, 0.09, 0.95)),
        GlobalZIndex(10),
        Visibility::Hidden,
        ContextMenuPanel,
    ));
}
