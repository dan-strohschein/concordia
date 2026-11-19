---
name: concordia-fix-bug-with-replay
description: "Fixes a gameplay bug test-first: reproduce it in a replay, capture it as a failing assertion, fix the code, then resimulate to prove the fix and hand both to QA."
when_to_use: "Use when a task reports a gameplay bug, a regression or a divergence between replays."
user-invocable: false
---
# Fixing a bug with a replay

> **Not available yet:** needs `replay_record`, `replay_run` (M3 phase 3.1) and `assert_eval` (M3 phase 3.2). Until then, reproduce with `sim_step` and `query_run`, and write a failing Rust test before fixing.

Never fix what you haven't reproduced. Never weaken a test or an assertion to make it pass.

## Steps
1. **Reproduce.** Record the bug: `mcp__concordia__replay_record` the scenario from the task (see `concordia-record-replay`). Confirm with `mcp__concordia__replay_run {"mode": "verify"}` that it replays.
2. **Capture it as a failing check.** Write a playtest assertion that fails because of the bug (`concordia-write-playtest`), and evaluate it: `mcp__concordia__assert_eval` must fail for the reason the task describes.
3. **Find the cause.** Use `bisect` on a divergence, `mcp__concordia__log_tail`, and the code. State the root cause in one sentence before changing anything.
4. **Fix** the smallest thing that removes the cause, in your write paths. Run the project's tests (`cargo nextest run`).
5. **Prove it.** `mcp__concordia__replay_run {"mode": "resimulate"}` on the recorded replay, then the assertion again: it must pass now.
6. Hand to QA with the replay and the assertion.

## Output contract
```
Root cause: <one sentence>
Reproduced: <replay path>, failing assertion <name> (observed <value>)
Fix: <files>
Proof: resimulate ok; <assertion> passes (observed <value>); tests green
```
