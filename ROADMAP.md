# Ozbound Roadmap

A milestone is complete only when its acceptance criteria pass and tests are added or updated. Features should be implemented in small, reviewable increments.

## M0 — Foundation and playable prototype
- [x] Rust + Bevy project structure.
- [x] Top-down meadow scene with a controllable player.
- [x] Basic interaction prompt near an NPC.
- [x] Moonpetal collection objective and quest HUD.
- [x] Unit tests for quest progress.
- [ ] Verify `cargo run` and `cargo test` on macOS, Windows, and Linux.
  - Tests: record platform-specific build/test results.
  - Acceptance: application launches and all tests pass on each supported platform.


## M0.2 — World and gathering foundations
- [x] Add a pure-Rust rectangular tile map with bounds and walkability checks.
- [x] Add stack-based inventory with slot capacity and safe add/remove operations.
- [x] Add generic gathering nodes for moonpetals, timber, and copper ore.
- [x] Ensure failed inventory additions do not consume resource nodes.
- [x] Add shared interaction-range logic.
- [x] Integrate moonpetal pickup with inventory and HUD counts in the Bevy prototype.
- [x] Add unit tests for world bounds/walkability, inventory capacity/stacking/removal, gathering yields/depletion, and interaction range.
- [x] Integrate map tiles and collision into the rendered world; expose tree and ore nodes in the scene.
  - Done: `scene.rs` defines the meadow (text map), `movement.rs` resolves collision against it, trees and copper veins are gatherable in the Bevy scene via `E`.
  - Tests: tile collision, tunneling, map edges, text-map parsing, node placement validation, nearest-in-range selection, and meadow reachability (all pure Rust).
  - Acceptance: player cannot move into blocked tiles; gathering is available only in range; full inventory preserves resources.
  - Manual check (needs a window; not covered by unit tests): walk into trees, rock, water, and the map edge and confirm you stop; stand beside a tree/ore and press `E` to gather; fill the bag and confirm the node is not consumed.

## M0.3 — Low-poly 3D and click-to-move
- [x] Change direction from top-down keyboard play to a low-poly 3D world with mouse-driven, tile-based controls.
- [x] Pure-Rust tile pathfinding: shortest routes, diagonal steps that never cut blocked corners, and an `Adjacent` approach for blocked targets (trees, ore, NPCs).
  - Tests: straight and diagonal routes, corner cutting, walls and gaps, blocked/off-map/enclosed goals, already-adjacent targets, agreement with `is_reachable` on every meadow tile.
- [x] Pure-Rust mouse picking: ray vs ground plane and ray vs boxes; nearest object wins unless the ground is nearer.
  - Tests: oblique and vertical rays, parallel/upward rays, box entry distance, origin inside box, NaN input, ties, ground-to-tile round trip.
- [x] Click-to-walk, click-to-interact (walk beside the target, then act), automatic repeat gathering, hover action label, and click markers (yellow = walk, red = interact).
  - Tests: route/target task state (target handed out once, only after arrival; new clicks finish the current stride), gather-loop stop conditions, hover text.
- [x] Third-person orbit camera that follows the player (middle-drag or arrows to turn, scroll to zoom).
  - Tests: default view direction, offset length, clamping, yaw wrap, invalid input.
- [x] Original low-poly models generated in code (terrain, trees, boulders and copper ore, moonpetals, characters with walk and work animation).
- [x] Split the Bevy layer into focused modules (setup, models, pointer, actions, camera, HUD).
- [x] Retire pixel-space continuous collision (`movement.rs`); `TILE_SIZE` is now 1.0 world unit.
- [x] Esc quits (documented before, but not previously handled).
  - Acceptance: clicking ground walks a legal route; clicking an object walks beside it and then acts; a full bag never consumes a node; `cargo clippy --all-targets -- -D warnings` is clean.
  - Headless smoke run (Xvfb + software rendering, real mouse events) confirmed: hover labels, click-to-pick a moonpetal, chop a tree until spent, water refusal, guide dialogue, camera rotate/zoom, Esc quits. Still to check by eye: copper mining, arrow-key camera, mid-walk re-click, full bag, real-GPU frame rate.
  - Manual check (needs a window; not covered by unit tests): click grass, a path, and a blocked tile and confirm walk / walk / "You can't walk there."; click a tree and confirm the red cross, the walk, and repeated "You gathered..." messages, with the crown disappearing when spent; click mid-walk and confirm the player does not snap backwards; click the guide, then each moonpetal; fill the bag and confirm nodes are not consumed; turn the camera all the way round and zoom in and out; confirm hover text matches what a click does.
- [x] Right-click context menu (main action / Examine / Walk here / Cancel) as in classic MMORPGs.
  - Done: `context_menu.rs` (pure) builds the rows, keeps the menu inside the window, and maps a click to a row; `menu.rs` opens it, runs the choice, and draws the panel. A left-click while it is open picks a row and never also walks; a right-click elsewhere reopens it.
  - Tests: row order and labels, Cancel always last, blocked ground omits Walk here, flip/clamp placement, row hit-testing at edges, degenerate input, examine text for fresh and spent resources.
  - Manual check (needs a window): right-click a tree, the guide, a moonpetal, grass, and water; confirm the rows, that Walk here is absent on water, that the menu stays on screen near each window edge, that hovering highlights a row, that Examine only prints to the journal, and that clicking outside the menu closes it without moving the player.
- [x] Respawn timers for spent trees (30 s) and copper veins (45 s); moonpetals do not respawn.
  - Done: `respawn.rs` (pure one-shot countdown), `GatheringNode::restock` (only refills a spent node), `ResourceKind::respawn_seconds`, and the `respawn_nodes` system.
  - Tests: countdown fires once, large steps, zero and invalid durations/time steps, restock only when spent, zero-capacity nodes, per-kind durations.
  - Manual check (needs a window): chop a tree until spent, wait about 30 s, and confirm the crown returns and the tree can be chopped again.
- [ ] Polish pass on models and lighting after seeing them in a window.

## M1 — World movement and interaction
- [x] Replace the demo meadow with a tile-based map loaded from a local data file.
  - Done: `assets/maps/meadow.map` (plain text: `[map]`, `[player]`, `[guide]`, `[moonpetals]`, `[nodes]`; format documented at the top of the file) is parsed by the pure `level.rs` into a validated `Level`. `main.rs` loads it at startup (`OZBOUND_MAP=path` plays another file); if the default file is absent the identical compiled-in copy (`scene::DEFAULT_LEVEL`) is used so the game still starts from any directory. `setup.rs` builds the scene from the `Level` instead of constants, and `scene.rs` keeps only the built-in default and the per-tile variation helpers.
  - Errors: an invalid file stops startup with the file name, 1-based line, and a plain explanation (unknown glyph with column, ragged row, malformed coordinates, unknown resource, duplicate section, tile out of bounds, wrong tile for a tree/vein, unreachable guide/moonpetal/node, fewer moonpetals than the quest needs, node that never yields). An existing but invalid file is never silently replaced by the built-in map.
  - Tests: the shipped file parses and equals the original hand-written layout; the on-disk file equals the built-in copy; y-counts-up-from-bottom convention; comments, blank lines, CRLF, and case-insensitive headers; every error kind above with its line number; missing/unreadable/invalid files report their path. The old meadow-layout tests (size, reachability, routes to every target) moved to `level.rs` and run against the parsed level.
  - Acceptance: the game builds its world from a data file; map data errors are reported clearly with a line number; trees, rocks, and water from the file block movement as before.
  - Manual check (needs a window; not covered by unit tests): `cargo run` and confirm the meadow looks and plays as before; edit a glyph in `assets/maps/meadow.map` (for example turn a `.` into `~`) and confirm it shows up; break the file (add a `?` to a map row) and confirm the game exits with `invalid level file ... line N` instead of opening a window; run from another directory and confirm the built-in-meadow note appears.
- [x] Add collision against trees, rocks, fences, and water.
  - Done: new `F` (fence) and `S` (signpost) tiles in `world.rs`. `Tile::is_walkable` is the one place collision is decided; pathfinding, click validation, and level validation all use it. Fences render as posts with rails toward neighbouring fence tiles (`scene::fence_links`, `models::spawn_fence`); the meadow now has a five-tile fence rail south of the pond.
  - Tests: every tile kind's walkability, fence/sign glyph parsing, fence lines blocking and gaps letting through, routes never entering any blocker, fences approached from beside like other blocked targets, fence links (ends, corners, lone posts, map edge), and the meadow staying fully connected with the rail in place.
  - Manual check (needs a window): walk toward the fence rail south of the pond from every side and confirm the player routes around it; click a fence and confirm "You can't walk there."; click ground beyond it and confirm the route bends around the ends.
- [x] Add interactable objects and a consistent interaction range.
  - Done: readable signposts (`InteractionKind::Read`, `[signs]` section in level files). `InteractionKind::approach()` is the single rule for where the player must stand; `interaction::in_reach` is checked again on arrival and before each gather attempt, and a failed check says "You are too far away to do that." Setup builds every object's `Approach` from `approach()` instead of hard-coding it.
  - Tests: reach for both approaches (orthogonal only, diagonals and distance refused, symmetric, extreme coordinates), planned routes always end in reach for the guide, petals, nodes, and signs, sign parsing (spaces in text, malformed lines with line numbers), and sign validation (wrong tile, off map, duplicates, walled off). Unlisted `S` tiles are valid scenery.
  - Manual check (needs a window): click each signpost and confirm the player walks beside it and the journal shows its text; right-click one and confirm "Read signpost" and Examine; click a tree, then click far ground mid-chop and confirm chopping stops.
- [x] Separate input, movement, world interaction, and UI systems into focused modules.
  - Done: `actions.rs` is split into `movement.rs` (route following), `interact.rs` (arrival actions, gathering, respawn), and `animation.rs` (cosmetic only). Input stays in `pointer.rs` and `menu.rs`; UI in `menu.rs` and `hud.rs`. `docs/ARCHITECTURE.md` lists each module's single job and the per-frame order.
- [x] Add unit tests for map parsing, walkability, movement bounds, and interaction range.
  - Covered by the tests above plus the existing parser, bounds, and range tests; map errors keep their file and line numbers, including the new `[signs]` section and glyph legend.
  - Acceptance: player cannot pass through blocked tiles; map data errors are reported clearly; interactions only succeed within range.

## M2 — Quest and dialogue foundations
- [ ] Add data-driven dialogue nodes and choices.
- [ ] Add quest definitions, prerequisites, objectives, and completion rewards.
- [ ] Add a journal that lists active and completed quests.
- [ ] Add quest state persistence independent of rendering.
- [ ] Test dialogue branching, prerequisite checks, quest transitions, and invalid content.
  - Acceptance: a complete short quest can be started, advanced, completed, and verified through tests without launching Bevy.

## M3 — Inventory, gathering, and crafting
- [ ] Add stackable items, equipment slots, and inventory capacity.
- [ ] Add gathering nodes such as herbs, berries, and timber.
- [ ] Add simple recipes and a crafting interface.
- [ ] Define clear rules for full inventory and depleted gathering nodes.
- [ ] Test item stacking, capacity, gathering yields, recipe requirements, and crafting results.
  - Acceptance: all inventory rules are deterministic and covered by unit tests.

## M4 — Skills and progression
- [ ] Add skills such as Foraging, Woodcraft, Herbalism, and Wayfinding.
- [ ] Use a documented, deterministic XP curve.
- [ ] Gate selected recipes and quest options behind skills.
- [ ] Keep progression useful but avoid grind for its own sake.
- [ ] Test XP thresholds, level-ups, caps, and skill requirements.
  - Acceptance: skill progression can be simulated and tested without rendering.

## M5 — World content vertical slice
- [ ] Build an original starting village and nearby meadow/forest.
- [ ] Add 3–5 short quests with different objective types.
- [ ] Add a small bestiary and non-lethal/optional encounters as appropriate to the tone.
- [ ] Add readable original low-poly NPC models, portraits for dialogue, and ambient sound.
- [ ] Test each quest path, item reward, and key progression gate.
  - Acceptance: a new player can complete a 30–60 minute local adventure without blockers.

## M6 — Local save/load
- [ ] Define a versioned save schema.
- [ ] Save position, inventory, skills, quest state, and world changes to a local file.
- [ ] Add safe writes (temporary file + replace) and clear recovery errors.
- [ ] Test round trips, schema versions, missing/corrupt files, and defaults.
  - Acceptance: progress survives restart; invalid saves fail safely without overwriting recoverable data.

## M7 — Polish and accessibility
- [ ] Add key remapping and adjustable UI scale.
- [ ] Add a keyboard path for click-to-move and click-to-interact (for example Tab to cycle nearby targets, Enter to act), so mouse-first play is not required.
- [ ] Add readable dialogue, quest, inventory, and settings panels.
- [ ] Add pause menu and audio controls.
- [ ] Add performance profiling and a basic smoke-test checklist.
- [ ] Test settings validation and UI state transitions where practical.
  - Acceptance: all essential actions are reachable without a mouse; no critical text is clipped at supported resolutions.

## M8 — Optional future directions (not committed)
- [ ] Consider controller support.
- [ ] Consider a larger world split into local map regions.
- [ ] Evaluate multiplayer only if there is a compelling reason; do not make it a prerequisite.
- [ ] Consider mod-friendly plain-text content definitions.
- [ ] Test compatibility and migration before changing any public data formats.

## Definition of done for every feature

1. Acceptance criteria are written before implementation.
2. Unit tests are added first or alongside implementation for game rules and edge cases.
3. Regression tests cover any bug fixed.
4. Manual steps are documented for visual/platform behavior that cannot be unit-tested.
5. `cargo fmt --check`, `cargo test`, and `cargo clippy --all-targets --all-features -- -D warnings` pass where the local toolchain supports them.
6. Dependencies and architectural complexity are justified; prefer standard Rust and Bevy facilities over extra frameworks.
