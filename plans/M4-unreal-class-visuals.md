# M4: Unreal-class visuals
Status: In progress (4.1a, 4.1b1–b4, 4.1v(a), 4.1v(b) and 4.1v(c) done (D4's spike answered: mesh shaders and ray queries run on lavapipe); 4.2 done (`m4/4.2-meshlets`, PR #107, merged: offline meshlets); 4.3a in progress (`m4/4.3-indirect`, PR #108); 4.1v(a)'s device tests await the author's re-test) · Depends on: M1 (render), M2 (scripting host, graphs' WIT API), M3 (replay assertions, Studio) · Detail level: **Refined after M3 (2026-11-24; R1–R5 and D1–D10 approved by the author)** · Estimated effort: 32–45 agent-assisted engineer-weeks (two parallel tracks; see Effort estimate)

> This plan has full phase breakdowns and test lists. Its numbers (budgets, FLIP tolerances, page sizes, meshlet sizes) are **provisional** and must be recalibrated in the refinement pass after M3, using measurements from the M1–M3 renderer on the reference Mac. Every recalibrated number lands in a text file (`tests/perf/budgets/*.budget.ron`, per-test tolerances), never in code.

## M4 refinement (2026-11-24, against the code after M3)
**Approved by the author on 2026-11-24.** Where it disagrees with the phase text below, this section wins.** The author answered five questions on 2026-11-24 (R1–R5). The recommendations D1–D10 were approved with it.

### Author's decisions (2026-11-24)
- **R1: the proving ground is a new enemy in Lamplight.** The reference port moved to M5 (ADR-0048, ADR-0067), and M5 depends on M4, so 4.13 can't use a ported enemy.
  - 4.13 and `m4_graph_enemy.rs` use an **original Lamplight enemy** whose brain is a graph.
  - The port's enemy (the `caster` archetype and the translated brain suites) moves to M5.
- **R2: rendering first.** The order is 4.1 (oracles), then 4.1v, 4.2, 4.3, 4.4, 4.6, 4.7 and 4.8. Track G (4.9–4.12) follows, and 4.13 joins the two.
- **R3: Vulkan is built in M4,** as the new phase **4.1v**. Every rendering feature from 4.3 on lands on Metal and Vulkan together.
- **R4: the Vulkan backend's `unsafe` works like Metal's.**
  - A new crate, `concordia-rhi-vulkan`, uses `ash` (MIT/Apache-2.0).
  - Its `unsafe` stays inside that crate, with a `// SAFETY:` comment on every block, and tests run under the Khronos validation layers.
  - This is an approved exception to "no new `unsafe` outside the -sys crates", recorded in the 4.1v ADR.
- **R5: meshoptimizer is vendored** under `third_party/meshoptimizer` (MIT) behind a new `meshopt-sys` crate, following the `jolt-sys` pattern.

### What exists (checked in code)
**RHI:**
- `DeviceCaps` (`concordia-rhi/src/caps.rs:58`) holds backend, bindless, `mesh_shaders`, `ray_tracing`, buffer limits and Metal feature sets. It has no masking, no timestamp, MetalFX or mesh-output fields, and `Requirement` has only `M1`.
- Metal family probing covers apple6–10, metal3 and metal4 (`rhi-metal/src/probe.rs`). `mesh_shaders` is set from `metal3`. The M3 Pro reports apple6–9, metal3 and metal4.
- **Timing:** whole-command-buffer GPU time only (`Device::submit_timed`). There are no counter sample buffers.
- **Indirect:** `DispatchIndirect` exists, but there is no indirect draw, no viewport/scissor command and no texture-to-texture copy.
- **Mesh pipelines:** `MeshPipelineDesc` exists, but Metal's `create_mesh_pipeline` returns `Unsupported`.
- There are no acceleration structures or ray queries.
- Bindless (argument buffers, tier 2) and readback exist.
- **No Vulkan backend.** `ash` isn't in the lock; only `Backend::Vulkan` and SPIR-V shader sources exist.

**Shaders:** Slang 2026.22 runs as `slangc`, and `Stage` is Compute, Vertex or Fragment only. Both MSL and SPIR-V targets exist.

**Render:**
- The render graph has shadow, forward, tonemap and readback passes. Cluster culling is recorded outside the graph, and the frame is one command list.
- CSM: 3 cascades, 2048², 60 m.
- 4× MSAA. There are no motion vectors, jitter or TAA.
- Fixed EV100 exposure (no auto- or pre-exposure). Tonemaps: AgX, PBR Neutral and ACES.
- `render.stats` reports whole-frame `gpu_ms` and counts.
- Debug views are `ViewMode` values (overdraw, cascades, clusters, LOD); there's no `render.set_debug_view`.

**Assets:**
- `MeshData::meshlets` is an empty `Vec<u8>` placeholder.
- The glTF importer's settings are `{}`.
- The DDC is keyed by importer, version, platform, settings and bytes.
- meshoptimizer isn't present.

**Goldens:**
- FLIP (`nv-flip`) with mean and p99 lives in `concordia-testkit/src/golden.rs`.
- **Tolerances are set in code** (`Tolerance::mean`), not in text.
- Suites are under `tests/golden/<suite>/`. `CONCORDIA_REQUIRE_GPU` is 1 on macOS CI and 0 on Linux.

**Perf:**
- There is no harness, budget file or `reference.ron`.
- What exists: `--bench-latency`, `just editor-bench`, and the criterion benches `many_lights`, `crowd_100k` and `skinned_crowd`.
- **`examples/m1-sponza-like` doesn't exist;** the examples are `m0-sandbox`, `m1-physics-stack`, `m2-lamplight` and `m2-scripting`.
- Measured on the M3 Pro, all wall clock or whole frame:
  - 1,024 lights at 1080p: 2.5–3.2 ms;
  - 1,000 skinned characters: 6.5 ms;
  - the M1 gate scene: GPU p50 4.88 ms;
  - 10k entities: GPU p50 4.36 ms.
- There's no per-pass data.

**Gen:** the `mesh` provider makes box, sphere, capsule, cylinder, torus, extrude, lathe, heightfield and SDF surface nets (≤ 256 grid points per axis). There are no rock or ruin generators.

**Graphs and AI:**
- Reflect is a derive for types only (`name`, `category`, `presentation`, `default`, `range`, `widget`); there are no function attributes. `schema_for` gives JSON Schema.
- The WIT world `concordia:game@0.1.0` has three functions (`log`, `describe`, `run`). It is a batch column protocol (ADR-0062), and effects return as commands.
- Wasmtime 51: NaN canonicalization and fuel are on, epochs are off, and there's no Pulley.
- Script instances are stateless: state lives in components and migrates through reflect. A reload may not change the module name, processors, phases or order.
- `wasm-encoder` and `wit-component` 0.263 are in the lock, but only transitively.
- `concordia-command` has entity edits only; **there are no undoable file-document edits** (ADR-0016's shared item).
- There is no AI code: no BT, utility, perception, blackboard or `DecisionTrace`.
- `NavAgent.target` with `Arrived`/`Failed` gives a MoveTo. `AnimPlayer` exists, but one-shot requests are M13.
- Lamplight has wisps and townsfolk and no enemies.
- **Replay v1** (`deny_unknown_fields`) can't carry extra per-tick events. The assertion language's `event(..)` sees signals.
- `physics.raycast` is single-ray, reachable from native processors, and not from WASM.
- **Editor:** panels are top-level folders under `editor/ui/src/` (there's no `panels/`). There's no `@xyflow/react`, dagre or elkjs. Playwright has 10 e2e specs.
- Studio skills are `crates/concordia-studio/roster/skills/concordia-*`.

### Decisions (D1–D10; recommended, approved 2026-11-24)
- **D1: numbers come from 4.1's measurements.** 4.1 measures the M1 gate scene and Lamplight per pass on the M3 Pro. The budgets and the canyon gate's 16.6 ms target are then re-derived into `tests/perf/budgets/*.budget.ron`, and the author approves the file in 4.1's review. Until then, every number below stays provisional.
- **D2: golden tolerances move to text.** Each suite gets a `tests/golden/<suite>/tolerances.ron`; `Tolerance::mean` in code remains only as the default. This happens in 4.1, so M4's reference and regression goldens follow the plan's rule.
- **D3: the M1 baseline scene is the M1 gate scene**, and `m1-sponza-like` is dropped. The path-traced baseline is blessed on it and on a Lamplight night view.
- **D4: Vulkan testing.**
  - Linux CI runs the Vulkan backend on **lavapipe** (Mesa's CPU Vulkan, MIT), so Linux renders goldens for the first time.
  - On the Mac, Vulkan runs through MoltenVK (Apache-2.0) for conformance only. MoltenVK has no mesh shaders, so the compute fallback is what it tests.
  - Vulkan goldens compare against the same blessed images as Metal, each with its own tolerance in `tolerances.ron`.
  - Perf is asserted only on Metal (the reference machine).
  - **A spike checks** whether lavapipe exposes `VK_EXT_mesh_shader` and `VK_KHR_ray_query` in the Mesa that the Ubuntu 24.04 runner ships. If it doesn't, those paths are tested on Linux through the compute fallbacks, and the gap is recorded.
    - *Answered (4.1v(a) and (c)):* lavapipe has both extensions (the CI probe record), and Slang 2026.22's mesh, amplification and ray-query SPIR-V compiles, passes the backend's checks and runs on it under validation (`concordia-rhi-vulkan/src/spike`). Mesh shaders and ray queries are tested on Linux directly; the compute fallbacks remain for MoltenVK (no mesh shaders).
- **D5: the indirect-draw and per-pass-timing RHI additions land in 4.1** (timing) and 4.3a (indirect draws, viewport/scissor, texture copy), on both backends. M10 and M11 reuse them (ADR-0014, ADR-0015).
- **D6: graph nodes come from what the engine really exposes.** Sources:
  - reflected components (get/set);
  - signals (raise/on);
  - queries;
  - a new `#[graph_node(category, pure)]` attribute macro on Rust functions in `concordia-reflect-derive`;
  - a built-in core;
  - `concordia-ai` nodes.

  "Every WIT function becomes a node" is dropped: the world has no gameplay functions. `every_wit_gameplay_function_has_a_node` becomes `every_reflected_component_and_signal_has_nodes`.
- **D7: a graph compiles to a gameplay module,** the same column protocol as scripts (ADR-0062).
  - An `Event` graph is a processor. Graph variables live in a reflected component (state only in components), so "preserve variables on hot reload" is the existing reflect migration.
  - Edits that add or remove a processor need a stop and play, as scripts do today; the check says so.
  - Randomness is the per-processor, per-tick `seed`. The plan's "per-entity RNG" is reworded.
  - The IR interpreter remains the oracle.
  - `wasm-encoder` and `wit-component` become direct workspace dependencies at the lock's 0.263.
- **D8: decisions are signals.**
  - Brains raise an `ai.Decision` signal (state and choice), so playtest assertions use `event(..)` and replays compare them through world hashes; there's no new trace type.
  - Replay **v2** adds one field, `external: [(tick, source, payload)]`, in 4.12 for recorded plugin (LLM) outputs. It reads v1 the way v1 reads v0.
- **D9: 4.10 owns undoable document edits** (`EditOp::Document`, with a byte-exact undo), and ADR-0016's M12 reuses them. `graph.apply` is its first user.
- **D10: per-backend RHI capability profiles.**
  - `RhiCaps` replaces `DeviceCaps`, keeping its fields and adding the plan's (timestamp granularity, MetalFX, mesh output limits), plus `masked()`.
  - The plan's family table (Apple7+ mesh shaders, Apple9+ hardware RT) is checked against `probe.rs`.
  - Apple10 is tested from a recorded probe fixture, since there's no device to test on.

### Phases, against the code after M3
Numbers stay as they are because other plans cite them. Each phase gets its own branch `m4/<phase>-…`, PR and As built.

**Track R:**
- **4.1a** `m4/4.1-caps-timing` (**done**, see *As built* below): `RhiCaps` and masking (and the MetalFX temporal-scaler probe, author decision 2026-11-24; the scaler itself stays in 4.6); Metal counter sample buffers at encoder boundaries (perf mode gives each timed pass its own encoder); `PassTimer`, `FrameStats` and `render.stats` per pass; `BudgetFile`, `tests/perf/` (runner, `reference.ron`) and `concordia perf run`; golden `tolerances.ron` (D2).
- **4.1b** (re-planned 2026-11-24, author decisions: **two path-tracer backends, GPU by default and CPU as the fallback, cross-checked**; a shared `concordia-render-types` crate; CPU-only games become M14, the last milestone): **4.1b1** `m4/4.1b1-shared-shading` (the types crate, the material Slang module owned by `concordia-material`, M14's outline); **4.1b2** `m4/4.1b2-pathtrace-cpu` (backend-neutral core: scene, two-level SAH BVH, sampling, the CPU backend and its tests); **4.1b3** `m4/4.1b3-pathtrace-gpu` (Slang compute backend, GPU-vs-CPU convergence, fallback); **4.1b4** `m4/4.1b4-references` (emissive in physical units, nits, with existing materials converted and an ADR (author decision 2026-11-24); `capture.reference` with `backend: auto|gpu|cpu`, `just bless-reference`, D3 baselines, D1 recalibration with Lamplight at 1.5×, ADR-0090 and ADR-0101). Originally `m4/4.1-pathtrace`: `concordia-pathtrace` with its own BVH, `capture.reference` and `just bless-reference`; the furnace, analytic, convergence and determinism tests; the baseline references (D3); the budget recalibration (D1).
- **4.1v** `m4/4.1v-vulkan-*` (new; ADR-0088):
  - **(a)** the `ash` device, queues, memory, buffers, textures, samplers, bindless (descriptor indexing), pipelines from Slang SPIR-V, readback and timestamps. The RHI conformance suite passes on lavapipe and MoltenVK.
  - **(b)** surfaces and the M1 renderer on Vulkan: forward+, CSM, tonemap, the M1 goldens within tolerance; Linux CI on lavapipe through the on-demand `vulkan` workflow with `CONCORDIA_REQUIRE_VULKAN=1` (4.1v(a) found that `CONCORDIA_REQUIRE_GPU` keeps meaning Metal, and the author made the workflow on demand).
  - **(c)** the D4 spike, and the editor viewport on Vulkan where it runs (**done**, see As built: the spike passes on lavapipe; the Linux editor's viewport on X11).
- **4.2** `m4/4.2-meshlets`: `meshopt-sys` (R5) and `concordia-cluster`; the glTF importer's `meshlets: auto|true|false` setting; the DDC; `concordia cluster stats`.
- **4.3a** `m4/4.3-indirect`: indirect draws, viewport/scissor and texture copy on both backends (D5); the GPU scene buffers; the compute and indirect meshlet path.
- **4.3b** `m4/4.3-mesh-shaders`:
  - a Slang mesh and amplification spike first;
  - the mesh-shader path on Metal (Apple7+) and Vulkan (`VK_EXT_mesh_shader`);
  - motion vectors, including for skinned meshes.
- **4.4** culling; **4.6** TAA/TAAU and MetalFX (`objc2-metal-fx`, or our own binding if it falls short; MetalFX stays Apple-only); **4.7** VSM (a: directional clipmap and caching, b: local lights, SMRT and the CSM fallback); **4.8** DDGI, with RHI acceleration structures and ray queries on both backends first, and the software BVH fallback. Each is split at review time if it grows past one PR.
- **4.5** stays out of the gate.

**Track G:**
- **4.9** graph core (D6);
- **4.10** compiler, runtime and MCP (D7, D9);
- **4.11** the graph panel at `editor/ui/src/graph/` (dagre, MIT) and the skills `concordia-author-graph` and `concordia-author-behavior`;
- **4.12** `concordia-ai` (D8; native processors use `PhysicsQueries` for line of sight, and graphs reach it through a `physics.raycast` node that lowers to a query command);
- **4.13** the Lamplight enemy (R1). Its design is written in 4.13's refinement and is original to Lamplight. Tests:
  - `graph_brain_matches_rust_brain_per_tick`: the same enemy written as a graph and as a hand-written Rust module gives identical world hashes on every tick over a replay; this replaces the port-trace test;
  - the playtest assertions;
  - a Lamplight night with meshlets, VSM, GI and TAA (goldens and perf).

### The M4 gate, revised
- `m4_graph_enemy.rs` uses Lamplight (R1).
- `m4_fallbacks.rs` adds **(c) the Vulkan backend** (lavapipe on Linux CI; MoltenVK locally) within its tolerance of the Metal image, with no validation errors.
- The perf assertions run on Metal on the reference Mac only. Elsewhere they report `NotReference`, as planned.
- `regression.yml` gains `just gate-tests M4`.

### ADRs
- **0088:** the Vulkan backend: `ash`, the `unsafe` exception (R4), lavapipe in CI.
- **0089:** perf testing on reference hardware (written, 4.1a).
- **0090:** quality methodology.
- **0091:** shadows (VSM default, CSM fallback, placing M10's atlas).
- **0092:** GI staging.
- **0093:** the graph compilation target (D7).
- **0094:** runtime AI and the LLM plugin policy (D8's replay v2).
- ADR-0100 moves from Proposed to Accepted when 4.3a lands.

### As built
**4.1a** (`m4/4.1-caps-timing`, 2026-11-24). Author decisions at planning: perf code in a new crate `concordia-perf`; per-pass timing only in perf mode; `objc2-metal-fx` now, for the MetalFX cap.
- **Caps (D10):** `RhiCaps` replaces `DeviceCaps` with every field kept, plus `object_shaders`, `hw_ray_tracing`, `metalfx_temporal`, `timestamp_granularity` and the mesh output limits. `CapsMask` / `masked()` only remove features, and `new_headless_masked` makes a Metal device that reports and refuses them. The Metal probe is now a `ProbeRecord` (`concordia gpu-probe --record`) and a pure `caps_from_record`. The family table, checked against `probe.rs`: mesh and object shaders with `metal3` (Apple7+, Mac2; 256 vertices / 512 primitives); ray-tracing API from `supportsRaytracing`; hardware RT from Apple9; MetalFX from `MTLFXTemporalScalerDescriptor.supportsDevice`; stage-boundary timestamps. Fixtures: apple9 recorded on the M3 Pro; apple7, apple8 and apple10 hand-written from Apple's tables (no such device to record).
- **Timing (D5's first half):** RHI timer scopes (`BeginTimer` / `EndTimer`, `GpuTiming`), on Metal through counter sample buffers at encoder boundaries, calibrated against the CPU clock. `PassTimer` wraps every graph pass and `lights.cluster`. Perf mode (`Renderer::set_perf_mode`, `RenderModule::with_perf_mode`, `[render] perf_mode`) gives `render.stats` `pass_timing` and `passes`. Measured overhead: +0.03% to +0.8% (median of 150 adjacent on/off frame pairs; the test's limit is 2%).
- **Perf harness (ADR-0089):** `concordia-perf` (`BudgetFile`, `ReferenceMachine`, `HostProbe`, `PerfReport`, `Verdict` with `NotReference` winning even before a budget exists, `PerfScene`, the runner); `concordia perf run` and `just perf`; `tests/perf/` with `reference.ron`, scenes `m1-gate` and `lamplight-night` (the town's edit world at night, along a chase-camera path), and budgets.
- **D1 (M3 Pro, 1280×720, three runs of 300 frames):** M1 gate scene frame median 2.17–2.45 ms (the refinement's 4.88 ms came from another path), `forward` 1.58–1.81, shadow cascades about 0.15 each; Lamplight night frame 1.90–1.96 ms, `forward` 1.67–1.72. Proposed budgets: the worst run × 1.25, rounded up to 0.05 ms; `lights.cluster` median only, because its p95 sometimes reads about 1.5 ms (the first encoder after the frame's uploads waits on them). **The author approved the budget files on 2026-11-24 (4.1a review).**
- **D2:** `tests/golden/<suite>/tolerances.ron` for all six suites, numbers moved unchanged; `Tolerance::mean` is gone and `Tolerance::default()` is the only tolerance in code (`golden_tolerances_live_in_text`).
- Not in 4.1a: `FrameStats.raster_path` (no meshlet path yet; masking is observed through refused features, perf mode's `unsupported` and unchanged pixels), the path tracer and recalibration (4.1b), and Vulkan timestamps (4.1v).

**4.1b1** (`m4/4.1b1-shared-shading`, 2026-11-24):
- `concordia-render-types` (new, GPU-free): the render components, `CascadeSettings`, camera math (`Projection::matrix` → `ProjectionError`, `view_matrix`), `candela` and the CPU tonemap. `concordia-render` re-exports them under the old paths, so scenes, schemas and snapshots are unchanged. The hygiene test `gpu_free_crates_keep_their_dependencies` keeps it free of the renderer, the RHI and the shader crate; 4.1b2 adds `concordia-pathtrace` to it.
- `concordia-material/shaders/material.slang`: the slab BSDF and the program combinators, moved out of `forward.slang`, with the albedo tables behind `IAlbedoTables` (the renderer's `FrameTables`). `concordia_render::include_dirs()` serves every render shader compile. It compiles without the renderer (`tests/shader_module.rs`, Metal and SPIR-V), and every render golden and shader-vs-CPU comparison is unchanged.
- A mutation check found that no test covered `fresnel`'s Schlick term: the only featured slab at grazing light was metallic. `shader_matches_cpu_reference_at_grazing_light` gained a hazy dielectric and now catches it.
- `plans/M14-cpu-only.md`: CPU-only games, the last milestone.

**4.1b2** (`m4/4.1b2-pathtrace-cpu`, 2026-11-24):
- `concordia-pathtrace`: a backend-neutral core (rays, Möller–Trumbore, a binned-SAH `Tree` flattened depth-first, the per-mesh `Bvh` and the instance level `Accel`, counter-based `SampleStream`s, `EnvMap`, `Light`, `Scene` / `SceneBuilder`) and the CPU backend `render_cpu` (NEE for every light and the sun, one sky sample and one emitting triangle with MIS, a cosine/GGX mixture over the slabs, alpha cut-outs and blending, Russian roulette, rows in parallel). `scene_from_world` reads a world plus its assets the way the renderer would.
- Shared rather than copied: the light falloff (`distance_window`), `SpotCone`, the equirect mapping and the camera rule (`pick_camera`) in `concordia-render-types`; `AssetSource` and `TextureData::linear_rgb` in `concordia-assets`. The renderer uses them too.
- Tests: BVH and instancing against brute force (proptest); the white furnace (Lambert within 0.14%, rough GGX against `dfg::direct_albedo`); a disk light against closed-form geometry (1%); point, spot and sun exactly (0.1%); the convergence slope; bit-identical at 1 and 8 threads; worlds, views, errors, the sky and the clear color.
- Bugs found by tests: shadow rays measured from the surface but started off it stopped at an emitter's own plane (75% of area-light samples lost); no test covered `pick_camera`'s tie-break.
- Plan differences: the furnace uses a large quad seen at two angles (a tessellated sphere's shading normals would blur a 0.5% check); the convergence reference is 16k spp on 8×8 pixels (64k was too slow for every run).
- Emission was display-referred in the renderer (radiance = emission ÷ exposure), and the tracer matches it. **Author decision (2026-11-24): emissive becomes physical (nits) before 4.8, in 4.1b4**, with an ADR. Existing materials are converted once to look unchanged at their scene camera's exposure; unlit materials stay screen-referred.

**4.1b3** (`m4/4.1b3-pathtrace-gpu`, 2026-11-24):
- The GPU path tracer (`shaders/pathtrace.slang`, `GpuTracer`): the CPU algorithm step for step over the scene packed into `float4`/`uint4` rows; samples in batches of 16. `Backend::auto` picks the GPU when the tracer builds on a device, else the CPU; `render` dispatches.
- Shared rather than copied: the material row encoding (`concordia-material::rows`, moved from the renderer); `closure_factors` and `finish_closure` in `material.slang` (the renderer's `load_closure` now wraps its texture samples around them, goldens unchanged); `Target::from(Backend)` in `concordia-shaders`.
- Tests: every transport test on both backends (furnace, disk light, punctual lights, emission, sky and clear color, convergence, determinism); `gpu_and_cpu_tracers_converge` (statistical); `gpu_and_cpu_agree_on_layered_materials` (noise-free); the CPU fallback without a device. Every test passed on the first GPU run, so ten kernel mutations proved the GPU is exercised; one survivor showed the fixtures had no partly transparent slab, which was added.

**4.1b4** (`m4/4.1b4-references`, 2026-11-24):
- **Emission in nits (ADR-0102):** both renderers scale emission by exposure; the three emissive materials converted once (goldens and gates unchanged). Imported emission converts at `emissive_reference_ev100` (× 1.2 · 2^EV), null in new sidecars and filled by the project default, the main scene camera's EV (`AssetDb::set_setting_default`, `Project::main_camera_ev100`, `concordia_scene::read_components`).
- **`capture.reference`** (`PathTraceModule`, under `[render]`): capped at 1280 × 720 × 64 samples, GPU by default with a CPU fallback, PNG tone-mapped like `capture.screenshot`; the tech-artist role has it. Shared pieces: `register_components` in render-types, `EncodedImage::from_rgba8`.
- **Blessing (ADR-0090):** `ReferenceFile` and `Golden::bless_reference` in testkit (refused in CI, unblessed, or noisier than a fifth of the tolerance); the runner lives in the test-only `concordia-tests` (`src/reference.rs`), run by `just bless-reference <name>` as an ignored harness test (an hour's timeout), so testkit stays out of shipping crates. It writes the reference, the renderer's image and their FLIP map for review.
- **Textures in the tracer** (author decision after the first trial, where missing textures were most of the street view's 0.18 baseline): `TextureData::linear_rgba`, `MaterialProgram::textured` / `sampled_textures`, `SceneTexture`, per-hit shading on the CPU, and on the GPU a texel buffer with the same sampling rule plus `apply_texels` shared with the renderer. Normal and occlusion maps are not traced.
- **Baselines (approved 2026-11-24):** `m1_gate` 0.072, `lamplight_town` 0.102, `lamplight_street` 0.065 mean FLIP (noise ≤ 0.007), 1280×720 at 1024 spp. The remaining gaps are the renderer's: unshadowed moon, metal that can't reflect nearby geometry, shadow-map stair steps; the tracer shows the sky unfiltered (small).
- **Perf:** recalibration deferred by the author. Today's runs were disturbed by machine load: the 4.1a code itself measured Lamplight `forward` at 1.94–2.18 ms against 1.67–1.72 recorded, so budgets stay as approved in 4.1a. Canyon p95 raised to 19.0 ms.
- Mutation survivors and fixes: a two-camera case for the default reference EV; independent tests for the tone map and the chosen camera in `capture.reference`; an off-centre quadrature test for textured emitters (a symmetric one hid swapped barycentrics). Equivalent: lobes not re-derived from textured roughness (noise only).

**4.1b4 plan** (`m4/4.1b4-references`, 2026-11-24; the author's decisions on emissive units, imported emission, the baselines and reference access folded in):
Steps, each a commit and a push:
1. **Emissive in nits** (author decision 2026-11-24; ADR-0102):
   - `Slab::emissive × emissive_strength` becomes luminance in nits (cd/m²) in both renderers; exposure scales it like any light; it lights the world in the path tracer (and in GI later).
   - Unlit materials stay screen-referred.
   - Migration: the three emissive materials convert once at their scene camera's EV (factor 1.2 × 2^EV): M1 `emissive_glow` ×39 322 (EV 15), Lamplight `lamp_glass` and `wisp` ×1.2 (EV 0). Render and pathtrace tests that author emission convert the same way. Every golden must stay identical.
   - Importers (glTF, FBX) (author decision 2026-11-24): imported emission is screen brightness at a reference exposure, `emissive_reference_ev100` in `<file>.import.ron`, defaulting to the project's main scene camera EV (0, with a warning, without one); the importer converts with 1.2 · 2^EV. Imports look as they do today. Tests: `imported_emission_converts_at_the_reference_exposure`, `the_default_reference_is_the_main_cameras_ev`.
   - Tests first: `emission_is_scaled_by_exposure_like_light` (renderer, both tracers); `converted_materials_look_unchanged` (goldens).
2. **`capture.reference`** (author decision 2026-11-24: Engine API method from a `PathTraceModule` in `concordia-pathtrace`, enabled with `[render]`; through MCP capped: 64 spp preview by default, at most about a minute of work (a sample budget), scoped to the tech-artist and lighting roles; full quality only through `just bless-reference`): renders a world with the path tracer (`backend: auto|gpu|cpu`, `spp`, `seed`, `max_bounces`, `width`, `height`, camera or view) and returns a PNG tone-mapped like `capture.screenshot` (same tonemap, CPU reference). Tests: it matches `scene_from_world` + `render`; errors name missing cameras and assets; `backend: cpu` works without a GPU.
3. **`just bless-reference <scene>`** and `tests/golden/m4_reference/<scene>.reference.ron` (inputs: project, scene, view, size, spp, seed, max_bounces; results: backend, git hash, the renderer's FLIP against it = the baseline, the noise FLIP between two seeds). Blessing writes `<scene>.png` and never runs in CI. Tests: the file round-trips canonically; blessing is refused with `CI=true`; a reference whose noise FLIP exceeds a fifth of its suite's tolerance is refused.
4. **Textures in the tracer** (added 2026-11-24, author decision after the first baseline trial: without textures the Lamplight baselines measured missing paint, 0.18 FLIP on the street view, not lighting): base color (with alpha), emission, metallic-roughness and mix textures, sampled on both backends with the renderer's rule (bilinear, repeat, mip 0) and applied like its `load_closure` (`MaterialProgram::textured` on the CPU, the shared `apply_texels` in Slang). Normal and occlusion maps stay out (the tracer computes occlusion itself). Tests: sampling, unlit, lit, mask, mix, a textured emitter by quadrature, GPU against CPU sample for sample, and loading from worlds.
5. **Baselines (D3)** (author decision 2026-11-24): three references at 1280×720: the M1 gate camera, Lamplight's town camera over the square, and a Lamplight street-level view; shown on a review page (references, renderer images, FLIP heat maps, numbers) before they are committed.
6. **Recalibration (D1):** Lamplight budgets to a 1.5× margin; the plan's provisional canyon table re-derived from 4.1a's per-pass numbers. **Deferred by the author (2026-11-24)**: heavy perf testing waits for a later milestone and an idle machine (see As built); the canyon table was checked and its p95 raised.
7. **ADRs** 0090, 0101, 0102; docs, As built, checklist.

Out of scope: normal and occlusion maps in the tracer, skinning, representation LOD.

**4.1v(a)** (`m4/4.1v-vulkan-a`, 2026-11-24). Author decisions: Q2 `gpu-allocator`, Q3 home layouts plus global barriers, and **no TDD for the Vulkan device code** (written first, its tests after, re-tested on another machine later). The shared and pure pieces stayed test-first.
- **`concordia-rhi-vulkan`** (ADR-0088): `VulkanDevice` on `ash` 0.38 (`loaded`) and `gpu-allocator` 0.28, on Vulkan 1.3 core.
  - Device selection, from probe records by device type.
  - One universal queue and lazy retirement.
  - Bindless: set 0 a storage buffer of addresses, set 1 sampled images plus immutable samplers, set 2 storage images.
  - Heaps and placed textures, compute and graphics pipelines from checked SPIR-V, dynamic rendering with MSAA resolve, readback and timelines.
  - Timer scopes on timestamp queries.
  - Masking, a `ValidationLog` through debug-utils, and `Options { validation: Off | IfAvailable | Required }`.
  - Pure and tested on every OS: `convert`, `spirv`, `record` (caps), `track` (the planner, with a proptest) and `timing`.
- **Shared pieces:**
  - `concordia-shaders::conformance::SlangShaders` and `concordia-testkit::golden::RhiGoldens` (Metal's copies are gone); `conformance_tests!(…, finish = …)`; `vulkan_required()` (`CONCORDIA_REQUIRE_VULKAN`).
  - `Slots::drain`, and a backend-specific `Requirement::M1` message.
  - The conformance case `clip_space_y_is_up_and_back_faces_cull`, which runs on both backends.
- **The SPIR-V shader ABI:**
  - The buffer table is a storage buffer under `CONCORDIA_SPIRV`.
  - SPIR-V compiles keep source entry names (`-fvk-use-entrypoint-name`; Slang names them all `main` otherwise).
  - The shader cache key hashes each target's compiler arguments.
- **CI ran the device tests after all.** The `vulkan (lavapipe)` job (informational during this PR, on demand since 2026-11-25; moved from `ci.yml` to its own `vulkan.yml` when the hygiene test for ADR-0071 caught it) (`ubuntu-24.04`, Mesa lavapipe on LLVM 20.1.2, Vulkan 1.4.318, Khronos validation layers, `CONCORDIA_REQUIRE_VULKAN=1`) ran on every push:
  - First run: 53 of 63 passed; the other 10 failed on the entry-name bug.
  - Second run: 56 of 63 passed; the other 7 failed on one validation error, `DrawParameters` without `shaderDrawParameters`, which is now required. Their pixels had passed.
  - Third run: **64 of 64**, with no validation errors. The shared RHI goldens pass on lavapipe within the existing `tests/golden/rhi/tolerances.ron`, so there are no new goldens.
  - The lavapipe probe fixture is the record CI printed.
- **Bugs found by review before any run:**
  - `DEPTH_ATTACHMENT_OPTIMAL` needs `separateDepthStencilLayouts`, so depth attachments use `DEPTH_STENCIL_ATTACHMENT_OPTIMAL`.
  - `texture_heap_footprint` now checks format support first.
- **Tests written after their code or passing at once:**
  - Every `concordia-rhi-vulkan` test (by decision).
  - `msl_bindless_layout_is_unchanged`, and `clip_space_y_is_up_and_back_faces_cull` on Metal: both describe existing behaviour.
- **Test-first and seen failing for the right reason:** `vulkan_is_required_only_when_its_env_is_exactly_one`, `spirv_bindless_tables_use_sets_0_1_2`, `drain_takes_every_live_value_and_stales_their_handles`, `requirement_m1_names_the_backends_bindless_features`, `spirv_entry_points_keep_their_source_names` and `keys_cover_the_targets_compiler_arguments`. The `SlangShaders` tests were written first but failed only as part of the same compile error.
- **Mutation checks:**
  - 17 mutants, each restored with a fresh mtime, all killed by test failures rather than compile errors. They covered: the planner's global barrier, layout returns, aliasing and home precedence; copy layouts; SPIR-V stage matching; timestamp wrap; caps limits, device skipping and timestamp bits; the SPIR-V define, entry names and cache key; the SPIR-V buffer table; `drain`; the M1 message; and Metal's culling.
  - One survivor on the first pass: `drain` could leak its slots unnoticed. The test now checks that slots are reused.
  - Not run: mutants needing a Vulkan device (front face, the encoder's barriers, the address-table write). They wait for the re-test.
- **Deferred:**
  - Real GPUs: the author's Linux machine, and MoltenVK (portability enumeration and subset are wired but untested).
  - Whether the Vulkan workflow becomes required, or joins `regression`'s Linux job (author: later).
  - Checking `maxPerStageUpdateAfterBindResources`: the tables need 32 773 per stage; lavapipe and desktop drivers have far more.
  - Explicit-barrier fast paths and perf work.
  - Surfaces, the M1 renderer and `gpu-probe --backend vulkan` (4.1v(b)).

**4.1v(a) plan** (`m4/4.1v-vulkan-a`, 2026-11-24; ADR-0088, accepted; approved by the author on 2026-11-24 with Q2 `gpu-allocator` and Q3 home layouts plus global barriers. **Author decision: no TDD for Vulkan device code.** It is written first and its tests added after; they can't run on the author's Mac and are re-tested on another machine later. Non-Vulkan pieces (the shared harness, the testkit flag, the SPIR-V shader layout) and the pure modules stay test-first.):
Scope: the crate `concordia-rhi-vulkan` on `ash`, implementing the existing `Device` trait so the RHI conformance suite runs on it unchanged. Surfaces and the M1 renderer are 4.1v(b); the D4 spike and the editor viewport are 4.1v(c).

Facts found while planning (checked in code, 2026-11-24):
- **Metal hides synchronisation.** The Metal backend uses tracked hazards (RHI barriers only end an encoder), and the conformance cases rely on it: dispatch → copy, render → readback and upload → dispatch all run without barriers. Vulkan does no tracking, so the backend has to.
- **The buffer table can't be a uniform buffer on SPIR-V.** Slang lays `ParameterBlock<BufferTable>` (16 384 `uint*`) out as a std140 uniform block with a 16-byte stride: 256 KiB, above many drivers' 64 KiB `maxUniformBufferRange`. Declared as `StructuredBuffer<uint*>` on SPIR-V only, it becomes a storage buffer with an 8-byte stride at set 0, binding 0. Shaders keep `g_buffers.ptr[i]`, and the Metal output is unchanged (tried with `slangc` 2026.22).
- The texture tables already map cleanly: set 1 binding 0 (16 384 sampled images), set 1 binding 1 (4 samplers), set 2 binding 0 (storage images); push constants are `PushConstant` blocks.
- Metal's conformance harness (its Slang `ShaderProvider` and `GoldenCheck`) lives in `rhi-metal/tests/conformance.rs`. A second copy for Vulkan would duplicate it.
- `pipeline_creation_error_is_reported_not_panicked` hands Vulkan the single word `0xdeadbeef`; drivers may crash on malformed SPIR-V, so the backend checks the module before the driver sees it.

Design (ADR-0088):
- **Vulkan 1.3 core:** dynamic rendering (no `VkRenderPass`), synchronization2, timeline semaphores, descriptor indexing, buffer device addresses. lavapipe, MoltenVK and current desktop drivers all have them. A device without them is `NoDevice` with the missing features named.
- **Loading:** `ash` with `loaded`, so the loader is opened at run time. The crate builds everywhere, and without a loader (this Mac) every entry point is `NoDevice`.
- **Bindless:** one descriptor set layout per table (update-after-bind, partially bound), one pipeline layout per device (128 bytes of push constants for every stage), and immutable static samplers. The buffer table is a device-owned storage buffer of `VkDeviceAddress`es written when a bindless buffer is created.
- **Memory:** `gpu-allocator` (MIT OR Apache-2.0; Traverse Research), `Upload` → CPU-to-GPU, `Readback` → GPU-to-CPU, `GpuOnly` → device-local. A heap is one dedicated allocation; placed textures bind at an offset.
- **Hazards: home layouts plus a full barrier between steps** (author decision 2026-11-24, Q3). Each texture has a home layout from its usage: storage → `General`, sampled → `ShaderRead`, attachment-only → its attachment layout, copy-only → `General`. Each step (render pass, dispatch, copy) moves what it uses out of the home layout and back, and a global memory barrier separates consecutive steps. That serializes the steps as Metal's tracked mode does. RHI barriers matter only for aliasing (the texture's next use discards from `Undefined`); a later phase can make the render graph's barriers authoritative and drop the global ones (deferred; perf). *As built: new textures move to their home layout at the start of the next submission, so bindless-only writes never meet `Undefined`.*
- **Y up, as on Metal:** a negative-height viewport (core since 1.1), with the front face set so `CullMode::Back` keeps counter-clockwise triangles as it does on Metal.
- **Queues:** one universal queue. `QueueKind::Compute` and `Transfer` run on it until a phase needs async compute (as on Metal today).
- **Submission and retirement:** an internal timeline semaphore counts submissions. *As built: retirement is lazy, with no thread:* every submit, wait, timeline read and destroy retires what has finished, firing `submit_timed` callbacks, recycling command buffers and freeing resources destroyed while in use.
- **Timestamps (ADR-0089's scopes):** a query pool per timed submission, with `vkCmdWriteTimestamp2` at each scope edge and around the whole list, converted with `timestampPeriod` and relative to the first sample. Granularity is `draw_boundary` when the queue has `timestampValidBits` (`none` otherwise); `CapsMask::timestamps` masks it like Metal.
- **Mesh pipelines, ray tracing and surfaces:** `Unsupported` in (a), and the caps say false.
- **Validation:** the Khronos validation layer and a debug-utils messenger feed a `ValidationLog`. Tests fail on any error-severity message. Under `CONCORDIA_REQUIRE_VULKAN=1`, a missing layer or device fails instead of skipping.
- **`unsafe`** only in this crate (R4), with `#![deny(clippy::undocumented_unsafe_blocks)]` at the crate root. Raw `ash` calls stay in a few modules (`instance`, `device`, `memory`, `record`, `timing`); conversion, SPIR-V checks, caps and hazard planning are safe, pure and tested on every OS.

Testing:
- **Pure logic tests run everywhere**, including this Mac: conversions, the SPIR-V check, caps from probe records, the hazard planner (including a proptest).
- **Device tests** run the conformance suite plus Vulkan-only cases. Linux CI runs them on lavapipe through a new `vulkan (lavapipe)` job (Mesa's `mesa-vulkan-drivers`, `vulkan-validationlayers`) with `CONCORDIA_REQUIRE_VULKAN=1`. *As built: its own workflow, `.github/workflows/vulkan.yml`, on PRs touching the RHI, shaders, testkit or the backend, because `ci.yml` must stay fast (ADR-0071, `ci_workflow_runs_the_fast_checks_on_every_pr`; pinned by `vulkan_workflow_runs_the_backend_on_lavapipe_under_validation`). *After the PR's runs, the author made it on demand only (2026-11-25).*
- **On this Mac** the crate builds and its device tests skip (no loader). `CONCORDIA_REQUIRE_GPU=1` doesn't make them fail: it keeps meaning Metal.
- **No local red/green loop** (author decision 2026-11-24): no container. The `vulkan (lavapipe)` job runs **on demand only** (author decision 2026-11-25: `workflow_dispatch`, from Actions → vulkan → Run workflow or `gh workflow run vulkan --ref <branch>`, with an optional nextest `filter`; not required, revisited later).
- **Goldens:** the RHI goldens are shared across backends, with the same `tests/golden/rhi/tolerances.ron`. If lavapipe misses one, the images go on a review page before any per-backend golden or tolerance change. New images are collected with `INSTA_UPDATE=new` and never blessed in CI.
- **Deferred (author decision, 2026-11-24):** real GPUs, meaning the author's Linux machine and MoltenVK on the Mac. The plan's "conformance passes on lavapipe and MoltenVK" becomes lavapipe now and MoltenVK later. No perf work or budget changes.

Steps, each a commit and a push (device code first, then its tests, per the author's decision):
1. **Shared conformance harness** (no behaviour change):
   - Metal's Slang `ShaderProvider` moves into `concordia-shaders` (feature `conformance`, `SlangShaders::new(Target)`), and its `GoldenCheck` into `concordia-testkit` (`golden::RhiGoldens`).
   - `conformance_tests!` gains an optional per-case `finish` hook.
   - `concordia-testkit::vulkan_required()` reads `CONCORDIA_REQUIRE_VULKAN`, with the same "exactly `1`" rule.
   - Tests: `vulkan_required_only_for_exactly_1`, and the Metal suite unchanged.
2. **SPIR-V bindless ABI:**
   - `concordia-shaders` defines `CONCORDIA_SPIRV` for the SPIR-V target, and `bindless.slang` declares the buffer table as a storage buffer under it.
   - Tests first: `spirv_bindless_tables_use_sets_0_1_2` (reflection: set 0 binding 0 storage buffer; set 1 bindings 0 and 1; set 2 binding 0) and `msl_bindless_layout_is_unchanged`. Every Metal golden stays identical.
3. **Crate, loader and pure modules; CI and the container:**
   - `concordia-rhi-vulkan` (`ash`, `gpu-allocator`), with `probe()`, `new_headless()` and `new_headless_masked()`.
   - The `vulkan (lavapipe)` workflow (on demand).
   - Tests:
     - `formats_usages_and_states_map_to_vulkan` (every `Format`, `Layout`, `Stages`, `AccessFlags`, compare, cull and blend value);
     - `malformed_spirv_is_a_pipeline_compile_error` (magic, length, version, a missing entry point by name and stage);
     - `caps_from_records` (a recorded lavapipe probe and hand-written fixtures: no descriptor indexing, no timestamps, too few bindless slots);
     - `requirement_m1_names_vulkan_features`: `Requirement::M1`'s bindless message becomes backend-neutral;
     - `without_a_loader_the_device_is_no_device` (runs where no loader exists; under the require flag it asserts a device instead).
4. **Hazard planner** (pure): `plan(commands, homes) → steps with barriers`.
   - Example tests: `copy_after_dispatch_waits_for_shader_writes`, `render_targets_return_to_their_home_layout`, `copy_from_a_render_target_transitions_and_returns`, `aliasing_barrier_discards_from_undefined`, `draws_inside_one_pass_need_no_barrier` (as built also `msaa_resolve_targets_are_attachments_for_the_pass`, `copies_into_general_textures_stay_general`, `unknown_textures_are_reported`).
   - Property test: `every_command_sees_the_layout_it_needs`, which simulates layouts over random command lists.
5. **Device bring-up:**
   - Instance, validation messenger, device selection (discrete, then integrated, virtual, CPU), the universal queue, the allocator, the bindless sets and pipeline layout, timelines and the retire thread.
   - Conformance cases: `device_reports_caps_and_limits`, `timeline_fence_signals_monotonically`, `buffer_upload_and_readback_round_trips`, `invalid_descriptors_and_stale_handles_are_errors`.
   - Vulkan cases: `validation_errors_fail_the_test` (a test hook provokes one known error; the log must catch it) and `destroyed_resources_outlive_their_last_submission`.
6. **Compute:** compute pipelines from SPIR-V and bindless buffers. Cases: `compute_dispatch_writes_expected_values`, `compute_reads_bindless_buffer_by_index`, `pipeline_creation_error_is_reported_not_panicked`.
7. **Graphics:**
   - Textures, the static samplers, heaps and placed textures; graphics pipelines and dynamic rendering, with MSAA resolve, depth and blending.
   - Cases: `triangle_renders_golden`, `clear_color_readback_is_exact`, `depth_test_orders_overlapping_triangles`, `msaa4x_resolve_produces_antialiased_edge`, `barrier_between_compute_write_and_fragment_read_is_respected`, `transient_textures_aliased_in_same_heap_do_not_corrupt`, `static_samplers_filter_and_address_as_named`, `additive_blend_sums_overlapping_draws`, `readback_of_frame_n_does_not_block_frame_n_plus_1_submission`.
   - A new conformance case for both backends: `clip_space_y_is_up_and_back_faces_cull`.
8. **Timestamps:** `submit_timed_reports_the_gpu_time_once`, `timer_scopes_report_each_scope_once_in_order`, and `masked_timestamps_report_no_scopes` (Vulkan).
9. **Mutation checks, docs and the checklist:**
   - Mutation targets: the hazard planner's global barrier and layout returns, the buffer-table write, the front-face rule, timestamp ordering and the validation log.
   - Docs: ADR-0088 accepted, the As built, `plans/README.md`, the crate's `AGENTS.md` and README.

Out of scope: surfaces and present, the M1 renderer, `concordia gpu-probe --backend vulkan` (all 4.1v(b)); mesh shaders (4.3b); async queues; making the render graph's barriers authoritative; pipeline caches.

**4.1v(b)** (`m4/4.1v-vulkan-b`, PR #105, merged 2026-11-25 before its step 10; closed in 4.1v(c)'s step 0 on `m4/4.1v-vulkan-c`). Author decisions Q1 (`CONCORDIA_BACKEND` and `[render] backend`), Q2 (one `unsafe` block in `concordia-cli` for Linux windows) and Q3 (the whole render suite and the M1 gate golden on lavapipe); Vulkan device code written first and tested after.
- **Built** (ADR-0088 decisions 10–17):
  - `concordia-rhi-select` (backend choice, `CONCORDIA_BACKEND` over `[render] backend` over `auto`), with every device call site moved to it, and `concordia gpu-probe --backend` (and `--record`).
  - `SurfaceTarget::{Xlib, Xcb, Wayland, Headless}` (Metal refuses them) and `NativeView` from X11 and Wayland handles; Metal's surface tests became the shared surface conformance suite.
  - Vulkan surfaces and swapchains (a pure planner; sRGB over UNORM through `VK_KHR_swapchain_mutable_format`), the probe record's instance surface extensions.
  - Strict validation (`CONCORDIA_VULKAN_VALIDATION`), and a retire thread that fires timing and presentation callbacks as the GPU finishes.
  - Per-backend golden tolerances, `CONCORDIA_GOLDEN_KEEP=1`, and blessing refused on Vulkan.
  - The renderer through the selector: the render suite and the M1 gate golden on lavapipe; non-uniform indexing features; the SPIR-V pointer rule (`check_spirv_module`, checked for every render entry point on any OS).
  - `concordia run` in X11 and Wayland windows on Vulkan, with `WindowEvent::Closing` and `concordia_render::detach_surface`, and an Xvfb smoke test in the `vulkan` workflow.
- **Closed in 4.1v(c) step 0:**
  - **The glTF crash.** `gltf_metal_rough_spheres_golden` and `gltf_flight_helmet_golden` had never rendered on lavapipe (missing assets at 7a, then SIGSEGV from 7b on). 4.1v(b)'s last commit blamed the retire thread's timeline wait; the crash continued after that change and had begun before the retire thread existed. A `gdb` input for the workflow showed the fault inside lavapipe's non-uniform sampling loop (a lane's descriptor pointer null), with validation off and with LLVM's passes off; GPU-assisted validation hid it and reported nothing. Two fixes aimed at the wrong cause came first, each kept because it is right anyway: clamping sentinel indices into the table (`in_table`), and filling empty slots (null descriptors, then a fallback texture; ADR-0088 decision 18). Experiments on throwaway branches then settled it: a uniform material index left the crash, a uniform environment index removed it. The prefiltered environment levels are separate textures chosen per pixel by roughness, and lavapipe faults when neighbouring pixels sample different textures (likely a lavapipe bug, to report). `env_sample` now loops over the levels with a uniform index; Metal's images are pixel-identical before and after.
  - **`concordia run` crashed once at exit** (lavapipe's exit handlers racing the render and retire threads): it now drops its handle and waits for the engine thread (`concordia_api::join_engine`, test-first).
  - **Goldens** (run 45554457142, review page https://claude.ai/artifact/V6PnXY5Zho3QrVS7C7xm4C): all 43 render and gate goldens compared on lavapipe against the Metal goldens; 9 exceeded today's tolerance, mostly one-step sRGB rounding over flat areas. `tests/golden/render/tolerances.ron` gains `backends: { "vulkan": … }`: default mean 0.025, own entries `gltf_metal_rough_spheres` 0.030 (measured 0.0227), `gltf_metal_rough_spheres_no_textures` 0.035 (0.0243) and `view_mode_normals` 0.035 (0.0255), p99 0.25 throughout. Rule, as proposed on the first page: an entry where measured mean × 1.25 exceeds the tolerance that applies, rounded up to 0.005. The gates suite is unchanged. Committed on the author's standing delegation for this phase (2026-11-25); Metal's tolerances do not change.
  - **The `vulkan` workflow** green end to end on this code, and the lavapipe probe fixture refreshed from the printed record.
- **Test log.** The plan listed as test-first: the testkit tolerances and keep mode, `device_required(Backend)`, the selector (parsing, `auto` per OS, precedence, no loader), `gpu-probe --backend`, `SurfaceTarget` refusal on Metal, `NativeView` from X11/Wayland handles, the swapchain planner, the validation env and the manifest field. Each landed in the same commit as its code, and the session that wrote them did not record which it saw fail first, so this record cannot confirm the order. Written after their code, by decision: the Vulkan surface, strict-validation and retire-thread device tests, and the render suite on lavapipe. Passed at once: the shared surface cases on Metal (they describe Metal's existing behaviour).
- **Mutation checks** (run in 4.1v(c), each restored file touched): 13 mutants of 4.1v(b)'s key rules, all killed by test failures, none by compile errors: tolerance precedence (backend entry first; backend default before the shared one), keep mode, failures under `<suite>/<backend>/`, blessing refused on Vulkan, the env over the project setting, `auto` per OS, strict from the env, the planner's two images and its oversize refusal, the SPIR-V pointer rule, Metal refusing other surface targets (killed by its test crashing: the mutant treats an X11 pointer as a layer), and `NativeView` from Xlib handles. Not run: mutants of device code (they need a device; deferred with the re-test).
- **Deferred:** real GPUs and MoltenVK; `VK_KHR_present_wait`; exact present timing; making the workflow required.

**4.1v(b) plan** (`m4/4.1v-vulkan-b`, 2026-11-24; ADR-0088, amended; approved by the author on 2026-11-24 with **Q1** the backend chosen by `CONCORDIA_BACKEND` *and* a project setting `[render] backend` (env wins, then the manifest, then `auto`), **Q2** one `unsafe` block in `concordia-cli` for Linux windows, and **Q3** the whole `concordia-render` suite plus the M1 gate golden on lavapipe. Vulkan device code is written first and tested after, as in 4.1v(a); everything else is test-first.):
Scope: Vulkan surfaces and presentation through the existing RHI surface API, the M1 renderer on the Vulkan backend with its goldens on lavapipe, and `concordia gpu-probe --backend vulkan`. The editor viewport on Vulkan and the rest of the D4 spike are 4.1v(c).

Facts found while planning (checked in code, 2026-11-24):
- The renderer is backend-neutral: it takes any `Device` and picks SPIR-V or MSL from `caps().backend` (`render/src/pipelines.rs`). The RHI has no array or cube textures, so the renderer needs nothing 4.1v(a) lacks, as far as the code shows.
- About 18 places call `concordia_rhi_metal::new_headless` directly (the app's device factory, `gpu-probe`, render, UI and pathtrace tests, benches). Nothing chooses a backend.
- `SurfaceTarget` has only `MetalLayer`; `NativeView` (`concordia-platform`) maps only AppKit handles, though winit builds with x11 and wayland.
- The lavapipe probe record has `VK_EXT_mesh_shader` and `VK_KHR_ray_query` (D4's spike, half answered).

Design (ADR-0088, amended):
- **Backend choice:** a new crate `concordia-rhi-select`, the one place that turns `auto|metal|vulkan` into a device, a probe or a probe record. `auto` is Metal on macOS and Vulkan elsewhere. `CONCORDIA_BACKEND` overrides the project's `[render] backend`, which overrides `auto` (Q1). Every call site moves to it.
- **Surface targets:** `SurfaceTarget` gains `Xlib`, `Xcb`, `Wayland` and `Headless` (`VK_EXT_headless_surface`, for presenting in CI without a window). `MetalLayer` is reused by MoltenVK through `VK_EXT_metal_surface` (wired, untested). Metal refuses the new targets with `Unsupported`.
- **Swapchain:** a pure planner picks the image count (2 when allowed, like Metal), clamps the extent and checks the format; FIFO present mode. `acquire` with a zero timeout returns `None` when no image is free; the swapchain is rebuilt on resize or when out of date. Images rest in `PRESENT_SRC`. `on_presented` fires when the frame's GPU work retires (an approximation: a headless surface never reaches a display; `VK_KHR_present_wait` later).
- **Strict validation:** `CONCORDIA_VULKAN_VALIDATION=auto|on|strict`. Strict requires the layer, and any error-severity message makes the device's next submit, wait or readback return an error, so render tests that never see the device still fail on validation errors. The workflow sets `strict`.
- **Per-backend tolerances (D4):** `tolerances.ron` gains an optional `backends: { "vulkan": (default: …, goldens: {…}) }`; files without it still read. Most specific wins: the backend's golden entry, the shared golden entry, the backend's default, the shared default. Vulkan failures go under `<suite>/vulkan/`. `CONCORDIA_GOLDEN_KEEP=1` also writes comparisons and scores for passing checks (for the review page).
- **`concordia gpu-probe --backend auto|metal|vulkan`** (with `--record` too).
- **Linux windows** (Q2): `NativeView` gains X11 and Wayland handles; `concordia run` attaches them through one `unsafe` call in `concordia-cli`, the twin of the macOS one.

Testing:
- Test-first: testkit tolerances and keep mode, `device_required(Backend)`, the selector (parsing, `auto` per OS, env and manifest precedence, no loader), `gpu-probe --backend`, `SurfaceTarget` refusal on Metal, `NativeView` from X11/Wayland handles, the swapchain planner, the validation env, the manifest field.
- Metal's surface tests move into the shared conformance suite; each backend supplies a test surface (a `CAMetalLayer`, or `Headless`).
- Device code tested after: surfaces and strict validation, on lavapipe through the on-demand `vulkan` workflow (`CONCORDIA_REQUIRE_VULKAN=1`, `CONCORDIA_VULKAN_VALIDATION=strict`), which also runs the `concordia-render` suite and the M1 gate with `CONCORDIA_BACKEND=vulkan`, `gpu-probe --backend vulkan`, and a `concordia run` smoke test on Xvfb.
- Goldens compare against the blessed Metal images. Vulkan images, heat maps and any per-backend tolerance go on a review page before anything is committed; nothing is blessed in CI. A real feature gap goes back to the author, not into a looser tolerance (Q3).
- On the Mac the crates build and the Vulkan tests skip without a loader.

Steps, each a commit and a push:
1. This plan and the fold-ins (the on-demand workflow, D4 half answered).
2. Testkit: per-backend tolerances, keep mode, `device_required(Backend)`.
3. `concordia-rhi-select`, `[render] backend`, `gpu-probe --backend`; call sites switched.
4. `SurfaceTarget` and `NativeView` targets; the shared surface conformance cases.
5. Vulkan surfaces: the swapchain planner, then the device code; the probe record lists instance surface extensions.
6. Strict validation.
7. The M1 renderer on lavapipe: the render tests through the selector, the workflow steps, fixes.
8. Linux windows in `concordia run`, with the Xvfb smoke test.
9. The goldens review page.
10. Mutation checks, docs, the checklist.

Out of scope: the editor viewport on Vulkan and the rest of the D4 spike (4.1v(c)); exact present timing; Windows; mesh shaders; perf and budgets.

**4.1v(c)** (`m4/4.1v-vulkan-c`, PR #106, 2026-11-25). The author delegated this phase's decisions ("I will take your recommendations"); those taken are marked in the plan below. Vulkan device code was written first and tested after; the rest test-first.
- **D4's spike, answered.** On the CI runner's lavapipe (Mesa 25.2.8, LLVM 20.1.2, Vulkan 1.4.318, validation layers 1.3.275), Slang 2026.22's SPIR-V for a mesh shader (`MeshEXT`), an amplification shader (`TaskEXT`) launching it, and an inline ray query (`RayQuery` against a one-triangle BLAS/TLAS, in compute) compiles, passes the backend's SPIR-V checks, and runs correctly with no validation errors (`concordia-rhi-vulkan/src/spike`, test-only; 4 cases). So 4.3b's mesh path and 4.8's ray queries can be tested on Linux CI directly; MoltenVK, without mesh shaders, still needs the compute fallbacks D4 names. `concordia-shaders::Stage` gains `Mesh` and `Amplification`; the SPIR-V check knows `TaskEXT`/`MeshEXT`.
- **The Linux editor's viewport on Vulkan** (ADR-0088 decision 19): an X11 child window of the Tauri window on the editor's own XCB connection (x11rb's safe API, libxcb opened at run time), input-transparent so the page keeps the mouse, placed from the page's slot (`native::child_rect`), attached as `SurfaceTarget::Xcb`, and detached from GTK's `unrealize` before the window goes. The existing `attach_surface` block serves both platforms: **no new `unsafe`**. The page grabs the gizmo unless the shell's input does (`__CONCORDIA_NATIVE_INPUT__`, macOS only), and reports its slot at the size it presents (`slotSize`). X11 surfaces skip frames while their window resizes (the swapchain planner). No new crates in the editor's lock (`gtk`, `x11rb` and `raw-window-handle` were already there).
  - **Xvfb:** the `vulkan` workflow's new `editor` job builds the UI and the shell on Ubuntu 24.04 (WebKitGTK 4.1), runs Linux clippy, the shell's tests and the smoke test under `xvfb-run` with `CONCORDIA_BACKEND=vulkan` and strict validation (10 frames presented, a non-blank capture, `VIEWPORT detached`), and uploads a screenshot of the whole editor with its Vulkan viewport.
  - **Gaps:** Wayland embedding (needs an `unsafe` foreign-display call in the editor; under Wayland the page renders the viewport, `GDK_BACKEND=x11` gives the native one); page overlays inside the slot are covered by the child window; native gizmo grabs and the latency benches are macOS-only.
- **Found on the way** (and fixed): the glTF crash and the fallback texture (see 4.1v(b) above, ADR-0088 decision 18); `concordia run`'s exit race (`join_engine`); a macOS-only constant unused on Linux (the first Linux clippy of the editor).
- **Test log.** Test-first and seen failing for the right reason: `every_stage_has_its_own_slang_name` and `compiles_mesh_and_amplification_entries_to_spirv` (no such stage), `mesh_and_task_entry_points_use_the_ext_models` (no such model), the page's `native-flags` tests (`pageGrabsGizmo`, then `slotSize`/`slotRect`, not functions), the editor's `tests/native.rs` (6 tests, unresolved imports), the planner's resize case (the range error), and `tests/shutdown.rs` (`join_engine` missing). Passed at once: `spike_shaders_compile_to_spirv_that_passes_the_backends_checks` (written after the stage and model support), and the shared case `unwritten_and_freed_texture_slots_read_zero` on Metal (Metal already clears freed slots). Written after their code, by decision: the spike's device cases, the fallback texture (checked by that shared case on lavapipe), the X11 child window (the Xvfb smoke), and `env_sample`'s loop (the two glTF goldens, which crashed before it). Changed deliberately: two caps-table tests that encoded the old probe fixture's shape, and the smoke test, extended to Linux.
- **Mutation checks** (each restored file touched): 13 mutants of the new 4.1v(c) rules, all killed by test failures: the mesh stage name and model number, the slot's place below the page's top and its minimum size, `GDK_BACKEND`'s first choice and the Wayland default, Linux having no native input, the init script's input flag, the page's gizmo rule, `slotSize` and its use in the report, the planner's resize skip, and `join_engine`'s deadline (killed by the test hanging into a timeout). Not run: device-code mutants (deferred with the real-GPU re-test).
- **CI:** the `vulkan` workflow, green end to end on this branch (run 45557037960): the backend's 92 tests with the spike, the render suite (220) and the M1 gate golden on lavapipe within the new tolerances, `gpu-probe --backend vulkan --require-m1`, `concordia run` in an Xvfb window, the probe record, and the editor job.
- **Checklist** (2026-11-25, `CARGO_BUILD_JOBS=6`, M3 Pro): `just lint` and the Tauri clippy clean (Linux clippy of the shell in CI); `just wasm` and `just native-fixtures`; one `CONCORDIA_REQUIRE_GPU=1 cargo nextest run --workspace --no-fail-fast`: 2206 of 2207, the failure `every_crate_agents_md_has_required_sections` (4.1v(b)'s `concordia-rhi-select` AGENTS lacked four sections; added, then that test re-run alone); doc tests; `just gate-tests` M0, M1, M2 and M3; `just deny` and the editor's deny. `just editor-schemas` regenerates nothing, so neither 4.1v(b)'s `[render] backend` and `WindowEvent::Closing` nor this phase changed the Engine API or a reflected type, and `editor-test` was not needed (the UI's unit tests and lint ran: 123 passed).
- **Deferred:** real GPUs and MoltenVK; Wayland embedding; reporting the lavapipe sampling fault upstream; storing the prefiltered environment as one mipmapped texture (a re-bless); the shadow cascades' per-pixel texture choice (passes on lavapipe); making the workflow required.

**4.1v(c) plan** (`m4/4.1v-vulkan-c`, 2026-11-25; ADR-0088, amended. The author delegated this phase's decisions (2026-11-25: "I will take your recommendations"); the choices marked *(recommended, taken)* below were made on that basis and can be reversed. Vulkan device code is written first and tested after, as in 4.1v(a) and (b); everything else is test-first.):
Scope: the editor's native viewport on Vulkan on Linux, and the rest of D4's spike (do Slang 2026.22's mesh-shader and ray-query SPIR-V compile, pass our checks and run on lavapipe under validation?). Out: production mesh pipelines (4.3b), acceleration structures (4.8), perf, explicit-barrier fast paths, making the `vulkan` workflow required, real-GPU re-tests.

Step 0 closes 4.1v(b), whose PR merged before its step 10: the SIGSEGV fix confirmed on lavapipe, the goldens review page completed and `tolerances.ron` committed, the `vulkan` workflow green end to end, the lavapipe probe fixture refreshed from the printed record, 4.1v(b)'s mutation checks, its As built, ADR-0088 amended, `plans/README.md`, the crates' AGENTS/README, the top-level README, and the checklist.

Facts found while planning (checked in code, 2026-11-25):
- The editor's native viewport is macOS-only: `native.rs` puts an `NSView` with a `CAMetalLayer` under the transparent webview, and `slot_rect` returns an error elsewhere. The page drives orbit, pan and zoom itself in native mode (`viewport.navigate`); only the gizmo grab and the latency benches use the macOS event monitor.
- The editor already calls `concordia_render::attach_surface` in one `unsafe` block, under `cfg(target_os = "macos")`.
- Tauri on Linux is GTK 3 and WebKitGTK. `gtk` 0.18.2 and `x11rb` 0.13.2 are already in the editor's lock (through Tauri), so the Linux viewport adds no new licences.
- `concordia-shaders::Stage` has only compute, vertex and fragment; the backend's SPIR-V check knows only those execution models.
- The lavapipe probe record lists `VK_EXT_mesh_shader`, `VK_KHR_ray_query` and `VK_KHR_acceleration_structure`.

Design:
- **The Linux viewport is an X11 child window** *(recommended, taken)*, the twin of the macOS `NSView`: the editor opens its own XCB connection (x11rb's `XCBConnection`, safe API), creates an input-transparent child of the Tauri window at the page's slot, and attaches it as `SurfaceTarget::Xcb`. Moves and resizes follow `slot_rect` through a pure geometry function shared with nothing platform-specific.
  - **No new `unsafe` in the editor:** `native::install` returns a `SurfaceTarget` on both platforms, and the existing `attach_surface` block covers both. (An analogy: one door with one lock, used from two corridors, instead of a second door.)
  - **Detach before the window goes away:** GTK's `unrealize` signal on the Tauri window runs on the main thread before its X window is destroyed; the handler calls `concordia_render::detach_surface`, then destroys the child window and closes the connection.
  - **Wayland is a gap** *(recommended, taken)*: embedding a Wayland subsurface would need GTK's `wl_display` through an `unsafe` foreign-display call in the editor, which needs the author's approval. On a Wayland session the editor reports that the native viewport needs X11 and the page renders the viewport (as when attached); `GDK_BACKEND=x11` (XWayland) gives the native one.
  - The child window sits **above** the webview inside the slot (X11 children cover their parent's drawing), whereas on macOS the webview is on top. Page overlays inside the slot are hidden on Linux; recorded.
  - Gizmo grabs: the page grabs the gizmo itself unless the shell does (`__CONCORDIA_NATIVE_INPUT__`, true only on macOS).
- **The D4 spike** is test-only code in `concordia-rhi-vulkan` (`#[cfg(test)] mod spike`), reusing the crate's instance, validation log and SPIR-V checks, with its own small device that enables `VK_EXT_mesh_shader`, `VK_KHR_ray_query` and `VK_KHR_acceleration_structure`:
  - a mesh shader (no task stage) and a fragment shader draw one triangle, checked by reading pixels back;
  - a compute shader traces inline rays against a one-triangle BLAS/TLAS, checked by hit and miss counts;
  - `Stage` gains `Mesh` and `Amplification` (production: 4.3b needs them), and the SPIR-V check gains `MeshEXT` and `TaskEXT`.
  - The answer goes into D4 and ADR-0088, with any gap and the fallback D4 names (the compute meshlet path; the compute fallbacks for ray-query features).

Testing:
- Test-first: the slot-to-child geometry; the editor's init flags; the page's gizmo-grab rule (vitest); `Stage::Mesh`/`Amplification` names and cache keys; the SPIR-V check's new models; the spike shaders' Slang → SPIR-V → check on any OS with the toolchain.
- Device code tested after: the spike's mesh and ray-query runs, and the editor's Linux viewport, on lavapipe in the `vulkan` workflow under `CONCORDIA_VULKAN_VALIDATION=strict`.
- **Editor smoke on Xvfb:** a second job in the `vulkan` workflow builds the UI and the editor on Ubuntu 24.04 (WebKitGTK 4.1), runs `concordia-editor --smoke-test` under `xvfb-run` with `CONCORDIA_BACKEND=vulkan`, checks the smoke's frames and that no validation error was logged, the detach message, and uploads a screenshot of the Xvfb screen. If WebKitGTK cannot run there, the job is replaced by a smoke of the child-window path without the webview, and that is recorded.
- On the Mac: the crates and the editor build; Vulkan tests skip without a loader.

Steps, each a commit and a push:
0. Close 4.1v(b) (above), in several commits.
1. This plan.
2. `Stage::Mesh`/`Amplification`, the SPIR-V models, the spike shaders compiled and checked.
3. The spike's device runs on lavapipe; the answer recorded.
4. The editor's Linux viewport (geometry, child window, detach, flags, page rule).
5. The editor smoke on Xvfb in the `vulkan` workflow; fixes.
6. Mutation checks, docs (As built, ADR-0088, `plans/README.md`, AGENTS/README, the top-level README), the checklist.

Out of scope: Wayland embedding (needs `unsafe` in the editor), native gizmo grabs and latency benches on Linux, Windows, MoltenVK and real GPUs.

**4.2** (`m4/4.2-meshlets`, PR #107, 2026-11-25). The author approved Q1–Q8 as recommended. Everything was test-first, except where the test log below says otherwise.
- **`meshopt-sys`:** meshoptimizer v1.3 (MIT) vendored unmodified under `third_party/meshoptimizer` and built by `cc` (C++11, `-O2` in every profile, `-ffp-contract=off`, no exceptions or RTTI).
  - Hand-written bindings for `buildMeshlets`, `buildMeshletsBound`, `optimizeMeshlet` and `computeMeshletBounds`, checked against the header's blake3 and against the compiler's struct sizes (a size shim).
  - A checked wrapper (`safe`) refuses everything meshoptimizer asserts on, so `concordia-cluster` is `#![forbid(unsafe_code)]`.
- **`concordia-cluster`:**
  - `build` makes meshlets per material (128 triangles, 128 vertices, cone weight 0.25 by default), optimized and bounded.
  - Materials over 131,072 triangles are sorted by centroid Morton code and cut into piles, built in parallel on the `JobSystem` (`par_chunks_mut`, one pile per slot) and joined in pile order.
  - Also: `MeshletSize::for_limits`; `MeshletBlob` v1 (`encode` / validated `decode`); `cache_key`; `MeshletStats`; and `AUTO_MIN_TRIANGLES`, kept here so that import and stats share it.
- **Import:**
  - `ImportInput` gains `derived` (`DerivedCache`: the DDC in an import, `NoDerivedCache` elsewhere) and `jobs` (`import_jobs()`, one pool per process).
  - `ModelSettings::meshlets` (`"auto"` by default, written into new sidecars) and `attach_meshlets` per mesh: `auto` counts all of a mesh's parts; `true` refuses skinned meshes; a cache entry that doesn't decode is rebuilt.
  - glTF importer v5 and FBX importer v4.
  - Sample corpus: `auto` builds FlightHelmet's 4 large meshes, MetalRoughSpheres' 5 and MetalRoughSpheresNoTextures' 98. Every model takes `true` except Fox and SimpleSkin, which refuse it because they are skinned.
- **Inspection:**
  - `AssetDb::meshlet_stats` and the read-only method `asset.meshlet_stats` (MCP `asset_meshlet_stats`, for the modeler and tech-artist roles only; both prompts say when to use it), plus `concordia cluster stats [--json] <file | asset ref>`.
  - A mesh without meshlets says why: `meshlets: false`, skinned, or `auto` with its triangle count.
  - The agent guide documents it, and the editor's schemas are regenerated (two new generated types).
- **Differences from the plan:**
  - The 32-byte record has no reserved bytes: the sphere, cone, two offsets, the counts and the material fill it.
  - The header is 48 bytes, not 32: it adds the source vertex count, so decode can check every reference, and starts the records 16-byte aligned.
  - Triangle runs are padded to 4 bytes in memory too, so the in-memory layout is the GPU layout.
  - `meshlet_settings_change_rebuilds` at import level was dropped, because the sidecar has no meshlet-settings knob; `cache_key_changes_with_what_the_build_reads` covers it.
- **Found on the way** (fixed):
  - meshoptimizer zeroes the bounds of a meshlet whose triangles all have zero area. The build now encloses those meshlets itself.
  - meshoptimizer's cone cutoff is the *sine* of the normal cone's half-angle; the visibility cone is the normal cone widened by 90°. The stats first used `acos`.
  - `meshopt-sys` checked every position of the whole mesh for each meshlet, which is O(meshlets × vertices): 16 s in debug for 320k triangles, 0.4 s after the fix.
- **Cross-platform:** `output_hash_is_pinned` and the determinism tests pass on x86_64 Linux. This was checked in an emulated `rust:1-bookworm` container, with the author's approval; afterwards the image was removed and Docker Desktop quit. The CLI test's mesh is built from integers only, so its snapshots don't depend on libm.
- **Perf** (`meshlet_build_throughput_budget`, ignored, release, M3 Pro, the reference machine): 10,008,338 triangles become 78,191 meshlets in 1.08 s on 12 threads, against the provisional 10 s budget. No recalibration.
- **Test log:**
  - **Seen failing for the right reason before their code:**
    - `meshopt-sys`: `header_hash_matches_bindings` (no hash line), `a_quad_builds_one_meshlet`, `limits_beyond_meshoptimizer_are_refused`, `built_meshlets_stay_within_limits_and_buffers` (`unimplemented!`).
    - `concordia-cluster`: the 11 build and settings tests; `large_materials_are_split_into_spatial_piles`; `a_meshlet_of_zero_area_triangles_still_has_enclosing_bounds`; the 7 blob tests; the 2 stats tests (`unimplemented!`, then the `acos` bug).
    - `concordia-assets`: the 3 derived-cache tests.
    - `concordia-import-model`: the 7 meshlet-setting tests.
    - glTF: `meshlet_stats_explain_missing_meshlets` and `meshlet_stats_cover_every_mesh_of_an_asset`.
  - **Passed at once:**
    - `struct_sizes_match_the_c_compiler` and `vendored_version_is_1_3`, written with the bindings.
    - `wasm_guests_never_depend_on_sys_crates`, a guard.
    - `build_is_deterministic_across_thread_counts` and `small_meshes_match_meshoptimizers_unsplit_output`, while the build was still sequential.
    - `cluster_stats_refuses_unknown_assets`.
  - **Written after their code in the same step:**
    - the glTF and FBX meshlet tests (the DDC hit, the unrelated setting change, the 11 per-model corpus tests and the corpus-coverage test);
    - the CLI's `cluster stats` tests;
    - the roster test `only_the_modeler_and_tech_artist_read_meshlet_stats`.
    The mutation checks below show they bite.
  - **Corrected deliberately:**
    - A step-3 test assumed meshlets have at least 3 vertices; an all-degenerate meshlet may have 1 or 2.
    - The pile test's first two measures (an XY box, then a grid box) misjudged a tilted patch and an L-shaped Morton block. It now scrambles the triangle order and requires the 3 piles to span under 2.2 patches, against 3.0 for index-order piles.
    - `small_meshes_match…` compares meshlets run by run, because ours are padded.
    - `meshlet_stats_method_matches_cli` compares typed results, because the engine writes an `f32` as an `f64` in JSON.
    - Four tests pinning a new sidecar's settings text now include `"meshlets": "auto"`.
  - **Studio:** the existing `every_scoped_tool_is_used_by_the_role_prompt_or_its_skills` caught the tech artist's prompt, which lacked the new tool. The test catalog `PROJECT_TOOLS` gained the tool. `agent_guide_mentions_every_agent_tool` required the guide line.
- **Snapshots** (collected with `INSTA_UPDATE=new`, shown to the author on a review page and blessed by the author): `cli_cluster` hill text, two-nodes text and hill JSON (new); Studio `agent_modeler` and `agent_tech-artist` (the tool list and one prompt line each); and the M3 gates' `m3s_agents_and_skills` and `m3_fixture_agents_and_skills` (the tool lists).
- **Mutation checks** (each restored with a fresh mtime, `Cargo.lock` included): 23 mutants. 21 were killed by test failures and 2 by a test aborting on meshoptimizer's own assertion (the wrapper's index and limit checks, exactly what the wrapper prevents); none died of a compile error. They covered:
  - the pile threshold and the Morton sort;
  - `auto`'s threshold and skinning rule, and `true` on a skinned mesh;
  - the corrupt-cache rebuild;
  - the cache key's materials and settings;
  - decode's padding, reference and sphere checks;
  - the size choice;
  - the zero-area bounds fix;
  - three wrapper checks;
  - the glTF and FBX wiring;
  - the stats' reason, subset and angle;
  - the DDC as derived cache;
  - the hygiene guard (`meshopt-sys` added to `concordia-core`).
  One survivor on the first pass, `decode`'s padding check, killed after a one-triangle padding case was added. A first run was invalid (unquoted nextest filters failed in the shell) and was redone.
- **Checklist** (2026-11-25, `CARGO_BUILD_JOBS=6`, M3 Pro):
  - `just lint` and the Tauri clippy: clean.
  - `just wasm` and `just native-fixtures`: pass.
  - One `CONCORDIA_REQUIRE_GPU=1 cargo nextest run --workspace --no-fail-fast`: 2,266 of 2,272 passed. The 6 failures were the 4 snapshots then awaiting review, plus 2 M3 gate tests over the fixture game's copies of the roster's role files (synced). Each failing test was re-run alone after blessing: 8 of 8 pass.
  - Doc tests: pass.
  - `just gate-tests`: M1 4 of 4, M2 5 of 5, and M3 16 of 16. M3's run first failed on a seventh snapshot (the fixture's agents), which was reviewed, blessed and re-run.
  - `just deny` and the editor's deny: clean (the editor's lock gains the two path crates).
  - `just editor-schemas`: two new generated types (`asset.meshlet_stats`).
  - `just editor-test`: 123 unit tests and 22 end-to-end tests pass.
  - `just clean-builds`: run at the end, with free disk under 25 GB.
  - The `vulkan` workflow wasn't run: no RHI or shader change.
- **Disk:** the disk filled during step 6, leaving 356 MB free with `target/debug` at 93 GB. The corpus tests' "No space left on device" errors came from that. `just clean-builds` freed it.

**4.2 plan** (`m4/4.2-meshlets`, 2026-11-25; R5, ADR-0100. Decisions Q1–Q8 below were approved by the author as recommended on 2026-11-25. Everything is test-first: 4.2 is CPU-only and touches no device code.)
Scope: the vendored meshoptimizer behind `meshopt-sys`; `concordia-cluster` (meshlet build, bounds, normal cones, the encoded format, its build cache); the model importers' `meshlets` setting; `concordia cluster stats` and its Engine API method. Out: anything on the GPU (4.3), meshlet LODs (4.5), the vertex and index codecs, ray-tracing clusters (`buildMeshletsSpatial`, with 4.8), perf calibration.

Facts found while planning (checked in code, 2026-11-25):
- `MeshData::meshlets: Vec<u8>` is an empty placeholder (`concordia-assets/src/data.rs`), filled by nobody.
- The glTF importer writes one `MeshData` per primitive (`mesh/<mesh>/<p>`), and FBX one per material part (`mesh/<mesh>/<part>`): each mesh artifact already has exactly one material.
- Import settings are `ModelSettings` in `concordia-import-model`, shared by glTF and FBX; unknown keys are errors. The import's DDC key is blake3 over importer name and version, `PLATFORM`, the effective settings and the source bytes (`concordia-assets/src/key.rs`). `ImportInput` has no DDC or job-system handle.
- `concordia-core::jobs::JobSystem` already does deterministic chunked `par_map_reduce` on an owned pool (chunk size from the caller, never from the thread count).
- Mesh-output caps: Metal reports 256 vertices and 512 primitives on `metal3`; Vulkan reports 0 until 4.3b, and the lavapipe probe record doesn't capture the limits. `VK_EXT_mesh_shader` guarantees at least 256 and 256.
- The wasm and native guests depend only on `concordia-core`, `concordia-reflect` and `concordia-script-guest`, so a `-sys` crate below `concordia-assets` never reaches `just wasm` or `just native-fixtures`.
- meshoptimizer's latest release is v1.3 (2026-09-25). It is C++ with a C API, single-threaded and reentrant for the meshlet functions (`buildMeshlets*`: `max_vertices` ≤ 256, `max_triangles` ≤ 512; `optimizeMeshlet`; `computeMeshletBounds` with an 8-bit SNORM cone). Its vertex and index encoders are documented as not thread safe.

Design:
- **Q1 `meshopt-sys`:** vendor meshoptimizer **v1.3** (MIT) unmodified into `third_party/meshoptimizer/` (`src/*.cpp`, `meshoptimizer.h`, `LICENSE.md`, `VENDOR.md` with tag, commit and SHA-256), built by `cc` as C++ with `-O2` in every profile, `-ffp-contract=off` (no fused multiply-add, so x86 and ARM compute the same floats), no exceptions or RTTI, never downloading. Hand-written bindings for only what 4.2 uses (`buildMeshlets`, `buildMeshletsBound`, `optimizeMeshlet`, `computeMeshletBounds`, the `Meshlet` and `Bounds` structs), checked against the header's blake3 and against `sizeof` from a 10-line C++ size shim, as `ufbx-sys` does. **Unlike `ufbx-sys`, the crate also holds the small safe wrapper** (`meshopt_sys::safe`), validating every limit before the call and with a `// SAFETY:` comment on each block, so `concordia-cluster` stays `#![forbid(unsafe_code)]`. The wasm and native-fixtures builds are untouched (see Facts); a new hygiene test keeps it that way.
- **Q2 meshlet size:** one portable default, **128 triangles and 128 vertices**, which fits Metal (256/512) and every `VK_EXT_mesh_shader` device (≥ 256/256); the compute fallback has no limit. `MeshletSize::for_limits(mesh_shaders, max_vertices, max_primitives)` (plain numbers, so the crate needn't depend on `concordia-rhi`) picks 128 when it fits and 64 otherwise; 4.3 calls it with `RhiCaps` and gets a separately cached 64 build only on a device that needs it (none known). The build doesn't depend on the device, so headless imports need no GPU.
- **Q3 the encoded format and its key:** a GPU-ready, little-endian layout that 4.3 uploads without decoding, `MeshletBlob` v1:
  - header: magic `CMLT`, format version, settings, counts, source triangle count;
  - 32-byte meshlet records: sphere (center, radius: 4 × f32), cone axis and cutoff (4 × i8 SNORM, meshoptimizer's conservative rounding), vertex offset (u32), triangle offset (u32), vertex count (u8), triangle count (u8), material (u16) (*as built: no reserved bytes, as these fill the 32; the header grew to 48 bytes with the source vertex count*);
  - vertex references (u32 into the mesh's streams); micro-triangles (3 × u8, each meshlet padded to 4 bytes).
  Decoding validates everything and rejects a wrong magic or version. It's stored in `MeshData::meshlets` (one read for 4.3's loader), and the build is cached in the DDC under its own key, blake3 of `"concordia-meshlets"`, the format version, meshoptimizer's version, the canonical settings and a **content hash of only what the build reads** (positions, indices, per-triangle materials). So changing an unrelated import setting (emissive EV) or bumping the importer's version reuses the meshlets. `PLATFORM` is left out because the output is the same on every OS; a pinned-hash test proves it. **ADR-0100 is amended** with the format and the cache, rather than a new ADR. The vertex and index codecs are left out of v1; the format version bumps when they arrive (DDC size, which the canyon's instancing keeps small).
- **Q4 `auto`:** `ModelSettings` gains `meshlets: auto | true | false` (default `auto`, written into new sidecars), shared by glTF and FBX. `auto` builds meshlets for a **static** mesh (no joints or weights) whose primitives together have **more than 8,192 triangles**, so a rock split over two materials counts as one object. `true` builds for every static mesh and is an import error on a skinned one; `false` never builds. Importers that write `MeshData` bump their version once.
- **Q5 determinism across thread counts:** split a material's triangles into fixed piles, then build each pile in parallel. A mesh with up to 131,072 triangles is one pile, so it gets meshoptimizer's own unsplit result. A larger one is ordered by the Morton code of each triangle's centroid (ties by triangle index), then cut into 131,072-triangle piles. Each (material, pile) builds, optimizes and bounds its meshlets on `JobSystem`; results are joined in (material, pile) order. The piles depend only on the input, never on the thread count, so 1 and N threads give byte-identical blobs. `ImportInput` gains `jobs` and `derived` (a small `DerivedCache` trait over the DDC, in-memory in tests).
- **Q6 throughput:** `meshlet_build_throughput_budget` lands now as an `#[ignore]` perf test with `concordia-perf`'s verdict (`NotReference` off the M3 Pro), run once in release. The time is recorded in the As built, with no budget change (perf deferred).
- **Q7 `concordia cluster stats <asset>`:** for each mesh sub-asset it prints source triangles, meshlet count, triangles and vertices per meshlet (min, mean, max and fill %), sphere radius (min, mean, max), the share of meshlets whose cone can cull anything and the mean cone half-angle, encoded bytes and the settings. A mesh without meshlets says so and gives the reason (`auto` below 8k, skinned, `false`). `--json` prints the same data. **Also an Engine API method `asset.meshlet_stats`** (read-only, from the `[assets]` module, so MCP `asset_meshlet_stats`), given to the `modeler` and `tech-artist` roster roles only. The CLI calls the same function.
- **Q8 one PR:** the plan's 1–2 weeks fits one PR in eight steps (below). It is split at review time if it grows.

Crates and files: new `third_party/meshoptimizer/`, `crates/meshopt-sys/`, `crates/concordia-cluster/`; touched `concordia-assets` (`ImportInput`, `DerivedCache`, the `asset.meshlet_stats` method), `concordia-import-model` (`ModelSettings::meshlets`, the `auto` rule, the build call), `concordia-import-gltf` and `concordia-import-fbx` (versions, the call per mesh), `concordia-cli` (`cluster stats`), `concordia-api` types and the Studio roster, `docs/adr/0100`, `third_party/README.md`, `plans/README.md`, and the tests' hygiene checks. No RHI, shader or renderer change, so the `vulkan` workflow isn't run.

Tests first (each seen failing for the right reason before its code):
- `meshopt-sys`: `header_hash_matches_bindings`, `struct_sizes_match_the_c_compiler`, `vendored_version_is_1_3`, `a_quad_builds_one_meshlet`, `limits_beyond_meshoptimizer_are_refused`.
- `concordia-cluster` (proptests over random meshes, including degenerate and multi-material ones): `meshlets_respect_vertex_and_triangle_limits`, `every_source_triangle_appears_in_exactly_one_meshlet` (winding kept, corners compared up to rotation), `meshlets_never_span_two_materials`, `meshlet_bounds_contain_every_vertex`, `normal_cone_is_conservative` (with the stored SNORM cone and the sphere formula 4.4 will use: a rejected meshlet has no front-facing, non-degenerate triangle from random views), `meshlet_encoding_roundtrips_exactly`, `decode_rejects_bad_magic_version_and_ranges`, `build_is_deterministic_across_thread_counts` (1, 2 and 8 threads, several piles), `small_meshes_match_meshoptimizers_unsplit_output`, `settings_are_validated`, `size_for_limits_picks_128_then_64`, `output_hash_is_pinned` (same hash on macOS and Linux CI), `cache_key_ignores_unread_streams`.
- Import: `auto_builds_only_static_meshes_over_8k_triangles` (counted per mesh across primitives), `meshlets_true_and_false`, `meshlets_true_on_a_skinned_mesh_is_an_error`, `unknown_meshlets_value_is_an_error`, `ddc_reuses_meshlets_when_source_unchanged`, `meshlets_survive_an_unrelated_setting_change` (cache hit), `meshlet_settings_change_rebuilds`, `gltf_corpus_imports_with_meshlets` (the fixtures, plus the sample models when fetched), `fbx_mesh_gets_meshlets`.
- CLI and API: `cluster_stats_prints_counts_and_bounds` and `cluster_stats_json` (insta, reviewed on an artifact page before blessing), `cluster_stats_explains_missing_meshlets`, `meshlet_stats_method_matches_cli`, the roster's tool lists.
- Hygiene: `wasm_guests_never_depend_on_sys_crates` (cargo metadata), and the existing AGENTS sections test for the new crates.
- Perf: `meshlet_build_throughput_budget` (`#[ignore]`, release, Q6).
- Mutation checks with fresh mtimes over the pile rule, the `auto` threshold and skinning rule, the cache key, decode validation, the size choice and the safe wrapper's limit checks.

Steps, each a commit and a push (draft PR after step 1):
1. This plan.
2. `third_party/meshoptimizer` and `meshopt-sys` (bindings, size shim, safe wrapper), with its tests; `just deny` and the editor's deny.
3. `concordia-cluster`: settings, size choice, single-pile build, bounds and cones (proptests).
4. The piles and the job system; determinism; the pinned hash.
5. `MeshletBlob` v1 encode/decode; the build cache key.
6. Import: `ModelSettings::meshlets`, `DerivedCache` and `jobs` in `ImportInput`, glTF and FBX, the DDC tests, the corpus.
7. `concordia cluster stats`, `asset.meshlet_stats`, the roster; snapshots collected with `INSTA_UPDATE=new` and shown on an artifact page.
8. Perf run, mutation checks, docs (As built with the test log, Status, ADR-0100, `plans/README.md`, `third_party/README.md`, AGENTS/README), the checklist.

**4.3a** (`m4/4.3-indirect`, PR #108, 2026-11-25). The author approved Q1–Q10 as recommended, with two future-work notes (GPU-decided draw counts; texture copies that grow), and approved every golden and snapshot on review pages before blessing. Everything was test-first except where the test log says otherwise; Vulkan device code was written first and tested on lavapipe.
- **RHI (D5):** `DrawIndirect` / `DrawIndexedIndirect` (one record each), `SetViewport` / `SetScissor`, `CopyTexture`, validated by `CommandList::validate` and the shared `concordia_rhi::check` (argument range, region copies with `CopySide`, pass rectangles); Metal (blits, `drawPrimitives:indirectBuffer:`) and Vulkan (`vkCmdDraw*Indirect`, per-viewport Y flip, `vkCmdCopyImage`, planner moves for copies, `General` within one texture). 11 conformance cases in `indirect.rs` and `regions.rs`.
- **Scene buffers and the path** (ADR-0100 decisions 13–16): blobs uploaded unchanged, a 32-byte mesh-table row, the instance row's spare word; `meshlet::plan` (buckets by pipeline and material, one shadow bucket, fixed entry slots); `meshlet_expand` (no atomics), `meshlet_vs` and `meshlet_shadow_vs` through `surface_vertex` and the shared `fs`; buckets in the forward pass between the M1 opaque and blended draws; `[render] meshlets` (default on) and `Renderer::set_meshlets`.
- **Stats and views:** `render.stats` `raster_path` and `meshlets`; perf mode times `meshlet.expand`; `ViewMode::{meshlets, triangles}` through a flat id pair; the editor's View menu lists them; no `render.set_debug_view`.
- **Goldens** (`tests/golden/m4_meshlet/`, blessed from the M1 path on Metal): `meshlets_vs_forward`, `meshlet_shadows`, `masked_meshlets`, `skinned_composite` (each also held, meshlets against M1 in the same run, at mean 0.005), `view_meshlets`, `view_triangles`. On Metal every meshlet image equals its M1 image (FLIP 0.0). **On lavapipe** (runs 45650551494 and 45654070798, strict validation): every on-against-off check passes; `meshlet_shadows` 0.0026, `skinned_composite` 0.0034, `view_meshlets` 0.0025 and `view_triangles` 0.0054 pass their tolerances against the Metal goldens; `meshlets_vs_forward` and `masked_meshlets` measure 0.0094–0.0103 against the shared 0.005 (the M1 path differs from Metal as much), so a Vulkan entry of 0.015 each is proposed on the review page (https://claude.ai/artifact/Rb4JhyzBC5oFmaPqD1x6Rt), by 4.1v(c)'s rule.
- **Found and fixed on the way:** Metal aborted when a command failed inside an open encoder; Vulkan left a render pass open when a command failed, and created image views for copy-only textures (both found by the first lavapipe run, as validation errors only); `expand` is a Slang keyword; the SPIR-V pointer rule caught a pointer stored in a struct; the first test scenes were black (daylight exposure for a 4 lux sun), which a black-against-black match would have passed, so the stats now prove the path ran.
- **Limitation, unchanged from M1:** masked materials cast solid shadows (the shadow pass ignores alpha masks), on both paths.
- **Timings** (release, M3 Pro, 1280×720, 64 balls of 9,216 triangles, 150 frames after 30, medians, shadows cached; no budget changes): M1 path `forward` 1.941 ms, frame 2.078 ms; meshlet path `meshlet.expand` 0.004 ms, `forward` 2.157 ms (+11%: 384-vertex padded draws, nothing culled yet), frame 2.300 ms.
- **Test log:**
  - *Seen failing for the right reason before their code:* the six validation and `check` tests (stubs returned Ok); the indirect, viewport, scissor and copy conformance cases on Metal (`Unsupported`); the planner's copy tests and the property test; the copy-overlap assertion; `only_viewable_usages_get_image_views`; the seven `meshlet` unit tests and the bucket proptest; `expand_covers_every_entry_in_rows_of_groups`; the push-byte and layout tests (the layout test first exposed the `expand` keyword); `keep_writes_the_actual_image_of_a_missing_golden`; the render tests (the path never engaged), then only their missing goldens; the setting and stats test (no `raster_path`); the view-mode test (modes fell through to lit); the editor's `VIEW_MODES_MATCH_THE_ENGINE` type check.
  - *Passed at once:* `indirect_arguments_out_of_range_are_errors`, `viewport_and_scissor_outside_the_target_are_errors` and `copy_texture_rejects…` while everything was refused (mutants later showed they bite); `indirect_draw_after_compute_waits_for_shader_writes` (the global barrier covers it); `nothing_to_draw_plans_nothing`; `meshlet_push_keeps_the_fields_fs_reads…`; `meshlets_off_draws_everything_through_m1`; `perf_mode_times_meshlet_expand_before_the_draws`.
  - *Written after their code:* `expand_writes_one_entry_per_meshlet_and_counts_them` (the shader came first), `flipped_viewports_keep_y_up_inside_their_rectangle`, and the Vulkan device behaviour (lavapipe).
  - *Corrected deliberately:* the editor test's mode count (15 → 17); tolerance entries renamed to the goldens they hold (a hygiene test requires it).
- **Mutation checks** (29 mutants, each restored with a fresh mtime): all killed by test failures, none by compile errors, except two equivalent ones (dropping the out-of-clip position: an invalid slot also reads vertex 0) and one survivor: `triangle < count` → `<=`, invisible on spheres and on scattered triangles (the stray triangles were degenerate). `triangles_past_a_meshlets_count_are_never_drawn` (a hand-built blob whose stray read names a never-indexed triangle) kills it. Three mutants first failed to apply (a separator clash in the script) and were rerun.
- **Process note:** one commit (2a27076) went in with a clippy lint, because the lint's output was piped through a filter; the next commit fixed it, and the lint now gates by its exit status.
- **Checklist** (2026-11-25, `CARGO_BUILD_JOBS=6`, M3 Pro):
  - `just lint` and the Tauri clippy: clean. `just wasm` and `just native-fixtures`: pass.
  - One `CONCORDIA_REQUIRE_GPU=1 cargo nextest run --workspace --no-fail-fast`: 2,328 of 2,329 passed; the one failure was the M1 gate's MCP copy of the `view_mode` schema snapshot (the same two values the author approved), accepted and re-run alone: passes. Doc tests: pass.
  - `just deny` and the editor's deny: clean (both locks gain only the path dependency).
  - `just editor-schemas` (four generated files), `just editor-test`: 123 unit and 22 end-to-end tests pass.
  - The `vulkan` workflow: the RHI suite 107 of 107 on lavapipe after the two fixes; the render suite, the M1 gate golden, `gpu-probe` and the Xvfb window pass apart from the two goldens above.

**4.3a plan** (`m4/4.3-indirect`, 2026-11-25; D5, ADR-0100 and its 4.2 amendment. **The author approved Q1–Q10 as recommended on 2026-11-25**, with two notes for future work: the GPU must eventually decide the number of draws too (Q1), and texture copies will need to grow (Q3); both are in `later-backlog.md` §12. Everything is test-first, except Vulkan device code, which is written first and tested after on lavapipe, as in 4.1v.)
Scope: indirect draws, viewport/scissor and texture-to-texture copy on Metal and Vulkan (D5); the GPU scene buffers; the compute + indirect meshlet path drawing into Forward+ and the shadow cascades; `render.stats` for it; the `meshlets` and `triangles` view modes. Out: mesh shaders, the Slang mesh spike, motion vectors and the previous transform (4.3b); any culling beyond today's CPU instance frustum test (4.4); perf calibration.

Facts found while planning (checked in code, 2026-11-25):
- **The RHI** has `Draw`, `DrawIndexed`, `Dispatch` and `DispatchIndirect`, and `BufferUsage::INDIRECT` already. There is no indirect draw, no viewport or scissor command (each pass covers its whole target; Vulkan sets a negative-height viewport at pass start for Y up) and no texture-to-texture copy. Vulkan's pipelines already declare viewport and scissor dynamic.
- **Synchronisation:** Metal tracks hazards; Vulkan's planner (`track.rs`) puts a global barrier between steps. Its access masks must cover indirect-argument reads, and texture copies need layout moves.
- **The M1 path pulls vertices too:** `Draw` with `SV_VertexID` indexing the mesh's index buffer inside `forward.slang`'s `vs`. `fs` reads the frame, the material rows and the material's first row from push constants (`DrawPush`).
- **Pipelines are specialised per material shape** (`SHAPE_OPS`, `SHAPE_CLOSURES`, `LAYERED`), blend and double-sidedness. M1 batches draws by (pipeline, material, mesh).
- **Per-frame tables:** the instance table (`Instance { model, normal, skin: uint4 }`, `skin.w` unused) and the material rows are rebuilt and uploaded in full every frame from a power-of-two buffer pool. Mesh streams are uploaded once per mesh (`GpuMesh`), each as its own bindless buffer.
- **Instances are frustum-culled on the CPU** (`cull::Frustum`). Shadow casters are every `cast_shadows` item, whether the camera sees it or not.
- **Debug views** are `ViewMode` values chosen by `viewport.overlays` and `capture.screenshot` (`view_mode`). `overdraw` and `wireframe` use their own `debug` pipelines. There is no `render.set_debug_view`.
- **`MeshletBlob` v1** is already the GPU layout (48-byte header, 32-byte records, `u32` references, 4-byte-aligned `u8` triangle runs). Each mesh artifact has one material, so all of a mesh's meshlets share it.

Design (each item is a question for the author, with the recommendation):
- **Q1 indirect draws:** `Command::DrawIndirect { args, offset }` and `Command::DrawIndexedIndirect { indices, args, offset }`, one draw each. The arguments are the layout both APIs share (`vertex/index count, instance count, first vertex/index, [vertex offset,] first instance`, 16 and 20 bytes). There are no count buffers and no multi-draw: Metal has no count-buffer draw outside indirect command buffers, and 4.3a's draws are counted on the CPU (the GPU decides only the instance count). *Author note: the GPU must eventually decide the number of draws too (count buffers on Vulkan, indirect command buffers on Metal); `later-backlog.md` §12.* Validation: inside a pass, offset a multiple of 4. Backends check `INDIRECT` usage and the range. Our shaders take their bases from push constants and leave `first vertex`/`first instance` at 0; a conformance case pins `SV_VertexID`/`SV_InstanceID` under indirect draws on both backends.
- **Q2 viewport and scissor:** `Command::SetViewport(Viewport { x, y, width, height, min_depth, max_depth })` and `Command::SetScissor(Rect { x, y, width, height })`, inside passes only. Each `BeginRenderPass` resets both to the whole target, so existing code is unchanged. Vulkan keeps Y up by flipping every viewport it sets. Nothing in 4.3a uses them; VSM pages (4.7) and the M10 atlas do.
- **Q3 texture copy:** `Command::CopyTexture { src, dst, src_origin: TextureOrigin { mip, x, y }, dst_origin, extent }`, outside passes. Same format, both single-sample, both regions inside their mips. On Vulkan the planner moves both textures to transfer layouts and back. *Author note: this is the first version; slices, volumes, depth, compressed formats and MSAA come with the phases that need them (`later-backlog.md` §12).*
- **Q4 GPU scene buffers:**
  - **Instances:** the existing per-frame instance table, one table for both paths. `skin.w` (unused today) becomes the instance's mesh-table row + 1 (0 for none). The previous transform arrives in 4.3b with motion vectors, its first user.
  - **Mesh table** (new, per frame, 16 bytes a row): the blob's bindless index, the meshlet count, the max triangles per meshlet, and the material-independent flags.
  - **Geometry:** `MeshletBlob` v1 is uploaded **as is**, once per mesh at load, as one more bindless buffer beside the mesh's existing streams. It is decoded first, so a bad blob is an error and the mesh falls back to the M1 path. Positions, normals, tangents and UVs are the streams the M1 path already uploads.
  - **Updates and growth:** the instance and mesh tables and the per-frame meshlet buffers are uploaded in full each frame from the pool, as the instance and material tables are today. Geometry uploads once and is freed with the mesh. Incremental updates wait for a measured need (perf is deferred).
  - **Bindless slots:** one per meshlet mesh, plus four per frame (mesh table, work items, entries, arguments), against 16,384.
- **Q5 the compute + indirect path:**
  - **Buckets:** forward draws are grouped like M1's batches, by (pipeline, material). Shadow casters form one bucket.
  - **Work items** (CPU, per frame): one per (bucket, instance), with the instance, its mesh row and the first entry slot.
  - **`meshlet.expand`** (compute, before the shadow and forward passes, its own timed pass): one thread per meshlet of each work item. It writes an 8-byte entry (instance, meshlet) to its fixed slot and adds one to its bucket's instance count, from arguments preset to (3 × max triangles, 0, 0, 0). Fixed slots keep the entry list deterministic; 4.4 adds its culling test there and switches to appending.
  - **Drawing:** one `DrawIndirect` per bucket: 384 vertices (128 triangles × 3) per instance and one instance per meshlet. The new `meshlet_vs` finds its entry from `SV_InstanceID` plus the bucket's base and the triangle from `SV_VertexID / 3`. It reads the three local corners from the triangle run, the vertex references, then the mesh's streams. A triangle past the meshlet's count collapses to a point outside the clip volume, so it draws nothing.
  - **Shading:** `meshlet_vs` writes the same `VsOut` as `vs`, so `fs` is shared unchanged. Its push block keeps `frame`, `materials` and `material` at `DrawPush`'s offsets, so `fs` reads them as it does for M1 draws. Pipelines are the forward and debug variants with the vertex stage swapped, created lazily like today's.
  - **Shadows:** `meshlet_shadow_vs` draws the caster bucket into each cascade, so meshlet meshes cast the same shadows, and the shadow cache key covers them.
- **Q6 which meshes take the path:** a draw item whose mesh has a valid blob, is not skinned, and whose material is not blended; masked materials qualify (`fs` discards as today). A switch `[render] meshlets = true | false` (default true, like `perf_mode`) and `Renderer::set_meshlets` turn the path off, for comparisons. **Depth:** meshlet buckets draw inside the same forward pass, with the same MSAA color and depth targets, after the M1 opaque draws and before the sky and the blended draws, so skinned and translucent geometry depth-test against them unchanged.
- **Q7 goldens** (new suite `tests/golden/m4_meshlet/`, blessed on Metal only):
  - `meshlet_render_matches_forward_plus_golden`: a scene built in code: high-polygon static meshes with meshlets `true` at several transforms, one double-sided, a sun with CSM shadows falling on and from them, and an IBL environment. The blessed image `meshlets_vs_forward.png` is the **M1 path's** render (meshlets off). The test renders with meshlets off (must match at the suite default, mean 0.01) and on (mean ≤ 0.005, the plan's number, in `tolerances.ron`).
  - `masked_material_meshlets_render_alpha_cutouts_correctly` **lands here**: a cutout texture on a meshlet mesh, golden from the M1 path as above. 4.3b re-runs it on the mesh-shader path.
  - `skinned_mesh_composites_with_meshlet_depth` lands here too: the skinned column from the skinning tests and a translucent pane, crossing a meshlet mesh.
  - **lavapipe:** the same blessed images, through the `vulkan` workflow. A Vulkan entry goes in `tolerances.ron` only when measured above the default, by the existing rule (measured mean × 1.25, rounded up to 0.005), after the review page.
- **Q8 stats and debug views:**
  - `render.stats` gains `raster_path` (`"forward"` when no draw takes the path, `"compute_indirect"`; 4.3b adds `"mesh_shader"`), and `meshlets: { instances, meshlets, triangles, indirect_draws }`. `draws`, `draw_calls` and `triangles` stay totals over both paths (the meshlet triangles are the meshlets' real counts, not the padded 384-vertex draws). Perf mode times `meshlet.expand` as its own pass.
  - **No `render.set_debug_view` method:** it would duplicate `view_mode` on `viewport.overlays` and `capture.screenshot` (no-duplication rule). The plan's three views become `view_mode` values: `overdraw` exists; `meshlets` (a hashed color per meshlet; M1 draws grey) and `triangles` (a hashed color per triangle, both paths) land in 4.3a. That needs one flat `VsOut` field written by both vertex shaders. The editor's view menu picks them up from the regenerated schema.
- **Q9 ADR-0100** is amended with Q1–Q8 and moves to **Accepted** in this PR's last step, once the path renders on Metal and lavapipe with its goldens passing.
- **Q10 one PR**, in ten steps (below). The RHI additions are small, and the meshlet path is their first user. It is split at review time if it grows.

Changes while executing (recorded as they happened):
- **Steps 5–7 landed as one commit:** the scene data has no reader until the shaders exist, and clippy forbids dead code.
- **No atomics in `meshlet_expand`** (Q5 said each thread adds one to its bucket's count): the renderer has no atomics yet, and atomics through bindless pointers on Metal via Slang are unverified. Entries go to fixed slots, and the thread of each bucket's first entry writes the bucket's whole argument record from a bucket table (first entry, entries, vertices). Deterministic, and the GPU still writes every argument; 4.4 decides atomics or a prefix sum with culling. The entry point is `meshlet_expand` (`expand` is a Slang keyword).
- **The mesh-table row is 32 bytes** (Q4 said 16): blob, positions, normals, tangents; UVs, meshlet count, triangles per meshlet, 0. A bucket groups several meshes, so the streams come per instance.
- **Shared checks** in `concordia_rhi::check` (indirect argument range, texture copy, pass rectangles), run by both backends; viewports and scissors outside the pass's targets are errors (Metal requires it), a case added to the suite. Copies within one texture use `General` on Vulkan, and overlapping regions in one mip are refused.
- **Goldens:** the shadow scene became a golden (`meshlet_shadows`) because every tolerance entry must name one; each scene's on-against-off check uses its golden's entry (mean 0.005). The four goldens are the M1 path's images; the meshlet images matched them exactly on Metal (FLIP 0.0). Approved and blessed by the author on 2026-11-25.
- **Testkit:** keep mode also writes `<name>.actual.png` for a missing golden, so new images can be reviewed before blessing.
- **Found:** a recording error inside an encoder aborted the process on Metal, and left a render pass open on Vulkan; Vulkan created image views for copy-only textures. All fixed.
- **Limitation (unchanged from M1):** masked materials cast solid shadows (the shadow pass ignores alpha masks) on both paths.

Crates and files: `concordia-rhi` (commands, validation, conformance cases), `concordia-rhi-metal` (`encode.rs`), `concordia-rhi-vulkan` (`encode.rs`, `track.rs`); `concordia-render` (new `meshlet.rs` (buckets, work items, mesh table), `shaders/meshlet.slang` (`expand`, `meshlet_vs`, `meshlet_shadow_vs`), `forward.slang` (one `VsOut` field, the two modes), `gpu.rs`, `pipelines.rs`, `renderer.rs`, `shadow_pass.rs`, `stats.rs`, `view_mode.rs`, `module.rs` (`[render] meshlets`)), with a new dependency on `concordia-cluster` (blob decode); `concordia-api` stats types and the editor schemas; `tests/golden/m4_meshlet/`; `docs/adr/0100`, `plans/README.md`, AGENTS/README of the touched crates. No new crates and no new external dependencies, so the locks change only by the path dependency.

Tests first (each seen failing for the right reason before its code, except Vulkan device behaviour, tested after):
- **RHI validation** (pure): `indirect_draws_must_be_inside_a_pass_with_aligned_offsets`, `viewport_and_scissor_are_inside_passes_finite_and_non_empty`, `copy_texture_is_outside_passes`.
- **Conformance** (shared suite; Metal here, lavapipe in the workflow): `draw_indirect_reads_its_arguments_from_a_buffer`, `draw_indexed_indirect_reads_its_arguments_from_a_buffer`, `compute_written_arguments_drive_an_indirect_draw` (compute → indirect draw, the meshlet path's shape), `indirect_draws_see_vertex_and_instance_ids_from_zero`, `indirect_arguments_out_of_range_are_errors`, `viewport_maps_clip_space_into_its_rectangle` (Y stays up), `scissor_discards_outside_its_rectangle`, `each_pass_starts_with_the_full_viewport`, `copy_texture_copies_a_region_between_mips_and_textures`, `copy_texture_rejects_mismatched_formats_and_out_of_range_regions`.
- **Vulkan planner** (pure): `indirect_draw_after_compute_waits_for_shader_writes`, `copy_texture_moves_both_textures_and_returns_them`; the property test `every_command_sees_the_layout_it_needs` gains the new commands.
- **Render, CPU side:** `only_static_unblended_meshes_with_blobs_take_the_meshlet_path`, `buckets_follow_m1_batching`, `work_items_cover_every_meshlet_once` (proptest), `mesh_table_rows_point_at_the_blob_and_streams`, `invalid_blobs_fall_back_to_the_m1_path`, `meshlets_setting_reads_from_the_manifest`.
- **Render, GPU:** `expand_writes_one_entry_per_meshlet_and_counts_them` (readback), `meshlet_render_matches_forward_plus_golden`, `meshlet_shadows_match_m1_shadows` (shadow-only scene, image compare), `masked_material_meshlets_render_alpha_cutouts_correctly`, `skinned_mesh_composites_with_meshlet_depth`, `meshlet_and_triangle_view_modes_golden`, `render_stats_report_the_meshlet_path`, `meshlets_off_draws_everything_through_m1`.
- **Perf:** timings only: `meshlet.expand` and `forward` with the path on and off on a dense in-code scene, recorded in the As built. No budget changes.
- **Mutation checks** with fresh mtimes over: the path rule, bucket keys, slot offsets, the degenerate-triangle test, the push layout, the shadow bucket, the stats, the Vulkan planner's indirect access, and the viewport flip.

Steps, each a commit and a push (draft PR after step 1):
1. This plan, with the author's decisions folded in.
2. RHI commands and validation; conformance cases for indirect draws, on Metal.
3. Viewport, scissor and texture copy, on Metal.
4. Vulkan: the encoder, the planner (tests first), then the `vulkan` workflow on the RHI suite.
5. Scene buffers: blob upload, the mesh table, the instance row, the path rule, buckets and work items.
6. `meshlet.slang`: `expand` and `meshlet_vs`; forward drawing; the `m4_meshlet` goldens collected with `CONCORDIA_GOLDEN_KEEP=1`.
7. Shadows (`meshlet_shadow_vs`), masked and skinned composite goldens.
8. Stats, `[render] meshlets`, the two view modes; editor schemas.
9. The `vulkan` workflow on the render suite and the new goldens; the review page (Metal and lavapipe images, FLIP maps) before blessing.
10. Perf timings, mutation checks, docs (As built with the test log, Status, ADR-0100 accepted, `plans/README.md`, AGENTS/README), the checklist; ready for review.

### Effort (re-estimated)
- The plan's 32–45 weeks, plus **4.1v Vulkan at 6–10**, plus about 1 for D2, D6 and D9: **about 39–56 engineer-weeks**.
- Track R is now about 27–38 weeks on the critical path, with Track G (12–17) after it (R2).

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
- **Note (2026-11-20, ADR-0015):** the RHI's indirect draw commands are shared by this phase's compute fallback, M10 10.7 and M11 11.7 (Sparx); whichever lands first adds them and the others reuse them.
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
  - *Note (2026-11-20, ADR-0015):* particles come from M11 (Sparx); its sprite renderer writes the reactive mask once this phase exists.
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
- **Note (2026-11-20, ADR-0016):** `graph.apply` needs undoable document edits in `concordia-command`. M12 12.8 needs the same operation for rig, clip and pose-library files; whichever phase lands first adds it, records it in its ADR, and the other reuses it.
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
- **Note (2026-11-20, ADR-0016):** M12 recommends (its open question 1) that this panel later gain a view of `.rig.ron` rigs, edited through `rig.apply`, after M12's gate. Rigs don't become a `.graph.ron` kind.
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
   - `canyon_within_frame_budget_on_reference`: both camera paths (reference port isometric camera, and a ground-level fly-through), 2560×1440 output from 1706×960 internal (TAAU 1.5×), with meshlets + VSM + DDGI + TAA all on: **GPU frame time median ≤ 16.6 ms and p95 ≤ 19.0 ms (p95 raised from 18.0 by the author on 2026-11-24: 4.1a saw p95 1.2–2.3 ms above the median even on small scenes)**, and every pass within its budget in `tests/perf/budgets/m4-canyon.budget.ron` (provisional table below).
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
