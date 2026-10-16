//! Bevy components and resources shared by the game systems.

use bevy::prelude::*;
use ozbound::{
    context_menu::MenuEntry,
    gathering::GatheringNode,
    interaction::InteractionKind,
    inventory::Inventory,
    level::Level,
    pathfinding::Approach,
    quest::MoonpetalQuest,
    respawn::RespawnTimer,
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

#[derive(Resource, Default)]
pub struct GameProgress {
    pub quest: MoonpetalQuest,
    pub spoke_to_guide: bool,
    pub journal: String,
    pub inventory: Inventory,
}

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
}

#[derive(Component)]
pub struct Guide;

#[derive(Component)]
pub struct Moonpetal;

/// A tree or copper vein backed by the tested gathering rules.
#[derive(Component)]
pub struct ResourceNode(pub GatheringNode);

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
