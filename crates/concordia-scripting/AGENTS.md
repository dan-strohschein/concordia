# concordia-scripting: agent guide

## Purpose
The engine module that connects gameplay scripts (`concordia-script` and its WASM and native backends) to the Engine API, the edit world and play sessions. It owns `[scripts]` loading, the play schedule's script processors, and `script.reload`.

## Public entry points
- `ScriptsModule` (`new(ScriptsConfig)`, `link::<M>()` for statically linked modules); an `EngineModule` named `scripts`
- `ScriptsConfig` (`modules`, `cache`, `copies`, `session_seed`; `for_project`), `ScriptEntry` (`kind`, `path`, `source`; `file`), `ScriptKind` (`Wasm`, `Native`)
- `script.reload`: `ScriptReload`, `ScriptReloadParams` (`module`, `actor`), `ScriptReloadResult` (`ReloadedModule`: name, backend, version, `MigratedType`s, `edit_txn`)

## Invariants
- **Fingerprints (2.8, ADR-0072):** `fingerprints()` reports each loaded module as `scripts/<name>` with its backend and the BLAKE3 hash of the build it loaded: the file's bytes (WASM, native) or the JSON of a linked module's description. A reload updates the hash; replays use it to refuse a recording made with other code.
- **Load at engine start:** every configured module (files, then linked modules) is loaded in `register_types`, so script components work in scenes and the API before play. A module that fails to load (missing file, bad ABI, hidden state in a native module's `source`) is logged at error level and skipped; the engine still starts, and `script.reload` reports it (`ScriptFailed`) or loads it once fixed.
- The app adds this module last, so script types may refer to every other module's types.
- **Play:** each session gets every module's processors (after `enable_all`, so faulted processors get a fresh start) and one exclusive `Presentation` system, `scripts.emit_logs`, that sends script log lines and faults to the log on the engine thread in tick, then processor declaration order (the same at any thread count).
- **`script.reload` never builds:** it loads what is on disk. Per module:
  1. the changed component and signal types the edit world has stored are migrated as one transaction by `script:<module>` (`EditOp::MigrateType`, undoable to the exact previous version and values);
  2. the registry and the play world move to the new build (`ScriptHost::reload`);
  3. if step 2 fails, step 1 is undone (`Engine::undo_as`), so nothing changes.
  It is logged (`script reloaded`, with the caller's `actor`, `module`, `version`, `migrated`). A reload that changes processors is refused until a later phase rebuilds schedules.
- WASM modules are precompiled into the project's DDC (`.concordia/ddc`); native libraries are copied to `.concordia/native` before loading.
- Session seeds come from `[sim] seed`.

## How to test
`just wasm` and `just native-fixtures`, then `cargo nextest run -p concordia-scripting`: loading, play, faults across sessions, failed loads, the hidden-state check, reload logging, edit-world migration with undo/redo, the native edit-and-reload done criterion, and the `wanderers` sample on every backend (identical hashes at 1 and 8 threads). The app side: `concordia-app`'s `scripts_section_*` and `m2_scripting_example_*` tests; MCP: `binary_serves_script_reload_when_scripts_are_configured`.

## Common tasks
- **Add a module to a project:** build it, then add `[[scripts.module]]` with `kind` and `path` (and `source` for a native module's hidden-state check) to `concordia.toml`.
- **Link a module into a game binary:** `ScriptsModule::new(config).link::<MyModule>()`.

## Don'ts
- Don't emit tracing from script processors (worker threads); record and let `emit_logs` send it.
- Don't migrate the edit world outside the command log.
- Don't build modules from the engine; the developer or agent builds, `script.reload` loads.

## References
- ADR-0061 (native tier, migration), ADR-0062 (protocol, WASM), ADR-0032 (edit world vs play world), ADR-0046 (engine modules).
- Unreal Engine's public documentation on Live Coding and hot reload (the developer builds, the editor swaps code), concepts only; no Unreal source.

## Status
M2 phase 2.6d complete (2026-11-20); 2.8 added fingerprints.

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
