# concordia-import-fbx: agent guide

## Purpose
The FBX importer (`.fbx`, through `ufbx-sys`): meshes, materials (one `Slab` each, as glTF), textures, the node hierarchy as a prefab, skins and animation stacks as clips. Its output uses the glTF importer's asset types and sub-asset paths; everything format-neutral comes from `concordia-import-model`.

## Public entry points
- `FbxImporter` (an `concordia_assets::Importer`, name `fbx`, version 1)

## Invariants
- Sub-asset paths: `mesh/<mesh>/<part>` (one per non-empty material part), `material/<name>`, `texture/<file stem>`, `skin/<index>`, `clip/<stack name>`. Names come from the file (`/` → `_`); missing names use the index and duplicates get `#<index>`.
- Everything arrives right-handed, Y-up, in meters (ufbx converts while loading). UVs are flipped to the engine's V-down convention. ufbx's implicit root is the prefab root; each node's material slot decides its part's material.
- Skin weights are reduced to the four largest, normalized to sum to 1; joint indices are cluster indices into `SkinData::joints`, and `inverse_bind` is ufbx's geometry-to-bone matrix (column-major).
- Clips are baked to linear keys from time 0 (a stack's start is trimmed, tested by the hand-written `late_start.fbx`); curved keys are resampled at the file's frame rate (30 if it has none), already-linear keys stay as authored. `duration` is the stack's playback length and `frame_rate` the rate used. Channels target prefab entities and carry the bone's name (`target_name`), so a clip from another file drives a skeleton by name (2.4.1).
- Renderers of a skinned mesh get `Skinned` (`skin/<index>`).
- Textures: embedded bytes, else the file next to the source through `read_relative` (sandboxed to the project). A missing file is skipped with a warning and the material keeps its factor; an undecodable image is an error.
- Bad input (garbage, empty, truncated, corrupted) is an `AssetError::Import` starting `FBX: ` and never a panic. Only `src/ffi.rs` uses `unsafe`; it checks every pointer and copies data out while the scene lives.
- Imports are deterministic.

## How to test
`cargo nextest run -p concordia-import-fbx` (fixtures in `tests/fixtures/fbx`, made by `tools/fixtures/make_fbx.py`; see its README).

## Common tasks
- **Import another FBX feature** (lights, cameras, blend shapes): add a shim accessor (`ufbx-sys` AGENTS.md), wrap it in `ffi.rs`, map it in its own module into the glTF importer's asset type, and add a fixture to the Blender script.

## Don'ts
- Don't read absolute texture paths or open files outside `read_relative`.
- Don't add format-neutral code here (names, prefab, mesh finishing, weights): it belongs in `concordia-import-model`.
- Don't commit third-party FBX files; fixtures come from our script.

## References
- ufbx (MIT) and its documentation; Blender's FBX exporter (GPL, used only as a tool to write fixtures, never as code). No Autodesk FBX SDK or other non-open source was consulted.

## Status
M2 phase 2.3d; version 2 in 2.4.1 (`Skinned`, bone names, `frame_rate`). Lights, cameras and blend shapes are not imported yet; Godot `.fbx.import` mapping is M5.

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
