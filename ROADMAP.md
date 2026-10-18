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
- [x] Add data-driven dialogue nodes and choices.
  - Done: `dialogue.rs` (pure) parses `assets/dialogue/meadow.dialogue` into conversations of nodes and choices. Choices can carry conditions (`quest <id> is|not <status>`, `has <item> <n>`, `can_complete <id>`), nodes can run effects (`start <quest>`, `complete <quest>`), and a conversation's `entry` list picks the opening node from the current quest state. `DialogueSession` walks a conversation (`begin`, `view`, `choose`, `close`); talking to an NPC records that for `talk` objectives. Loading rejects anything that could trap the player: missing or hidden entries, nodes with no text or no always-available choice, unknown or unreachable nodes, and loops with no unconditional way out.
  - Tests: parsing of every field and condition, arrows and "if" inside choice text, every syntax error with its line number, every structural error above, quest ids checked against the quest file, choice indexes with hidden choices, effects that cannot apply being reported rather than hidden.
- [x] Add quest definitions, prerequisites, objectives, and completion rewards.
  - Done: `quests.rs` (pure) parses `assets/quests/meadow.quest` into a validated `QuestBook` (unique ids, known prerequisites, no cycles, at least one objective, positive amounts). Objectives are `talk <npc>` and `collect <item> <n>`, done in order. A quest can take items on hand-in and grant rewards. `QuestLog` does the transitions: `start`, `record` (events in, `QuestUpdate`s out), `complete`. Rewards never count as gathering, and surplus from one event is not carried over (rules are written at the top of `quests.rs`).
  - Tests: parsing and every content error, prerequisite checks (including a still-active prerequisite), ordered objectives, ignored and zero-quantity events, one event advancing several quests, capped surplus, completion taking items and granting rewards once, and all-or-nothing failure for missing items and a full bag.
- [x] Add a journal that lists active and completed quests.
  - Done: `journal.rs` (pure) builds `Journal { active, completed }` from the quest book and log, with finished steps, the current step with its count, and a hand-in step for ready quests; future steps stay hidden. `Journal::render` gives the same as text.
  - Tests: empty journal, hidden future steps, ready and completed placement, exact rendered text, quests missing from the book.
- [x] Add quest state persistence independent of rendering.
  - Done: `quest_save.rs` adds `QuestLog::to_text` / `from_text`, a versioned (`ozbound-quests 1`) plain-text format. Loading validates against the quest definitions and refuses unknown or duplicate quests, impossible progress, newer format versions, and quests started without their prerequisites. It does no file I/O; safe file writes belong to M6.
  - Tests: round trips of every stage, play continuing after a load, comment/CRLF/BOM tolerance, every refusal with its line number, no partial loads.
- [x] Test dialogue branching, prerequisite checks, quest transitions, and invalid content.
  - Acceptance: a complete short quest can be started, advanced, completed, and verified through tests without launching Bevy.
  - Done: `tests/quest_playthrough.rs` plays the shipped "Moonpetals for the Apothecary" through the public API: accept at the guide, gather with real `GatheringNode`s, save and reload mid-quest, early hand-in does nothing, hand-in takes petals and pays timber once, journal and later dialogue change, a full bag delays the reward without losing anything, and declining starts nothing.
- [x] Wire the new modules into the Bevy layer.
  - Done: `content.rs` loads both files with the level's rules (custom path must load, absent default falls back to the built-in copy, invalid file stops startup with file and line). `talk.rs` wraps `DialogueSession`; `dialogue_ui.rs` draws the panel and takes clicks or 1-9; a conversation is modal and ends on Esc or walking away. `interact.rs` sends `QuestEvent::Gathered` after every successful pickup, chop, or mine. `hud.rs` shows a quest tracker and a journal panel (J, `Journal::render`). `quest_store.rs` + `persist.rs` save the quest log to `saves/quests.txt` on change (temp file + rename; an unreadable save is reported and never overwritten). `MoonpetalQuest` is gone from the Bevy layer (still used by level validation). The guide's "bag full" line now also covers missing petals, since the bag is not saved before M6.
  - Tests: content loading (fallbacks, custom/invalid files, quest/dialogue mismatch), quest save file (round trip, missing/invalid/unwritable, no leftover temp file), update/failure/tracker messages, and `Talk` (offer, accept, decline, bad index, hand-in once, talk objectives).
  - Headless smoke run (Xvfb + software rendering, real mouse and key events) confirmed: walk to the guide, dialogue panel, accept by click, quest tracker and journal panel, save file written, restart shows "Welcome back" with the quest restored, Esc quits. Still to check by eye: number keys inside a conversation, Esc closing a conversation, hand-in with timber, full-bag reply, an invalid save file.
  - Manual check (needs a window): pick petals and watch the counts; talk early and confirm nothing is handed over; fill the bag and confirm the guide asks for room; hand in and confirm timber appears; break `saves/quests.txt` and confirm the game warns and does not overwrite it.

## M3 — Inventory, gathering, and crafting
- [x] Add stackable items, equipment slots, and inventory capacity.
  - Done: `inventory.rs` counts capacity in slots. One slot holds up to `ItemId::max_stack()` of an item (99 for ordinary items, 1 for equipment), so 150 timber take two slots and each walking staff takes one. `can_add` answers without changing anything and `add`/`remove` are all or nothing. New items: meadowmint, honeyberry, mint tea, walking staff, copper charm (existing item keys are unchanged). `equipment.rs` adds five slots (head, body, feet, hand, charm); equipping moves one item from the bag into its slot, swaps with whatever was there, and taking an item off needs a free bag slot.
  - Tests: stack spill and spare-room rules, atomic oversized adds, overflow, zero-slot bags, removal freeing slots, equipment never stacking, unique item keys and labels, frozen legacy keys; equip/unequip/swap, not-equippable and not-in-bag errors, full-bag refusal, and the invariant that a swap can never fail for room.
- [x] Add gathering nodes such as herbs, berries, and timber.
  - Done: meadowmint patches (`herb`) and honeyberry bushes (`berry`) are new node kinds in `[nodes]` sections. They stand on walkable ground and are gathered by standing on them (`InteractionKind::Forage`, `Approach::OnTile`), repeat automatically like chopping, and grow back (20 s and 25 s). Level validation checks the tile itself is reachable for these, not just a neighbour. The shipped meadow has two of each.
  - Tests: kind-to-item/label/respawn/interaction mapping, placement on walkable ground only, parsing, duplicates, never-yielding nodes, sealed-off plants, error text listing the valid kinds, shipped meadow routes to every ground node.
- [x] Add simple recipes and a crafting interface.
  - Done: `crafting.rs` parses `assets/recipes/meadow.recipes` (`[recipe id]` with `name`, repeated `input = item amount`, one `output = item amount`) into a validated `RecipeBook`; three recipes ship (walking staff, copper charm, mint tea). Loading follows the other content files: `OZBOUND_RECIPES` must load, the default file is used if present, an absent default falls back to the built-in copy with a note, and an invalid file stops startup with file and line. `pack.rs` turns recipes, bag, and gear into panel rows and applies a chosen row; `pack_ui.rs` draws it. Press `C` for the panel, click a row or press 1-9. The panel owns the mouse while open; `C` or `Esc` closes it.
  - Tests: every parse error with its line number, duplicate ids/inputs, output-from-itself, empty files; crafting results; `status` and `craft` always agree; panel rows and messages for every case, including that an enabled row always changes something and a disabled row never does; recipe loading (default, absent, invalid, custom).
- [x] Define clear rules for full inventory and depleted gathering nodes.
  - Done: the rules are written at the top of `inventory.rs`, `equipment.rs`, and `crafting.rs`, and summarised in `docs/ARCHITECTURE.md`. In short: a spent node says so before the bag is looked at; a full bag never consumes a node, but a stack with spare room still takes more of its item; crafting needs every input and room for the result (inputs it uses may free that room); taking gear off needs a free slot; every failure changes nothing and says why.
- [x] Test item stacking, capacity, gathering yields, recipe requirements, and crafting results.
  - Acceptance: all inventory rules are deterministic and covered by unit tests.
  - Done: `tests/crafting_playthrough.rs` plays the rules through the public API: forage, gather, craft every shipped recipe, equip; full-bag and depleted-node behaviour; crafting that needs the slots it frees; refused craft keeping its inputs; gear removal needing room while a swap does not; and a check that the meadow supplies every recipe's ingredients and that one of everything fits in a default bag.
  - Manual check (needs a window; not covered by unit tests): click a meadowmint patch and a honeyberry bush and confirm you walk onto them, forage repeatedly, and the crop disappears when spent and returns after about 20-25 s; press `C` and confirm the panel lists three recipes greyed out; gather three timber and confirm "Make walking staff" turns bright, click it, then click "Equip walking staff" and confirm the bag line moves it to "Equipped: hand: walking staff"; fill the bag and confirm foraging says the inventory is full without consuming the plant and that a craft with no room refuses and keeps the ingredients; confirm clicks on panel rows never walk the player, and that `Esc` closes the panel before it quits.
  - Not yet: the bag and equipment are not saved (that is M6), and equipment has no gameplay effect yet (skills and stats are M4).

## M4 — Skills and progression
- [x] Add skills such as Foraging, Woodcraft, Herbalism, and Wayfinding.
  - Done: `skills.rs` (pure Rust) holds the four skills and a `Skills` value (XP per skill; level follows from XP). Gathering teaches Foraging (moonpetals, honeyberries), Herbalism (meadowmint), and Woodcraft (trees, copper ore); each craft teaches the skill named in its recipe; every tile walked counts toward Wayfinding. The HUD and the `C` panel show a `Skills:` line with levels and progress. `GameProgress` owns the skills.
- [x] Use a documented, deterministic XP curve.
  - Done: total XP to reach level L is `30 x (L-1)^2`, levels 1-10 (0, 30, 120, 270, 480, 750, 1080, 1470, 1920, 2430), tabulated in the `skills.rs` docs. XP is capped at the level-10 total. No randomness, timers, or bonuses.
- [x] Gate selected recipes and quest options behind skills.
  - Done: recipe files take `require = skill level` (copper charm needs Woodcraft 3, mint tea needs Herbalism 2); a locked recipe stays in the panel with what it needs and is refused before ingredients are checked. Dialogue takes `skill <id> at_least|below <n>`. The guide offers the new "Mint for the Apothecary" quest only at Herbalism 2 (and says what to practise before then), and an "anywhere worth exploring?" option at Wayfinding 2. `skills::check_gates` runs at startup and refuses content that asks for a level nothing teaches.
- [x] Keep progression useful but avoid grind for its own sake.
  - Done: every XP source has an `until` level and stops teaching once the skill reaches it (trees stop at Woodcraft 4, meadowmint at Herbalism 4, walking at Wayfinding 3), so higher levels need the gated recipes, and repeating one easy action cannot max a skill. Gates are checked for reachability. No skill points, decay, or daily limits.
- [x] Test XP thresholds, level-ups, caps, and skill requirements.
  - Acceptance: skill progression can be simulated and tested without rendering.
  - Done: unit tests for the curve table and round trips, level-up reporting (single and multi-level), the XP cap and overflow, per-skill tracking, requirement parsing and errors, the no-grind ceiling, walking XP in bulk and one tile at a time, recipe and dialogue gating, reachability chains, and gate-check errors. `tests/progression_playthrough.rs` plays the unlocks through the public API.
  - Manual check (needs a window): chop trees and watch the HUD `Skills:` line and a "Your Woodcraft level is now 2." message; press `C` and confirm the copper charm reads "needs Woodcraft level 3" until then; after the first quest, talk to the guide at Herbalism 1 and 2 and compare the options; walk around and confirm Wayfinding creeps up.
  - Not yet: skills are not saved (M6), and equipment still has no gameplay effect.

## M5 — World content vertical slice
- [x] Build an original starting village and nearby meadow/forest.
  - Done: `assets/maps/hollowmere.map` (40 x 24): the meadow in the south-west, a river with a one-tile bridge on the main road, a village east of it (fenced apothecary garden, woodcutter's yard, notice boards, a village street), and a forest along the north edge with gatherable trees. The level format gained `[npcs]` (id, tile, appearance, name), `[places]` (named spots for `visit` objectives), and `[creatures]`; each is validated for valid unique ids, walkable and reachable tiles, and no two things on one tile. The game now starts in Hollowmere; `meadow.map` and its quest/dialogue files stay as the small, fully tested tutorial fixtures.
  - Not yet: the village has no buildings (only fences, signs, standing stones, and people); a cottage tile and model belong to the polish pass.
- [x] Add 3–5 short quests with different objective types.
  - Done: five quests in `assets/quests/hollowmere.quest` using five objective types: *Moonpetals for the Apothecary* and *Mint for the Apothecary* (`collect`, the latter gated on Herbalism 2), *A Staff for the Bridgewarden* (`craft`), *Waymarks of Hollowmere* (`visit` x3 then `talk`, gated on Wayfinding 2), and *The Shy Hare* (`befriend` then `talk`). New objectives: `craft <item> <n>`, `visit <place>`, `befriend <creature>`, fed by `QuestEvent::{Crafted, Visited, Befriended}`.
  - Tests: parsing and every malformed form with its line number, journal wording, events counting only while the objective is current, wrong item/place/creature ignored, zero amounts ignored, hand-in of crafted items.
- [x] Add a small bestiary and non-lethal/optional encounters as appropriate to the tone.
  - Done: `bestiary.rs` + `assets/bestiary/hollowmere.bestiary`: a meadow hare, a pond heron, and a lantern moth. There is no combat. Approaching a creature marks it seen; offering what it likes (taken from the bag) befriends it once and may give a gift, and nothing is taken unless the gift fits. Creatures are optional apart from the one quest that asks for the hare. The journal (J) lists field notes.
  - Tests: parsing and errors, no-offering/too-few/right-offering paths, one befriending per creature, atomic gift-room rule (including the slot freed by the offering), unknown creatures, field-notes text.
- [x] Add readable original low-poly NPC models, portraits for dialogue, and ambient sound.
  - Done: `appearance.rs` holds each person's colours and hat shape as data; the 3D model and a 16 x 16 pixel-art dialogue portrait are both built from it, so a character matches their portrait. Four villagers join the guide (apothecary, woodcutter, bridgewarden, surveyor), each with a distinct look; creatures and standing-stone waymarks have small original models. `ambient.rs` generates a looping wind-and-birdsong WAV in code (no audio files to license) that plays quietly.
  - Tests: every appearance key known and distinct, portrait size/opacity/colours/silhouette, WAV header and length, deterministic, quiet but audible, silent edges so repeats do not click.
- [x] Test each quest path, item reward, and key progression gate.
  - Done: `village.rs` loads the level, quests, dialogue, bestiary, and recipes together and refuses a world that does not hold together: an NPC without a conversation, a conversation for someone absent, a `visit` to an unknown place, a `befriend` for a creature missing from the bestiary or the level, a quest nobody offers or can hand in, and quests that ask for items the world never supplies. `tests/village_playthrough.rs` plays every shipped quest through the public API on the real map, including real routes to every person, place, and creature.
  - Acceptance (not yet met in full): a new player can complete a 30–60 minute local adventure without blockers.
  - Done toward it: all quests are completable in one session in tests, and nothing is unreachable on foot. Wayfinding 2 (needed for the surveyor) takes about 300 tiles of walking.
  - Not yet: no one has played the slice for 30–60 minutes in a window, so the length and pacing are unverified.
  - Manual check (needs a window; not covered by unit tests): `cargo run` and confirm the village, people, portraits in conversation, standing stones, and creatures appear; talk to every villager; approach the hare without and with a honeyberry; craft a walking staff and hand it to Tamsin; walk to the three waymarks and report to Ivo after reaching Wayfinding 2; open the journal (J) and confirm field notes; confirm the ambient sound plays and that a machine with no audio device still runs.

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
