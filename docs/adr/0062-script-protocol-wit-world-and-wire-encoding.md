# ADR-0062: The script protocol: script-defined types, wire encoding and the WIT world
- Status: Accepted (M2 phase 2.6a, 2026-11-20: types, wire encoding, named access; phase 2.6b, 2026-11-20: the backend seam, the WIT world, the WASM backend and its determinism settings; author's decisions)
- Date: 2026-11-20
- Builds on: ADR-0003 (scripting tiers), ADR-0021 (WASM guest build path), ADR-0063 (determinism contract), ADR-0069 (commands and signals), ADR-0070 (templates and traits)

## Context
Phase 2.6 brings gameplay scripts to Terra. Before it started, the author weighed whether to keep WASM or switch to C++ or C#. Two requirements came out of that:
- **Speed:** nothing may be JIT-compiled or interpreted at runtime.
- **A swappable scripting host,** so WASM can be replaced later.

Scripts must also define their own components, signals and traits. Otherwise WASM mods and agent-written code could only touch engine components.

Terra knew only Rust types, keyed by `TypeId`. Its processors reached components through typed views. Neither works for a module loaded at runtime.

## Decision
1. **The contract is a protocol, not a runtime.**
   - A script module describes its types and processors.
   - It exchanges wire-encoded batches with the engine, one call per processor per tick.
   - The WASM component model (WIT) and the native C ABI are two bindings of that protocol.
   - Engine-side features are written once against a backend trait (2.6b).
2. **Script-defined types are dynamic reflect types.**
   - A `TypeDesc` is a serializable mirror of `TypeInfo`: shape, docs, defaults, ranges and widgets.
   - `TypeRegistry::register_dynamic` builds and leaks a `TypeInfo` from it, then registers or replaces it by name.
   - Values are `DynamicValue`s: the type plus its canonical wire bytes.
   - A dynamic type behaves exactly like a `#[derive(Reflect)]` type of the same shape. Its JSON, canonical hash, schema, scene text and wire bytes are all the same, and tests pin this.
   - Descriptions may always name the built-in scalars, `Any`, `EntityId` and the glam types.
   - A static type's name can't be taken.
   - `migrate` converts a value to a new version of its type: kept fields keep their values, added ones take their defaults, and removed ones are reported.
3. **The wire encoding** is a fixed layout per type, derived from its `Shape`:
   - little-endian scalars with no padding;
   - `Option` as a `u8` tag plus its payload;
   - enums as a `u32` tag plus the largest payload, with unused bytes zero;
   - UUIDs as 16 bytes.

   Strings, lists, maps and `Any` (as JSON text) are `(u32 offset, u32 length)` references into a side arena after the fixed rows.
   - A column is `rows × stride` fixed bytes, then the arena. Readers get O(1) access to any row.
   - The encoding is canonical: arena data is laid out depth-first in declaration order, and map keys ascend.
   - Decoding validates bounds, tags, UTF-8, scalars and ranges, and never panics.
   - The codec is serde-driven and guided by the shape, so Rust values, JSON and dynamic values encode identically, with no derive changes.
4. **Terra identifies components by stable name** (unique per world) in processor access, presentation sets, observers and views. `TypeId` stays only for typed fast paths.
   - Dynamic components are stored in their own columns; types with no data are tags.
   - Processors declare access by name (`reads_named`, `writes_named`, `receives_named`) and join the same conflict graph and waves.
   - `View::batch` encodes the declared columns of every match into wire columns, in typed-query order.
   - `View::write_batch` decodes rows back atomically.
   - Signal payloads arrive as wire columns.
   - By-name commands (`insert_reflect`, `remove_named`, `signal_reflect`) apply at phase end like every command.
5. **The backend seam (2.6b).** `concordia-script` holds `ScriptBackend` (loads a `ModuleSource`: a file, bytes, or a module linked into the engine) and `ScriptModule` (`describe`, `run(&self, processor, tick context, batch)`, crossing stats).
   - Everything engine-side is written once there: `ScriptHost` registers a module's types and traits, and each declared processor becomes a `ScriptProcessor` in Terra's schedule.
   - The protocol's types live in their own small crate, `concordia-script-protocol`, so guests don't compile the engine (author's decision, 2026-11-20).
   - A **conformance suite** (`conformance_suite!`) checks every backend against one conformance module. The in-process `TestBackend` runs SDK modules natively and treats a panic as a trap.
6. **One crossing per processor per tick.** A run receives the processor's declared columns (reads, then writes) for every match and its delivered signals, and returns full column writes plus commands.
   - The run's whole output is decoded and checked before anything applies, so a fault (trap, out of fuel, returned error, or protocol violation) applies nothing. The processor is disabled until a reload, and the fault is reported and logged as `script:<module>`.
   - Commands apply at phase end in plan order (ADR-0069). `spawn-template` spawns a compiled template.
   - Time and randomness ride in the tick context. Each processor gets its own seed: the first 8 bytes of blake3 over (session seed, tick, name).
7. **Script traits** are parameterless traits that add components with their type defaults and require processors (`register_dynamic_trait`). Templates use them like Rust traits.
8. **The WIT world** `concordia:game/gameplay@0.1.0` (`wit/concordia-game`):
   - it exports `describe() -> module-desc` and `run(processor, tick-context, batch) -> result<run-output, string>`;
   - it imports only `host.log` (the `types` interface declares types, not functions);
   - type descriptions cross as canonical JSON of `TypeDesc`, because WIT has no recursive types;
   - column data crosses as `list<u8>` wire bytes, and entity ids as 16 big-endian UUID bytes.
9. **The WASM backend** (`concordia-script-wasm`, Wasmtime pinned at `=51.0.0`, which fixes RUSTSEC-2026-0587):
   - **Ahead of time:** components are compiled once with Cranelift and cached in the DDC under (namespace, wasm bytes, Wasmtime version, blake3 of `Engine::precompile_compatibility_hash`). A rejected entry is recompiled. The only `unsafe` is deserializing such an entry.
   - **Deterministic:** NaN canonicalization, no wasm threads (Wasmtime is built without its `threads` feature, nor its `cache` feature, which pulls in MPL-2.0 code), deterministic relaxed SIMD, fuel metering and no epoch interruption.
   - **Fuel per run:** `per_run + per_row × (rows + delivered signals)`. The default is 10M + 50k per row, chosen from the benchmark (a trivial integrator needs about 13k per row). Exhausting it is a deterministic `OutOfFuel` fault.
   - **A fresh instance per run** from one `InstancePre`, with the pooling allocator, whose `max_memory_size` is the memory cap. A fresh instance costs about 21 µs on the reference machine.
   - **Virtual WASI:** both clocks read the run's simulated time; secure and insecure random are the processor's streams 1 and 2; there are no preopens, env, args or stdin; stdout and stderr become log lines.
   - Guests use `std` on `wasm32-wasip2`, with these virtualized imports (this settles ADR-0021's open question; `no_std` isn't needed).

## Alternatives
- **Persistent instances per processor (2.6b):** instantiating would cost nothing, but guest statics would survive between runs, so hidden state could break determinism and reloads. A fresh instance costs about 21 µs per processor per tick. Kept as the fallback if that ever matters.
- **Epoch interruption instead of fuel:** cheaper, but it depends on wall-clock time, so faults wouldn't be reproducible.
- **`cargo component` or typed WIT records for type descriptions:** rejected (ADR-0021, and WIT has no recursive types).
- **A schema'd format such as postcard or bincode:** compact, but variable-length, so there's no random access to rows. The guest would need a decoding pass, and a column couldn't be patched in place.
- **The component model's own canonical ABI for every component:** a record type per component. Script-defined types would then need generated WIT at runtime, and the native tier couldn't share it.
- **Storing dynamic values as JSON:** simple, but it loses `f32`-ness, so hashes would differ from Rust twins, and every batch would cost a JSON round trip.
- **Synthetic `TypeId`s for dynamic types:** impossible on stable Rust. Name identity is already unique per world.
- **Keeping WASM out and embedding C# or C++ instead:** C# needs a runtime with a JIT or NativeAOT. C++ has no sandbox and no cross-platform float determinism. Both fit the native tier's C ABI later.

## Consequences
- **Leaks:** dynamic type infos are leaked, once per registration (including reloads). That's bounded in practice and documented.
- **Versions:** a world stores one version of each dynamic type. A new version must be migrated in before values of it are inserted (2.6c, 2.6d).
- **API changes:** `FieldInfo::default` and `TypeInfo::default` became `DefaultValue` (`get()`), so dynamic types can carry stored defaults. `TypeRegistration::deserialize` became a method. `TypeRef` gained direct references (`TypeRef::to`).
- **Unchanged:** edit-world hashes and command journals are unaffected.
- **Reloads (2.6b)** swap a module's build between ticks and keep the world. Since 2.6c (ADR-0061), changed types are migrated through reflect. A changed name or processor set is still refused until 2.6d adds schedule rebuilds.
- **Cost (2.6b, `crates/concordia-script-wasm/benches/BASELINE.md`):**
  - With every guard, WASM takes about 1.55–1.6× native time for the same logic.
  - Fuel costs about 30% over unguarded WASM; NaN canonicalization costs nothing measurable.
  - The per-row wire path (the host's batch and write-back, and the SDK's serde decoding) dominates both natively and on WASM, at about 0.9 µs per entity natively.
  - A typed fast path for fixed-layout columns is the next optimization; it changes no protocol.

## Update (M2 phase 2.10a, 2026-11-23): input reaches scripts as a component
- **Problem:** scripts see only component batches, signals and commands, never resources. Player input lives in the `ActionStates` resource, so no script could read it.
- **Decision:** `concordia-input` adds the `InputActions { watch, states }` component. Its `input.actions` system (in `PrePhysics`, after `input.sample` and `ui.interact`, before `nav.click`, each when scheduled) fills `states` every tick for the watched names. Unknown names are left out.
- **Consumed input** (the UI holding the pointer or the keyboard) reads as released with the kind's zero value. Edges come from the mirror itself: a consumed action that showed pressed reports `just_released`, and one seen again while held reports `just_pressed`. Typing in a text box therefore never moves a character.
- **Not changed:** the protocol (no WIT change) and the wire encoding. Scripts mirror `InputActions`, `ActionState` and `ActionValue` field for field, as they mirror other engine types.
- **Ordering:** a script that moves a character reads it in `PrePhysics` with `after("input.actions")`; any later phase works too.
- **Alternative rejected:** resource access in the protocol. It would add a second data path, and its access declarations, to both backends, for one resource.
