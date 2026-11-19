# concordia-assets: agent guide

## Purpose
The asset pipeline: source files with `<file>.import.ron` sidecars holding stable ids, importers producing derived data in the content-addressed DDC, built-in assets, hot reload, and `asset.import` / `asset.list` (ADR-0044).

## Public entry points
- `AssetDb` (`new`, `register`, `scan`, `import` → `ImportReport`, `load_texture` / `load_mesh` / `load_material`, `load_artifact`, `list`, `path_of`, `generation`, `on_files_changed`)
- `Importer` (trait), `ImportInput`, `ImportOutput`, `Artifact`, `AssetKind`, `SubIds`; `ImageImporter` (`png`, `jpg`, `hdr`) and `ImageSettings`; `MaterialImporter` (`.material.ron`, read with the engine's RON reader into `MaterialData`)
- `AssetRef` (`"asset:<uuid>[#sub]"`, `"builtin:<kind>/<name>"`), `Builtin` / `BuiltinAsset`
- Data: `TextureData`, `MeshData`, `MaterialData` (`.material.ron`), `AlphaMode`
- `Sidecar`, `import_key`, `PLATFORM`; `AssetsModule` (EngineModule)

## Invariants
- An asset's id lives in its sidecar: re-imports and moves (with the sidecar) keep it. Sub-asset ids are UUIDv5(parent, sub-path), recorded in the sidecar so they survive reordering.
- The DDC key covers importer name and version, platform, canonical settings and source bytes, and nothing else. Artifacts sit under `artifact_key(key, sub)` and an index under the key itself. A missing entry means re-import, never an error.
- Imports are deterministic (identical bytes in, identical blobs out). Derived data is postcard; the DDC is never committed.
- Paths are project-relative, with no `..`.
- `asset.import` touches files, not the scene, so it is attributed in `log.tail` and not undoable (decision D9).

## How to test
`cargo nextest run -p concordia-assets`

## Common tasks
- **Add an importer:** importers claim file-name suffixes (`png`, `material.ron`), so a compound suffix never captures `.scene.ron` or `.prefab.ron`. Implement `Importer` (deterministic; settings via serde from `ImportInput::settings`), `AssetDb::register` it from the composition root, and add tests for the sidecar, determinism and a missing DDC.
- **Add a built-in:** extend `Builtin::of` and the `refs` test.

## Don'ts
- Don't store absolute paths or timestamps in derived data or keys.
- Don't write the DDC except through `concordia_core::ddc::Ddc`.

## References
- `image` crate (MIT/Apache) for PNG, JPEG and Radiance HDR; postcard (MIT/Apache).

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
