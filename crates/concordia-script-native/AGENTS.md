# concordia-script-native: agent guide

## Purpose
The native tier of gameplay scripting (ADR-0003, ADR-0061): a `ScriptBackend` that loads module libraries speaking the C ABI from `concordia-script-abi`, and runs modules linked into the engine. Registration, batching, staging, faults and reload migration are `concordia-script`'s; this crate only loads libraries and carries calls.

## Public entry points
- `NativeBackend` (`new(copy_dir)`, `link::<M>()`, `load_library(path)`, `loaded_libraries`; `ScriptBackend::load` takes `ModuleSource::File` or `Linked`)
- `NativeModule` (`ScriptModule`; `copy_path`)
- `check_module_source(dir)`, `check_source(file, text)`, `Finding`, `SourceCheckError`

## Invariants
- **Copy-on-load:** every load copies the library to a unique name in the copy directory first (process id plus a counter), so a rebuilt library is never mistaken for the old one.
- **ABI first:** `concordia_module_abi_version` is read, and must equal `ABI_VERSION`, before `concordia_module_v1` is called or its table read. A missing symbol, a null table or a version mismatch is `ScriptError::Load`.
- **Old libraries stay loaded** until the backend is dropped (play stops); then their copies are deleted.
- A guest panic arrives as a `Panicked` reply and becomes `ScriptFault::Trap`; a malformed reply is `ScriptFault::Protocol`. Log lines travel inside the reply, so `calls_out` is always 0 and one run is one crossing.
- The static path (`Linked`) is `concordia_script::TestBackend`; both paths run the same SDK code and hash identically.
- All host-side `unsafe` is in `backend.rs`, each block with a SAFETY note.
- **The source check** rejects `static mut`, statics whose type mentions atomics, `Cell`/`RefCell`/`UnsafeCell`, `OnceCell`/`OnceLock`/`LazyLock`/`LazyCell`/`Lazy`, `Mutex`/`RwLock`/`Condvar`, and the `thread_local!`/`lazy_static!` macros, anywhere in the file (nested modules, function bodies), with file and line, in source order.

## How to test
`just native-fixtures`, then `cargo nextest run -p concordia-script-native`: the conformance suite on the dylib path (`tests/conformance.rs`) and the native promises (`tests/native.rs`: stable names, reload, layout migration, ABI refusal, panics, static-versus-dylib hashes, the source check).

## Common tasks
- **Build a module library:** give the module crate `[features] dylib = []` and `crate-type = ["rlib", "cdylib"]`; `export_module!(MyModule)` exports the ABI under that feature; `cargo build --features dylib`.
- **Reload during play:** `let module = backend.load_library(&path)?; host.reload(module, &mut registry, &mut world)?;` between ticks.

## Don'ts
- Don't unload libraries while play runs.
- Don't load a library without the copy (macOS would hand back the cached old image).
- Don't call a table whose version wasn't checked.
- Don't load untrusted code here; it runs with the engine's privileges (use WASM).

## References
- `libloading` documentation (ISC); the Rustonomicon on FFI.
- `syn` documentation (`visit`, span locations) for the source check.
- Bevy and Fyrox hot-reload write-ups (MIT/Apache-2.0), and Apple's `dlopen` documentation on image caching by path; concepts only.

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
