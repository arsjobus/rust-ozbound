use bevy::prelude::*;
use ozbound::{inventory::{Inventory, ItemId}, quest::MoonpetalQuest};

const PLAYER_SPEED: f32 = 190.0;
const INTERACT_DISTANCE: f32 = 88.0;

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
        .add_systems(Startup, setup)
        .add_systems(
            Update,
            (
                move_player,
                interact_with_guide,
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

#[derive(Resource, Default)]
struct GameProgress {
    quest: MoonpetalQuest,
    spoke_to_guide: bool,
    journal: String,
    inventory: Inventory,
}

fn setup(mut commands: Commands) {
    commands.spawn(Camera2dBundle::default());

    // Meadow background.
    commands.spawn(SpriteBundle {
        sprite: Sprite {
            color: Color::srgb(0.30, 0.48, 0.30),
            custom_size: Some(Vec2::new(1800.0, 1200.0)),
            ..default()
        },
        transform: Transform::from_xyz(0.0, 0.0, -5.0),
        ..default()
    });

    // A winding, storybook path represented by simple blocks.
    for (position, size) in [
        (Vec3::new(0.0, -120.0, -2.0), Vec2::new(800.0, 78.0)),
        (Vec3::new(210.0, 20.0, -2.0), Vec2::new(70.0, 340.0)),
        (Vec3::new(-250.0, 115.0, -2.0), Vec2::new(300.0, 54.0)),
    ] {
        commands.spawn(SpriteBundle {
            sprite: Sprite {
                color: Color::srgb(0.72, 0.65, 0.48),
                custom_size: Some(size),
                ..default()
            },
            transform: Transform::from_translation(position),
            ..default()
        });
    }

    // Decorative trees around the meadow perimeter.
    for (x, y) in [
        (-400.0, 230.0), (-340.0, 270.0), (-470.0, 70.0),
        (390.0, 230.0), (450.0, 160.0), (420.0, -260.0),
        (-390.0, -260.0), (-300.0, -290.0), (60.0, 270.0),
    ] {
        commands.spawn(SpriteBundle {
            sprite: Sprite {
                color: Color::srgb(0.12, 0.30, 0.16),
                custom_size: Some(Vec2::new(54.0, 62.0)),
                ..default()
            },
            transform: Transform::from_xyz(x, y, 0.0),
            ..default()
        });
        commands.spawn(SpriteBundle {
            sprite: Sprite {
                color: Color::srgb(0.36, 0.24, 0.14),
                custom_size: Some(Vec2::new(14.0, 28.0)),
                ..default()
            },
            transform: Transform::from_xyz(x, y - 29.0, -0.5),
            ..default()
        });
    }

    // Player.
    commands.spawn((
        SpriteBundle {
            sprite: Sprite {
                color: Color::srgb(0.24, 0.60, 0.92),
                custom_size: Some(Vec2::new(28.0, 36.0)),
                ..default()
            },
            transform: Transform::from_xyz(-80.0, -110.0, 2.0),
            ..default()
        },
        Player,
    ));

    // Village guide NPC.
    commands.spawn((
        SpriteBundle {
            sprite: Sprite {
                color: Color::srgb(0.86, 0.70, 0.28),
                custom_size: Some(Vec2::new(30.0, 38.0)),
                ..default()
            },
            transform: Transform::from_xyz(110.0, 5.0, 2.0),
            ..default()
        },
        Guide,
    ));

    // Three moonpetals placed along the path.
    for (x, y) in [(-10.0, -145.0), (180.0, -115.0), (260.0, 90.0)] {
        commands.spawn((
            SpriteBundle {
                sprite: Sprite {
                    color: Color::srgb(0.85, 0.80, 0.98),
                    custom_size: Some(Vec2::new(18.0, 18.0)),
                    ..default()
                },
                transform: Transform::from_xyz(x, y, 1.0),
                ..default()
            },
            Moonpetal { collected: false },
        ));
    }

    // UI overlay.
    commands.spawn((
        TextBundle::from_section(
            "OZBOUND\nWASD / arrows: move   E: talk   Esc: quit\nFind the village guide to begin.",
            TextStyle {
                font_size: 20.0,
                color: Color::WHITE,
                ..default()
            },
        )
        .with_style(Style {
            position_type: PositionType::Absolute,
            top: Val::Px(14.0),
            left: Val::Px(16.0),
            ..default()
        }),
        HudText,
    ));

    commands.insert_resource(ClearColor(Color::srgb(0.19, 0.32, 0.22)));
}

fn move_player(
    time: Res<Time>,
    keys: Res<ButtonInput<KeyCode>>,
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
        let movement = direction.normalize() * PLAYER_SPEED * time.delta_seconds();
        transform.translation.x = (transform.translation.x + movement.x).clamp(-470.0, 470.0);
        transform.translation.y = (transform.translation.y + movement.y).clamp(-300.0, 300.0);
    }
}

fn interact_with_guide(
    keys: Res<ButtonInput<KeyCode>>,
    player: Query<&Transform, With<Player>>,
    guide: Query<&Transform, With<Guide>>,
    mut progress: ResMut<GameProgress>,
) {
    if !keys.just_pressed(KeyCode::KeyE) {
        return;
    }
    let (Ok(player), Ok(guide)) = (player.get_single(), guide.get_single()) else {
        return;
    };
    let distance = player.translation.truncate().distance(guide.translation.truncate());
    if distance <= INTERACT_DISTANCE {
        progress.spoke_to_guide = true;
        progress.journal = "Guide: The moonpetals glow after dusk. Please gather three for the village apothecary.".into();
    } else {
        progress.journal = "There is nobody close enough to talk to.".into();
    }
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
        if !petal.collected && player_pos.distance(transform.translation.truncate()) < 28.0 {
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
    let prompt = if nearby_guide { "\n[E] Talk to the village guide" } else { "" };
    let journal = if progress.journal.is_empty() {
        "Journal: A quiet meadow awaits."
    } else {
        &progress.journal
    };

    text.sections[0].value = format!(
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
