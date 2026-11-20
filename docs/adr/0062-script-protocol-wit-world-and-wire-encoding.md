# ADR-0062: The script protocol: script-defined types, wire encoding and the WIT world
- Status: Accepted in part (M2 phase 2.6a, 2026-11-20: types, wire encoding, named access; author's decisions). Phase 2.6b adds the backend seam, the WIT world and the determinism settings.
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

## Alternatives
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
