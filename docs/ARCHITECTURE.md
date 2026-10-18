# Architecture Notes

## Current layout

```text
src/
├── main.rs          # Bevy app wiring and system order (binary crate)
├── setup.rs         # Startup: terrain, props, characters, lights, camera, HUD
├── models.rs        # Original low-poly models built from primitives
├── components.rs    # Bevy components and resources
├── pointer.rs       # INPUT: hover label, left-click walk/interact, click markers
├── movement.rs      # MOVEMENT: glides the player along the planned route
├── interact.rs      # WORLD INTERACTION: talk/read/pick/gather on arrival, node respawn
├── animation.rs     # Visual-only: limb swings, floating collectables
├── camera_rig.rs    # Orbit camera that follows the player
├── menu.rs          # INPUT + UI: right-click menu open/choose, draws the panel
├── hud.rs           # UI: objective, bag, equipment, journal, hover label
├── pack_ui.rs       # INPUT + UI: C opens the crafting-and-gear panel, click/1-9 picks a row
├── lib.rs           # Public pure-Rust domain modules (library crate)
├── world.rs         # Tile positions, map bounds, walkability (all collision), 3D tile centers
├── pathfinding.rs   # BFS routes, no corner cutting, Adjacent/OnTile approach
├── picking.rs       # Ray vs ground and boxes: what is under the cursor
├── task.rs          # Route + pending interaction; step and facing helpers
├── orbit.rs         # Orbit-camera angles, clamps, zoom
├── interaction.rs   # Interaction kinds, hover text, per-kind approach, `in_reach` range rule
├── inventory.rs     # Item IDs, stack sizes, slot capacity, all-or-nothing add/remove
├── equipment.rs     # Equipment slots; equip/unequip/swap between bag and slots
├── crafting.rs      # Recipes (parsed/validated), all-or-nothing crafting, recipe file loading
├── pack.rs          # Crafting-and-gear panel as data: rows, applying a row, bag/equipment lines
├── skills.rs        # Skills, XP curve, level requirements, XP sources, reachability check
├── gathering.rs     # Resource nodes, gather transaction, placement, keep-going rule
├── context_menu.rs  # Menu rows, placement on screen, row hit-testing
├── respawn.rs       # One-shot countdown for respawning nodes
├── level.rs         # Level data file parser/validator: map, player, guide, moonpetals, nodes, signs, npcs, places, creatures
├── village.rs       # Loads level + quests + dialogue + bestiary + recipes together and cross-checks them
├── bestiary.rs      # Creatures, field notes, and the non-lethal, all-or-nothing `approach` encounter
├── appearance.rs    # Character looks as data, plus the 16x16 dialogue portrait drawn from them
├── ambient.rs       # Generates the looping ambient WAV in code
├── textdata.rs      # Shared reader for plain-text content files (sections, fields, ids, line-numbered errors)
├── quests.rs        # Quest definitions (parsed/validated), QuestLog transitions, events and updates
├── dialogue.rs      # Dialogue conversations/nodes/choices, conditions and effects, DialogueSession
├── journal.rs       # Active/completed quest view built from the quest book and log
├── quest_save.rs    # Versioned plain-text save/load of the QuestLog (no file I/O)
├── quest_store.rs   # Reads the old quest-only save file (pre-M6) for migration
├── save.rs          # The full game save: SaveGame, versioned text format, validated loading, WorldChanges
├── save_store.rs    # Reads/writes the save file (temp file + rename, backup, bad saves never overwritten, legacy migration)
├── content.rs       # Loads quest + dialogue files (custom path, default path, built-in fallback)
├── talk.rs          # One open conversation: current view, choices, journal lines per step
├── scene.rs         # Built-in default level (compiled in), per-tile variation, fence rail links
└── quest.rs         # Original moonpetal counter (only used by level validation now)
```

## Boundaries

- The **library crate** (`lib.rs` and the modules it lists) is pure Rust: no Bevy types, fully unit-tested without a window. The **binary crate** (`main.rs` and its modules) renders, reads input, and connects the two.
- System modules have one job each, and they run in this order every frame: **input** (`pointer`, `menu::menu_input`) turns the mouse into orders; **movement** (`movement::follow_route`) walks the route; **world interaction** (`interact::interact`, `interact::respawn_nodes`) acts on arrival; **animation** is cosmetic; **UI** (`menu::sync_menu_ui`, `dialogue_ui`, `hud`) draws state; `persist::persist_game` saves the game about once a second when it changed, and `persist::save_on_exit` (in `Last`) once more on exit. A conversation is modal: while `TalkState` is open, `pointer` and `menu` stand aside, `dialogue_ui::dialogue_input` takes clicks and 1-9, and Esc or walking away closes it. The crafting panel (`PackState`, toggled with C by `pack_ui::pack_input`) owns the mouse the same way, closes on C or Esc, and gives way to a conversation. Only input writes the player's `Task`; only `interact` changes quest state and nodes; the inventory changes in `interact` (gathering, hand-ins) and in `pack_ui` (crafting, equipping), both through the tested pure modules.
- Input flow: `pointer.rs` turns the cursor into a ray, `picking::pick` says what it hits, `pathfinding::plan_route` plans a route, and `Task` stores it. `actions.rs` glides the player along the route and runs the interaction on arrival.
- World space is 3D, Y-up. One tile is one world unit (`TILE_SIZE` = 1.0). Map tile `(x, y)` sits at world `(x, h, -y)`: map "north" is Bevy's forward direction (`-z`). Use `WorldMap::tile_center_3d` and `picking::ground_to_tile` to convert; text maps list the top row first.
- Gameplay is tile-based: walkability, routes, and interaction ranges ("stand on or orthogonally beside the target tile") are decided on tiles. Smooth motion is display only: the logical `Player.tile` updates as each tile is reached.
- A new click plans from the tile being walked toward, and the stride in progress is kept, so the player never snaps backwards.
- Collision has one source of truth: `Tile::is_walkable`. Water, trees, rocks, fences, and signposts all block, and pathfinding, click validation, and level validation all ask the map, so the 3D models (including fences) never decide what can be walked through.
- Interaction range has one source of truth too: `InteractionKind::approach()` says where the player must stand (on the tile for ground pickups, orthogonally beside it for everything else), the planner walks there, and `interaction::in_reach` is re-checked when the player arrives and before every gather attempt, so an object can never be used from further away than the planner would have walked to.
- Fences draw a rail toward each neighbouring fence tile (`scene::fence_links`), so a row of `F` tiles looks like one fence. Signposts are `S` tiles; an entry in the level's `[signs]` section makes one readable, and an `S` tile without an entry is plain scenery.
- Collision treats anything outside the map as blocked, so no separate position clamp is needed.
- Gathering attempts add the yield to inventory before decrementing node availability, so a full inventory never consumes a resource. Repeat gathering stops when `should_keep_gathering` says so (spent node, full bag, or any failure) or when the player starts walking.
- Spent trees and veins get a `Respawning` timer the moment an action empties them; when it finishes `GatheringNode::restock` refills them and their visual is shown again.
- Right-click opens a menu that owns the mouse while open (`pointer` skips input), so one click never both picks a row and walks. Row geometry comes from `MenuMetrics`, shared by the UI layout and the click test.
- Inventory capacity counts occupied slots. One slot holds up to `ItemId::max_stack()` of one item (99 for ordinary items, 1 for equipment), so a pile of 150 timber takes two slots. Adding more of an item uses the spare room in its last stack first.
- Full-bag, depleted-node, crafting, and equipment rules (every failure changes nothing and says why):

  | Situation | Result |
  | --- | --- |
  | Node is spent | `GatherError::Depleted`, checked before the bag, so "make room" is never suggested for a spent plant. It restocks when its respawn timer finishes. |
  | Bag has no room for the yield | `GatherError::Inventory(Full)`; the node keeps its supply. A stack with spare room still takes more of its item. |
  | Craft needs a skill level the player lacks | `CraftError::SkillTooLow` lists every unmet requirement; checked before ingredients; nothing is taken and no XP is given. |
  | Craft is missing ingredients | `CraftError::Missing` lists every shortfall; nothing is taken. |
  | Craft result does not fit | `CraftError::NoRoom`; nothing is taken. Inputs the craft uses may free the slot it needs, so this is decided on a copy of the bag. |
  | Equip into an occupied slot | Swap; the old item returns to the bag. Equipment never stacks, so this cannot fail for room. |
  | Unequip with a full bag | `EquipError::BagFull`; the item stays equipped. |
- Visuals are generated in code (flat-shaded primitives), so there are no external assets or licences to track. The world is described by a plain-text level file (`assets/maps/meadow.map`, parsed by `level.rs`); a copy is compiled in as a fallback for when the file is not found. An invalid level file stops startup with a file/line error rather than falling back.

- Quests and dialogue are plain-text content (`assets/quests/*.quest`, `assets/dialogue/*.dialogue`) parsed by pure modules. Every parser error carries a line number, and loading validates the whole file before anything is used, so a bad file never half-loads. Dialogue is also checked against the quest file (`DialogueBook::check_against`).
- Quest state changes are all-or-nothing. `QuestLog::complete` works on a copy of the bag and only commits if the items are there and the rewards fit; a failed hand-in changes nothing and the quest stays ready. Quest progress comes only from `QuestEvent`s (items gathered, NPC spoken to), never from what happens to be in the bag.
- A conversation can never trap the player: content validation requires an always-available choice on every node and an unconditional route to the end from everywhere. Dialogue effects that fail (for example a full bag at hand-in) are returned in `Outcome::failures`, and content routes those cases with the `can_complete` condition.
- Recipes are plain text too (`assets/recipes/*.recipes`, `crafting::RecipeBook::parse`) and load like the other content files (`crafting::load`). Plants that grow on open ground (herbs, berries) are `InteractionKind::Forage` with `Approach::OnTile`; level validation checks their own tile is reachable.
- Saves are text with a version header. `QuestLog::from_text` refuses states that play could not have produced and never loads partially. The full save (`save.rs`) embeds the quest log's own lines and adds everything else; `save_store.rs` writes it to disk (see below).

- The Hollowmere world is several files that must agree, so `village::check` validates them together after each parses on its own: every person in the level has a conversation and vice versa, `talk`/`visit`/`befriend` targets exist, every quest has a dialogue node that starts it and one that hands it in, and every item a quest collects, crafts, or takes can actually be had (ignoring quest rewards, so a quest never depends on its own reward). Quest events come from the Bevy layer and nowhere else: `interact` (gathered, befriended), `pack_ui` (crafted, from what actually arrived in the bag), and `movement` (visited, when a tile is reached).
- Portraits and 3D models share one `Appearance`, and the ambient sound is synthesised, so the game still ships no art or audio files. Creature approaches and quest events never change anything unless the pure rule says so (`bestiary::approach` takes the offering only if the gift fits).

- The save (M6) is one file, `saves/game.txt`, read once at startup (`main::restore_game`) and written by `persist.rs`. `save.rs` owns the format and every validation rule (the save is checked against the loaded level, quests, and bestiary, and never loads partly); `save_store.rs` owns the file safety (temp file + flush + rename; an invalid file is never overwritten; `.bak` copy after a good load). `setup.rs` reads `StartingSave` to place the player and to skip picked moonpetals and restore used nodes; after that the ECS is the live state and `persist.rs` snapshots it (`WorldChanges::capture`) and compares with what was last written. Respawn timers are deliberately not saved: a node saved as spent starts regrowing again at load.
- To change the format: bump `save::FORMAT_VERSION`, keep reading every older version, add a test that loads a saved sample of each, and update the header of `save.rs`.

## Testing strategy

- Unit-test pure rules, including invalid inputs and edge cases.
- Add integration tests for multi-step quests and save/load as those features arrive.
- Keep manual smoke-test instructions for windowing, rendering, input, and platform-specific behavior.
- Every roadmap feature must list tests and acceptance criteria before coding.

## Technical choices

- Rust + Bevy 0.15 for the game loop and 3D rendering. Building needs Rust 1.88 or newer (the locked `image` crate requires it; Bevy 0.15 itself needs 1.82).
- Pure domain logic wherever practical, to keep tests fast and independent of a window.
- Local-first: saves and content stay on the user's device. Networking is not part of the initial design.
- Minimal dependencies; review licensing and maintenance before adding crates.
