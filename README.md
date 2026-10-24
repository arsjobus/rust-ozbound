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
- A slim HUD (quest tracker and hover label), a chat area for game messages and emotes, and bag, skills, and quest-book panels on a toolbar.

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

Everything can be done from the keyboard; the mouse is optional. The same list is in the game (Esc, then Controls).

| Input | Action |
|---|---|
| Left-click ground | Walk to that tile |
| Left-click tree, copper vein, moonpetal, the guide, a sign, or a creature | Walk beside it (or onto it), then chop / mine / pick / talk / read (a sign opens in the dialogue panel) / approach |
| Left-click a meadowmint patch or honeyberry bush | Walk onto it, then forage until it is spent |
| Right-click | Action menu for what is under the cursor |
| W A S D | Take one step, relative to the camera; hold to keep walking |
| Tab / Shift+Tab | Select the next / previous thing worth visiting (nearest first); the hover label says what it is and how far |
| Enter or Space | Go to the selected thing and use it (with nothing selected, the nearest) |
| X | Examine the selected thing |
| 1-9 | Pick a numbered row in a conversation or the crafting panel |
| B / K / J | Open or close the bag / skills / quest book (one panel at a time; the toolbar buttons do the same). PageUp / PageDown turn the open panel's pages |
| T or / | Type in the chat area (`/` starts a command): Enter sends, Esc cancels. `/wave`, `/dance`, `/cheer` play a short gesture; `/help` lists the commands. `/edit` turns the world editor on and off (see below). PageUp / PageDown scroll the chat when no panel is open |
| C | Open or close the crafting-and-gear panel (click a row, or press 1-9) |
| Middle-mouse drag, or arrow keys | Turn and tilt the camera |
| Scroll wheel, or + / - | Zoom |
| F3 | Show or hide the frame-time overlay |
| Esc | Close the most urgent open thing (pause menu, right-click menu, chat box, conversation, crafting panel, side panel); with nothing open, open the pause menu |

## World editor

Type `/edit` on the open world. A ribbon along the top has **Select**, **Select & Move**, **Place**, **Undo**, **Redo**, **Export asset**, and **Exit editor**. Click an object (the guide, a villager, a creature, a tree, copper vein, herb patch, berry bush, moonpetal, or signpost) to select it; with Select & Move, click ground in the same scene to move it there. Right-click for an extended menu (Select, Move here, Place here, Remove). Delete removes the selection; Ctrl+Z undoes and Ctrl+Y (or Ctrl+Shift+Z) redoes. An edit that would leave something unreachable or break the story is refused with a reason. Edits are kept in `saves/edits.txt` and replayed at start-up; the shipped level files are never changed. Objects, people, and creatures are described by small files in `assets/library/` (format in `src/asset_file.rs`). Design notes: `docs/ARCHITECTURE.md`.

## Pause menu and settings

Esc with nothing else open pauses the game (the clock stops; the ambient sound keeps playing so you can hear volume changes). Use Up / Down, Enter or Space, Left / Right, and Esc, or click:

- **Settings**: master volume (0-10), ambient sound on/off, text size (Normal / Large / Larger), and the frame-time overlay.
- **Controls**: the list above.
- **Quit game**: the only way Esc ever leaves the game; progress is saved on exit as before. Closing the window also saves.

Settings are saved at once to `saves/settings.txt` (set `OZBOUND_SETTINGS=path` to use another file). It is a small plain-text file with a version header (`ozbound-settings 1`, documented at the top of `src/settings.rs`); delete it to go back to the defaults. Like the save, it is written through a temporary file and a rename, and a file that cannot be read is reported with its file name and line, replaced by the defaults for that session, and **never overwritten**.

Text size scales the whole interface. The smallest supported window is 800 x 600; in a small window a large size is lowered just enough for every panel to fit, so text is never clipped. Long text is wrapped to its panel, and a long journal is split into pages.

## Performance

F3 (or the pause menu) shows a frame-time overlay: frames per second over the last two seconds, the average and 1% low frame time, and the worst frame. Start the game with `OZBOUND_PROFILE=1` to also print a session summary (frames, average, worst, slow frames) on exit. `docs/SMOKE_TEST.md` is the checklist to run before a release.

## Saving

Ozbound saves your game by itself: about a second after anything changes, and once more when you quit. The save is one plain-text file, `saves/game.txt` (set `OZBOUND_SAVE=path` to use another file, for example one per map). It holds where you stand, your bag and worn gear, skill XP, quest progress, the creatures you have met, which moonpetals you have picked, and which trees, veins, and plants have been used. A tree, vein, or plant that was spent when you quit is spent when you return and starts growing back again; moonpetals never grow back.

- **Safe writes.** Each save is written to `game.txt.tmp`, flushed, and renamed over `game.txt`, so a crash while saving leaves the previous save whole.
- **A save that does not fit is not thrown away, and saving keeps working.** If the file cannot be loaded as it is (a map that was resized or redrawn since, a hand edit, damage), the game first copies it to `saves/game.txt.unusable-1` (then `-2`, and so on; earlier copies are never overwritten). If only *where things stand* is wrong, your bag, gear, skills, quests, and creatures met are kept and you start again at the beginning of the village; if the progress itself is damaged, a new game starts. The console and the chat area say which happened and name the copy. A save from a *newer* game version, or a file that cannot be read or copied, is left exactly as it is and nothing is saved that session. A write that fails (a full disk, say) is retried every 30 seconds instead of ending saving.
- **Backup.** Every time a save loads successfully, the game copies it to `saves/game.txt.bak`. To go back to it, copy that file over `game.txt` while the game is closed.
- **Old saves.** Versions before M6 kept only quest progress in `saves/quests.txt`. If there is no `game.txt` yet, that file is read once and carried into the new save; it is left where it is.
- **Format.** Versioned (`ozbound-save 1`), documented at the top of `src/save.rs`. The save is checked against the world it is loaded into, and nothing is loaded partly. Delete `saves/` to start over; the folder is not tracked by git.

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

### Quests and dialogue in the game

Talking to the village guide opens a dialogue panel (click a choice or press 1-9; Esc or walking away closes it). Quests and conversations come from `assets/quests/meadow.quest` and `assets/dialogue/meadow.dialogue` (override with `OZBOUND_QUESTS` / `OZBOUND_DIALOGUE`; absent default files fall back to built-in copies, invalid files stop startup with file and line). The HUD shows a quest tracker, and **J** opens the journal. The whole game is saved automatically (see *Saving* below).

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

## Inventory, foraging, and crafting (M3)

- `inventory.rs`: slot-based bag. One slot holds up to 99 of an ordinary item (equipment is one per slot), so large piles take more than one slot; every add and remove is all or nothing.
- `equipment.rs`: five equipment slots; equipping swaps with what was there, and taking gear off needs a free bag slot.
- `crafting.rs`: recipes from `assets/recipes/meadow.recipes` (`name`, `input = item amount`, `output = item amount`) and all-or-nothing crafting. Set `OZBOUND_RECIPES=path/to/other.recipes` to use another file; errors name the file and line.
- `pack.rs`: the crafting-and-gear panel as data (rows, what choosing one does, the bag and equipment lines), so it is tested without a window.
- New gatherables in the meadow: meadowmint patches and honeyberry bushes. They grow back after about 20 to 25 seconds.

The full-bag, spent-node, crafting, and equipment rules are tabulated in `docs/ARCHITECTURE.md`. Bag contents and equipment are part of the save file (see *Saving* below).

## Skills and progression (M4)

- `skills.rs`: Foraging, Woodcraft, Herbalism, and Wayfinding, levels 1-10. XP to reach level L is `30 x (L-1)^2` (30, 120, 270, ... 2430). Gathering, crafting, and walking teach XP, and each source stops teaching at a ceiling level so easy repetition cannot max a skill.
- Recipes can say `require = woodcraft 3` and `xp = woodcraft 40 6`; dialogue can say `skill herbalism at_least 2` (or `below 2`). The copper charm needs Woodcraft 3, mint tea needs Herbalism 2, and the guide's mint quest needs Herbalism 2.
- The HUD and the `C` panel show your skill levels and progress. The game refuses to start if content asks for a level nothing can teach. Skills are part of the save file (see *Saving* below).

## v0.5 Bluebrook vertical slice

The game now starts in **Bluebrook** (`assets/maps/bluebrook.map`): the meadow, a bridge over the river, a village with four villagers besides the guide, and a forest. There are five short quests (collect, craft, visit, befriend, and talk objectives), three gentle creatures you can approach (no combat), dialogue portraits, and quiet ambient sound that is generated in code.

- Talk to villagers, hand in quests, and press `J` for the quest book (quests and field notes), `B` for your bag, `K` for skills and `C` for crafting.
- Approach a creature with something it likes to win its trust; it is always optional and harmless.
- `OZBOUND_MAP`, `OZBOUND_QUESTS`, `OZBOUND_DIALOGUE`, `OZBOUND_BESTIARY`, and `OZBOUND_RECIPES` play other files. The older meadow files remain as the small tutorial fixtures used by the tests.
- At startup the whole world is cross-checked (`src/village.rs`), so a content mistake stops the game with a plain message instead of leaving a quest that cannot be finished.

## v0.6 The eastern village and the road (M9)

The starting village is now **Bluebrook**, in the Land of the Munchkins. It has three plots with walk-in houses, gardens, and vineyards, a wooden bridge, and thickets along the forest edge. Walk east to the end of the main road to travel to the Yellow Brick Road, a second region with its own map and music.

- `assets/world/locations.world` is the data form of `LOCATIONS.md`: every place, where it leads, which can be played, and its music.
- Put your own music in `assets/bgm/` (ogg, wav, or mp3; see `assets/bgm/README.md`). Without it each area plays a short theme generated in code.
- `OZBOUND_MAP` still replaces the starting village's map. Other regions are read from `assets/maps/` (or built-in copies).
- Saves are now format 2 (they record which region you are in). Older saves still load.

## v0.7 Villagers walk and talk (M10)

Villagers now have lives. Each one thinks with a shared behavior tree: some stand still, some wander around their home, and they play the same wave, dance, and cheer emotes you can. Click one and they stop, turn to face you, and wave goodbye when you finish talking.

- Set who does what in the `[behaviors]` section of the level file (`assets/maps/bluebrook.map` shows the format): `id stay` or `id wander <radius>`, optionally `pause <min> <max>` and `emotes wave dance cheer`.
- The behavior engine (`src/behavior.rs`) and NPC rules (`src/npc.rs`) are pure Rust and unit-tested; `tests/npc_playthrough.rs` simulates the whole village.
