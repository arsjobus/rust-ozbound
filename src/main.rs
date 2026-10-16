//! Ozbound: a low-poly 3D, click-to-move fairytale RPG. Game rules live in the `ozbound`
//! library crate (pure Rust, unit-tested); these modules only render, read input, and glue.

// Bevy systems take one parameter per resource/query they use, so long signatures are normal.
#![allow(clippy::too_many_arguments)]

mod actions;
mod camera_rig;
mod components;
mod hud;
mod models;
mod pointer;
mod setup;

use bevy::prelude::*;
use ozbound::scene;

use components::{GameProgress, HoverText, Meadow};

fn main() {
    App::new()
        .add_plugins(DefaultPlugins.set(WindowPlugin {
            primary_window: Some(Window {
                title: "Ozbound — A Fairytale RPG".into(),
                resolution: (1100.0, 720.0).into(),
                ..default()
            }),
            ..default()
        }))
        .insert_resource(ClearColor(Color::srgb(0.56, 0.76, 0.92)))
        .insert_resource(GameProgress::default())
        .insert_resource(HoverText::default())
        .insert_resource(Meadow(scene::meadow_map().expect("built-in meadow map is valid")))
        .add_systems(Startup, setup::setup)
        .add_systems(
            Update,
            (
                pointer::pointer,
                actions::follow_route,
                actions::interact,
                actions::animate_limbs,
                actions::float_props,
                pointer::fade_markers,
                camera_rig::update_camera,
                hud::update_hud,
                exit_on_escape,
            )
                .chain(),
        )
        .run();
}

fn exit_on_escape(keys: Res<ButtonInput<KeyCode>>, mut exit: EventWriter<AppExit>) {
    if keys.just_pressed(KeyCode::Escape) {
        exit.send(AppExit::Success);
    }
}
