# Background music

Each playable region in `assets/world/locations.world` can name a track here:

```text
[location MUNCHKIN_LANDS]
music = bluebrook.ogg      # played from this folder
theme = village            # built-in fallback, generated in code
```

- Supported formats: **ogg** (Vorbis), **wav**, and **mp4** (AAC audio). Use a plain file name
  with that extension, no folders.
- The file loops. Keep it under 64 MB (it is read whole into memory); a minute or two of ogg is
  typically 1-2 MB.
- If the file is missing, the area plays its built-in theme and the console says why. If the
  file does not start like the format its name claims, it is refused the same way (the game's
  audio decoder cannot be trusted with arbitrary bytes). A damaged file with a correct header
  can still fail to play.
- Music follows the ambient-sound volume and on/off setting.
- Only use music you have the right to use.

The tracks named in `locations.world` are `bluebrook.ogg` and `yellow_road_east.ogg`; none are
shipped, so the built-in themes play until you add your own.
