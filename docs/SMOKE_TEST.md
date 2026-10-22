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
