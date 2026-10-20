//! Third-person orbit camera that follows the player: middle-drag or arrow keys to turn,
//! scroll wheel or the + and - keys to zoom. The angle and distance maths lives in the tested `ozbound::orbit`.

use bevy::input::mouse::{MouseMotion, MouseScrollUnit, MouseWheel};
use bevy::prelude::*;
use ozbound::orbit::OrbitCamera;

use crate::components::{ChatState, MainCamera, Player};

const KEY_TURN_SPEED: f32 = 1.8;
const MOUSE_TURN_SPEED: f32 = 0.005;
const ZOOM_PER_NOTCH: f32 = 1.5;
/// Zoom speed (world units per second) for the + and - keys, the keyboard way to zoom.
const ZOOM_KEY_SPEED: f32 = 8.0;
/// Pixel-based scrolling (trackpads) reports many small steps; this converts them to notches.
const PIXELS_PER_NOTCH: f32 = 40.0;

#[derive(Resource)]
pub struct Rig {
    pub orbit: OrbitCamera,
    pub focus: Vec3,
}

impl Rig {
    pub fn new(focus: Vec3) -> Self {
        Self { orbit: OrbitCamera::default(), focus }
    }

    pub fn camera_transform(&self) -> Transform {
        Transform::from_translation(self.focus + Vec3::from_array(self.orbit.offset())).looking_at(self.focus, Vec3::Y)
    }
}

pub fn update_camera(
    time: Res<Time>,
    keys: Res<ButtonInput<KeyCode>>,
    mouse: Res<ButtonInput<MouseButton>>,
    mut motion: EventReader<MouseMotion>,
    mut wheel: EventReader<MouseWheel>,
    chat: Res<ChatState>,
    mut rig: ResMut<Rig>,
    player: Query<&Transform, (With<Player>, Without<MainCamera>)>,
    mut camera: Query<&mut Transform, With<MainCamera>>,
) {
    let dt = time.delta_secs();
    // The arrow keys and + / - belong to the chat box while it is open.
    let keys_live = !chat.keys_captured;

    let mut turn = 0.0;
    let mut tilt = 0.0;
    if keys_live && keys.pressed(KeyCode::ArrowLeft) {
        turn += KEY_TURN_SPEED * dt;
    }
    if keys_live && keys.pressed(KeyCode::ArrowRight) {
        turn -= KEY_TURN_SPEED * dt;
    }
    if keys_live && keys.pressed(KeyCode::ArrowUp) {
        tilt += KEY_TURN_SPEED * dt;
    }
    if keys_live && keys.pressed(KeyCode::ArrowDown) {
        tilt -= KEY_TURN_SPEED * dt;
    }
    // Always drain mouse motion so a long drag doesn't replay later.
    let drag: Vec2 = motion.read().map(|m| m.delta).sum();
    if mouse.pressed(MouseButton::Middle) {
        turn -= drag.x * MOUSE_TURN_SPEED;
        tilt += drag.y * MOUSE_TURN_SPEED;
    }
    rig.orbit.rotate(turn, tilt);

    for event in wheel.read() {
        let notches = match event.unit {
            MouseScrollUnit::Line => event.y,
            MouseScrollUnit::Pixel => event.y / PIXELS_PER_NOTCH,
        };
        rig.orbit.zoom(-notches * ZOOM_PER_NOTCH);
    }

    if keys_live && keys.any_pressed([KeyCode::Equal, KeyCode::NumpadAdd]) {
        rig.orbit.zoom(-ZOOM_KEY_SPEED * dt);
    }
    if keys_live && keys.any_pressed([KeyCode::Minus, KeyCode::NumpadSubtract]) {
        rig.orbit.zoom(ZOOM_KEY_SPEED * dt);
    }

    if let Ok(player) = player.get_single() {
        let target = player.translation + Vec3::Y * 0.9;
        let blend = 1.0 - (-8.0 * dt).exp();
        rig.focus = rig.focus.lerp(target, blend);
    }
    if let Ok(mut transform) = camera.get_single_mut() {
        *transform = rig.camera_transform();
    }
}
