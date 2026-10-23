# Networking design (M12)

Ozbound is single-player and local-first, and stays that way by default: no account, server, or
telemetry is needed to play. This document says how the game is arranged so that a server-hosted or
peer-hosted mode could be added later without rewriting the rules. **Nothing here is networked
yet.** Multiplayer is optional (see M15) and not a prerequisite for anything.

## The shape

```
 input (mouse, keys)        render (meshes, UI, audio)
        |                              ^
        v                              |
   Command  ------------>  simulation (the rules)  -----> replicated state
 (ozbound::net)             one owner of truth              (components with NetId)
```

- **The simulation owns the truth.** Whoever runs it (the same process in single-player; a host in
  multiplayer) applies commands and decides what happened. A client never changes the world; it
  asks.
- **Players ask with `Command`s.** `net::Command` is plain data (walk, interact, choose, craft
  row, say) with a one-line text form. It names objects by `NetId`, never by entity, because entity
  numbers differ between machines.
- **Objects have stable names.** `NetId` is a 64-bit FNV-1a hash of `scope|kind|x|y`: the same on
  every machine and every run, and the same when a chunk is unloaded and loaded again. The scope is
  `overworld` for everything on the open world (world tiles are the same for everyone) and the
  region id for a place played on its own (a cave), whose tiles are local. Two objects of one kind
  cannot share a tile; the tests check that every shipped region and the whole composed overworld
  have unique ids.
- **A character owns what it has.** Bag, gear, skills, quests, and creatures met are components of
  the player entity (`PlayerProgress`), so a second player is a second entity, not a second copy of
  global state.
- **Rules stay pure.** Everything a server would need to check a command (reach, bag room, skill
  gates, quest prerequisites) is already in the Bevy-free library and unit-tested, so it can run in
  a headless server.

## What is in place

| Piece | Where |
| --- | --- |
| Stable object ids | `net::NetId`, `net::objects`, `streaming::assign_net_ids` (adds `Replicated`) |
| Command vocabulary and text form | `net::Command::{encode, decode}` |
| Per-character state as components | `components::PlayerProgress`, `ProgressRef`, `ProgressMut` |
| Rules without a window | the library crate (`lib.rs`) |
| Local, versioned save | `save.rs`, `save_store.rs` |

## What is not (the next steps, in order)

1. **Route input through commands.** Today `pointer`, `menu`, `keyboard`, `dialogue_ui`, and
   `pack_ui` act on the world directly. Each should emit a `Command` event that one system applies;
   single-player applies it at once. Do this first: it is useful on its own (it makes input
   testable) and every later step depends on it.
2. **One conversation, route, and panel per player.** `TalkState`, `TalkPartner`, `KeyTarget`, and
   `PackState` are global; they become per-player components.
3. **A snapshot and a delta format.** A text or binary form for "the objects of this chunk and their
   state" (nodes' actions left, petals picked, NPC tile and pose), versioned like the save. The
   save's `WorldChanges` is the seed of it. Today only the current chunk's changes are saved; the
   snapshot must cover every loaded chunk.
4. **An asset manifest.** A list of every file a world needs (map, quests, dialogue, recipes,
   bestiary, music) with a version and a hash, so a client can say what it has and a host can say
   what is missing. Files are already plain text with line-numbered errors, which makes hashing and
   refusing a mismatch simple.
5. **Transport and authority.** Pick one (a host a player runs, or a dedicated server) only when a
   real reason appears. The commands are small and can be checked, rate-limited, and logged. Never
   trust a client's claim of state; trust only its commands.
6. **NPCs and respawn timers on the host.** NPC brains are seeded and deterministic
   (`npc::seed_from`) and run in `Update` against the loaded chunks; on a host they must run once,
   for the union of the players' chunks. Respawn timers are not saved and would need to be.

## Rules for new code

- A change to the world on a player's behalf should be reachable from a `Command`.
- A thing that must exist on every machine gets a `NetId` from `net::NetId::of`, with its kind
  spelled by the `net::kind_*` helpers (so `net::objects` and the running world cannot disagree).
- Per-character state is a component of the character, not a resource.
- Do not let the server's view depend on rendering (meshes, cameras, audio) or on frame rate.
- Do not add a network dependency until step 5; prefer standard Rust and Bevy facilities.
