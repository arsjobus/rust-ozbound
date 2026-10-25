//! Startup: builds the 3D meadow, its props and characters, lights, camera, and HUD.

use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::f32::consts::TAU;

use bevy::image::ImageSampler;
use bevy::pbr::{CascadeShadowConfigBuilder, NotShadowCaster};
use bevy::prelude::*;
use bevy::ui::FocusPolicy;
use bevy::render::{
    mesh::{Indices, PrimitiveTopology},
    render_asset::RenderAssetUsages,
    render_resource::{Extent3d, TextureDimension, TextureFormat},
};
use ozbound::{
    ambient,
    appearance::{portrait_rgba, Appearance, PORTRAIT_SIZE},
    gathering::{validate_node_placement, GatheringNode, ResourceKind},
    heightfield::{HeightGrid, HeightSet},
    interaction::InteractionKind,
    level::{NodeSpawn, SignSpawn},
    overworld::Overworld,
    npc::{seed_from, Brain},
    respawn::RespawnTimer,
    save::WorldChanges,
    scene,
    task::Task,
    terrain_mesh::{build_terrain, Neighbour, Rect as TerrainRect, TerrainMaterial},
    world::{Tile, TilePos, WorldMap},
};

use ozbound::side_panel::Tab;
use ozbound::text_fit::{
    DIALOGUE_PANEL, DOCK_PANEL, HUD_PX, JOURNAL_PX, MARGIN, PACK_PANEL, PAUSE_PANEL, ROW_GAP, TOOLBAR_BUTTON_HEIGHT, TOOLBAR_BUTTON_PAD, TOOLBAR_GAP, TOOLBAR_PX,
};

use crate::camera_rig::Rig;
use crate::components::*;
use crate::models::{self, Kit};

fn vec3(v: [f32; 3]) -> Vec3 {
    Vec3::from_array(v)
}

/// Where something standing on `tile` is: the middle of the tile at the height of the ground
/// there, plus `lift`.
pub fn stand(map: &WorldMap, heights: &HeightSet, tile: TilePos, lift: f32) -> [f32; 3] {
    map.tile_center_3d(tile, heights.tile_height(tile) + lift)
}

/// Pick box centered on a tile: `half` wide in x and z, `height` tall, standing on the ground.
fn pick_box(map: &WorldMap, heights: &HeightSet, tile: TilePos, half: f32, height: f32) -> ([f32; 3], [f32; 3]) {
    let c = stand(map, heights, tile, 0.0);
    ([c[0] - half, c[1], c[2] - half], [c[0] + half, c[1] + height, c[2] + half])
}

/// The gathering rules for a node, set to what the save says is left on it (if it was used).
fn node_for(spawn: &NodeSpawn, saved: &WorldChanges) -> GatheringNode {
    let mut node = GatheringNode::new(spawn.kind, spawn.actions, spawn.yield_per_action);
    if let Some(left) = saved.node_remaining(spawn.tile) {
        node.set_remaining(left);
    }
    node
}

/// A node that was saved as spent starts the game spent: its crop, crown, or flecks are hidden
/// and its regrowth clock starts now (timers are not saved).
fn show_spent(commands: &mut Commands, root: Entity, spent_visual: Entity, node: &GatheringNode) {
    if !node.is_depleted() {
        return;
    }
    commands.entity(spent_visual).insert(Visibility::Hidden);
    if let Some(seconds) = node.kind().respawn_seconds() {
        commands.entity(root).insert(Respawning(RespawnTimer::new(seconds)));
    }
}

/// Startup, once: click markers, ambient sound, light, camera, and every panel. Nothing here
/// depends on the region, so travelling never rebuilds it.
pub fn setup_shell(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut sounds: ResMut<Assets<AudioSource>>,
    settings: Res<UserSettings>,
) {
    // Meshes and materials shared by everything the world spawns, built once so chunks that
    // stream in and out reuse them instead of making new ones.
    commands.insert_resource(Kit::new(&mut meshes, &mut materials));

    // Click markers: a yellow cross for walking, a red one for interacting.
    let marker = |color: Color| StandardMaterial { base_color: color, unlit: true, ..default() };
    commands.insert_resource(ClickAssets {
        bar: meshes.add(Cuboid::new(0.55, 0.03, 0.09)),
        walk: materials.add(marker(Color::srgb(1.0, 0.92, 0.2))),
        interact: materials.add(marker(Color::srgb(0.95, 0.15, 0.12))),
    });

    // Ambient sound: wind and distant birds, generated in code and looped quietly.
    let wav = ambient::meadow_loop_wav(24);
    let ambience = sounds.add(AudioSource { bytes: wav.into() });
    // Its volume follows the settings (`settings_ui::apply_settings` keeps it in step afterwards).
    commands.spawn((
        AudioPlayer::new(ambience),
        PlaybackSettings::LOOP.with_volume(bevy::audio::Volume::new(settings.0.ambient_gain())),
        AmbientSound,
    ));

    // Light: a warm sun with shadows plus soft sky fill.
    commands.insert_resource(AmbientLight { color: Color::srgb(0.80, 0.88, 1.0), brightness: 260.0 });
    commands.spawn((
        DirectionalLight { color: Color::srgb(1.0, 0.95, 0.85), illuminance: 11_000.0, shadows_enabled: true, ..default() },
        Transform::from_xyz(-9.0, 16.0, 7.0).looking_at(Vec3::ZERO, Vec3::Y),
        CascadeShadowConfigBuilder { maximum_distance: 60.0, first_cascade_far_bound: 18.0, ..default() }.build(),
    ));

    // Camera: `build_world` places it (and the follow rig) once it knows where the player stands.
    commands.spawn((Camera3d::default(), Transform::from_xyz(0.0, 8.0, 10.0).looking_at(Vec3::ZERO, Vec3::Y), MainCamera));

    // HUD: the hover label and the quest tracker, stacked in the top-left corner. Everything
    // else that used to live here has its own panel (chat, bag, skills, quest book).
    commands
        .spawn(Node {
            position_type: PositionType::Absolute,
            top: Val::Px(MARGIN),
            left: Val::Px(MARGIN),
            flex_direction: FlexDirection::Column,
            align_items: AlignItems::FlexStart,
            row_gap: Val::Px(6.0),
            ..default()
        })
        .with_children(|stack| {
            stack.spawn((
                Text::new(""),
                TextFont { font_size: 22.0, ..default() },
                TextColor(Color::srgb(1.0, 0.95, 0.55)),
                Node { padding: UiRect::axes(Val::Px(10.0), Val::Px(4.0)), ..default() },
                BackgroundColor(Color::NONE),
                HoverHud,
            ));
            stack.spawn((
                Text::new(""),
                TextFont { font_size: HUD_PX, ..default() },
                TextColor(Color::WHITE),
                Node { padding: UiRect::all(Val::Px(8.0)), ..default() },
                BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.45)),
                HudText,
            ));
        });

    // Toolbar: one button per side panel, top-right. `hud::toolbar_input` reads the clicks.
    commands
        .spawn(Node {
            position_type: PositionType::Absolute,
            top: Val::Px(MARGIN),
            right: Val::Px(MARGIN),
            flex_direction: FlexDirection::Row,
            column_gap: Val::Px(TOOLBAR_GAP),
            ..default()
        })
        .with_children(|bar| {
            for tab in Tab::ALL {
                bar.spawn((
                    Button,
                    Node {
                        height: Val::Px(TOOLBAR_BUTTON_HEIGHT),
                        padding: UiRect::axes(Val::Px(TOOLBAR_BUTTON_PAD), Val::Px(0.0)),
                        align_items: AlignItems::Center,
                        justify_content: JustifyContent::Center,
                        ..default()
                    },
                    BackgroundColor(Color::srgba(0.05, 0.05, 0.08, 0.78)),
                    ToolbarButton(tab),
                    UiBlocker,
                ))
                .with_children(|button| {
                    button.spawn((Text::new(tab.button_text()), TextFont { font_size: TOOLBAR_PX, ..default() }, TextColor(Color::WHITE)));
                });
            }
        });

    // Side panel (bag, skills, or quest book; toggled by the toolbar or B / K / J):
    // `hud::update_dock_panel` fills in the text.
    commands.spawn((
        Text::new(""),
        TextFont { font_size: JOURNAL_PX, ..default() },
        TextColor(Color::WHITE),
        Node {
            position_type: PositionType::Absolute,
            top: Val::Px(DOCK_PANEL.offset),
            right: Val::Px(MARGIN),
            width: Val::Px(DOCK_PANEL.width),
            padding: UiRect::all(Val::Px(DOCK_PANEL.padding)),
            ..default()
        },
        BackgroundColor(Color::srgba(0.05, 0.05, 0.08, 0.88)),
        Interaction::default(),
        FocusPolicy::Block,
        Visibility::Hidden,
        UiBlocker,
        DockPanel,
    ));

    // Dialogue panel: hidden until a conversation opens; `dialogue_ui::sync_dialogue_ui` fills it in.
    commands.spawn((
        Node {
            position_type: PositionType::Absolute,
            left: Val::Percent(50.0),
            bottom: Val::Px(DIALOGUE_PANEL.offset),
            width: Val::Px(DIALOGUE_PANEL.width),
            margin: UiRect::left(Val::Px(-DIALOGUE_PANEL.width / 2.0)),
            padding: UiRect::all(Val::Px(DIALOGUE_PANEL.padding)),
            flex_direction: FlexDirection::Column,
            row_gap: Val::Px(ROW_GAP),
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
            top: Val::Px(PACK_PANEL.offset),
            width: Val::Px(PACK_PANEL.width),
            margin: UiRect::left(Val::Px(-PACK_PANEL.width / 2.0)),
            padding: UiRect::all(Val::Px(PACK_PANEL.padding)),
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

    // Pause menu panel: hidden until Esc; `pause_ui::sync_pause_ui` fills in the rows.
    commands.spawn((
        Node {
            position_type: PositionType::Absolute,
            left: Val::Percent(50.0),
            top: Val::Px(PAUSE_PANEL.offset),
            width: Val::Px(PAUSE_PANEL.width),
            margin: UiRect::left(Val::Px(-PAUSE_PANEL.width / 2.0)),
            padding: UiRect::all(Val::Px(PAUSE_PANEL.padding)),
            flex_direction: FlexDirection::Column,
            row_gap: Val::Px(ROW_GAP),
            ..default()
        },
        BackgroundColor(Color::srgba(0.07, 0.06, 0.10, 0.96)),
        GlobalZIndex(20),
        Visibility::Hidden,
        PausePanel,
    ));

    // Frame-time overlay (F3, or Frame-time overlay in the pause menu): bottom right.
    commands.spawn((
        Text::new("measuring..."),
        TextFont { font_size: 16.0, ..default() },
        TextColor(Color::srgb(0.7, 1.0, 0.7)),
        Node { position_type: PositionType::Absolute, bottom: Val::Px(MARGIN), right: Val::Px(MARGIN), padding: UiRect::all(Val::Px(4.0)), ..default() },
        BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.55)),
        Visibility::Hidden,
        PerfHud,
    ));
}

/// A rectangle of tiles in the map's frame: columns `x0..x1`, rows `y0..y1`.
#[derive(Clone, Copy, Debug)]
pub struct Area {
    pub x0: i32,
    pub y0: i32,
    pub x1: i32,
    pub y1: i32,
}

impl Area {
    /// Every tile of the map.
    pub fn of_map(map: &WorldMap) -> Self {
        Self { x0: 0, y0: 0, x1: map.width() as i32, y1: map.height() as i32 }
    }

    /// The tiles of an overworld chunk, in world tiles.
    pub fn of_chunk(chunk: &ozbound::overworld::Chunk) -> Self {
        Self { x0: chunk.origin.x, y0: chunk.origin.y, x1: chunk.origin.x + chunk.width, y1: chunk.origin.y + chunk.height }
    }

    pub fn contains(&self, tile: TilePos) -> bool {
        tile.x >= self.x0 && tile.x < self.x1 && tile.y >= self.y0 && tile.y < self.y1
    }
}

/// Which scene the ground being built belongs to, and what lies beyond its edge: the true
/// tile (and whether it is a bridge deck) of the neighbouring scene, loaded or not, so a wall
/// on a seam is drawn once by the higher side (see `ozbound::terrain_mesh`).
pub struct Ground<'a> {
    pub scene: &'a str,
    /// True for a chunk of the open world (its ground is removed with the chunk); false for a
    /// whole region off the overworld (removed when the player travels).
    pub chunk: bool,
    pub neighbour: Neighbour<'a>,
    /// The ground heights of the scene (and its neighbours), built before anything is spawned.
    pub heights: &'a HeightSet,
}

/// The ground material handle for a terrain material.
fn ground_material(kit: &Kit, material: TerrainMaterial) -> Handle<StandardMaterial> {
    match material {
        TerrainMaterial::Grass(i) => kit.grass[i.min(2)].clone(),
        TerrainMaterial::DarkGrass => kit.dark_grass.clone(),
        TerrainMaterial::Path(i) => kit.path[i.min(1)].clone(),
        TerrainMaterial::Water(i) => kit.water[i.min(1)].clone(),
    }
}

/// What lies beside a chunk, from the overworld: the tile of whichever region holds the world
/// tile, whether that region is loaded or not.
pub fn overworld_neighbour(atlas: &Atlas) -> impl Fn(TilePos) -> Option<(Tile, bool)> + '_ {
    let bridges: RefCell<HashMap<String, HashSet<TilePos>>> = RefCell::new(HashMap::new());
    move |world| {
        let (chunk, local) = atlas.overworld.as_ref()?.to_local(world)?;
        let level = atlas.level(&chunk.id)?;
        let tile = level.map.tile_at(local).ok()?;
        let bridge = tile == Tile::Path
            && bridges
                .borrow_mut()
                .entry(chunk.id.clone())
                .or_insert_with(|| scene::bridge_tiles(&level.map).into_iter().map(|b| b.tile).collect())
                .contains(&local);
        Some((tile, bridge))
    }
}

/// Makes sure `heights` holds the ground of the chunks `ids`, building the ones it lacks from
/// the true tiles of the whole overworld (so a chunk's edge matches its neighbour's whether or
/// not the neighbour is loaded).
pub fn ensure_chunk_heights(heights: &mut HeightSet, overworld: &Overworld, atlas: &Atlas, ids: &[String]) {
    let lookup = overworld_neighbour(atlas);
    for id in ids {
        if heights.has(id) {
            continue;
        }
        let Some(chunk) = overworld.chunk(id) else { continue };
        let (x0, y0) = (chunk.origin.x, chunk.origin.y);
        heights.insert(id, HeightGrid::build(x0, y0, x0 + chunk.width, y0 + chunk.height, heights.half(), &lookup));
    }
}

/// The ground of a whole region off the overworld, in its own tiles.
pub fn region_heights(id: &str, map: &WorldMap) -> HeightSet {
    let half = (map.width() as f32 / 2.0, map.height() as f32 / 2.0);
    let bridges: HashSet<TilePos> = scene::bridge_tiles(map).into_iter().map(|b| b.tile).collect();
    let lookup = |pos: TilePos| map.tile_at(pos).ok().map(|tile| (tile, bridges.contains(&pos)));
    let mut set = HeightSet::new(half);
    set.insert(id, HeightGrid::build(0, 0, map.width() as i32, map.height() as i32, half, &lookup));
    set
}

/// Builds what stands on `area` of `map`: terrain, props, houses, people, and creatures, using
/// the spawns of `level` that lie inside the area. `changes` (in the map's tiles) says what the
/// player has already picked or used up there. Used for a whole region off the overworld and for
/// each chunk as it streams in; nothing here touches the player, the camera, or the floor.
#[allow(clippy::too_many_arguments)]
pub fn spawn_area(
    commands: &mut Commands,
    kit: &Kit,
    map: &WorldMap,
    level: &ozbound::level::Level,
    area: Area,
    changes: &WorldChanges,
    wildlife: &Wildlife,
    portraits: &mut Portraits,
    images: &mut Assets<Image>,
    meshes: &mut Assets<Mesh>,
    ground: &Ground,
) {
    let heights = ground.heights;
    // Ground: the scene's terrain as a few merged meshes, one per ground material (see
    // `ozbound::terrain_mesh`), tinted from the same small palette as before so the meadow isn't
    // one flat colour. Water sits lower than the grass, so its banks read as little cliffs.
    // Seams between scenes are stitched by that module's rule: the higher tile draws the wall.
    let bridges = scene::bridge_tiles(map);
    let bridge_set: HashSet<TilePos> = bridges.iter().map(|b| b.tile).collect();
    let terrain = build_terrain(
        ground.scene,
        map,
        TerrainRect { x0: area.x0, y0: area.y0, x1: area.x1, y1: area.y1 },
        &|pos| bridge_set.contains(&pos),
        ground.neighbour,
    );
    for (material, part) in &terrain.parts {
        let mesh = Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default())
            .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, part.positions.clone())
            .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, part.normals.clone())
            .with_inserted_indices(Indices::U32(part.indices.clone()));
        // The mesh is positioned by its vertices, so it is tagged here: `tag_world_entities`
        // finds a scene by an entity's position, and this one sits at the origin.
        let mut entity = commands.spawn((
            Mesh3d(meshes.add(mesh)),
            MeshMaterial3d(ground_material(kit, *material)),
            Transform::IDENTITY,
            SceneTerrain,
            NotShadowCaster,
        ));
        if ground.chunk {
            entity.insert(ChunkEntity(terrain.scene.clone()));
        } else {
            entity.insert(RegionEntity);
        }
    }

    // Validate resource nodes once; scenery below attaches gameplay to the valid ones.
    let mut nodes: HashMap<TilePos, NodeSpawn> = HashMap::new();
    for node in level.nodes.iter().copied().filter(|n| area.contains(n.tile)) {
        match validate_node_placement(map, node.tile, node.kind) {
            Ok(()) => {
                nodes.insert(node.tile, node);
            }
            Err(error) => error!("skipping {} at {:?}: {error:?}", node.kind.label(), node.tile),
        }
    }

    let signs: HashMap<TilePos, &SignSpawn> = level.signs.iter().filter(|s| area.contains(s.tile)).map(|sign| (sign.tile, sign)).collect();

    // Trees, rocks, fences, and signposts stand on every blocking tile. Only the trees and
    // rocks listed as resource nodes, and the signposts listed in `[signs]`, can be clicked;
    // the rest is scenery that shuts the player in. Collision itself comes from the tile
    // map (`Tile::is_walkable`), never from these models.
    for y in area.y0..area.y1 {
        for x in area.x0..area.x1 {
            let pos = TilePos::new(x, y);
            let tile = map.tile_at(pos).expect("loop stays inside the map");
            let at = vec3(stand(map, heights, pos, 0.0));
            let yaw = scene::tile_noise(pos, 4) * TAU;
            match tile {
                Tile::Tree => {
                    let scale = 0.9 + scene::tile_noise(pos, 5) * 0.3;
                    let leaf = scene::tile_variant(pos, 6, 3);
                    // A tree tile with no chopping node is a thicket, so a tree that can be harvested
                    // is never confused with the scenery that closes the paths.
                    let Some(node) = nodes.get(&pos).filter(|n| n.kind == ResourceKind::Tree) else {
                        models::spawn_thicket(commands, kit, at, yaw, scale, leaf);
                        continue;
                    };
                    let (root, crown) = models::spawn_tree(commands, kit, at, yaw, scale, leaf);
                    {
                        let (pick_min, pick_max) = pick_box(map, heights, pos, 0.45, 2.5 * scale);
                        let gathering = node_for(node, changes);
                        show_spent(commands, root, crown, &gathering);
                        commands.entity(root).insert((
                            ResourceNode(gathering),
                            SpentVisual(crown),
                            Interactable { kind: InteractionKind::ChopDown, name: "tree", tile: pos, approach: InteractionKind::ChopDown.approach(), pick_min, pick_max },
                        ));
                    }
                }
                Tile::Rock => {
                    let scale = 0.95 + scene::tile_noise(pos, 7) * 0.2;
                    let shade = scene::tile_variant(pos, 8, 2);
                    let node = nodes.get(&pos).filter(|n| n.kind == ResourceKind::CopperVein);
                    let (root, flecks) = models::spawn_rock(commands, kit, at, yaw, scale, shade, node.is_some());
                    if let (Some(node), Some(flecks)) = (node, flecks) {
                        let (pick_min, pick_max) = pick_box(map, heights, pos, 0.5, 1.0);
                        let gathering = node_for(node, changes);
                        show_spent(commands, root, flecks, &gathering);
                        commands.entity(root).insert((
                            ResourceNode(gathering),
                            SpentVisual(flecks),
                            Interactable { kind: InteractionKind::Mine, name: "copper vein", tile: pos, approach: InteractionKind::Mine.approach(), pick_min, pick_max },
                        ));
                    }
                }
                Tile::Garden => {
                    models::spawn_garden_bed(commands, kit, at, (scene::tile_noise(pos, 11) - 0.5) * 0.12, scene::tile_variant(pos, 12, 3));
                }
                Tile::Vine => {
                    let along_x = scene::vine_runs_along_x(map, pos);
                    models::spawn_vine_row(commands, kit, at, along_x, 2 + scene::tile_variant(pos, 13, 3));
                }
                Tile::Fence => {
                    let rails = scene::fence_links(map, pos).rails();
                    models::spawn_fence(commands, kit, at, &rails);
                }
                Tile::Sign => {
                    let post = models::spawn_signpost(commands, kit, at, (scene::tile_noise(pos, 9) - 0.5) * 0.5);
                    if let Some(sign) = signs.get(&pos) {
                        let (pick_min, pick_max) = pick_box(map, heights, pos, 0.4, 1.2);
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

    // Wooden bridges where the path crosses water.
    for bridge in bridges.iter().filter(|b| area.contains(b.tile)) {
        // The deck is level at y = 0, over the river bed below it.
        let at = vec3(map.tile_center_3d(bridge.tile, 0.0));
        models::spawn_bridge_tile(commands, kit, at, bridge.along_x, bridge.first, bridge.last);
    }

    // Houses: walls (lowered while the player is inside) and a roof (hidden while inside).
    for house in level.houses.iter().filter(|h| area.contains(h.origin)) {
        let (walls, roof) = models::spawn_house(commands, kit, map, house, scene::tile_variant(house.origin, 14, 2));
        commands.entity(walls).insert(HouseWalls(house.id.clone()));
        commands.entity(roof).insert(HouseRoof(house.id.clone()));
    }

    // Moonpetals along the path. They are picked up by clicking them.
    for (i, tile) in level.moonpetals.iter().copied().enumerate().filter(|(_, t)| area.contains(*t)) {
        // A petal picked in an earlier session stays picked.
        if changes.is_picked(tile) {
            continue;
        }
        if let Err(error) = validate_node_placement(map, tile, ResourceKind::Moonpetal) {
            error!("skipping moonpetal at {tile:?}: {error:?}");
            continue;
        }
        let at = vec3(stand(map, heights, tile, 0.12));
        let (pick_min, pick_max) = pick_box(map, heights, tile, 0.4, 0.7);
        let flower = models::spawn_moonpetal(commands, kit, at);
        commands.entity(flower).insert((
            Moonpetal,
            Floaty { base_y: at.y, phase: i as f32 * 2.1 },
            Interactable { kind: InteractionKind::Pick, name: "moonpetal", tile, approach: InteractionKind::Pick.approach(), pick_min, pick_max },
        ));
    }

    // Herb patches and berry bushes grow on open ground and are used by standing on them.
    // Only valid, listed nodes become clickable; the crop (not the soil or bush) hides when spent.
    for spawn in level.nodes.iter().filter(|n| area.contains(n.tile) && matches!(n.kind, ResourceKind::HerbPatch | ResourceKind::BerryBush)) {
        let pos = spawn.tile;
        if !nodes.contains_key(&pos) {
            continue;
        }
        let at = vec3(stand(map, heights, pos, 0.0));
        let yaw = scene::tile_noise(pos, 10) * TAU;
        let (root, crop, height) = match spawn.kind {
            ResourceKind::HerbPatch => {
                let (root, plants) = models::spawn_herb_patch(commands, kit, at, yaw);
                (root, plants, 0.5)
            }
            _ => {
                let (root, berries) = models::spawn_berry_bush(commands, kit, at, yaw);
                (root, berries, 0.8)
            }
        };
        let (pick_min, pick_max) = pick_box(map, heights, pos, 0.42, height);
        let kind = InteractionKind::Forage;
        let gathering = node_for(spawn, changes);
        show_spent(commands, root, crop, &gathering);
        commands.entity(root).insert((
            ResourceNode(gathering),
            SpentVisual(crop),
            Interactable { kind, name: spawn.kind.label(), tile: pos, approach: kind.approach(), pick_min, pick_max },
        ));
    }

    let start_at = vec3(stand(map, heights, level.player_start, 0.0));

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

    // The village guide faces the player's starting tile. Only the story level has one.
    if let Some(guide_tile) = level.guide.filter(|t| area.contains(*t)) {
        let guide_at = vec3(stand(map, heights, guide_tile, 0.0));
        let toward_start = ozbound::task::facing_yaw(start_at.x - guide_at.x, start_at.z - guide_at.z).unwrap_or(0.0);
        let guide = models::spawn_humanoid(commands, kit, &kit.guide, guide_at, toward_start);
        let (pick_min, pick_max) = pick_box(map, heights, guide_tile, 0.4, 1.95);
        commands.entity(guide).insert((
            Guide,
            NpcBrain(Brain::new(level.behavior_of(ozbound::level::GUIDE_ID), guide_tile, seed_from(ozbound::level::GUIDE_ID))),
            NpcBody::default(),
            Npc(ozbound::level::GUIDE_ID.to_string()),
            Interactable { kind: InteractionKind::TalkTo, name: "Village Guide", tile: guide_tile, approach: InteractionKind::TalkTo.approach(), pick_min, pick_max },
        ));
        portraits.0.insert(ozbound::level::GUIDE_ID.to_string(), portrait_for("guide", images));
    }

    // The other villagers, each facing the player's starting tile like the guide does.
    // Interactable names are `&'static str`; the few names in the level file are leaked once
    // at startup, which is simpler than threading a lifetime through every system.
    for npc in level.npcs.iter().filter(|n| area.contains(n.tile)) {
        let at = vec3(stand(map, heights, npc.tile, 0.0));
        let toward = ozbound::task::facing_yaw(start_at.x - at.x, start_at.z - at.z).unwrap_or(0.0);
        let body = models::spawn_humanoid(commands, kit, kit.look_for(npc.look.key().unwrap_or("traveller")), at, toward);
        let (pick_min, pick_max) = pick_box(map, heights, npc.tile, 0.4, 1.95);
        let name: &'static str = Box::leak(npc.name.clone().into_boxed_str());
        commands.entity(body).insert((
            NpcBrain(Brain::new(level.behavior_of(&npc.id), npc.tile, seed_from(&npc.id))),
            NpcBody::default(),
            Npc(npc.id.clone()),
            Interactable { kind: InteractionKind::TalkTo, name, tile: npc.tile, approach: InteractionKind::TalkTo.approach(), pick_min, pick_max },
        ));
        portraits.0.insert(npc.id.clone(), portrait_for(npc.look.key().unwrap_or("traveller"), images));
    }

    // Named places are marked by a standing stone, so the player can see where to walk to.
    for place in level.places.iter().filter(|p| area.contains(p.tile)) {
        let at = vec3(stand(map, heights, place.tile, 0.0));
        models::spawn_waymark(commands, kit, at, scene::tile_noise(place.tile, 11) * TAU);
    }

    // Creatures stand on open ground and are approached from beside, never clicked through.
    for (i, spawn) in level.creatures.iter().enumerate().filter(|(_, c)| area.contains(c.tile)) {
        let Some(creature) = wildlife.bestiary.get(&spawn.id) else { continue };
        let moth = spawn.id == "lantern_moth";
        let at = vec3(stand(map, heights, spawn.tile, if moth { 0.8 } else { 0.0 }));
        let toward = ozbound::task::facing_yaw(start_at.x - at.x, start_at.z - at.z).unwrap_or(0.0);
        let body = models::spawn_creature(commands, kit, &spawn.id, at, toward);
        let (pick_min, pick_max) = pick_box(map, heights, spawn.tile, 0.45, if moth { 1.3 } else { 1.6 });
        let name: &'static str = Box::leak(creature.name.clone().into_boxed_str());
        commands.entity(body).insert((
            Creature(spawn.id.clone()),
            Interactable { kind: InteractionKind::Approach, name, tile: spawn.tile, approach: InteractionKind::Approach.approach(), pick_min, pick_max },
        ));
        if moth {
            commands.entity(body).insert(Floaty { base_y: at.y, phase: i as f32 * 1.7 });
        }
    }

}

/// Builds the world the player is in: the floor, the player, the camera, and the terrain and
/// everything on it. Runs at startup and again after every journey that leaves or enters the
/// overworld (see `travel.rs`, which despawns the previous world first).
///
/// On the overworld it works out which chunks are near the player, stitches them into
/// `Meadow` / `LevelData` (in world coordinates), and spawns each; `streaming::stream_chunks`
/// keeps that set up to date from then on. Off the overworld (a cave, or when a custom map
/// replaces Bluebrook) `Meadow` / `LevelData` already hold the one region in its own tiles and
/// the whole map is spawned.
pub fn build_world(
    mut commands: Commands,
    mut meadow: ResMut<Meadow>,
    mut level: ResMut<LevelData>,
    saved: Res<StartingSave>,
    atlas: Res<Atlas>,
    current: Res<CurrentRegion>,
    mut streaming: ResMut<Streaming>,
    mut memory: ResMut<ChunkMemory>,
    mut heights: ResMut<GroundHeights>,
    wildlife: Res<Wildlife>,
    kit: Res<Kit>,
    mut portraits: ResMut<Portraits>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut images: ResMut<Assets<Image>>,
    mut cameras: Query<&mut Transform, With<MainCamera>>,
) {
    let overworld = atlas.overworld.as_ref().filter(|o| o.chunk(&current.0).is_some());
    let player_tile = if let Some(overworld) = overworld {
        // The save's tile and changes belong to the region the player is in (its own tiles).
        let tile = overworld.to_world(&current.0, saved.0.player).expect("the current region is a chunk");
        memory.0.entry(current.0.clone()).or_insert_with(|| overworld.changes_to_world(&current.0, &saved.0.changes));
        let loaded = overworld.wanted(tile, &[]);
        let composite = overworld.compose(&|id| atlas.level(id), &loaded);
        let neighbour = overworld_neighbour(&atlas);
        let (width, height) = overworld.bounds();
        heights.0 = HeightSet::new((width as f32 / 2.0, height as f32 / 2.0));
        ensure_chunk_heights(&mut heights.0, overworld, &atlas, &loaded);
        for id in &loaded {
            let chunk = overworld.chunk(id).expect("wanted chunks exist");
            let none = WorldChanges::none();
            let ground = Ground { scene: id, chunk: true, neighbour: &neighbour, heights: &heights.0 };
            spawn_area(&mut commands, &kit, &composite.map, &composite.level, Area::of_chunk(chunk), memory.0.get(id).unwrap_or(&none), &wildlife, &mut portraits, &mut images, &mut meshes, &ground);
        }
        meadow.0 = composite.map;
        level.0 = composite.level;
        streaming.loaded = loaded;
        streaming.interior = false;
        tile
    } else {
        streaming.loaded.clear();
        streaming.interior = true;
        heights.0 = region_heights(&current.0, &meadow.0);
        let ground = Ground { scene: &current.0, chunk: false, neighbour: &|_| None, heights: &heights.0 };
        spawn_area(&mut commands, &kit, &meadow.0, &level.0, Area::of_map(&meadow.0), &saved.0.changes, &wildlife, &mut portraits, &mut images, &mut meshes, &ground);
        saved.0.player
    };
    let map = &meadow.0;

    // The player stands where the save left them (the level's start in a new game).
    let player_at = vec3(stand(map, &heights.0, player_tile, 0.0));
    let player = models::spawn_humanoid(&mut commands, &kit, &kit.traveller, player_at, 0.0);
    commands.entity(player).insert((Player { tile: player_tile, task: Task::idle(), work: None, emote: None }, PlayerProgress::from_save(&saved.0)));

    // A wide dark floor under the world so the map edge doesn't float in the sky colour.
    let floor = (map.width().max(map.height()) as f32 + 160.0).max(240.0);
    commands.spawn((
        Mesh3d(meshes.add(Plane3d::default().mesh().size(floor, floor))),
        MeshMaterial3d(materials.add(StandardMaterial { base_color: Color::srgb(0.16, 0.30, 0.20), perceptual_roughness: 1.0, ..default() })),
        Transform::from_xyz(0.0, -0.55, 0.0),
        NotShadowCaster,
        WorldFloor,
    ));

    // The follow camera starts south of the player looking north; `Rig` keeps it following.
    let rig = Rig::new(player_at + Vec3::Y * 0.9);
    if let Ok(mut transform) = cameras.get_single_mut() {
        *transform = rig.camera_transform();
    }
    commands.insert_resource(rig);
}
