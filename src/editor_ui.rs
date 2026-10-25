//! The world editor in the game window: `/edit` turns it on, a ribbon bar appears along the
//! top, and the mouse selects, moves, removes, and places the things that stand in the open
//! world. Every rule (what may go where, undo and redo, what is kept between sessions) lives in
//! the tested `ozbound::editor`; this module only reads the mouse and keys, draws the ribbon and
//! the selection marker, and rebuilds the loaded scenes after an edit.
//!
//! While the editor is on, clicking the world does not walk the player or start a
//! conversation (the keyboard still walks, so you can go and edit another scene), and villagers
//! stand at the tile the level gives them rather than wandering off it.

use std::path::{Path, PathBuf};

use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
use bevy::ui::FocusPolicy;
use bevy::window::PrimaryWindow;
use ozbound::asset_file::{self, Library};
use ozbound::chat::Kind;
use ozbound::context_menu::{build_edit_menu, menu_metrics, place_menu, row_at, MenuAction};
use ozbound::text_fit::logical_size;
use ozbound::editor::{self, Button as EditorButton, Edit, EditorState, GroundClick, History, Levels, Loaded, ObjectRef, Tool};
use ozbound::level::Level;
use ozbound::picking::{ground_to_tile, pick_over, ray_tall_tile_hit, ray_terrain_hit, Pick, PickBox};
use ozbound::village::{self, World as Bluebrook};
use ozbound::world::TilePos;

use crate::components::*;
use crate::pointer::marker_position;
use crate::streaming::{stash, Pointing};

/// Set to keep the edit layer somewhere else.
const EDITS_ENV: &str = "OZBOUND_EDITS";
/// Set to read the asset library from another directory.
const LIBRARY_ENV: &str = "OZBOUND_LIBRARY";
const RIBBON_HEIGHT: f32 = 44.0;
const BUTTON_HEIGHT: f32 = 30.0;

/// What a right-click in the editor was opened on, kept until a row is chosen.
#[derive(Debug, Clone)]
pub struct MenuTarget {
    /// The editable object under the cursor: its scene, which object, and what to call it.
    pub object: Option<(String, ObjectRef, String)>,
    /// The tile under the cursor, as the scene it is in and the tile in that scene.
    pub ground: Option<(String, TilePos)>,
}

/// Everything the editor keeps between frames.
#[derive(Resource)]
pub struct EditorSession {
    pub state: EditorState,
    pub history: History,
    /// `None` when the asset library could not be read (the editor then stays off).
    pub library: Option<Library>,
    /// Why the editor is unavailable, if it is.
    pub problem: Option<String>,
    /// The world as loaded, for the whole-world check that every edit must pass.
    pub source: Bluebrook,
    pub layer_path: PathBuf,
    /// False when an edit file exists but could not be used: it is then left untouched.
    pub saving: bool,
    /// The loaded scenes must be rebuilt from the edited levels.
    pub rebuild: bool,
    pub menu: Option<MenuTarget>,
    pub library_dir: PathBuf,
}

/// Run condition: the editor is on.
pub fn editing(session: Res<EditorSession>) -> bool {
    session.state.active
}

/// Run condition: the editor is off (the gameplay mouse and Tab handlers use this).
pub fn not_editing(session: Res<EditorSession>) -> bool {
    !session.state.active
}

/// The whole-world check of an edited level: the world with every region's current level and
/// this one replaced must still pass `village::check`.
fn world_check(source: &Bluebrook, levels: &Levels, id: &str, level: &Level) -> Result<(), String> {
    let mut world = source.clone();
    world.set_levels(levels);
    world.check_with(id, level)
}

/// Reads the asset library and the saved edits, applies the edits to `world`'s levels (so the
/// save restore, the atlas, and the first scene all see the edited world), and returns the
/// session with notes for the console and the chat. Nothing here stops the game from starting:
/// a bad file is reported and left alone.
pub fn start_session(world: &mut Bluebrook) -> (EditorSession, Vec<String>) {
    let mut notes = Vec::new();
    let library_dir = std::env::var_os(LIBRARY_ENV).map_or_else(|| PathBuf::from(asset_file::LIBRARY_DIR), PathBuf::from);
    let (library, problem) = match Library::load(&library_dir) {
        Ok((library, note)) => {
            notes.extend(note);
            (Some(library), None)
        }
        Err(error) => {
            notes.push(format!("The asset library could not be read, so the editor is off: {error}"));
            (None, Some(error))
        }
    };
    let layer_path = std::env::var_os(EDITS_ENV).map_or_else(|| PathBuf::from(editor::LAYER_PATH), PathBuf::from);
    let source = world.clone();
    let mut history = History::default();
    let mut saving = true;
    if let Some(library) = &library {
        match editor::load_layer(&layer_path) {
            Loaded::Absent => {}
            Loaded::Unusable(why) => {
                saving = false;
                notes.push(format!("Your world edits were not applied, and the file is left as it is: {why}"));
            }
            Loaded::Layer(layer) => {
                let mut levels = world.all_levels();
                // Each edit is judged against the shipped world; the finished world is then
                // checked as a whole, so edits in different scenes cannot add up to a broken one.
                let check = |id: &str, level: &Level| source.check_with(id, level);
                let outcome = editor::replay(&mut levels, &layer, library, &check).and_then(|replayed| {
                    let mut edited = source.clone();
                    edited.set_levels(&levels);
                    village::check(&edited).map(|()| replayed).map_err(|why| format!("the edited world as a whole is not valid: {why}"))
                });
                match outcome {
                    Ok(replayed) => {
                        world.set_levels(&levels);
                        if !layer.edits.is_empty() {
                            notes.push(format!("Applied {} saved world edit(s) from {}.", layer.edits.len(), layer_path.display()));
                        }
                        history = replayed;
                    }
                    Err(why) => {
                        saving = false;
                        notes.push(format!("Your world edits were not applied, and {} is left as it is: {why}", layer_path.display()));
                    }
                }
            }
        }
    }
    let session = EditorSession { state: EditorState::default(), history, library, problem, source, layer_path, saving, rebuild: false, menu: None, library_dir };
    (session, notes)
}

// ---------------------------------------------------------------------------------------------
// The ribbon
// ---------------------------------------------------------------------------------------------

#[derive(Component)]
pub struct RibbonRoot;

#[derive(Component)]
pub struct RibbonButton(pub EditorButton);

#[derive(Component)]
pub struct RibbonHint;

/// The outline that shows which object is selected.
#[derive(Component)]
pub struct SelectionMarker;

pub fn spawn_ribbon(mut commands: Commands) {
    commands
        .spawn((
            Node {
                position_type: PositionType::Absolute,
                top: Val::Px(0.0),
                left: Val::Px(0.0),
                width: Val::Percent(100.0),
                height: Val::Px(RIBBON_HEIGHT),
                align_items: AlignItems::Center,
                column_gap: Val::Px(6.0),
                padding: UiRect::horizontal(Val::Px(10.0)),
                overflow: Overflow::clip(),
                display: Display::None,
                ..default()
            },
            BackgroundColor(Color::srgb(0.07, 0.07, 0.12)),
            GlobalZIndex(15),
            Interaction::default(),
            FocusPolicy::Block,
            UiBlocker,
            RibbonRoot,
        ))
        .with_children(|bar| {
            for button in EditorButton::ALL {
                bar.spawn((
                    Button,
                    Node {
                        height: Val::Px(BUTTON_HEIGHT),
                        padding: UiRect::axes(Val::Px(10.0), Val::Px(0.0)),
                        align_items: AlignItems::Center,
                        justify_content: JustifyContent::Center,
                        ..default()
                    },
                    BackgroundColor(Color::srgba(0.05, 0.05, 0.08, 0.78)),
                    RibbonButton(button),
                    UiBlocker,
                ))
                .with_children(|b| {
                    b.spawn((Text::new(""), TextFont { font_size: 15.0, ..default() }, TextLayout::new_with_no_wrap(), TextColor(Color::WHITE)));
                });
            }
            bar.spawn((
                Text::new(""),
                TextFont { font_size: 14.0, ..default() },
                TextLayout::new_with_no_wrap(),
                TextColor(Color::srgb(0.85, 0.85, 0.7)),
                Node { margin: UiRect::left(Val::Px(14.0)), ..default() },
                RibbonHint,
            ));
        });
}

fn place_label(session: &EditorSession) -> Option<String> {
    let library = session.library.as_ref()?;
    let placeable = library.placeable();
    placeable.get(session.state.place_index % placeable.len().max(1)).map(|a| a.name.clone())
}

/// Shows or hides the ribbon and keeps its buttons and hint in step with the editor.
pub fn sync_ribbon(
    session: Res<EditorSession>,
    atlas: Res<Atlas>,
    mut root: Query<&mut Node, (With<RibbonRoot>, Without<RibbonButton>)>,
    mut buttons: Query<(&RibbonButton, &Interaction, &mut BackgroundColor, &Children)>,
    mut texts: Query<(&mut Text, &mut TextColor), Without<RibbonHint>>,
    mut hint: Query<&mut Text, With<RibbonHint>>,
) {
    if let Ok(mut node) = root.get_single_mut() {
        let wanted = if session.state.active { Display::Flex } else { Display::None };
        if node.display != wanted {
            node.display = wanted;
        }
    }
    if !session.state.active {
        return;
    }
    let label = place_label(&session);
    let items = editor::ribbon(&session.state, &session.history, label.as_deref());
    for (button, interaction, mut background, children) in &mut buttons {
        let Some(item) = items.iter().find(|i| i.button == button.0) else { continue };
        let color = match (item.enabled, item.active, *interaction) {
            (false, _, _) => Color::srgba(0.05, 0.05, 0.08, 0.5),
            (true, true, _) => Color::srgba(0.25, 0.32, 0.5, 0.92),
            (true, false, Interaction::Hovered | Interaction::Pressed) => Color::srgba(0.18, 0.18, 0.26, 0.9),
            (true, false, Interaction::None) => Color::srgba(0.05, 0.05, 0.08, 0.78),
        };
        if background.0 != color {
            background.0 = color;
        }
        for child in children.iter() {
            if let Ok((mut text, mut text_color)) = texts.get_mut(*child) {
                if text.0 != item.label {
                    text.0 = item.label.clone();
                }
                text_color.0 = if item.enabled { Color::WHITE } else { Color::srgba(1.0, 1.0, 1.0, 0.4) };
            }
        }
    }
    if let Ok(mut text) = hint.get_single_mut() {
        let line = match &session.state.selection {
            Some(selection) => {
                let found = atlas.level(&selection.region).and_then(|level| editor::find(level, &selection.object));
                match found {
                    Some(object) => {
                        let detail = object.to_asset(None).map_or_else(|| "a readable sign".to_string(), |asset| asset.describe());
                        format!("Selected: {detail} at ({}, {})", object.tile().x, object.tile().y)
                    }
                    None => "Nothing selected".to_string(),
                }
            }
            None => "Right-click for more. Ctrl+Z / Ctrl+Y, Delete.".to_string(),
        };
        if text.0 != line {
            text.0 = line;
        }
    }
}

// ---------------------------------------------------------------------------------------------
// Changing the world
// ---------------------------------------------------------------------------------------------

/// What the editor needs to remember about the scenes that are loaded right now before an edit
/// replaces their levels (see `streaming::stash`).
#[derive(SystemParam)]
pub struct WorldState<'w, 's> {
    streaming: Res<'w, Streaming>,
    memory: ResMut<'w, ChunkMemory>,
    petals: Query<'w, 's, &'static Interactable, With<Moonpetal>>,
    nodes: Query<'w, 's, (&'static ResourceNode, &'static Interactable)>,
}

impl WorldState<'_, '_> {
    /// Stores what the player has picked and used in every loaded scene, so a rebuilt scene
    /// shows the same world. Must be called *before* the levels change: it compares the
    /// entities with the level they were built from.
    fn stash_loaded(&mut self, atlas: &Atlas) {
        let Some(overworld) = atlas.overworld.as_ref() else { return };
        let petals: Vec<TilePos> = self.petals.iter().map(|p| p.tile).collect();
        let nodes: Vec<(TilePos, u32)> = self.nodes.iter().map(|(n, at)| (at.tile, n.0.remaining_actions())).collect();
        let loaded = self.streaming.loaded.clone();
        stash(&mut self.memory, overworld, atlas, &loaded, &petals, &nodes);
    }
}

fn say(chat: &mut ChatState, text: impl Into<String>) {
    chat.log.push(Kind::System, text);
}

fn save_layer(session: &EditorSession, chat: &mut ChatState) {
    if !session.saving {
        return;
    }
    if let Err(error) = editor::save_layer(&session.layer_path, &session.history.layer()) {
        say(chat, format!("Your edit could not be saved: {error}"));
    }
}

/// Which way an edit is made.
enum Change {
    Do(Edit),
    Undo,
    Redo,
}

/// Makes an edit, undoes, or redoes, and tells the player what happened.
fn change(session: &mut EditorSession, atlas: &mut Atlas, world: &mut WorldState, chat: &mut ChatState, how: Change) {
    let EditorSession { history, library, source, state, .. } = &mut *session;
    let Some(library) = library.as_ref() else { return };
    // The world as it stands: the check judges the edit against every other scene's edits.
    let snapshot = atlas.levels.clone();
    let check = |id: &str, level: &Level| world_check(source, &snapshot, id, level);
    world.stash_loaded(atlas);
    let result = match &how {
        Change::Do(edit) => history.apply(&mut atlas.levels, edit.clone(), library, &check),
        Change::Undo => history.undo(&mut atlas.levels, library, &check),
        Change::Redo => history.redo(&mut atlas.levels, library, &check),
    };
    match result {
        Ok(message) => {
            if let Change::Do(edit) = &how {
                state.follow(edit);
            }
            state.prune(&atlas.levels);
            say(chat, message);
            session.rebuild = true;
            save_layer(session, chat);
        }
        Err(error) => say(chat, error.to_string()),
    }
}

/// Handles `/edit`: turns the editor on or off, with a line saying what happened.
pub fn toggle_editor(
    mut session: ResMut<EditorSession>,
    mut chat: ResMut<ChatState>,
    mut menu: ResMut<ContextMenuState>,
    atlas: Res<Atlas>,
    streaming: Res<Streaming>,
    mut world: WorldState,
) {
    if !chat.edit_requested {
        return;
    }
    chat.edit_requested = false;
    if session.state.active {
        session.state.toggle();
        session.menu = None;
        menu.0 = None;
        say(&mut chat, "World editor off.");
        return;
    }
    if let Some(problem) = &session.problem {
        say(&mut chat, format!("The editor is off: {problem}"));
        return;
    }
    if streaming.interior || atlas.overworld.is_none() {
        say(&mut chat, "The editor works on the open world. Leave this place first.");
        return;
    }
    // Villagers go back to the tiles the level gives them, so what you click is what you edit.
    world.stash_loaded(&atlas);
    session.state.toggle();
    session.rebuild = true;
    menu.0 = None;
    say(&mut chat, "World editor on. Click an object to select it, then click ground to move it. /edit leaves.");
    if !session.saving {
        say(&mut chat, "Your saved edits could not be used, so edits made now are not saved. See the console.");
    }
}

/// Rebuilds the loaded scenes from the (edited) levels: removes them and lets the streamer
/// build them again. Runs before `stream_chunks` in the frame.
pub fn rebuild_scenes(
    mut commands: Commands,
    mut session: ResMut<EditorSession>,
    mut streaming: ResMut<Streaming>,
    mut pointing: Pointing,
    chunks: Query<Entity, With<ChunkEntity>>,
) {
    if !session.rebuild {
        return;
    }
    session.rebuild = false;
    for entity in &chunks {
        commands.entity(entity).despawn_recursive();
    }
    streaming.loaded.clear();
    pointing.clear();
}

// ---------------------------------------------------------------------------------------------
// Mouse and keys
// ---------------------------------------------------------------------------------------------

/// The scene and scene tile of a world tile, if a scene covers it.
fn scene_tile(atlas: &Atlas, world: TilePos) -> Option<(String, TilePos)> {
    let (chunk, local) = atlas.overworld.as_ref()?.to_local(world)?;
    Some((chunk.id.clone(), local))
}

/// What the cursor is over: the editable object (scene, reference, name) and the ground tile.
struct Under {
    object: Option<(String, ObjectRef, String, TilePos)>,
    ground: Option<TilePos>,
}

/// How tall a thicket stands above the ground, for clicking it (the tallest mound of its model).
const THICKET_TOP: f32 = 1.3;

fn look(
    window: &Window,
    camera: &(&Camera, &GlobalTransform),
    atlas: &Atlas,
    meadow: &Meadow,
    heights: &GroundHeights,
    targets: &Query<(Entity, &Interactable)>,
) -> Option<Under> {
    let cursor = window.cursor_position()?;
    let ray = camera.0.viewport_to_world(camera.1, cursor).ok()?;
    let (origin, dir) = (ray.origin.to_array(), ray.direction.as_vec3().to_array());
    let boxes = targets.iter().map(|(entity, it)| PickBox { id: entity, min: it.pick_min, max: it.pick_max });
    let terrain = ray_terrain_hit(origin, dir, &|x, z| heights.0.height_at_world(x, z));
    let picked = pick_over(origin, dir, terrain, boxes);
    let object_distance = if let Pick::Object { distance, .. } = picked { distance } else { f32::INFINITY };
    // Thickets are scenery without a click box, so the ray is walked through the tiles instead:
    // the first thicket it passes through counts, unless something nearer was hit.
    let thicket = ray_tall_tile_hit(
        origin,
        dir,
        &|x, z| heights.0.height_at_world(x, z),
        &|x, z| ground_to_tile(&meadow.0, x, z),
        &|tile| scene_tile(atlas, tile).and_then(|(region, local)| atlas.level(&region).map(|level| editor::is_thicket(level, local))).unwrap_or(false),
        THICKET_TOP,
    )
    .filter(|(_, distance)| *distance < object_distance);
    let object = if let Some((tile, _)) = thicket {
        scene_tile(atlas, tile).map(|(region, local)| (region, ObjectRef::Thicket(local), "thicket".to_string(), tile))
    } else {
        match picked {
            Pick::Object { id, .. } => targets.get(id).ok().and_then(|(_, it)| {
                let (region, local) = scene_tile(atlas, it.tile)?;
                let found = editor::object_at(atlas.level(&region)?, local)?;
                Some((region, found.reference(), found.name(), it.tile))
            }),
            _ => None,
        }
    };
    let ground = terrain.and_then(|hit| ground_to_tile(&meadow.0, hit.x, hit.z));
    Some(Under { object, ground })
}

/// Clicks, the right-click menu, and the keyboard shortcuts of the editor.
pub fn editor_input(
    windows: Query<&Window, With<PrimaryWindow>>,
    cameras: Query<(&Camera, &GlobalTransform), With<MainCamera>>,
    buttons: Res<ButtonInput<MouseButton>>,
    keys: Res<ButtonInput<KeyCode>>,
    meadow: Res<Meadow>,
    heights: Res<GroundHeights>,
    targets: Query<(Entity, &Interactable)>,
    players: Query<&Player>,
    mut atlas: ResMut<Atlas>,
    mut session: ResMut<EditorSession>,
    mut menu: ResMut<ContextMenuState>,
    mut chat: ResMut<ChatState>,
    blockers: Query<&Interaction, With<UiBlocker>>,
    ui_scale: Res<UiScale>,
    mut world: WorldState,
) {
    let on_ui = blockers.iter().any(|i| *i != Interaction::None);
    // Keyboard: not while the chat box has the keys.
    if !chat.keys_captured {
        let control = keys.any_pressed([KeyCode::ControlLeft, KeyCode::ControlRight, KeyCode::SuperLeft, KeyCode::SuperRight]);
        let shift = keys.any_pressed([KeyCode::ShiftLeft, KeyCode::ShiftRight]);
        if control && keys.just_pressed(KeyCode::KeyZ) {
            change(&mut session, &mut atlas, &mut world, &mut chat, if shift { Change::Redo } else { Change::Undo });
        } else if control && keys.just_pressed(KeyCode::KeyY) {
            change(&mut session, &mut atlas, &mut world, &mut chat, Change::Redo);
        } else if keys.just_pressed(KeyCode::Delete) {
            if let Some(selection) = session.state.selection.clone() {
                change(&mut session, &mut atlas, &mut world, &mut chat, Change::Do(Edit::Remove { region: selection.region, object: selection.object }));
            }
        }
    }
    let left = buttons.just_pressed(MouseButton::Left);
    let right = buttons.just_pressed(MouseButton::Right);
    if !left && !right {
        return;
    }
    let (Ok(window), Ok(camera)) = (windows.get_single(), cameras.get_single()) else { return };
    let player_tile = players.get_single().map(|p| p.tile).ok();
    // Menu geometry is in the interface's own pixels, which the text-size setting scales.
    let scale = if ui_scale.0.is_finite() && ui_scale.0 > 0.0 { ui_scale.0 } else { 1.0 };

    // An open menu owns the next click: a left-click picks a row, any other closes it.
    if let Some(open) = menu.0.take() {
        if on_ui && !left {
            return;
        }
        if left {
            let Some(cursor) = window.cursor_position() else { return };
            let cursor = cursor / scale;
            let Some(row) = row_at((open.origin.x, open.origin.y), open.metrics, open.entries.len(), (cursor.x, cursor.y)) else { return };
            let target = session.menu.take();
            let Some(target) = target else { return };
            let action = open.entries[row].action;
            run_menu_action(action, &target, player_tile, &mut session, &mut atlas, &mut world, &mut chat);
            return;
        }
        session.menu = None;
        // A right-click while open falls through and opens a new menu.
        if !right {
            return;
        }
    }
    if on_ui {
        return;
    }
    let Some(under) = look(window, &(camera.0, camera.1), &atlas, &meadow, &heights, &targets) else { return };
    let ground = under.ground.and_then(|tile| scene_tile(&atlas, tile).map(|(region, local)| (region, local, tile)));

    if left {
        if let Some((region, object, _, _)) = under.object {
            session.state.select(&region, object);
            return;
        }
        let Some((region, local, _)) = ground else { return };
        let placeable: Vec<_> = session.library.as_ref().map(|l| l.placeable()).unwrap_or_default();
        match session.state.ground_click(&placeable) {
            GroundClick::Nothing => session.state.clear_selection(),
            GroundClick::MoveSelection(selection) => {
                if selection.region != region {
                    say(&mut chat, "An object stays in its own scene. Pick a tile in the same scene.");
                } else if player_tile.is_some_and(|p| scene_tile(&atlas, p) == Some((region.clone(), local))) {
                    say(&mut chat, "You are standing there.");
                } else {
                    change(&mut session, &mut atlas, &mut world, &mut chat, Change::Do(Edit::Move { region, object: selection.object, to: local }));
                }
            }
            GroundClick::PlaceAsset(asset) => {
                if player_tile.is_some_and(|p| scene_tile(&atlas, p) == Some((region.clone(), local))) {
                    say(&mut chat, "You are standing there.");
                } else {
                    change(&mut session, &mut atlas, &mut world, &mut chat, Change::Do(Edit::Place { region, asset, tile: local }));
                }
            }
        }
        return;
    }

    // Right-click: the extended menu.
    let Some(cursor) = window.cursor_position() else { return };
    let selected_name = session.state.selection.as_ref().and_then(|s| atlas.level(&s.region).and_then(|l| editor::find(l, &s.object))).map(|o| o.name());
    let asset = place_label(&session).filter(|_| session.state.tool == Tool::Place);
    let entries = build_edit_menu(under.object.as_ref().map(|(_, _, name, _)| name.as_str()), selected_name.as_deref(), ground.is_some(), asset.as_deref());
    let logical = logical_size(window.width(), window.height(), scale);
    let metrics = menu_metrics(&entries, logical.0);
    let at = place_menu((cursor.x / scale, cursor.y / scale), metrics.size(entries.len()), logical);
    session.menu = Some(MenuTarget {
        object: under.object.map(|(region, object, name, _)| (region, object, name)),
        ground: ground.map(|(region, local, _)| (region, local)),
    });
    menu.0 = Some(OpenMenu { origin: Vec2::new(at.0, at.1), metrics, entries, target: None, ground: None });
}

fn run_menu_action(
    action: MenuAction,
    target: &MenuTarget,
    player_tile: Option<TilePos>,
    session: &mut EditorSession,
    atlas: &mut Atlas,
    world: &mut WorldState,
    chat: &mut ChatState,
) {
    match action {
        MenuAction::Cancel | MenuAction::Interact | MenuAction::Examine | MenuAction::WalkHere => {}
        MenuAction::Select => {
            if let Some((region, object, _)) = &target.object {
                session.state.select(region, object.clone());
            }
        }
        MenuAction::Remove => {
            if let Some((region, object, _)) = &target.object {
                change(session, atlas, world, chat, Change::Do(Edit::Remove { region: region.clone(), object: object.clone() }));
            }
        }
        MenuAction::MoveHere => {
            let (Some(selection), Some((region, local))) = (session.state.selection.clone(), target.ground.clone()) else { return };
            if selection.region != region {
                say(chat, "An object stays in its own scene. Pick a tile in the same scene.");
            } else if player_tile.is_some_and(|p| scene_tile(atlas, p) == Some((region.clone(), local))) {
                say(chat, "You are standing there.");
            } else {
                change(session, atlas, world, chat, Change::Do(Edit::Move { region, object: selection.object, to: local }));
            }
        }
        MenuAction::PlaceHere => {
            let (Some((region, local)), Some(library)) = (target.ground.clone(), session.library.as_ref()) else { return };
            let placeable = library.placeable();
            let Some(asset) = placeable.get(session.state.place_index % placeable.len().max(1)).map(|a| a.id.clone()) else { return };
            if player_tile.is_some_and(|p| scene_tile(atlas, p) == Some((region.clone(), local))) {
                say(chat, "You are standing there.");
            } else {
                change(session, atlas, world, chat, Change::Do(Edit::Place { region, asset, tile: local }));
            }
        }
    }
}

/// Clicks on the ribbon's buttons.
pub fn ribbon_input(
    buttons: Query<(&Interaction, &RibbonButton), Changed<Interaction>>,
    model: ModelSource,
    mut session: ResMut<EditorSession>,
    mut atlas: ResMut<Atlas>,
    mut chat: ResMut<ChatState>,
    mut menu: ResMut<ContextMenuState>,
    wildlife: Res<Wildlife>,
    mut world: WorldState,
) {
    for (interaction, button) in &buttons {
        if *interaction != Interaction::Pressed || !session.state.active {
            continue;
        }
        match button.0 {
            EditorButton::Select => session.state.choose(Tool::Select),
            EditorButton::SelectMove => session.state.choose(Tool::SelectMove),
            EditorButton::Place => {
                let count = session.library.as_ref().map_or(0, |l| l.placeable().len());
                session.state.next_asset(count);
            }
            EditorButton::Undo if session.history.can_undo() => change(&mut session, &mut atlas, &mut world, &mut chat, Change::Undo),
            EditorButton::Redo if session.history.can_redo() => change(&mut session, &mut atlas, &mut world, &mut chat, Change::Redo),
            EditorButton::Undo | EditorButton::Redo => {}
            EditorButton::Export => export_selected(&mut session, &atlas, &wildlife, &model, &mut chat),
            EditorButton::Exit => {
                session.state.toggle();
                session.menu = None;
                menu.0 = None;
                say(&mut chat, "World editor off.");
            }
        }
    }
}

/// What it takes to read the triangles of an object that is standing in the world.
#[derive(bevy::ecs::system::SystemParam)]
pub struct ModelSource<'w, 's> {
    targets: Query<'w, 's, (Entity, &'static Interactable)>,
    children: Query<'w, 's, &'static Children>,
    parts: Query<'w, 's, (&'static Mesh3d, Option<&'static MeshMaterial3d<StandardMaterial>>, &'static GlobalTransform)>,
    roots: Query<'w, 's, &'static GlobalTransform>,
    meshes: Res<'w, Assets<Mesh>>,
    materials: Res<'w, Assets<StandardMaterial>>,
}

impl ModelSource<'_, '_> {
    /// The meshes of the object standing on world tile `tile`, in the object's own frame (as it
    /// was built: unturned, scale one, origin at its foot), or `None` when no such object is in
    /// the world right now.
    fn parts_at(&self, tile: TilePos) -> Option<Vec<ozbound::obj::Part>> {
        let (root, _) = self.targets.iter().find(|(_, it)| it.tile == tile)?;
        let to_model = self.roots.get(root).ok()?.compute_matrix().inverse();
        let mut stack = vec![root];
        let mut out = Vec::new();
        while let Some(entity) = stack.pop() {
            if let Ok((mesh, material, global)) = self.parts.get(entity) {
                if let Some(part) = self.meshes.get(&mesh.0).and_then(|m| mesh_part(m, to_model * global.compute_matrix(), material.and_then(|m| self.materials.get(&m.0)))) {
                    out.push(part);
                }
            }
            if let Ok(kids) = self.children.get(entity) {
                // Reverse so the first child is read first: the export order matches the model's.
                stack.extend(kids.iter().rev());
            }
        }
        (!out.is_empty()).then_some(out)
    }
}

/// One Bevy mesh as an export part (triangle lists only; anything else is skipped).
fn mesh_part(mesh: &Mesh, transform: Mat4, material: Option<&StandardMaterial>) -> Option<ozbound::obj::Part> {
    use bevy::render::mesh::{Indices, PrimitiveTopology, VertexAttributeValues};
    if mesh.primitive_topology() != PrimitiveTopology::TriangleList {
        return None;
    }
    let float3 = |attribute| match mesh.attribute(attribute) {
        Some(VertexAttributeValues::Float32x3(values)) => Some(values.clone()),
        _ => None,
    };
    let positions = float3(Mesh::ATTRIBUTE_POSITION)?;
    let normals = float3(Mesh::ATTRIBUTE_NORMAL).filter(|n| n.len() == positions.len()).unwrap_or_default();
    let indices: Vec<u32> = match mesh.indices() {
        Some(Indices::U32(list)) => list.clone(),
        Some(Indices::U16(list)) => list.iter().map(|i| u32::from(*i)).collect(),
        None => (0..positions.len() as u32).collect(),
    };
    let color = material.map_or([0.8, 0.8, 0.8, 1.0], |m| {
        let c = m.base_color.to_srgba();
        [c.red, c.green, c.blue, c.alpha]
    });
    Some(ozbound::obj::Part { positions, normals, indices, transform: transform.to_cols_array(), color })
}

fn export_selected(session: &mut EditorSession, atlas: &Atlas, wildlife: &Wildlife, model: &ModelSource, chat: &mut ChatState) {
    let Some(selection) = session.state.selection.clone() else { return };
    let Some(object) = atlas.level(&selection.region).and_then(|level| editor::find(level, &selection.object)) else { return };
    let creature_name = match &selection.object {
        ObjectRef::Creature(id) => wildlife.bestiary.get(id).map(|c| c.name.clone()),
        _ => None,
    };
    let Some(asset) = object.to_asset(creature_name.as_deref()) else {
        say(chat, format!("{} carries its own text, so it is not exported as an asset.", object.name()));
        return;
    };
    match asset_file::save_asset(Path::new(&session.library_dir), &asset) {
        Ok(path) => {
            say(chat, format!("Exported {} to {}.", asset.name, path.display()));
            if let Some(library) = session.library.as_mut() {
                library.upsert(asset.clone());
            }
        }
        Err(error) => {
            say(chat, error);
            return;
        }
    }
    // The geometry goes beside the asset file as <id>.obj and <id>.mtl.
    let world_tile = atlas.overworld.as_ref().and_then(|o| o.to_world(&selection.region, object.tile()));
    let Some(parts) = world_tile.and_then(|tile| model.parts_at(tile)) else {
        say(chat, format!("The model of {} is not in the world right now, so only the asset file was written.", asset.name));
        return;
    };
    match ozbound::obj::build_obj(&asset.id, &parts).and_then(|file| ozbound::obj::save_obj(Path::new(&session.library_dir), &asset.id, &file).map(|path| (path, file))) {
        Ok((path, file)) => say(chat, format!("Exported the geometry of {} to {} ({} triangles; colours in the .mtl beside it).", asset.name, path.display(), file.triangles)),
        Err(error) => say(chat, format!("The asset file was written, but not its geometry: {error}")),
    }
}

// ---------------------------------------------------------------------------------------------
// The selection marker
// ---------------------------------------------------------------------------------------------

/// Keeps one highlight on the selected object's tile (and none when nothing is selected or its
/// scene is not loaded).
pub fn sync_selection(
    mut commands: Commands,
    session: Res<EditorSession>,
    atlas: Res<Atlas>,
    meadow: Res<Meadow>,
    heights: Res<GroundHeights>,
    streaming: Res<Streaming>,
    assets: Res<ClickAssets>,
    mut markers: Query<(Entity, &mut Transform), With<SelectionMarker>>,
) {
    let wanted = session.state.selection.as_ref().filter(|_| session.state.active).and_then(|selection| {
        let level = atlas.level(&selection.region)?;
        let tile = editor::find(level, &selection.object)?.tile();
        if !streaming.loaded.contains(&selection.region) {
            return None;
        }
        let world = atlas.overworld.as_ref()?.to_world(&selection.region, tile)?;
        Some(marker_position(&meadow.0, &heights.0, world))
    });
    match (wanted, markers.get_single_mut()) {
        (Some(at), Ok((_, mut transform))) => {
            if transform.translation != at {
                transform.translation = at;
            }
        }
        (Some(at), Err(_)) => {
            commands
                .spawn((Transform::from_translation(at).with_scale(Vec3::splat(1.15)), Visibility::default(), SelectionMarker))
                .with_children(|marker| {
                    for angle in [std::f32::consts::FRAC_PI_4, -std::f32::consts::FRAC_PI_4] {
                        marker.spawn((Mesh3d(assets.bar.clone()), MeshMaterial3d(assets.interact.clone()), Transform::from_rotation(Quat::from_rotation_y(angle))));
                    }
                });
        }
        (None, found) => {
            if let Ok((entity, _)) = found {
                commands.entity(entity).despawn_recursive();
            }
        }
    }
}
