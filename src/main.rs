//! Ozbound: a low-poly 3D, click-to-move fairytale RPG. Game rules live in the `ozbound`
//! library crate (pure Rust, unit-tested); these modules only render, read input, and glue.

// Bevy systems take one parameter per resource/query they use, so long signatures are normal.
#![allow(clippy::too_many_arguments)]

mod animation;
mod camera_rig;
mod chat_ui;
mod components;
mod dialogue_ui;
mod hud;
mod interact;
mod keyboard;
mod menu;
mod models;
mod movement;
mod pack_ui;
mod pause_ui;
mod persist;
mod pointer;
mod settings_ui;
mod setup;

use std::path::PathBuf;

use bevy::prelude::*;
use ozbound::{
    save::{SaveContext, SaveGame},
    save_store::{self, Loaded, Start},
    settings::Settings,
    settings_store, skills,
    text_fit::MIN_WINDOW,
    village::{self, World as Hollowmere},
};

use components::{
    ChatState, ContextMenuState, DockState, GameContent, GameProgress, HoverText, KeyTarget, LevelData, Meadow, PackState, Pause, PerfStats, Portraits, Recipes, SaveState,
    SettingsStore, StartingSave, TalkPortrait, TalkState, UserSettings, Wildlife,
};
use pause_ui::running;

/// Set these to play other files; a missing or invalid file is then an error. Without them the
/// shipped Hollowmere files are used, or the identical built-in copies when the game is started
/// from another directory.
const MAP_ENV: &str = "OZBOUND_MAP";
const QUESTS_ENV: &str = "OZBOUND_QUESTS";
const DIALOGUE_ENV: &str = "OZBOUND_DIALOGUE";
const BESTIARY_ENV: &str = "OZBOUND_BESTIARY";
const RECIPES_ENV: &str = "OZBOUND_RECIPES";
/// Where the game is saved (override with `OZBOUND_SAVE`, for example to keep a save per map).
const SAVE_PATH: &str = "saves/game.txt";
const SAVE_ENV: &str = "OZBOUND_SAVE";
/// The quest-only save that earlier versions wrote. It is read once, when there is no full
/// save yet, and left where it is (override with `OZBOUND_QUEST_SAVE`).
const LEGACY_QUEST_SAVE_PATH: &str = "saves/quests.txt";
const LEGACY_QUEST_SAVE_ENV: &str = "OZBOUND_QUEST_SAVE";
/// Where the settings are kept (override with `OZBOUND_SETTINGS`).
const SETTINGS_ENV: &str = "OZBOUND_SETTINGS";

/// Reads the settings file. A missing file gives the defaults; an unreadable or invalid one is
/// reported, the defaults are used, and the file is left alone (settings are not written this
/// session), so the player's own edits are never overwritten.
fn load_settings() -> (UserSettings, SettingsStore, String) {
    let path = std::env::var_os(SETTINGS_ENV).map_or_else(|| PathBuf::from(settings_store::DEFAULT_PATH), PathBuf::from);
    match settings_store::load(&path) {
        Ok(settings) => (UserSettings(settings), SettingsStore { path, enabled: true, saved: settings }, String::new()),
        Err(error) => {
            eprintln!("warning: {error}; using the default settings and not saving them this session");
            let defaults = Settings::default();
            let message = "Your settings file could not be read, so the defaults are in use. See the console for details.";
            (UserSettings(defaults), SettingsStore { path, enabled: false, saved: defaults }, message.to_string())
        }
    }
}

/// Loads the level, quests, dialogue, bestiary, and recipes, and checks that they fit
/// together (see `ozbound::village`). An invalid file stops startup with its name and line.
fn load_world() -> Result<Hollowmere, String> {
    let env_path = |name: &str| std::env::var_os(name).map(PathBuf::from);
    let (map, quests, dialogue, bestiary, recipes) =
        (env_path(MAP_ENV), env_path(QUESTS_ENV), env_path(DIALOGUE_ENV), env_path(BESTIARY_ENV), env_path(RECIPES_ENV));
    let loaded = village::load(&village::Sources {
        map_custom: map.as_deref(),
        quests_custom: quests.as_deref(),
        dialogue_custom: dialogue.as_deref(),
        bestiary_custom: bestiary.as_deref(),
        recipes_custom: recipes.as_deref(),
        ..village::Sources::defaults()
    })?;
    for note in &loaded.notes {
        eprintln!("{note}");
    }
    Ok(loaded.world)
}

/// Restores the saved game (see `ozbound::save` for what it holds). An unreadable or invalid
/// save is reported and left alone: the game starts fresh and does not save over the old file
/// this session, so the player can still recover it.
fn restore_game(world: &Hollowmere) -> (SaveGame, SaveState, String) {
    let env_path = |name: &str, default: &str| std::env::var_os(name).map_or_else(|| PathBuf::from(default), PathBuf::from);
    let path = env_path(SAVE_ENV, SAVE_PATH);
    let legacy = env_path(LEGACY_QUEST_SAVE_ENV, LEGACY_QUEST_SAVE_PATH);
    let ctx = SaveContext { level: &world.level, quests: &world.content.quests, bestiary: &world.bestiary };
    match save_store::load(&path, Some(&legacy), &ctx) {
        Ok(Loaded { game, start }) => {
            if start == Start::Loaded {
                // Keep the save that just loaded, in case a later session writes something worse.
                if let Err(error) = save_store::back_up(&path) {
                    eprintln!("warning: could not keep a backup of {}: {error}", path.display());
                }
            }
            let message = match start {
                Start::NewGame => "",
                Start::Loaded => "Welcome back. Your progress was restored. Press J to open your quest book.",
                Start::MigratedQuests => "Welcome back. Your quest progress was carried over to the new save file.",
            };
            // A migrated game is not on disk in the new format yet, so it counts as unsaved.
            let saved = if start == Start::MigratedQuests { SaveGame::new_game(&world.level) } else { game.clone() };
            (game, SaveState { path, enabled: true, saved, idle: 0.0 }, message.to_string())
        }
        Err(error) => {
            eprintln!("warning: {error}; starting a new game and not saving this session");
            let message = "Your save could not be read, so progress will not be saved this session. See the console for how to recover it.";
            let game = SaveGame::new_game(&world.level);
            (game.clone(), SaveState { path, enabled: false, saved: game.clone(), idle: 0.0 }, message.to_string())
        }
    }
}

fn main() {
    let world = load_world().unwrap_or_else(|message| {
        eprintln!("error: {message}");
        std::process::exit(1);
    });
    // Refuse recipes or dialogue that ask for a skill level nothing in the game can teach.
    skills::check_gates(&world.recipes, &world.content.dialogue).unwrap_or_else(|message| {
        eprintln!("error: {message}");
        std::process::exit(1);
    });
    let (game, save_state, mut welcome) = restore_game(&world);
    let mut chat = ChatState::default();
    chat.log.push(ozbound::chat::Kind::System, "Press T or / to chat. Type /help to see the chat commands.");
    let (settings, settings_file, settings_note) = load_settings();
    if !settings_note.is_empty() {
        welcome = if welcome.is_empty() { settings_note } else { format!("{welcome} {settings_note}") };
    }
    let Hollowmere { level, content, bestiary, recipes } = world;
    App::new()
        .add_plugins(
            DefaultPlugins
                .set(WindowPlugin {
                    primary_window: Some(Window {
                        title: "Ozbound — A Fairytale RPG".into(),
                        resolution: (1100.0, 720.0).into(),
                        // Below this the panels would not fit (see `ozbound::text_fit`).
                        resize_constraints: WindowResizeConstraints { min_width: MIN_WINDOW.0, min_height: MIN_WINDOW.1, ..default() },
                        ..default()
                    }),
                    ..default()
                })
                // Portraits are 16x16 pixel art: keep them crisp instead of blurred.
                .set(ImagePlugin::default_nearest()),
        )
        .insert_resource(ClearColor(Color::srgb(0.56, 0.76, 0.92)))
        .insert_resource(GameProgress {
            log: game.quests.clone(),
            notice: welcome,
            inventory: game.bag.clone(),
            equipment: game.equipment.clone(),
            skills: game.skills.clone(),
        })
        .insert_resource(GameContent(content))
        .insert_resource(Recipes(recipes))
        .insert_resource(Wildlife { bestiary, notes: game.notes.clone() })
        .insert_resource(Portraits::default())
        .insert_resource(TalkPortrait::default())
        .insert_resource(PackState::default())
        .insert_resource(save_state)
        .insert_resource(settings)
        .insert_resource(settings_file)
        .insert_resource(Pause::default())
        .insert_resource(DockState::default())
        .insert_resource(chat)
        .insert_resource(KeyTarget::default())
        .insert_resource(PerfStats::default())
        .insert_resource(StartingSave(game))
        .insert_resource(TalkState::default())
        .insert_resource(HoverText::default())
        .insert_resource(ContextMenuState::default())
        .insert_resource(Meadow(level.map.clone()))
        .insert_resource(LevelData(level))
        .add_systems(Startup, (setup::setup, chat_ui::spawn_chat_panel))
        .add_systems(Last, (persist::save_on_exit, settings_ui::report_on_exit))
        .add_systems(
            Update,
            (
                // Gameplay input and the world: all of it stands aside while the game is paused.
                (
                    settings_ui::toggle_overlay,
                    // Chat reads the keyboard first; the systems after it stand aside while the
                    // chat box is open (see `ChatState::keys_captured`).
                    chat_ui::chat_input.run_if(running),
                    hud::dock_input.run_if(running),
                    hud::toolbar_input.run_if(running),
                    pack_ui::pack_input.run_if(running),
                    pointer::pointer.run_if(running),
                    menu::menu_input.run_if(running),
                    keyboard::keyboard_walk.run_if(running),
                    keyboard::keyboard_target.run_if(running),
                    keyboard::keyboard_hint.run_if(running),
                    dialogue_ui::dialogue_input.run_if(running),
                    movement::follow_route,
                    interact::interact,
                    interact::respawn_nodes,
                    persist::persist_game,
                    animation::tick_emotes,
                    animation::animate_limbs,
                    animation::float_props,
                    pointer::fade_markers,
                    camera_rig::update_camera.run_if(running),
                )
                    .chain(),
                // Esc and the pause menu run after the input systems, so a click that resumes the
                // game is never also read as a click on the world.
                (
                    chat_ui::collect_notices,
                    pause_ui::pause_input,
                    settings_ui::apply_settings,
                    settings_ui::record_frames,
                    menu::sync_menu_ui,
                    dialogue_ui::sync_dialogue_ui,
                    pack_ui::sync_pack_ui,
                    pause_ui::sync_pause_ui,
                    chat_ui::sync_chat_ui,
                    hud::update_hud,
                    hud::sync_toolbar,
                    hud::update_dock_panel,
                )
                    .chain(),
            )
                .chain(),
        )
        .run();
}
