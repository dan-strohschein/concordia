# concordia-assets: agent guide

## Purpose
The asset pipeline: source files with `<file>.import.ron` sidecars holding stable ids, importers producing derived data in the content-addressed DDC, built-in assets, hot reload, `asset.import` / `asset.list` (ADR-0044), `material.get` / `material.set` (ADR-0068, M2 2.3c), and the animation graph format (`.animgraph.ron`, M2 2.4.3).

## Public entry points
- `AssetDb` (`new`, `register`, `scan`, `import` → `ImportReport`, `load_texture` / `load_mesh` / `load_material` (a layered `Material`), `load_artifact`, `list`, `path_of`, `id_at`, `generation` (bumped whenever an import finds new content, so the renderer re-sends a changed material), `on_files_changed`)
- `Importer` (trait), `ImportInput`, `ImportOutput`, `Artifact`, `AssetKind`, `SubIds`; `ImageImporter` (`png`, `jpg`, `hdr`) and `ImageSettings`; `MaterialImporter` (`.material.ron` → `Material` through `read_material`: layered text, or M1's flat text as one slab; version 2)
- `AssetRef` (`"asset:<uuid>[#sub]"`, `"builtin:<kind>/<name>"`), `Builtin` / `BuiltinAsset`
- `AssetSource` (M4 4.1b2, moved from `concordia-render`): meshes, materials and textures by reference, implemented for `AssetDb` (and by the renderer's `MemoryAssets`); used by the renderer and the path tracer. `TextureData::linear_rgb()` and `srgb_to_linear` decode mip 0 for both (RGBA16F, RGBA8 linear and sRGB; R8 is refused).
- Data: `TextureData`, `MeshData`, `AlphaMode`; skins and clips: `SkinData`, `ClipData` (`frame_rate`: FBX's authored rate, `None` for glTF; 2.4.1; `events`: `ClipEvent { time, name }` sorted by time, checked by `validate_events`; 2.4.4), `Channel` (`target`, `target_name`: the bone's name, so clips bind to any skeleton; 2.4.1), `ChannelPath`, `Interpolation`
- `Skinned { skin }` (component, 2.4.1, ADR-0074): the entity's `MeshRenderer` mesh is deformed by `asset:<id>#skin/<index>`; registered with `MeshRenderer` by `register_types`
- Prefabs: `AssetDb::load_prefab_doc(&AssetRef, &TypeRegistry)` and `prefab_doc(reference, kind, bytes, registry)`, the one reader of prefab artifacts (`prefab.instantiate`, `concordia-import-model` and `concordia-anim` use it; 2.4.1)
- Layered materials (ADR-0068): `Material` (named `nodes`, `root`, `alpha_mode`, `double_sided`, `unlit`; registered for `schema.get {"name": "Material"}`), `MaterialNode` (`Slab`, `Vertical`, `Horizontal`, `Coverage`), `Slab`, `Haze`, `Fuzz`, `Subsurface`, `ThinFilm`, `MaterialError` (`field`, `node`), `SURFACE`; `read_material` (layered text, or M1 text migrated to one slab), `read_material_with_format` (also says `MaterialFormat::Layered` or `M1`), `write_material`, `Material::from_slab`, `validate`, `reachable`. Material artifacts (`AssetKind::Material`) hold a `Material`; `builtin:material/default` is M1's white dielectric as one slab. Evaluation is in `concordia-material`.
- M1's flat format is read only by the private `material_m1::M1Material` (M1's fields and defaults: white, metal 1, roughness 1), which migrates to one slab. Nothing writes it; `MaterialData` was retired in 2.3c.
- `MaterialGet` / `MaterialSet` (`material_method.rs`): `material.get {path}` → `{material, migrated, id}`; `material.set {path, material, actor?}` validates, writes canonical text (only if it changed), imports, logs "material saved" with the actor and publishes an `asset` event → `{id, written}`. Refusals are `InvalidParams` with `field`, `node` and a pointer such as `/material/nodes/coat/Slab/base_color/0`. `asset.import` publishes the `asset` event too.
- Animation graphs (2.4.3): `AnimGraph` (`parameters`, `nodes`, `root`; registered for `schema.get {"name": "AnimGraph"}`; `validate`, `node`, `clips`, `reachable`), `Parameter` (`Float`, `Int`, `Bool`, `Trigger`), `AnimNode` (`Clip`, `Blend2`, `BlendN`, `OneShot`, `TimeScale`, `Transition`, `StateMachine`; `children`), `TransitionInput`, `StateTransition`, `SwitchMode`, `AdvanceMode`, `FadeCurve`, `AnimGraphError` (`node()`); `read_animgraph`, `write_animgraph`; `AnimGraphImporter` (`animgraph`, version 1, `.animgraph.ron` → `AssetKind::AnimGraph`), registered by `AssetDb::new`. Evaluation is in `concordia-anim`.
- Audio kinds (2.7a.1, ADR-0066): `AssetKind::Sound` and `AssetKind::Cue`, produced by `concordia-audio`'s `SoundImporter` (`wav`, `ogg`) and `CueImporter` (`.cue.ron`). Those importers live there and are registered by whoever builds the `AssetDb` (2.7a.2: the `audio` module).
- `Sidecar`, `import_key`, `PLATFORM`; `AssetsModule` (EngineModule)
- `TextureData::linear_rgba()` (as the GPU samples it: sRGB color decoded, alpha linear, R8 as (r, 0, 0, 1)); `linear_rgb()` is its RGB.
- `AssetDb::set_setting_default(key, value)` fills an importer's null-declared settings (and the cache key) with the project's value; `AssetsModule::with_setting_default` (M4 4.1b4: `emissive_reference_ev100`, ADR-0102).
- `ImportInput::derived` (`DerivedCache`: the DDC in an import, `NoDerivedCache` elsewhere) for sub-results keyed by their own inputs, and `ImportInput::jobs` (`import_jobs()`, one pool per process) (M4 4.2)
- `AssetDb::meshlet_stats(&AssetRef)` → `Vec<MeshMeshlets>` (each mesh sub-asset's `MeshletStats`, or why it has none), and the method `asset.meshlet_stats {asset}` (`AssetMeshletStats`; read-only) (M4 4.2)

## Invariants
- An asset's id lives in its sidecar: re-imports and moves (with the sidecar) keep it. Sub-asset ids are UUIDv5(parent, sub-path), recorded in the sidecar so they survive reordering.
- The DDC key covers importer name and version, platform, canonical settings and source bytes, and nothing else. Artifacts sit under `artifact_key(key, sub)` and an index under the key itself. A missing entry means re-import, never an error.
- Imports are deterministic (identical bytes in, identical blobs out). Derived data is postcard; the DDC is never committed.
- Paths are project-relative, with no `..`.
- A layered material is valid only if every name is `[A-Za-z0-9_-]+`, the root and every reference exist, there is no cycle, every node is reachable from the root, and every value is in range (arrays per element); errors name the node and field. A node may be used twice.
- An animation graph is valid only if:
  - every node, parameter and state name is `[A-Za-z0-9_-]+`;
  - the root and every referenced node, state and parameter exist, and each parameter is of the type its use needs (blend amounts, weights and scales are `Float`, a transition's `select` is `Int`, a one-shot's `request` is a `Trigger`, conditions are `Bool` or `Trigger`);
  - there is no cycle, every node is reachable from the root, and each node has at most one parent;
  - in every state machine, every state is reachable from the start through transitions that aren't disabled, and no transition goes to itself;
  - defaults are finite, `Blend2` amounts are in 0..=1, `BlendN` weights are ≥ 0 with one per input, fades are finite and ≥ 0, and `select` defaults are in range.

  Errors name the node (`AnimGraphError::node`). Clips are checked when `concordia-anim` binds the graph.
- `asset.import` and `material.set` touch files, not the scene, so they are attributed in `log.tail` and not undoable (decision D9). `material.set` takes only `.material.ron` paths inside the project and never writes a material that fails `validate`.
- A derived sub-result is keyed by what produces it, never by the import key, so it survives unrelated setting changes; a cached entry that doesn't validate is rebuilt by its user (M4 4.2).

## How to test
`cargo nextest run -p concordia-assets`

## Common tasks
- **Add an importer:** importers claim file-name suffixes (`png`, `material.ron`), so a compound suffix never captures `.scene.ron` or `.prefab.ron`. Implement `Importer` (deterministic; settings via serde from `ImportInput::settings`), `AssetDb::register` it from the composition root, and add tests for the sidecar, determinism and a missing DDC. A model format maps into `concordia-import-model` (names, prefab, mesh finishing, weights) like the glTF and FBX importers, and is added to `concordia_app::model_importers()`.
- **Add a built-in:** extend `Builtin::of` and the `refs` test.

## Don'ts
- Don't store absolute paths or timestamps in derived data or keys.
- Don't write the DDC except through `concordia_core::ddc::Ddc`.

## References
- `image` crate (MIT/Apache) for PNG, JPEG and Radiance HDR; postcard (MIT/Apache).
- Animation graphs: Godot Engine documentation (MIT) for the node set and transition fields (`AnimationTree`, `AnimationNodeStateMachineTransition`, `AnimationNodeOneShot`, `AnimationNodeTransition`), concepts only; see `concordia-anim`.
- Layered materials: Epic Games' public Substrate documentation and the SIGGRAPH 2023 Substrate course (Hillaire & de Rousiers); no Unreal source consulted (see `concordia-material`).
- M1's flat format is glTF 2.0's metallic-roughness model (Khronos glTF 2.0 specification, §3.9).

## Status
M4 phase 4.1b2: `AssetSource` and texture decoding shared with the path tracer. M1 phase 1.4; layered material model M2 phase 2.3a; imported and loaded as `Material` since 2.3b; `material.get` / `material.set` and `MaterialData` retired in 2.3c; `Skinned`, clip bone names and frame rates, and `load_prefab_doc` in 2.4.1; the animation graph format and importer in 2.4.3; `ClipData::events` (`ClipEvent`) and `validate_events` in 2.4.4. `TextureFormat::R8` and `AssetKind::Font` / `Theme` in 2.7b.1 (the UI's glyph atlas, fonts and themes). M3 phase 3.7a: `shapes` (`cube(size)`, `plane(size)`, `sphere(radius, segments, rings)`, `mesh`), shared by the built-ins and `concordia-gen`; `Sidecar::provenance` (`Provenance { provider, provider_version, request, seed, inputs_hash }`, skipped when absent, so hand-made sidecars are unchanged); `AssetImportParams` and `AssetImportResult` exported.

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
