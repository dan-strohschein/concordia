---
name: concordia-record-replay
description: "Records a play session to a .replay.ron file and replays it to prove behavior: verify (every checkpoint hash must match) or resimulate (new hashes on current code)."
when_to_use: "Use when a task needs reproducible evidence of gameplay behavior, or a reviewer must re-run exactly what you saw."
user-invocable: false
---
# Recording and replaying

> `ab` runs and divergence bisection to the entity and field come in M3 phase 3.1. `mcp__concordia__input_inject` exists only in projects with an `[input] map`.

A replay is text (`tests/replays/<name>.replay.ron`, format version 0): a header (engine, scene and its hash, seed, tick rate, the script builds), the input changes per tick, and checkpoint hashes. It replays bit-exactly on the same build and platform, at any thread count. Recording and replaying never change the scene.

## Record
1. Open the scene: `mcp__concordia__scene_open {"path": "scenes/arena.scene.ron"}`.
2. Start: `mcp__concordia__replay_record {"action": "start", "path": "tests/replays/jump-over-gap.replay.ron", "checkpoint_every": 60}`. This starts a fresh play world from the scene.
3. Drive it: `mcp__concordia__input_inject` events at ticks, and `mcp__concordia__sim_step` ticks.
4. Stop and write the file: `mcp__concordia__replay_record {"action": "stop"}`.

Recording is lost if the play world is discarded (`sim_stop`, `scene_open`) before stop. If a script module is rebuilt and reloaded with different code while recording, stop refuses: record again.

## Clicking through menus
The in-game UI reads the pointer, clicks, wheel and keys through the input map's `ui_*` actions, so menu sessions record and replay like any other. Find the widget under a point with `mcp__concordia__ui_hit_test {"x": 640.0, "y": 380.0, "world": "play"}` (its `widget`, `path` and `part`), then click it: `mcp__concordia__input_inject {"events": [{"event": {"PointerMove": {"x": 640.0, "y": 380.0}}}, {"event": {"MouseButton": {"button": "Left", "pressed": true}}}]}`, a `sim_step`, the release (`"pressed": false`) and another step. Check the widget's `UiState` and value with `component_get` (`"world": "play"`) and that its signal (`UiPressed`, `UiToggled`, `UiSelected`) was raised. Keys: inject `{"Key": {"key": "Tab", "pressed": true}}` and its release to move focus. A different window size is input too: `{"Viewport": {"width": 1920.0, "height": 1080.0}}`.

## Run
- `verify` (default): `mcp__concordia__replay_run {"path": "tests/replays/jump-over-gap.replay.ron"}` compares every checkpoint. `ok: true` proves nothing changed. On a divergence it stops and reports the checkpoint `tick`, the `last_matching_tick` and the `parts` that differ (component names, or `@clock`, `@rng`, `@entities`, `@signals`).
- `resimulate`: `mcp__concordia__replay_run {"path": "tests/replays/jump-over-gap.replay.ron", "mode": "resimulate"}` replays the inputs on current code and returns new checkpoint hashes. Use it to prove a fix.
- A replay recorded with another scene, seed or build is refused (`ReplayMismatch`, listing the fields). Pass `"force": true` only to see where they diverge.

After a run the play world stays at the last tick, so `mcp__concordia__query_run {"world": "play"}` inspects the replayed state.

## Output contract
In the task comment: the replay path, the mode, the result (checkpoints matched, or the first divergence with its parts), and what it proves.
