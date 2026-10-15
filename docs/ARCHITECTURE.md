# Architecture Notes

## Current layout

```text
src/
├── main.rs          # Bevy app, visuals, keyboard input, HUD
├── lib.rs           # Public pure-Rust domain modules
├── world.rs         # Tile positions, map bounds, walkability
├── inventory.rs     # Item IDs, stacks, slot capacity
├── gathering.rs    # Resource nodes, gather transaction, node placement validation
├── interaction.rs  # Shared interaction-range calculation and nearest-target pick
├── movement.rs     # Tile collision: box clearance and sliding movement
├── scene.rs        # Meadow text map, spawn tiles, resource node placements
└── quest.rs         # Moonpetal quest state
```

## Boundaries

- `main.rs` owns rendering, Bevy entities, input, and the HUD. It draws whatever `scene.rs` describes and asks the pure modules whether moves and interactions are allowed.
- `world.rs`, `inventory.rs`, `gathering.rs`, `interaction.rs`, `movement.rs`, `scene.rs`, and `quest.rs` contain deterministic rules and do not depend on Bevy.
- World space is centered on the origin; `WorldMap::tile_center` / `world_to_tile` convert between tile and world coordinates (`TILE_SIZE` = 32). Text maps list the top row first; `y` increases upward.
- Collision treats anything outside the map as blocked, so no separate position clamp is needed.
- Gathering attempts add the yield to inventory before decrementing node availability. This prevents a full inventory from consuming a resource.
- Inventory capacity counts distinct occupied item stacks. Adding more of an existing item uses its existing slot.
- The prototype renders the tile map, blocks movement on trees, rock, and water, and lets the player gather timber and copper ore from adjacent tiles. The meadow is still compiled in as a text constant; loading maps from a local data file is M1.

## Testing strategy

- Unit-test pure rules, including invalid inputs and edge cases.
- Add integration tests for multi-step quests and save/load as those features arrive.
- Keep manual smoke-test instructions for windowing, rendering, input, and platform-specific behavior.
- Every roadmap feature must list tests and acceptance criteria before coding.

## Technical choices

- Rust + Bevy 0.15 for the game loop and rendering.
- Pure domain logic wherever practical, to keep tests fast and independent of a window.
- Local-first: saves and content stay on the user's device. Networking is not part of the initial design.
- Minimal dependencies; review licensing and maintenance before adding crates.
