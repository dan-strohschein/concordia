# concordia-replay: agent guide

## Purpose
Records play sessions as input changes, checkpoint hashes and every tick's hash (`.replay.ron` v1; v0 is read and migrated) and replays them to verify the same world states, naming the exact first divergent tick (ADR-0072). Checkpoint stills (component values as JSON in `.concordia/ddc`) name the differing fields; `update` re-blesses a file; `compare_replays` diffs two files. `ab` (3.1c) replays on another project's engine from an injected `EngineFactory` in lockstep and names the suspect system from an observed tick.

## Public entry points
- `ReplayModule` (EngineModule, `[replay]`; `with_factory(EngineFactory)` for `ab`): `replay.record` (`ReplayRecord`, `RecordAction::start|stop`) and `replay.run` (`ReplayRun`, `RunMode::verify|resimulate`, `force`)
- `Replay` (`tick_hash`), `ReplayHeader`, `InputChange`, `Checkpoint`, `FORMAT_VERSION` (1), `VERSION_0`, `TICK_HASH_HEX`, `TICKS_PER_LINE`, `read_replay` (versions 0 and 1), `write_replay` (version 1), `ReplayError`
- Results: `ReplayRecordResult`, `ReplayRunResult`, `Divergence` (with `fields`, `FieldDiff`), `Mismatch`, `TickHash`
- `assert.eval` (`AssertEval`, `AssertEvalParams { assertions, source: live|replay|scenario, path }`, `AssertEvalResult`, `AssertOutcome`, `AssertStatus`) and the `.playtest.ron` format (`Playtest`)
- `compare_replays(a, b, ddc) -> ReplayDiff` (offline: header, first input and hash tick, parts, fields)
- Layout: `methods.rs` (API types and dispatch), `recorder.rs` (start/stop), `runner.rs` (prepare/run), `record.rs` (the recording and its tick-end hook, also used to re-bless), `verify.rs` (header and checkpoint comparisons), `still.rs` (stills: take, store, load, diff), `diff.rs` (two files), `ab.rs` (two builds), `assert_method.rs` (`assert.eval`, playtests), `eval.rs` (a predicate on one tick's world), `monitor.rs` (temporal operators, early exit, closest approach), `format.rs`

## Invariants
- Never touches the edit world or the command journal (ADR-0032); recording and running each start a fresh play world from the open scene, and log `replay recorded` / `replay run` with the actor.
- Inputs are recorded after binding: per tick, the actions whose `ActionState` changed. Playback goes through `InputSampler::play_back`; this crate never writes `ActionStates`.
- Recording happens in an end-of-tick hook (after the clock advances), so every checkpoint hash equals what `sim.step` reports at that tick. Checkpoints: tick 0, every `checkpoint_every`, and the last tick. Every tick's world hash is kept too (16 hex digits, 64 per line); `verify` steps tick by tick with them and compares parts at the next checkpoint.
- A run refuses a different engine version, platform, manifest hash, scene, scene hash, seed, tick rate or fingerprints (module code, the input map, the template catalog; `verify::fingerprints`) (`ReplayMismatch`, −32019) unless forced; unknown recorded actions are a mismatch too. A divergence is a result (`ok: false`), not an error.
- Stopping refuses (and writes nothing) if any module fingerprint changed during the recording.
- Files are canonical RON (`concordia-scene` document writer); reading checks the version, every hash, the per-tick hash lines and tick order. A version 0 file reads as version 1 without the fields it never recorded, and those are not compared.

## How to test
`cargo nextest run -p concordia-replay` (needs `just wasm` for the script tests); the M1 physics replay is `cargo nextest run -p concordia-tests --test integration replay_m1_physics::`.

## Common tasks
- **Add a header field:** add it to `ReplayHeader` (doc comment), fill it in `verify::current_header`, compare it in `verify::mismatches` if a difference must refuse a run, and accept the format snapshot after review.
- **Report another kind of code identity:** implement `EngineModule::fingerprints` in that module; no change here.

## Don'ts
- Don't add replay systems to the schedule; use end-of-tick hooks (read-only) and the sampler's playback.
- Don't compare headers field by field outside `verify.rs`.
- Don't journal anything or write to the edit world.
- Don't restore stills into a world: they are for diffs only (decision R2); restoring is later-backlog.

## References
- Deterministic lockstep and replay practice: Glenn Fiedler, "Deterministic Lockstep" and "Fix Your Timestep!" (gafferongames.com); GGPO's public documentation on input-only replays.
- BLAKE3 (the `blake3` crate, Apache-2.0/CC0) for content and state hashes.

## Status
M2 phase 2.8 (v0). M3 phase 3.1c: `ab` mode, `suspect_system`, `EngineFactory`. M3 phase 3.1b: stills and field diffs, `update`, `compare_replays`, and the committed `tests/replays/m1-physics.replay.ron`. M3 phase 3.1a: format version 1 (platform, manifest hash, task, per-tick hashes, content fingerprints; exact first divergent tick; `concordia replay migrate`). M2 phase 2.7b.3: input changes carry the tick's typed text (`InputChange::text`, omitted when empty, so files without typing are unchanged and the format stays version 0); ticks with only typing are recorded; runs play it back (`play_back_text`). M3 phase 3.2b: `assert.eval` over a replay, a `.playtest.ron` scenario or the current tick (ADR-0084), with `concordia-assert` parsing and checking.

## Engineering rules
These apply to every change, by humans and agents alike.
1. **TDD on every phase.** Write the tests first, see them fail, implement, then refactor.
2. **SOLID.** Depend on abstractions at crate boundaries; extend through traits rather than by editing callers.
3. **No duplicate functionality.** Before writing any implementation, search for existing functionality (grep, `cargo doc`, crate `AGENTS.md` files). Extend or refactor what exists; never write a parallel version.
4. **Single responsibility at every level:** function, type, module and crate.
5. **Small functions** that do one thing.
6. **Validate parameters.** Every function checks its inputs:
   - Rust: encode invariants in types (newtypes, enums, `NonZero*`); validate at boundaries and return `Result` with a descriptive error; use `debug_assert!` for internal invariants. Public APIs never panic on bad input.
   - TypeScript: strict types, plus runtime validation at API and IPC boundaries.
7. **Short comments.** One-line doc comments on public items; no essays, and no comments that restate the code.
8. **Idiomatic code.**
   - Rust: follow the Rust API Guidelines and keep it clippy-clean; `thiserror` in libraries, `anyhow` only in binaries and tests.
   - Editor UI: idiomatic React and TypeScript, with function components, hooks, strict mode, ESLint and Prettier.
