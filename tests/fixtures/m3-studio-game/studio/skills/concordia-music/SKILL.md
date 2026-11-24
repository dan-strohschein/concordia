---
name: concordia-music
description: "Writes scores rendered by the music provider, or brings finished tracks: streamed Ogg imports with loop points, looping cues on the Music bus, a music AudioEmitter in the scene, and seamless-loop checks with audio_render."
when_to_use: "Use when a task needs music playing in a scene: importing a track, setting where it loops, starting it with an emitter, setting its level on the Music bus, or checking that the loop is seamless."
user-invocable: false
---
# Music

Music is a long sound played by a looping cue on the `Music` bus. Write it as a score for the music provider, or bring a finished track the project may use.

## Writing a score
`mcp__concordia__gen_list {}` gives the score schema. A score is a tempo, bars and tracks of notes on the beat grid (MIDI pitch, 60 = middle C); instruments are `Sine`, `Square`, `Saw`, `Pluck`, `Kick`, `Snare` and `Hat`. Keep the score next to its output as `assets/music/<name>.score.ron` so it can be edited and rendered again.
`mcp__concordia__gen_run {"provider": "music", "request": {"bpm": 96.0, "beats_per_bar": 4, "bars": 8, "loop_from_bar": 2, "tracks": [{"instrument": "Pluck", "volume": 0.6, "notes": [{"beat": 0.0, "beats": 1.0, "pitch": 57}]}, {"instrument": "Kick", "volume": 0.8, "notes": [{"beat": 0.0, "beats": 0.5, "pitch": 36}]}]}, "seed": 1, "path": "assets/music/arena.wav"}`
It renders the WAV, imports it, and sets `loop_start` / `loop_end` in its sidecar (from `loop_from_bar` to the end), so skip step 1 below and go on from the cue. Change the score and render again rather than editing the WAV.

## Bringing a finished track

## Steps
1. **Import the track** (a finished one; a rendered score is already imported): `mcp__concordia__asset_import {"path": "assets/music/arena.ogg"}`. In its sidecar `assets/music/arena.ogg.import.ron`, set `settings` to `{"stream": Some(true), "loop_start": 96000, "loop_end": Some(3168000)}`: `"stream"` decodes it while it plays (Ogg over 10 s streams anyway), and `loop_start` / `loop_end` are frames (48,000 a second at 48 kHz; the file's own rate otherwise). Without them the whole file loops. Re-import after editing. `mcp__concordia__asset_list {}` lists it.
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
