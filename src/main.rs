//! Ozbound: a low-poly 3D, click-to-move fairytale RPG. Game rules live in the `ozbound`
//! library crate (pure Rust, unit-tested); these modules only render, read input, and glue.

// Bevy systems take one parameter per resource/query they use, so long signatures are normal.
#![allow(clippy::too_many_arguments)]

mod animation;
mod camera_rig;
mod components;
mod hud;
mod interact;
mod menu;
mod models;
mod movement;
mod pointer;
mod setup;

use bevy::prelude::*;
use ozbound::{
    level::{Level, LoadError},
    scene,
};

use components::{ContextMenuState, GameProgress, HoverText, LevelData, Meadow};

/// Where the game looks for its level, relative to the working directory.
const LEVEL_PATH: &str = "assets/maps/meadow.map";
/// Set this to play a different level file; a missing or invalid file is then an error.
const LEVEL_ENV: &str = "OZBOUND_MAP";

/// Loads the level from disk. A custom path (`OZBOUND_MAP`) must load. The default file is
/// used when it exists; if it is simply absent (the game was started from another
/// directory) the identical built-in copy is used instead. An existing file that is
/// invalid is never papered over: the error says which file and line to fix.
fn load_level() -> Result<Level, String> {
    if let Some(custom) = std::env::var_os(LEVEL_ENV) {
        return Level::load(&custom).map_err(|e| e.to_string());
    }
    match Level::load(LEVEL_PATH) {
        Err(LoadError::Io { error, .. }) if error.kind() == std::io::ErrorKind::NotFound => {
            eprintln!("note: {LEVEL_PATH} not found; using the built-in meadow");
            scene::meadow_level().map_err(|e| format!("built-in meadow is invalid: {e}"))
        }
        other => other.map_err(|e| e.to_string()),
    }
}

fn main() {
    let level = load_level().unwrap_or_else(|message| {
        eprintln!("error: {message}");
        std::process::exit(1);
    });
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
        .insert_resource(ContextMenuState::default())
        .insert_resource(Meadow(level.map.clone()))
        .insert_resource(LevelData(level))
        .add_systems(Startup, setup::setup)
        .add_systems(
            Update,
            (
                pointer::pointer,
                menu::menu_input,
                movement::follow_route,
                interact::interact,
                interact::respawn_nodes,
                animation::animate_limbs,
                animation::float_props,
                pointer::fade_markers,
                camera_rig::update_camera,
                menu::sync_menu_ui,
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
