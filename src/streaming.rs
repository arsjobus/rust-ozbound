//! Keeps the loaded part of the open world in step with where the player is.
//!
//! The overworld (`ozbound::overworld`) places every region as a chunk of one big map. Only the
//! chunks near the player exist as entities: [`stream_chunks`] spawns a chunk when the player
//! comes within its load margin and despawns it once the player is beyond its unload margin.
//! `Meadow` (the tile map) and `LevelData` (what stands on it) always describe exactly the
//! loaded chunks, in world coordinates, with the tiles of unloaded chunks blocked, so routes,
//! clicks, and interactions never reach a chunk that is not there.
//!
//! Nothing the player has done is lost when a chunk is dropped: its moonpetals picked and trees
//! used are stored in [`ChunkMemory`] and applied again when the chunk loads. Meshes and
//! materials are shared (`models::Kit`), so loading a chunk adds entities, not new assets. The
//! zone music of a chunk is read when it loads (`music_ui::sync_track_files`).

use bevy::prelude::*;
use ozbound::{overworld::Overworld, save::WorldChanges, world::TilePos};

use crate::components::*;
use crate::models::Kit;
use crate::setup::{ensure_chunk_heights, overworld_neighbour, spawn_area, Area, Ground};

/// What the player has done in the chunks `ids`, put into `memory` (world tiles). Called just
/// before those chunks are despawned.
pub fn stash(memory: &mut ChunkMemory, overworld: &Overworld, atlas: &Atlas, ids: &[String], petals: &[TilePos], nodes: &[(TilePos, u32)]) {
    for id in ids {
        let Some(level) = atlas.level(id) else { continue };
        memory.0.insert(id.clone(), overworld.capture_changes(id, level, petals, nodes));
    }
}

/// The UI state that may point at an entity a dropped chunk takes with it.
#[derive(bevy::ecs::system::SystemParam)]
pub struct Pointing<'w> {
    pub talk: ResMut<'w, TalkState>,
    pub key: ResMut<'w, KeyTarget>,
    pub menu: ResMut<'w, ContextMenuState>,
    pub hover: ResMut<'w, HoverText>,
}

impl Pointing<'_> {
    /// Drops every reference to what was in the world.
    pub fn clear(&mut self) {
        self.talk.0 = None;
        self.key.0 = None;
        *self.menu = ContextMenuState::default();
        *self.hover = HoverText::default();
    }
}

/// What building a chunk's world needs to create: portraits, images, and meshes.
#[derive(bevy::ecs::system::SystemParam)]
pub struct Gfx<'w> {
    pub portraits: ResMut<'w, Portraits>,
    pub images: ResMut<'w, Assets<Image>>,
    pub meshes: ResMut<'w, Assets<Mesh>>,
}

/// Loads and unloads chunks as the player moves, and keeps `CurrentRegion` on the chunk the
/// player stands in.
pub fn stream_chunks(
    mut commands: Commands,
    atlas: Res<Atlas>,
    mut streaming: ResMut<Streaming>,
    mut memory: ResMut<ChunkMemory>,
    mut heights: ResMut<GroundHeights>,
    mut meadow: ResMut<Meadow>,
    mut level: ResMut<LevelData>,
    mut current: ResMut<CurrentRegion>,
    wildlife: Res<Wildlife>,
    kit: Res<Kit>,
    mut gfx: Gfx,
    mut pointing: Pointing,
    players: Query<&Player>,
    chunk_entities: Query<(Entity, &ChunkEntity)>,
    petals: Query<&Interactable, With<Moonpetal>>,
    nodes: Query<(&ResourceNode, &Interactable)>,
) {
    if streaming.interior {
        return;
    }
    let (Some(overworld), Ok(player)) = (atlas.overworld.as_ref(), players.get_single()) else { return };
    if let Some(chunk) = overworld.chunk_at(player.tile) {
        if current.0 != chunk.id {
            current.0 = chunk.id.clone();
        }
    }
    let wanted = overworld.wanted(player.tile, &streaming.loaded);
    let unload: Vec<String> = streaming.loaded.iter().filter(|id| !wanted.contains(id)).cloned().collect();
    let load: Vec<String> = wanted.iter().filter(|id| !streaming.loaded.contains(id)).cloned().collect();
    if unload.is_empty() && load.is_empty() {
        return;
    }

    // Remember what was done in the chunks that go, then remove them.
    if !unload.is_empty() {
        let petal_tiles: Vec<TilePos> = petals.iter().map(|p| p.tile).collect();
        let node_state: Vec<(TilePos, u32)> = nodes.iter().map(|(n, at)| (at.tile, n.0.remaining_actions())).collect();
        stash(&mut memory, overworld, &atlas, &unload, &petal_tiles, &node_state);
        for (entity, tag) in &chunk_entities {
            if unload.contains(&tag.0) {
                commands.entity(entity).despawn_recursive();
            }
        }
        // Nothing may keep pointing at something that was just removed.
        pointing.clear();
    }

    let composite = overworld.compose(&|id| atlas.level(id), &wanted);
    let none = WorldChanges::none();
    let neighbour = overworld_neighbour(&atlas);
    ensure_chunk_heights(&mut heights.0, overworld, &atlas, &load);
    for id in &load {
        let chunk = overworld.chunk(id).expect("wanted chunks exist");
        let ground = Ground { scene: id, chunk: true, neighbour: &neighbour, heights: &heights.0 };
        let Gfx { portraits, images, meshes } = &mut gfx;
        spawn_area(&mut commands, &kit, &composite.map, &composite.level, Area::of_chunk(chunk), memory.0.get(id).unwrap_or(&none), &wildlife, portraits, images, meshes, &ground);
    }
    meadow.0 = composite.map;
    level.0 = composite.level;
    streaming.loaded = wanted;
}

/// Marks the root entities that were just spawned with the chunk they stand in, so
/// [`stream_chunks`] can remove them with it. Off the overworld they are marked as the region's
/// (`RegionEntity`) and removed when the player travels. The player, camera, light, floor, and
/// UI belong to no chunk.
pub fn tag_world_entities(
    mut commands: Commands,
    atlas: Res<Atlas>,
    streaming: Res<Streaming>,
    meadow: Res<Meadow>,
    fresh: Query<
        (Entity, &Transform),
        (Added<Transform>, Without<RegionEntity>, Without<ChunkEntity>, Without<Parent>, Without<Node>, Without<Camera>, Without<DirectionalLight>, Without<Player>, Without<WorldFloor>),
    >,
) {
    for (entity, transform) in &fresh {
        let chunk = (!streaming.interior)
            .then(|| atlas.overworld.as_ref())
            .flatten()
            .zip(meadow.0.world_to_tile((transform.translation.x, -transform.translation.z)))
            .and_then(|(overworld, tile)| overworld.chunk_at(tile));
        match chunk {
            Some(chunk) => commands.entity(entity).insert(ChunkEntity(chunk.id.clone())),
            None => commands.entity(entity).insert(RegionEntity),
        };
    }
}

/// What tells an interactable object's kind apart (see `assign_net_ids`).
#[derive(bevy::ecs::query::QueryData)]
pub struct FreshObject {
    entity: Entity,
    at: &'static Interactable,
    sign: Option<&'static SignText>,
    npc: Option<&'static Npc>,
    creature: Option<&'static Creature>,
    node: Option<&'static ResourceNode>,
    petal: Option<&'static Moonpetal>,
}

/// Gives every interactable object that was just spawned its stable network name (see
/// `ozbound::net`). The name comes from what the object is and where it stands, so the same
/// tree has the same id on every run and on every machine, and re-spawning a chunk gives the
/// same objects the same ids.
pub fn assign_net_ids(
    mut commands: Commands,
    current: Res<CurrentRegion>,
    streaming: Res<Streaming>,
    fresh: Query<FreshObject, (Added<Interactable>, Without<Replicated>)>,
) {
    let scope = if streaming.interior { current.0.as_str() } else { ozbound::net::OVERWORLD_SCOPE };
    for FreshObjectItem { entity, at, sign, npc, creature, node, petal } in &fresh {
        let kind = if let Some(Npc(id)) = npc {
            ozbound::net::kind_npc(id)
        } else if let Some(Creature(id)) = creature {
            ozbound::net::kind_creature(id)
        } else if let Some(node) = node {
            ozbound::net::kind_node(node.0.kind().label())
        } else if petal.is_some() {
            ozbound::net::kind_moonpetal().to_string()
        } else if sign.is_some() {
            ozbound::net::kind_sign().to_string()
        } else {
            continue;
        };
        commands.entity(entity).insert(Replicated(ozbound::net::NetId::of(scope, &kind, at.tile)));
    }
}
