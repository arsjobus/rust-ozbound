# Architecture Notes

## Current layout

```text
src/
├── main.rs          # Bevy app wiring and system order (binary crate)
├── setup.rs         # Startup: terrain, props, characters, lights, camera, HUD
├── models.rs        # Original low-poly models built from primitives
├── components.rs    # Bevy components and resources
├── pointer.rs       # Hover label, left-click walk/interact, click markers
├── actions.rs       # Route following, talk/pick/gather, limb animation
├── camera_rig.rs    # Orbit camera that follows the player
├── menu.rs          # Right-click menu: open, choose a row, draw the panel
├── hud.rs           # Objective, bag, journal, hover label
├── lib.rs           # Public pure-Rust domain modules (library crate)
├── world.rs         # Tile positions, map bounds, walkability, 3D tile centers
├── pathfinding.rs   # BFS routes, no corner cutting, Adjacent/OnTile approach
├── picking.rs       # Ray vs ground and boxes: what is under the cursor
├── task.rs          # Route + pending interaction; step and facing helpers
├── orbit.rs         # Orbit-camera angles, clamps, zoom
├── interaction.rs   # Interaction kinds and hover text; range helpers
├── inventory.rs     # Item IDs, stacks, slot capacity
├── gathering.rs     # Resource nodes, gather transaction, placement, keep-going rule
├── context_menu.rs  # Menu rows, placement on screen, row hit-testing
├── respawn.rs       # One-shot countdown for respawning nodes
├── level.rs         # Level data file parser/validator: map, player, guide, moonpetals, nodes
├── scene.rs         # Built-in default level (compiled in) and deterministic per-tile variation
└── quest.rs         # Moonpetal quest state
```

## Boundaries

- The **library crate** (`lib.rs` and the modules it lists) is pure Rust: no Bevy types, fully unit-tested without a window. The **binary crate** (`main.rs` and its modules) renders, reads input, and connects the two.
- Input flow: `pointer.rs` turns the cursor into a ray, `picking::pick` says what it hits, `pathfinding::plan_route` plans a route, and `Task` stores it. `actions.rs` glides the player along the route and runs the interaction on arrival.
- World space is 3D, Y-up. One tile is one world unit (`TILE_SIZE` = 1.0). Map tile `(x, y)` sits at world `(x, h, -y)`: map "north" is Bevy's forward direction (`-z`). Use `WorldMap::tile_center_3d` and `picking::ground_to_tile` to convert; text maps list the top row first.
- Gameplay is tile-based: walkability, routes, and interaction ranges ("stand on or orthogonally beside the target tile") are decided on tiles. Smooth motion is display only: the logical `Player.tile` updates as each tile is reached.
- A new click plans from the tile being walked toward, and the stride in progress is kept, so the player never snaps backwards.
- Collision treats anything outside the map as blocked, so no separate position clamp is needed.
- Gathering attempts add the yield to inventory before decrementing node availability, so a full inventory never consumes a resource. Repeat gathering stops when `should_keep_gathering` says so (spent node, full bag, or any failure) or when the player starts walking.
- Spent trees and veins get a `Respawning` timer the moment an action empties them; when it finishes `GatheringNode::restock` refills them and their visual is shown again.
- Right-click opens a menu that owns the mouse while open (`pointer` skips input), so one click never both picks a row and walks. Row geometry comes from `MenuMetrics`, shared by the UI layout and the click test.
- Inventory capacity counts distinct occupied item stacks. Adding more of an existing item uses its existing slot.
- Visuals are generated in code (flat-shaded primitives), so there are no external assets or licences to track. The world is described by a plain-text level file (`assets/maps/meadow.map`, parsed by `level.rs`); a copy is compiled in as a fallback for when the file is not found. An invalid level file stops startup with a file/line error rather than falling back.

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
