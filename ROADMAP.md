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
- [ ] Integrate map tiles and collision into the rendered world; expose tree and ore nodes in the scene.
  - Tests: tile collision and node interaction edge cases.
  - Acceptance: player cannot move into blocked tiles; gathering is available only in range; full inventory preserves resources.

## M1 — World movement and interaction
- [ ] Replace the demo meadow with a tile-based map loaded from a local data file.
- [ ] Add collision against trees, rocks, fences, and water.
- [ ] Add interactable objects and a consistent interaction range.
- [ ] Separate input, movement, world interaction, and UI systems into focused modules.
- [ ] Add unit tests for map parsing, walkability, movement bounds, and interaction range.
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
- [ ] Add readable original NPC portraits/sprites and ambient sound.
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
- [ ] Add readable dialogue, quest, inventory, and settings panels.
- [ ] Add pause menu and audio controls.
- [ ] Add performance profiling and a basic smoke-test checklist.
- [ ] Test settings validation and UI state transitions where practical.
  - Acceptance: all essential actions work from keyboard; no critical text is clipped at supported resolutions.

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
