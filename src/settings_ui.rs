//! Applies the settings to the running game and writes the settings file, and runs the
//! frame-time overlay. What the settings are, how they are stored, and the numbers the overlay
//! shows are the tested `ozbound::{settings, settings_store, perf}`; this module connects them
//! to Bevy.
//!
//! - Text size becomes the UI scale, lowered if the window is too small for the panels
//!   (`text_fit::fit_scale`), so text is reduced rather than clipped.
//! - The ambient loop's volume follows the volume setting and the ambient switch.
//! - Changed settings are written at once (temp file + rename). A settings file that could not
//!   be read at startup, or a write that fails, switches writing off for the session so the
//!   player's file is never overwritten, and says so in the chat area and on the console.

use bevy::audio::AudioSinkPlayback;
use bevy::prelude::*;
use bevy::window::PrimaryWindow;
use ozbound::{settings_store, text_fit::fit_scale};

use crate::components::*;

/// F3 shows or hides the frame-time overlay.
pub fn toggle_overlay(keys: Res<ButtonInput<KeyCode>>, mut settings: ResMut<UserSettings>) {
    if keys.just_pressed(KeyCode::F3) {
        settings.0.show_perf = !settings.0.show_perf;
    }
}

pub fn apply_settings(
    settings: Res<UserSettings>,
    mut store: ResMut<SettingsStore>,
    mut progress: ResMut<GameProgress>,
    windows: Query<&Window, With<PrimaryWindow>>,
    mut ui_scale: ResMut<UiScale>,
    sinks: Query<&AudioSink, With<AmbientSound>>,
    mut overlays: Query<&mut Visibility, With<PerfHud>>,
) {
    // Text size, limited by the window (checked every frame so resizing is handled too).
    if let Ok(window) = windows.get_single() {
        let scale = fit_scale(settings.0.requested_ui_scale(), window.width(), window.height());
        if (ui_scale.0 - scale).abs() > 1.0e-4 {
            ui_scale.0 = scale;
        }
    }
    let gain = settings.0.ambient_gain();
    for sink in &sinks {
        if (sink.volume() - gain).abs() > 1.0e-4 {
            sink.set_volume(gain);
        }
    }
    for mut visibility in &mut overlays {
        let wanted = if settings.0.show_perf { Visibility::Inherited } else { Visibility::Hidden };
        if *visibility != wanted {
            *visibility = wanted;
        }
    }
    if store.enabled && settings.0 != store.saved {
        match settings_store::save(&store.path, &settings.0) {
            Ok(()) => store.saved = settings.0,
            Err(error) => {
                eprintln!("warning: {error}; settings will not be saved this session");
                store.enabled = false;
                progress.notice = "Your settings could not be saved. See the console for details.".into();
            }
        }
    }
}

/// Records every real frame (the clock keeps running while paused, so a pause is not a stall) and
/// refreshes the overlay a few times a second while it is shown.
#[allow(clippy::manual_is_multiple_of)] // `is_multiple_of` needs a newer compiler than the minimum supported
pub fn record_frames(time: Res<Time<Real>>, settings: Res<UserSettings>, mut stats: ResMut<PerfStats>, mut overlay: Query<&mut Text, With<PerfHud>>) {
    stats.0.record(time.delta_secs());
    if settings.0.show_perf && stats.0.frames() % 15 == 0 {
        if let Ok(mut text) = overlay.get_single_mut() {
            text.0 = stats.0.overlay_text();
        }
    }
}

/// Prints the session's frame-time summary when the game exits, if `OZBOUND_PROFILE` is set.
pub fn report_on_exit(mut exits: EventReader<AppExit>, stats: Res<PerfStats>) {
    if exits.read().next().is_some() && std::env::var_os("OZBOUND_PROFILE").is_some() {
        if let Some(summary) = stats.0.summary() {
            eprintln!("{summary}");
        }
    }
}
