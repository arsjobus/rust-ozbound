# Background music

Each playable region in `assets/world/locations.world` can name a track here:

```text
[location MUNCHKIN_LANDS]
music = bluebrook.mp3      # played from this folder
theme = village            # built-in fallback, generated in code
```

and, for the open world, each zone in `assets/world/overworld.world` names its own track the
same way (`music = bluebrook.mp3`). Both places must name the file you want: the zone decides
what plays while you walk the overworld, the location is used off it (inside a cave).

- Supported formats: **ogg** (Vorbis), **wav**, and **mp3**. Use a plain file name with that
  extension, no folders. (mp4/m4a is not supported.)
- The file loops. Keep it under 64 MB (it is read whole into memory); a minute or two of ogg is
  typically 1-2 MB.
- If the file is missing, the area plays its built-in theme and the console says why. If the
  file does not start like the format its name claims, it is refused the same way (the game's
  audio decoder cannot be trusted with arbitrary bytes). A damaged file with a correct header
  can still fail to play.
- Music follows the ambient-sound volume and on/off setting.
- Only use music you have the right to use.

The tracks named in `locations.world` and `overworld.world` are `bluebrook.mp3` (shipped) and
`yellow_road_east.ogg` (not shipped yet, so the road plays its built-in theme).

`bluebrook.wav` in this folder is an old uncompressed copy of the same tune from before mp3
played directly; nothing refers to it any more and it can be deleted.
