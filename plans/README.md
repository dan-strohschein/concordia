# Milestone Plans

The top-level design is in [`../docs/design.md`](../docs/design.md). This folder holds one **implementation plan per milestone**. Executing a milestone plan produces code, test-first, one phase at a time.

> The engine is **Concordia** (ADR-0008). Crates use the prefix `concordia-`, the CLI binary is `concordia`, and the MCP server is `concordia`.

## Index

| Milestone | Plan | Depends on | Detail | Status |
|---|---|---|---|---|
| M-1 Toolchain & scaffold | [M-1-toolchain-and-scaffold.md](M-1-toolchain-and-scaffold.md) | none | Ready to execute | Gate passed |
| M0 Headless core | [M0-headless-core.md](M0-headless-core.md) | M-1 | Ready to execute | Gate passed |
| M1 See it | [M1-see-it.md](M1-see-it.md) | M0 | Ready to execute | Gate passed (2026-11-19) |
| M2 Engine breadth (was Port begins; ADR-0048) | [M2-port-begins.md](M2-port-begins.md) | M1 | Refined (ADR-0067, approved 2026-11-19) | **Gate passed (2026-11-23)**: the author accepted M2 by playing the sample game Lamplight; the scripted CI gate (2.10d), deferred to M3, landed in M3 3.9a as `just gate-tests M2` |
| M3 Closed loop & Studio v1 | [M3-closed-loop-and-studio.md](M3-closed-loop-and-studio.md) | M2 (Studio phases may start after M0) | Refined after M2 (2026-11-23: R1 two gates, R2 replay stills without restore) | **Gate passed, scripted and live (2026-11-24)**: every phase is done; `just gate-tests M3` is green, and `just gate-live M3` passed on its first attempt. the `regression` workflow is green on `main` (2026-11-24). It closes once the author has rechecked the windowed items; Studio phases 3.3–3.6, 3.8 run right after M1 (ADR-0049) |
| M4 Unreal-class visuals | [M4-unreal-class-visuals.md](M4-unreal-class-visuals.md) | M1 (render), M2 (graphs) | Refined after M3 (2026-11-24: R1 Lamplight enemy, R2 rendering first, R3 Vulkan in M4, R4 `ash` unsafe fenced like Metal, R5 `meshopt-sys`; D1–D10 approved) | Done (2026-11-27; end-of-milestone checks passed, PR #150; accepted deferrals: Vulkan acceleration structures → M9, meshlet LODs and perf budgets → M7, the default flips → with M9) |
| M5 Bring your game | [M5-bring-your-game.md](M5-bring-your-game.md) | M2 (importer framework) | Needs refinement after M4 | Planned |
| M6 Evolve (plugins) | [M6-evolve-plugins.md](M6-evolve-plugins.md) | M2 (parallel with M3) | Needs refinement after M2 | Planned |
| M7 Terra (large-scale simulation) | [M7-terra.md](M7-terra.md) | M0 (Phase 7.0 rename, done); M1, M2, M4 (simulation features) | Needs refinement after M4 | In progress (Phase 7.0 done; rest after M4) |
| M8 Advanced physics (destruction, GPU garments; ADR-0012) | [M8-advanced-physics.md](M8-advanced-physics.md) | M1 (destruction); M2 (garments) | Refined after M4 (2026-11-27: Q1–Q8 approved) | In progress (Phase 8.0 groundwork done; Phase 8.1 fracture authoring done: Manifold cutter, five patterns, hulls, `.breakable.ron`, `fracture.generate`; 8.2 the connection graph and cluster levels; 8.3 breaking at runtime on Jolt; 8.4a fields) |
| M9 Advanced lighting (Photon GI; ADR-0013) | [M9-advanced-lighting.md](M9-advanced-lighting.md) | M4 (ray tracing, HZB, VSM, TAA, path tracer, `gi::Technique`), M2 2.3 (materials) | Outline; needs refinement after M4 | Planned |
| M10 Advanced shadows (tiled shadow atlas; ADR-0014) | [M10-advanced-shadows.md](M10-advanced-shadows.md) | M1 (render graph, CSM, clustered lights, `render.stats`); M2 (skinned meshes, port content for measurement); M4 4.1 and 4.3 reused if they land first | Outline; needs refinement after M2 | Planned |
| M11 Advanced particles (Sparx; ADR-0015) | [M11-sparx-particles.md](M11-sparx-particles.md) | M1 (RHI compute, render graph, `ShaderLibrary`, asset hot reload, `render.stats`); M2 (signals, materials 2.3b, scripts, replay); M3 Studio roster; M10 10.7 or M4 4.3 indirect draws reused if they land first | Outline; needs refinement after M2 | Planned |
| M12 Rigging v1.0 (procedural rigs, in-engine keyframing; ADR-0016) | [M12-rigging.md](M12-rigging.md) | M2 (animation 2.4, phases and presentation 2.0, FBX 2.3d, scripts 2.6, replay 2.8); M1 (gizmos, overlays, inspector, goldens); M3 Studio roster; M4 4.10 document transactions reused if they land first | Outline; needs refinement after M2 2.4 | Planned |
| M13 Advanced Godot features (the AnimationTree features 2.4.3 left out; ADR-0017) | [M13-advanced-godot-features.md](M13-advanced-godot-features.md) | M2 (animation graphs 2.4.3, `anim.*` tools 2.4.4, the animator 2.4.5, `SimRng` and replay 2.8); before M5's AnimationTree import (recommended) | Outline; needs refinement after M2 2.4 | Planned |
| M14 CPU-only games (author decision, 2026-11-24; deliberately the last milestone) | [M14-cpu-only.md](M14-cpu-only.md) | M4 (4.1b2 CPU path-tracing core, 4.1v Vulkan); all rendering milestones | Outline; needs refinement when it starts | Planned |
| Later | [later-backlog.md](later-backlog.md) | none | Backlog | n/a |

Status values: `Planned` → `Refined` → `In progress (phase N)` → `Gate passed`.

---

## Milestone plan template

Every milestone plan uses these sections, in this order:

```
# M<n>: <Title>
Status · Depends on · Detail level · Estimated effort

## Goal & context
## Scope / Non-goals
## Dependencies (what must already exist, from which milestone)
## Crates, modules & directories (created / touched)
## Phases
### Phase <n>.<k>: <name>
- Objective
- Deliverables
- Tests first        (named tests with the behavior each asserts; written and failing BEFORE implementation)
- Implementation notes (key types, APIs, algorithms, libraries, papers/talks)
- Done criteria
## Milestone gate (the executable acceptance test: exact command and what it asserts)
## Risks & mitigations
## Open questions
## ADRs to write
## Effort estimate
```

Phase numbering: `M0` phases are `0.1`, `0.2`, …; `M-1` phases are `-1.1`, `-1.2`, ….

---

## Shared conventions (all plans MUST use these names)

### Repository layout (project root = this folder's parent)
```
Cargo.toml            # workspace
rust-toolchain.toml   # pinned stable channel
justfile              # task runner entry points: just test, just lint, just gate M0, ...
CLAUDE.md, AGENTS.md
docs/                 # design.md, clean-room.md, adr/NNNN-title.md, guides
plans/                # this folder
crates/<crate-name>/  # every Rust crate; each has src/, tests/, AGENTS.md, README.md
wit/                  # WIT packages for the WASM gameplay API (package: concordia:game@0.1.0)
editor/               # Tauri v2 app: editor/ui (TypeScript + React + Vite, pnpm), editor/src-tauri
tests/                # cross-crate integration, gates/, golden/, replays/, fixtures/
tools/                # dev scripts, exporter plugins (tools/unreal-exporter, tools/unity-exporter)
third_party/          # vendored C/C++ sources (Jolt, Recast) with LICENSE files
examples/             # small sample projects used by tests and docs
```

### Canonical crate names (`crates/`)
| Crate | Purpose | First milestone |
|---|---|---|
| `concordia-core` | math (glam re-export), ids (`EntityId` UUIDv7), time, tracing/logging, job system | M0 |
| `concordia-reflect` (+ `concordia-reflect-derive`) | type registry, `#[derive(Reflect)]`, JSON Schema export, doc strings | M0 |
| `concordia-terra` | **Terra**, the entity system (ADR-0010): own archetype ECS, queries, systems, schedules, fixed-step deterministic loop; renamed from `concordia-ecs` in M7 Phase 7.0 and grown into large-scale simulation in M7 | M0 |
| `concordia-command` | command log, transactions, undo/redo, `Actor` attribution | M0 |
| `concordia-scene` | text scene/prefab format: read, write, canonical formatting, stable IDs | M0 |
| `concordia-api` | Engine API: typed request/response types, JSON-RPC 2.0 server (stdio + WebSocket), sessions | M0 |
| `concordia-mcp` | MCP server binary, a thin adapter over `concordia-api` | M0 |
| `concordia-cli` | the `concordia` binary: `concordia new/run/headless/test/import/studio ...` | M0 |
| `concordia-platform` | windowing and OS integration (winit), file watching | M1 |
| `concordia-input` | input actions, bindings, input injection API; the viewport as input and `ConsumedActions` (M2 2.7b.2); typed text (M2 2.7b.3); `InputActions` for scripts (M2 2.10a) | M1 |
| `concordia-rhi` | RHI traits and types (indirect draws, viewport and scissor, texture-to-texture copies and the shared `check`s since M4 4.3a); backends `concordia-rhi-metal` (M1), `concordia-rhi-vulkan` (M4 4.1v, ADR-0088; `CONCORDIA_REQUIRE_VULKAN=1` requires it) | M1 |
| `concordia-rhi-select` | The one place that turns a backend choice into a device or probe: `CONCORDIA_BACKEND` (`auto`, `metal`, `vulkan`; empty is unset) beats the project's `[render] backend`, which beats `auto` (Metal on macOS, Vulkan elsewhere). Vulkan validation for devices the engine opens: `CONCORDIA_VULKAN_VALIDATION` is `off` (default), `auto`, `on` or `strict` (strict: any validation error fails the next call) | M4 4.1v(b) |
| `concordia-shaders` | Slang compilation, reflection, shader cache | M1 |
| `concordia-render` | render graph, PBR clustered forward+, materials, cameras, capture; GPU skinning of posed characters (M2 2.4.2); the compute + indirect meshlet path into Forward+ and the cascades, `[render] meshlets` (on by default), `render.stats` `raster_path` and `meshlets`, the `meshlets` and `triangles` view modes (M4 4.3a, ADR-0100); the mesh-shader route where `RhiCaps` allows, `raster_path` `mesh_shader` and `meshlets.mesh_draws` (4.3b); motion vectors on every path, `Renderer::read_motion`, the `motion` view mode (4.3b2); GPU frustum and normal-cone culling of meshlet-path instances and meshlets on both routes and per shadow cascade, the deterministic GPU prefix sum (`scan`), `[render] culling`, `render.stats` `meshlets` per cull stage, `Renderer::read_culling` (4.4a); temporal anti-aliasing and upscaling: jitter, mip bias, per-view history, the own TAA/TAAU, `[render] temporal` and `upscale`, `render.set_temporal`, `render.stats` `frame.temporal` (4.6a, ADR-0103); the `metalfx` backend through the RHI's `TemporalScale` (4.6b); virtual shadow maps for the sun, `[render] shadows` and `[render.vsm]`, `render.set_shadows`, `render.stats` `frame.shadows`, the `vsm_pages`, `vsm_levels`, `vsm_cache` and `shadow_mask` view modes, the CSM fallback (4.7a, ADR-0091) | M1 |
| `concordia-render-types` | the render vocabulary shared by the renderer and the path tracer: reflected render components, camera math, photometric units, CPU tone mapping; no GPU dependencies (ADR-0101) | M4 (4.1b1) |
| `concordia-material` | Substrate-style material evaluation: closure programs, the closure budget, the CPU reference BSDF and albedo tables (ADR-0068); the model, `.material.ron` and `material.get/set` live in `concordia-assets` (2.3c); `concordia-render`'s shader mirrors it (2.3b) | M2 (2.3a) |
| `concordia-assets` | asset database, importers registry, content-addressed derived-data cache (`.concordia/ddc`), hot reload; layered materials (2.3) and animation graphs (`.animgraph.ron`, 2.4.3) as canonical text; importers' `DerivedCache` (sub-results such as meshlets, keyed by their own inputs), the shared import job pool and `asset.meshlet_stats` (M4 4.2) | M1 |
| `concordia-import-gltf` | glTF 2.0 importer | M1 |
| `concordia-import-model` | format-neutral model import shared by the glTF and FBX importers: sub-asset names, the prefab from a node tree, mesh finishing, texture color spaces, skin weights reduced to four, the `meshlets` setting and meshlet building per mesh (M4 4.2) (M5's `concordia-interchange` may build on it) | M2 (2.3d) |
| `concordia-physics` (+ `jolt-sys`) | Jolt via its C API, safe wrapper, ECS integration; characters' inner bodies for sensors (M2 2.10a) | M1 |
| `concordia-audio` | WAV (hound) and Ogg Vorbis (lewton) decoding, the `sound` and `cue` importers (`AssetKind::Sound`, `Cue`), `.cue.ron` cues with seeded variation and `.buses.ron` layouts, an own 48 kHz mixer (buses, voices, stealing, streaming, equal-power spatialization) over `rtrb` rings that never allocates in the callback, and the offline backend with bit-identical renders (2.7a.1, ADR-0066); the `[audio]` module (`AudioEmitter`, `AudioListener`, `AnimEventCues`, the `PlayCue` / `StopCue` signals, the `audio.collect` presentation processor, `audio.*` methods, the `audio` debug layer) and the `cpal` device for `concordia run` (2.7a.2) | M2 (2.7a) |
| `concordia-anim` | skeletons from imported skins, one hashed `Pose` per character, clips bound by bone name and sampled per glTF, `AnimPlayer`s in `FrameEnd`, the `[anim]` module (2.4.1, ADR-0074); the CPU reference for skinning, palettes and skinned bounds (2.4.2; the renderer skins on the GPU); blend graphs and state machines (`AnimGraphPlayer`, `AnimGraphState`, 2.4.3); clip events as signals, `BoneAttachment`, property tweens and `anim.inspect` (2.4.4); the animator role and the original fixture character (2.4.5); the `.clip.ron` text model, curves, keyframe methods and the `PoseProcessor` seam (M12) | M2 (2.4.1) |
| `concordia-nav` (+ `recast-sys`) | navmesh build from static colliders and marks, queries, cache, `nav.*`, the `navmesh` debug layer (2.5a, ADR-0077); `NavAgent` path following, `NavArrived` / `NavPathFailed`, the `nav_paths` layer (2.5b.1); `ClickToMove` and `nav.click` (2.5b.2a); windowed play in `concordia run` (2.5b.2b, through `concordia_app::Player`); `CrowdAgent` and crowd agents' corridors in `nav.plan` (2.5c.1); the boundary queries and a query pool sharded by thread (2.5c.2) | M2 |
| `concordia-script-protocol` | the script protocol's types: module, processor and trait descriptors, batches, run outputs (ADR-0062) | M2 (2.6b) |
| `concordia-script` | scripting host seam: `ScriptBackend`, `ScriptHost`, `ScriptProcessor`, the conformance suite, the in-process test backend (ADR-0062) | M2 (2.6b) |
| `concordia-script-abi` | the native module C ABI: function table, symbols and guest-side glue (ADR-0061) | M2 (2.6c) |
| `concordia-scripting` | the scripts engine module: `[scripts]` loading, play processors, `script.reload` with edit-world migration (ADR-0061) | M2 (2.6d) |
| `concordia-script-wasm` | Wasmtime component host for `wit/` world `concordia:game/gameplay` | M2 |
| `concordia-import-godot` | Godot 4 `.tscn`/`.tres`/`project.godot` importer, GDScript→Rust translation support | M5 (moved from M2, ADR-0067) |
| `concordia-ui` | in-game UI (ADR-0060): retained widgets as components (`UiRoot`, `UiNode`, `UiStyle`, `UiThemed`, `UiText`, `UiImage`, `UiNinePatch`, `UiProgress`, `UiSeparator`), taffy flex/grid/stack layout with Godot-style anchors and canvas stretch, parley text and a swash glyph atlas, `.theme.ron` themes, the `font` and `theme` importers (`AssetKind::Font`, `Theme`), built-in Noto fonts, the draw list (`concordia_api::UiDrawList`) the renderer's UI pass draws, `ui.inspect` and the `ui_bounds` layer (2.7b.1); the controls (`UiButton`, `UiToggle`, `UiSlider`, `UiDropdown`, `UiTabs`, `UiScroll`, `UiTooltip`, `UiFocus`, `UiSounds`, `UiState`), `ui.interact` in the simulation from the `ui_*` actions, focus, pointer consumption and `ui.hit_test` (2.7b.2); `UiTextBox`, `UiSpinBox`, `UiList`, typed text, the engine clipboard (`ui.clipboard`) and world-space roots (2.7b.3) | M2 (2.7b) |
| `concordia-assert` | the playtest assertion language: grammar v1, parser with spans, canonical printer, type checker against reflection with units, Rust builder (ADR-0084); `concordia-replay` evaluates it (`assert.eval`) | M3 (3.2) |
| `concordia-replay` | the replay engine module (`[replay]`): input log v0 with checkpoint hashes and divergence reports (M2 2.8, ADR-0072); format v1 with per-tick hashes, stills for field diffs, re-blessing and A/B runs (ADR-0086), and playtest assertions (`assert.eval`) (M3) | M2 (2.8) |
| `concordia-studio` | Agent Studio: role configs, team, task board, `studio sync` → Claude Code files; the default roster and engine skills live in its `roster/` (a project gets its own editable `studio/`) | M3 |
| `concordia-gen` | generated content: the provider interface and registry, `[gen]` with `gen.list` / `gen.run` (provenance in sidecars), procedural meshes as GLB (3.7a); sfx, music, textures (3.7b); the Blender bridge (a separate Blender process, ADR-0085) and `[[gen.plugin]]` out-of-process providers (3.7c); seeded rocks, canyon strips and the M4 canyon spec, output hashes in provenance and `gen.restore` (M4 4.4c, ADR-0085 amended) | M3 |
| `concordia-graph` | visual graphs stored as text, compiler to fast code | M4 |
| `concordia-ai` | runtime AI: behavior trees, utility AI, optional LLM planner plugins | M4 |
| `concordia-import-unity`, `concordia-import-unreal` | importers targeting the interchange format | M5 |
| `concordia-interchange` | interchange model (USD/glTF + scene text) shared by importers | M5 (with the Godot importer, ADR-0067) |
| `concordia-testkit` | dev-dependency helpers: FLIP golden compare, `gpu_test!`, bless support | M1 |
| `concordia-script-native` | native script backend: dylib modules with copy-on-load hot reload, static path, mutable-statics source check (ADR-0061) | M2 (2.6c) |
| `concordia-script-guest` | guest SDK for `wasm32-wasip2` gameplay components | M2 |
| `concordia-import-fbx` (+ `ufbx-sys`) | FBX import via ufbx (MIT, vendored v0.23.1), into the glTF importer's asset types | M2 (2.3d) |
| `concordia-translate` | logic-translation tooling (GDScript, C# and Blueprint front ends) | M5 |
| `concordia-cluster` (+ `meshopt-sys`) | meshlet building for GPU culling (ADR-0100): 128-triangle meshlets (64 for a device that needs it) per material, bounding spheres and conservative normal cones, a build deterministic for any thread count (Morton piles), `MeshletBlob` v1 (the GPU-ready encoded form) and its DDC build key, `MeshletStats`; safe Rust over `meshopt-sys`, raw FFI to vendored meshoptimizer v1.3 (MIT) with a checked wrapper (M4 4.2, R5) | M4 (4.2) |
| `concordia-perf` | perf testing on reference hardware (ADR-0089): budget files, the reference machine, reports, verdicts (`NotReference` off the reference) and the scripted-camera runner behind `concordia perf run` | M4 (4.1a) |
| `concordia-pathtrace` | reference path tracer (test oracle; must not depend on `concordia-render`): backend-neutral scene, two-level SAH BVH and counter-based sampling, the CPU backend (4.1b2), the GPU backend (4.1b3) | M4 (4.1b2) |
| `concordia-ai-llm-local` | opt-in example local-LLM provider; not a default workspace member | M4 |
| `concordia-evolve` | Evolve plugin system: manifests, resolution, lockfile, trust, orchestration over the native and WASM hosts | M6 |
| `concordia-evolve-api` | stable extension-point API that native plugins link (WIT package `concordia:evolve` for WASM plugins) | M6 |
| `concordia-terra-sim` | Terra templates, traits and spawners (2.0c, ADR-0070); LOD and representation (2.4b). Processing phases and signals live in `concordia-terra` (2.0, 2.0b) | M2 (moved from M7, ADR-0067) |
| `concordia-terra-crowd` | Terra crowds and traffic over `concordia-nav`: `CrowdState`, the `CrowdGrid` snapshot, the `crowd.*` chain (DetourCrowd's steps; surface and physics agents), the `Crowd` trait and the `crowd` debug layer (2.5c.1, ADR-0077); sampled avoidance, the local wall boundary, the LOD opt-in and the `crowd_10k` bench (2.5c.2); lane traffic: lanes and junctions as entities, the IDM, junction grants, braking for people, the `Traffic` trait and the `lanes` layer (2.5d, ADR-0079) | M2 (scale and AI-driven behavior in M7) |
| `concordia-fracture` (+ `manifold-sys`) | offline fracture (ADR-0180; cutting through vendored Manifold v3.5.5, Apache-2.0, behind a checked wrapper): patterns, mesh cutting, interior faces, hulls, cluster levels, connection graph | M8 |
| `concordia-destruction` | runtime destruction on Jolt: strain, breaking, fields, debris, caches | M8 |
| `concordia-cloth` | garments: asset, CPU reference and GPU (Slang) XPBD solver, collision | M8 |
| `concordia-sdf` *(proposed)* | mesh signed distance fields: offline sparse-brick build in the DDC, CPU reference queries; independent of the renderer | M9 |
| `concordia-photon` *(proposed)* | Photon GI: global SDF clipmap, surface cache, tracer chain, screen probes, radiance cache, reflections, translucency volume (implements `gi::Technique`) | M9 |
| `concordia-bvh` *(proposed)* | CPU AABB trees: deterministic builds, refit, frustum and sphere queries; shadow-caster culling first, and the base M4's ray `bvh` module grows from | M10 |
| `concordia-sparx` *(proposed)* | Sparx particles without a GPU: the `.vfx.ron` effect format, attribute layouts, the module contract and built-in modules, template codegen to Slang, the `DataInterface` trait, the CPU reference simulator (test oracle) and the `vfx` engine module; the GPU runtime is `concordia-render::sparx` | M11 |
| `concordia-rig` *(proposed)* | procedural rigs without a GPU: the `.rig.ron` format, rig units and the `RigUnit` trait, IK solvers (two-bone, FABRIK, CCD), aim, look-at and constraints, the compiler and evaluator, backward solve, baking, the pose library and the `rig` engine module | M12 |

**Test package:** `tests/` is the `concordia-tests` package (`publish = false`, `autotests = false`). Its integration tests and gates fold into one test binary, `integration` (`tests/integration/main.rs` declares each file as a module; 2026-11-27), as do `concordia-app`'s and `concordia-mcp`'s (`tests/main.rs`): select a file's tests with `--test integration <file>::` or `-E 'binary_id(concordia-tests::integration) & test(/^<file>::/)'`. Only `bless_reference` (release-only) keeps its own binary. Shared helpers (MCP test client, binary locators, gate checkers) live in its library, `tests/src/`.

New crates may be proposed in a plan's "Open questions" section; they are added here during the consistency pass.

### Text formats & file extensions
- Format: **RON**, with canonical formatting (sorted keys, one entity per block, stable UUIDv7 ids, trailing commas). ADR-0009 (M0) confirms or replaces it.
- `.scene.ron`: scenes. `.prefab.ron`: prefabs. `.material.ron`: materials. `.input.ron`: input maps (M1). `.animgraph.ron`: animation graphs (M2). `.cue.ron`: audio cues, `.buses.ron`: audio bus layouts (M2 2.7a, ADR-0066). `.theme.ron`: UI themes (M2 2.7b, ADR-0060). `.data.ron`: typed data documents (M2). `.replay.ron`: replays (v0 input log in M2, extended in M3). `.playtest.ron`: assertion suites (M3). `.score.ron`: procedural music scores (M3). `.graph.ron`: visual graphs (M4). `.template.ron`: Terra entity templates (M7). `.breakable.ron`: fracture recipes, `.garment.ron`: garments (M8). `.vfx.ron`: Sparx effects, `.vfxmod.ron` (+ a `.slang` of the same stem): Sparx modules (M11, proposed). `.rig.ron`: rigs, `.clip.ron`: authored clips (keyed control curves, sampled bone tracks), `.poselib.ron`: pose libraries (M12, proposed). `evolve.toml`: Evolve plugin manifest; `evolve.lock`: project plugin lockfile (M6). `.cmdlog.jsonl`: command journal, at `.concordia/journal/<scene-id>.cmdlog.jsonl` (M0).
- **Exception (ADR in M3):** Studio role files (`studio/agents/<role>.md`) and tasks (`studio/tasks/T-0001-<slug>.task.md`) are Markdown with **YAML frontmatter**, to match Claude Code's native agent format.
- **Ids:** entities created in the engine get UUIDv7. **Imported** entities get name-based UUIDv5, derived from the source id plus a project import namespace, so re-importing is deterministic.
- **Asset references:** `"asset:<uuid>"`, `"asset:<uuid>#<sub-path>"`, `"builtin:<kind>/<name>"`. DDC layout: `.concordia/ddc/<2 hex>/<blake3 key>`.
- Project manifest: `concordia.toml` in a game project's root. `[render] meshlets` (default true) turns the meshlet path on or off (M4 4.3a); which meshlet route it takes (compute + indirect, or mesh shaders) follows the device's caps, not a setting (4.3b). `[render] culling` (default true) turns the meshlet path's GPU culling on or off (M4 4.4). `[render] temporal` (`off` (default), `own`, `metalfx`) picks the temporal resolve and `[render] upscale` (1.0 (default), 1.5 or 2.0) the ratio of output to internal size (M4 4.6, ADR-0103). `[render] shadows` (`csm` (default during 4.7) or `vsm`) picks the sun's shadows, and `[render.vsm]` (`pool_pages` 4096, `levels` 16, `resolution_bias` −1, `max_distance` 1000, `depth_bias` 1, `normal_bias` 1) sizes the virtual shadow maps (M4 4.7, ADR-0091). Engine modules are sections (`[assets]`, `[render]`, `[physics]`, `[scripts]`, `[replay]`, `[studio]`, `[anim]` since 2.4.1).
- Assets keep their source formats (`.glb`, `.png`, `.wav`, ...). Per-asset import settings go in a sidecar file `<file>.import.ron`, holding a stable asset id. Model sidecars (glTF, FBX) take `"meshlets": "auto" | true | false` (default `"auto"`: static meshes over 8,192 triangles; M4 4.2, ADR-0100).
- **Generated sources** (M4 4.4c, ADR-0085 amended): a sidecar's `provenance` records the provider, version, request, seed, `inputs_hash` and `output` (BLAKE3 of the bytes). A project may leave large generated files out of git (its own `.gitignore`) and commit only the sidecars; `gen.restore` remakes and checks them, and `concordia perf run` and the canyon tests restore before loading (`examples/m4-canyon`).
- Derived data beside import artifacts: a meshlet build is cached under its own DDC key (blake3 of the format, build and meshoptimizer versions, the settings and the positions, indices and materials it reads; no platform) and stored as `MeshletBlob` v1 in `MeshData::meshlets` (ADR-0100, amended in M4 4.2).

### Engine API & MCP naming
- The Engine API is JSON-RPC 2.0. Method names are `domain.verb`, e.g. `scene.open`, `scene.save`, `entity.spawn`, `entity.despawn`, `component.get`, `component.set`, `query.run`, `schema.get`, `txn.begin`, `txn.commit`, `txn.undo`, `txn.redo`, `sim.play`, `sim.pause`, `sim.step`, `capture.screenshot`, `input.inject`, `asset.import`, `replay.record`, `replay.run`, `assert.eval`, `task.list/create/update` (M3), `log.tail`.
- MCP tools map 1:1, with `.` replaced by `_` (e.g. `entity_spawn`). There is no budget on the whole `tools/list`: no agent gets it all. Each Studio role gets only the tools its roster entry names, and each enabled default role's tool definitions stay under 50k tokens (ADR-0073, amended 2026-11-21 from 22k; superseding the whole-list budgets of ADR-0033 and ADR-0070). Every MCP tool's input schema is **generated from `concordia-reflect`**, never hand-written.
- Every mutating call carries an `actor` (grammar `<human|agent|system|tool|script>:<name>[#n]`). It may be omitted on the wire when the session has a default (`session.open`), but the recorded actor is always present. `human:*` actors are not accepted over MCP. The MCP server is named `concordia` in `.mcp.json`, so Claude Code tools appear as `mcp__concordia__<tool>`.
- **Edit world vs play world:** the command log records mutations of the **edit world** only. Simulation (physics, scripts, AI) runs in a separate **play world** (`world: "edit" | "play"` param on read methods) and never mutates the edit world. Injected input and play-world outcomes are recorded by `concordia-replay`, not the command log.
- **Additional methods agreed in the plans** (MCP names follow the same rule): M0: `session.open`, `scene.new`, `scene.info`, `entity.get`, `component.patch`, `component.remove`, `txn.abort`, `txn.history`, `sim.stop`. M1: `asset.list`, `physics.raycast`, `events.subscribe`. M2: `terra.spawn_batch/despawn_batch/stats/inspect`, `script.reload` (from the `scripts` module, `[scripts]`), `nav.build`, `nav.path`, `viewport.pointer_ray` (2.5b.2a), `replay.record/run` (v0, from the `replay` module, `[replay]`; 2.8), `material.get/set` (from the `assets` module, `[assets]`; 2.3c), `anim.inspect` (from the `anim` module, `[anim]`; 2.4.4; `anim.set_param` / `anim.travel` dropped for `component.patch` with `world: "play"`), `audio.play/stop/buses/render/inspect` (from the `audio` module, `[audio]`; 2.7a.2, ADR-0066; none mutates the world). M5: `port.*`, `shader.translate` (moved from M2, ADR-0067). M3: `task.get/claim/comment/approve/reject`, `studio.roster/status/activity/sync`, `studio.ask/answer/escalate/questions` (3.6b), `gen.list/run` (`audit.record` is replaced by `file_edit` entries in the Studio journal, ADR-0080). Studio methods come from the `studio` engine module (`[studio]` in `concordia.toml`), not the core set. M4: `graph.*`, `ai.inspect/trace`, `render.stats`, `perf.capture`, `capture.reference`, `asset.meshlet_stats` (from the `assets` module, read-only; 4.2); `render.set_temporal` (session-scoped, not a world mutation; 4.6a), `render.set_shadows` (session-scoped; 4.7a), and `capture.screenshot`'s `temporal_frames` (frames rendered before the capture, by default a whole jitter cycle with temporal on); `render.set_debug_view` was dropped in 4.3a: `view_mode` on `capture.screenshot` and `viewport.overlays` selects debug views (meshlets, triangles, overdraw and the rest). M5: `import.scan/run/report/licenses`, `translate.*`. M11 (proposed): `vfx.validate`, `vfx.stats` (from the `vfx` module, `[vfx]`; VFX artist scope only). M12 (proposed): `rig.validate`, `rig.apply`, `rig.evaluate`, `rig.bake`, `rig.pose_save`, `rig.pose_apply` (from the `rig` module, `[rig]`), and `anim.key_set`, `anim.key_get`, `anim.key_delete`, `anim.preview` (from M2 2.4's animation module); animator scope; the unit catalogue is the MCP resource `concordia://rig/units`.
- Errors: JSON-RPC codes −32001…−32019 (M1 added −32013 `GpuUnavailable`; M3 added −32014 `TaskNotFound`, −32015 `TaskTransition`, −32016 `NotPermitted`, −32017 `EngineExited`; M2 added −32018 `ScriptFailed` and −32019 `ReplayMismatch`), with `error.data = { kind, suggestions[], ... }` (defined in M0). MCP resources use `concordia://` URIs.

### Testing conventions (TDD)
- Runner: `cargo nextest run`. Doc tests: `cargo test --doc`. Editor UI: `vitest` + Playwright.
- Unit tests live in the crate (`#[cfg(test)]`); crate integration tests in `crates/<c>/tests/`; cross-crate tests in `tests/`.
- Test names describe behavior: `spawned_entity_has_stable_uuid`, `undo_restores_previous_component_value`.
- **Checks per phase and per milestone** (author decisions 2026-11-25; the second, later that day, narrows the first because full per-phase checks were too slow and resource-hungry): **a phase runs only its new tests and the few existing tests its code directly affects** (the test files of the modules it changed, chosen by hand, not every dependent crate), plus `cargo fmt` and clippy on the crates it changed (`-D warnings`). No mutation checks, no `vulkan` workflow, no local lavapipe (Docker) run and no `just check` per phase. **A milestone's end runs everything:** `just check full` (the whole suite, the gates included), the full mutation sweep over everything the milestone built (`just mutants`), and the on-demand `vulkan` workflow. Build outputs are cleaned (`just clean-builds`) only when free disk is under 25 GB.
- **Gate tests** go in `tests/gates/m<n>_*.rs` (`M-1` → prefix `m_minus1`) and run with `just gate M<n>`, which runs `just ci` first. A milestone is done only when its gate passes in CI: a green run of the manual `regression` workflow on main (ADR-0071; the automatic `ci` workflow runs only the fast checks).
- **Live-agent gates:** `#[ignore]` tests named `m<n>_live_*`, run with `just gate-live M<n>`. They drive the real `claude -p` (never `--bare`, which bypasses the subscription login), strip API-key env vars from the child process, and assert outcomes, never transcripts.
- **Private-content gates:** gates that need the reference port content (M2, parts of M4) keep their test file in `tests/gates/` but read the game through `CONCORDIA_PORT_SRC`/`CONCORDIA_PORT_PROJECT`. They run on the self-hosted runner, the author's Mac, never in public CI, and fail (never skip) when their env var is set but the content is missing.
- **Reference hardware:** the author's Apple M3 Pro (18-core GPU, 18 GB, Metal 4). It is the self-hosted GPU runner and the perf reference (`tests/perf/reference.ron`). Perf tests report `NotReference` elsewhere, and gate perf tests fail on it (`concordia perf run --require-reference`, exit 2). Perf scenes are `tests/perf/scenes/<name>.perf.ron`, budgets `tests/perf/budgets/<name>.budget.ron`, reports `target/perf/<name>.perf.ron` (ADR-0089; `just perf`). A perf scene's optional `temporal: Some((backend: …, upscale: …))` sets the temporal resolve it measures with (M4 4.6a), and `shadows: Some(csm | vsm)` the sun's shadows (4.7a).
- Golden images: `tests/golden/<suite>/<name>[@<class>].png`, compared with a perceptual metric (NVIDIA FLIP) and a tolerance from the suite's `tests/golden/<suite>/tolerances.ron` (a default plus per-golden entries; M4 4.1a, D2; no tolerances in code). They're refreshed with `just bless` (`CONCORDIA_BLESS=1`; never allowed when `CI=true`, and only on Metal), and failure artifacts go to `target/golden-failures/` (`<suite>/<backend>/` for a non-Metal backend). A tolerance file may hold `backends: { "vulkan": (default: …, goldens: {…}) }`; the most specific entry wins (the backend's golden, the shared golden, the backend's default, the shared default; M4 4.1v(b), D4). `CONCORDIA_GOLDEN_KEEP=1` also writes comparisons and scores for passing checks, and the image of a missing golden (`<name>.actual.png`; M4 4.3a), for review pages. `paths: { "<golden>": (…) }` (shared or per backend) holds how closely two draw paths of a golden's scene agree, e.g. the compute fallback against the mesh-shader path (M4 4.3b); `culling: { "<golden>": (…) }` how closely the scene rendered with GPU culling on agrees with it rendered off (M4 4.4). Temporal goldens (`m4_temporal`, M4 4.6) are taken after a settled history, never from a single frame.
- Snapshot tests for text output (scene files, schemas, generated agent files): `insta`.
- Property tests for serialization and ECS invariants: `proptest`.
- Determinism tests: run the same fixed-step simulation twice (and across thread counts), then compare world-state hashes.
- Every phase lists its tests before its implementation notes, and those tests are committed failing first.

### Principles to honor in every plan
0. Subscription-first: nothing requires an LLM API key. Agents act through MCP, the CLI and Claude Code native files.
1. One API: the editor, CLI, tests and agents all use the Engine API.
2. Text is the source of truth.
3. Closed loop: headless, capture, inject, replay, assert.
4. Every mutation is a transaction with an actor.
5. Clean room (see `docs/clean-room.md`).
6. AI-legible: small crates, `AGENTS.md` per crate, doc comments on every public item.
7. TDD.

---

## ADR number registry
To avoid collisions, each milestone draws ADR numbers from its own block. Use the next free number in your block, and add a row here when you write one.

| Block | Owner | Assigned |
|---|---|---|
| 0001–0008 | Design | 0001 Rust · 0002 subscription-first · 0003 scripting tiers · 0004 editor · 0005 interchange · 0006 licensing · 0007 TDD/plans · 0008 name (Concordia) |
| 0009 | M0 | 0009 scene text format (written: canonical RON, own reader/writer) |
| 0010–0019 | Design additions | 0010 Terra entity system · 0011 Evolve plugin system · 0012 advanced physics milestone M8 (written) · 0013 advanced lighting milestone M9, Photon (written) · 0014 advanced shadows milestone M10, tiled shadow atlas (written, Proposed) · 0015 advanced particles milestone M11, Sparx (written, Proposed) · 0016 rigging milestone M12, procedural rigs and in-engine keyframing (written, Proposed) · 0017 advanced Godot features milestone M13 (written, Proposed) |
| 0020–0029 | M-1 | 0020 toolchain pinning · 0021 WASM guest build path · 0022 CI and supply chain |
| 0030–0039 | M0 | 0030 own archetype ECS (written: safe column storage) · 0031 Engine API error model (written) · 0032 edit world vs play world (written) · 0033 MCP adapter (written: rmcp pin, protocol versions) |
| 0040–0059 | M1 | 0040–0045 as named in M1's plan (0040, 0041, 0042, 0043, 0044, 0045 written; 0041 gained characters' inner bodies for sensors as an update in M2 2.10a) · 0046 engine modules (written) · 0047 editor UI stays web, native viewport, thin panels (written) · 0048 editor v1 in M1, engine breadth before the port (written) · 0049 agent-first end of M1, Studio right after M1 (written) |
| 0060–0079 | M2 | 0060–0066 as named in M2's plan (0060 in-game UI: retained ECS widgets, taffy plus anchors, parley and swash, the draw-list seam and one batched pass (written, 2.7b.1; P1 replaced cosmic-text); 0066 audio stack: cpal, lewton, hound, an own mixer outside the world, `.cue.ron` (written, 2.7a.1; the engine module, the device and the tools added as decision 8, 2.7a.2); 0061 native tier: C ABI, hot reload and migration (written, 2.6c; scripts module and `script.reload`, 2.6d); 0062 script protocol: types and wire encoding written in 2.6a, backend seam, WIT world and WASM backend in 2.6b, input for scripts (`InputActions`) added as an update in 2.10a; 0063 gameplay determinism contract written; 0064–0065 move to M5 with the port) · 0067 M2 refinement: engine breadth on Terra's Mass core (written) · 0068 Substrate-style material model (written, 2.3a; the renderer and the Metal closure budget of 8, 2.3b; migration, `material.get/set` and the editor, 2.3c) · 0069 Terra storage kinds, command buffers, signals and observers (written) · 0070 Terra templates, traits and spawners; tool-list budget 16k (written; the budget superseded by 0073) · 0071 fast automatic CI, manual regression suite (written; amends ADR-0022) · 0072 replay v0: input log, checkpoints and build identity (written, 2.8) · 0073 tool budgets per agent role, no whole-list budget (written; the animator's numbers added to its consequences, 2.4.5) · 0074 animation runtime: one pose per character, clips bound by bone name, sampled in `FrameEnd` (written, 2.4.1; rendering poses added to its consequences, 2.4.2; graphs, 2.4.3; events, attachments, tweens and `anim.inspect` added to its consequences, 2.4.4) · 0075 play-world writes through `component.set` / `component.patch` (written, 2.4.3; amends 0032) · 0076 Terra LOD: viewers, a hashed level, elapsed time, opt-in processors and the signal rule (written, 2.4b.1); representation LOD added to its consequences, 2.4b.2 · 0077 navigation: Recast/Detour navmeshes from static colliders, engine-native marks, per-agent crowds as Terra processors (written, 2.5a; N1); agents, N2 and N3 added as an update (2.5b.1); crowds and N5 added as an update (2.5c.1); avoidance, the local boundary, LOD and the sharded query pool added as an update (2.5c.2) · 0078 pointer rays as input, the lens (`camera_ray`, `viewport.pointer_ray`), `PhysicsQueries` and click-to-move (written, 2.5b.2a; N2, N4); windowed play added as an update (2.5b.2b) · 0079 lane traffic: lanes and junctions as entities, the IDM, junction grants, braking for people (written, 2.5d; N6, N7) |
| 0080–0099 | M3 | 0080 shared engine and Studio attribution (written; `asset.import` checked against the file or its sidecar, M2 2.4.5) · 0081 team questions and answers (written) · 0082 shared engine: discovery, attach and lifetime (written) · 0083 Studio panel and Claude Code launcher (written) · 0084 playtest assertion language v1 (written, 3.2a) · 0085 the Blender bridge and content plugins run as separate programs (written, 3.7c; amended in M4 4.4c: output hashes and `gen.restore`) · 0086 replay format version 1 (written) · 0087 Studio files use Markdown with YAML frontmatter (written) |
| 0100–0119 | M4 | 0100 meshlet geometry pipeline (accepted, 4.3a; amended in 4.2 with `MeshletBlob` v1, the build cache and the `meshlets` setting, in 4.3a with the RHI additions, scene buffers and the compute + indirect path, and in 4.3b with the mesh-shader path) · 0101 reference path tracer: GPU by default, CPU fallback, one core (accepted, 4.1b) · 0102 emission in physical units (accepted, 4.1b4) · 0089 perf testing on reference hardware (written, 4.1a) and 0090 quality methodology (accepted, 4.1b4), numbered from the refinement's list, outside the block |
| 0120–0139 | M5 | |
| 0140–0159 | M6 | 0140–0142 as named in M6's plan |
| 0160–0179 | M7 | 0160–0161 as named in M7's plan |
| 0180–0199 | M8 | 0180–0183 as named in M8's plan |
| 0200–0219 | M9 | 0200–0202 as named in M9's plan |
| 0220–0239 | M10 | 0220–0222 as named in M10's plan |
| 0240–0259 | M11 | 0240–0243 as named in M11's plan |
| 0260–0279 | M12 | 0260–0264 as named in M12's plan |
| 0280–0299 | M13 | 0280–0281 as named in M13's plan |

## Consistency pass log (2026-11-17)
- Merged each plan's proposed conventions into the sections above.
- Test package unified as `concordia-tests` (M1 had `concordia-gates`; M0 and M3 helper paths moved to `tests/src/`).
- ADRs renumbered into blocks (M0: 0010–0012 → 0030–0032; M1: 0010–0015 → 0040–0045).
- M-1 pin must be ≥ 1.88 (`rmcp`).
- M4 Phase 4.13 depends on the `caster` archetype being ported after M2's slice (brute and creep only), with a `brute` fallback.
- M5: CUE4Parse (Apache-2.0) is allowed as a reference, with NOTICE attribution.
- The reference machine is fixed as the author's M3 Pro.

## M0 phase 0.0 log (2026-11-17)
- ADR-0009: RON confirmed by the author (written as Accepted in phase 0.5).
- ADR-0033 added to M0's block (MCP adapter).
- `rmcp` pinned `=3.11.0`; MCP protocol 2026-07-28 is stateless (no `initialize`), 2025-11-25 still supported.
- Crate `AGENTS.md` required sections extended with *Public entry points*, *Common tasks* and *Don'ts* (template and hygiene test updated).
- M0 gate tests are written at the start of phase 0.7 and excluded from the default nextest run until phase 0.8.
- Phase 0.3: ECS storage is safe Rust (ADR-0030), so the Miri decision is moot; no nightly toolchain installed.

## Terra and Evolve additions (2026-11-17)
- Author decisions: the entity system is **Terra** (ADR-0010): `concordia-ecs` is renamed `concordia-terra` right after the M0 PR merges (M7 Phase 7.0) and grows Mass-style features in M7. The plugin system is **Evolve** (ADR-0011), milestone M6, native and WASM plugins from local paths and pinned git sources.
- Numbering: M6 and M7 are new milestone numbers, not schedule positions. M6 starts after M2 (in parallel with M3); M7's simulation features start after M4. Existing milestones keep their numbers.
- Done 2026-11-18 (M7 Phase 7.0): the crate is `concordia-terra`, and the M1–M5 plans, design doc and crate docs use the new name; only ADRs and plan history mention `concordia-ecs`.
- Actor grammar gains `plugin:<id>` in M6 (ADR-0011).
- The M3 `concordia-gen` provider seam and the later-backlog "optional plugin crate" items become Evolve plugins once M6 lands.

## M1 start (2026-11-18)
- M1 is reconciled with M0 as built, and its dependencies are re-checked (see the M1 plan's *Execution notes*). Author decisions D1–D9 are recorded there. OpenGL is rejected as a backend (ADR-0040's alternatives).
- New Phase 1.0, the engine-module seam (ADR-0046): feature crates add types, methods, play systems and play-time state through `EngineModule`. Each engine has its own method registry; `concordia_api::methods()` stays the core set.
- `sim.*` accept an attribution-only `actor`, and `sim.step` logs `sim stepped`. Input injection and physics act only on the play world and are never journaled (ADR-0032).
