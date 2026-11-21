# M4: Unreal-class visuals
Status: Planned · Depends on: M1 (render), M2 (scripting host, graphs' WIT API), M3 (replay assertions, Studio) · Detail level: **Needs refinement after M3** · Estimated effort: 32–45 agent-assisted engineer-weeks (two parallel tracks; see Effort estimate)

> This plan has full phase breakdowns and test lists. Its numbers (budgets, FLIP tolerances, page sizes, meshlet sizes) are **provisional** and must be recalibrated in the refinement pass after M3, using measurements from the M1–M3 renderer on the reference Mac. Every recalibrated number lands in a text file (`tests/perf/budgets/*.budget.ron`, per-test tolerances), never in code.

## Goal & context
M1 gave us a clustered forward+ PBR renderer with cascaded shadow maps (CSM), a render graph, Slang shaders and golden-image tests. M2 and M3 gave us WASM scripting, the reference port vertical slice, record/replay and playtest assertions, and Agent Studio. M4 adds the parts that make the engine's visuals and authoring look like Unreal's, and it builds them only from public research:

1. **Meshlet geometry with GPU culling (ADR-0100).** Offline meshlets (meshoptimizer), bindless geometry buffers, and per-meshlet frustum, normal-cone and two-pass HZB occlusion culling in object/mesh shaders (Metal 3 first) or in compute with indirect draws. The hardware rasterizer draws the survivors into M1's clustered Forward+ pass. Per-meshlet LODs and HLODs follow later; there is no software rasterizer, visibility buffer or geometry streaming.
2. **Virtual shadow maps (VSM).** Page-based, cached across frames, and able to handle many lights. VSM replaces CSM for opaque geometry; CSM stays as the fallback path.
3. **Dynamic GI**, staged: world-space irradiance probes in the style of DDGI first; a screen-space radiance cache and surface cache later.
4. **Temporal AA and upscaling.** Our own TAA/TAAU, with MetalFX as an optional backend on Apple.
5. **`concordia-graph`.** Visual graphs stored as text (`.graph.ron`), a node library generated from `concordia-reflect` and the WIT API, a type checker, a compiler to WASM components, an editor graph panel, and MCP tools so agents author graphs as text.
6. **`concordia-ai`.** Runtime AI with no LLM: behavior trees, utility AI, perception and a blackboard. It also defines an optional, opt-in interface for LLM planner and dialogue plugins.

All of this is test-driven. Perf budgets are tests. Shading quality is measured against our own reference path tracer, which is a test oracle with its own tests. CPU-side algorithms (meshlet building, culling references, VSM page tables, the graph type checker) get ordinary unit and property tests.

**Clean room is critical in this milestone.** Nanite, Lumen and VSM are Unreal features. We implement only from the public papers and talks in the References appendix, from public API docs, and from permissively licensed code (meshoptimizer, MIT). No contributor who has studied Unreal's renderer source may write the equivalent system here (`docs/clean-room.md`). Each new crate's or module's `AGENTS.md` carries a References section copied from the appendix entries it used.

## Scope / Non-goals
**In scope**
- Static and rigid-instanced opaque and alpha-masked geometry through the meshlet path.
- Directional, spot and point lights in VSM. Up to 64 shadowed local lights per frame, plus any number of unshadowed spell lights through the existing light clusters.
- Diffuse dynamic GI (DDGI-style probes) with fast response to spell lights. Specular GI comes from a probe-based fallback plus the M1 reflection path; full-quality specular GI is stage B.
- TAA and TAAU on every backend, and the MetalFX temporal scaler on Apple.
- `concordia-graph` with Event, Function, Behavior (behavior tree) and Utility graph kinds.
- `concordia-ai`: behavior trees, utility AI, perception, blackboard, and the LLM plugin interface with a deterministic mock provider.
- A perf harness, a reference path tracer, and quality methodology that later milestones reuse.

**Non-goals (deferred; see `later-backlog.md`)**
- Skinned or deforming meshes as meshlets. They stay on the M1 forward+ path and share the depth buffer.
- Translucent meshlets. Translucent surfaces stay on the M1 forward+ path.
- A software rasterizer, a visibility buffer with material binning, a cluster LOD DAG, and virtualized geometry streaming from disk (ADR-0100). Any of them needs its own ADR.
- GI stage B (screen-space radiance cache, surface cache) as a *requirement*. It is scaffolded here and built as M9, Photon (ADR-0013).
- Hardware-ray-traced reflections (M9, Photon); frame generation (MetalFX frame interpolation); D3D12; consoles.
- Tessellation or displacement of meshlets; foliage and voxel LODs.
- Shipping any LLM model or provider in the default build.

## Dependencies (what must already exist, from which milestone)
| Needed | From |
|---|---|
| `concordia-rhi` with a Metal backend, compute, indirect dispatch/draw, timestamp queries, headless offscreen targets | M1 |
| Render graph, clustered forward+ PBR, CSM, `.material.ron`, camera, `capture.screenshot` | M1 |
| `concordia-shaders` (Slang → MSL/SPIR-V, reflection, shader cache) | M1 |
| `concordia-assets` with the DDC (`.concordia/ddc`), importer registry, hot reload; `concordia-import-gltf` | M1 |
| Golden-image infrastructure (`tests/golden/`, FLIP comparison, `just bless`) | M1 |
| Jolt raycasts (perception line of sight) | M1 |
| `concordia-script-wasm` host, `wit/` package `concordia:game@0.1.0`, deterministic fixed step | M2 |
| `concordia-nav` (move-to tasks), `concordia-anim` (play-montage tasks) | M2 |
| reference port slice with a ported enemy (the M2 Rust/WASM port of the game's enemy brain: PATROL/CHASE/ATTACK) | M2 |
| `concordia-replay` with `.replay.ron`, playtest assertions (`assert.eval`) | M3 |
| `concordia-studio` (`studio sync`) and `concordia-gen` procedural meshes | M3 |
| Editor shell (Tauri v2 + React panels) as an Engine API client | M1 |

## Crates, modules & directories (created / touched)
**Created**
- `crates/concordia-graph/`: graph model, `.graph.ron` read/write with canonical formatting, node library, type checker, IR, interpreter (the test oracle), WASM compiler.
- `crates/concordia-ai/`: blackboard, behavior tree runtime, utility AI, perception, `LlmProvider` plugin interface, mock provider.
- `crates/concordia-cluster/` *(proposed)*: CPU-only meshlet building and per-meshlet bounds and normal cones for GPU culling (ADR-0100). Testable without a GPU.
- `crates/meshopt-sys/` *(proposed)*: bindgen over vendored `third_party/meshoptimizer` (MIT, v1.0 or later).
- `crates/concordia-pathtrace/` *(proposed)*: a reference compute path tracer with its own BVH. It may **not** depend on `concordia-render`, so the oracle stays independent of the code under test.
- `crates/concordia-ai-llm-local/` *(proposed, opt-in, not a default workspace member)*: an example local-model provider plugin.
- `tests/gates/m4_meshlet_geometry.rs`, `m4_lighting_quality.rs`, `m4_fallbacks.rs`, `m4_graph_enemy.rs`.
- `tests/golden/m4_meshlet/`, `m4_vsm/`, `m4_gi/`, `m4_taa/`, `m4_reference/` (path-traced references), `m4_graph/`.
- `tests/perf/`: the harness runner, `reference.ron` (the reference machine), `budgets/*.budget.ron`.
- `examples/m4-canyon/` (gate scene with ≥20M instanced source triangles, procedural, CC0), `examples/m4-lighting/` (quality scenes).
- `editor/ui/src/panels/graph/`: the graph panel.
- `studio/skills/author-graph/`, `studio/skills/author-behavior/`: Claude Code skills for graph authoring.

**Touched**
- `concordia-rhi`, `concordia-rhi-metal`, `concordia-rhi-vulkan`: a capability struct (`RhiCaps`), mesh/object (task) stages, ray queries and acceleration structures, timestamp granularity, cap masking for tests.
- `concordia-shaders`: mesh/object stages through Slang, a Slang capability profile per backend.
- `concordia-render`: new modules `meshlet` (scene buffers, culling, HZB, both draw paths, drawing into Forward+), `vsm`, `gi`, `temporal` (TAA/TAAU, the MetalFX backend), `perf` (pass timing and budget checks). CSM and forward+ remain.
- `concordia-assets`, `concordia-import-gltf`: a "meshlet build" derived-data stage, triggered by `<file>.import.ron` (`meshlets: true`).
- `concordia-reflect` (+ derive): `#[reflect(graph_node(...))]` metadata and pure/impure markers.
- `wit/`: an `concordia:game/ai` interface (blackboard, perception queries) for graphs and WASM scripts.
- `concordia-script-wasm`: loads graph-compiled components; Wasmtime configured for NaN canonicalization.
- `concordia-replay`: decision traces, and recording of non-deterministic plugin outputs (LLM responses).
- `concordia-api` / `concordia-mcp`: the `graph.*`, `ai.*`, `render.*` and `perf.*` methods (see Proposed convention additions).
- `concordia-studio`: generates the graph-authoring skills and the role permissions for graph files.
- `concordia-gen`: high-poly procedural rock and ruin generators for the gate scene.

## Phases
There are two parallel tracks. **Track R (rendering)** is 4.1–4.8; 4.1 comes first because it builds the oracles every later phase tests against. **Track G (graphs and AI)** is 4.9–4.12 and depends only on M2/M3, so it can start on day one. Phase 4.13 joins the tracks.

---

### Phase 4.1: Perf harness, capability matrix and reference path tracer (oracles first)
- **Objective:** Before writing any feature, build the measuring instruments: per-pass GPU timing with budgets as tests, a capability matrix with masking (so one machine can test every fallback), and a path-traced reference renderer that is itself verified.
- **Deliverables**
  - `RhiCaps { mesh_shaders, object_shaders, ray_query, hw_ray_tracing, metalfx_temporal, timestamp_granularity, max_mesh_output_vertices, max_mesh_output_primitives }`, filled in at device creation from Metal GPU families and runtime probes, and Vulkan features/extensions. Also `RhiCaps::masked(&CapsMask)` for tests.
  - `concordia-render::perf`: `PassTimer`, which wraps each render-graph pass in timestamp samples. On Apple Silicon, Metal counter sampling is supported only at encoder (stage) boundaries, so in perf mode the render graph gives every timed pass its own encoder. It also provides `FrameStats` (per-pass ms, counters such as meshlets drawn and VSM pages resident) and `BudgetFile` (RON).
  - The `tests/perf/` runner: warm up 120 frames, measure 300 frames along a scripted camera path, report median and p95 per pass, write `target/perf/<scene>.perf.ron`, and compare against `tests/perf/budgets/<scene>.budget.ron`.
  - `tests/perf/reference.ron`, which names the reference machine (GPU family, core count, OS). **Reference machine: the author's Apple M3 Pro (18-core GPU, 18 GB, Metal 4, macOS 26)**, which supports Metal 3 mesh shaders. The file lets a perf test tell whether it is running on reference hardware.
  - `concordia-pathtrace`: a progressive unidirectional path tracer in Slang compute with next-event estimation and MIS, its own CPU-built SAH BVH (binned), our PBR BRDF (GGX + Lambert, energy-compensated to match the M1 model), emissive triangles, analytic lights and an environment map. It uses fixed seeds (deterministic), a fixed tonemap and exposure, and writes a PNG.
  - `capture.reference` (Engine API) and `just bless-reference <scene>`, which renders and commits `tests/golden/m4_reference/<scene>.png` plus `<scene>.reference.ron` metadata (spp, seed, git hash).
- **Tests first**
  - `caps_report_matches_known_family_table`: on Metal, the caps derived from the GPU family match a hand-written table for Apple7, Apple8, Apple9 and Apple10 (mesh shaders: Apple7+; hardware RT: Apple9+).
  - `masked_caps_hide_features_from_render_paths`: with `mesh_shaders` masked, the meshlet pipeline selects the compute + indirect fallback (asserted through `FrameStats.raster_path`).
  - `pass_timer_reports_every_graph_pass_once`: a three-pass graph yields three monotonic, non-zero timings.
  - `pass_timer_overhead_under_2_percent`: frame time with timers on vs off, on a fixed scene.
  - `budget_file_roundtrips_canonically` (`insta` snapshot).
  - `budget_violation_names_pass_and_excess`: a synthetic stats input over budget fails with a message such as `meshlet.draw: p95 2.71 ms > budget 2.00 ms (+35%)`.
  - `perf_test_on_non_reference_hardware_reports_but_does_not_claim_pass`: the result status is `NotReference`, never `Pass`.
  - `pathtrace_white_furnace_conserves_energy`: a unit-albedo Lambert sphere inside a uniform white environment renders with mean radiance 1.0 ± 0.5% (Lambert) and ± 3% (rough GGX, after energy compensation).
  - `pathtrace_matches_analytic_irradiance_from_disk_light`: the radiance of a diffuse plane under a disk light matches the closed form within 1%.
  - `pathtrace_converges_at_expected_rate`: RMSE against a 64k-spp image falls roughly as 1/√spp (the slope from a log-log fit lies within [−0.6, −0.4]).
  - `pathtrace_is_deterministic_for_fixed_seed`: two runs are bit-identical.
  - `pathtrace_bvh_traversal_matches_brute_force` (`proptest`): random rays against random triangle soups give the same nearest hit as brute force.
- **Implementation notes**
  - FLIP comes from M1. A test reports both the mean and the 99th-percentile FLIP, and a golden-test tolerance names both.
  - There are two classes of golden test: **reference** goldens (against the path tracer; loose; measure accuracy) and **regression** goldens (against our own blessed output; tight, about 0.01 mean; catch drift). Every M4 lighting feature has both.
  - The path tracer deliberately shares no code with `concordia-render` except the BRDF Slang module, and the BRDF is pinned by the furnace test. The path tracer uses its own BVH even where hardware RT exists.
  - References: PBRT 4th ed. (Pharr, Jakob, Humphreys); Veach thesis (MIS); FLIP (Andersson et al. 2020); Apple *Metal Feature Set Tables* and "Sampling GPU data into counter sample buffers"; Kulla & Conty 2017 (energy compensation).
- **Done criteria:** all tests are green; `concordia perf run examples/m1-sponza-like` produces a `.perf.ron`; one reference image is blessed for an M1 scene and the M1 forward+ renderer's FLIP against it is recorded as the **baseline** (the starting point for GI gains).

---

### Phase 4.2: Offline meshlet building (CPU)
- **Objective:** Split any static mesh into meshlets with bounds and normal cones for GPU culling, and cache the result in the DDC. All of this is CPU work, tested without a GPU.
- **Deliverables**
  - `meshopt-sys` (vendored meshoptimizer v1.0 or later, MIT) exposing `meshopt_buildMeshlets*`, `meshopt_buildMeshletsSpatial`, `meshopt_computeMeshletBounds` / `meshopt_computeClusterBounds`, `meshopt_optimizeMeshlet`, and the vertex and index codecs.
  - `concordia-cluster`: `MeshletBuildSettings { max_triangles: 64 | 128, max_vertices: 64..=128, cone_weight }`; `MeshletMesh { meshlets, vertices, indices }`; `Meshlet { bounds: Sphere, cone: NormalCone, material, vertex_range, triangle_range }`.
  - A meshlet never spans two materials, so each draw shades with one material.
  - Encoded meshlet data written to `.concordia/ddc`, keyed by content hash plus settings hash.
  - A `meshlets: true | false | auto` field in `<file>.import.ron`; `auto` selects static meshes with more than 8k triangles.
- **Tests first**
  - `meshlets_respect_vertex_and_triangle_limits` (`proptest` over random meshes).
  - `every_source_triangle_appears_in_exactly_one_meshlet`.
  - `meshlets_never_span_two_materials`.
  - `meshlet_bounds_contain_every_vertex` (`proptest`).
  - `normal_cone_is_conservative`: for random views, a meshlet the cone test rejects has no front-facing triangle.
  - `meshlet_encoding_roundtrips_exactly`.
  - `build_is_deterministic_across_thread_counts`: same input gives byte-identical output with 1 and N threads.
  - `ddc_reuses_meshlets_when_source_unchanged`: the second import is a cache hit.
  - `meshlet_build_throughput_budget`: a 10M-triangle mesh builds in under 10 s on the reference Mac (a perf test with `NotReference` semantics; provisional).
- **Implementation notes**
  - 64 or 128 triangles per meshlet is chosen per backend from `RhiCaps` (`max_mesh_output_*`); the default is the larger size that fits.
  - References: Kapoulkine, meshoptimizer README and API docs (meshlets, cluster bounds); Kubisch 2018; Apple WWDC22 mesh shaders; Wihlidal 2016.
- **Done criteria:** `concordia import` builds meshlets for the glTF test corpus; all tests are green; `concordia cluster stats <asset>` prints meshlet count, triangles per meshlet and bounds statistics.

---

### Phase 4.3: GPU scene buffers and meshlet drawing into Forward+
- **Objective:** Draw meshlets (no culling yet) straight into M1's clustered Forward+ pass, through mesh shaders or a compute fallback, with the same image as the M1 path.
- **Decision (ADR-0100):** meshes are split offline into 64- or 128-triangle meshlets with meshoptimizer, and the geometry is uploaded into large bindless GPU buffers. Per-meshlet frustum, backface (cone) and HZB occlusion culling (4.4) runs in object/mesh shaders (Metal 3 first, then `VK_EXT_mesh_shader`, then D3D12 mesh shaders), or in compute with indirect draws where mesh shaders are missing. The hardware rasterizer draws the surviving meshlets into the Forward+ pass (or a G-buffer if M4 adds one). There is no software rasterizer and no visibility-buffer material binning. LOD starts with culling only; discrete per-meshlet LODs and HLODs come later (4.5), and virtualized geometry streaming is out of scope at first.
- **Deliverables**
  - GPU scene buffers: an instance table (transform, previous transform, mesh, flags) and bindless meshlet vertex, index and header buffers.
  - Draw paths, selected by `RhiCaps`:
    1. **Mesh shader path** (Metal 3 object+mesh on Apple7+; `VK_EXT_mesh_shader`; D3D12 mesh shaders when that backend exists): the object stage takes a batch of meshlets and amplifies the survivors; the mesh stage emits ≤128 triangles; the fragment stage is the M1 Forward+ shader.
    2. **Compute + indirect fallback** (no mesh shaders): a compute pass writes indirect draw arguments for the surviving meshlets, drawn with indirect indexed draws that pull vertices from the bindless buffers.
  - Motion vectors from current and previous instance transforms (for TAA in 4.6).
  - Skinned, translucent and other non-meshlet geometry stays on the M1 path and shares the depth buffer.
- **Tests first**
  - `meshlet_render_matches_forward_plus_golden`: the same glTF scene drawn through meshlets and through M1's path differs by mean FLIP ≤ 0.005 (`m4_meshlet/meshlets_vs_forward.png`).
  - `compute_fallback_matches_mesh_shader_path`: with `mesh_shaders` masked, the image is FLIP ≤ 0.002 against the mesh-shader output.
  - `masked_material_meshlets_render_alpha_cutouts_correctly` (golden).
  - `motion_vectors_zero_for_static_scene_and_exact_for_translation` (GPU readback).
  - `skinned_mesh_composites_with_meshlet_depth` (golden).
- **Implementation notes**
  - Metal mesh shaders: the object stage is `[[object]]` and the mesh stage `[[mesh]]` with `mesh_grid_properties`. Respect the device's `max_mesh_output_*` caps. Slang's Metal mesh-shader support must be verified early (see Risks). If it is missing, a small hand-written MSL shim is allowed only for the stage entry points, behind `concordia-shaders`.
  - References: Apple WWDC22 "Transform your geometry with Metal mesh shaders"; Khronos `VK_EXT_mesh_shader` spec; Microsoft DirectX mesh shader spec; Kubisch, "Introduction to Turing Mesh Shaders" (NVIDIA 2018); Haar & Aaltonen, "GPU-Driven Rendering Pipelines" (SIGGRAPH 2015 Advances); Wihlidal, "Optimizing the Graphics Pipeline with Compute" (GDC 2016).
- **Done criteria:** a glTF scene renders through meshlets on both draw paths with goldens passing; the editor's viewport has debug views for meshlets, triangles and overdraw (`render.set_debug_view`).

---

### Phase 4.4: GPU culling: frustum, normal cone and two-pass HZB occlusion
- **Objective:** Draw only what can be seen: instance culling, then per-meshlet frustum, backface-cone and HZB occlusion culling, with no false negatives.
- **Deliverables**
  - Instance culling (frustum, HZB), then meshlet culling (frustum, normal cone, HZB), in the object stage or in the compute fallback.
  - **HZB source: two-pass re-test (ADR-0100).** Pass 1 draws the meshlets visible last frame, tested against last frame's HZB → build the HZB from the new depth → pass 2 re-tests everything pass 1 culled against the new HZB and draws what has become visible → the final HZB is kept for the next frame and for VSM (4.7).
  - Per-view culling infrastructure (a view array) so VSM pages and multiple cameras reuse the same passes.
- **Tests first**
  - `two_pass_occlusion_has_no_false_negatives_on_camera_cut`: a camera teleports between frames; the image is FLIP ≤ 0.001 against a render with occlusion culling disabled, on the very first frame after the cut.
  - `two_pass_occlusion_has_no_false_negatives_with_moving_occluder` (scripted 120-frame sequence; every frame compared).
  - `occlusion_culls_at_least_90_percent_in_occluded_city_scene`: `FrameStats.meshlets_drawn / meshlets_after_frustum` ≤ 0.10 behind a wall.
  - `backface_cone_culling_is_conservative`: random views on a sphere; no visible triangle is lost (equal to a no-cone-culling render).
  - `hzb_mips_are_conservative_max_depth`: CPU check of each mip against its parent texels.
  - `gpu_culling_matches_cpu_reference`: for 50 fixed views, the GPU's surviving meshlet set equals a CPU frustum-and-cone reference (set equality via readback, occlusion off).
  - `compute_fallback_culls_the_same_meshlets_as_mesh_shaders` (masked caps).
- **Implementation notes**
  - References: Greene, Kass, Miller, "Hierarchical Z-Buffer Visibility" (SIGGRAPH 1993); Haar & Aaltonen 2015 (two-phase occlusion); Wihlidal 2016 (cone and cluster culling); Kapoulkine, meshoptimizer docs (cluster bounds and cones).
- **Done criteria:** the gate canyon scene (built procedurally by `concordia-gen` with fixed seeds) renders with stable meshlet counts on both draw paths; all tests are green.

---

### Phase 4.5: Meshlet LODs and HLODs (follow-up, not in the gate)
- **Objective:** Once culling is measured, add discrete per-meshlet LODs and HLOD proxies, so triangle cost falls with distance. This phase starts only after 4.4's numbers are in, and it is not part of the M4 gate.
- **Scope when it starts:** discrete LOD levels per mesh built with `meshopt_simplifyWithAttributes`, each split into meshlets; LOD selection per instance (or per meshlet group) by projected error; HLOD proxies per cell, cached in the DDC (see `later-backlog.md`). Its tests are written when the phase is refined; they include crack-free transitions and a "rendered triangles track screen size" check.
- **Out of scope:** virtualized geometry streaming from disk and a software rasterizer. Either needs its own ADR.

---

### Phase 4.6: Temporal AA and upscaling (own TAA/TAAU, MetalFX option)
- **Objective:** Provide the temporal resolve that VSM filtering, GI and micro-geometry all rely on, and render internally below output resolution.
- **Deliverables**
  - A `TemporalUpscaler` trait with `OwnTaau` (all backends) and `MetalFxTemporal` (Apple; through `objc2-metal-fx`, MIT/Apache) implementations, chosen by `render.temporal.backend = Own | MetalFx | Off`.
  - Own TAAU: Halton(2,3) sub-pixel jitter (8 or 16 phases), reprojection with the motion vectors from 4.3, history sampling with Catmull-Rom, YCoCg variance clipping, disocclusion detection from depth/velocity, reactive mask (written by VFX and particles), and luminance-weighted resolve. Upscale ratios are 1.0×, 1.5× and 2.0×.
  - Mip bias for texture sampling at the internal resolution (`log2(internal/output) − ε`).
  - Exposure and pre-exposure plumbing shared with GI.
- **Tests first**
  - `jitter_sequence_is_halton_2_3_and_zero_mean`.
  - `taa_static_scene_converges_to_ssaa_reference`: after 32 frames, FLIP ≤ 0.03 against a 16× supersampled render (`m4_taa/static_converged.png`).
  - `taau_1_5x_static_scene_within_tolerance_of_native_taa` (FLIP ≤ 0.04).
  - `taa_no_ghosting_after_disocclusion`: a moving occluder; the mean luminance error in the disoccluded region 4 frames later is below threshold.
  - `taa_reactive_mask_prevents_spell_particle_smearing` (golden sequence).
  - `metalfx_backend_matches_own_taau_within_loose_tolerance` (Apple only, skipped elsewhere with a reason): FLIP ≤ 0.06.
  - `upscaler_backends_are_interchangeable_at_runtime`: switching backends mid-run produces no validation errors and a valid frame.
  - `temporal_pass_budget` (perf): ≤ 0.8 ms at 1440p output.
- **Implementation notes:** Karis, "High Quality Temporal Supersampling" (SIGGRAPH 2014); Salvi, "An Excursion in Temporal Supersampling" (GDC 2016); Yang, Liu, Salvi, "A Survey of Temporal Antialiasing Techniques" (EG STAR 2020); Apple WWDC22 "Boost performance with MetalFX Upscaling" and the MetalFX docs.
- **Done criteria:** the canyon scene runs at 1706×960 internal → 2560×1440 output with both backends; goldens pass.

---

### Phase 4.7: Virtual shadow maps
- **Objective:** Page-based, cached, high-resolution shadows for the sun and many local lights. VSM replaces CSM for meshlet and forward geometry, and CSM stays as the fallback.
- **Note (2026-11-20, ADR-0014):** M10 plans a tiled shadow atlas for local lights that needs no meshlets. If it lands first, this phase reuses its light selection (`shadow_atlas::select`, which this phase's `many_local_lights_select_top_64_by_importance` tests), its invalidation model, its `render.stats` counters and `concordia-bvh`. Whether VSM local lights replace the atlas or become an opt-in tier above it is M10's open question 1.
- **Deliverables**
  - Virtual address space: 16k × 16k texels per virtual shadow map, 128 × 128 texel pages (a 128 × 128 page table). Physical page pool: a depth atlas (default 8192², 4096 pages; configurable).
  - **Directional light:** a clipmap of N levels (default 16), each 2× the extent of the previous, centered on the camera and snapped to page granularity so cached pages stay valid as the camera moves.
  - **Local lights:** spot lights get 1 virtual SM; point lights get 6 (cube faces). A resolution LOD is chosen from screen-space size; at most 64 shadowed local lights are chosen per frame by importance (screen coverage × intensity × "shadowed" flag). Spell lights default to unshadowed and can opt in.
  - **Page marking:** from the scene depth, every visible pixel marks the page it needs in each relevant light (with a mip/LOD bias), compacted into a request list.
  - **Caching:** pages persist across frames; invalidation comes from moved, added or removed instances (their prev/current bounds projected into light space) and from light changes. A static/dynamic split keeps two cached layers and composites only the dynamic one each frame.
  - **Rendering:** reuses the 4.4 per-view culling and the 4.3 draw paths (depth only) with page-rect culling, into physical pages.
  - **Sampling:** PCF in page space first; shadow-map ray marching (SMRT, a few steps along the light direction through the VSM for contact-hardening soft shadows) as stage 2 within this phase.
  - `render.shadows = Vsm | Csm` per project; CSM is auto-selected when caps lack the needed features or VSM is off.
- **Tests first**
  - `clipmap_level_selection_matches_texel_density` (CPU unit test: each pixel's level gives ≤ 1 shadow texel per screen pixel at the configured bias).
  - `clipmap_snapping_keeps_cached_pages_valid_under_camera_translation`: a 1 m camera move invalidates 0 static pages.
  - `page_marking_requests_only_pages_covering_visible_receivers` (debug readback vs a CPU reference over a depth buffer).
  - `moving_caster_invalidates_only_overlapping_pages` (counter check).
  - `vsm_direct_shadow_matches_path_traced_direct_lighting`: on `m4-lighting/sun_courtyard` and `spell_arena_direct`, VSM direct lighting vs `concordia-pathtrace` in direct-only mode has mean FLIP ≤ 0.05 (reference goldens; hard shadows).
  - `vsm_beats_csm_on_contact_shadows`: the FLIP error against the reference is lower for VSM than for CSM at the same scene (a guard against a loose tolerance passing a CSM-quality result).
  - `vsm_regression_goldens` (`m4_vsm/*.png`, tight).
  - `many_local_lights_select_top_64_by_importance` (CPU unit test).
  - `256_spell_lights_scene_within_vsm_budget` (perf): VSM marking + render + sample ≤ 2.5 ms on reference.
  - `physical_pool_exhaustion_degrades_to_coarser_level_not_artifacts`: with the pool shrunk 8×, the image stays artifact-free (no light leaks: compared against a lit-mask golden).
  - `vsm_falls_back_to_csm_when_disabled` (golden equals the M1 CSM golden).
- **Implementation notes**
  - References: Fernando, Fernandez, Bala, Greenberg, "Adaptive Shadow Maps" (SIGGRAPH 2001); Lefohn, Sengupta, Owens, "Resolution-Matched Shadow Maps" (ACM TOG 2007); Lauritzen, Salvi, Lefohn, "Sample Distribution Shadow Maps" (I3D 2011); Epic's public documentation and talks on virtual shadow maps (UE5 docs "Virtual Shadow Maps"; "Virtual Shadow Maps in Fortnite Battle Royale Chapter 4", Unreal Engine tech blog 2023); Haar & Aaltonen 2015 (virtual texturing of shadows in GPU-driven pipelines); Barrett, "Sparse Virtual Textures" (GDC 2008) for the page-table mechanics.
  - Implement only what those public sources describe; parameters are our own and calibrated by tests.
- **Done criteria:** the canyon and reference port arena scenes use VSM by default; the reference and regression goldens pass; budget tests pass on reference.

---

### Phase 4.8: Dynamic GI, stage A (DDGI-style probes); stage B scaffold
- **Objective:** Fully dynamic diffuse GI that stays robust with many fast-changing spell lights and can be measured against the path tracer.
- **Why stage A is DDGI-style probes (justification)**
  - *Isometric ARPG camera:* the view is a bounded, fixed-angle slab of world (about 40–80 m across) looking down. A camera-centered probe volume (default 32×8×32 probes at 1.5 m spacing, with one coarser cascade) covers everything visible with few probes and no screen-edge failures. Screen-space GI alone fails in exactly this case: emitters and occluders just off screen, or hidden under roofs and trees, still light the visible floor.
  - *Many dynamic spell lights:* each probe traces a fixed ray budget per frame and shades its hits with the **same light clusters and VSM** as the main view. GI cost therefore scales with probes × rays, not with the number of lights, and a fireball's bounce light appears without any per-light work. Direct light from spells stays in the clustered resolve; GI carries only the bounce.
  - *Robustness:* irradiance plus a depth-moments visibility test per probe (Chebyshev) controls leaking. Probe relocation and classification handle probes inside geometry. The method is well documented, simple, and runs on every RT-capable backend.
  - *Responsiveness:* standard DDGI hysteresis (about 0.97) lags behind spell flashes. We use the adaptive hysteresis from Majercik et al. 2021: lower hysteresis where the luminance change is large, plus a "light event" boost when a spell light spawns in or near a probe's cell.
  - *Limits that motivate stage B:* probes give low-frequency diffuse light only, with no fine contact GI and weak glossy GI. Stage B (screen-space radiance cache with world-space fallback, plus a surface cache) addresses this, after M4, if the gate's quality metrics or reference port art direction call for it.
- **Deliverables**
  - Ray tracing source: RHI ray queries over a **proxy BLAS per asset**, built from a simplified copy of the mesh (`meshopt_simplify`, error threshold for example 1% of bounds) so tracing cost is independent of the full-detail source. Metal ray tracing (hardware on Apple9+, the Metal intersector in software on Apple7/8); Vulkan `VK_KHR_ray_query`. Fallback: the compute BVH from `concordia-pathtrace`'s traversal library, factored into a shared `bvh` module *below* both crates (the oracle keeps its own instance and test).
  - Probe update: R rays per probe per frame (default 128, with rotation randomized each frame) → hit shading (albedo × (direct light from clusters + VSM + previous-frame probe irradiance for multi-bounce)) → octahedral irradiance (8×8 + border) and distance moments (16×16) atlases → adaptive-hysteresis blend.
  - Probe relocation and state (active/inactive/sleeping) and a camera-scrolling volume with re-use of scrolled probes.
  - Sampling in the resolve: trilinear probe blend with normal bias, view bias, Chebyshev visibility, and backface weighting.
  - Emissive surfaces (spell VFX meshes) are counted at ray hits.
  - **Stage B scaffold:** the `gi::Technique` trait and a `ScreenProbes` skeleton behind a feature flag with one golden-free smoke test. Entry criteria for building it are in Open questions.
- **Tests first**
  - `probe_octahedral_encoding_roundtrips` (unit test).
  - `chebyshev_visibility_rejects_probe_behind_wall` (unit test with synthetic moments).
  - `probe_relocation_moves_probes_out_of_geometry` (CPU unit test on SDF/ray test results).
  - `adaptive_hysteresis_reacts_within_4_frames_to_spell_light` (CPU model test plus GPU readback: after a light spawns, probe irradiance reaches 90% of its converged value within 4 frames; with the light static it is stable to ±1%).
  - `gi_furnace_test_no_energy_gain`: a closed white room with uniform emission; GI irradiance does not exceed the analytic infinite-bounce value.
  - `gi_cornell_spell_matches_path_traced_reference`: `m4-lighting/cornell_spell` mean FLIP ≤ 0.10 against `m4_reference/cornell_spell.png` (provisional).
  - `gi_iso_arena_32_spell_lights_matches_reference`: `m4-lighting/iso_arena` at the reference port's isometric camera, mean FLIP ≤ 0.10.
  - `gi_improves_flip_over_no_gi_by_40_percent`: on both scenes, FLIP(GI) ≤ 0.6 × FLIP(no GI, ambient term only). This keeps the reference tolerance honest.
  - `gi_light_leak_test_thin_wall`: a lit room next to a dark room; luminance in the dark room is ≤ 2% of the lit side.
  - `gi_temporal_stability`: a static scene's per-pixel luminance variance over 60 frames is below threshold (no flicker).
  - `gi_regression_goldens` (`m4_gi/*.png`, tight).
  - `gi_budget` (perf): probe update + sample ≤ 2.0 ms on reference in the canyon scene.
  - `gi_software_bvh_fallback_matches_ray_query_path` (masked `ray_query`): FLIP ≤ 0.02.
- **Implementation notes**
  - References: Majercik, Guertin, Nowrouzezahrai, McGuire, "Dynamic Diffuse Global Illumination with Ray-Traced Irradiance Fields" (JCGT 2019); Majercik, Müller, Keller, Nowrouzezahrai, McGuire, "Scaling Probe-Based Real-Time Dynamic Global Illumination for Production" (JCGT 2021); McGuire et al., "Real-Time Global Illumination using Precomputed Light Field Probes" (I3D 2017); Cigolle et al., "A Survey of Efficient Representations for Independent Unit Vectors" (JCGT 2014, octahedral mapping); for stage B: Wright, "Radiance Caching for Real-Time Global Illumination" (SIGGRAPH 2021 Advances) and Wright, Narkowicz, Kelly, "Lumen: Real-time Global Illumination in Unreal Engine 5" (SIGGRAPH 2022 Advances); Apple WWDC "Discover Metal ray tracing" talks; Khronos `VK_KHR_ray_query`.
- **Done criteria:** GI is on by default in the canyon, lighting and reference port arena scenes; reference and regression goldens and the budget pass.

---

### Phase 4.9: `concordia-graph` core: format, node library, type checker
- **Objective:** A text-first graph language that agents and humans both edit. It must have stable diffs, a node library generated from the engine's own types, and a type checker whose errors an agent can act on.
- **Deliverables**
  - `.graph.ron` format (canonical: nodes sorted by UUIDv7 id, edges sorted, and a **separate `layout` block** for positions, so semantic diffs never mix with cosmetic ones; the editor auto-lays out when layout is missing):
    ```ron
    Graph(
      id: "0193b0f2-...", kind: Behavior, version: 1,
      signature: (inputs: [], outputs: []),
      variables: [(name: "target", ty: "EntityId")],
      nodes: {
        "0193b0f2-...a1": (op: "ai.selector"),
        "0193b0f2-...a2": (op: "game.enemy_brain.choose_skill",
                           inputs: { "distance": Link("0193b0f2-...a3", "out") }),
      },
      exec: [("0193b0f2-...a1", "child0", "0193b0f2-...a2", "in")],
      layout: { "0193b0f2-...a1": (0, 0) },
    )
    ```
  - Graph kinds: `Event` (entry points `on_tick`, `on_event(...)`), `Function` (pure or impure, callable from other graphs and WASM), `Behavior` (behavior tree structure whose leaves are tasks or `Function` graphs), and `Utility` (considerations and response curves).
  - **Node library**, generated, not hand-written:
    1. Every WIT function in `concordia:game` (and the new `concordia:game/ai`) becomes a node.
    2. Every reflected component gets get/set nodes.
    3. Every Rust function marked `#[reflect(graph_node(category = "...", pure))]` becomes a node.
    4. A built-in core: flow (branch, sequence, for-each, gate), math, comparisons, conversions, variables.
    5. `concordia-ai` nodes (composites, decorators, tasks, considerations).
    Each node has a stable `op` id, typed pins, a doc string, and examples.
  - Type system: bool, i32, i64, f32, Vec2/3/4, Quat, EntityId, AssetRef<T>, String, enums and structs from reflect, List<T>, Option<T>; exec pins; limited generics (`T: Numeric`) monomorphized at check time.
  - Type checker producing `Diagnostic { code, node_id, pin, message, suggestion }` as JSON.
- **Tests first**
  - `graph_ron_roundtrips_canonically` (`insta` snapshot plus `proptest`: parse(print(g)) == g, and print is idempotent).
  - `layout_changes_do_not_change_semantic_hash`.
  - `node_library_snapshot_is_stable` (`insta` snapshot of the generated library; a change is an explicit review).
  - `every_wit_gameplay_function_has_a_node` (set comparison against the WIT package).
  - `reflected_component_get_set_nodes_have_correct_types`.
  - `type_checker_rejects_type_mismatch_with_node_and_pin`.
  - `type_checker_rejects_data_cycle` (a data-flow cycle without a delay/variable node).
  - `type_checker_rejects_unconnected_required_input`.
  - `type_checker_rejects_impure_node_in_pure_function`.
  - `type_checker_monomorphizes_numeric_generics`.
  - `type_checker_rejects_unknown_op_with_closest_suggestion` (edit distance; for agents).
  - `diagnostics_are_valid_json_schema` (the schema is generated from `concordia-reflect`).
  - `graph_kind_rules_enforced`: e.g. Behavior graphs need exactly one root, and Utility considerations output f32 in [0,1].
- **Implementation notes:** RON via `serde`; canonical printer shared with `concordia-scene` conventions. The node library is emitted at build time into `concordia-graph`'s generated module and as JSON through `graph.list_nodes`. References: Unreal's *public* Blueprint docs for UX expectations only (no code). Keep the checker a simple bidirectional checker with explicit annotations at graph signatures and variables.
- **Done criteria:** ten hand-written example graphs check clean; ten broken ones produce the expected diagnostics (snapshot).

---

### Phase 4.10: Graph compiler, runtime, hot reload and MCP authoring
- **Objective:** Compile checked graphs to fast, sandboxed and deterministic code, and let agents author them entirely through MCP as text.
- **Compiler choice: WASM components (via an in-process emitter), with an IR interpreter as oracle and debugger**
  - *Codegen to Rust* was rejected as the default. It needs a Rust toolchain on the author's machine and a rebuild per edit (seconds to minutes), it is not sandboxed (a graph from a mod or an agent could do anything), and it cannot hot-reload in a shipped game.
  - *A register VM* was rejected as the shipping path. It is a second runtime to secure and optimize, and it would be about 5–20× slower than Cranelift-compiled WASM for math-heavy graphs.
  - *A WASM component* reuses M2's host, WIT API, sandbox, fuel/epoch limits, determinism settings and hot reload. It runs near native speed (ADR-0003), works on platforms without JIT through Wasmtime's Pulley interpreter, and is emitted **in-process** with `wasm-encoder` plus `wit-component` (Apache-2.0/MIT): no external toolchain, so compiles take milliseconds.
  - The **IR interpreter** is kept anyway, as the differential-testing oracle and for editor single-stepping and breakpoints.
- **Deliverables**
  - Lowering: Graph → typed SSA-like IR (`concordia-graph::ir`) → WASM core module → component targeting world `concordia:game/gameplay` (plus `concordia:game/ai`). Optimizations: constant folding, dead-node elimination, and inlining of small Function graphs.
  - Behavior graphs lower to an `concordia-ai` BT asset (a flat node array, native runtime) plus a WASM component holding the leaf Function graphs and decorators' conditions.
  - Determinism: Wasmtime `cranelift_nan_canonicalization(true)`, no host time access (time comes from the fixed step), and RNG through the engine's seeded per-entity RNG.
  - Hot reload: a changed `.graph.ron` → check → compile → swap the component at the next fixed-step boundary, preserving graph variables by name and type.
  - Debug: `graph.trace` records executed nodes and pin values per tick for a selected entity (interpreter mode or instrumented WASM).
  - Engine API methods (MCP tools generated 1:1): `graph.list_nodes`, `graph.check`, `graph.compile`, `graph.apply` (a mutating structured patch: add/remove node, connect/disconnect, set literal, set variable; takes an `actor` and goes through `concordia-command`), `graph.trace`. Whole-file text edits by agents are also valid; `asset` hot reload picks them up, and `graph.check` validates them.
- **Tests first**
  - `compiled_graph_matches_interpreter` (`proptest`: random well-typed pure graphs with random inputs; outputs are bit-identical between the IR interpreter and compiled WASM, with NaN canonicalization on).
  - `compiled_graph_is_deterministic_across_runs_and_thread_counts` (world-state hash).
  - `compiled_component_validates_with_wasm_tools` (component validation in-process).
  - `graph_compile_time_under_50ms_for_500_nodes` (perf).
  - `graph_call_overhead_within_1_5x_of_equivalent_rust_wasm` (a bench graph vs a hand-written Rust WASM component doing the same thing).
  - `hot_reload_preserves_variables_with_same_name_and_type`.
  - `hot_reload_of_broken_graph_keeps_previous_version_and_reports_diagnostics`.
  - `graph_apply_is_a_transaction_with_actor_and_undoable` (`txn.undo` restores the file byte-for-byte).
  - `mcp_graph_tools_schemas_generated_from_reflect` (snapshot).
  - `agent_authors_graph_over_mcp_end_to_end` (integration: a scripted MCP client creates a Function graph from the node list, checks it, compiles it, attaches it to an entity, steps the sim and reads the result via `component.get`).
  - `graph_trace_reports_executed_nodes_in_order`.
  - `runaway_graph_is_stopped_by_fuel_limit_without_crashing_engine`.
- **Implementation notes:** `wasm-encoder` and `wit-component` from the Bytecode Alliance; Wasmtime config shared with `concordia-script-wasm`. References: the WebAssembly Component Model spec; Cytron et al. 1991 (SSA) for the IR; ADR-0003.
- **Done criteria:** graphs run in the reference port slice and hot-reload; an agent can do a full author → check → compile → run loop over MCP only.

---

### Phase 4.11: Editor graph panel and Studio skills
- **Objective:** A visual editor over the same text, and agent skills, so humans and agents work on the same graph files side by side.
- **Deliverables**
  - `editor/ui/src/panels/graph/`: React panel using React Flow (`@xyflow/react`, MIT). Node palette from `graph.list_nodes`, inline diagnostics from `graph.check`, edits through `graph.apply` (so every human edit is a transaction with `human:local`), live trace overlay from `graph.trace`, and a "view as text" split showing the canonical `.graph.ron`.
  - Agent edits show live, with role attribution (from the command log) highlighted on nodes.
  - Studio: `studio/skills/author-graph/` and `author-behavior/` (how to list nodes, write a graph, check and compile, attach, and write a replay assertion). `studio sync` lets Gameplay Programmer and Level Designer roles write `**/*.graph.ron`.
- **Tests first**
  - `graph_panel_renders_nodes_and_edges_from_fixture` (vitest).
  - `graph_panel_edit_sends_graph_apply_with_human_actor` (vitest with a mocked API client).
  - `graph_panel_shows_diagnostics_on_offending_pin` (vitest).
  - `graph_panel_reflects_agent_edit_within_one_second` (Playwright against a headless engine: an MCP client applies an edit and the panel updates).
  - `graph_panel_undo_round_trips_file_bytes` (Playwright plus a file check).
  - `studio_sync_emits_graph_skills_and_permissions` (`insta` snapshot of the generated `.claude/skills/*` and agent tool allowlists).
- **Implementation notes:** no editor-only logic; the panel is a pure Engine API client. Auto-layout uses `elkjs` (EPL-2.0, *editor-only tool, not runtime*; flag it for license review) or `dagre` (MIT, preferred).
- **Done criteria:** a human edits a graph the agent created, and both edits appear in the command log with the right actors.

---

### Phase 4.12: `concordia-ai` runtime: behavior trees, utility AI, perception, blackboard; LLM plugin interface
- **Objective:** Production runtime AI that needs no LLM, is deterministic and replayable, and stays within a CPU budget for hundreds of agents. It also defines an optional LLM extension point that a shipped game can use only with a provider the game developer chooses.
- **Deliverables**
  - `Blackboard`: typed keys declared per agent archetype (reflected), change observers, and scopes (per agent, per squad, global).
  - **Behavior trees:** an event-driven BT in a flat array layout. Composites (Sequence, Selector, Parallel with policies, UtilitySelector, RandomSelector with the seeded RNG); decorators (Condition with abort-self/lower-priority on blackboard change, Cooldown, Loop, TimeLimit, Inverter); tasks (MoveTo via `concordia-nav`, Wait, PlayAnim via `concordia-anim`, CastSkill, FaceTarget, SetBlackboard, RunGraph). Time-sliced ticking with a per-frame budget.
  - **Utility AI:** considerations with response curves (linear, polynomial, logistic, logit, piecewise-linear), the Infinite Axis Utility System compensation factor, momentum/inertia bonus, and weighted-random top-N selection.
  - **Perception:** sight (cone, range, LOS via a Jolt raycast, time-sliced), hearing (stimulus events with radius and falloff), damage stimuli, a memory with decay, and team/faction filters. Queries are batched.
  - **LLM plugin interface (opt-in):** `trait LlmProvider { fn submit(&self, req: LlmRequest) -> LlmTicket; fn poll(&self, t: LlmTicket) -> LlmPoll; }` with `PlannerRequest` (goal, observation summary, an **action schema generated from reflect**) and `DialogueRequest`. Rules:
    1. Never on the sim thread; results arrive asynchronously with a deadline.
    2. Outputs are validated against the action schema, and invalid output falls back to the BT.
    3. Every response is recorded in the replay log, so replays are deterministic without the provider.
    4. The default build has no LLM dependency and no network access.
    5. Providers are WASM components implementing a WIT interface (`concordia:ai-plugin/llm-provider`) or native crates behind a cargo feature.
    6. A shipped game uses a provider *the game developer* chose and bundled (e.g. `concordia-ai-llm-local`, a local model through a permissively licensed runtime). The developer's Claude subscription is never available at runtime, and docs say so.
  - `MockLlmProvider`: deterministic, scripted responses (for tests and CI).
  - `ai.inspect` (blackboard, active BT path, last utility scores, perception memory) and `ai.trace`.
- **Tests first**
  - `bt_sequence_selector_parallel_semantics` (table-driven unit tests, Colledanchise & Ögren semantics).
  - `bt_condition_abort_lower_priority_on_blackboard_change`.
  - `bt_tick_is_deterministic_across_thread_counts` (500 agents; world hash).
  - `bt_time_slicing_respects_budget` (perf: 500 agents ≤ 1.0 ms CPU per fixed step on reference).
  - `utility_response_curves_match_closed_forms` (`proptest`).
  - `utility_compensation_factor_matches_iaus_formula`.
  - `utility_selection_is_seeded_and_reproducible`.
  - `perception_sight_respects_cone_range_and_occlusion` (Jolt scene fixture).
  - `perception_memory_decays_and_forgets`.
  - `perception_hearing_event_reaches_only_agents_in_radius`.
  - `blackboard_schema_is_reflected_and_validates_writes`.
  - `llm_planner_output_validated_against_action_schema` (mock returns an invalid action → fallback BT runs; the diagnostic is logged).
  - `llm_timeout_falls_back_to_behavior_tree`.
  - `replay_reproduces_llm_decisions_without_provider` (record with the mock; replay with no provider; identical world hash).
  - `default_build_has_no_llm_or_network_dependency` (a `cargo tree` / `cargo-deny` check that `concordia-ai`'s default features pull in no HTTP or model-runtime crates).
  - `bt_graph_kind_compiles_to_bt_runtime` (Behavior `.graph.ron` → BT asset; matches the expected flat array snapshot).
- **Implementation notes:** References: Isla, "Handling Complexity in the Halo 2 AI" (GDC 2005); Champandard, AiGameDev behavior-tree articles (event-driven BTs, 2007–2012); Colledanchise & Ögren, *Behavior Trees in Robotics and AI* (CRC 2018); Mark & Dill, "Improving AI Decision Modeling Through Utility Theory" (GDC 2010); Mark, "Building a Better Centaur: AI at Massive Scale" (GDC 2015, IAUS); Graham, "An Introduction to Utility Theory" (Game AI Pro, 2013); Orkin, "Three States and a Plan: The AI of F.E.A.R." (GDC 2006) for the planner-plugin framing; Rabin (ed.), *Game AI Pro* perception chapters.
- **Done criteria:** BT, utility and perception examples run under replay; the mock LLM path is tested; `concordia-ai-llm-local` builds only when explicitly enabled.

---

### Phase 4.13: reference port integration: graph-scripted enemy and the visual showcase
- **Dependency (consistency pass):** the `caster` archetype is **not** in the reference port's vertical slice (moved from M2 to M5 by ADR-0048/0067; it has only melee enemies). It must be ported through the M3 Studio port ledger before this phase starts. If it hasn't been, run this phase against a slice melee enemy with melee-range assertions, and treat `caster` as a follow-up.
- **Objective:** Prove both tracks on the real game. A reference port enemy's behavior is re-authored as graphs and passes the port's replay assertions, and a reference port level is rendered with meshlets, VSM, GI and TAA.
- **Deliverables**
  - `caster` archetype behavior as graphs (in the port project folder (`CONCORDIA_PORT_PROJECT`)):
    - `enemy_caster.graph.ron` (Behavior): PATROL / CHASE / ATTACK positioning, leash check, preferred range.
    - `choose_skill.graph.ron` (Function): filter in-band, off-cooldown, positive-weight skills → seeded weighted pick, or `None` (the weapon fallback). This mirrors the behavior of the reference port's enemy-brain script, which the author owns, so this is ordinary porting rather than a clean-room concern.
    - `preferred_range.graph.ron` (Function): the **longest** band, never above `VisionRange`.
  - A swap so the slice can run the `caster` archetype with either the M2 Rust/WASM brain or the graph brain (`concordia.toml` per-archetype setting).
  - Translated `enemy_brain_tests` / `enemy_ai_tests` (from M2) run against the graph functions.
  - The replay `tests/replays/m4_caster_graph.replay.ron`: the player engages a caster pack at range, closes to melee, then retreats out of leash.
  - The reference port arena level imported with `meshlets: auto`, VSM, GI and TAA on; showcase goldens and perf.
- **Tests first**
  - `graph_choose_skill_passes_translated_enemy_brain_suite` (every translated case from the GDScript `enemy_brain_tests` passes against the compiled graph).
  - `graph_choose_skill_matches_rust_port_decisions` (`proptest`: random skill lists, distances, cooldowns and seeds; identical choice to the M2 port).
  - `graph_preferred_range_never_exceeds_vision_range` (property).
  - `caster_graph_replay_decision_trace_equals_rust_port_trace`: the same replay with the graph brain and the M2 brain gives an identical per-tick decision trace (state, chosen skill).
  - `caster_graph_replay_assertions_pass`: the playtest assertions in the replay hold (the caster holds at preferred range, casts `fireball` at least 3 times, never casts with cooldown > 0, never casts as the player (player HP/MP and cooldowns untouched by enemy casts), returns to PATROL after leash).
  - `port_arena_vsm_gi_golden` (regression goldens at the isometric camera).
  - `port_arena_within_frame_budget` (perf, reference).
- **Implementation notes:** decisions are emitted to the replay as `DecisionTrace` events by both brains, so the traces are directly comparable. The graph brain's RNG draws must happen in the same order as the Rust port's; this is a spec item in `choose_skill.graph.ron`'s doc string.
- **Done criteria:** all tests green; the graph brain is the default for `caster` in the slice.

---

## Milestone gate
**Command:** `just gate M4`, which expands to `cargo nextest run --profile gate -E 'binary(/^m4_/)'` and must run on the **reference Mac** (a self-hosted CI runner; see `tests/perf/reference.ron`). On non-reference hardware the perf assertions report `NotReference` and the gate fails with that reason. It never passes silently.

The gate is four test binaries in `tests/gates/`:

1. **`m4_meshlet_geometry.rs`**
   - `canyon_has_at_least_20m_source_triangles`: loads `examples/m4-canyon` (procedural via `concordia-gen`, fixed seeds, CC0) and asserts the sum over instances of source triangles is ≥ 20,000,000 (provisional; raised when meshlet LODs land in 4.5).
   - `canyon_culling_draws_at_most_25_percent_of_meshlets`: along both camera paths, `meshlets_drawn / meshlets_total` ≤ 0.25 (provisional).
   - `canyon_within_frame_budget_on_reference`: both camera paths (reference port isometric camera, and a ground-level fly-through), 2560×1440 output from 1706×960 internal (TAAU 1.5×), with meshlets + VSM + DDGI + TAA all on: **GPU frame time median ≤ 16.6 ms and p95 ≤ 18.0 ms**, and every pass within its budget in `tests/perf/budgets/m4-canyon.budget.ron` (provisional table below).
   - `canyon_no_culling_holes`: no frame along the paths differs from a render with culling disabled (FLIP ≤ 0.001), so culling never drops visible geometry.
2. **`m4_lighting_quality.rs`**
   - For `sun_courtyard`, `spell_arena_direct`, `cornell_spell` and `iso_arena` in `examples/m4-lighting/`: VSM-only direct lighting is within mean FLIP ≤ 0.05 of the path-traced direct-only reference; full VSM + GI is within mean FLIP ≤ 0.10 of the path-traced full reference (`tests/golden/m4_reference/`); and `FLIP(GI) ≤ 0.6 × FLIP(no GI)`.
3. **`m4_fallbacks.rs`**
   - With caps masked to (a) no mesh shaders and (b) no ray query, the canyon gate view renders within FLIP ≤ 0.01 of the full-caps image, with no validation errors. (Perf is not asserted for fallbacks; their timings are recorded.)
4. **`m4_graph_enemy.rs`**
   - `caster_graph_replay_assertions_pass` and `caster_graph_replay_decision_trace_equals_rust_port_trace` (Phase 4.13), run headless through the Engine API, with the enemy's behavior loaded from `.graph.ron` compiled by `concordia-graph`.
   - `graph_authored_over_mcp_is_used_in_replay`: before the replay, the test authors a one-node change (e.g. the preferred-range margin literal) via `graph.apply` with `agent:gameplay-programmer#1`, asserts the change is in the command log with that actor, then reverts it via `txn.undo` and asserts the file is byte-identical.

**Provisional per-pass budgets** (reference Mac, 1706×960 internal, 2560×1440 output; median ms):

| Pass | Budget |
|---|---|
| `meshlet.cull` (instance + meshlet, both HZB passes) | 1.0 |
| `meshlet.draw` (Forward+ shading of surviving meshlets: PBR + clustered lights) | 5.0 |
| `meshlet.hzb` | 0.3 |
| `vsm.mark + vsm.render + vsm.sample` | 2.5 |
| `gi.probe_update + gi.sample` | 2.0 |
| `forward_plus` (skinned, translucent, VFX) | 1.2 |
| `temporal.taau` | 0.8 |
| `post` (tonemap, bloom, UI composite) | 0.8 |
| headroom | 2.0 |
| **total** | **16.6** |

## Risks & mitigations
| Risk | Mitigation |
|---|---|
| **Clean-room contamination** (Nanite, Lumen and VSM are well known; contributors or agents may have seen Unreal source) | Every phase cites its public sources in `AGENTS.md`. Agents are never given Unreal source. Reviewers check that the references cover each algorithm. Anyone who has studied the UE5 renderer source does not write these modules. |
| Reference Mac too slow for the canyon + VSM + GI at 16.6 ms | Budgets are recalibrated after M3 (the refinement pass); the internal resolution and TAAU ratio are the knobs, recorded in the budget file. The gate fixes the *triangle count* and *quality*, and the resolution may be renegotiated only by ADR. |
| No mesh shaders on a backend or device | The compute + indirect fallback is gate-tested via cap masking (ADR-0100). |
| Culling alone isn't enough for dense scenes without LODs | The gate's triangle count is set for culling only and is provisional; meshlet LODs and HLODs (4.5) are the planned follow-up. |
| Slang's Metal mesh/object stage support is immature | Spike in the first week of 4.3. The compute fallback works without mesh shaders meanwhile. If it falls short, use a minimal MSL entry-point shim inside `concordia-shaders`, with the bodies still in Slang; report or fix upstream (Slang is Apache-2.0 with LLVM exception). |
| DDGI lag or leaks under rapid spell lights | Adaptive hysteresis and light-event boosts are tested (`adaptive_hysteresis_reacts_within_4_frames...`); leak tests; stage B is ready as a follow-up. |
| Path-traced reference disagrees with the renderer because of BRDF mismatch rather than GI error | Shared, furnace-tested BRDF module; direct-only reference comparisons isolate shadow error from GI error. |
| FLIP tolerances that are too loose to mean anything | Paired "beats the baseline" tests (`vsm_beats_csm...`, `gi_improves_flip_over_no_gi...`). |
| Perf tests flaky (thermal, background load) | Warm-up, median/p95 over 300 frames, a dedicated runner with Low Power Mode off, three retries allowed only for perf, with every retry's results logged. |
| Graph compiler/interpreter divergence | Differential property tests (4.10) run in CI on every change. |
| Graph format churn breaks agent-authored graphs | A `version` field plus `concordia graph migrate`; the node-library snapshot test forces explicit review of op changes. |
| LLM plugin creeping into the default path | The `default_build_has_no_llm_or_network_dependency` test, and `cargo-deny` rules. |
| Large DDC sizes for dense scenes | Instancing in the gate scene; meshopt vertex and index codecs; DDC garbage collection. |

## Open questions
1. **Which Mac is the reference?** The plan assumes an Apple9+ (M3/M4-class) Pro or Max GPU. Confirmed: the author's M3 Pro (Apple9), which has Metal 3 mesh shaders.
2. Is 1706×960 → 2560×1440 the right gate resolution, or should the gate target 1080p output for the first pass?
3. Should VSM fully replace CSM as the default for projects that do not use meshlets? (Proposed: yes on Apple9+, CSM elsewhere until measured.) For local lights, see also M10's tiled shadow atlas (ADR-0014, its open question 1).
4. **GI stage B entry criteria:** start stage B (screen-space radiance cache + surface cache) only if (a) the reference port's art review flags missing contact or glossy GI, or (b) `iso_arena` FLIP cannot get under 0.08 with DDGI tuning. Should stage B be M4.5 or go into `later-backlog.md`?
   *Answered (2026-11-20, ADR-0013):* stage B is its own milestone, **M9 Advanced lighting (Photon)**. M4 keeps the `gi::Technique` seam, the proxy BLAS and the probe volume general enough for Photon to build on (its radiance cache may reuse the probes).
5. Can the proxy BLAS for GI and the path tracer share an acceleration-structure build without compromising oracle independence? (Proposed: share the builder library, keep separate instances and separate tests.)
6. Should Behavior graphs replace a dedicated BT editor entirely, or does `concordia-ai` also want a compact BT text view? (Proposed: one format, `.graph.ron`, with an editor *view mode* for trees.)
7. Which local-model runtime does `concordia-ai-llm-local` wrap (license, Metal support, model size)? It is out of the gate and only needs an interface-level decision in M4.
8. New crates `concordia-cluster` (now meshlet building only, ADR-0100), `meshopt-sys`, `concordia-pathtrace` and `concordia-ai-llm-local`: approve them during the consistency pass, or fold `concordia-cluster` into `concordia-assets` (it is small now) and `concordia-pathtrace` into `concordia-render` (weaker oracle isolation)?
9. Is `objc2-metal-fx` (or equivalent) mature enough, or do we write a thin binding ourselves?

## ADRs to write
- **ADR-0100: Meshlet geometry pipeline** (written, Proposed). Offline meshlets via meshoptimizer, bindless buffers, per-meshlet frustum/cone/two-pass HZB culling in mesh shaders (Metal 3 first) with a compute + indirect fallback, drawn into Forward+; no software raster, visibility buffer or streaming; LODs and HLODs later.
- **ADR: Shadows: VSM default, CSM fallback.** It must also place M10's local-light atlas (ADR-0014).
- **ADR: GI staging: DDGI-style probes first, radiance cache later**, with the isometric/spell-light justification and stage B entry criteria.
- **ADR: Graph compilation target: WASM components plus an IR interpreter as oracle** (alternatives: Rust codegen, register VM).
- **ADR: Runtime AI and the LLM plugin policy.** No LLM by default; opt-in providers chosen by the game developer; replay recording of responses; never the developer's subscription.
- **ADR: Perf testing on reference hardware.** Reference machine definition, budget files, `NotReference` semantics, and the self-hosted runner.
- **ADR: Quality methodology.** The path-traced oracle, reference vs regression goldens, and "beats baseline" tests.

## Effort estimate
Rough, for one lead plus AI agents, in engineer-weeks (focused). Track G runs in parallel with Track R.

| Phase | Weeks |
|---|---|
| 4.1 Harness, caps, path tracer | 3–4 |
| 4.2 Meshlet build | 1–2 |
| 4.3 Scene buffers, mesh-shader and compute draw paths | 3–4 |
| 4.4 Culling, two-pass HZB | 2–3 |
| 4.5 Meshlet LODs & HLODs (follow-up, not in the total) | (3–4) |
| 4.6 TAA/TAAU, MetalFX | 2–3 |
| 4.7 Virtual shadow maps | 5–6 |
| 4.8 DDGI (+ stage B scaffold) | 4–6 |
| 4.9 Graph core | 2–3 |
| 4.10 Graph compiler & MCP | 3–4 |
| 4.11 Graph panel & skills | 2–3 |
| 4.12 `concordia-ai` | 3–4 |
| 4.13 reference port integration & gate | 2–3 |
| **Total** | **32–45** (Track R ≈ 20–28 on the critical path; Track G ≈ 12–17 in parallel; 4.5 extra) |

---

## References (appendix)
All of these are public. None are, or contain, Unreal Engine source code.

**Meshlet geometry and GPU culling** (Nanite talks are for comparison only, concepts only)
- Karis, B., Stubbe, R., Wihlidal, G. "A Deep Dive into Nanite Virtualized Geometry." SIGGRAPH 2021, Advances in Real-Time Rendering in Games.
- Kapoulkine, A. meshoptimizer (MIT): README and API docs, `demo/clusterlod.h`. https://github.com/zeux/meshoptimizer
- Kapoulkine, A. "Billions of triangles in minutes." zeux.io, 2025. https://zeux.io/2025/09/30/billions-of-triangles-in-minutes/
- Garland, M., Heckbert, P. "Surface Simplification Using Quadric Error Metrics." SIGGRAPH 1997.
- Hoppe, H. "Progressive Meshes." SIGGRAPH 1996.
- Haar, U., Aaltonen, S. "GPU-Driven Rendering Pipelines." SIGGRAPH 2015, Advances in Real-Time Rendering.
- Wihlidal, G. "Optimizing the Graphics Pipeline with Compute." GDC 2016.
- Greene, N., Kass, M., Miller, G. "Hierarchical Z-Buffer Visibility." SIGGRAPH 1993.
- Kubisch, C. "Introduction to Turing Mesh Shaders." NVIDIA Developer Blog, 2018.
- Khronos. `VK_EXT_mesh_shader` and `VK_KHR_ray_query` specifications.
- Microsoft. DirectX mesh shader specification.
- Apple. "Transform your geometry with Metal mesh shaders" (WWDC22); *Metal Feature Set Tables* and *Metal capabilities by GPU family* (developer.apple.com/metal/capabilities), including mesh shader availability.

**Shadows**
- Fernando, R., Fernandez, S., Bala, K., Greenberg, D. "Adaptive Shadow Maps." SIGGRAPH 2001.
- Lefohn, A., Sengupta, S., Owens, J. "Resolution-Matched Shadow Maps." ACM TOG 2007.
- Lauritzen, A., Salvi, M., Lefohn, A. "Sample Distribution Shadow Maps." I3D 2011.
- Barrett, S. "Sparse Virtual Textures." GDC 2008.
- Epic Games. "Virtual Shadow Maps" (public Unreal Engine 5 documentation), and "Virtual Shadow Maps in Fortnite Battle Royale Chapter 4" (Unreal Engine tech blog, 2023). Technique descriptions only.

**Global illumination**
- Majercik, Z., Guertin, J.-P., Nowrouzezahrai, D., McGuire, M. "Dynamic Diffuse Global Illumination with Ray-Traced Irradiance Fields." JCGT 8(2), 2019.
- Majercik, Z., Müller, T., Keller, A., Nowrouzezahrai, D., McGuire, M. "Scaling Probe-Based Real-Time Dynamic Global Illumination for Production." JCGT 10(2), 2021.
- McGuire, M., et al. "Real-Time Global Illumination using Precomputed Light Field Probes." I3D 2017.
- Cigolle, Z., et al. "A Survey of Efficient Representations for Independent Unit Vectors." JCGT 2014.
- Wright, D. "Radiance Caching for Real-Time Global Illumination." SIGGRAPH 2021, Advances in Real-Time Rendering.
- Wright, D., Narkowicz, K., Kelly, P. "Lumen: Real-time Global Illumination in Unreal Engine 5." SIGGRAPH 2022, Advances in Real-Time Rendering.
- Pharr, M., Jakob, W., Humphreys, G. *Physically Based Rendering*, 4th ed., 2023.
- Veach, E. *Robust Monte Carlo Methods for Light Transport Simulation*. PhD thesis, Stanford, 1997.
- Kulla, C., Conty, A. "Revisiting Physically Based Shading at Imageworks." SIGGRAPH 2017 Course.
- Andersson, P., et al. "FLIP: A Difference Evaluator for Alternating Images." HPG 2020.
- Apple. "Discover Metal ray tracing" / "Your guide to Metal ray tracing" (WWDC20–23).

**Temporal**
- Karis, B. "High Quality Temporal Supersampling." SIGGRAPH 2014, Advances in Real-Time Rendering.
- Salvi, M. "An Excursion in Temporal Supersampling." GDC 2016.
- Yang, L., Liu, S., Salvi, M. "A Survey of Temporal Antialiasing Techniques." Eurographics STAR 2020.
- Apple. "Boost performance with MetalFX Upscaling" (WWDC22), and the MetalFX framework docs.

**Graphs and runtime AI**
- WebAssembly Component Model and WIT specifications; Bytecode Alliance `wasm-encoder`, `wit-component`, Wasmtime (incl. Pulley) docs.
- Cytron, R., et al. "Efficiently Computing Static Single Assignment Form and the Control Dependence Graph." ACM TOPLAS 1991.
- Isla, D. "Handling Complexity in the Halo 2 AI." GDC 2005.
- Champandard, A. J. Behavior-tree articles, AiGameDev.com, 2007–2012.
- Colledanchise, M., Ögren, P. *Behavior Trees in Robotics and AI: An Introduction.* CRC Press, 2018.
- Mark, D., Dill, K. "Improving AI Decision Modeling Through Utility Theory." GDC 2010.
- Mark, D. "Building a Better Centaur: AI at Massive Scale." GDC 2015.
- Graham, R. "An Introduction to Utility Theory." *Game AI Pro*, 2013.
- Orkin, J. "Three States and a Plan: The AI of F.E.A.R." GDC 2006.
- Rabin, S. (ed.). *Game AI Pro* series (perception and sensing chapters).
- React Flow (`@xyflow/react`, MIT) docs; dagre (MIT).

---

## Proposed convention additions
For the consistency pass in `plans/README.md`:

- **Crates:** `concordia-cluster` (meshlet building and per-meshlet bounds for GPU culling, M4, ADR-0100), `meshopt-sys` (meshoptimizer bindings, M4; vendored in `third_party/meshoptimizer`), `concordia-pathtrace` (reference path tracer, test oracle, M4; must not depend on `concordia-render`), `concordia-ai-llm-local` (opt-in example LLM provider; not a default workspace member).
- **Engine API / MCP methods:**
  - `graph.list_nodes`, `graph.check`, `graph.compile`, `graph.apply` (mutating, takes `actor`), `graph.trace`.
  - `ai.inspect`, `ai.trace`.
  - `render.stats`, `render.set_debug_view` (session-scoped, not a world mutation).
  - `perf.capture` (run a scripted camera path and return `.perf.ron`).
  - `capture.reference` (path-traced reference render).
  - MCP names follow the existing rule (`graph_apply`, `perf_capture`, …).
- **Files and directories:**
  - `tests/perf/reference.ron`, `tests/perf/budgets/<scene>.budget.ron` (perf budgets as text).
  - `target/perf/<scene>.perf.ron` (results; not committed).
  - `tests/golden/m4_reference/<scene>.png` + `<scene>.reference.ron` (path-traced references and their metadata).
  - `just bless-reference <scene>`.
  - `.graph.ron` gains a `layout` block that is excluded from the semantic hash.
- **Test conventions:**
  - Perf tests return `Pass | Fail | NotReference`; gate perf tests fail on `NotReference`.
  - A nextest `gate` profile.
  - The "reference vs regression golden" split, and a "beats baseline" companion test for every reference-tolerance test.
  - Cap-masking (`RhiCaps::masked`) as the standard way to test GPU fallbacks on one machine.
- **WIT:** `concordia:game/ai` (blackboard and perception for scripts and graphs), and `concordia:ai-plugin/llm-provider` (opt-in provider plugins).
- **Import settings:** `meshlets: true | false | auto` in `<file>.import.ron`.
- **Reflect attribute:** `#[reflect(graph_node(category = "...", pure))]`.
