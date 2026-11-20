---
name: concordia-fix-bug-with-replay
description: "Fixes a gameplay bug test-first: reproduce it in a replay, capture it as a failing check, fix the code, then resimulate to prove the fix and hand both to QA."
when_to_use: "Use when a task reports a gameplay bug, a regression or a divergence between replays."
user-invocable: false
---
# Fixing a bug with a replay

> **Not available yet:** `mcp__concordia__assert_eval` (M3 phase 3.2). Until then, capture the bug as a failing Rust test (step 2).

Never fix what you haven't reproduced. Never weaken a test or an assertion to make it pass.

## Steps
1. **Reproduce.** Record the bug: `mcp__concordia__replay_record {"action": "start", "path": "tests/replays/<bug>.replay.ron"}`, drive the scenario from the task, then `mcp__concordia__replay_record {"action": "stop"}` (see `concordia-record-replay`). Confirm it replays: `mcp__concordia__replay_run {"path": "tests/replays/<bug>.replay.ron"}`.
2. **Capture it as a failing check.** Write a test that fails because of the bug: a Rust test in your paths now, and with M3 phase 3.2 a playtest assertion (`concordia-write-playtest`) evaluated with `mcp__concordia__assert_eval`. It must fail for the reason the task describes.
3. **Find the cause.** Use the divergence report's tick and parts, `mcp__concordia__log_tail`, and the code. State the root cause in one sentence before changing anything.
4. **Fix** the smallest thing that removes the cause, in your write paths. Run the project's tests (`cargo nextest run`).
5. **Prove it.** `mcp__concordia__replay_run {"path": "tests/replays/<bug>.replay.ron", "mode": "resimulate"}` on the recorded replay, then the failing check again: it must pass now.
6. Hand to QA with the replay and the failing check.

## Output contract
```
Root cause: <one sentence>
Reproduced: <replay path>, failing test <name> (observed <value>)
Fix: <files>
Proof: resimulate ok; <test> passes (observed <value>); tests green
```
