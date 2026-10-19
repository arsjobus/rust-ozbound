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
- [ ] X on a selected thing prints its description in the journal line.
- [ ] In a conversation, 1-9 pick choices and Esc closes it; in the crafting panel (C), 1-9 pick rows and C or Esc closes it.
- [ ] J opens the journal; PageUp / PageDown turn pages when there are several.
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
- [ ] Start every quest and open the journal: the text pages cleanly, nothing is cut off at the bottom.
- [ ] With the journal open, the top-left HUD text does not run under it.

## 6. Performance

- [ ] F3 shows the frame-time overlay; it reads "smooth" (about 60 fps) while walking the village on the target machine.
- [ ] `OZBOUND_PROFILE=1 cargo run`, play for a minute, quit: the summary line is printed. Note the average and worst frame here: ______
- [ ] No hitch (a worst frame over about 100 ms) when opening the pause menu, a conversation, or the journal.

## 7. Saving still works

- [ ] Pick two moonpetals, chop a tree until spent, quit, restart: the world and quest state are as left (see the M6 manual check in `ROADMAP.md` for the full list).
