# ADR-0086: Replay format version 1 (per-tick hashes, stills, A/B)
- Status: Accepted (M3 phases 3.1a–c, 2026-11-23; written at M3's close, 2026-11-24; decision R2 by the author, the rest my recommendations under the author's standing approval for M3)
- Date: 2026-11-24
- Builds on: ADR-0072 (replay v0: input log, checkpoints, build identity), ADR-0063 (determinism)

## Context
Replay v0 could say *that* a run diverged, and the checkpoint where it did, but not *when* or *why*: a checkpoint every 60 ticks is a minute of guesswork at the tick level, and the hashes name only the parts (component types) that differ. Agents fixing bugs need the exact tick, the fields, and the system to look at. They also need to re-bless a replay after an intended change, and to compare two builds.

## Decision
1. **Format version 1** (`FORMAT_VERSION = 1`). The reader takes v0 and v1, and the writer writes v1. `concordia replay migrate` rewrites a v0 file, keeping the same final hash. The header gains `platform`, `manifest_hash` and `task`. Build identity reuses each module's own fingerprints, extended to the input map and the template catalog.
2. **A hash every tick:** 16 hex digits (64 bits) per tick, 64 ticks per line, next to the full part hashes at each checkpoint. A divergence is found at its exact tick.
3. **Stills, never restored (decision R2).** At each checkpoint, a text snapshot of every component goes into the DDC (`.concordia/ddc`, keyed by BLAKE3), and the file lists its key. Stills only feed field-level diffs (entity, component, path, expected, actual). Every run still starts at tick 0. A missing still means a report without values and a note saying why. Mid-stream restore belongs to the later record/replay test system ("a Playwright for games").
4. **Re-blessing:** `replay.run` in `resimulate` mode with `update: true` rewrites the file on the current build with the same inputs. It is never allowed in verify mode, and never runs in CI.
5. **A/B:** `mode: "ab"` with `other: <project dir>` runs two engines in lockstep and reports the first differing tick, its fields and the suspect system. The second engine comes from an `EngineFactory` the app injects. The suspect system comes from Terra's `Schedule::run_observed`: an observer after each system on one chosen tick, for diagnostics only.
6. **Offline comparison:** `compare_replays` / `concordia replay diff` compare two files: the headers, the first differing input and tick, and the parts and fields.

## Consequences
- Files grow by 16 hex digits a tick: about 58 KB for an hour at 60 Hz, before compression.
- A replay names its platform, so committed replays are verified on that platform only; other platforms resimulate and check outcomes. The gates record and verify fresh replays on every platform.
- After a code change, a replay recorded on the old build is refused (`ReplayMismatch`) until it is re-blessed. The fix-bug skill re-blesses before asserting the fix (found by the M3 gate).

## Alternatives considered
- **Restoring mid-stream from snapshots:** worlds can't yet serialize resources, scripts or physics state. Deferred to the record/replay test system.
- **Full hashes every tick:** too large. 64 bits locate the tick, and checkpoints carry the full parts.
