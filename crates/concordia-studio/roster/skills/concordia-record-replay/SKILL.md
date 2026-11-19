---
name: concordia-record-replay
description: "Records a play session or a scripted input scenario to a .replay.ron file and replays it to prove behavior: verify (bit-exact), resimulate (on current code) and ab comparison, with divergence bisection."
when_to_use: "Use when a task needs reproducible evidence of gameplay behavior, or a reviewer must re-run exactly what you saw."
user-invocable: false
---
# Recording and replaying

> **Not available yet:** needs `replay_record` and `replay_run` (M3 phase 3.1); `input_inject` exists only in projects with an `[input] map`. Until then, report evidence with simulation steps and screenshots instead.

A replay is text (`tests/replays/<name>.replay.ron`): the inputs per tick plus checkpoint hashes. It replays bit-exactly on the same build and platform.

## Record
1. Open the scene: `mcp__concordia__scene_open {"path": "scenes/arena.scene.ron"}`.
2. Start recording: `mcp__concordia__replay_record {"action": "start", "path": "tests/replays/jump-over-gap.replay.ron"}`.
3. Drive the scenario: `mcp__concordia__input_inject` actions and `mcp__concordia__sim_step` ticks, or a scripted scenario passed to `replay_record`.
4. Stop: `mcp__concordia__replay_record {"action": "stop"}`.

## Run
- `verify`: same inputs, expected checkpoints; any hash mismatch is a divergence. Use it to prove nothing changed.
- `resimulate`: inputs only, on current code; produces new checkpoints. Use it to prove a fix.
- `ab`: two builds or two replays in lockstep; reports the first divergence.

`mcp__concordia__replay_run {"path": "...", "mode": "verify", "bisect": true}` reports the first divergent tick, entity, component, field and (when known) the system.

## Output contract
In the task comment: the replay path, the mode, the result (checkpoints matched, or the first divergence), and what it proves.
