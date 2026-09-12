# Showcase menagerie clip

The overlay is invisible to screen recorders (gdigrab, ddagrab), so the
showcase video is made from the offscreen `--snapshot` path instead:

```powershell
.\tools\showcase\render_frames.ps1   # 4 creatures x 120 frames, ~25 min (araneus dominates)
python tools\showcase\assemble.py    # clips + captions -> output\showcase\out\desktopfly-menagerie.mp4
```

`--snapshot-seconds` only advances the simulation in `--habitat` mode; frames
are deterministic from the seed. `DESKTOPFLY_SNAPSHOT_BG=13151f` composites
over the showcase card colour instead of the checkerboard.
