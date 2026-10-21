//! Zone music: crossfades between tracks as the player crosses zone boundaries.
//!
//! Which zone a tile belongs to is `ozbound::overworld::Overworld::zone_at`; how the tracks
//! fade is the tested `ozbound::soundscape::Mixer`; which file or built-in theme a track is
//! comes from `ozbound::music::choose`. This module only plays the result: one looping
//! `AudioPlayer` per audible track, with its volume set every frame to the music setting times
//! the mixer's gain for that track.
//!
//! Tracks are handed to Bevy as raw bytes (not through the asset server), so `.mp3` works
//! whatever the asset loader lists. They are read when a chunk that has the zone loads and
//! dropped when no loaded chunk needs them, so the music of the next zone is ready before the
//! player reaches its border. Two zones that name the same track share one voice: crossing
//! between them changes nothing.
//!
//! Off the overworld (a cave) the region's own `music` / `theme` from the location register
//! plays instead, and the mixer fades to it the same way.

use std::collections::HashSet;
use std::path::Path;

use bevy::prelude::*;
use ozbound::{
    music::{self, Track},
    overworld::TrackSpec,
    soundscape::{Mixer, FADE_SECONDS},
};

use crate::components::*;

/// Reads a zone's track (once) and keeps the handle; `None` means the zone is silent.
fn load_track(sound: &mut Soundscape, sources: &mut Assets<AudioSource>, spec: &TrackSpec, place: &str) {
    if sound.tracks.contains_key(spec) {
        return;
    }
    let choice = music::choose(Path::new(music::BGM_DIR), spec.music.as_deref(), spec.theme.as_deref());
    if let Some(note) = &choice.note {
        eprintln!("music for {place}: {note}");
    }
    let handle = match choice.track {
        Track::File { bytes, .. } | Track::Builtin { bytes, .. } => Some(sources.add(AudioSource { bytes: bytes.into() })),
        Track::Silence => None,
    };
    sound.tracks.insert(spec.clone(), handle);
}

/// What the player's surroundings play right now: the zone's id, its name, and its track.
fn current_zone(atlas: &Atlas, streaming: &Streaming, region: &CurrentRegion, player: &Player) -> Option<(String, String, TrackSpec)> {
    if let (false, Some(overworld)) = (streaming.interior, atlas.overworld.as_ref()) {
        let zone = overworld.zone_at(player.tile)?;
        return Some((zone.id.clone(), zone.name.clone(), zone.track.clone()));
    }
    let location = atlas.locations.get(&region.0)?;
    Some((location.id.clone(), location.name.clone(), TrackSpec { music: location.music.clone(), theme: location.theme.clone() }))
}

/// Follows the player through the zones: reads the tracks the loaded chunks need, points the
/// mixer at the player's zone, announces a new zone in the chat, and plays the mix.
pub fn sync_music(
    mut commands: Commands,
    time: Res<Time<Real>>,
    atlas: Res<Atlas>,
    streaming: Res<Streaming>,
    region: Res<CurrentRegion>,
    settings: Res<UserSettings>,
    mut sound: ResMut<Soundscape>,
    mut progress: ResMut<GameProgress>,
    mut sources: ResMut<Assets<AudioSource>>,
    players: Query<&Player>,
    voices: Query<(Entity, &BgmVoice, Option<&AudioSink>)>,
) {
    let Ok(player) = players.get_single() else { return };
    let here = current_zone(&atlas, &streaming, &region, player);

    // Tracks for the zones of every loaded chunk are read as the chunk loads and dropped when
    // nothing loaded (and nothing playing) needs them any more.
    if streaming.is_changed() || sound.tracks.is_empty() {
        let mut needed: HashSet<TrackSpec> = HashSet::new();
        if let (false, Some(overworld)) = (streaming.interior, atlas.overworld.as_ref()) {
            for id in &streaming.loaded {
                for zone in overworld.zones_in(id) {
                    load_track(&mut sound, &mut sources, &zone.track, &zone.name);
                    needed.insert(zone.track.clone());
                }
            }
        }
        if let Some((_, name, spec)) = &here {
            load_track(&mut sound, &mut sources, spec, name);
            needed.insert(spec.clone());
        }
        let playing: Vec<TrackSpec> = voices.iter().map(|(_, voice, _)| voice.0.clone()).collect();
        sound.tracks.retain(|spec, _| needed.contains(spec) || playing.contains(spec));
    }

    let master = settings.0.ambient_gain();
    if let Some((id, name, spec)) = here {
        if sound.zone.as_deref() != Some(id.as_str()) {
            // The first zone of a session is not announced (the world is just appearing), and
            // a journey off the overworld already says where you went.
            if sound.zone.is_some() && !streaming.interior {
                progress.notice = format!("You enter {name}.");
            }
            load_track(&mut sound, &mut sources, &spec, &name);
            match sound.mixer.as_mut() {
                Some(mixer) => mixer.set_target(Some(spec)),
                None => sound.mixer = Some(Mixer::starting_with(spec, FADE_SECONDS)),
            }
            sound.zone = Some(id);
        }
    }

    let Some(mixer) = sound.mixer.as_mut() else { return };
    let frame = mixer.tick(time.delta_secs());
    let gain_of = |spec: &TrackSpec| frame.gains.iter().find(|(key, _)| key == spec).map_or(0.0, |(_, gain)| *gain);
    for spec in &frame.start {
        let Some(Some(handle)) = sound.tracks.get(spec) else { continue };
        commands.spawn((
            AudioPlayer::new(handle.clone()),
            PlaybackSettings::LOOP.with_volume(bevy::audio::Volume::new(master * gain_of(spec))),
            BgmVoice(spec.clone()),
        ));
    }
    for (entity, voice, sink) in &voices {
        if frame.stop.contains(&voice.0) {
            commands.entity(entity).despawn();
        } else if let Some(sink) = sink {
            let volume = master * gain_of(&voice.0);
            if (sink.volume() - volume).abs() > 1.0e-4 {
                sink.set_volume(volume);
            }
        }
    }
}
