# Game Design: Ozbound

## High-level pitch

A gentle, exploratory, low-poly 3D RPG set in a reimagined fairy-tale country inspired by the 1900 Oz novel. It borrows the *shape* of a classic MMORPG's satisfying world loop—explore, talk, gather, learn skills, complete quests, improve equipment—but starts as a private, offline, single-player game.

## Design pillars

1. **A world to wander.** Landmarks, paths, hidden corners, and small discoveries should reward curiosity without requiring a map full of icons.
2. **Useful, understandable activities.** Gathering, crafting, and skills should have clear purposes and visible outcomes.
3. **Quests with character.** Short stories and human-scale requests are preferred over repetitive errands.
4. **Player-directed pace.** No daily login rewards, streak pressure, loot-box mechanics, or artificial urgency.
5. **Cozy wonder with occasional danger.** A fairytale mood, not a grimdark world.
6. **Small systems that compose.** Quest, inventory, skill, and world rules should be independently testable.

## Originality boundaries

The 1900 novel is a source of public-domain inspiration in the United States, but later Oz books, translations, illustrations, film adaptations, and branding can have separate rights. Keep this project anchored to the first novel's public-domain material and original additions. Do not copy the 1939 film's visual designs, dialogue, songs, costumes, or other distinctive adaptation elements. Use original models, music, names for new characters, and quest writing.

The RuneScape reference is about interaction patterns, pacing, and a general low-poly feel—not copying its models, textures, names, interface, maps, quests, or exact systems. Character, tree, and rock models are built from scratch.

## Look and controls

- **Art direction:** chunky, flat-shaded low-poly 3D: faceted trees, boulders, blocky characters, a bright storybook palette. Models are original and generated from simple primitives; they should read clearly at a glance from a raised third-person camera.
- **Controls:** point-and-click, in the tradition of classic MMORPGs. Left-click a tile to walk there; left-click an object to walk beside it and interact. Hovering names the action. The camera orbits with middle-drag or arrow keys and zooms with the wheel.
- **Movement is tile-based.** Routes are planned over whole tiles; characters glide between tile centers. This keeps rules simple, testable, and readable.
- **Accessibility:** mouse-first play must not be the only way to play; a keyboard path is planned (see M7).

## Core play loop (target)

Explore a region → meet a character or discover a problem → gather clues/materials → use a skill or craft an item → resolve the quest → unlock a new route, recipe, story, or useful tool.

## First vertical slice

- One small low-poly meadow.
- One original village guide NPC.
- Three collectible moonpetals.
- A short dialogue prompt.
- A completed objective message.
- No combat, networking, monetization, or procedural grind.

## Future skills (candidate list)

- Foraging: identify useful plants.
- Woodcraft: collect and shape timber.
- Herbalism: prepare simple remedies.
- Wayfinding: notice landmarks and open shortcuts.
- Tinkering: repair odd mechanisms found along the road.

Skills should unlock possibilities rather than exist only as XP bars.

## Questions to resolve before expanding

- Is the protagonist an original traveller, or a new character arriving from Kansas? Decide based on desired closeness to the novel.
- Should Dorothy and the other book characters appear, or should the game use an original cast in the same broad setting?
- What is the tone: cozy adventure, classic storybook, or more open-ended sandbox?
- Is the world a connected map, or a set of compact regions with clear transitions?
