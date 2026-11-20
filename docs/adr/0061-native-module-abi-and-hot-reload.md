# ADR-0061: The native script tier: C ABI, hot reload and migration
- Status: Accepted (M2 phases 2.6c and 2.6d, 2026-11-20; author's decisions)
- Date: 2026-11-20
- Builds on: ADR-0003 (scripting tiers), ADR-0062 (the script protocol and backend seam), ADR-0063 (determinism contract)

## Context
ADR-0003's first tier is gameplay written in Rust, compiled to native code and reloaded while the game runs, the way Unreal reloads C++. ADR-0062 made the scripting host swappable: modules speak one protocol, and backends carry it. The native tier is a second backend for that protocol. It must:
- load a rebuilt library while play continues, without losing the world;
- survive a component's layout changing between builds;
- refuse libraries built for another ABI;
- contain guest panics like WASM traps;
- give the same results as linking the module into the engine;
- keep the "no hidden state" rule, without a sandbox to enforce it.

## Decision
1. **A versioned C ABI** (`concordia-script-abi`; author's decision: its own small crate, so host and guest share one definition and the guest SDK stays free of `unsafe`).
   - A module library exports two symbols. `concordia_module_abi_version() -> u32` is read first. `concordia_module_v1() -> *const ModuleV1` returns a `#[repr(C)]` table: `abi_version`, `describe`, `run` and `free`.
   - A version mismatch is refused before the table is read, because a module for another ABI may lay its table out differently.
   - `describe` returns the `ModuleDesc` as JSON. It's called once per load, and type descriptions are JSON already (ADR-0062).
   - `run` takes one encoded call (processor, tick context, batch) and returns one encoded reply: log lines, then the output or an error. The encoding is a small binary envelope in `concordia_script_protocol::codec`: little-endian, `u32` lengths, and column bytes copied whole. Decoding checks every length and never panics.
   - Buffers are freed by the side that allocated them (the module's `free`).
   - **Nothing unwinds across the ABI.** The guest glue catches panics and returns them as a `Panicked` reply, with the log lines kept. The host treats that like a WASM trap.
   - The guest SDK's `export_module!` emits the exports only when the module crate's own `dylib` feature is on. A plain `rlib` exports no C symbols, so several modules can link statically into one binary.
2. **The native backend** (`concordia-script-native`) has two paths.
   - **The dylib path** copies each library to a unique file before loading it (`libloading`). macOS caches loaded libraries by path, so rebuilding in place would otherwise bring back old code.
     - Old libraries stay loaded until the backend is dropped (play stops), because unloading code that may still be referenced isn't sound.
     - Copies are deleted when the backend is dropped.
   - **The static path** runs modules linked into the engine, in process. This is 2.6b's in-process backend, reused.
   - Both paths run the same `run_module` code, so they give identical world hashes.
   - All host-side `unsafe` (loading, calling the table) is in this crate. The guest side's `unsafe` (reading the call buffer, `#[no_mangle]`) is in `concordia-script-abi`.
3. **Reloads migrate types** (any backend; in `concordia-script`).
   - `ScriptHost::reload(module, registry, world)` re-registers only the component and signal types that changed. If a helper type changes, every component and signal type is re-registered.
   - Every stored value, tag value and pending signal of those types moves to the new version through reflect's `migrate`: kept fields keep their values, added fields take their defaults, and removed fields are reported.
   - The migration is all or nothing. Terra's `World::migrate_dynamic_all` stages every conversion before committing any. If one fails, the old versions are registered again and the world is left as it was.
   - The report lists the added and removed fields per type, as JSON Pointer paths. Traits are re-registered against the new versions. Processors take the new versions from the next tick.
   - A type can't move between dense and tag storage.
   - A reload still can't change the module's name or processors, or remove types: that needs a schedule rebuild (2.6d).
4. **No hidden state, by check.** Native code has no fresh instance per run, so `check_module_source` parses a module's sources with `syn` and rejects, with file and line:
   - `static mut`;
   - statics whose types are atomics, cells, locks or lazy values;
   - `thread_local!` and `lazy_static!`.

   It's a library function; 2.6d runs it before `script.reload`. WASM modules don't need it, because their statics reset every run.
5. **Fixtures.** `just native-fixtures` builds the conformance module as a library (plus a `v2` build, a `layout2` build that changes two layouts, and a library claiming ABI version 999). It uses its own target directory, so those features never unify into the workspace build.

6. **The scripts engine module and `script.reload` (2.6d; author's decisions).**
   - Scripts are an engine module, `concordia-scripting`, enabled by `[scripts]` in `concordia.toml` like `[physics]` or `[studio]`. So `script.reload` is a module method: the core set stays at 38 methods and 35 MCP tools, and `[scripts]` adds one of each. `concordia new` writes an empty `[scripts]`.
   - Modules load when the engine starts, so script components work in scenes and the API before play. A module that fails to load is logged and skipped, so the editor still opens. The app adds the module last, so script types may use every other module's types.
   - **`script.reload` reloads what is on disk; it never builds.** For each module:
     - the changed types the edit world has stored are migrated by **`EditOp::MigrateType`**, as one transaction by `script:<module>` (`Engine::commit_edits_as`);
     - then the registry and the play world move to the new build;
     - if that second step fails, the edit transaction is undone (`Engine::undo_as`).
   - **`MigrateType` is fully undoable and replayable.** Its inverse removes the component, returns the type to its previous version with nothing stored, and sets every previous value exactly. Because of this, applying, undoing, redoing and replaying commands take `&mut TypeRegistry`.
   - **The registry keeps every version of a dynamic type.** Re-registering a description it had before, while the types it refers to are unchanged, restores that exact version. Undo, rollback and repeated reloads therefore never leave a world holding a version the registry doesn't.
   - **Deterministic logs:** script processors only record log lines and faults, because they run on worker threads. An exclusive end-of-tick system sends them to the log on the engine thread, in tick then declaration order.
   - The new error kind `ScriptFailed` (−32018) covers a module that fails to load, reload or migrate.

## Alternatives
- **`script.reload` as a core method** (the plan's 39 methods and 36 tools): every project would depend on Wasmtime and the native loader, and the method would exist even where no scripts are configured.
- **Journaling an edit-world migration as `SetComponent` ops:** simpler, but undo and journal replay fail once a field is removed, because the old values no longer validate against the new type.

- **The Rust ABI across `dylib`s:** simpler to write, but it ties the engine and every module to one exact compiler and its layout choices. The C ABI with byte envelopes doesn't care which compiler built the module.
- **JSON for run calls:** one codec for everything, but per-element byte arrays would make the native tier slower than WASM.
- **A serde binary format (postcard, bincode):** a new dependency. It can't carry `TypeDesc`'s skipped fields, and it still encodes byte arrays one element at a time.
- **Unloading old libraries on reload:** it saves memory, but thread-local destructors and function pointers that outlive the swap make it unsound. Play sessions are short, so we keep them.
- **Guest-side FFI glue in the SDK:** no new crate, but the SDK would carry `unsafe`, and the host would depend on the SDK for the ABI's definition.
- **Rejecting static mutable state at compile time** (a lint in the macro): a macro only sees the item it's given, not the module's other files or statics inside functions. The source check sees everything.

## Consequences
- **The native tier runs the whole conformance suite** and gives the same world hashes as the static path, at 1 and 8 threads.
- A module crate gains a `dylib` feature to build its library: `cargo build --features dylib`.
- **`ScriptHost::reload` now takes the world** and returns a `ReloadReport`, and `ScriptHost::set_templates` replaces templates after a type change.
- **A fix to 2.6b:** after any reload, inserting a script component through the registry failed, because every type was re-registered with a new version. Only changed types are re-registered now.
- **Trust:** native modules run with the engine's privileges. Code that isn't trusted belongs in WASM (ADR-0062).
- **Since 2.6d:**
  - A scene saved before a module changed a type layout still holds the old shape. Opening it with the new module registered at engine start fails to validate the removed fields. Reload while the scene is open (the migration is journaled), then save.
  - Undoing a migration while play runs leaves the play world on the new version until play stops.
