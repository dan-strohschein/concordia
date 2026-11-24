---
name: concordia-sound-design
description: "Imports sounds (wav, ogg) and sets their sidecars, writes .cue.ron cues and .buses.ron layouts, wires cues to gameplay (AudioEmitter, PlayCue, AnimEventCues, AudioListener), and checks them with audio_render, audio_inspect and audio_buses."
when_to_use: "Use when a task needs a sound effect heard in the game: importing it, making a cue with variations, hooking it to an emitter, a gameplay event or an animation event, balancing buses, or checking that it plays on the right tick at the right level."
user-invocable: false
---
# Sound design

Audio is presentation: what plays never changes the simulation or a world hash. Gameplay asks for sound with **hashed requests** (an `AudioEmitter` component, a `PlayCue` signal, `AnimEventCues` on a character) and the engine's mixer does the rest. Headless engines (MCP, tests, CI) mix **offline**, sample-exact: 48 kHz, 800 frames per tick at 60 Hz, so you listen by numbers and by file.

## Steps
1. **Import the sound:** `mcp__concordia__asset_import {"path": "assets/sfx/hit.wav"}` (`.wav` 8–32-bit or float, `.ogg` Vorbis; mono or stereo). Its sidecar `assets/sfx/hit.wav.import.ron` takes `settings` such as `{"gain_db": -3.0, "loop_start": 0, "loop_end": None, "stream": None}` (frames for the loop points; `stream` only matters for Ogg, which streams when longer than 10 s). Re-import after editing it. `mcp__concordia__asset_list {}` lists imported sounds and cues.
2. **Write a cue** (`assets/sfx/hit.cue.ron`; `mcp__concordia__schema_get {"name": "Cue"}` gives its shape) and import it like a sound. Each play draws the sound by weight, then a volume and a pitch from the ranges, from `seed`: the same seed always renders the same audio.
3. **Wire it**, on an entity found with `mcp__concordia__query_run {"with": ["Name"], "select": ["Name", "Transform"]}` and read with `mcp__concordia__entity_get {"id": "018bcfe5-6800-7000-a424-000000000002"}`:
   - A loop or ambience: `mcp__concordia__component_set {"entity": "018bcfe5-6800-7000-a424-000000000002", "component": "AudioEmitter", "value": {"cue": "asset:0190b5c0-0000-7000-8000-0000000000a2", "playing": true, "volume_db": -3.0}}`. A looping cue loops while `playing` is true; a one-shot cue plays each time `playing` turns true. Positioned at the entity's `Transform` when the cue has `spatial`.
   - Footsteps and swings: `mcp__concordia__component_set {"entity": "018bcfe5-6800-7000-a424-000000000003", "component": "AnimEventCues", "value": {"cues": {"step_left": "asset:0190b5c0-0000-7000-8000-0000000000a3", "swing": "asset:0190b5c0-0000-7000-8000-0000000000a2"}}}` on the character that plays the clip (the animator names the events).
   - Gameplay one-shots: the gameplay programmer raises the `PlayCue { cue, volume_db }` signal on an entity (and `StopCue { cue }` to stop what it started); give them the cue id and the moment.
   - The listener: `mcp__concordia__component_set {"entity": "018bcfe5-6800-7000-a424-000000000004", "component": "AudioListener", "value": {"gain": 1.0}}` on the camera or the player (the lowest-id one is heard). Without one, positioned sounds play centred.
   - Save the scene you changed: `mcp__concordia__scene_save {}`.
4. **Listen:** `mcp__concordia__audio_render {"cue": "asset:0190b5c0-0000-7000-8000-0000000000a2", "seconds": 1.0, "path": "assets/sfx/checks/hit.wav"}` renders the cue alone and reports `peak_dbfs`, `rms_dbfs` and a `hash`. `mcp__concordia__audio_play {"cue": "asset:0190b5c0-0000-7000-8000-0000000000a2", "at": [2.0, 0.0, 0.0]}` previews outside the world (heard on a device in `concordia run`); `mcp__concordia__audio_stop {}` stops previews.
5. **In the game:** `mcp__concordia__sim_step {"ticks": 120}`, then `mcp__concordia__audio_render {"path": "assets/sfx/checks/play.wav"}` (the play world's mix so far; the file keeps the newest 30 s) and `mcp__concordia__audio_inspect {}` (each voice's cue, entity, bus, gains and position, the listener, and steal, drop and underrun counts). Turn an emitter off in the running game with `mcp__concordia__component_patch {"entity": "018bcfe5-6800-7000-a424-000000000002", "component": "AudioEmitter", "patch": {"playing": false}, "world": "play"}`, read one with `mcp__concordia__component_get {"entity": "018bcfe5-6800-7000-a424-000000000002", "component": "AudioEmitter", "world": "play"}`, and stop with `mcp__concordia__sim_stop {}`. A cue that can't play is a warning in `mcp__concordia__log_tail {"level": "warn"}`.
6. **Balance:** `mcp__concordia__audio_buses {"set": [{"name": "SFX", "gain_db": -3.0}]}` changes a bus until the engine closes and reports each bus's peak since the last call; write the levels you settle on into the layout file.

A cue (`.cue.ron`, canonical text):
```ron
(
    sounds: [
        (sound: "asset:0190b5c0-0000-7000-8000-0000000000b1", weight: 2),
        (sound: "asset:0190b5c0-0000-7000-8000-0000000000b2", weight: 1),
    ],
    volume_db: (min: -2.0, max: 0.0),
    pitch_semitones: (min: -1.0, max: 1.0),
    bus: "SFX",
    priority: 200,
    max_instances: Some(4),
    spatial: Some((min_distance: 1.0, max_distance: 30.0)),
    seed: 7,
)
```

A bus layout (`.buses.ron`; Master is implicit, a bus with no `send` goes to Master):
```ron
(
    buses: [
        (name: "SFX", gain_db: -3.0),
        (name: "Steps", gain_db: -6.0, send: Some("SFX")),
        (name: "Music", gain_db: -6.0),
    ],
)
```

## Rules
- Levels: peaks below −1 dBFS; match the RMS of the cues beside it. Gains are 20·log₁₀ of an amplitude: −6 dB halves it.
- A signal raised on tick t sounds on tick t + 1; offline, a sound started on tick t begins at frame 800·t (60 Hz).
- When voices run out, a new sound takes the lowest-priority oldest voice it outranks or ties; otherwise it is dropped (`drops` in `audio_inspect`). `max_instances` stops the oldest of the same cue.
- Cues and sounds load when play starts: edit, re-import, then play again.
- Ask a human to point `[audio] buses` in `concordia.toml` at your layout; you may not edit `concordia.toml`.
