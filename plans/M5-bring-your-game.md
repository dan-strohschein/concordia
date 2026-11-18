# M5: Bring your game
Status: Planned · Depends on: M2 (importer framework, `concordia-interchange` v0, WASM host), M3 (replay, playtest assertions, Studio), M4 (visual graphs, `concordia-graph`) · Detail level: **Needs refinement after M4** · Estimated effort: ~19 weeks (range 16–24), author plus agents

> This plan has full phase breakdowns and test lists. It must get a refinement pass when M4 completes, because the shapes of `concordia-interchange` v0 (M2), the replay and assertion APIs (M3) and the `.graph.ron` node set (M4) will change details here. Places that depend on those shapes are marked **(refine)**.

## Goal & context
Makers should be able to bring an existing Unity or Unreal game over with little fuss. M5 delivers:

1. A generalized, versioned **interchange model** (`concordia-interchange` v1) that every importer (Godot, Unity, Unreal) targets. It includes a machine-readable **unmapped-feature report** that agents use as a work list.
2. A **direct Unity project importer** (`concordia-import-unity`), plus an optional Unity editor package (`tools/unity-exporter`) for data the YAML can't express.
3. An **Unreal path**: our own editor plugin (`tools/unreal-exporter`) that writes an interchange bundle using Unreal's own glTF/USD exporters plus JSON metadata, and `concordia-import-unreal` to read that bundle. An experimental, feature-flagged direct `.uasset` reader sits beside it.
4. **Logic translation tooling.** C# MonoBehaviours and Blueprints are translated *by the user's own agents* (Claude Code on a subscription, with no API keys), using MCP tools the engine provides. Those tools serve structure and ASTs, API mapping tables and test scaffolds generated from observed behavior.
5. A **license surfacing report** that tells the user where content probably came from (Unity Asset Store, Fab, engine or sample content) and what to check. It never decides legality.

All of this is proven on **sample projects we author ourselves** (MIT). Their exported bundles and YAML are committed as fixtures, so CI never needs a Unity or Unreal editor.

Principles applied: subscription-first (the engine provides tools and the agents think), one API (`import.*` and `translate.*` are Engine API methods, so the CLI, MCP and editor all share them), text as source of truth (all import output is `.scene.ron`/`.prefab.ron`/`.material.ron` plus source-format assets with `.import.ron` sidecars), closed loop (oracles, replay and goldens), transactions (an import is one attributed transaction), clean room (the exporter is our code against public plugin APIs, and the `.uasset` reader learns only from permissively licensed readers and public observation), and TDD.

## Scope / Non-goals
**In scope**
- `concordia-interchange` v1: entity/component/scene model, PBR material mapping with fallbacks, skeletons and animation, animation controllers (best effort), lights, cameras, physics shapes, bodies and materials, collision layers, audio references, prefabs, prefab variants and overrides, instancing, terrain/heightfields, script references (translation units), behavior traces, the import report, coordinate-system normalization, schema versioning and migrations, and a generated JSON Schema for external exporters.
- Unity 2022.3 LTS and Unity 6 (6000.x) projects using Force Text serialization; best effort for 2021.3. Built-in Standard, URP Lit and HDRP Lit materials. FBX (via ufbx, from M2) and OBJ models. Textures (PNG, TGA, JPG, PSD composite, TIFF, EXR). Audio formats `concordia-audio` supports.
- Unreal Engine 5.4+ editor plugin (C++ and Python) plus a headless commandlet; `.ixbundle` reader. Unreal 6 support is tracked as an open question.
- An experimental `.uasset`/`.umap` reader for **uncooked editor packages only** (inventory, dependency graph, tagged properties of simple objects), behind the `experimental-uasset` cargo feature, off by default.
- Translation tooling for C# and Blueprints, targeting Rust, WASM (`concordia:game` WIT) or `.graph.ron`.
- The license surfacing report.
- Our own Unity and Unreal sample projects, committed fixtures, `just export-fixtures`, and the M5 gate.

**Non-goals (M5)**
- Automatic logic conversion. Agents translate logic; the tooling only serves structure, maps, mechanical skeletons and tests.
- Unity binary serialization (we detect it and tell the user how to switch), Unity UI/uGUI/UI Toolkit/TextMeshPro layouts, Shader Graph/Amplify custom shaders beyond parameter extraction, VFX Graph, Timeline, Cinemachine, NetCode, and DOTS/Entities.
- Unreal cooked content, IoStore (`.utoc`/`.ucas`), `.pak` reading, Niagara, Control Rig, MetaSounds, World Partition streaming (the plugin exports loaded cells only), and Nanite/Lumen settings beyond what the report records.
- Re-import that preserves user edits made after import (later backlog). M5 re-import is deterministic overwrite, so it diffs cleanly.
- A legal verdict on any asset.
- Round-tripping back *into* Unity or Unreal. "Round-trip" in design.md's M5 gate means source → engine, verified against oracles exported from the source editor.

## Dependencies (what must already exist, from which milestone)
| Need | From |
|---|---|
| ECS, reflection (`#[derive(Reflect)]` → JSON Schema), command log with `Actor`, `.scene.ron`/`.prefab.ron` with prefab overrides, Engine API and MCP adapter | M0 |
| `concordia-assets` (importer registry, DDC, `.import.ron` sidecars), `concordia-import-gltf`, `concordia-render` PBR and capture, `concordia-physics` (Jolt: box/sphere/capsule/convex/trimesh/heightfield shapes), golden-image harness (FLIP) | M1 |
| `concordia-interchange` v0 (Godot), FBX import via ufbx, `concordia-script-wasm`, `concordia-audio`, `concordia-anim` (skeletons, clips, state machines), minimal `concordia-replay`, `concordia-import-godot` GDScript translation support | M2 |
| `concordia-replay` record/replay, playtest assertions (`assert.eval`), `concordia-studio` (`studio sync`, skills, task board) | M3 |
| `concordia-graph` (`.graph.ron`, node registry, compiler) | M4 |
| Local only, for regenerating fixtures: Unity Hub with Unity 6 LTS; Unreal Engine 5.x via the Epic launcher; Blender 5.2 (installed) | user machine |

## Crates, modules & directories (created / touched)
**Created**
- `crates/concordia-import-unity/`
  - `src/project.rs` (folder scan, `ProjectVersion.txt`, serialization-mode detection), `src/meta.rs` (`.meta` GUID database), `src/yaml/` (Unity YAML document splitter and tolerant parser), `src/class_id.rs` (class-ID registry from Unity's public "YAML Class ID Reference")
  - `src/refs.rs` (fileID/GUID resolution), `src/prefab.rs` (instances, variants, overrides, stripped objects)
  - `src/components/` (transform, renderers, colliders, rigidbody, light, camera, audio, animator, terrain, monobehaviour)
  - `src/material.rs` (Standard/URP Lit/HDRP Lit → PBR), `src/model.rs` (ModelImporter settings, FBX/OBJ subobject mapping), `src/texture.rs`, `src/anim.rs` (AnimatorController and `.anim` → interchange), `src/settings.rs` (tags, layers, physics matrix, time)
  - `src/export_bundle.rs` (merges optional `tools/unity-exporter` output), `src/license.rs` (Asset Store/UPM evidence)
- `crates/concordia-import-unreal/`
  - `src/bundle.rs` (`.ixbundle` reader and validation), `src/level.rs`, `src/material_graph.rs` (UE material expression graph → PBR rules), `src/landscape.rs`, `src/blueprint.rs` (Blueprint JSON → translation units and graph skeletons), `src/license.rs` (Fab/engine/sample-content evidence)
  - `src/uasset/` (behind feature `experimental-uasset`): `summary.rs`, `names.rs`, `imports_exports.rs`, `tagged_props.rs`, `inventory.rs`
- `crates/concordia-translate/` (**proposed new crate**; see the final section): the translation-unit model, C# front end (tree-sitter-c-sharp), Blueprint graph model, API map tables (`maps/unity-api.map.ron`, `maps/unreal-k2.map.ron`, migrating `maps/godot-api.map.ron` from M2), scaffold generator, and trace → replay/assertion converter. It is shared with `concordia-import-godot`.
- `tools/unity-exporter/`: a UPM package `com.concordia.exporter` (C#): `Editor/Core/` (pure C#, no UnityEngine references: JSON writer, schema DTOs), `Editor/Export/` (ScriptableObjects, terrain, lighting/baked data, procedural meshes via glTFast, oracle bounds), `Editor/Trace/` (Play Mode behavior-trace recorder), `Editor/CLI.cs` (batch-mode entry points), and `Tests/Editor/` (Unity Test Framework EditMode).
- `tools/unreal-exporter/ConcordiaExporter/`: `ConcordiaExporter.uplugin`, `Source/EngineExporterEditor/` (C++ editor module: Blueprint graph dump, material expression dump, landscape heightmap/weightmap export, `UEngineExportCommandlet`, PIE trace recorder), `Content/Python/concordia_exporter/` (orchestration, glTF/USD export calls, manifest writing), `tests/` (pytest against a stub `unreal` module, runnable in CI).
- `tools/unreal-exporter/samples/build_sample.py`: rebuilds the Unreal sample level from shared sources inside the editor.
- `tools/fixtures/`: `blender/make_sample_meshes.py` (our meshes, FBX/OBJ/glTF), `textures/make_textures.py` (procedural PNGs), `export_fixtures.sh` (driven by `just export-fixtures`).
- `examples/shared-sources/` (meshes, textures and WAVs we generate; MIT), `examples/unity-sample/` (Unity project, Force Text, Visible Meta Files), `examples/unreal-sample/` (UE project, Blueprint-only, no Starter Content).
- `tests/fixtures/unity/` (minimal hand-checked YAML, per-version captures, per-pipeline `.mat` captures, `unity-sample-export/`, `traces/`, `oracle/`), `tests/fixtures/unreal/` (`sample.ixbundle/`, `uasset/`, `traces/`, `oracle/`), `tests/fixtures/license/` (synthetic Asset Store cache and Fab evidence built from our own content), `tests/fixtures/fixtures.lock.ron`.
- `tests/gates/m5_unity_import.rs`, `m5_unreal_import.rs`, `m5_translation.rs`, `m5_licenses.rs`; `tests/golden/m5/`; `tests/replays/m5/`.
- `studio/skills/port-unity-script/`, `studio/skills/port-blueprint/`, `studio/skills/review-import-report/`.

**Touched**
- `crates/concordia-interchange` (generalized to v1), `crates/concordia-import-godot` (migrated to v1 and `concordia-translate`), `crates/concordia-assets` (texture channel repack/invert derived data, `.ixbundle` registry entries), `crates/concordia-api` and `concordia-mcp` (`import.*` and `translate.*` methods), `crates/concordia-cli` (`concordia import ...`, `concordia translate ...`), `crates/concordia-render` (terrain splat material template), `crates/concordia-physics` (collision-layer matrix from import, friction/restitution combine modes), `crates/concordia-anim` (controller import surface) **(refine)**, `justfile` (`gate M5`, `export-fixtures`, `test-tools`), and CI (a Python tool-test job; `cargo-deny` allowlist for new dependencies).

## Phases

### Phase 5.1: Interchange v1 (schema, normalization, report, lowering)
- **Objective:** Turn `concordia-interchange` from the Godot-shaped v0 into a versioned, engine-neutral v1 that all three importers and both external exporters target, and lower it into engine scene, prefab, material and asset files in one transaction.
- **Deliverables**
  - Reflected Rust types: `IxDocument { schema_version, source: IxSource, coordinate_system: IxCoordSys, assets, textures, materials, skeletons, animations, anim_controllers, prefabs, scenes, physics: IxPhysicsSettings, scripts: Vec<IxScriptRef>, traces, report: ImportReport }`.
  - Scene model: `IxNode { id: IxId, name, parent, transform: IxTrs, active, tags, layer, components: Vec<IxComponent>, prefab_instance: Option<IxPrefabInstance> }`. `IxId` is a source-stable string such as `unity:<guid>:<fileID>` or `unreal:/Game/Maps/Courtyard.Courtyard:PersistentLevel.BP_Door_2`.
  - `IxComponent` variants: `MeshRenderer`, `SkinnedMesh`, `InstancedMesh`, `Light`, `Camera`, `Collider`, `RigidBody`, `AudioSource`, `Animator`, `Terrain`, `Script`, `Unknown { source_type, raw }`.
  - Materials: `IxMaterial { name, model: IxPbr, fallback: Option<IxFallback>, source_shader }`. `IxPbr` follows glTF metallic-roughness plus emissive strength, clearcoat, transmission and alpha mode/cutoff. Every texture slot is an `IxTextureRef { texture, uv_set, transform, channels: ChannelMap }`, where `ChannelMap` gives a swizzle plus per-channel invert, so Unity's metallic-R/smoothness-A and Unreal's DirectX normals become derived-data repacks. `IxFallback { reason, nearest: IxPbr, baked: Option<baked textures> }`.
  - Lights carry `IxLightIntensity { value, unit: Lux | Lumen | Candela | Nits | Unitless { source_pipeline } }`; conversion to engine units is centralized and reported when it approximates.
  - Physics: `IxShape` = `Box | Sphere | Capsule { axis } | ConvexMesh | TriMesh | Heightfield`; `IxBody`; `IxPhysicsMaterial { friction, restitution, combine modes }`; a layer matrix.
  - Prefabs: `IxPrefab { roots, variant_of, overrides: Vec<IxOverride { target: IxId, property_path, value }>, removed_components, added_nodes }`.
  - Animation: `IxAnimController { parameters, layers, state_machines, states, transitions (conditions, exit time, duration), blend_trees_1d_2d }` (best effort).
  - Terrain: `IxTerrain { heightmap (16-bit PNG asset), size_m, height_scale_m, layers (albedo/normal textures, weightmaps), holes }`.
  - `IxScriptRef { unit_id, language: CSharp | Blueprint | GDScript, source_path, field_values }`, and `IxBehaviorTrace` (defined in 5.7, schema lives here).
  - **Import report:** `ImportReport { schema_version, importer, source, summary, entries: Vec<ReportEntry> }`, with `ReportEntry { key (stable hash of category, source ref and feature), severity: Info | Approximated | Dropped | NeedsAgent | Blocked, category: ReportCategory, source_ref { file, object, property_path }, message, fallback_applied, agent_hint: Option<{ tool, args }> }`. Entries are canonically sorted. It is written as `import-report.ron` and returned by `import.report`.
  - `normalize(doc) -> IxDocument`: converts any declared coordinate system to engine space (handedness, up axis, forward, meters per unit) for transforms, quaternions, mesh-space references and light/camera directions.
  - `migrate`: v0 (M2 Godot) → v1, with policy: minor versions are additive, and a major bump requires a migration plus a fixture.
  - A generated JSON Schema at `crates/concordia-interchange/schema/interchange-v1.schema.json` (from `concordia-reflect`, never hand-written), consumed by the C# and Python exporters.
  - `lower(doc, opts) -> ImportPlan` (pure) and `apply(plan, actor)`, which writes `.scene.ron`, `.prefab.ron`, `.material.ron`, copied source assets with `.import.ron` sidecars, `import-report.ron` and `port/units/*.unit.ron` through `concordia-command`, in one transaction.
  - Deterministic engine ids for imported entities: a name-based UUID derived from `IxId` plus the project's import namespace, so re-import is stable (see convention additions).
  - Engine API: `import.scan` (dry run that returns plan summary and report), `import.run`, `import.report`; MCP tools `import_scan`, `import_run`, `import_report`.
- **Tests first**
  - `ix_schema_json_matches_snapshot`: the generated JSON Schema equals the committed insta snapshot, so any schema change is visible in review.
  - `ix_document_roundtrips_ron_and_json` (proptest): arbitrary `IxDocument`s survive RON → value → JSON → value unchanged.
  - `ix_rejects_unknown_major_version`: `schema_version: "2.0"` fails with a typed `UnsupportedVersion` error naming the supported range.
  - `ix_accepts_newer_minor_with_unknown_fields_reported`: a v1.9 document with an unknown field loads and produces an `Info` report entry instead of failing.
  - `ix_v0_godot_fixture_migrates_to_v1_snapshot`: the M2 Godot interchange fixture migrates to a v1 snapshot and lowers to scene output identical to M2's.
  - `normalize_left_handed_y_up_negates_z_position`: a Unity-space point `(1,2,3)` maps to engine `(1,2,-3)`.
  - `normalize_left_handed_rotation_preserves_visual_orientation`: a rotated cube's world-space corner positions match after conversion (property test over random quaternions).
  - `normalize_unreal_cm_z_up_to_engine_meters_y_up`: `(100, 200, 300)` cm in UE space maps to the expected meters in engine space, and a UE forward vector maps to engine forward.
  - `normalize_is_identity_for_engine_space`: no-op on a document already in engine space.
  - `pbr_channel_map_unity_metallic_smoothness_repacks`: a 2×2 metallic(R)/smoothness(A) texture produces engine roughness = 1 − smoothness and metallic in the expected channels through `concordia-assets` derived data.
  - `pbr_channel_map_directx_normal_inverts_green`: the green channel is inverted, and the result is cached by content hash.
  - `material_fallback_always_yields_valid_engine_material`: any `IxMaterial` with only a fallback lowers to a `.material.ron` that `concordia-render` loads.
  - `light_unitless_intensity_is_converted_and_reported_approximated`: the result carries an `Approximated` entry naming the source pipeline and the scale used.
  - `report_entries_sorted_and_keys_stable`: the same input in shuffled order gives byte-identical `import-report.ron`.
  - `report_key_stable_across_runs`: the hash for a fixed entry equals a committed constant.
  - `lower_prefab_variant_produces_prefab_with_overrides`: a variant lowers to a `.prefab.ron` that references its base, with only overridden properties.
  - `lower_assigns_deterministic_entity_ids`: lowering twice gives identical ids; a different namespace gives different ids.
  - `apply_is_single_transaction_with_actor`: after `import.run` with actor `agent:porter#1`, the command log has one transaction attributed to that actor, and `txn.undo` removes every created file and entity.
  - `import_scan_does_not_mutate_project`: the project tree hash is unchanged after `import.scan`.
  - `mcp_import_tools_schemas_generated_from_reflect`: MCP `import_*` input schemas equal the reflected schemas.
- **Implementation notes**
  - Keep `IxComponent` closed (an enum) plus `Unknown` so every importer's leftovers become report entries, never silent drops. Every lowering branch that drops data must push a `ReportEntry` (enforced by a test helper `assert_no_silent_drop` used in the importer suites).
  - Coordinate conventions: engine space is right-handed, +Y up, meters (the glTF convention), assuming M0/M1 fixed that **(refine)**. Unity is left-handed, +Y up, +Z forward, meters. Unreal is left-handed, +Z up, +X forward, centimeters. Implement conversion as a single basis-change matrix `B` with `T' = B·T·B⁻¹`, and test it with property tests rather than per-case formulas.
  - JSON on the wire from external exporters; canonical RON for anything written into a game project.
  - References: glTF 2.0 spec (PBR, `KHR_materials_*`, `KHR_texture_transform`), OpenUSD `UsdPreviewSurface` docs, Unity Manual "Standard Shader"/"URP Lit"/"HDRP Lit" pages, Unreal docs "Physical Lighting Units".
- **Done criteria:** all tests green; the Godot importer runs on v1 with its M2 tests unchanged; the JSON Schema is committed; `crates/concordia-interchange/AGENTS.md` documents the versioning policy and the report format for agents.

### Phase 5.2: Sample projects & fixture pipeline
- **Objective:** Create small, legally redistributable Unity and Unreal samples that we author under MIT, plus a reproducible, manual fixture-regeneration step, so every later phase tests against real editor output.
- **Deliverables**
  - `examples/shared-sources/`: Blender-scripted meshes (crate, pillar, door, pickup gem, a skinned "bobber" with a 3-bone rig and one clip, a low-poly ground), procedural textures (albedo, normal, metallic/smoothness, mask), and synthesized WAVs (an ambient loop and a pickup chime). Every file is generated by our scripts, and licenses are recorded in `examples/shared-sources/LICENSES.md`.
  - `examples/unity-sample/` (Unity 6 LTS, URP, Force Text, Visible Meta Files):
    - `Assets/Scenes/Courtyard.unity`
    - Prefabs `Crate`, `CrateVariant` (color and scale override, one added component, one removed component) and `Pickup` (nested prefab)
    - URP Lit materials (opaque, alpha-clip, emissive, metallic-mapped)
    - FBX and OBJ models, a terrain patch, a directional, point and spot light, two cameras
    - Box/sphere/capsule/mesh colliders, a rigidbody stack, a physic material, an AudioSource
    - An Animator for the door (2 states, 1 bool parameter), the bobber clip
    - `Scripts/PickupSpinner.cs` (bob, rotate, trigger → score and deactivate) and `PlayerMover.cs` (CharacterController WASD)
    - A `LootTable` ScriptableObject
    - A deliberately unmappable custom shader and component (to exercise the report)
  - `examples/unreal-sample/` (UE 5.x, Blueprint-only, no Starter Content):
    - `Courtyard.umap` built by `build_sample.py` from the same shared sources
    - `BP_Door` (overlap trigger → timeline → rotate), `BP_Pickup`
    - A master material with parameters and instances, one "complex" material (a noise-driven graph that needs baking)
    - A landscape (64×64 quads, 2 paint layers), lights (directional, point, spot, rect, sky), a CineCamera
    - Static mesh simple collision, a simulated-physics stack, an AudioComponent
  - `tests/fixtures/unity/` captures:
    - per-version minimal YAML (2021.3, 2022.3, 6000.x)
    - per-pipeline `.mat` captures (Standard, URP Lit, HDRP Lit) made in throwaway batch-mode projects
    - a binary-serialized scene sample
    - `unity-sample-export/` (exporter output), `oracle/` (per-renderer world AABBs and per-light world transforms dumped by the exporter), `traces/pickup_spinner.trace.json`, reference screenshots `oracle/*.png`
  - `tests/fixtures/unreal/` captures: `sample.ixbundle/`, `oracle/`, `traces/bp_door.trace.json`, `uasset/` (a few tiny packages we authored, per supported UE version).
  - `tests/fixtures/fixtures.lock.ron`: per fixture set, the editor version, exporter source hash, generator script hash and date.
  - `just export-fixtures [unity|unreal|blender|all]` (manual, local only): reads `UNITY_EDITOR`/`UE_EDITOR` paths from the environment, runs Blender → Unity batch mode (`-batchmode -executeMethod ConcordiaExporter.Cli.ExportFixtures -quit`) → UE commandlet (`UnrealEditor-Cmd ... -run=EngineExport`), and rewrites `fixtures.lock.ron`.
- **Tests first**
  - `fixtures_lock_matches_exporter_sources`: the hash of `tools/unity-exporter/**` and `tools/unreal-exporter/**` equals the hash recorded in `fixtures.lock.ron`. It fails with "run `just export-fixtures`" when an exporter changed without regenerated fixtures.
  - `sample_projects_contain_only_licensed_files`: every file under `examples/unity-sample`, `examples/unreal-sample` and `examples/shared-sources` is either generated by a script listed in `LICENSES.md` or is an editor-generated settings/meta file. The test fails on any unlisted binary.
  - `unreal_sample_has_no_engine_or_starter_content`: no `/Engine/` or `StarterContent` packages are copied under `examples/unreal-sample/Content`.
  - `unity_sample_uses_force_text_and_visible_meta`: `ProjectSettings/EditorSettings.asset` has `m_SerializationMode: 2`, and every asset has a `.meta`.
  - `ixbundle_fixture_validates_against_schema`: `sample.ixbundle/**/*.json` validates against `interchange-v1.schema.json` (using the `jsonschema` crate).
  - `unity_export_fixture_validates_against_schema`: same check for the Unity exporter output.
  - Python (`just test-tools`, CI): `test_manifest_writer_sorts_and_versions`, `test_asset_path_to_bundle_path_is_stable`, `test_engine_content_is_skipped` (against the stub `unreal` module).
- **Implementation notes**
  - The Unreal sample's level layout comes from `build_sample.py` (text), so the scene is reproducible. Blueprint graphs and the landscape are authored once by hand, because adding Blueprint graph nodes isn't exposed to Python. The resulting small `.uasset`s are committed as our own MIT content (see open questions).
  - The Unity sample deliberately avoids Unity package assets in `Assets/`. `Packages/manifest.json` only references packages (URP), which are never vendored. `Library/` is git-ignored.
  - Keep each fixture set small (target < 15 MB total) so no Git LFS is needed.
- **Done criteria:** both samples open in their editors; `just export-fixtures all` regenerates byte-stable fixtures (running it twice gives no diff apart from the lock date); the CI schema and lock tests pass.

### Phase 5.3: Unity project reader (YAML, GUIDs, serialization detection)
- **Objective:** Read a Unity project folder into a resolved object graph: every YAML document, every `.meta` GUID, and every `{fileID, guid, type}` reference.
- **Deliverables**
  - `UnityProject::open(path)`: validates `Assets/`, `ProjectSettings/` and `ProjectVersion.txt`, and detects the serialization mode.
  - `SerializationReport`: Force Text, Mixed or Force Binary, per offending file. For non-text projects it returns a `Blocked` report entry with exact steps: *Edit → Project Settings → Editor → Asset Serialization → Mode: Force Text*, then *Version Control → Mode: Visible Meta Files*. This makes Unity reserialize every asset; it advises committing first and lists the binary files found.
  - `MetaDb`: GUID → path, importer type and settings (`ModelImporter`, `TextureImporter`, `AudioImporter`, `NativeFormatImporter`, `MonoImporter`, `DefaultImporter`, `PrefabImporter`), plus `internalIDToNameTable` (2019.3+) and legacy `fileIDToRecycleName` subobject tables.
  - Unity YAML front end: splits the multi-document stream on `--- !u!<classID> &<fileID>` headers, including the `stripped` suffix and negative/64-bit fileIDs. It honors `%YAML 1.1` and `%TAG !u! tag:unity3d.com,2011:`, then parses each body with a standard YAML parser. The output is `UnityObject { class_id, file_id, stripped, type_name, body: YamlValue }`.
  - `ClassId` registry (GameObject 1, Transform 4, Camera 20, Material 21, MeshRenderer 23, MeshFilter 33, Rigidbody 54, MeshCollider 64, BoxCollider 65, AnimationClip 74, AudioSource 82, Animator 95, RenderSettings 104, Light 108, MonoBehaviour 114, MonoScript 115, SphereCollider 135, CapsuleCollider 136, SkinnedMeshRenderer 137, CharacterController 143, Terrain 218, RectTransform 224, AnimatorController 91, PrefabInstance 1001, …). Unknown IDs keep their raw body.
  - `RefResolver`: local `{fileID: n}`, cross-file `{fileID, guid, type: 2|3}`, built-in resources (well-known built-in GUIDs, mapped to engine defaults with a report entry), and missing references (an `Approximated`/`Dropped` entry, never a panic).
- **Tests first**
  - `splits_multi_document_stream_with_tags`: a 3-object fixture yields 3 `UnityObject`s with the right class IDs and fileIDs.
  - `parses_stripped_document_header`: `--- !u!4 &123 stripped` sets `stripped = true`.
  - `parses_negative_and_64bit_file_ids`: `&-8679921383154817045` and `&9223372036854775807` round-trip as `i64`.
  - `tolerates_unity_specific_yaml_quirks`: the fixture covers the `%TAG` directive, `{fileID: 0}` inline maps, hex-encoded arrays and very long single-line arrays.
  - `yaml_fixtures_parse_for_each_supported_version`: 2021.3, 2022.3 and 6000.x captures all parse with zero errors.
  - `detects_force_text_mode`: the sample project returns `ForceText`.
  - `detects_binary_scene_and_reports_switch_instructions`: the binary fixture gives a `Blocked` entry whose message contains "Force Text" and the menu path, and `import.scan` returns it without panicking.
  - `detects_missing_meta_files_and_advises_visible_meta`: a missing `.meta` produces a `Blocked` entry with the version-control setting.
  - `meta_db_maps_guid_to_path`: every GUID in the sample resolves, and duplicate GUIDs are reported as `Blocked`.
  - `meta_db_reads_internal_id_to_name_table`: an FBX `.meta` maps internal IDs to mesh, material and animation names.
  - `meta_db_reads_legacy_file_id_to_recycle_name`: a 2018-style fixture uses the legacy table.
  - `resolves_local_and_cross_file_references`: a MeshFilter's mesh reference resolves to an FBX subobject.
  - `builtin_resource_reference_maps_to_engine_default_and_reports`: the Unity default cube or material maps to the engine default with an `Approximated` entry.
  - `missing_reference_is_reported_not_fatal`.
  - `class_id_registry_names_match_public_reference`: a snapshot of the registry table.
- **Implementation notes**
  - The YAML parser is `saphyr` or `yaml-rust2` (MIT/Apache); evaluate both on the 6000.x fixture for speed and quirk tolerance. Split documents ourselves first, because Unity's tags and `stripped` markers confuse general YAML loaders.
  - Binary detection: YAML files start with `%YAML`; binary Unity serialized files don't. Never attempt to parse binary serialization (non-goal).
  - Parse in parallel per file with the M0 job system. Memory: stream large scenes document by document.
  - References: Unity Manual "Format of Text Serialized files", "YAML Class ID Reference", "Asset Metadata"; Unity "Prefab" and "Asset Serialization" docs. Format knowledge comes from these docs and from observing our own sample files.
- **Done criteria:** the sample project and all version fixtures load into a fully resolved graph with zero unresolved references except deliberate ones, and the binary and missing-meta paths give actionable messages.

### Phase 5.4: Unity scenes, prefabs, components, materials & models
- **Objective:** Convert the resolved Unity graph into `IxDocument` scenes and prefabs, with mapped components, materials and models, verified against the editor-exported oracle.
- **Deliverables**
  - Hierarchy: GameObject → `IxNode` (name, `m_IsActive`, tag, layer), Transform/RectTransform → local TRS (RectTransform layout is reported as unmapped), and child order from `m_Children` (Unity 6 `SceneRoots` for root order).
  - **Prefabs:** a PrefabInstance (1001) resolves `m_SourcePrefab`, applies `m_Modification.m_Modifications` (`target`, `propertyPath` such as `m_LocalPosition.x` or `m_Materials.Array.data[0]`, `value`, `objectReference`), `m_RemovedComponents`, `m_RemovedGameObjects`, `m_AddedGameObjects`, `m_AddedComponents` and `m_TransformParent`.
    - Nested prefabs and variants (a prefab whose root is a PrefabInstance) become `IxPrefab { variant_of }`.
    - Stripped objects resolve through `m_CorrespondingSourceObject` and `m_PrefabInstance`. References to non-stripped instance objects use the community-observed fileID rule (`(instanceFileID XOR sourceFileID) & 0x7FFF_FFFF_FFFF_FFFF`), verified against fixtures.
    - Unrecognized property paths become report entries that name the path.
  - **Components:**
    - MeshFilter + MeshRenderer → `MeshRenderer` (submesh → material slots, shadow flags); SkinnedMeshRenderer → `SkinnedMesh` (bones, root bone, bounds).
    - Box, Sphere, Capsule (`m_Direction` 0/1/2 → axis), Mesh (convex → `ConvexMesh`, otherwise `TriMesh`) and CharacterController (capsule plus a report hint) → `Collider`, with `isTrigger`.
    - Rigidbody (mass, drag, angularDrag, useGravity, isKinematic, constraints → lock axes, collision detection → CCD) → `RigidBody`; PhysicMaterial (dynamic/static friction, bounciness, combine modes) → `IxPhysicsMaterial`.
    - Light (type, color, intensity, range, spot angles, shadows; HDRP `HDAdditionalLightData` units when present) and Camera (projection, FOV, clip planes, ortho size, clear flags; URP/HDRP additional-data components recognized by known package script GUIDs) are mapped.
    - AudioSource (clip, volume, pitch, loop, playOnAwake, spatialBlend → spatial yes/no with min/max distance and rolloff; mixer groups reported) → `AudioSource`.
    - A MonoBehaviour with `m_Script` → `Script { unit_id, field_values }`, where serialized fields are kept as typed values and object references are resolved to `IxId`s. Package-provided MonoBehaviours with no Assets source are reported as `PackageComponent`.
  - **Project settings:** `TagManager.asset` (tags and layers), `DynamicsManager.asset` (gravity, layer collision matrix), `TimeManager.asset` (fixed timestep) → `IxPhysicsSettings`.
  - **Materials:** shader identification by GUID/fileID (Built-in Standard and Standard (Specular setup), URP Lit/Simple Lit/Unlit, HDRP Lit/Unlit, with GUIDs captured in fixtures). Property mapping:
    - `_Color`/`_BaseColor`, `_MainTex`/`_BaseMap` → base color
    - `_Metallic`, `_Glossiness`/`_Smoothness`, `_GlossMapScale` → metallic and roughness factors
    - `_MetallicGlossMap` (R metallic, A smoothness) → a `ChannelMap` repack
    - HDRP `_MaskMap` (R metallic, G AO, B detail mask, A smoothness) → repack plus occlusion
    - `_BumpMap`/`_NormalMap`, `_BumpScale` → normal (OpenGL convention)
    - `_OcclusionMap`, `_EmissionMap`/`_EmissionColor` (HDR) → emissive with strength
    - `_Mode`/`_Surface`/`_AlphaClip`/`_Cutoff` → alpha mode
    - `_Cull` → double-sided
    - `_MainTex_ST` → UV transform
    - Specular setup → approximate metallic conversion (reported).
    - Unknown shaders (custom, Shader Graph, third party) → `IxFallback`: nearest PBR from common property names, plus a `NeedsAgent` entry naming the shader file. The optional exporter can add baked previews.
  - **Models:** ModelImporter settings (`globalScale`, `useFileScale`, `bakeAxisConversion`, normals/tangents import, material import mode, `importAnimation`, animation clips split by frame ranges) are applied to the FBX (ufbx) and OBJ (`tobj`) loads. Unity's model-space axis conversion is reproduced so world placement matches the oracle. Internal fileIDs map to ufbx nodes, meshes and materials by name table.
  - **Textures:** TextureImporter (`textureType` normal → linear plus normal flag, `sRGBTexture`, wrap/filter, `maxTextureSize`, alpha source); formats PNG/JPG/TGA/TIFF/EXR via `image`/`exr`, and PSD via the `psd` crate (composite only, reported).
  - Terrain from YAML when the heightmap is readable; otherwise report it and defer to the exporter (5.5).
- **Tests first**
  - `gameobject_hierarchy_matches_unity_order`: the Courtyard root and child order equals the snapshot.
  - `transform_local_trs_converted_to_engine_space`: compared with oracle world transforms within 1e-4.
  - `inactive_gameobject_imports_inactive`.
  - `prefab_instance_applies_property_modifications`: the CrateVariant color and scale overrides appear in the lowered prefab.
  - `prefab_variant_lowers_with_variant_of_base`.
  - `prefab_removed_component_is_absent_and_added_component_present`.
  - `nested_prefab_resolves_two_levels`: Pickup inside a Crate group.
  - `stripped_object_resolves_via_corresponding_source`.
  - `prefab_instance_object_file_id_follows_observed_xor_rule`: checked against every instance object in the fixtures.
  - `unknown_modification_property_path_reported`.
  - `mesh_renderer_submesh_material_slots_preserved`.
  - `skinned_mesh_bones_and_root_bone_mapped`.
  - `capsule_collider_direction_maps_axis`: all three directions.
  - `mesh_collider_convex_flag_selects_shape`.
  - `rigidbody_constraints_map_to_lock_axes`.
  - `physic_material_combine_modes_mapped_or_reported`.
  - `layer_collision_matrix_imported_symmetric`.
  - `light_types_and_spot_angles_mapped`; `urp_light_intensity_reported_approximated`; `hdrp_light_uses_physical_units_when_present`.
  - `camera_fov_and_clip_planes_mapped`.
  - `audio_source_spatial_settings_mapped`.
  - `monobehaviour_becomes_script_component_with_typed_fields`: PickupSpinner's `spinSpeed`, `bobHeight` and `scoreValue` are preserved.
  - `package_monobehaviour_reported_as_package_component`.
  - `standard_shader_material_maps_to_pbr` (snapshot); `urp_lit_material_maps_to_pbr` (snapshot); `hdrp_lit_mask_map_repacked` (snapshot).
  - `specular_setup_material_converted_and_reported`.
  - `alpha_clip_material_maps_cutoff`.
  - `emission_hdr_color_maps_strength`.
  - `unknown_shader_yields_fallback_and_needs_agent_entry`.
  - `fbx_model_import_respects_global_scale_and_use_file_scale`.
  - `imported_renderer_world_bounds_match_unity_oracle`: every renderer's world AABB is within 1 mm of `oracle/bounds.json`. This is the key correctness test for axis and scale conversion.
  - `obj_model_matches_unity_oracle_bounds`.
  - `normal_map_texture_imported_linear_and_flagged`.
  - `psd_texture_imports_composite_and_reports`.
  - `unity_import_report_has_no_silent_drops`: every unconsumed YAML field of a known class maps to a report entry or to an explicit ignore-list with a reason.
- **Implementation notes**
  - Implement component mappers as a registry `ClassId → fn(&UnityObject, &mut Ctx) -> Vec<IxComponent>`, so agents can add mappers one class at a time with a fixture each.
  - Known package script GUIDs (URP/HDRP additional data and so on) live in `src/known_scripts.ron`, captured from our fixtures, never copied from Unity packages.
  - Libraries: `ufbx` (MIT, from M2), `tobj` (MIT), `image` (MIT/Apache), `exr` (BSD-3), `psd` (MIT/Apache); all checked with `cargo-deny`.
- **Done criteria:** the Courtyard scene imports with an oracle-bounds match and every material snapshot passes; it opens and renders in the engine (manual check plus the gate goldens in 5.8).

### Phase 5.5: Unity animation, the optional editor exporter & behavior traces
- **Objective:** Best-effort Animator and clip import, plus the optional `tools/unity-exporter` package for data the YAML misses (baked data, ScriptableObjects with typed schemas, terrain, procedural meshes, oracles and behavior traces).
- **Deliverables**
  - `.controller` (AnimatorController 91, AnimatorStateMachine 1107, AnimatorState 1102, AnimatorStateTransition 1101, BlendTree 206) → `IxAnimController`; `.anim` (74) generic transform curves → `concordia-anim` clips (keyed TRS, Hermite tangents resampled at a fixed rate); FBX-embedded clips through ufbx.
  - Humanoid/muscle-space clips, Avatar masks, IK, sub-state-machine edge cases and state behaviours → `NeedsAgent`/`Dropped` entries.
  - `tools/unity-exporter` (UPM package `com.concordia.exporter`, our C#, MIT/Apache) adds these menu items and batch entry points:
    - `Export Interchange Extras`: ScriptableObjects as typed JSON with field types from reflection; terrain heightmap (16-bit PNG), splat maps and layers; lightmap/light-probe presence summary (baked data is reported, not converted, in M5); procedural or `.asset` meshes via glTFast (Apache-2.0, a declared package dependency, not vendored); and shader property dumps for custom shaders plus baked material previews.
    - `Export Oracles`: per-renderer world AABBs, light and camera world transforms, and reference screenshots from scene cameras.
    - `Record Behavior Trace`: Play Mode with `Time.captureDeltaTime = 1/60`, a scripted input file, and recording of selected objects' transforms and selected fields per frame → `*.trace.json` (`IxBehaviorTrace`).
  - `export_bundle.rs` merges exporter output into the `IxDocument` from the YAML importer. The YAML is primary; the exporter fills only declared gaps and never contradicts the YAML silently (conflicts are reported).
  - Pure C# logic in `Editor/Core` is unit-tested with EditMode tests during `just export-fixtures`. Its JSON output is schema-validated in CI (5.2).
- **Tests first**
  - `animator_controller_states_and_transitions_imported`: the Door controller has 2 states, 1 bool parameter, and transitions with conditions and duration.
  - `blend_tree_1d_imported`: a fixture.
  - `anim_clip_curves_resampled_match_unity_samples`: bobber clip poses at 5 timestamps match `oracle/anim_samples.json` within 1e-3.
  - `humanoid_clip_reported_needs_agent`.
  - `scriptable_object_from_yaml_imports_raw_fields`, and `scriptable_object_from_exporter_has_typed_fields`: LootTable entries have typed weights.
  - `terrain_from_exporter_becomes_heightfield_and_splat_material`: the Jolt heightfield collider exists, and the terrain bounds match the oracle.
  - `exporter_conflict_with_yaml_reported`.
  - `exporter_output_absent_still_imports_with_entries`: without the exporter, terrain and typed SO fields become `NeedsAgent` entries naming the exporter menu item.
  - `behavior_trace_fixture_parses_and_validates`.
  - Unity EditMode (run locally in `just export-fixtures`): `TraceRecorder_RecordsFixedStepFrames`, `JsonWriter_IsCanonical`, `OracleBounds_MatchRendererBounds`.
- **Implementation notes**
  - Lower terrain to chunked heightfield meshes (derived data), a Jolt `HeightFieldShape`, and the `terrain_splat4` material template added to `concordia-render` (a proposed addition). More than four layers is reported.
  - Animator mapping targets the M2 `concordia-anim` state machine **(refine)**.
  - The exporter uses only public UnityEditor APIs (`AssetDatabase`, `SerializedObject`, `TerrainData.GetHeights`, `EditorApplication`).
- **Done criteria:** the door animates in the engine with the oracle-sampled poses; the sample imports with and without the exporter output, with the expected report difference snapshotted.

### Phase 5.6: Unreal exporter plugin, bundle importer & experimental `.uasset` reader
- **Objective:** Our own UE editor plugin exports levels to an `.ixbundle`, and `concordia-import-unreal` turns that bundle into a playable scene. Separately, an opt-in experimental `.uasset` reader gives inventory without the plugin.
- **Deliverables**
  - **Plugin** (`tools/unreal-exporter/ConcordiaExporter`, UE 5.4+; MIT OR Apache-2.0, our code only, public APIs). Plugin dependencies are `PythonScriptPlugin`, `EditorScriptingUtilities`, `GLTFExporter`, and optionally `USDImporter`.
    - Python orchestration: walk the level's actors and components; export static and skeletal meshes, animation sequences and textures with the glTF exporter (GLB per asset; material baking on for materials the Rust rules can't map); textures as source PNG/TGA/EXR via `AssetExportTask`; SoundWaves as WAV.
    - Write `bundle.json`, `levels/<Level>.level.json` (actors with raw UE transforms, component data, and `coordinate_system: { handedness: left, up: Z, forward: X, meters_per_unit: 0.01 }`), `materials/*.json`, `blueprints/*.json` and `oracle/`.
    - Optional USD export of a level layer (`.usda`) for users who prefer USD, behind `--usd`.
    - C++ editor module, for what Python can't reach:
      - `DumpBlueprint(UBlueprint*)`: components from the Simple Construction Script, variables with types and defaults, graphs with nodes (class, member reference, pins with direction, type, default and links), and timelines with curves.
      - `DumpMaterialGraph(UMaterialInterface*)`: the expression list (class, relevant properties, input links, root property connections), material-instance parent chain and parameter overrides, and static switches.
      - Landscape heightmap and weightmap export, and a body-setup dump (simple collision boxes, spheres, capsules/sphyls and convex hulls; complex-as-simple flag).
      - `UEngineExportCommandlet` for headless `just export-fixtures`, and a PIE trace recorder (fixed 60 Hz, scripted input).
    - Content filter: `/Engine/**` and `/Script/**` assets are **never exported**. References to them are written as named references (for example `engine_ref:/Engine/BasicShapes/Cube`) and mapped to engine defaults by the importer with a report entry. Plugin content is exported only with an explicit flag.
  - **`concordia-import-unreal`:**
    - Bundle validation against the JSON Schema.
    - Actor → `IxNode`, with mapping for:
      - StaticMeshComponent, InstancedStaticMeshComponent/HISM (→ `InstancedMesh`), SkeletalMeshComponent
      - Directional, Point, Spot, Rect and Sky lights (units: lux, candela, lumens, nits; Rect is approximated)
      - CameraComponent/CineCameraComponent (filmback → FOV)
      - collision (simple shapes, complex-as-simple → `TriMesh`), simulate-physics bodies, physical materials, collision channels/profiles → layers
      - AudioComponent with a SoundWave (a SoundCue is reported, and its first wave is used if it is trivial)
    - Fog, post-process, sky atmosphere and volumetric clouds become report entries **(refine against M4 render features)**.
  - **Material graph → PBR** (`material_graph.rs`), applied as ordered rules:
    1. material-instance parameters with recognized names on a known parent pattern;
    2. root inputs traced through `Constant*`, `VectorParameter`/`ScalarParameter`, `TextureSample`/`TextureSampleParameter2D` (+ `ComponentMask` → channel), `Multiply` by constant (→ factor), `TextureCoordinate` tiling (→ UV transform), `OneMinus` (→ invert);
    3. otherwise the glTF-exporter-baked textures with an `Approximated` entry;
    4. otherwise `IxFallback` plus `NeedsAgent`.
    UE normal maps (DirectX convention) get a green-invert `ChannelMap`.
  - **Landscape:** heightmap, scale and weight layers → `IxTerrain` (the same lowering as Unity terrain).
  - **Experimental `.uasset` reader** (cargo feature `experimental-uasset`, off by default, labelled experimental in the CLI as `concordia import unreal --experimental-uasset <Content dir>`).
    - Scope: uncooked editor packages from the UE versions listed in fixtures. Package summary (magic tag, file and licensee versions, custom versions), name map, import and export tables, asset-registry tags, and tagged-property reading for simple classes (material-instance parameters, actor transforms in `.umap` where unversioned) → `import.scan` inventory, dependency graph and license evidence.
    - **Clear limits:** no mesh or texture bulk data, no cooked/IoStore/`.pak`, and no version guessing (unknown versions return `UnsupportedVersion`). It never replaces the plugin path.
    - Sources: learn only from UAssetAPI (MIT, verified) and the Rust `unreal_asset` crate from AstroTechies/unrealmodding (MIT; evaluate it as a direct dependency first), plus hex observation of our own fixtures. CUE4Parse (Apache-2.0) is also an allowed reference per `docs/clean-room.md`; anything derived from it carries NOTICE attribution (consistency-pass decision). No Unreal source is read.
- **Tests first**
  - `bundle_rejects_schema_invalid_manifest`.
  - `bundle_level_actors_imported_with_stable_ids`.
  - `ue_transforms_normalized_cm_zup_to_engine`.
  - `imported_actor_world_bounds_match_unreal_oracle`: within 1 mm (the key conversion test).
  - `instanced_static_mesh_becomes_instanced_mesh_component`.
  - `ue_light_units_converted`: per light type and unit; rect light reported as approximated.
  - `cine_camera_filmback_to_fov`.
  - `simple_collision_shapes_mapped_from_body_setup`; `complex_as_simple_becomes_trimesh`.
  - `simulated_physics_body_mapped_with_physical_material`.
  - `collision_profile_maps_to_layers`.
  - `engine_content_reference_maps_to_default_and_reports`.
  - `sound_cue_reported_needs_agent`.
  - `material_instance_parameters_map_to_pbr` (snapshot); `material_graph_constant_and_texture_inputs_traced` (snapshot); `material_graph_component_mask_selects_channel`; `material_graph_multiply_constant_becomes_factor`; `complex_material_uses_baked_textures_and_reports_approximated`; `unmappable_material_falls_back_needs_agent`.
  - `ue_normal_map_green_inverted`.
  - `landscape_heightmap_imported_as_terrain_matching_oracle_bounds`.
  - `blueprint_dump_parsed_into_graph_model`: BP_Door has its nodes, pins, links and timeline.
  - `unreal_import_report_has_no_silent_drops`.
  - Python (CI): `test_engine_paths_never_exported`, `test_level_json_is_canonical`, `test_bundle_layout`.
  - Feature `experimental-uasset`:
    - `uasset_reads_package_summary_for_supported_versions`
    - `uasset_unknown_version_returns_unsupported_error`
    - `uasset_name_map_and_exports_match_bundle_inventory`: the cross-check against the plugin's bundle for the same assets.
    - `uasset_material_instance_params_match_bundle`
    - `uasset_reader_never_panics_on_fuzzed_input` (proptest/cargo-fuzz corpus seeded with the fixtures)
    - `uasset_feature_disabled_by_default`: the default build doesn't expose the CLI flag.
- **Implementation notes**
  - The plugin stays a thin "dumper"; all mapping logic lives in Rust, so it's TDD'd in CI. The plugin's pure-Python helpers are pytest-tested against a stub `unreal` module.
  - Exporter workflow for users: install the plugin into `<Project>/Plugins/`, then *Tools → Export to Engine*, or run the commandlet. The user's own Unreal editor does the export.
  - glTF over USD as the default asset carrier, because `concordia-import-gltf` is mature (M1) and the glTF exporter bakes complex materials. USD level layers are optional until an engine USD reader exists (open question).
  - References: Unreal docs for the glTF Exporter, USD Exporter, Python Editor Scripting, Commandlets, Landscape heightmap import/export, and Physical Lighting Units; UAssetAPI docs and source (MIT); `unreal_asset` (MIT).
- **Done criteria:** the sample bundle imports and renders with oracle bounds matching; the plugin's Python tests pass in CI; the experimental reader passes its tests behind the flag, and a CI job builds both with and without the feature.

### Phase 5.7: Logic translation tooling for agents
- **Objective:** Give the user's agents everything needed to translate C# and Blueprints into Rust, WASM or `.graph.ron` and prove the translation with tests, without the engine ever calling an LLM.
- **Deliverables**
  - `concordia-translate` crate (extracting the shared parts of M2's GDScript support **(refine)**):
    - `TranslationUnit { id, language, source_path, symbols, dependencies, usages (scene instances with field values), target: Rust | Wasm | Graph, status: Untranslated | InProgress | Translated | Verified, verification }`, persisted as `port/units/<id>.unit.ron` (text, diffable). Units can be mirrored to the Studio task board (`task.create`).
    - **C# front end:** `tree-sitter` with `tree-sitter-c-sharp` (MIT) → our stable, simplified `CsAst` JSON (types, members, statements, expressions, attributes) and a symbol summary:
      - classes deriving `MonoBehaviour`/`ScriptableObject`
      - serialized fields (`public`, `[SerializeField]`)
      - Unity messages (Awake, OnEnable, Start, Update, FixedUpdate, LateUpdate, OnDisable, OnDestroy, OnTriggerEnter/Exit, OnCollisionEnter/Exit and so on)
      - coroutines
      - Unity API call sites resolved through `using` directives
    - **Blueprint model:** from the 5.6 dump: events, functions, variables, the component tree, timelines, and nodes keyed by member reference.
    - **API map tables** (`maps/unity-api.map.ron`, `maps/unreal-k2.map.ron`; text, reviewable): `{ source, kind, wit_target, rust_target, graph_node, semantics, coordinate_note, confidence }`. Examples: `UnityEngine.Transform.position` → `concordia:game/transform.get-position` (note: space is already normalized by the importer); `Time.deltaTime` → the fixed-step dt; `KismetMathLibrary::Add_VectorVector` → `math.add_vec3`; `Actor::K2_SetActorLocation` → `transform.set_position`.
    - **Mechanical Blueprint skeleton:** for mapped nodes, a deterministic transliteration to `.graph.ron`. Unmapped nodes become `port.unmapped` placeholder nodes carrying the original node JSON, and the graph compiler reports them as errors until an agent replaces them. This is structure conversion, not logic conversion.
    - **Scaffold generator:**
      - For a C# unit targeting WASM: a `cargo component` guest crate skeleton with a component struct for the serialized fields (typed), lifecycle hook signatures wired to `concordia:game` exports with empty bodies, and a `README` listing the mapped API calls.
      - For any unit with a behavior trace: a replay test (`tests/replays/<unit>.replay.ron` + `tests/translation/<unit>.rs`) that replays the trace's input log and asserts the sampled values within the trace's tolerances via `assert.eval`.
    - **Trace converter:** `IxBehaviorTrace` → `.replay.ron` (input log at fixed step) plus an assertion list (frame, entity `IxId` → engine id, property, expected, tolerance).
  - **Engine API / MCP** (every input schema is reflected):
    - `translate.list` (units with status, filters)
    - `translate.inspect` (structure summary, or the AST of one symbol or method, paged to keep agent context small)
    - `translate.map` (look up source API symbols → targets, including "no mapping, see notes")
    - `translate.scaffold` (generate the target skeleton and tests; idempotent)
    - `translate.verify` (build and run the unit's tests and replay; returns pass/fail with per-assertion diffs)
    - `translate.mark` (set status with actor)
    - MCP tools `translate_list`, `translate_inspect`, `translate_map`, `translate_scaffold`, `translate_verify`, `translate_mark`
    - CLI `concordia translate <verb>`
  - **Skills** shipped in `studio/skills/` and compiled by `studio sync`: `port-unity-script`, `port-blueprint` (inspect → map → scaffold → implement → verify → mark, with the rule "never edit generated assertions to pass") and `review-import-report` (triage `NeedsAgent` entries into tasks).
  - The committed translations used by the gate: `examples/unity-sample-port/pickup_spinner/` (Rust → WASM component, translated by an agent through these tools and committed), and `examples/unreal-sample-port/bp_door.graph.ron`.
- **Tests first**
  - `csharp_monobehaviour_symbols_extracted`: PickupSpinner yields the class, 3 serialized fields with types, Update and OnTriggerEnter.
  - `csharp_ast_json_matches_snapshot`.
  - `csharp_unity_api_calls_resolved_via_usings`.
  - `csharp_coroutine_detected_and_flagged`.
  - `csharp_parse_errors_reported_not_fatal`.
  - `unity_api_map_targets_resolve_in_wit`: every `wit_target` exists in `wit/` world `concordia:game/gameplay`, so the map can't rot.
  - `k2_map_targets_resolve_in_graph_node_registry`: every `graph_node` exists in the `concordia-graph` registry.
  - `blueprint_skeleton_transliterates_mapped_nodes`: BP_Door's mapped nodes appear in the snapshot `.graph.ron`.
  - `unmapped_k2_node_becomes_placeholder_with_source_payload`.
  - `graph_with_placeholders_fails_compile_listing_them`.
  - `scaffold_wasm_guest_matches_snapshot`; `scaffold_is_idempotent`.
  - `trace_converts_to_replay_and_assertions_snapshot`.
  - `trace_entity_ids_map_through_import_ids`.
  - `translate_verify_reports_per_assertion_diffs`: a deliberately wrong translation fixture fails with a readable diff.
  - `translate_mark_records_actor_in_command_log`.
  - `translate_tools_need_no_api_key`: the MCP server runs every `translate_*` tool with the environment cleared of `*_API_KEY` variables.
  - `studio_sync_emits_porting_skills`: a snapshot of `.claude/skills/port-*`.
  - `committed_pickup_spinner_translation_passes_replay` (also asserted in the gate).
  - `committed_bp_door_graph_passes_replay`: BP_Door's trace replay (door yaw at sampled frames ± 0.5°).
- **Implementation notes**
  - Keep the AST JSON our own and stable (versioned with the interchange schema), not raw tree-sitter output, so agent skills and snapshots don't break when the grammar updates.
  - Traces from different physics engines diverge, so the sample behaviors are kinematic or trigger-driven, and assertions use tolerances and sampled frames (for example every 10th frame plus event frames), not every frame.
  - Coordinate normalization happens at import, so translated code works in engine space. The map's `coordinate_note` warns where source code hardcodes axes (for example `Vector3.forward`).
  - References: tree-sitter docs; the WIT/component model spec; Unity Scripting API docs (public) for semantics; Unreal Blueprint API reference (public docs) for K2 node semantics.
- **Done criteria:** an agent session in Claude Code (subscription, no key) uses only `translate_*`/`import_*` MCP tools and file edits to produce and verify the PickupSpinner translation; that translation and BP_Door's are committed with passing replays.

### Phase 5.8: License surfacing report & gate assembly
- **Objective:** Tell users where imported content probably came from and what license questions to check (never deciding), and assemble the executable M5 gate.
- **Deliverables**
  - `LicenseReport { schema_version, entries: Vec<LicenseEntry { asset (path/IxId), origin: UserDeclared | UnityAssetStore { package, publisher } | UnityPackage { name, version, license_file } | FabOrMarketplace { hint } | EpicSampleOrStarter | UnrealEngineContent | Unknown, evidence: Vec<Evidence { kind, detail, confidence: High | Medium | Low }>, implications: Vec<ImplicationRef> }> }`. It is written as `license-report.ron` plus a human-readable `license-report.md`, and is available through `import.licenses` / MCP `import_licenses`.
  - **Evidence sources (Unity):**
    - GUID match against the user's local Asset Store cache (`.unitypackage` archives under the platform's Asset Store cache folder; read-only, opt-in with `--scan-asset-store-cache`). Package GUIDs are preserved on import, so this is High confidence.
    - `Packages/manifest.json`/`packages-lock.json` (registry, git or local, plus license files in package folders when present).
    - Publisher-style folder names and LICENSE/README/"Third-Party Notices" files (Low/Medium).
  - **Evidence sources (Unreal):**
    - Package paths and asset-registry tags exported by the plugin, or read by the experimental reader.
    - `AssetImportData` source paths.
    - Megascans metadata.
    - Known Epic sample/starter path patterns.
    - `/Engine` references (always `UnrealEngineContent`: never exported, mapped to engine defaults).
    - Matches against the local Fab/Vault cache when the user opts in.
  - **User declarations:** `<project>/import/licenses.toml` maps paths or packages to the license the user states they hold. These are shown as `UserDeclared` and never validated.
  - **Implications table** (`crates/concordia-interchange/data/license-implications.ron`): neutral, link-backed questions per origin family, for example: "Fab Standard License content is generally usable across engines; content marked UE-Only, legacy Marketplace-licensed content and Epic sample content may be restricted to Unreal Engine. Check the listing and your Fab library." Every entry states that the report is not legal advice. **No field anywhere expresses allowed or forbidden.**
  - CLI/editor: `concordia import` prints a license summary and the report path. Import never blocks on license data; an optional `--require-license-ack` pauses until the user acknowledges.
  - Gate wiring: `tests/gates/m5_*.rs`, `tests/golden/m5/*.png`, `just gate M5`.
- **Tests first**
  - `asset_store_guid_match_is_high_confidence`: uses the synthetic cache fixture built from our own content.
  - `folder_heuristic_is_low_confidence`.
  - `upm_registry_package_listed_with_license_file`.
  - `user_declaration_overrides_display_but_keeps_evidence`.
  - `engine_content_reference_always_listed_as_unreal_engine_content`.
  - `epic_sample_path_pattern_flagged`.
  - `fab_cache_match_requires_opt_in`: no cache scan without the flag.
  - `license_report_schema_has_no_verdict_fields`: the reflected schema contains no field named or typed as a verdict (`allowed`, `permitted`, `legal`, `forbidden`, `ok`), and implications are questions or links only.
  - `license_report_matches_snapshot` (each sample).
  - `import_never_blocks_on_license_without_ack_flag`.
  - `cache_scan_is_read_only`: cache folder mtimes and hashes are unchanged.
- **Implementation notes**
  - `.unitypackage` is a gzip tar of `<guid>/{asset, asset.meta, pathname}`; read it with `flate2` and `tar` (MIT/Apache) and match on GUID plus content hash.
  - Cache paths differ by OS; keep them in config with defaults documented, and treat them as user data that is never uploaded or logged beyond the local report.
  - The license data is local only and never sent anywhere.
- **Done criteria:** both samples (with synthetic evidence fixtures) produce snapshot-matching reports, and the gate passes in CI on the macOS runner.

## Milestone gate
**Command:** `just gate M5`. It runs `cargo nextest run --profile gate -E 'binary(/^m5_/)'` over `tests/gates/m5_*.rs` on the macOS (Metal) CI runner. No Unity or Unreal editor is needed: inputs are `examples/unity-sample/` (YAML project), `tests/fixtures/unity/unity-sample-export/`, `tests/fixtures/unreal/sample.ixbundle/` and the committed translations.

**What it asserts:**
1. `m5_unity_import.rs`
   - `unity_sample_imports_into_playable_scene`: through the Engine API (`import.run`, actor `agent:gate`) into a temp project, then `scene.open` Courtyard. The scene has an active camera, mapped renderers, colliders, bodies, lights and audio, and the PickupSpinner WASM component is attached. `sim.step` runs 600 fixed ticks (10 s at 60 Hz) with zero `error` entries in `log.tail`, no panics, all body transforms finite and above the kill plane, and identical world-state hashes across two runs.
   - `unity_sample_matches_golden_screenshots`: `capture.screenshot` from both scene cameras at tick 120, compared with `tests/golden/m5/unity_courtyard_{main,overview}.png` (FLIP mean ≤ 0.05, per-test tolerance file).
   - `unity_sample_unmapped_report_matches_snapshot`: `import-report.ron` equals the insta snapshot (including the deliberate custom-shader and custom-component entries).
   - `unity_import_is_deterministic`: two imports produce byte-identical output trees.
2. `m5_unreal_import.rs`: the same four assertions for `sample.ixbundle` (`unreal_courtyard_{main,overview}.png`), with BP_Door's committed `.graph.ron` compiled and attached.
3. `m5_translation.rs`: `translated_pickup_spinner_passes_replay_assertion` replays `tests/replays/m5/pickup_spinner.replay.ron` (generated from the Unity trace) in the imported scene. Sampled bob height and yaw match the Unity trace within tolerance, the pickup deactivates within ±2 frames of the traced frame, and the score equals 1.
4. `m5_licenses.rs`: `license_reports_match_snapshots_without_verdicts` covers both samples.

The gate passes only if all of these pass in CI. Goldens are refreshed with `just bless` only by deliberate review. Fidelity against the source editors is checked separately by the non-gating oracle tests: bounds within 1 mm (gating, in 5.4/5.6) and FLIP against the editor reference screenshots (logged only, for tracking).

## Risks & mitigations
| Risk | Mitigation |
|---|---|
| Unity YAML and prefab semantics vary by version (2021 → Unity 6: `SceneRoots`, removed/added GameObjects in modifications) | Per-version fixtures; a tolerant splitter; unknowns become report entries; the class-ID registry is data-driven |
| Axis and scale conversion errors (FBX model axes, Unity's model-space flip, UE cm/Z-up) | Oracle bounds exported by our own editor tooling are the ground truth, at 1 mm tolerance; basis-change property tests |
| Material fidelity is poor for custom shaders and complex UE graphs | Layered rules → baked textures → fallback, all reported with agent hints; goldens catch regressions |
| CI can't run the editors, so fixtures go stale | `fixtures.lock.ron` hash check fails CI when exporter sources change; `just export-fixtures` is documented and deterministic |
| Unreal plugin APIs change between UE versions | A thin C++ module, Python for the rest; a supported-version matrix; commandlet smoke run in `just export-fixtures` per installed version |
| Behavior traces diverge across physics engines | Kinematic/trigger-driven sample behaviors; sampled frames with tolerances; event-frame windows |
| `.uasset` format churn and legal ambiguity | Feature flag off by default; supported versions explicit; fuzzing; learn only from MIT sources; legal review before promotion |
| License detection false positives or negatives | Confidence levels, evidence listed, user declarations, and an explicit no-verdict schema test |
| Scope is large | Phases are independently shippable: Unity (5.3–5.5) can ship before Unreal (5.6); the uasset reader is optional |

## Open questions
1. **Unreal plugin distribution terms.** Our plugin is our own code against public APIs and is distributed as source under MIT OR Apache-2.0. We believe the UE EULA permits distributing such source plugins (many MIT plugins exist publicly), but we have not confirmed which EULA clauses govern plugins that link against engine modules, nor the separate terms for distribution on Fab. **Needs review before release** (as `docs/clean-room.md` already requires). Until then, distribute source only, no prebuilt binaries.
2. Committing the small `.uasset`/`.umap` files we authored for the Unreal sample and the uasset fixtures: we believe content we create is ours to license MIT, but this needs confirmation alongside question 1. The fallback is to commit only `build_sample.py` and the exported bundle, and keep uasset fixtures local (with those tests `#[ignore]` unless `CONCORDIA_UASSET_FIXTURES` is set).
3. Is the `.uasset` reader legally and ethically fine to ship by default eventually (reading the user's own files for interoperability)? It stays experimental until reviewed.
4. **CUE4Parse is Apache-2.0, not MIT.** Apache-2.0 is allowed by `docs/clean-room.md` and our licenses, but this milestone's brief limits uasset learning to MIT readers. **Resolved in the consistency pass:** CUE4Parse may serve as a reference, with NOTICE attribution for anything derived.
5. Unreal 6 support: target it once its plugin APIs are public; also decide the oldest supported UE 5 version (proposed 5.4).
6. Does the engine have a USD reader by M5 (for example the `openusd` Rust crate)? If not, USD level layers stay optional and glTF plus JSON is the only required path.
7. Does M0's prefab override model support property-path overrides on nested prefab children and removed components? If not, extend `concordia-scene` in 5.1 **(refine)**.
8. Engine coordinate convention and light units: confirm M0/M1 fixed right-handed +Y-up meters and physical light units.
9. Should a dedicated Studio role (`porter`) exist, or are the porting skills on the Gameplay Programmer role enough?
10. Should humanoid (muscle-space) retargeting be in M5, or later?

## ADRs to write
- ADR: Interchange schema v1, versioning and migration policy (JSON wire format and RON in-repo, generated JSON Schema for external exporters).
- ADR: Deterministic ids for imported entities (name-based UUID from the source id plus a project namespace, alongside UUIDv7 for natively created entities).
- ADR: Unreal exporter plugin (glTF-first bundle, Rust-side mapping, never exporting `/Engine` content, distribution and licensing terms).
- ADR: Experimental `.uasset` reader policy (scope, flag, allowed sources, review gate).
- ADR: Logic translation workflow (`concordia-translate`, translation units, API maps, trace-derived replay tests; the engine never calls an LLM).
- ADR: License surfacing policy (inform, never decide; no-verdict schema; local-only data).
- ADR: Sample projects and fixture regeneration (authored-by-us content, `fixtures.lock.ron`, `just export-fixtures`).

## Effort estimate
| Phase | Estimate |
|---|---|
| 5.1 Interchange v1 | 2 weeks |
| 5.2 Samples & fixture pipeline | 1.5 weeks (needs local Unity/UE installs) |
| 5.3 Unity reader | 1.5 weeks |
| 5.4 Unity scenes/prefabs/components/materials/models | 3 weeks |
| 5.5 Unity animation, exporter, traces | 2 weeks |
| 5.6 Unreal plugin + importer (+1 week uasset reader) | 4.5 weeks |
| 5.7 Translation tooling | 3 weeks |
| 5.8 License report + gate | 1.5 weeks |
| **Total** | **~19 weeks** (16–24), author plus agents; 5.3–5.5 and 5.6 can run in parallel after 5.2 |

## Proposed convention additions
- **Crate:** `concordia-translate` (M5, extracted from M2 GDScript support): translation units, C#/Blueprint front ends, API map tables, scaffolds, trace → replay conversion. It is used by `concordia-import-godot`, `-unity` and `-unreal`.
- **Engine API / MCP:** `import.scan`, `import.run`, `import.report`, `import.licenses` (MCP `import_scan`, `import_run`, `import_report`, `import_licenses`); `translate.list`, `translate.inspect`, `translate.map`, `translate.scaffold`, `translate.verify`, `translate.mark` (MCP `translate_*`). `asset.import` stays for single assets.
- **CLI:** `concordia import unity <dir>`, `concordia import unreal <bundle> [--experimental-uasset]`, `concordia translate <verb>`.
- **Files and extensions:**
  - `.ixbundle/` (exporter bundle directory: `bundle.json`, `levels/*.level.json`, `materials/*.json`, `blueprints/*.json`, `oracle/`, assets)
  - `*.trace.json` (behavior traces from source editors)
  - `import-report.ron`, `license-report.ron` + `license-report.md`
  - `port/units/<id>.unit.ron` (translation units)
  - `<project>/import/licenses.toml` (user license declarations)
  - `crates/concordia-interchange/schema/interchange-v1.schema.json`
  - `tests/fixtures/fixtures.lock.ron`
  - `crates/concordia-translate/maps/*.map.ron`
- **Ids:** imported entities use name-based (UUIDv5-style) ids derived from the source `IxId` and a project import namespace; this is an exception to "UUIDv7 everywhere" that keeps re-import deterministic.
- **just recipes:** `just export-fixtures [unity|unreal|blender|all]` (manual, local editors), `just test-tools` (pytest for `tools/unreal-exporter`).
- **Cargo feature:** `experimental-uasset` on `concordia-import-unreal`.
- **Render:** a `terrain_splat4` built-in material template in `concordia-render` for imported terrains and landscapes.
- **Unity package id:** `com.concordia.exporter`; **Unreal plugin name:** `ConcordiaExporter`. Both are renamed with the engine (ADR-0008).
- **Examples:** `examples/shared-sources/`, `examples/unity-sample/`, `examples/unreal-sample/`, `examples/unity-sample-port/`, `examples/unreal-sample-port/`.
