# concordia-script: agent guide

## Purpose
The engine side of gameplay scripting (ADR-0062), written once against a backend trait so the host is swappable. A backend (WASM in `concordia-script-wasm`, native in 2.6c, the in-process `TestBackend` here) only loads modules and carries protocol calls; registration, scheduling, batching, staging, fault containment, logging and seeding live here.

## Public entry points
- `ScriptBackend` (`name`, `load(&ModuleSource)`), `ScriptModule` (`describe`, `run(&self, ..) -> RunReport`, `stats`), `ModuleSource` (`File`, `Bytes`, `Linked`), `ModuleStats`
- `ScriptHost` (`load`, `processors(&registry, &templates)`, `set_templates`, `enable_all`, `emit_logs`, `faults`, `logs`, `is_disabled`, `stats`, `reload(module, &mut registry, &mut world)` → `ReloadReport` of `TypeMigration`s), `HostConfig` (`session_seed`), `FaultReport`, `LogLine`, `Templates`
- `ScriptProcessor` (a Terra `Processor`), `register_module`, `changed_types`, `processor_seed`
- `TestBackend` (`link::<M: Module>()`)
- `ScriptError`, `ScriptFault` (`Trap`, `OutOfFuel`, `Returned`, `Protocol`)
- Feature `conformance`: `conformance_suite!(loader)`, `conformance::{Rig, run_all, PROCESSORS, SESSION_SEED, mover_template}`
- Protocol types re-exported from `concordia-script-protocol` (`protocol`, `Batch`, `RunOutput`, ...)

## Invariants
- **One crossing per processor per tick:** `ScriptProcessor::run` builds one batch (reads then writes, every match; received signals by entity then raise order) and makes one `ScriptModule::run` call, whatever the entity count.
- **Nothing applies from a bad run:** column writes (declared, once each, `rows × stride`, valid wire) and commands (known types, valid values, known templates, earlier pending targets, none in `Presentation`) are decoded and checked in full before any is written or queued. A fault (trap, out of fuel, returned error, protocol violation) applies nothing, disables that processor until a reload, is recorded as a `FaultReport` and logged (`tracing::warn!`) as `script:<module>`; the rest of the schedule keeps running.
- Commands go to the processor's `CommandBuffer`, so they apply at phase end in plan order like every Terra command; `spawn-template` records a `CompiledTemplate` spawn into it.
- Log lines (the `log` import, and on WASM stdout/stderr) are kept even for a faulting run and attributed to `script:<module>`. Processors only record lines and faults (they run on worker threads); `emit_logs`, called on the engine thread, sends the new ones to `tracing` ordered by tick, then processor declaration order, so the log is the same at any thread count (2.6d).
- Each run gets `TickContext { tick, dt, time: SimClock::time, seed: processor_seed(session, tick, name) }`; processors never touch `SimRng`. Script processors declare `reads_resource::<SimClock>()`.
- Modules keep no state between runs (`run(&self)`); WASM enforces it with fresh instances. The test backend can't (statics survive natively; 2.6c's source check covers native modules).
- `register_module` validates the description, then registers helper types, components and signals as dynamic types (`register_dynamic_component`) and traits as parameterless dynamic traits (`concordia_terra_sim::register_dynamic_trait`).
- A reload may change behavior and types, not the module's name or processors (names, phases, access, order), and may not remove types (`ReloadChangesProcessors`; schedule rebuilds are 2.6d). Only changed component and signal types are re-registered (all of them when a helper type changed); the world's values, tag values and pending signals move to the new versions through reflect `migrate`, all at once (`World::migrate_dynamic_all`). If any value can't convert, the old versions are registered again and the world is untouched (`ScriptError::Migration`). Traits are re-registered; processors take the new versions from the next run; templates holding changed types must be recompiled and passed to `set_templates`. A reload re-enables disabled processors (ADR-0061).

## How to test
`cargo nextest run -p concordia-script --features conformance` (seed unit tests, the conformance suite on the test backend, `test_backend_passes_conformance`, and `tests/reload.rs`: migration, rollback and refusals with in-test modules). Every backend crate runs the same suite: `concordia_script::conformance_suite!(load)` in its tests, with a loader for the conformance module (`tests/fixtures/script/conformance-guest`).

## Common tasks
- **Add a conformance behavior:** add a processor to the conformance module (`tests/fixtures/script/conformance-guest`), a `pub fn <behavior>(load: Loader)` in `src/conformance.rs`, and its name to `conformance_suite!` and `run_all`; rebuild fixtures with `just wasm`. Every backend picks it up.
- **Add a backend:** implement `ScriptBackend` and `ScriptModule` in a new crate; run `conformance_suite!` in its tests.
- **Run a module in a world:** `let host = ScriptHost::load(backend.load(&source)?, &mut registry, HostConfig { session_seed })?;` then add `host.processors(&registry, &templates)?` to the `Schedule`.

## Don'ts
- Don't apply any part of a run's output before all of it is checked.
- Don't give script processors `SimRng` or wall-clock time; time and randomness ride in the `TickContext`.
- Don't put backend-specific logic here (Wasmtime, `libloading`); keep this crate binding-neutral.

## References
- Unreal Engine's public Mass documentation on processors, processing phases, deferred commands and signals, concepts only (clean room, ADR-0067); no Unreal source.
- WebAssembly Component Model documentation (one call per batch to keep boundary crossings flat; ADR-0003).
- Glenn Fiedler, "Floating Point Determinism" (Gaffer on Games), for per-processor seeds and simulated time only.
- blake3 specification (seed derivation).

## Status
M2 phase 2.6b complete (2026-11-20); 2.6c added reload migration (ADR-0061); 2.6d added `emit_logs`, `enable_all` and `changed_types`.

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
