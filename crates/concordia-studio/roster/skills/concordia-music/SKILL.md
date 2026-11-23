---
name: concordia-music
description: "Brings finished music into the game: streamed Ogg imports with loop points, looping cues on the Music bus, a music AudioEmitter in the scene, and seamless-loop checks with audio_render."
when_to_use: "Use when a task needs music playing in a scene: importing a track, setting where it loops, starting it with an emitter, setting its level on the Music bus, or checking that the loop is seamless."
user-invocable: false
---
# Music

Music is a long sound played by a looping cue on the `Music` bus. Writing new scores (`.score.ron`) waits for the music provider and `gen_run` (M3 phase 3.7); until then, bring finished tracks the project may use.

## Steps
1. **Import the track:** `mcp__concordia__asset_import {"path": "assets/music/arena.ogg"}`. In its sidecar `assets/music/arena.ogg.import.ron`, set `settings` to `{"stream": Some(true), "loop_start": 96000, "loop_end": Some(3168000)}`: `"stream"` decodes it while it plays (Ogg over 10 s streams anyway), and `loop_start` / `loop_end` are frames (48,000 a second at 48 kHz; the file's own rate otherwise). Without them the whole file loops. Re-import after editing. `mcp__concordia__asset_list {}` lists it.
2. **Write the cue** (`assets/music/arena.cue.ron`, below) and import it: `looping: true`, `bus: "Music"`, no `spatial`, a high `priority` so effects never steal it.
3. **Start it in a scene:** find the music entity with `mcp__concordia__query_run {"with": ["Name"], "select": ["Name"]}` (any entity works; music isn't positioned), then `mcp__concordia__component_set {"entity": "018bcfe5-6800-7000-a424-000000000009", "component": "AudioEmitter", "value": {"cue": "asset:0190b5c0-0000-7000-8000-0000000000c1"}}` and `mcp__concordia__scene_save {}`. Read it back with `mcp__concordia__component_get {"entity": "018bcfe5-6800-7000-a424-000000000009", "component": "AudioEmitter"}`.
4. **Check the loop:** `mcp__concordia__audio_render {"cue": "asset:0190b5c0-0000-7000-8000-0000000000c1", "seconds": 130.0, "path": "assets/music/checks/arena.wav"}` renders two loops; listen to the join. Rendering is sample-exact and the same twice (its `hash`).
5. **Set the level:** `mcp__concordia__audio_buses {"set": [{"name": "Music", "gain_db": -6.0}]}`, then render a scene with effects: `mcp__concordia__sim_step {"ticks": 600}` and `mcp__concordia__audio_render {}`. Tell the sound designer the gain you settled on, for the bus layout.
6. **Preview** a cue on a device (in `concordia run`) with `mcp__concordia__audio_play {"cue": "asset:0190b5c0-0000-7000-8000-0000000000c1"}` and `mcp__concordia__audio_stop {}`. Pause the music in the running game with `mcp__concordia__component_patch {"entity": "018bcfe5-6800-7000-a424-000000000009", "component": "AudioEmitter", "patch": {"playing": false}, "world": "play"}`, and end play with `mcp__concordia__sim_stop {}`.

A music cue:
```ron
(
    sounds: [
        (sound: "asset:0190b5c0-0000-7000-8000-0000000000c2"),
    ],
    bus: "Music",
    priority: 250,
    looping: true,
)
```

## Rules
- A loop point is a frame index: `loop_end` is excluded, so the loop is `loop_end − loop_start` frames long.
- Music never crossfades yet; one emitter per track, turned on and off with `playing`.
- Music sits under the effects: aim for an RMS several dB below the sound designer's cues.
