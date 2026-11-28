# ADR-0072: Replay v0: input log, checkpoints and build identity
- Status: Accepted (M2 phase 2.8, 2026-11-20)
- Date: 2026-11-20
- Builds on: ADR-0032 (edit vs play world: injected input and play outcomes belong to replay, never the command log), ADR-0046 (engine modules), ADR-0063 (determinism contract, presentation state not hashed), ADR-0061 and ADR-0062 (script modules and reload)

## Context
The engine is deterministic per binary and machine: the same inputs from the same edit world give the same play-world hash at any thread count. Agents and tests need that as evidence they can keep: "this is what happened, and it still happens". M2 needs a minimal form (an input log with checkpoints); M3 phase 3.1 generalizes it (snapshots, bisection to the field, `ab` runs).

## Decision
1. **A module, not core.** `concordia-replay` is an engine module enabled by `[replay]` in `concordia.toml`, like `[scripts]` and `[studio]`. It adds `replay.record` and `replay.run` (MCP `replay_record`, `replay_run`), so projects without it keep their tool set. `concordia new` writes `[replay]`, and `concordia studio init` adds it, because the default QA, level-designer and gameplay-programmer roles use it.
2. **What is recorded: action states after binding.** For each tick where some action's state changed, the replay stores those actions' full `ActionState` (pressed, just pressed, just released, value). A press and release within one tick survives, and rebinding keys doesn't break a replay. Raw pointer positions reach the simulation only through actions (e.g. `PointerPosition`), so they need no separate track.
3. **Playback belongs to the input crate.** `InputSampler::play_back(changes, until)` makes the sampler produce the recorded states for those ticks and drop device events queued for them. Replay never overwrites `ActionStates`, so nothing depends on module order. Unknown actions, or values of the wrong kind, are refused before the run.
4. **Checkpoints at tick ends.** Terra gains end-of-tick hooks (`Schedule::add_tick_end`, run by `SimLoop` after the clock advances, with `&World` only), so a checkpoint hash is exactly what `sim.step` reports. Recording reads `ActionStates` in the same hook. Checkpoints are taken at tick 0, every `checkpoint_every` ticks (default 60) and at the last tick.
5. **Hashes name what diverged.** `World::state_digest` computes the world hash and, in the same pass, one hash per part: each hashed component by name, plus `@entities`, `@clock`, `@rng` and `@signals`. The world hash is unchanged. Presentation-only components are already excluded (ADR-0063), so nothing more is needed to restrict the hash to simulation state.
6. **Build identity.** The header holds the engine version, the scene path and its edit-world hash, the seed, the tick rate, the recording's thread count (for information only) and module fingerprints. `EngineModule::fingerprints` lets a module say what code it runs; the scripts module reports each loaded module by the BLAKE3 hash of its file (WASM or native), or of its description for modules linked into the engine. Replay depends on no module crate for this.
7. **Mismatch and divergence.**
   - `replay.run` refuses a replay whose engine, scene, scene hash, seed or fingerprints differ, with the new error kind `ReplayMismatch` (−32019) listing them. `force: true` runs it anyway and returns the list.
   - A divergence is a result, not an error: verify mode stops at the first checkpoint that differs and reports its tick, the last matching tick and the differing parts. `resimulate` reports fresh hashes instead.
8. **Hot reload.**
   - `script.reload` of an identical build during a recording leaves every checkpoint unchanged.
   - If any fingerprint differs when recording stops (code was reloaded with a change), stop refuses with `ReplayMismatch`: v0 replays assume one build throughout.
9. **Format.** `.replay.ron` version 0, canonical RON through `concordia-scene`'s document writer; the reader checks the version, every hash and tick order.
10. **Project settings.** Replays depend on `[sim] seed`, which no engine built from a project applied before this phase. `Project::engine_config` now builds every project engine's settings (modules, `[sim]` rate and seed, `[journal]` sync) for the CLI, MCP server and editor.

## Alternatives
- **Core methods.** These would be always on, but every project would get two more tools, and the core crate can't see input (input depends on the API crate).
- **Recording raw device events (pre-binding).** That is simpler to inject back, but a key rebinding would silently change a replay. M3 may add raw events behind a flag.
- **Overwriting `ActionStates` from a replay system.** This is fragile, because it must run after `input.sample`, and exclusive systems order only by registration.
- **Hashing in a `Presentation`-phase system.** It wouldn't need a Terra change, but the clock isn't advanced yet there, so checkpoints wouldn't equal `sim.step`'s hash.
- **Recording code changes mid-session.** Recording a reload event and replaying it is possible, but v0 refuses instead. M3 can add it with snapshots.

## Consequences
- Per-part hashes cost a second hash of each component value, at checkpoints only.
- A replay is valid for one engine version, platform and build. Thread count doesn't matter.
- Divergence is located to a checkpoint window and the parts that differ. Finding the exact tick and field is M3 phase 3.1's bisection.
- Agents get reproducible evidence. QA records and verifies, and programmers resimulate to prove fixes.
- **Note (M2 phase 2.5b.2a, ADR-0078):** pointer rays (`ActionValue::Ray`) are recorded like any action value, so a click replays without the window size or camera. The format stays version 0.
- **Note (M2 phase 2.7b.2, ADR-0060):** the in-game UI reads only actions (`ui_pointer`, `ui_click`, `ui_scroll`, `ui_viewport`, the `ui_*` keys). The window's size is input too: `InputEvent::Viewport` feeds the new `Binding::Viewport`, so a resize is recorded like any action value. The format stays version 0.
- **Note (M2 phase 2.7b.3, ADR-0060):** typed text can't be an action value, so each input change gains an optional `text` list (inserts and editing commands, in order) and ticks with only typing are recorded too. A tick without typing writes no `text` field, so earlier files read unchanged and the format stays version 0; playback feeds the text back through `InputSampler::play_back_text`.
- **Note (M3 phase 3.1a): format version 1.** The writer writes version 1, and the reader takes 0 and 1 (`concordia replay migrate` rewrites a version 0 file).
  - **Header:** gains `platform` (`<OS>-<arch>`), `manifest_hash` (BLAKE3 of `concordia.toml`) and `task`.
  - **Content in the fingerprints:** `modules` also fingerprints content: the input map (`input/map`, through `InputModule::fingerprints`) and the template catalog (`engine/templates`).
  - **Per-tick hashes:** a `hashes` block holds every tick's world hash, the first 16 hex digits each, 64 ticks per line. `verify` steps tick by tick, reports the exact first divergent `tick`, and compares parts at the next checkpoint (`parts_tick`). Bisection is no longer needed.
  - **Mismatches:** `platform`, `manifest_hash` and `rate_hz` join the compared fields. Fields a version 0 file never recorded are not compared.
  - **Snapshots (R2, the author's decision):** they are stills for field diffs (3.1b) and are never restored. Raw device events stay out (`later-backlog`).
- **Note (M3 phase 3.1b): stills.**
  - **What they are:** at each checkpoint the recorder keeps the hashed component values as JSON in `.concordia/ddc` (BLAKE3-keyed). Their key is on the checkpoint (`still`), and `"stills": false` leaves them out.
  - **On a divergence:** `verify` diffs the recorded still against the replayed world at `parts_tick`, listing up to 32 `fields`. A still missing from the cache leaves the list empty, with a `fields_note` and a warning.
  - **Re-blessing:** `resimulate` with `update` rewrites the file on current code.
  - **Offline:** `compare_replays` and `concordia replay diff` compare two files.
  - **Stills are never restored** (R2).
- **Note (M3 phase 3.1c): `ab` runs.**
  - **What it does:** `replay.run` `mode: "ab"` with `other` (another project directory) replays the file here and on an engine for `other` in lockstep. The engine comes from an `EngineFactory` the app injects into `ReplayModule`, because the replay crate can't build projects.
  - **On a divergence:** at the first tick whose hashes differ, both rerun to just before it, then run that tick observed: `Schedule::run_observed`, `SimLoop::step_observed` and `Engine::step_play_observed` call back after each wave and each phase flush. The first label whose hashes differ is `suspect_system`, and the fields are diffed between the two live worlds at that exact tick.
  - **What stays the same:** normal ticks are unchanged; observation exists only for diagnostics.
- **Note (M4's end, 2026-11-27, the author's decision): native modules are fingerprinted by source.** A native library's bytes differ between builds of the same source (paths, the build folder, the toolchain's layout), so the committed Snuffer replay stopped matching after a rebuild that changed no code. A native module's fingerprint is now BLAKE3 of its description's JSON and, when `[[scripts.module]] source` names its folder, every `.rs` file under it (relative path and contents, in path order); without a source folder, of the description alone, like a linked module. WASM modules keep the hash of their file. Dependencies outside the folder are not hashed: the per-tick world hashes still catch any change in behaviour. Lamplight's fog declares its source.
