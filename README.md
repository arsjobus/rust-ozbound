# Ozbound

**Ozbound** is the working title for a relaxed, top-down role-playing adventure inspired by the exploration, skills, quests, gathering, and readable world of classic MMORPGs such as RuneScape. It is a single-player Rust + Bevy project at this stage—not a RuneScape clone and not an online game.

The setting takes inspiration from L. Frank Baum's 1900 novel *The Wonderful Wizard of Oz*. The game should develop its own quests, dialogue, characters, art direction, and world structure rather than reproduce a later film adaptation.

## Current starter prototype

- A small top-down meadow rendered with simple shapes.
- WASD or arrow-key movement.
- A player marker and a nearby village guide.
- Walk near the guide and press **E** to interact.
- A small quest objective: collect three moonpetals by walking over them.
- HUD with quest progress and a simple journal message.
- Unit tests for the quest state.

This is deliberately a small, understandable foundation. Art, maps, inventory, skills, and dialogue are future work.

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
| WASD / arrow keys | Move |
| E | Talk to the village guide when nearby |
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
