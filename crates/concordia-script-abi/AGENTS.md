# concordia-script-abi: agent guide

## Purpose
The one definition of the native module ABI (ADR-0061), and the guest side of it. The guest SDK's `export_module!` uses `export_entry!` here; the host (`concordia-script-native`) reads the same table. All guest-side FFI `unsafe` lives in this crate; host-side `unsafe` lives in `concordia-script-native`.

## Public entry points
- `ABI_VERSION`, `VERSION_SYMBOL` (`concordia_module_abi_version`), `TABLE_SYMBOL` (`concordia_module_v1`)
- `ModuleV1` (`abi_version`, `describe`, `run`, `free`), `Buffer` (`EMPTY`, `from_vec`), `DescribeFn`, `RunFn`, `FreeFn`, `VersionFn`, `TableFn`, `STATUS_OK`, `STATUS_ERROR`
- `Entry` (what a module provides: `describe`, `run(&Call, log)`), `guest::table::<E>()`, `export_entry!(Type)`

## Invariants
- The version symbol is read before the table: a host must refuse a module whose version differs from `ABI_VERSION` without touching its table.
- `describe` returns JSON of `ModuleDesc`; `run` takes one `codec` call and returns one `codec` reply. Data crosses only as byte buffers.
- **Nothing unwinds across the ABI:** panics in `describe` become `STATUS_ERROR` with the message; panics in `run` become a `Panicked` reply that keeps the log lines.
- A malformed call is a `BadCall` reply, never a crash; a null pointer with length 0 is the empty call.
- Buffers are freed by the module that allocated them (`free`), exactly once.
- Changing `ModuleV1`'s layout or meaning needs a new `ABI_VERSION` and table symbol.

## How to test
`cargo nextest run -p concordia-script-abi`: the glue called exactly as a host calls it (through the exported symbols and table, freeing buffers with the module's `free`). End to end on real libraries: `concordia-script-native`'s tests (after `just native-fixtures`).

## Common tasks
- **Add a table function:** that's a new ABI: add `ModuleV2` and `concordia_module_v2`, keep v1 loadable or refuse it explicitly, bump `ABI_VERSION`, and update the native loader.

## Don'ts
- Don't let a panic or a Rust type cross the boundary; only bytes and `#[repr(C)]` types.
- Don't free a module's buffer with the host's allocator.

## References
- The Rustonomicon on FFI and unwinding; the Rust Reference on `extern "C"` functions and `#[unsafe(no_mangle)]`.
- `libloading` documentation (the host side).
- Bevy and Fyrox hot-reload write-ups (MIT/Apache-2.0), read for the problem space only; the design is our own.

## Status
M2 phase 2.6c complete (2026-11-20).

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
