use bevy::prelude::*;
use ozbound::{
    gathering::{validate_node_placement, GatherError, GatheringNode, ResourceKind},
    interaction::{nearest_in_range, within_interaction_range},
    inventory::{Inventory, InventoryError, ItemId},
    movement::resolve_move,
    quest::MoonpetalQuest,
    scene::{self, NodeSpawn},
    world::{Tile, TilePos, WorldMap, TILE_SIZE},
};

const PLAYER_SPEED: f32 = 190.0;
const INTERACT_DISTANCE: f32 = 88.0;
/// Trees and ore sit on blocking tiles, so the player stands beside them:
/// 1.75 tiles reaches an adjacent tile center (1 tile) but not two tiles away.
const GATHER_DISTANCE: f32 = TILE_SIZE * 1.75;
const PETAL_PICKUP_DISTANCE: f32 = 28.0;
/// Collision box half extents (x, y) around the player's center.
const PLAYER_HALF_EXTENTS: (f32, f32) = (11.0, 13.0);

fn main() {
    App::new()
        .add_plugins(DefaultPlugins.set(WindowPlugin {
            primary_window: Some(Window {
                title: "Ozbound — A Fairytale RPG".into(),
                resolution: (1000.0, 700.0).into(),
                ..default()
            }),
            ..default()
        }))
        .insert_resource(GameProgress::default())
        .insert_resource(Meadow(scene::meadow_map().expect("built-in meadow map is valid")))
        .add_systems(Startup, setup)
        .add_systems(
            Update,
            (
                move_player,
                interact,
                collect_moonpetals,
                update_hud,
            ),
        )
        .run();
}

#[derive(Component)]
struct Player;

#[derive(Component)]
struct Guide;

#[derive(Component)]
struct Moonpetal {
    collected: bool,
}

#[derive(Component)]
struct HudText;

/// The collision/rules map for the current scene. Rendering is derived from it.
#[derive(Resource)]
struct Meadow(WorldMap);

/// A tree or ore vein in the world, backed by the tested gathering rules.
#[derive(Component)]
struct ResourceNode(GatheringNode);

fn resource_label(kind: ResourceKind) -> &'static str {
    match kind {
        ResourceKind::Moonpetal => "moonpetal",
        ResourceKind::Tree => "tree",
        ResourceKind::CopperVein => "copper vein",
    }
}

fn tile_color(tile: Tile) -> Color {
    match tile {
        Tile::Grass => Color::srgb(0.30, 0.48, 0.30),
        Tile::Path => Color::srgb(0.72, 0.65, 0.48),
        Tile::Water => Color::srgb(0.20, 0.40, 0.70),
        Tile::Tree => Color::srgb(0.12, 0.30, 0.16),
        Tile::Rock => Color::srgb(0.45, 0.45, 0.48),
    }
}

fn node_color(kind: ResourceKind, depleted: bool) -> Color {
    match (kind, depleted) {
        (ResourceKind::Tree, false) => Color::srgb(0.36, 0.24, 0.14),
        (ResourceKind::CopperVein, false) => Color::srgb(0.80, 0.47, 0.25),
        (_, true) => Color::srgb(0.25, 0.25, 0.25),
        (ResourceKind::Moonpetal, false) => Color::srgb(0.85, 0.80, 0.98),
    }
}

fn tile_center_vec3(map: &WorldMap, tile: TilePos, z: f32) -> Vec3 {
    let (x, y) = map.tile_center(tile);
    Vec3::new(x, y, z)
}

#[derive(Resource, Default)]
struct GameProgress {
    quest: MoonpetalQuest,
    spoke_to_guide: bool,
    journal: String,
    inventory: Inventory,
}

fn setup(mut commands: Commands, meadow: Res<Meadow>) {
    let map = &meadow.0;
    commands.spawn(Camera2d);

    // One sprite per tile; the map is the single source of truth for what is solid.
    for y in 0..map.height() as i32 {
        for x in 0..map.width() as i32 {
            let tile = map.tile_at(TilePos::new(x, y)).expect("loop stays inside the map");
            commands.spawn((
                Sprite {
                    color: tile_color(tile),
                    custom_size: Some(Vec2::splat(TILE_SIZE)),
                    ..default()
                },
                Transform::from_translation(tile_center_vec3(map, TilePos::new(x, y), -5.0)),
            ));
        }
    }

    // Trees and copper veins: each sits on a blocking tile and is gathered from beside it.
    for NodeSpawn { tile, kind, actions, yield_per_action } in scene::RESOURCE_NODES {
        if let Err(error) = validate_node_placement(map, tile, kind) {
            error!("skipping {} at {tile:?}: {error:?}", resource_label(kind));
            continue;
        }
        commands.spawn((
            (
                Sprite {
                    color: node_color(kind, false),
                    custom_size: Some(Vec2::splat(TILE_SIZE * 0.6)),
                    ..default()
                },
                Transform::from_translation(tile_center_vec3(map, tile, 0.0)),
            ),
            ResourceNode(GatheringNode::new(kind, actions, yield_per_action)),
        ));
    }

    // Player.
    commands.spawn((
        (
            Sprite {
                color: Color::srgb(0.24, 0.60, 0.92),
                custom_size: Some(Vec2::new(28.0, 36.0)),
                ..default()
            },
            Transform::from_translation(tile_center_vec3(map, scene::PLAYER_START, 2.0)),
        ),
        Player,
    ));

    // Village guide NPC.
    commands.spawn((
        (
            Sprite {
                color: Color::srgb(0.86, 0.70, 0.28),
                custom_size: Some(Vec2::new(30.0, 38.0)),
                ..default()
            },
            Transform::from_translation(tile_center_vec3(map, scene::GUIDE_TILE, 2.0)),
        ),
        Guide,
    ));

    // Three moonpetals placed along the path.
    for tile in scene::MOONPETAL_TILES {
        if let Err(error) = validate_node_placement(map, tile, ResourceKind::Moonpetal) {
            error!("skipping moonpetal at {tile:?}: {error:?}");
            continue;
        }
        commands.spawn((
            (
                Sprite {
                    color: node_color(ResourceKind::Moonpetal, false),
                    custom_size: Some(Vec2::new(18.0, 18.0)),
                    ..default()
                },
                Transform::from_translation(tile_center_vec3(map, tile, 1.0)),
            ),
            Moonpetal { collected: false },
        ));
    }

    // UI overlay.
    commands.spawn((
        Text::new("OZBOUND\nWASD / arrows: move   E: interact   Esc: quit\nFind the village guide to begin."),
        TextFont { font_size: 20.0, ..default() },
        TextColor(Color::WHITE),
        Node {
            position_type: PositionType::Absolute,
            top: Val::Px(14.0),
            left: Val::Px(16.0),
            ..default()
        },
        HudText,
    ));

    commands.insert_resource(ClearColor(Color::srgb(0.19, 0.32, 0.22)));
}

fn move_player(
    time: Res<Time>,
    keys: Res<ButtonInput<KeyCode>>,
    meadow: Res<Meadow>,
    mut player: Query<&mut Transform, With<Player>>,
) {
    let Ok(mut transform) = player.get_single_mut() else {
        return;
    };

    let mut direction = Vec2::ZERO;
    if keys.pressed(KeyCode::KeyW) || keys.pressed(KeyCode::ArrowUp) {
        direction.y += 1.0;
    }
    if keys.pressed(KeyCode::KeyS) || keys.pressed(KeyCode::ArrowDown) {
        direction.y -= 1.0;
    }
    if keys.pressed(KeyCode::KeyA) || keys.pressed(KeyCode::ArrowLeft) {
        direction.x -= 1.0;
    }
    if keys.pressed(KeyCode::KeyD) || keys.pressed(KeyCode::ArrowRight) {
        direction.x += 1.0;
    }

    if direction != Vec2::ZERO {
        let movement = direction.normalize() * PLAYER_SPEED * time.delta_secs();
        // Collision (including the map edge) is decided by the tested pure rules.
        let (x, y) = resolve_move(
            &meadow.0,
            (transform.translation.x, transform.translation.y),
            (movement.x, movement.y),
            PLAYER_HALF_EXTENTS,
        );
        transform.translation.x = x;
        transform.translation.y = y;
    }
}

fn interact(
    keys: Res<ButtonInput<KeyCode>>,
    player: Query<&Transform, With<Player>>,
    guide: Query<&Transform, With<Guide>>,
    mut nodes: Query<(&Transform, &mut ResourceNode, &mut Sprite)>,
    mut progress: ResMut<GameProgress>,
) {
    if !keys.just_pressed(KeyCode::KeyE) {
        return;
    }
    let (Ok(player), Ok(guide)) = (player.get_single(), guide.get_single()) else {
        return;
    };
    let actor = (player.translation.x, player.translation.y);

    // The guide takes priority over resources when both are in range.
    if within_interaction_range(actor, (guide.translation.x, guide.translation.y), INTERACT_DISTANCE) {
        progress.spoke_to_guide = true;
        progress.journal = "Guide: The moonpetals glow after dusk. Please gather three for the village apothecary.".into();
        return;
    }

    let candidates = nodes
        .iter()
        .enumerate()
        .map(|(index, (transform, _, _))| (index, (transform.translation.x, transform.translation.y)));
    let Some(index) = nearest_in_range(actor, candidates, GATHER_DISTANCE) else {
        progress.journal = "There is nothing close enough to interact with.".into();
        return;
    };
    let Some((_, mut node, mut sprite)) = nodes.iter_mut().nth(index) else {
        return;
    };

    let kind = node.0.kind();
    // Gathering only consumes the node when the inventory accepted the yield.
    progress.journal = match node.0.gather(&mut progress.inventory) {
        Ok(amount) if node.0.is_depleted() => {
            sprite.color = node_color(kind, true);
            format!("You gathered {amount} from the {}. It is spent for now.", resource_label(kind))
        }
        Ok(amount) => format!(
            "You gathered {amount} from the {} ({} left).",
            resource_label(kind),
            node.0.remaining_actions()
        ),
        Err(GatherError::Depleted) => format!("This {} has nothing left to give.", resource_label(kind)),
        Err(GatherError::Inventory(InventoryError::Full)) => {
            "Your inventory is full. Make room before gathering more.".into()
        }
        Err(GatherError::Inventory(_)) => "You cannot carry that right now.".into(),
    };
}

fn collect_moonpetals(
    mut commands: Commands,
    player: Query<&Transform, With<Player>>,
    mut petals: Query<(Entity, &Transform, &mut Moonpetal)>,
    mut progress: ResMut<GameProgress>,
) {
    let Ok(player) = player.get_single() else {
        return;
    };
    let player_pos = player.translation.truncate();

    for (entity, transform, mut petal) in &mut petals {
        if !petal.collected && player_pos.distance(transform.translation.truncate()) < PETAL_PICKUP_DISTANCE {
            // Inventory first: a full bag must never silently consume a world resource.
            if progress.inventory.add(ItemId::Moonpetal, 1).is_ok() {
                petal.collected = true;
                commands.entity(entity).despawn();
                progress.quest.collect_one();
                progress.journal = if progress.quest.is_complete() {
                    "Quest objective complete! Return to the guide with the moonpetals.".into()
                } else {
                    format!("You gathered a moonpetal. {}", progress.quest.progress_text())
                };
            } else {
                progress.journal = "Your inventory is full. Make room before gathering more.".into();
            }
        }
    }
}

fn update_hud(
    progress: Res<GameProgress>,
    mut hud: Query<&mut Text, With<HudText>>,
    player: Query<&Transform, With<Player>>,
    guide: Query<&Transform, With<Guide>>,
    nodes: Query<(&Transform, &ResourceNode)>,
) {
    let Ok(mut text) = hud.get_single_mut() else {
        return;
    };

    let nearby_guide = match (player.get_single(), guide.get_single()) {
        (Ok(player), Ok(guide)) => {
            player.translation.truncate().distance(guide.translation.truncate()) <= INTERACT_DISTANCE
        }
        _ => false,
    };

    let objective = if progress.quest.is_complete() {
        "Objective: Return to the guide"
    } else if progress.spoke_to_guide {
        "Objective: Gather 3 moonpetals"
    } else {
        "Objective: Talk to the village guide"
    };
    let nearby_node = player.get_single().ok().and_then(|player| {
        let actor = (player.translation.x, player.translation.y);
        let candidates = nodes
            .iter()
            .enumerate()
            .map(|(index, (transform, _))| (index, (transform.translation.x, transform.translation.y)));
        nearest_in_range(actor, candidates, GATHER_DISTANCE)
            .and_then(|index| nodes.iter().nth(index))
            .map(|(_, node)| node.0.clone())
    });
    let prompt = match (nearby_guide, nearby_node) {
        (true, _) => "\n[E] Talk to the village guide".to_string(),
        (false, Some(node)) if node.is_depleted() => format!("\nThis {} is spent", resource_label(node.kind())),
        (false, Some(node)) => format!("\n[E] Gather from the {}", resource_label(node.kind())),
        (false, None) => String::new(),
    };
    let journal = if progress.journal.is_empty() {
        "Journal: A quiet meadow awaits."
    } else {
        &progress.journal
    };

    text.0 = format!(
        "OZBOUND — A FAIRYTALE RPG\nWASD / arrows: move   E: interact   Esc: quit\n{}\n{}{}\nBag: {} moonpetals | {} timber | {} copper ore\n{}",
        objective,
        progress.quest.progress_text(),
        prompt,
        progress.inventory.count(ItemId::Moonpetal),
        progress.inventory.count(ItemId::Timber),
        progress.inventory.count(ItemId::CopperOre),
        journal,
    );
}
