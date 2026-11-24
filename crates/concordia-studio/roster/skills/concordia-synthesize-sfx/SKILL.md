---
name: concordia-synthesize-sfx
description: "Makes sound effects from no samples and no API key: the procedural synthesizer (oscillators, noise, envelopes, filters, pitch sweeps) through gen_run, normalized and imported with provenance."
when_to_use: "Use when a task needs a new sound effect or a variation of one (pickup, hit, jump, explosion, UI click)."
user-invocable: false
---
# Synthesizing sound effects

## Steps
1. Start from a preset close to the brief: `pickup`, `hit`, `jump`, `explosion`, `ui_click` (`mcp__concordia__gen_list {}` shows the request schema: `base_hz`, `pitch_sweep`, `duration_ms`).
2. Generate: `mcp__concordia__gen_run {"provider": "sfx", "request": {"preset": "jump", "pitch_sweep": 1.4, "duration_ms": 220}, "seed": 3, "path": "assets/sfx/jump.wav"}`.
3. Keep it short and readable: most game SFX are 50–400 ms. Peaks stay below −1 dBFS (the provider normalizes); match the loudness of existing effects.
4. Make variations by changing the seed, not the whole recipe, so they read as one family.
5. `gen_run` already imported the file (its result has the asset `id`) and recorded the recipe in its sidecar. Write its cue, import the cue (`mcp__concordia__asset_import {"path": "assets/sfx/jump.cue.ron"}`) and hook it to its event as `concordia-sound-design` describes.

## Output contract
In the task comment: the file, preset, parameters and seed, duration, and which event plays it.
