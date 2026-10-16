# Ozbound

**Ozbound** is the working title for a relaxed, low-poly 3D role-playing adventure inspired by the exploration, skills, quests, gathering, and readable world of classic MMORPGs such as RuneScape. You play almost entirely with the mouse: click a tile to walk there, click an object to interact with it. It is a single-player Rust + Bevy project at this stage—not a RuneScape clone and not an online game.

The setting takes inspiration from L. Frank Baum's 1900 novel *The Wonderful Wizard of Oz*. The game should develop its own quests, dialogue, characters, art direction, and world structure rather than reproduce a later film adaptation.

## Current prototype

- A small low-poly 3D meadow: flat-shaded terrain tiles, faceted trees, boulders, glowing moonpetals, and blocky traveller characters. Every model is generated in code from simple primitives, so there are no external art assets.
- **Click a tile to walk there.** A yellow cross marks the spot; the player finds a route around trees, rocks, and water.
- **Click an object to interact.** A red cross marks it, the player walks up beside it, then talks, picks, chops, or mines. Chopping and mining repeat automatically until the node is spent, the bag is full, or you click elsewhere.
- Hovering shows what a click will do ("Chop down tree", "Talk to Village Guide", "Walk here").
- A third-person orbit camera that follows the player.
- A small quest: talk to the village guide, then pick three moonpetals.
- HUD with quest progress, bag contents, and a journal line.

This is deliberately a small, understandable foundation. Skills, dialogue trees, crafting, and richer world content are future work.

## Requirements

- Rust toolchain with Cargo
- A desktop platform supported by Bevy 0.15 (macOS, Windows, or Linux)
- Platform development libraries required by Bevy, if any

## Run

From this directory:

```sh
cargo run
```

Run the unit tests:

```sh
cargo test
```

The first build downloads Bevy and its dependencies. No account, game server, analytics, or cloud service is required.

## Controls

| Input | Action |
|---|---|
| Left-click ground | Walk to that tile |
| Left-click tree, copper vein, moonpetal, or the guide | Walk beside it, then chop / mine / pick / talk |
| Hover | Shows the action a click would perform |
| Middle-mouse drag, or arrow keys | Turn and tilt the camera |
| Scroll wheel | Zoom |
| Esc | Quit |

## Project principles

- **Single-player and local-first by default.** No accounts, telemetry, or online dependency in the starter.
- **Human-directed play.** No engagement tricks, daily streaks, or manipulative reward loops.
- **Understandable architecture.** Keep game rules in small Rust modules that can be tested without launching a window.
- **Test every feature.** Every roadmap feature includes unit-test work and explicit acceptance criteria.
- **Data ownership.** Plan saves as local files with a documented, versioned format.
- **Incremental scope.** Build a small vertical slice before adding expansive systems.

## Roadmap

See [`ROADMAP.md`](ROADMAP.md) for milestones and acceptance criteria.

## Design and architecture

See [`docs/DESIGN.md`](docs/DESIGN.md) and [`docs/ARCHITECTURE.md`](docs/ARCHITECTURE.md).

## Copyright and inspiration

The 1900 novel *The Wonderful Wizard of Oz* by L. Frank Baum is public domain in the United States; see [Project Gutenberg's edition](https://www.gutenberg.org/ebooks/43936). Copyright and trademark rules vary by jurisdiction. This project should use original game assets and should not copy protected material from later adaptations, including the 1939 film's distinctive designs, costumes, music, dialogue, or imagery. The title **Ozbound** is provisional and should be checked before public release.

This project is not affiliated with or endorsed by the Baum estate, any film studio, or the RuneScape rights holders.

## v0.2 domain foundations

The game rules are now separated into pure Rust modules under `src/`:

- `world.rs`: rectangular tile map, bounds validation, and walkability rules.
- `inventory.rs`: item IDs, stack counts, capacity, and safe add/remove operations.
- `gathering.rs`: gathering nodes for moonpetals, timber, and copper ore; a node is consumed only when inventory accepts the yield.
- `interaction.rs`: shared interaction-range check for characters and objects.
- `quest.rs`: quest progress state and tests.

Moonpetal collection in the Bevy prototype now adds the item to a 20-slot inventory and shows inventory counts in the HUD. The map, generic gathering nodes, and shared interaction helper are domain foundations for the next integration pass; the visual meadow still uses placeholder shapes.

Run `cargo test` to test domain rules without opening a game window. These checks are not reported as passing until run on a machine with Rust/Cargo installed.

## v0.3 playable world integration (historical; controls replaced in v0.4)

The Bevy prototype now renders a 30x20 tile meadow from `scene.rs` and uses the pure rules for movement and gathering:

- Trees, rock, water, and the map edge block movement (`movement.rs`); the player slides along walls.
- Trees yield timber and copper veins yield copper ore. Stand beside one and press **E**. A full bag leaves the node untouched; spent nodes turn gray.
- The guide takes priority over resources when both are in range.

Controls are unchanged. Domain tests (`cargo test`) cover collision, text-map parsing, node placement, interaction range selection, and meadow reachability. Visual behavior still needs a manual check; see the M0.2 note in `ROADMAP.md`.

## v0.4 low-poly 3D, click-to-move

The game changed direction from a top-down, keyboard-driven prototype to a low-poly 3D world with classic point-and-click MMORPG controls. The art is original, in the general "chunky low-poly" spirit and not a copy of any existing game's models. The rules stayed in pure Rust, and the new input rules are pure and tested too:

- `pathfinding.rs`: breadth-first routes over tiles; diagonal steps never cut a blocked corner; `Adjacent` approach for blocked targets like trees.
- `picking.rs`: ray vs ground and ray vs box, deciding what the cursor is over.
- `task.rs`: the route being walked and the interaction waiting at its end; a new click finishes the current stride first.
- `orbit.rs`: orbit-camera angles, clamping, and zoom.
- `scene.rs`: also provides deterministic per-tile variation, so the world looks the same on every run.

`movement.rs` (pixel-space continuous collision for WASD) was removed: routes now follow whole tiles, so tile walkability alone decides what you can reach. One tile is now one world unit (`TILE_SIZE = 1.0`).

The Bevy side is split into focused modules: `setup.rs` (build the world), `models.rs` (low-poly models), `pointer.rs` (hover and clicks), `actions.rs` (walking, gathering, animation), `camera_rig.rs`, and `hud.rs`.

Esc now actually quits (earlier versions documented it but never handled the key).

### Keyboard accessibility

Mouse-first play is the intended style, but it is not accessible to everyone. A keyboard path (for example Tab to cycle nearby targets and Enter to act) is tracked in `ROADMAP.md` under M7.

### Verification status

- `cargo test`: 74 tests pass (Linux, Rust 1.89). `cargo clippy --all-targets -- -D warnings` is clean.
- The game was also launched in a headless Linux session with software rendering (Xvfb + llvmpipe) and driven with real mouse events. Observed working: hover labels, walking to a clicked moonpetal and picking it, chopping a tree until it was spent (crown disappears, timber added), the "You can't walk there." refusal on water, talking to the guide, middle-drag camera rotation, scroll zoom, and Esc to quit.
- Not yet checked by eye: copper mining, arrow-key camera, clicking mid-walk, a full bag, and real-GPU frame rate. See the manual checklist in `ROADMAP.md` (M0.3).
- Building needs Rust 1.88 or newer because the committed `Cargo.lock` pins an `image` release that requires it.

## Level data (M1)

The meadow now lives in `assets/maps/meadow.map`, a plain-text file with `[map]`, `[player]`, `[guide]`, `[moonpetals]`, and `[nodes]` sections (the format is described at the top of the file; `y` counts up from the bottom row). The game loads it at startup; set `OZBOUND_MAP=path/to/other.map` to play a different file. If the file has a mistake the game prints the file name, line number, and what is wrong, then exits. If `assets/maps/meadow.map` is missing (for example when run from another directory) the identical built-in copy is used.

## Quests and dialogue (M2)

Quests and conversations are now data, handled by pure Rust modules under `src/`:

- `quests.rs`: quest definitions from `assets/quests/meadow.quest` (prerequisites, ordered `talk`/`collect` objectives, items taken on hand-in, item rewards) and the player's `QuestLog`.
- `dialogue.rs`: conversations from `assets/dialogue/meadow.dialogue`, with choices that depend on quest and bag state, and effects that start or complete quests.
- `journal.rs`: the active/completed quest list. `quest_save.rs`: a versioned text format for the quest log.
- `textdata.rs`: the shared reader behind the content files; errors name the file line.

These are tested without a window, including a full play-through of the moonpetal quest (`tests/quest_playthrough.rs`). The Bevy prototype still uses the original moonpetal counter and the guide's fixed line; connecting it to the new modules is the open item at the end of M2 in `ROADMAP.md`.
