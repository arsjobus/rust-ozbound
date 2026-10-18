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
    save::{SaveContext, SaveGame},
    save_store::{self, Loaded, Start},
    skills,
    village::{self, World as Hollowmere},
};

use components::{ContextMenuState, GameContent, GameProgress, HoverText, LevelData, Meadow, PackState, Portraits, Recipes, SaveState, StartingSave, TalkPortrait, TalkState, Wildlife};

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
                Start::Loaded => "Welcome back. Your progress was restored. Press J to read your journal.",
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
    let (game, save_state, welcome) = restore_game(&world);
    let Hollowmere { level, content, bestiary, recipes } = world;
    App::new()
        .add_plugins(
            DefaultPlugins
                .set(WindowPlugin {
                    primary_window: Some(Window {
                        title: "Ozbound — A Fairytale RPG".into(),
                        resolution: (1100.0, 720.0).into(),
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
            journal: welcome,
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
        .insert_resource(StartingSave(game))
        .insert_resource(TalkState::default())
        .insert_resource(HoverText::default())
        .insert_resource(ContextMenuState::default())
        .insert_resource(Meadow(level.map.clone()))
        .insert_resource(LevelData(level))
        .add_systems(Startup, setup::setup)
        .add_systems(Last, persist::save_on_exit)
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
                persist::persist_game,
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
