//! Right-click context menu: opening it on whatever is under the cursor, choosing a row,
//! and drawing the panel. Which rows exist and which row a click lands on is decided by the
//! tested `ozbound::context_menu`; this module only connects it to Bevy.

use bevy::prelude::*;
use bevy::window::PrimaryWindow;
use ozbound::{
    context_menu::{build_menu, place_menu, row_at, MenuAction, MenuMetrics},
    interaction::examine_text,
    picking::{ground_to_tile, pick, ray_ground_hit, Pick, PickBox},
};

use crate::components::*;
use crate::pointer::{order_interact, order_walk, GROUND_Y};

/// Row geometry shared by the UI layout and the click test.
pub const METRICS: MenuMetrics = MenuMetrics { width: 190.0, row_height: 26.0 };
const ROW_IDLE: Color = Color::srgba(0.0, 0.0, 0.0, 0.0);
const ROW_HOVER: Color = Color::srgba(0.35, 0.30, 0.10, 0.9);

/// Opens the menu on right-click; while it is open, a left-click picks a row (and never also
/// walks), and any right-click closes it (reopening at the new spot).
pub fn menu_input(
    mut commands: Commands,
    windows: Query<&Window, With<PrimaryWindow>>,
    cameras: Query<(&Camera, &GlobalTransform), With<MainCamera>>,
    buttons: Res<ButtonInput<MouseButton>>,
    meadow: Res<Meadow>,
    assets: Res<ClickAssets>,
    targets: Query<(Entity, &Interactable)>,
    nodes: Query<&ResourceNode>,
    mut players: Query<&mut Player>,
    mut menu: ResMut<ContextMenuState>,
    mut progress: ResMut<GameProgress>,
    talk: Res<TalkState>,
) {
    // A conversation owns the mouse; a menu that was open when it began is dismissed.
    if talk.0.is_some() {
        if menu.0.is_some() {
            menu.0 = None;
        }
        return;
    }
    let left = buttons.just_pressed(MouseButton::Left);
    let right = buttons.just_pressed(MouseButton::Right);
    if !left && !right {
        return;
    }
    let Ok(window) = windows.get_single() else { return };
    let Some(cursor) = window.cursor_position() else { return };
    let map = &meadow.0;

    if let Some(open) = menu.0.take() {
        if left {
            let origin = (open.origin.x, open.origin.y);
            let Some(row) = row_at(origin, METRICS, open.entries.len(), (cursor.x, cursor.y)) else { return };
            let Ok(mut player) = players.get_single_mut() else { return };
            let object = open.target.and_then(|id| targets.get(id).ok());
            match open.entries[row].action {
                MenuAction::Cancel => {}
                MenuAction::Examine => {
                    if let Some((id, it)) = object {
                        let spent = nodes.get(id).is_ok_and(|node| node.0.is_depleted());
                        progress.journal = examine_text(it.kind, spent);
                    }
                }
                MenuAction::Interact => {
                    if let Some((id, it)) = object {
                        order_interact(&mut commands, map, &assets, &mut player, &mut progress, id, it);
                    }
                }
                MenuAction::WalkHere => {
                    if let Some(tile) = open.ground {
                        order_walk(&mut commands, map, &assets, &mut player, &mut progress, tile);
                    }
                }
            }
            return;
        }
        // Right-click while open: the old menu is closed; fall through to open a new one.
    }
    if !right {
        return;
    }

    let Ok((camera, camera_transform)) = cameras.get_single() else { return };
    let Ok(ray) = camera.viewport_to_world(camera_transform, cursor) else { return };
    let (origin, dir) = (ray.origin.to_array(), ray.direction.as_vec3().to_array());
    let boxes = targets.iter().map(|(entity, it)| PickBox { id: entity, min: it.pick_min, max: it.pick_max });
    let object = match pick(origin, dir, GROUND_Y, boxes) {
        Pick::Object { id, .. } => targets.get(id).ok(),
        _ => None,
    };
    // The ground tile under the cursor, even when an object stands in front of it.
    let ground = ray_ground_hit(origin, dir, GROUND_Y)
        .and_then(|hit| ground_to_tile(map, hit.x, hit.z))
        .filter(|tile| map.can_walk_to(*tile));

    let entries = build_menu(object.map(|(_, it)| (it.kind, it.name)), ground.is_some());
    let size = METRICS.size(entries.len());
    let at = place_menu((cursor.x, cursor.y), size, (window.width(), window.height()));
    menu.0 = Some(OpenMenu { origin: Vec2::new(at.0, at.1), entries, target: object.map(|(id, _)| id), ground });
}

/// Draws the menu: rebuilds the rows when the menu opens or closes, and highlights the row
/// under the cursor every frame.
pub fn sync_menu_ui(
    mut commands: Commands,
    windows: Query<&Window, With<PrimaryWindow>>,
    menu: Res<ContextMenuState>,
    mut panels: Query<(Entity, &mut Node, &mut Visibility), With<ContextMenuPanel>>,
    mut rows: Query<(&MenuRow, &mut BackgroundColor)>,
) {
    let Ok((panel, mut node, mut visibility)) = panels.get_single_mut() else { return };

    if menu.is_changed() {
        commands.entity(panel).despawn_descendants();
        match &menu.0 {
            Some(open) => {
                node.left = Val::Px(open.origin.x);
                node.top = Val::Px(open.origin.y);
                *visibility = Visibility::Inherited;
                commands.entity(panel).with_children(|panel| {
                    for (index, entry) in open.entries.iter().enumerate() {
                        panel
                            .spawn((
                                Node {
                                    width: Val::Px(METRICS.width),
                                    height: Val::Px(METRICS.row_height),
                                    align_items: AlignItems::Center,
                                    padding: UiRect::horizontal(Val::Px(10.0)),
                                    ..default()
                                },
                                BackgroundColor(ROW_IDLE),
                                MenuRow(index),
                            ))
                            .with_children(|row| {
                                row.spawn((
                                    Text::new(entry.label.clone()),
                                    TextFont { font_size: 17.0, ..default() },
                                    TextColor(Color::WHITE),
                                ));
                            });
                    }
                });
            }
            None => *visibility = Visibility::Hidden,
        }
    }

    let hovered = match (&menu.0, windows.get_single().ok().and_then(|w| w.cursor_position())) {
        (Some(open), Some(cursor)) => row_at((open.origin.x, open.origin.y), METRICS, open.entries.len(), (cursor.x, cursor.y)),
        _ => None,
    };
    for (row, mut background) in &mut rows {
        background.0 = if Some(row.0) == hovered { ROW_HOVER } else { ROW_IDLE };
    }
}
