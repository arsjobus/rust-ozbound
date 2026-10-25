//! Right-click context menu: opening it on whatever is under the cursor, choosing a row,
//! and drawing the panel. Which rows exist and which row a click lands on is decided by the
//! tested `ozbound::context_menu`; this module only connects it to Bevy.

use bevy::prelude::*;
use bevy::window::PrimaryWindow;
use ozbound::chat::Kind;
use ozbound::{
    context_menu::{build_menu, fit_label, menu_metrics, place_menu, row_at, MenuAction, MENU_FONT_PX, MENU_PAD_X},
    interaction::examine_text,
    picking::{ground_to_tile, pick_over, ray_terrain_hit, Pick, PickBox},
    text_fit::logical_size,
};

use crate::components::*;
use crate::pointer::{order_interact, order_walk};

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
    pack: Res<PackState>,
    mut chat: ResMut<ChatState>,
    blockers: Query<&Interaction, With<UiBlocker>>,
    (heights, ui_scale): (Res<GroundHeights>, Res<UiScale>),
) {
    // A conversation or the crafting panel owns the mouse; a menu that was open when it
    // began is dismissed.
    if talk.0.is_some() || pack.0 {
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
    // A click on the chat, the toolbar, or a side panel belongs to that panel, not the world
    // behind it. It dismisses an open menu, except a left-click, which may be picking a row.
    let over_ui = blockers.iter().any(|interaction| *interaction != Interaction::None);
    if over_ui && (right || menu.0.is_none()) {
        menu.0 = None;
        return;
    }
    let Ok(window) = windows.get_single() else { return };
    let Some(physical_cursor) = window.cursor_position() else { return };
    // Menu geometry is in the interface's own pixels, which the text-size setting scales.
    let scale = if ui_scale.0.is_finite() && ui_scale.0 > 0.0 { ui_scale.0 } else { 1.0 };
    let cursor = physical_cursor / scale;
    let logical = logical_size(window.width(), window.height(), scale);
    let map = &meadow.0;

    if let Some(open) = menu.0.take() {
        if left {
            let origin = (open.origin.x, open.origin.y);
            let Some(row) = row_at(origin, open.metrics, open.entries.len(), (cursor.x, cursor.y)) else { return };
            let Ok(mut player) = players.get_single_mut() else { return };
            let object = open.target.and_then(|id| targets.get(id).ok());
            match open.entries[row].action {
                MenuAction::Cancel => {}
                MenuAction::Examine => {
                    if let Some((id, it)) = object {
                        let spent = nodes.get(id).is_ok_and(|node| node.0.is_depleted());
                        chat.log.push(Kind::Examine, examine_text(it.kind, spent));
                    }
                }
                MenuAction::Interact => {
                    if let Some((id, it)) = object {
                        order_interact(&mut commands, map, &heights.0, &assets, &mut player, &mut progress, id, it);
                    }
                }
                MenuAction::WalkHere => {
                    if let Some(tile) = open.ground {
                        order_walk(&mut commands, map, &heights.0, &assets, &mut player, &mut progress, tile);
                    }
                }
                // The editor's rows only appear in the editor's own menu (see `editor_ui.rs`).
                MenuAction::Select | MenuAction::MoveHere | MenuAction::Remove | MenuAction::PlaceHere => {}
            }
            return;
        }
        // Right-click while open: the old menu is closed; fall through to open a new one.
    }
    if !right {
        return;
    }

    let Ok((camera, camera_transform)) = cameras.get_single() else { return };
    let Ok(ray) = camera.viewport_to_world(camera_transform, physical_cursor) else { return };
    let (origin, dir) = (ray.origin.to_array(), ray.direction.as_vec3().to_array());
    let boxes = targets.iter().map(|(entity, it)| PickBox { id: entity, min: it.pick_min, max: it.pick_max });
    let terrain = ray_terrain_hit(origin, dir, &|x, z| heights.0.height_at_world(x, z));
    let object = match pick_over(origin, dir, terrain, boxes) {
        Pick::Object { id, .. } => targets.get(id).ok(),
        _ => None,
    };
    // The ground tile under the cursor, even when an object stands in front of it.
    let ground = terrain
        .and_then(|hit| ground_to_tile(map, hit.x, hit.z))
        .filter(|tile| map.can_walk_to(*tile));

    let entries = build_menu(object.map(|(_, it)| (it.kind, it.name)), ground.is_some());
    let metrics = menu_metrics(&entries, logical.0);
    let at = place_menu((cursor.x, cursor.y), metrics.size(entries.len()), logical);
    menu.0 = Some(OpenMenu { origin: Vec2::new(at.0, at.1), metrics, entries, target: object.map(|(id, _)| id), ground });
}

/// Draws the menu: rebuilds the rows when the menu opens or closes, and highlights the row
/// under the cursor every frame.
pub fn sync_menu_ui(
    mut commands: Commands,
    windows: Query<&Window, With<PrimaryWindow>>,
    menu: Res<ContextMenuState>,
    mut panels: Query<(Entity, &mut Node, &mut Visibility), With<ContextMenuPanel>>,
    mut rows: Query<(&MenuRow, &mut BackgroundColor)>,
    ui_scale: Res<UiScale>,
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
                                    width: Val::Px(open.metrics.width),
                                    height: Val::Px(open.metrics.row_height),
                                    align_items: AlignItems::Center,
                                    padding: UiRect::horizontal(Val::Px(MENU_PAD_X)),
                                    ..default()
                                },
                                BackgroundColor(ROW_IDLE),
                                MenuRow(index),
                            ))
                            .with_children(|row| {
                                row.spawn((
                                    Text::new(fit_label(&entry.label, open.metrics.label_chars())),
                                    TextFont { font_size: MENU_FONT_PX, ..default() },
                                    TextLayout::new_with_no_wrap(),
                                    TextColor(Color::WHITE),
                                ));
                            });
                    }
                });
            }
            None => *visibility = Visibility::Hidden,
        }
    }

    let scale = if ui_scale.0.is_finite() && ui_scale.0 > 0.0 { ui_scale.0 } else { 1.0 };
    let hovered = match (&menu.0, windows.get_single().ok().and_then(|w| w.cursor_position())) {
        (Some(open), Some(cursor)) => row_at((open.origin.x, open.origin.y), open.metrics, open.entries.len(), (cursor.x / scale, cursor.y / scale)),
        _ => None,
    };
    for (row, mut background) in &mut rows {
        background.0 = if Some(row.0) == hovered { ROW_HOVER } else { ROW_IDLE };
    }
}
