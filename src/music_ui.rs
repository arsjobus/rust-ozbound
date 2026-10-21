//! Background music: starts the current region's theme and swaps it when the player travels.
//! Which file or built-in theme plays is decided by `ozbound::music::choose` from the region's
//! entry in `assets/world/locations.world`; this module only plays the result.
//!
//! The track is handed to Bevy as raw bytes (not through the asset server), so `.mp4` works
//! even though Bevy's asset loader does not list that extension. Its volume and on/off state
//! follow the ambient-sound setting (`settings_ui::apply_settings`).

use std::path::Path;

use bevy::prelude::*;
use ozbound::music::{self, Track};

use crate::components::*;

/// Starts the music for the current region when it differs from what is playing.
pub fn sync_music(
    mut commands: Commands,
    region: Res<CurrentRegion>,
    atlas: Res<Atlas>,
    settings: Res<UserSettings>,
    mut state: ResMut<BgmState>,
    mut sounds: ResMut<Assets<AudioSource>>,
    playing: Query<Entity, With<BgmSound>>,
) {
    if state.0.as_deref() == Some(region.0.as_str()) {
        return;
    }
    state.0 = Some(region.0.clone());
    for entity in &playing {
        commands.entity(entity).despawn();
    }
    let Some(location) = atlas.locations.get(&region.0) else { return };
    let choice = music::choose(Path::new(music::BGM_DIR), location.music.as_deref(), location.theme.as_deref());
    if let Some(note) = &choice.note {
        eprintln!("music for {}: {note}", location.name);
    }
    let bytes = match choice.track {
        Track::File { bytes, .. } | Track::Builtin { bytes, .. } => bytes,
        Track::Silence => return,
    };
    let source = sounds.add(AudioSource { bytes: bytes.into() });
    commands.spawn((
        AudioPlayer::new(source),
        PlaybackSettings::LOOP.with_volume(bevy::audio::Volume::new(settings.0.ambient_gain())),
        BgmSound,
    ));
}
