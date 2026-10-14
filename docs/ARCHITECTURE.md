# Architecture Notes

## Current layout

```text
src/
├── main.rs          # Bevy app, visuals, keyboard input, HUD
├── lib.rs           # Public pure-Rust domain modules
├── world.rs         # Tile positions, map bounds, walkability
├── inventory.rs     # Item IDs, stacks, slot capacity
├── gathering.rs    # Resource nodes and gather transaction
├── interaction.rs  # Shared interaction-range calculation
└── quest.rs         # Moonpetal quest state
```

## Boundaries

- `main.rs` owns rendering, Bevy entities, input, and the demo scene.
- `world.rs`, `inventory.rs`, `gathering.rs`, `interaction.rs`, and `quest.rs` contain deterministic rules and do not depend on Bevy.
- Gathering attempts add the yield to inventory before decrementing node availability. This prevents a full inventory from consuming a resource.
- Inventory capacity counts distinct occupied item stacks. Adding more of an existing item uses its existing slot.
- The prototype currently integrates inventory-backed moonpetal collection. The tile map and generic tree/ore gathering models are tested foundations; visual tile collision and their in-world interactions remain roadmap work.

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
