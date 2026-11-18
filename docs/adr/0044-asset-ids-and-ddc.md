# ADR-0044: Asset ids and the derived-data cache
- Status: Accepted
- Date: 2026-11-18

## Context
Text is the source of truth, and binaries exist only in the build cache (design principle 2). Scenes refer to assets that artists re-export and move. Derived data (decoded textures, meshes) is expensive to build, must be reproducible, and must be safe to delete.

## Decision
- **Sidecars.** Every source file has a `<file>.import.ron` holding its stable `AssetId` (UUIDv7), importer name and version, importer settings (an untyped map each importer validates), and the `sub_assets` map (`mesh/Body`, `material/Steel`, …) of UUIDv5(parent id, sub-path). Moving a file together with its sidecar keeps every reference. Sub-asset paths come from source names (index fallback), so re-exports that reorder content keep their ids.
- **References.** `AssetRef` is a string: `asset:<uuid>`, `asset:<uuid>#<sub-path>` or `builtin:<kind>/<name>`. Its JSON Schema carries a pattern, so MCP clients and the inspector can validate it.
- **The DDC.** `.concordia/ddc/<2 hex>/<64 hex blake3>` (`concordia_core::ddc::Ddc`), shared with the shader cache. Writes go to a temp file and are renamed, so a crash never leaves a partial entry. An import's key is blake3 of importer name and version, platform, canonical settings and source bytes, and nothing else. Artifacts are stored under `blake3(key, sub-path)`, with an index under the key. A miss means re-import, never an error, and deleting the directory is always safe. Derived data is postcard-encoded; prefabs are canonical RON text.
- **Imports are deterministic** (tested byte for byte). `asset.import` is attributed in `log.tail` and idempotent; it is not undoable, because it changes files rather than the scene (decision D9).
- **Importers** implement one trait (`Importer`) and are registered by the composition root: image (PNG/JPEG/HDR) and glTF in M1.
- **glTF specifics.** Meshes, materials and textures become sub-assets, and the node hierarchy becomes a prefab with `MeshRenderer` components. Skins and clips are stored for M2. Lights and cameras are stored as data until their components exist (phases 1.5 and 1.6). Tangents are generated with MikkTSpace, with V flipped for glTF's UV convention.

## Consequences
The DDC is per machine and never committed. Changing an importer's output requires bumping its version. Texture compression and platform-specific formats (M2) slot into the same key through `PLATFORM` and the settings.

## Alternatives considered
- Path-based asset ids: break on every move.
- Ids embedded in source files (impossible for PNG or glTF without modifying them).
- A database file instead of sidecars: not diffable, and conflicts in version control.
