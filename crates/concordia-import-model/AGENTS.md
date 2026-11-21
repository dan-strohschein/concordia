# concordia-import-model: agent guide

## Purpose
Format-neutral model import shared by the glTF and FBX importers: stable sub-asset names, the prefab from a node tree, mesh finishing, texture color spaces and skin weight reduction, so every model format produces the same artifacts.

## Public entry points
- `ModelTree` / `ModelNode` / `MeshPart` and `write_prefab` (→ each node's `EntityId`)
- `load_prefab` → `PrefabSummary` / `EntitySummary`, and `load_prefab_doc` (the full `PrefabDoc`)
- `finish_mesh` (bounds, smooth normals, zero UVs, tangents, validation), `generate_tangents` (MikkTSpace, glTF's V-down convention)
- `unique_names`, `TextureUsage` (`settings()` for `decode_image`), `reduce_influences`, `MAX_INFLUENCES`, `insert_artifact`

## Invariants
- A `MeshPart` with a `skin` (`skin/<index>`) gives its renderer entity `Skinned` beside `MeshRenderer` (2.4.1).
- Prefab entity ids are UUIDv5(asset id, `entity/node/<name>`); extra mesh parts are child entities `entity/node/<name>/primitive/<p>`. The prefab is canonical `.prefab.ron` text at sub-path `""`. A node reached twice, or a child index out of range, is an error.
- Names: the source name, or the index when missing; duplicates get `#<index>`; `/` becomes `_`.
- Influences: the four largest finite, positive weights, normalized; ties keep the lower joint; none → joint 0 fully.
- The glTF importer's artifacts were byte-identical before and after moving this code here (2.3d).

## How to test
`cargo nextest run -p concordia-import-model`; the glTF and FBX importer tests cover it end to end.

## Common tasks
- **A new model importer:** read the format into a `ModelTree` and `MeshData` streams (UVs V-down), then call `write_prefab`, `finish_mesh` and `reduce_influences`.

## Don'ts
- Don't change id derivation or names: they keep imported ids stable across re-imports.

## References
- Morten Mikkelsen, MikkTSpace (`bevy_mikktspace`, Zlib/MIT/Apache); Khronos glTF 2.0 specification (UV convention, skins).

## Status
M2 phase 2.3d (moved out of `concordia-import-gltf`); `MeshPart::skin` and `Skinned` in 2.4.1; `load_prefab_doc` reads through `AssetDb::load_prefab_doc`.

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
