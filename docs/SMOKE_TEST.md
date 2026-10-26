# Smoke-test checklist

Run this before calling a build good. The unit and integration tests cover the rules; this list
covers what needs a window, a keyboard, and your eyes. Record the platform, the date, and the
commit next to your results (M0 asks for macOS, Windows, and Linux).

Start from a clean state (`saves/` removed) unless a step says otherwise. Run `cargo test` and
`cargo clippy --all-targets --all-features -- -D warnings` first.

## 1. Launch and quit

- [ ] `cargo run` opens a window of at least 800 x 600 and the meadow draws.
- [ ] Esc opens the pause menu; Down, Down, Down, Enter on **Quit game** exits cleanly.
- [ ] Closing the window with its close button also exits, and the last action was saved.

## 2. Keyboard only (do not touch the mouse for this section)

- [ ] W, A, S, D step the player; holding a key keeps walking; a blocked step says "Something is in the way."
- [ ] Turn the camera with the arrow keys (a quarter turn or so): W still moves "up the screen".
- [ ] Tab selects the nearest thing; the hover label names it and says how many steps away; Tab again moves on; Shift+Tab goes back; a full lap returns to the start.
- [ ] Enter goes to the selected thing and uses it (walk to the guide and talk; walk to a tree and chop).
- [ ] X on a selected thing prints its description in the chat area (and not in the HUD).
- [ ] In a conversation, 1-9 pick choices and Esc closes it; in the crafting panel (C), 1-9 pick rows and C or Esc closes it.
- [ ] J opens the quest book, B the bag, K skills; each key closes its own panel, opening another switches to it; PageUp / PageDown turn pages when there are several.
- [ ] + and - zoom the camera.

## 3. Mouse

- [ ] Left-click ground, an object, and a blocked tile (walk / interact / "You can't walk there.").
- [ ] Right-click a tree, the guide, and the ground; the menu appears, stays on screen near the window edges, and a click outside closes it without walking.
- [ ] Scroll zooms; middle-drag turns the camera.

## 4. Pause menu and settings

- [ ] Esc with nothing open pauses: the player stops mid-walk, a gathering timer does not advance, and the ambient sound keeps playing.
- [ ] Volume: Left / Right change it step by step and you can hear it; at 0 it is silent; Enter at 10 wraps to 0.
- [ ] Ambient sound off is silent; back on returns at the chosen volume.
- [ ] Text size Normal / Large / Larger visibly scales the HUD, panels, and menu; nothing is cut off at the window edge.
- [ ] Resize the window to the smallest allowed size at Larger: text shrinks to fit instead of clipping.
- [ ] Esc steps back from Settings to the first page, and again resumes; clicking Resume does not also walk the player.
- [ ] Quit and restart: volume, ambient, text size, and overlay settings are as you left them (`saves/settings.txt`).
- [ ] Put `volume = 99` in `saves/settings.txt` and start: the game says the settings could not be read, uses defaults, and the file is unchanged after you change a setting and quit.

## 5. Panels at the smallest window (800 x 600)

- [ ] Talk to every villager and read each line: no dialogue text or choice is cut off (the portrait stays visible).
- [ ] Open the crafting panel with a busy bag: every row is visible.
- [ ] Start every quest and open the quest book: the text pages cleanly, nothing is cut off at the bottom.
- [ ] With a side panel open, the top-left quest tracker does not run under it.
- [ ] Fill the bag with every item and equip gear, then open the bag panel: nothing is cut off; the toolbar and chat area do not overlap each other or the panel.
- [ ] Talk to a villager at this size: the dialogue panel sits above the chat area without covering it.

## 6. Performance

- [ ] F3 shows the frame-time overlay; it reads "smooth" (about 60 fps) while walking the village on the target machine.
- [ ] `OZBOUND_PROFILE=1 cargo run`, play for a minute, quit: the summary line is printed. Note the average and worst frame here: ______
- [ ] No hitch (a worst frame over about 100 ms) when opening the pause menu, a conversation, or a side panel.

## 7. Saving still works

- [ ] Pick two moonpetals, chop a tree until spent, quit, restart: the world and quest state are as left (see the M6 manual check in `ROADMAP.md` for the full list).

## 8. HUD, chat, and panels (M8)

- [ ] The top-left shows only the quest tracker (and a hover label when the mouse is over something); no game title and no list of controls.
- [ ] The toolbar (Bag, Skills, Quests) is top-right; clicking a button opens its panel, clicking it again closes it, and the open one is highlighted.
- [ ] Clicking a toolbar button, the chat area, or an open panel does not make the player walk.
- [ ] Gather something, examine a tree (X or right-click Examine): the lines appear in the chat area, in different colours, and nothing appears in the HUD.
- [ ] Press T, type a line, press Enter: it appears as `You: ...`. While typing, W A S D, Tab, Space, 1-9 and the arrow keys do not move the player or the camera or pick anything.
- [ ] Press / then `wave`, Enter: the player raises an arm and the chat says `You wave.`; the same for `/dance` and `/cheer`. The pose eases in and out and the player ends standing normally.
- [ ] `/jump` and `/wave please` give a short explanation in the chat and do nothing else; `/help` lists the commands.
- [ ] Start walking or chopping: any emote stops. Type `/wave` while walking: the chat says you cannot.
- [ ] Esc while typing cancels the line (and does not open the pause menu); Esc again with nothing open pauses.
- [ ] PageUp / PageDown scroll the chat when no side panel is open; a note under the log says older messages are showing; a new message returns to the newest.
- [ ] Read a signpost: it opens in the dialogue panel titled Signpost with one choice, Done reading (1 or click); Esc and walking away close it too. The text is also recorded in the chat.

## 9. Houses, plots, bridge, thickets, regions, and music (M9)

- [ ] The game starts in Bluebrook; the village, plots, and houses are well spaced apart. The forest edge is made of dark leafy thickets, with no sticks poking out of them; the few tall pines that can be chopped (hover says "tree") look different from them.
- [ ] The river bridge looks like a wooden bridge (planks, rails, posts at each end) and you can walk across it; the river flows under it.
- [ ] Three plots east of the river each have a house, with garden beds (cabbages, tomatoes, pumpkins) and a vineyard (trellis rows with grapes). Garden beds and vines block walking; the grass lanes between them do not.
- [ ] Walk into each house through its doorway: the roof disappears and the walls drop to a low rim while you are inside; walk out and the house is whole again. Walls block you. The walls are thin and the room feels roomy; looking straight down on the low rim and floor, nothing flickers (no z-fighting) at any camera angle.
- [ ] Mistress Pell is inside her house; you can walk in and talk to her.
- [ ] Walk east along the main road to its end (x = 38): the chat says you travel to the Yellow Brick Road, east; the world changes, the quest tracker, chat, bag and panels are unchanged, and you can still talk to nobody on the road (it has no villagers yet).
- [ ] Cross the road's three-tile bridge; the sign near it reads correctly; the waystation has a house, a garden, and vineyard rows.
- [ ] Walk back west to the road's west end: you return to Bluebrook beside its east end, not on the exit tile.
- [ ] Quit while on the road and restart: you are on the road where you left off. Delete saves/game.txt and restart: you start in Bluebrook.
- [ ] Music: the village and the road play different tunes. Drop an ogg, wav, or mp3 file into `assets/bgm/` under the name the zone asks for (the start village asks for `bluebrook.mp3`; see `assets/world/overworld.world`), restart: it plays instead. A text file renamed `bluebrook.ogg` is refused with a console note and the built-in theme plays. Turning ambient sound off (pause menu) silences the music too.

## 10. Villagers walk and talk (M10)

- [ ] Wait a minute near the village: Mistress Pell, Brae, and Ivo stroll around their homes (Pell a little, Ivo the most), pause, and stroll again. Tamsin by the bridge and the guide never leave their tiles. Nobody walks into water, trees, walls, plants, waymarks, the creatures, a doorway, or onto you.
- [ ] Watch Brae, Ivo, Tamsin, and the guide for a few minutes: now and then one waves, dances, or cheers (Brae cheers, Ivo and Tamsin dance, the guide and Tamsin wave), with the same easing and hops as your own emotes, and then goes back to what they were doing. Pell never gestures.
- [ ] Click a villager who is strolling: they finish their step, stop, and you walk up beside them; the conversation opens. They never wander off while you walk over, and the quest tracker/dialogue behave as before.
- [ ] During a conversation the villager stands still and turns to face you (move the camera around to check). Pick "goodbye", then repeat with Esc, and again by walking away: each time the villager turns to you, waves once, and the chat says `<name> waves goodbye.`
- [ ] After the wave they rest a moment and carry on with their day. Click a villager and then click the ground somewhere else before arriving: they just carry on, with no wave.
- [ ] Read a signpost: nobody waves.
- [ ] Walk so that a wandering villager is in your way: they stop short of your tile (you can still walk through them as before). Right-click a villager while they are moving: the menu opens for them, and Talk to still walks you to where they stop.
- [ ] Hover over a moving villager: the "Talk to ..." label follows their body, not the tile they left.
- [ ] Walk east across the chunk border and come back: the villagers are back on their home tiles (they are not saved) and carry on.

## 11. Saving, progress as components, and object ids (M12)

- [ ] Take a save made before M9 (or edit `saves/game.txt` so its `world` line says `world 40 24`), start the game: the chat says your saved game did not fit this version of the world and names `saves/game.txt.unusable-1`; your bag, gear, skills, and quest progress are as they were, you stand at the start of the village, and the console prints the reason. Quit and start again: a plain "Welcome back" and no new `.unusable` file.
- [ ] Walk onto the tile where Brae, Ivo, or Pell started (they have wandered off), wait two seconds, quit, and start: you are restored on that tile and the chat says "Welcome back".
- [ ] Replace `saves/game.txt` with some text that is not a save: a new game starts, the chat names the copy, `saves/game.txt.unusable-N` holds your text, and progress is saved from then on.
- [ ] Make `saves/` read-only while playing (or fill the disk): the chat says progress could not be saved and that the game will keep trying, once. Restore access: within about 30 seconds the chat says "Saving works again" and `saves/game.txt` is current.
- [ ] Gather, craft, equip, and talk to the guide as before; the bag, skills, and quest panels (B, K, J), the quest tracker, and the crafting panel (C) all update. Walk through a cave exit and back: your bag, gear, skills, quests, and field notes are unchanged.
- [ ] Quit and start: everything above is restored.

## 12. World editor and terrain meshes (M12 in ROADMAP.md)

- [ ] Start the game and look around: the ground looks as before (shaded grass, path, water with low banks). Walk across a chunk border (east along the road): no gap, no doubled wall, no flicker at the seam, including where water meets grass.
- [ ] Type `/edit`: a ribbon appears along the top (Select, Select & Move, Place, Undo, Redo, Export asset, Exit editor) and the chat says the editor is on. Undo, Redo, and Export asset are greyed. `/help` lists `/edit`. Clicking the ground no longer walks the player; WASD still does.
- [ ] Click the signpost: an X marks it and the ribbon names it. Click bare grass in the same scene: it moves there, the chat says "Moved signpost.", Undo lights up. Click Undo (or Ctrl+Z): it is back and the chat says "Undid: ...". Click Redo (or Ctrl+Y): it moves again. Quit and start: the sign is where you left it (`saves/edits.txt` exists). Undo it again and the file disappears.
- [ ] Select a tree and move it onto grass: the old tile is grass, the new tile is a tree you can chop. Try to move it onto water, a path, or the tile you stand on: it stays and the chat says why.
- [ ] Select Pell the apothecary and move her to another free tile: she stands there and wanders from there when you leave the editor. Try to Delete her: the chat says that would break the world and she stays.
- [ ] Right-click a tree: the menu offers Select, Remove, Cancel (and Move here if something is selected). Remove it: it is gone, grass remains. Undo: it is back in the same place.
- [ ] Click Place: the label cycles (Honeyberry bush, Copper vein, ...). Click grass: it is placed. Right-click grass: Place here is offered.
- [ ] Select a villager and click Export asset: the chat names `assets/library/<id>.asset` and the file reads back the villager's name, look, and behavior.
- [ ] Move something across a scene border (select, then click a tile past the edge): the chat says an object stays in its own scene.
- [ ] Leave with Exit editor or `/edit`: the ribbon goes, clicking walks again, villagers wander again.
- [ ] Put `garbage` in `saves/edits.txt` and start: the console and chat say your edits were not applied; the file is left as it is and edits made now are not saved.
- [ ] Enter a cave and type `/edit`: the chat says the editor works on the open world.

## 13. Editor, chunks, hills and geometry export (M13 in ROADMAP.md)

- [ ] The ground rolls in gentle faceted hills where nothing is built, and is flat under the path, the bridge, the houses, the gardens and the vineyard. People, trees, signs and the player stand on the ground (no floating, no sinking) and the player rises and falls as they walk. Water is lower than its banks.
- [ ] Walk east along the road past the chunk border at x = 64 and back: no gap, no step, no flicker in the ground at the seam. Walk to the north opening of Bluebrook (x = 31-32, y = 38-39): an empty meadow lies beyond.
- [ ] Click on a hillside, a slope near water, and a far hill: the player walks to the tile drawn under the cursor, and the hover label and the click marker sit on the ground there.
- [ ] `/edit`, then right-click a thicket: the menu offers Select thicket, Remove thicket, Cancel. Select it (left-click), then click bare grass: it moves there and the chat says "Moved thicket.". Try to move it onto the path, water, a tree or yourself: it stays and the chat says why. Undo and Redo work. Remove it and Undo.
- [ ] Right-click different things (tree, copper vein, a villager with a long name, bare grass with something selected): every row is one line, nothing overlaps, and the menu stays inside the window. Repeat with a larger text size in the pause menu and in a narrow window.
- [ ] Select a tree or copper vein and click Export asset: the chat names the `.asset` file and then the `.obj` file with its triangle count. Open `assets/library/<id>.obj` (and its `.mtl`) in Blender or MeshLab: the model looks like the one in the game, upright, in its colours.

## 14. Models from files and modular fences (M14 in ROADMAP.md)

- [ ] Start the game with `assets/library/tree.obj` renamed away: the console says `model tree: no file, so generated ...`, the file is back, and the trees look the same as before. Do the same for `fence_post.obj` and `fence_rail.obj`.
- [ ] Damage `signpost.obj` (add a line `v 1 2`): the console names the file and line, the game still shows signposts, and the file is left as you made it. Restore it.
- [ ] Trees keep their three leaf colours; copper veins show orange flecks and plain rocks show none; moonpetals still glow; chop a tree until spent and the crown disappears and returns; same for mint, berries, and copper.
- [ ] `/edit`, choose Place until the label reads Fence, click bare grass three times in a row: the pieces join into one rail. Place one beside the road chunk seam: it joins across it. Select a middle piece, move it away, and the run reconnects around the gap. Remove, Undo, Redo.
- [ ] Open `assets/library/tree.obj` in Blender: the crown is a separate group.
