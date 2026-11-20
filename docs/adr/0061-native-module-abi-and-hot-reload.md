# ADR-0061: The native script tier: C ABI, hot reload and migration
- Status: Accepted (M2 phase 2.6c, 2026-11-20; author's decisions)
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

## Alternatives
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
