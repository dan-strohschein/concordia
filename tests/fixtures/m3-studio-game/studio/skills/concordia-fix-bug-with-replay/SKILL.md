---
name: concordia-fix-bug-with-replay
description: "Fixes a gameplay bug test-first: reproduce it in a replay, capture it as a failing check, fix the code, then resimulate to prove the fix and hand both to QA."
when_to_use: "Use when a task reports a gameplay bug, a regression or a divergence between replays."
user-invocable: false
---
# Fixing a bug with a replay

Never fix what you haven't reproduced. Never weaken a test or an assertion to make it pass.

## Steps
1. **Reproduce.** Record the bug: `mcp__concordia__replay_record {"action": "start", "path": "tests/replays/<bug>.replay.ron"}`, drive the scenario from the task with `mcp__concordia__input_inject` (keys and pointer events from the project's input map, at ticks) and `mcp__concordia__sim_step`, then `mcp__concordia__replay_record {"action": "stop"}` (see `concordia-record-replay`). Confirm it replays: `mcp__concordia__replay_run {"path": "tests/replays/<bug>.replay.ron"}`.
2. **Capture it as a failing check.** State the expected behaviour as an assertion and check it over the recorded replay: `mcp__concordia__assert_eval {"assertions": ["within 600 ticks: distance(tag(\"player\"), tag(\"door\")) < 2m"], "source": "replay", "path": "tests/replays/<bug>.replay.ron"}`. It must fail for the reason the task describes; its `witness` shows the observed value. Add a Rust test in your paths when the rule lives in code.
3. **Find the cause.** Use the divergence report's tick and parts, `mcp__concordia__log_tail`, and the code. State the root cause in one sentence before changing anything.
4. **Fix** the smallest thing that removes the cause, in your write paths. Run the project's tests (`cargo nextest run`).
5. **Prove it.** The replay names the old build, so re-bless it on yours with the same inputs: `mcp__concordia__replay_run {"path": "tests/replays/<bug>.replay.ron", "mode": "resimulate", "update": true, "force": true}` (copy the file first if you want to keep the bug's recording). Then run the same `mcp__concordia__assert_eval` again: it must pass now. To see exactly where your build and the old one part, `replay_run` with `"mode": "ab"` and the old copy's directory as `other` names the first differing tick and the system.
6. Hand to QA with the replay and the failing check.

## Output contract
```
Root cause: <one sentence>
Reproduced: <replay path>, failing assertion <text> (observed <witness>)
Fix: <files>
Proof: resimulate ok; the assertion passes at tick <n> (observed <witness>); tests green
```
