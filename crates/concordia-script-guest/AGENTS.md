# concordia-script-guest: agent guide

## Purpose
The SDK gameplay modules are written with (ADR-0062). One crate for every backend: it builds natively (the in-process `TestBackend` and 2.6c's native tier link it) and for `wasm32-wasip2` with plain `cargo build` plus `wit-bindgen` (ADR-0021), where `export_module!` emits the component's exports.

## Public entry points
- `Module` (`describe() -> ModuleDesc`, `run(processor, &mut RunCtx) -> Result<(), String>`), `export_module!(Type)` (WASM exports on `wasm32`; the native C ABI when the module crate's `dylib` feature is on, ADR-0061)
- `RunCtx`: `tick`, `dt`, `time`, `rng` (the engine-seeded `SimRng`), `ids`, `read::<T>()`, `write(&[T])`, `signals::<T>()`, `spawn`, `spawn_template`, `insert`, `remove::<T>`, `despawn`, `signal`, `log`
- `describe_type::<T>()`, `processor(name, phase)` → `ProcessorBuilder` (`reads`, `writes`, `with`, `without`, `receives`, `reads_named`, `writes_named`, `after`, `before`, `build`)
- `value(&T)`, `run_module::<M>` (what backends call), `math` (libm), re-exports `EntityId`, `SimRng`, `Phase`, `Target`, `TraitDesc`, `ModuleDesc`, `protocol`

## Invariants
- Component, signal and helper types are `#[derive(Reflect)]` Rust types with a stable `#[reflect(name = "<module>.<Type>")]`; `describe_type` sends their `TypeDesc`, and the engine registers them as dynamic types that behave exactly like the Rust type (same wire bytes, JSON and hash).
- `read` and `write` move whole columns; `write` must cover every batch row. Writing a column twice in one run keeps the last.
- Spawns (`spawn`, `spawn_template`) return `Target::Pending(n)` in recording order; commands apply at the end of the phase.
- Modules keep no state between runs: on WASM every run is a fresh instance, so statics reset. Natively they would survive; don't rely on them.
- Use `math` (libm) for transcendental functions so native and WASM builds agree bit for bit; the WASM host canonicalizes NaNs.
- Log through `ctx.log`; on WASM, stdout and stderr also become log lines. Time and randomness come only from `RunCtx` (on WASM, WASI clocks read simulated time and WASI random is the processor's stream 1).

## Native libraries (ADR-0061)
- `export_module!` emits the `concordia_module_v1` exports (through `concordia-script-abi`) only when the calling crate enables its own `dylib` feature, so plain `rlib` builds export no C symbols and many modules can be linked statically into one binary.
- The native backend copies and loads the library; `check_module_source` (in `concordia-script-native`) rejects statics that keep state.

## How to test
`cargo nextest run -p concordia-script-guest` (run-context unit tests). The conformance module (`tests/fixtures/script/conformance-guest`) is written with this SDK and checked on every backend; `just wasm` builds it for `wasm32-wasip2`, and `just lint` runs clippy for that target.

## Common tasks
- **Write a module:** a `cdylib` + `rlib` crate depending on this SDK, `concordia-reflect` and `serde`; implement `Module`, call `export_module!(MyModule)`; build with `cargo build --target wasm32-wasip2 --release`.
- **Process entities:** `let vel: Vec<Vel> = ctx.read()?; let mut pos: Vec<Pos> = ctx.read()?; ...; ctx.write(&pos)` with `processor("m.move", Phase::PostPhysics).reads::<Vel>().writes::<Pos>()`.

## Don'ts
- Don't keep state in statics or thread-locals; store it in components.
- Don't use `f32::sin` and friends where results feed the simulation; use `math`.
- Don't call WASI APIs for files, sockets or the environment: the host provides none.

## References
- wit-bindgen README and the WebAssembly Component Model documentation (guest bindings, `pub_export_macro`).
- libm (Rust port of musl's libm, MIT/Apache-2.0) for portable float math.
- Bytecode Alliance `wasip2` crate documentation (clocks and random APIs used by the conformance module).

## Status
M2 phase 2.6b complete (2026-11-20); 2.6c added the native export (ADR-0061).

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
