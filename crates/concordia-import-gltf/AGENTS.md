# concordia-import-gltf: agent guide

## Purpose
The glTF 2.0 importer (`.gltf` and `.glb`): meshes, metallic-roughness materials (each imported as a one-slab `Material`, ADR-0068) and textures, the node hierarchy as a prefab, skins and clips (for M2), lights and cameras (ADR-0044).

## Public entry points
- `GltfImporter` (an `concordia_assets::Importer`), `SUPPORTED_EXTENSIONS`
- `generate_tangents(&mut MeshData)` (MikkTSpace, glTF UV convention)
- `load_prefab(&mut AssetDb, &ImportReport)` → `PrefabSummary`

## Invariants
- Sub-asset paths: `mesh/<mesh>/<primitive>`, `material/<name>`, `texture/<image>`, `skin/<index>`, `clip/<name>`, `light/<name>`, `camera/<name>`. Names come from the file (`/` → `_`); missing names use the index and duplicates get `#<index>`.
- Prefab entity ids are UUIDv5(asset id, `entity/node/<name>`), so reordering nodes keeps them. Extra primitives become child entities. The main artifact (`""`) is canonical `.prefab.ron` text.
- Missing normals → area-weighted smooth normals. Missing UVs → zeros. Missing tangents → MikkTSpace with V flipped (glTF V points down); with no UVs, any perpendicular.
- An unsupported extension in `extensionsRequired` is an error naming it; only triangle primitives are imported.
- Imports are deterministic.

## How to test
`just gltf-assets && cargo nextest run -p concordia-import-gltf` (Khronos samples from the pinned commit, plus hand-made fixtures in `tests/fixtures`).

## Common tasks
- **Support an extension:** enable the `gltf` feature, map it in `material.rs` / `mesh.rs`, add it to `SUPPORTED_EXTENSIONS`, and add a fixture test.
- **Add a sample model:** it must be CC0 or CC-BY-4.0 with no mark; add it to `tests/fixtures/gltf-sample-assets/manifest.ron` (with its SHA-256 and attribution) and run `just gltf-assets`.

## Don'ts
- Don't name sub-assets by index when the file has names (ids must survive re-exports).
- Don't commit sample models (they are fetched).

## References
- Khronos glTF 2.0 specification and extension specifications; Morten Mikkelsen, MikkTSpace (`bevy_mikktspace`, Zlib/MIT/Apache); `gltf` crate (MIT/Apache).

## Status
M1 phase 1.4.

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
