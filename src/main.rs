//! Ozbound: a low-poly 3D, click-to-move fairytale RPG. Game rules live in the `ozbound`
//! library crate (pure Rust, unit-tested); these modules only render, read input, and glue.

// Bevy systems take one parameter per resource/query they use, so long signatures are normal.
#![allow(clippy::too_many_arguments)]

mod animation;
mod camera_rig;
mod components;
mod dialogue_ui;
mod hud;
mod interact;
mod menu;
mod models;
mod movement;
mod pack_ui;
mod persist;
mod pointer;
mod setup;

use std::path::PathBuf;

use bevy::prelude::*;
use ozbound::{
    content::{self, Content},
    crafting::{self, RecipeBook},
    level::{Level, LoadError},
    quest_store,
    quests::QuestLog,
    scene,
    skills,
};

use components::{ContextMenuState, GameContent, GameProgress, HoverText, LevelData, Meadow, PackState, QuestSave, Recipes, TalkState};

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

/// Set these to play other quest or dialogue files; a missing or invalid file is then an error.
const QUESTS_ENV: &str = "OZBOUND_QUESTS";
const DIALOGUE_ENV: &str = "OZBOUND_DIALOGUE";
/// Where quest progress is saved (override with `OZBOUND_QUEST_SAVE`).
const QUEST_SAVE_PATH: &str = "saves/quests.txt";
const QUEST_SAVE_ENV: &str = "OZBOUND_QUEST_SAVE";

/// Loads the quest and dialogue files with the same rules as the level: custom paths must
/// load, absent default files fall back to the built-in copies, and an invalid file stops
/// startup with its name and line.
fn load_content() -> Result<Content, String> {
    let loaded = content::load_with_custom(
        std::env::var_os(QUESTS_ENV).map(PathBuf::from),
        std::env::var_os(DIALOGUE_ENV).map(PathBuf::from),
    )?;
    for note in &loaded.notes {
        eprintln!("{note}");
    }
    Ok(loaded.content)
}

/// Set this to play other recipes; a missing or invalid file is then an error.
const RECIPES_ENV: &str = "OZBOUND_RECIPES";

/// Loads the recipe file with the same rules as the other content files.
fn load_recipes() -> Result<RecipeBook, String> {
    let custom = std::env::var_os(RECIPES_ENV).map(PathBuf::from);
    let loaded = crafting::load(custom.as_deref(), std::path::Path::new(crafting::RECIPES_PATH))?;
    if let Some(note) = &loaded.note {
        eprintln!("{note}");
    }
    Ok(loaded.book)
}

/// Restores saved quest progress. An unreadable or invalid save is reported and left alone:
/// the game starts with a fresh log and does not save over the old file this session.
fn restore_quests(content: &Content) -> (QuestLog, QuestSave, String) {
    let path = std::env::var_os(QUEST_SAVE_ENV).map_or_else(|| PathBuf::from(QUEST_SAVE_PATH), PathBuf::from);
    match quest_store::load(&path, &content.quests) {
        Ok(Some(log)) => {
            let message = if log.started().next().is_some() { "Welcome back. Press J to read your journal." } else { "" };
            let save = QuestSave { path, enabled: true, saved: log.clone() };
            (log, save, message.to_string())
        }
        Ok(None) => (QuestLog::new(), QuestSave { path, enabled: true, saved: QuestLog::new() }, String::new()),
        Err(error) => {
            eprintln!("warning: {error}; starting with a fresh quest log and not saving this session");
            let message = "Your quest save could not be read, so progress will not be saved this session. See the console.";
            (QuestLog::new(), QuestSave { path, enabled: false, saved: QuestLog::new() }, message.to_string())
        }
    }
}

fn main() {
    let level = load_level().unwrap_or_else(|message| {
        eprintln!("error: {message}");
        std::process::exit(1);
    });
    let content = load_content().unwrap_or_else(|message| {
        eprintln!("error: {message}");
        std::process::exit(1);
    });
    let recipes = load_recipes().unwrap_or_else(|message| {
        eprintln!("error: {message}");
        std::process::exit(1);
    });
    // Refuse recipes or dialogue that ask for a skill level nothing in the game can teach.
    skills::check_gates(&recipes, &content.dialogue).unwrap_or_else(|message| {
        eprintln!("error: {message}");
        std::process::exit(1);
    });
    let (log, quest_save, welcome) = restore_quests(&content);
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
        .insert_resource(GameProgress { log, journal: welcome, ..default() })
        .insert_resource(GameContent(content))
        .insert_resource(Recipes(recipes))
        .insert_resource(PackState::default())
        .insert_resource(quest_save)
        .insert_resource(TalkState::default())
        .insert_resource(HoverText::default())
        .insert_resource(ContextMenuState::default())
        .insert_resource(Meadow(level.map.clone()))
        .insert_resource(LevelData(level))
        .add_systems(Startup, setup::setup)
        .add_systems(
            Update,
            (
                pack_ui::pack_input,
                pointer::pointer,
                menu::menu_input,
                dialogue_ui::dialogue_input,
                movement::follow_route,
                interact::interact,
                interact::respawn_nodes,
                persist::persist_quests,
                animation::animate_limbs,
                animation::float_props,
                pointer::fade_markers,
                camera_rig::update_camera,
                menu::sync_menu_ui,
                dialogue_ui::sync_dialogue_ui,
                pack_ui::sync_pack_ui,
                hud::update_hud,
                hud::update_journal_panel,
                exit_on_escape,
            )
                .chain(),
        )
        .run();
}

/// Esc closes an open conversation or the crafting panel; with neither open it quits.
fn exit_on_escape(
    keys: Res<ButtonInput<KeyCode>>,
    mut talk: ResMut<TalkState>,
    mut pack: ResMut<PackState>,
    mut exit: EventWriter<AppExit>,
) {
    if keys.just_pressed(KeyCode::Escape) {
        if talk.0.is_some() {
            talk.0 = None;
        } else if pack.0 {
            pack.0 = false;
        } else {
            exit.send(AppExit::Success);
        }
    }
}
