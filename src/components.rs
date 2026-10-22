//! Bevy components and resources shared by the game systems.

use std::collections::HashMap;
use std::path::PathBuf;

use bevy::prelude::*;
use ozbound::{
    bestiary::{Bestiary, FieldNotes},
    chat::{ChatInput, ChatLog},
    emote::ActiveEmote,
    side_panel::{Dock, Tab},
    pause::PauseMenu,
    perf::FrameStats,
    settings::Settings,
    content::Content,
    context_menu::MenuEntry,
    crafting::RecipeBook,
    equipment::Equipment,
    gathering::GatheringNode,
    interaction::InteractionKind,
    inventory::Inventory,
    level::Level,
    pathfinding::Approach,
    quests::QuestLog,
    respawn::RespawnTimer,
    save::SaveGame,
    skills::Skills,
    talk::Talk,
    task::Task,
    world::{TilePos, WorldMap},
};

/// The rules map for the current scene. Rendering and clicking are derived from it.
#[derive(Resource)]
pub struct Meadow(pub WorldMap);

/// The loaded level: where the player, guide, moonpetals, and resource nodes stand.
/// `Meadow` holds the same map for the systems that only need tiles.
#[derive(Resource)]
pub struct LevelData(pub Level);

/// The validated quest and dialogue content for this session.
#[derive(Resource)]
pub struct GameContent(pub Content);

#[derive(Resource, Default)]
pub struct GameProgress {
    pub log: QuestLog,
    /// A message waiting to be shown in the chat area. `chat_ui::collect_notices` moves it
    /// into the log every frame, so systems just set this and never touch the chat directly.
    pub notice: String,
    pub inventory: Inventory,
    pub equipment: Equipment,
    /// Skill XP and levels.
    pub skills: Skills,
}

/// The validated recipes for this session.
#[derive(Resource)]
pub struct Recipes(pub RecipeBook);

/// Whether the crafting-and-gear panel (C) is open. While it is, it owns the mouse and the
/// number keys, like a conversation does.
#[derive(Resource, Default)]
pub struct PackState(pub bool);

/// The panel that lists recipes and gear rows.
#[derive(Component)]
pub struct PackPanel;

/// One clickable panel row; the index matches `ozbound::pack::rows`.
#[derive(Component)]
pub struct PackRowButton(pub usize);

/// The conversation that is open, if any. While one is, it owns the mouse and the number keys.
#[derive(Resource, Default)]
pub struct TalkState(pub Option<Talk>);

/// Where the game is saved. `enabled` is false when an unreadable save was found at startup (or a
/// write failed), so that file is never overwritten. `saved` is the game as last written, and
/// `idle` the seconds since the last check for changes.
#[derive(Resource)]
pub struct SaveState {
    pub path: PathBuf,
    pub enabled: bool,
    pub saved: SaveGame,
    pub idle: f32,
}

/// The game as it was when the program started (a loaded save or a new game). `setup` reads it
/// to place the player and to leave out the moonpetals already picked and the nodes already used.
#[derive(Resource)]
pub struct StartingSave(pub SaveGame);

/// Who a character is in the dialogue file (`npc = ...`).
#[derive(Component)]
pub struct Npc(pub String);

/// An NPC's mind (see `ozbound::npc`): decides when it walks, rests, gestures, and turns.
#[derive(Component)]
pub struct NpcBrain(pub ozbound::npc::Brain);

/// An NPC's body: where it is gliding to, which way it wants to face, and the emote it plays.
/// Only `npc_ui` writes it; `animation::animate_limbs` reads it to pose the limbs.
#[derive(Component, Default)]
pub struct NpcBody {
    /// The point (world space) the body is gliding toward, if it is walking.
    pub glide: Option<Vec3>,
    /// The direction (yaw) to turn toward when standing still.
    pub face_yaw: Option<f32>,
    pub emote: Option<ActiveEmote>,
}

/// Who the open conversation is with (`None` for a sign or when nothing is open). Set when the
/// player reaches an NPC; `npc_ui` clears it once the conversation is closed.
#[derive(Resource, Default)]
pub struct TalkPartner(pub Option<Entity>);

/// Which bestiary entry a creature in the world is (`id` from `[creatures]`).
#[derive(Component)]
pub struct Creature(pub String);

/// The bestiary and what the player has met so far.
#[derive(Resource)]
pub struct Wildlife {
    pub bestiary: Bestiary,
    pub notes: FieldNotes,
}

/// Dialogue portraits by NPC id, drawn once at startup from each NPC's appearance.
#[derive(Resource, Default)]
pub struct Portraits(pub HashMap<String, Handle<Image>>);

/// The portrait of the person the open conversation is with.
#[derive(Resource, Default)]
pub struct TalkPortrait(pub Option<Handle<Image>>);

/// The panel that shows the current line of dialogue and its choices.
#[derive(Component)]
pub struct DialoguePanel;

/// One clickable choice; the index matches `Talk::view().choices`.
#[derive(Component)]
pub struct ChoiceRow(pub usize);

/// The panel that lists active and completed quests (toggled with J).
/// The panel under the toolbar that shows the open side panel (bag, skills, or quest book).
#[derive(Component)]
pub struct DockPanel;

/// A toolbar button that toggles a side panel.
#[derive(Component)]
pub struct ToolbarButton(pub Tab);

/// Marks UI the mouse can hover or click: while the cursor is over it, clicks and right-clicks
/// do not reach the world behind it.
#[derive(Component)]
pub struct UiBlocker;

/// The panel holding the chat log and the input line.
#[derive(Component)]
pub struct ChatPanel;

/// What the cursor is over, shown as an action label such as "Chop down tree".
#[derive(Resource, Default)]
pub struct HoverText(pub String);

#[derive(Component)]
pub struct MainCamera;

/// Repeating gather/mine attempts against a node while the player stands beside it.
pub struct Work {
    pub target: Entity,
    pub timer: Timer,
}

/// The player's logical state. `tile` is the last tile reached; the `Transform` glides
/// between tiles for display.
#[derive(Component)]
pub struct Player {
    pub tile: TilePos,
    pub task: Task<Entity>,
    pub work: Option<Work>,
    /// A gesture being played (`/wave` and friends), if any.
    pub emote: Option<ActiveEmote>,
}

#[derive(Component)]
pub struct Guide;

#[derive(Component)]
pub struct Moonpetal;

/// A tree or copper vein backed by the tested gathering rules.
#[derive(Component)]
pub struct ResourceNode(pub GatheringNode);

/// What a signpost says when read.
#[derive(Component)]
pub struct SignText(pub String);

/// The child entity to hide once its node is spent (tree crown, copper flecks).
#[derive(Component)]
pub struct SpentVisual(pub Entity);

/// Something the player can left-click. The pick box is an axis-aligned box in world space.
#[derive(Component)]
pub struct Interactable {
    pub kind: InteractionKind,
    pub name: &'static str,
    pub tile: TilePos,
    pub approach: Approach,
    pub pick_min: [f32; 3],
    pub pick_max: [f32; 3],
}

/// A swinging arm or leg. `owner` is the character it belongs to.
#[derive(Component)]
pub struct Limb {
    pub owner: Entity,
    pub swing: f32,
    pub is_arm: bool,
    /// -1 for the limb on the character's -x side, +1 for the +x side (used by emote poses).
    pub side: f32,
}

/// Gentle bobbing and turning for collectables.
#[derive(Component)]
pub struct Floaty {
    pub base_y: f32,
    pub phase: f32,
}

/// The small cross that appears where the player clicked.
#[derive(Component)]
pub struct ClickMarker {
    pub timer: Timer,
}

#[derive(Resource)]
pub struct ClickAssets {
    pub bar: Handle<Mesh>,
    pub walk: Handle<StandardMaterial>,
    pub interact: Handle<StandardMaterial>,
}

#[derive(Component)]
pub struct HudText;

#[derive(Component)]
pub struct HoverHud;

/// Counts down on a spent tree or vein; when it finishes the node is restocked and shown again.
#[derive(Component)]
pub struct Respawning(pub RespawnTimer);

/// A right-click menu that is currently open. `target` is the object the menu was opened on
/// (if any) and `ground` the walkable tile under the cursor at that moment.
pub struct OpenMenu {
    pub origin: Vec2,
    pub entries: Vec<MenuEntry>,
    pub target: Option<Entity>,
    pub ground: Option<TilePos>,
}

#[derive(Resource, Default)]
pub struct ContextMenuState(pub Option<OpenMenu>);

/// The UI panel that shows the menu rows.
#[derive(Component)]
pub struct ContextMenuPanel;

/// One clickable row of the menu panel; the index matches `OpenMenu::entries`.
#[derive(Component)]
pub struct MenuRow(pub usize);

/// The player's settings (volume, text size, overlay). Systems read this; `settings_ui` applies it.
#[derive(Resource)]
pub struct UserSettings(pub Settings);

/// Where the settings file is, whether it may be written (false after an unreadable file was
/// found or a write failed, so the player's file is never clobbered), and what was last written.
#[derive(Resource)]
pub struct SettingsStore {
    pub path: PathBuf,
    pub enabled: bool,
    pub saved: Settings,
}

/// The pause menu while it is open. While it is, the game clock is stopped and every gameplay
/// input system stands aside (see `main.rs`, `pause_ui.rs`).
#[derive(Resource, Default)]
pub struct Pause(pub Option<PauseMenu>);

/// The panel that shows the pause menu.
#[derive(Component)]
pub struct PausePanel;

/// One clickable row of the pause menu; the index matches `PauseMenu::lines`.
#[derive(Component)]
pub struct PauseRow(pub usize);

/// Which side panel (bag, skills, quest book) is open and which page of it is showing.
#[derive(Resource, Default)]
pub struct DockState {
    pub dock: Dock,
    pub page: usize,
}

impl DockState {
    /// Opens `tab` (or closes it if it is the open one) and starts again at its first page.
    pub fn toggle(&mut self, tab: Tab) {
        self.dock.toggle(tab);
        self.page = 0;
    }

    pub fn close(&mut self) {
        self.dock.close();
        self.page = 0;
    }
}

/// The chat area: the message log and the text-entry box.
#[derive(Resource, Default)]
pub struct ChatState {
    pub log: ChatLog,
    pub input: ChatInput,
    /// Whether the box was open when this frame began. Gameplay key handlers stand aside while
    /// it is set, so the Enter that sends a line is not also read as "go to the selected
    /// thing". Written only by `chat_ui::chat_input`.
    pub keys_captured: bool,
}

/// The thing chosen with Tab, if any (see `keyboard.rs`).
#[derive(Resource, Default)]
pub struct KeyTarget(pub Option<Entity>);

/// Recent frame times for the overlay and the exit summary.
#[derive(Resource, Default)]
pub struct PerfStats(pub FrameStats);

/// The frame-time overlay text.
#[derive(Component)]
pub struct PerfHud;

/// The looping ambient sound, so its volume can follow the settings.
#[derive(Component)]
pub struct AmbientSound;

/// Marks everything the current region put into the world (terrain, props, people, the
/// player, click markers). Travelling to another region despawns all of it and builds the new
/// region's world; see `travel.rs`.
#[derive(Component)]
pub struct RegionEntity;

/// The walls of a house (lowered while the player is inside); `0` is the house's id. Houses are
/// found by id, not by position in a list, because the list changes as chunks load and unload.
#[derive(Component)]
pub struct HouseWalls(pub String);

/// The roof of a house (hidden while the player is inside); `0` is the house's id.
#[derive(Component)]
pub struct HouseRoof(pub String);

/// Every playable region's level, in the region's own tile coordinates, plus how the regions
/// are placed in the open world. `LevelData` and `Meadow` hold what is loaded right now (in
/// world coordinates while on the overworld); this keeps the originals.
#[derive(Resource)]
pub struct Atlas {
    pub locations: ozbound::locations::Locations,
    pub levels: Vec<(String, Level)>,
    /// `None` when a custom starting map replaces Bluebrook: regions are then separate places.
    pub overworld: Option<ozbound::overworld::Overworld>,
}

impl Atlas {
    pub fn level(&self, id: &str) -> Option<&Level> {
        self.levels.iter().find(|(region, _)| region == id).map(|(_, level)| level)
    }
}

/// The location id of the region the player is in.
#[derive(Resource)]
pub struct CurrentRegion(pub String);

/// An exit the player has just walked onto. `travel::apply_travel` takes it next frame.
#[derive(Resource, Default)]
pub struct TravelOrder(pub Option<ozbound::level::ExitSpawn>);

/// Present for the frame in which the new region's world must be built.
#[derive(Resource)]
pub struct RebuildWorld;

/// Marks everything a loaded overworld chunk put into the world (terrain, props, people).
/// `stream_chunks` despawns it when the chunk is dropped; the id is the chunk's location id.
#[derive(Component)]
pub struct ChunkEntity(pub String);

/// The wide floor under the world (rebuilt with the world, not part of any chunk).
#[derive(Component)]
pub struct WorldFloor;

/// Which chunks are loaded, and whether the player is on the overworld at all.
#[derive(Resource, Default)]
pub struct Streaming {
    /// Location ids of the chunks that are spawned right now.
    pub loaded: Vec<String>,
    /// True inside a place off the overworld (a cave): `Meadow` and `LevelData` then hold that
    /// one region in its own coordinates and nothing streams.
    pub interior: bool,
}

/// What the player has changed in chunks that are not loaded right now (picked moonpetals,
/// used trees), in world tiles. A chunk's entry is applied when it loads again, so walking
/// away and back keeps the world as it was left (until the game is closed: only the chunk the
/// player stands in is written to the save, see `ozbound::save`).
#[derive(Resource, Default)]
pub struct ChunkMemory(pub HashMap<String, ozbound::save::WorldChanges>);

/// One playing background track of a zone. The volume is `settings * gain`, set every frame.
#[derive(Component)]
pub struct BgmVoice(pub ozbound::overworld::TrackSpec);

/// The zone music: the crossfading mixer, the zone the player was last in, and the loaded track
/// files (read when a chunk that has the zone loads, dropped when no loaded chunk needs them).
#[derive(Resource, Default)]
pub struct Soundscape {
    pub mixer: Option<ozbound::soundscape::Mixer<ozbound::overworld::TrackSpec>>,
    /// The id of the zone (or, off the overworld, the region) the player is in.
    pub zone: Option<String>,
    pub tracks: HashMap<ozbound::overworld::TrackSpec, Option<Handle<AudioSource>>>,
}
