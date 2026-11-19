---
name: concordia-write-playtest
description: "Writes a .playtest.ron assertion suite that checks a scene or mechanic over simulated time, and evaluates it with assert_eval so acceptance criteria are proven by the engine, not by eye."
when_to_use: "Use when a task's acceptance criteria describe behavior over time (falls, reaches, collides, stays within) or a fix needs a regression check."
user-invocable: false
---
# Writing a playtest

> **Not available yet:** needs `assert_eval` (M3 phase 3.2) and, for scripted input, `replay_run` (M3 phase 3.1). Until then, check criteria with `sim_step` plus `query_run` and record the numbers in your task comment.

A playtest is text (`tests/playtests/<name>.playtest.ron`): a scene, optional inputs or a replay, and assertions evaluated at ticks.

## Steps
1. Turn each acceptance criterion into one assertion with a tick budget: "the crate rests on the floor" becomes "by tick 300, `Crate.Transform.translation.y` < 1.0 and its speed < 0.01".
2. Name entities by `Name`, never by id; ids differ between scenes.
3. Write the file under `tests/playtests/` (shared by the level designer and QA).
4. Evaluate it: `mcp__concordia__assert_eval {"path": "tests/playtests/crate-rests.playtest.ron"}`. Each assertion reports pass or fail, the tick and the observed value.
5. A failing assertion is information: fix the scene or the code, never the threshold, unless the task's criterion itself was wrong (then say so in a comment).

## Output contract
In the task comment: the playtest path and each assertion's result with its observed value.
