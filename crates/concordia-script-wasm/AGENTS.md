# concordia-script-wasm: agent guide

## Purpose
The WASM binding of the script protocol (ADR-0062): loads WebAssembly components that implement the WIT world `concordia:game/gameplay` (`wit/concordia-game`) and runs them on Wasmtime, deterministically and sandboxed. Everything engine-side (registration, batching, staging, faults) is `concordia-script`'s; this crate is only a `ScriptBackend`.

## Public entry points
- `WasmBackend` (`new(WasmConfig)`, `with_cache(Ddc)`, `cache_key`, `load_wasm`; `ScriptBackend::load` takes `ModuleSource::File` or `Bytes`)
- `WasmModule` (`ScriptModule`; `from_cache`)
- `WasmConfig` (`fuel`, `nan_canonicalization`, `memory_cap`, `max_instances`; `Default`, `unguarded`), `Fuel` (`per_run`, `per_row`, `budget`)
- `sim_nanos` (what the guest's clocks read), `WASMTIME_VERSION`

## Invariants
- **Ahead of time:** a component is compiled once with Cranelift; with a DDC it is cached under `DdcKey::of([namespace, wasm, WASMTIME_VERSION, blake3(engine.precompile_compatibility_hash())])`, so a new wasm, Wasmtime version or config never reuses a stale entry. An entry Wasmtime rejects is recompiled and replaced. Nothing is JIT-compiled or interpreted during play.
- **Deterministic engine:** NaN canonicalization (default on), no wasm threads (Wasmtime is built without its `threads` and `cache` features), `relaxed_simd_deterministic(true)`, fuel metering (no epoch interruption), Cranelift only.
- **Fuel per run:** each run starts with `per_run + per_row × (batch rows + delivered signals)`. Running out is `ScriptFault::OutOfFuel` with a fixed message (no backtrace), so the same inputs fault identically.
- **Fresh instance per run:** every `describe` and `run` instantiates from one `InstancePre` into a new `Store`, using the pooling allocator; guest statics never survive a run.
- **Memory cap:** the pooling allocator's `max_memory_size` is `memory_cap`; growing past it fails inside the guest.
- **Virtual WASI:** wall and monotonic clocks are frozen at the run's simulated time (`sim_nanos(time)`); secure random is `SimRng::new(seed).fork(1)`, insecure random `fork(2)`; no preopens, env, args or stdin; stdout and stderr are captured and appended to the run's log lines.
- Traps, fuel exhaustion and malformed outputs become `ScriptFault`s; the host process never panics on guest behavior.
- Imports offered: `concordia:game/host` (`log`, counted as an outbound crossing) and WASI p2. `describe` and `run` calls count as inbound crossings.
- The only `unsafe` is `Component::deserialize` of a DDC entry (see the SAFETY note: the DDC is trusted local build output, and the key and Wasmtime's header checks bind the blob to this engine).

## How to test
`just wasm` (builds `conformance_guest.wasm` and `conformance_guest_v2.wasm`), then `cargo nextest run -p concordia-script-wasm`: the conformance suite on WASM (`tests/conformance.rs`) and the WASM-only promises (`tests/wasm.rs`: fuel, NaN canonicalization, virtual WASI, statics, the AOT cache, reload, the WIT world, the guest build). Benchmarks: `just bench -p concordia-script-wasm` (record in `benches/BASELINE.md`).

## Common tasks
- **Change the WIT world:** edit `wit/concordia-game/world.wit`, then the guest glue (`concordia-script-guest/src/wasm.rs`) and `src/convert.rs`; bump the package version for breaking changes; `just wasm`.
- **Tune guards:** change `WasmConfig::default` only with new numbers in `benches/BASELINE.md`.
- **Upgrade Wasmtime:** bump the workspace pin and `WASMTIME_VERSION` together (the cache key changes, so old entries are ignored); keep the M-1 gate and this crate's tests green.

## Don'ts
- Don't enable epoch interruption, Wasmtime's `threads` feature or non-deterministic relaxed SIMD; don't enable Wasmtime's `cache` feature (MPL-2.0 dependency; the DDC caches instead).
- Don't hand guests real clocks, OS randomness, files, sockets or the environment.
- Don't keep instances or stores between runs.
- Don't deserialize anything that didn't come from this engine's DDC key.

## References
- Wasmtime documentation (docs.wasmtime.dev): `Config` determinism settings (`cranelift_nan_canonicalization`, `relaxed_simd_deterministic`), fuel, the pooling allocator, `InstancePre`, `Component::serialize`/`deserialize` and `Engine::precompile_compatibility_hash`.
- `wasmtime-wasi` documentation: `WasiCtxBuilder`, `HostWallClock`, `HostMonotonicClock`, `MemoryOutputPipe`.
- WebAssembly specification notes on NaN nondeterminism and the relaxed-SIMD deterministic profile.
- The M-1 scaffold gate (`tests/gates/m_minus1_scaffold.rs`) for the closed `WasiCtx` pattern (ADR-0021).

## Status
M2 phase 2.6b complete (2026-11-20).

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
