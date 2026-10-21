# Game Design Document (GDD)

**Project:** Ozbound  
**Genre:** Single-player, low-poly 3D exploration RPG  
**Engine:** Rust + Bevy 0.15  
**Status:** Living design document  
**Companion documents:** [README.md](README.md) · [ROADMAP.md](ROADMAP.md) · [docs/DESIGN.md](docs/DESIGN.md) · [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md) · [LOCATIONS.md](LOCATIONS.md)

---

## 1. High Concept

Ozbound is a relaxed, low-poly 3D role-playing adventure inspired by the exploration and progression loops of classic MMORPGs. The player explores a storybook world, meets its inhabitants, gathers useful materials, develops practical skills, crafts items, completes quests, and discovers new paths—all in a private, offline, single-player experience.

The game draws inspiration from L. Frank Baum's 1900 *The Wonderful Wizard of Oz*, but it is not a direct retelling of the novel and is not a RuneScape clone. The protagonist, starting settlement, quests, dialogue, maps, and visual identity are original.

**One-sentence pitch:** Explore a strange and welcoming storybook land, help its inhabitants, gather and craft useful things, develop practical skills, and uncover new paths at your own pace.

### Core Promise

Ozbound gives players the satisfying sense of gradual discovery and progression found in classic role-playing games without mandatory online services, social pressure, artificial urgency, or systems designed to keep players logged in.

The player owns their time and determines how to engage with the world.

## 2. Product Principles

1. **Player-directed pace:** No daily rewards, streaks, battle passes, loot boxes, artificial urgency, or punishment for taking a break.
2. **A world to wander:** Landmarks, paths, environmental clues, and conversations reward curiosity without flooding the screen with markers.
3. **Understandable actions:** Players should know what an interaction does, why an action is unavailable, and what they can do next.
4. **Useful progression:** Skills unlock possibilities, recipes, dialogue, routes, and knowledge—not just bigger numbers.
5. **Cozy wonder:** The world can contain mysteries, strange creatures, obstacles, and occasional danger without defaulting to grimness or cruelty.
6. **Local ownership:** No required account, game server, telemetry, analytics, or cloud service. Saves and settings remain on the player's device.
7. **Simple, durable systems:** Prefer composable rules, readable data, explicit file formats, and minimal dependencies.
8. **Originality:** Borrow broad genre conventions, not another game's distinctive maps, models, writing, interface, or assets.
9. **Testable development:** Every feature requires acceptance criteria, unit-test goals, regression coverage, and manual checks where automation cannot verify behavior.

## 3. Design Pillars

### 3.1 A World to Wander

The player should be able to understand a place by exploring it rather than consulting an external guide. Paths, rivers, bridges, fences, buildings, gardens, forests, and landmarks create spatial meaning.

Optional discoveries reward attention, but essential progression should not depend on arbitrary pixel hunting.

### 3.2 Useful, Satisfying Activities

Gathering and crafting should serve a visible purpose. Timber can become a tool; herbs can become a remedy; a crafted item can help an NPC or open a new interaction.

Avoid resources and activities that exist only to inflate playtime.

### 3.3 Small Stories with Character

Quests should be short, authored, and grounded in the people and places of the world. A request should reveal something about a character, settlement, or surrounding landscape.

Prefer a few memorable steps over repetitive errands.

### 3.4 Gentle, Legible Progression

Skills improve through relevant actions. Players should understand which activities teach a skill and what a level enables.

Progression is deterministic. Skills do not decay, and there are no daily caps or random XP bonuses.

### 3.5 Mouse-First, Not Mouse-Only

The primary interaction style is classic point-and-click: click a tile to walk, or click a target to approach and interact.

Keyboard controls must provide alternatives for essential actions. Hover text, context menus, journal entries, and clear failure messages keep the game discoverable.

## 4. Target Player Experience

The player is an independent traveller arriving in an unfamiliar part of Oz. They find a settlement, meet its residents, learn how the local area works, and gradually become capable of helping with more involved tasks.

The intended emotional rhythm is:

1. **Arrive:** See a distinctive place and understand where to begin.
2. **Orient:** Meet a local character and learn about an immediate concern.
3. **Explore:** Walk around the village and surrounding landscape.
4. **Gather:** Collect materials from visible, understandable sources.
5. **Learn:** Gain practical skill progress through relevant activities.
6. **Make or help:** Craft a useful object, befriend a creature, or complete a request.
7. **Discover:** Find another landmark, conversation, recipe, or path.
8. **Return when ready:** Continue later without penalties for time away.

Players should never feel that they must play every day or rush to avoid losing progress.

## 5. World and Setting

### 5.1 Source Material

The primary literary inspiration is Baum's 1900 *The Wonderful Wizard of Oz*. The game may draw on public-domain names, places, creatures, and story ideas from that work while creating its own geography, characters, stories, and presentation.

Use these source labels in planning documents and content records:

- `NOVEL_1900` — Directly grounded in the original novel.
- `LATER_OZ` — Material from later Oz books, not automatically included in the initial scope.
- `GAME_ORIGINAL` — Content invented for Ozbound.
- `ADAPTATION` — A game interpretation where the source does not specify precise geography or details.

Do not present an invented map as a canonical map from the novel. The locations in [LOCATIONS.md](LOCATIONS.md) are a content-planning inventory, not a commitment to reproduce the novel's plot in order.

### 5.2 Protagonist and Cast

- The protagonist is an original traveller, not Dorothy.
- The protagonist's name, background, and reason for travelling remain open design decisions.
- Original villagers, guides, craftspeople, and creatures are the default cast.
- Characters from the 1900 novel may be included if they serve the game's original stories, but they are not required for the starting area.
- Dialogue, quests, personality, and visual designs must be created for this game.

### 5.3 Regional Structure

The long-term world may include original interpretations of:

- The Land of the Munchkins.
- The Emerald City.
- The Country of the Winkies.
- The Country of the Quadlings.
- The Yellow Brick Road.
- The Field of Deadly Poppies.
- Dainty China Country.
- Other locations and landscapes from the original novel.

Build connected regions incrementally instead of attempting the whole of Oz before the starting area is enjoyable.

Each region should have:

- A clear identity and visual language.
- A few memorable landmarks.
- Sensible entrances and exits.
- Inhabitants or environmental details that make it feel lived in.
- At least one meaningful activity or discovery.
- Source and canon notes where the game adapts existing material.

### 5.4 Starting Region: Bluebrook

Bluebrook is an original starting village and surrounding landscape. It is the first playable content slice and establishes the game's systems before the world expands into larger Oz-inspired regions.

Current layout concept:

- Meadow in the south-west.
- A river and one-tile bridge on the main road.
- Village to the east.
- Fenced apothecary garden, woodcutter's yard, village street, and notice boards.
- Forest along the northern edge with gatherable trees.
- Standing-stone waymarks and optional local discoveries.

The current map is a 40 × 24 tile region. Buildings and walk-in interiors are planned but not yet complete. See M5 and M9 in [ROADMAP.md](ROADMAP.md).

## 6. Core Gameplay

### 6.1 Core Loop

**Explore → interact → gather or investigate → learn or craft → help someone → gain access or knowledge → explore further.**

A typical session might involve speaking to a villager, collecting herbs, crafting a remedy or tool, completing a request, gaining skill progress, and following a clue to a landmark.

Players can also ignore quests for a while and forage, explore, read signs, or befriend local creatures.

### 6.2 Movement and Navigation

- The world is 3D, while gameplay navigation is tile-based.
- One tile is one world unit.
- Clicking a walkable tile plans a route; the character moves smoothly between tile centres.
- Diagonal movement must not cut through blocked corners.
- Clicking an object or character moves the player to a valid interaction position before acting.
- Water, trees, rocks, fences, and map edges are governed by world data, not just visible models.
- The orbit camera follows the player and can be rotated and zoomed.
- Regions connect through deliberate exits or transitions.

### 6.3 Interactions

The player can interact with characters, resources, signs, creatures, and other objects.

- Hover text previews the action.
- Left-click performs the main action or approaches the target first.
- Right-click opens a context menu where appropriate.
- Examine offers useful information or flavour without unexpectedly changing state.
- Reachability and interaction range are checked when the action occurs.
- Failures explain themselves in plain language.
- Failed actions must not consume items, resources, or quest progress.

### 6.4 Gathering

Current gathering includes foraging, chopping, and mining.

Examples include moonpetals, timber, copper ore, meadowmint, and honeyberries.

Rules:

- Check inventory capacity before consuming a resource.
- A full inventory must not consume a node.
- Repeated gathering stops when the node is depleted, the bag is full, the action fails, or the player gives another movement order.
- Respawn behavior is defined per resource and must not create artificial urgency.
- A resource's visual state communicates whether it is available or depleted.
- Moonpetals are story collectibles and do not respawn; trees, veins, herbs, and berries have defined respawn behavior.

### 6.5 Inventory and Equipment

Inventory is slot-based. Ordinary items can stack; equipment occupies one slot per item.

Five equipment slots currently exist:

- Head.
- Body.
- Feet.
- Hand.
- Charm.

Rules:

- Capacity and stack limits are explicit.
- The game explains inventory restrictions and item requirements.
- Equipment can be swapped when the inventory has room for the displaced item.
- Unequipping requires an available inventory slot.
- Equipment bonuses are not required for the initial quests. Add them when they have a clear gameplay purpose.

### 6.6 Crafting

Recipes are local, data-driven content. Each recipe defines inputs, output, and optional skill requirements.

- Show recipe requirements and why a recipe is locked.
- Missing ingredients, insufficient skill, or insufficient result capacity must not consume inputs or grant XP.
- A successful craft consumes its inputs, creates its output, and awards the defined skill XP.
- Recipes should connect gathering to useful outcomes, quests, or exploration.
- Start with a small number of meaningful recipes rather than a large catalogue of filler items.

### 6.7 Skills and Progression

Current skills:

- **Foraging:** Collecting useful plants and berries.
- **Woodcraft:** Working with timber and basic materials; current XP sources also include mining.
- **Herbalism:** Collecting and using herbs.
- **Wayfinding:** Learning the land by travelling and visiting places.

Progression is deterministic, levels run from 1–10, and the XP curve is documented in code. XP sources have explicit ceilings so players cannot max a skill through one trivial repeated action.

Design requirements:

- Each skill has a readable purpose.
- Level gates lead to content the player can actually reach and complete.
- The game communicates which actions teach a skill and what a level unlocks.
- Avoid skill decay, mandatory grinding, random XP multipliers, and daily limits.
- Add a skill only when it supports distinct gameplay.

### 6.8 Quests and Dialogue

Quest objectives currently support talking to a character, collecting items, crafting items, visiting a named place, and befriending a creature.

Dialogue is data-driven and can offer conditional choices, start quests, and complete hand-ins.

Current Bluebrook quests:

- *Moonpetals for the Apothecary*
- *Mint for the Apothecary*
- *A Staff for the Bridgewarden*
- *Waymarks of Bluebrook*
- *The Shy Hare*

Quest rules:

- Objectives are ordered and clearly described.
- Only the active objective receives progress.
- Unrelated items, visits, or creatures do not count accidentally.
- Hand-ins validate requirements and reward capacity before committing.
- Failed hand-ins never destroy progress.
- Conversations always have a safe exit.
- Quest text explains why the task matters to the character or place.

### 6.9 Creatures and Encounters

The current tone favours observation, approach, and befriending over combat.

Current examples include a meadow hare, pond heron, and lantern moth.

- Approaching a creature can add a field note to the journal.
- Befriending is optional unless a quest explicitly uses it.
- An offering is consumed only if the interaction succeeds and any gift fits in the inventory.
- Creatures should not provide repeatable duplicate rewards.
- Combat is outside the current vertical slice. Any future combat proposal must justify its fit with the game's tone and core loop.

### 6.10 Exploration and Discovery

Exploration is driven by visible places, signs, local dialogue, paths, and curiosity.

Future discoveries may include:

- Landmarks and waymarks.
- New recipes and materials.
- Shortcuts or newly passable routes.
- Optional creature encounters.
- Small environmental stories.
- New settlements and region transitions.

The journal can record discoveries, but the interface should not be filled with markers that remove the need to explore.

## 7. Interface and Controls

The interface should be calm, readable, and useful without taking up more screen space than necessary.

Current controls:

- Left-click: Walk or approach/interact.
- Right-click: Open the context menu.
- WASD: Step relative to the camera; hold to continue.
- Tab / Shift+Tab: Select the next/previous reachable target.
- Enter / Space: Travel to and use the selected target; otherwise use the nearest target.
- X: Examine the selected target.
- 1–9: Choose a dialogue or crafting option.
- J: Open/close the journal; PageUp/PageDown change pages.
- C: Open/close crafting and gear.
- Middle-mouse drag or arrow keys: Rotate/tilt the camera.
- Scroll wheel or +/-: Zoom.
- F3: Show/hide the frame-time overlay.
- Esc: Close the most urgent panel; if none is open, open the pause menu.

Planned interface improvements in M8:

- Separate chat/journal messages from the initial HUD.
- Move the bag into a standalone inventory panel with a B shortcut.
- Provide standalone skills and journal panels.
- Use the dialogue style for readable sign messages.
- Add short emotes such as `/wave`, `/dance`, and `/cheer`.
- Reduce the persistent HUD footprint.

Interface requirements:

- Essential actions must be available by keyboard.
- No conversation should trap the player.
- Long text wraps and paginates.
- Text scaling must not clip critical information.
- Pausing stops gameplay time.
- A click that resumes the game must not also move the player.
- Controls and settings should be explained in-game.

## 8. Art and Audio Direction

### 8.1 Visuals

- Chunky, flat-shaded, low-poly 3D.
- Storybook colours, readable silhouettes, and recognizable landmarks.
- Models should remain understandable from a raised third-person camera.
- Designs must be original rather than copying a film, game, or adaptation's visual identity.
- Prefer reusable environmental pieces and clear terrain rules over expensive detail.

The prototype generates models from simple primitives in code. This is a useful early art direction, not a requirement that all final assets be procedurally generated.

### 8.2 Audio

The prototype synthesizes quiet looping wind-and-birdsong ambience. Regional music may be introduced later.

Audio should:

- Be optional and have a master volume control.
- Never be required to understand a quest.
- Loop cleanly without distracting clicks or abrupt transitions.
- Use original or properly licensed material.
- Avoid a large asset pipeline before the world needs it.

## 9. Saving, Ownership, and Offline Play

Ozbound is designed as a local game.

- No account, game server, telemetry, analytics, or cloud service is required.
- Progress is saved locally in a versioned plain-text file.
- Settings are stored separately from progress.
- Writes use a temporary file and replacement.
- Invalid saves are never silently overwritten.
- A backup is kept after a successful load.
- Content definitions remain local and readable where supported.
- Format changes require a version update, compatibility tests, and a documented migration policy.

Multiplayer is not part of the initial design. Consider networking or modding only when there is a concrete player benefit and the design does not compromise local play.

## 10. Scope and Priorities

### 10.1 Current Foundation

The repository already includes:

- A low-poly 3D scene and tile-based pathfinding.
- Click-to-move and click-to-interact.
- Camera orbit and zoom.
- NPC dialogue, signs, context menus, and journal.
- Inventory, gathering, respawning, crafting, equipment, and skills.
- Data-driven quests and dialogue.
- Optional non-lethal creature befriending and field notes.
- Local save/load and settings.
- Keyboard alternatives and UI fitting rules.
- Pure Rust domain modules with unit and integration tests.

Bluebrook is the current starting world, not the full map of Oz. Some systems are foundations and may not yet have final balance or presentation. Refer to [ROADMAP.md](ROADMAP.md) for implementation status and manual verification tasks.

### 10.2 Near-Term Priorities

1. Finish and manually verify the Bluebrook vertical slice.
2. Improve HUD, chat/journal, inventory, skills, and sign-reading UI (M8).
3. Build out Bluebrook with reusable buildings, interiors, gardens, and an improved bridge (M9).
4. Add restrained NPC movement and social behaviour (M10).
5. Expand into additional authored regions only after the starting experience is readable and enjoyable.
6. Evaluate Web builds and editor mode as separate technical milestones, without letting them delay core game quality.

### 10.3 Out of Scope for the Initial Game

- Mandatory multiplayer or online accounts.
- Live-service progression, daily rewards, and time-limited events.
- Pressure-based monetization or random-reward systems.
- Combat as the default answer to every obstacle.
- Procedural content volume for its own sake.
- Copying another game's distinctive assets, maps, interface, or quests.
- Recreating the 1939 film.
- Building the whole map of Oz before validating the starting area.

## 11. Technical and Content Constraints

This document defines player experience. [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md) remains authoritative for implementation.

- Rust + Bevy 0.15.
- Keep domain rules in pure Rust modules wherever practical.
- Use Bevy for rendering, input, animation, audio playback, and ECS integration.
- Keep level, quest, dialogue, recipe, and bestiary content data-driven where supported.
- Validate linked content together at startup and produce clear file/line errors.
- Never silently replace invalid user-authored content.
- Avoid dependencies unless their benefits justify their maintenance and licensing costs.
- The game should still start without optional expansion content.

## 12. Testing and Definition of Done

Every feature must define acceptance criteria and test goals before or alongside implementation.

### Automated Coverage

- Unit tests for pure domain rules, edge cases, and invalid inputs.
- Regression tests for every bug fix.
- Integration/playthrough tests for multi-step quests, crafting, progression, and save/load.
- Validation tests for linked level, NPC, quest, dialogue, recipe, and creature content.
- Persistence tests for malformed, outdated, or incompatible files and safe recovery.

### Manual Checks

Automated tests cannot fully verify rendering, real input, audio devices, or performance. Document manual steps for:

- Movement and interaction in a real window.
- Panel layering and text readability.
- Camera behaviour and collision.
- Audio playback and volume.
- Save/load in a real play session.
- Supported platform behaviour.
- Performance on real hardware.

### Feature Completion Checklist

- [ ] Player-facing purpose and acceptance criteria are written.
- [ ] Unit tests are added first or alongside implementation.
- [ ] Regression and integration tests are added or updated.
- [ ] Failure cases leave inventory, resources, and quest state consistent.
- [ ] Manual checks are documented for visual/platform behaviour.
- [ ] `cargo fmt --check`, `cargo test`, and `cargo clippy --all-targets --all-features -- -D warnings` pass where supported.
- [ ] Relevant README, roadmap, design, architecture, or content documentation is updated.

## 13. Open Design Decisions

Do not make irreversible decisions before the current slice has been play-tested.

- [x] What are the protagonist's name, background, and reason for travelling?
- Player should be able to create and load a character. They can design and name the character they play.

- [x] How closely should named regions from the novel map onto the game's original geography?
- Regions can map 1-1 as long as they remain in the realm of public domain.

- [x] Which locations and book characters belong beyond Bluebrook?
- All minor and major characters as long as they remain in the realm of public domain, have their own unique design that doesn't resemble anything from later copywritten works.

- [x] Should the world be one connected overworld or compact maps with authored transitions?
- One connected overworld but we can have cells or compact player areas accessible via transitions I.e. caves or portals.

- [x] What is the final inventory presentation: grid, list, or hybrid?
- Grid

- [x] Which equipment slots should have gameplay effects?
- All equipment slows should have gameplay effects except vanity would have a cosmetic effect to the player only

- [x] How should buildings and interiors work across map transitions?
- The roofs of buildings should auto-remove while the player walks in them, and the roofs should reappear if the player is no longer in the building

- [x] Which NPC behaviours add life without making the world noisy or unpredictable?
- Some NPCs can have patrol routes, some NPCs may stand at a designated spot, some NPCs may go pretend to fish or gather resources from a nearby plot

- [x] What is the smallest satisfying long-term progression loop after the starting village?
- A main quest line that takes the player to see the wizard of oz, and multiple boss encounters with the wicked witch of the west and made-up minions

- [x] Is controller support worth the implementation cost?
- Not supporting controllers in the short-term

- [x] Is the world editor a developer tool, a player-facing creative mode, or both?
Developer tool, but we might want to allow the player to own a plot of land and build on it so keep it flexible

- [x] What play-test evidence would justify adding combat, multiplayer, or more skills?
Deferring decision till later.

## 14. Source and Originality Policy

The 1900 novel is the primary public-domain inspiration, subject to jurisdiction-specific rules. Public-domain status does not automatically clear later books, illustrations, film designs, music, dialogue, or trademarks.

For the game:

- Use original models, music, dialogue, maps, and quest writing.
- Label later-book material separately from the 1900 novel.
- Do not copy distinctive protected material from later adaptations.
- Check the working title and branding before public release.
- Document licenses for third-party assets introduced later.

Primary reference: L. Frank Baum, *The Wonderful Wizard of Oz* (1900), [Project Gutenberg](https://www.gutenberg.org/ebooks/43936).

Ozbound is not affiliated with or endorsed by the Baum estate, any film studio, or the RuneScape rights holders.
