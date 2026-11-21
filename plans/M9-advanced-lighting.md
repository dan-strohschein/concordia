# M9: Advanced lighting (Photon global illumination)
Status: Planned (ADR-0013) · Depends on: M4 (ray queries and proxy BLAS, HZB, VSM, temporal AA/upscaling, the reference path tracer, the `gi::Technique` seam, the perf harness), M2 phase 2.3 (Substrate-style material programs) · Detail level: **Needs refinement after M4** · Estimated effort: ~28 weeks (range 22–36), author plus agents

> Outline plan with phase breakdowns and test lists. Refine when M4 lands: the ray-tracing capability matrix, the proxy BLAS and the meshlet scene buffers as built, the temporal resolve, M4's DDGI probes (which Photon's radiance cache should reuse), and the gate scenes' budgets on the reference machine.

## Goal & context
**Photon** is Concordia's fully dynamic global illumination and reflections system, in the class of Unreal Engine 5's Lumen. Indirect light, bounce light, emissive lighting, sky occlusion and reflections all respond within a few frames to any change: a light moves, a door opens, a wall breaks (M8), the time of day changes. Nothing is baked.

**Clean room.** Lumen is part of Unreal Engine, which is source-available, not open source. Photon is a reimplementation of the *techniques* Epic has described in public (SIGGRAPH Advances talks, public documentation) plus the open research they build on, never a copy of code. No Unreal source may be read, ported or paraphrased, and no contributor who has studied Unreal's Lumen source may write Photon modules (`docs/clean-room.md`). Every module's `AGENTS.md` lists the references in the appendix that it used.

**What M4 leaves us:**
- `RhiCaps::{ray_query, hw_ray_tracing}`, ray queries over a **proxy BLAS per asset**, and the shared compute `bvh` fallback;
- meshlet scene buffers and the two-pass HZB;
- virtual shadow maps, the clustered lights, and TAA/TAAU with MetalFX;
- `concordia-pathtrace`, the reference oracle;
- DDGI-style probes (stage A) behind `gi::Technique`, with a `ScreenProbes` skeleton for stage B;
- the perf harness and budget files.

M4's open question 4 ("stage B: M4.5 or backlog?") is answered by this milestone: **stage B is Photon**.

**What Photon adds, in the shape of the public Lumen design:**
1. **A scene representation for tracing that is independent of the triangles:**
   - per-mesh signed distance fields;
   - a global distance-field clipmap around the camera;
   - a **surface cache**: each mesh is covered by a few oriented "cards" that capture its material (albedo, normal, emissive, opacity) into an atlas.
2. **Surface cache lighting:** direct light (clustered lights plus VSM) and multi-bounce radiosity are computed in the cards, amortized over frames. A ray hit only has to read the cache, never shade the full material.
3. **A tracing pipeline from cheap to expensive:** screen-space traces against the HZB first, then distance fields (mesh SDFs near, global SDF far), or hardware ray queries where the device has them. Hits are lit from the surface cache.
4. **Final gather:**
   - adaptive **screen probes** traced with importance sampling;
   - a world-space **radiance cache** for long rays (reusing M4's probe infrastructure);
   - spatial and temporal filtering, and integration to full resolution with short-range ambient occlusion.
5. **Reflections:** rough reflections reuse the screen probes; glossy and mirror reflections trace dedicated rays, denoised. Integrated with the Substrate-style closures (haze, layered slabs) from M2 2.3.
6. **Translucency and fog:** a froxel lighting volume for translucent surfaces and volumetric fog; sky lighting and far-field tracing.
7. **Tooling:** debug view modes, `render.stats` breakdowns, scalability tiers, and a Studio skill for the tech artist.

## Scope / Non-goals
**Scope**
- Fully dynamic diffuse GI, specular reflections, emissive lighting, sky occlusion and translucency lighting, on both tracing paths:
  - **hardware** ray queries (the reference M3 Pro has hardware ray tracing);
  - **software** distance fields (devices without hardware ray tracing).
- Static and rigid moving meshes, including meshlet geometry.
- Scalability tiers (low / medium / high / epic), with budgets in the perf harness.
- Agent-facing tools: view modes, stats, an MCP-readable GI report, and a skill.

**Non-goals (deferred; see `later-backlog.md`)**
- Skinned and deforming meshes *in the scene representation*. They receive GI but are traced as capsules or proxies at most; revisit after M8 garments.
- Baked lightmaps (Photon replaces them; a bake path, if ever needed, is its own ADR).
- Path-traced final output (the path tracer stays the oracle).
- D3D12 and consoles.
- Caustics and spectral transport.

## Dependencies (what must already exist, from which milestone)
| Needed | From |
|---|---|
| `RhiCaps` (`ray_query`, `hw_ray_tracing`), ray queries, proxy BLAS per asset, shared `bvh` fallback | M4 4.1, 4.8 |
| Meshlet scene buffers, per-instance data on the GPU, two-pass HZB | M4 4.3, 4.4 |
| Virtual shadow maps (shading surface-cache hits) | M4 4.7 |
| TAA/TAAU, motion vectors, history buffers, pre-exposure | M4 4.6 |
| `gi::Technique` trait, DDGI probes (radiance cache base), `ScreenProbes` skeleton | M4 4.8 |
| `concordia-pathtrace` reference oracle, quality methodology, perf harness and budget files | M4 4.1 |
| Substrate-style material programs (card capture, closures for reflections) | M2 2.3 |
| DDC (`.concordia/ddc`) for derived SDFs and card layouts | M1 1.4 |
| Destruction (dynamic scene changes in the gate's stress test; optional) | M8 |

## Crates, modules & directories (created / touched)
**Created**
- `crates/concordia-sdf/` *(proposed)*: mesh distance fields, computed offline (sparse bricks, deterministic, stored in the DDC), plus CPU reference queries (distance, sphere tracing). It doesn't depend on the renderer, so the path tracer, VFX and physics can use it.
- `crates/concordia-photon/` *(proposed)*: the GI system as render-graph passes: global SDF clipmap, surface cache (cards, atlas, capture, lighting), tracers, screen probes, radiance cache, reflections and the translucency volume. It implements M4's `gi::Technique`.
- `examples/m9-photon/`: original gate scenes (below).

**Touched**
- `concordia-render`: Photon passes in the frame graph, a thin normal/roughness buffer if the forward+ path lacks one (decided in 9.0), view modes, and `render.stats`.
- `concordia-assets`: an SDF and card-layout build step per mesh asset (an importer-side derived artifact).
- `concordia-studio`: the tech artist's scope and a `concordia-tune-gi` skill.

## Phases
### Phase 9.0: Refinement against M4 as built
- **Objective:** reconcile with M4 and set the gate scenes and budgets.
- **Deliverables:** an updated plan. Decisions on:
  - the normal/roughness buffer (forward+ has none for screen probes);
  - whether the radiance cache reuses M4's DDGI probes directly;
  - the software tracing path: required everywhere, or a fallback tier only (open question 1);
  - budgets on the reference machine.
  - ADR-0200 (Photon architecture).
- **Tests first:** the gate test files exist and fail (`tests/gates/m9_*.rs`), and the gate scenes load headless.

### Phase 9.1: Mesh distance fields (`concordia-sdf`)
- **Deliverables:**
  - sparse-brick SDFs per mesh: resolution from bounds and a quality setting, a narrow band, mips;
  - a deterministic build stored in the DDC;
  - CPU sphere tracing;
  - a GPU upload format.
- **Tests first:**
  - `sdf_of_analytic_shapes_matches_exact_distance` (sphere, box, torus, within half a voxel);
  - `sdf_sign_is_correct_for_closed_and_reports_open_meshes`;
  - `sdf_build_is_deterministic_and_cached`;
  - `sphere_trace_hits_match_triangle_raycast` (against the `bvh` library, within one voxel);
  - `thin_walls_keep_a_minimum_thickness` (no tracing through 10 cm walls).

### Phase 9.2: Global distance field clipmap
- **Deliverables:**
  - camera-centered clipmap levels composed from the instances' mesh SDFs on the GPU;
  - incremental updates for moved or changed instances only;
  - a coarse far-field level.
- **Tests first:**
  - `global_sdf_matches_union_of_mesh_sdfs` (GPU readback vs CPU);
  - `moving_one_instance_updates_only_its_bricks`;
  - `clipmap_scrolls_without_full_rebuild`;
  - `global_sdf_trace_agrees_with_mesh_sdf_trace_near_the_camera`.

### Phase 9.3: Surface cache: cards and capture
- **Deliverables:**
  - automatic card placement per mesh (oriented boxes from surface clustering, a capped count) and card LODs by screen size;
  - a capture pass that rasterizes the material (via the 2.3 material program) into an atlas of albedo, normal, emissive and opacity;
  - an atlas allocator with a per-frame capture budget.
- **Tests first:**
  - `cards_cover_every_visible_surface_of_test_meshes` (coverage ≥ 99% of the area);
  - `card_capture_matches_material_eval` (albedo and emissive against `concordia-material`);
  - `atlas_allocation_is_deterministic_and_reuses_freed_space`;
  - `capture_respects_the_per_frame_budget_and_prioritizes_by_screen_size`.

### Phase 9.4: Surface cache lighting
- **Deliverables:**
  - direct light into the cards (clustered lights, VSM shadows, sky);
  - radiosity: cards trace a few rays per texel against the scene representation for multi-bounce;
  - temporal accumulation, and an update scheduler (dirty cards first, then round robin).
- **Tests first:**
  - `card_direct_lighting_matches_forward_shading` (same point, same lights);
  - `radiosity_converges_to_path_traced_irradiance_in_a_box_room`;
  - `card_lighting_furnace_has_no_energy_gain`;
  - `moving_light_updates_card_lighting_within_n_frames`.

### Phase 9.5: Tracing pipeline
- **Deliverables:**
  - a `Tracer` trait with `ScreenTracer` (HZB), `SdfTracer` (mesh SDFs near, global SDF far) and `HardwareTracer` (ray queries over the proxy BLAS);
  - a hit-lighting lookup in the surface cache;
  - a fallback chain chosen from `RhiCaps`.
- **Tests first:**
  - `screen_trace_hits_match_depth_buffer_geometry`;
  - `software_and_hardware_tracers_agree_on_hit_distance` (within tolerance on the gate scenes);
  - `hit_lighting_reads_the_card_under_the_hit`;
  - `missing_rays_fall_back_to_sky_and_far_field`;
  - `tracer_choice_follows_caps` (masked caps in tests).

### Phase 9.6: Final gather: screen probes and radiance cache
- **Deliverables:**
  - adaptive screen-probe placement (with extra probes at depth and normal discontinuities);
  - octahedral probe radiance with importance sampling (BRDF × the previous frame's lighting);
  - a world-space radiance cache for long rays (clipmapped probes, building on M4's DDGI probes);
  - spatial and temporal filtering, and full-resolution integration with short-range AO and bent normals;
  - `Photon` as the `gi::Technique` (DDGI stays the low tier).
- **Tests first:**
  - `photon_gi_matches_path_traced_indirect` (mean FLIP ≤ 0.08 on the gate scenes; indirect only);
  - `photon_beats_ddgi_on_contact_gi` (FLIP improvement on the contact-shadow scene);
  - `no_light_leaks_through_20cm_walls` (irradiance behind a lit wall ≤ ε);
  - `emissive_surfaces_light_a_dark_room`;
  - `gi_responds_within_8_frames_after_a_door_opens`;
  - `gi_furnace_test_no_energy_gain`.

### Phase 9.7: Reflections
- **Deliverables:**
  - rough reflections from the screen probes;
  - dedicated reflection rays above a roughness threshold, through the tracer chain, with hit lighting from the surface cache (or full hit shading at the epic tier);
  - a spatio-temporal denoiser;
  - per-closure reflections for the Substrate-style slabs (haze lobe, clear coat over base).
- **Tests first:**
  - `mirror_reflection_matches_path_traced` (mean FLIP ≤ 0.05);
  - `glossy_reflection_matches_path_traced` (≤ 0.08);
  - `clear_coat_reflects_twice` (coat and base lobes both present);
  - `reflections_are_stable_under_camera_motion` (temporal flicker metric).

### Phase 9.8: Translucency, fog, sky and far field
- **Deliverables:**
  - a froxel lighting volume, fed from the radiance cache, for translucent surfaces and volumetric fog;
  - sky occlusion and sky lighting;
  - far-field tracing beyond the clipmap.
- **Tests first:**
  - `translucent_surface_receives_gi_from_the_volume`;
  - `fog_is_lit_by_bounce_light`;
  - `sky_occlusion_darkens_interiors` (path-traced comparison);
  - `far_field_hits_are_lit_from_the_coarse_level`.

### Phase 9.9: Scalability, tooling and Studio
- **Deliverables:**
  - tiers (low = DDGI; medium = Photon with software tracing and probe reflections; high = hardware tracing; epic = hit shading), selected in `concordia.toml` `[render] gi` with a per-platform default;
  - view modes: surface cache, cards, distance fields, screen probes, radiance cache and GI only;
  - `render.stats` per pass;
  - an MCP-readable GI report (card counts, atlas use, update lag);
  - the tech artist's scope and a `concordia-tune-gi` skill, with Studio sync snapshots.
- **Tests first:**
  - `each_tier_stays_within_its_budget_on_reference` (perf);
  - `view_modes_render_goldens`;
  - `gi_report_names_stale_and_missing_cards`;
  - `tech_artist_can_tune_gi_over_mcp` (deterministic agent-gate step);
  - the skill tests.

### Phase 9.10: Gate

## Milestone gate
`just gate M9` runs M9's tests on the original scenes in `examples/m9-photon/` (our own content):
- **atrium:** sun through an opening, bounce light;
- **dark room:** lit only by emissive panels;
- **door:** a door opens on a lit room;
- **gallery:** mirror, glossy and clear-coated objects;
- **fog hall:** volumetric fog with a translucent screen.

It asserts:
1. **Quality against the path tracer:** indirect-only mean FLIP ≤ 0.08 and full-frame mean FLIP ≤ 0.10 (atrium, dark room, gallery); `FLIP(Photon) ≤ 0.7 × FLIP(DDGI)` on the atrium.
2. **Dynamics:** after the door opens, indirect light in the room reaches 90% of its converged value within 8 frames, and is stable to ±1% when nothing changes.
3. **No leaking:** irradiance behind 20 cm walls stays under ε on the door scene.
4. **Both tracing paths:** the hardware and software paths each pass 1–3, the software path with looser tolerances set in 9.0.
5. **Performance on reference:** the high tier's Photon passes total ≤ 4.0 ms at M4's internal resolution (refined in 9.0); every tier stays within its budget file.
6. **Energy:** furnace tests pass for card lighting and the final gather.

An optional live gate has the Studio tech artist tune GI on the atrium scene over MCP to meet a stated FLIP target.

## Risks & mitigations
| Risk | Mitigation |
|---|---|
| **Clean-room contamination** (Lumen is well known, and its source is readable by anyone with an Unreal license) | Only the public talks and documentation in the appendix are used; agents are never given Unreal source. Each module's `AGENTS.md` cites its references, reviewers check the references cover each algorithm, and contributors who have read Lumen's source don't write Photon. |
| The scope is very large (Lumen took a team years) | Tiers ship incrementally: DDGI (M4) keeps working at every step, and each phase is useful on its own (SDFs → software AO and shadows; surface cache → cheaper hit lighting). The gate fixes quality metrics, not feature parity. |
| The forward+ renderer has no G-buffer for screen probes | 9.0 decides on a thin normal/roughness prepass. Its cost is measured against the budget. |
| Leaking and noise artifacts | Dedicated leak and flicker tests; SDF minimum thickness; probe placement at discontinuities; path-traced comparisons on every phase. |
| Software tracing too slow on non-RT devices | It is the medium tier with lower ray counts; a device without hardware ray tracing may stay on DDGI if the budget can't be met (open question 1). |
| Card coverage fails on complex or thin meshes | A coverage test per asset at import, reported in the GI report; a fallback to SDF-only hit lighting with the albedo from the mesh's average material. |

## Open questions
1. Must the software distance-field path meet the gate on every platform, or is it a fallback tier (with DDGI as the floor)? Proposed: required on the reference machine, with hardware ray tracing disabled through `CapsMask`; a fallback elsewhere.
2. Should the radiance cache *be* M4's DDGI probe volume (one system, two consumers), or a separate clipmap? Proposed: reuse, decided in 9.0.
3. Skinned meshes in the scene representation: capsules, a per-frame low-res SDF, or nothing until M8 garments?
4. Should Photon ship a `.photon.ron` per-scene settings document, or live entirely in `concordia.toml` and per-volume components?

## ADRs to write
- **ADR-0013** (design block, written): the Advanced lighting milestone (M9, Photon).
- **ADR-0200:** Photon architecture: the scene representation (SDFs plus surface cache), the tracer chain, the final gather and the tiers.
- **ADR-0201:** the mesh distance field format and build (sparse bricks in the DDC).
- **ADR-0202:** the surface cache: card placement, the atlas, and its update scheduling.

## Effort estimate
| Phase | Weeks |
|---|---|
| 9.0 Refinement | 1 |
| 9.1 Mesh SDFs | 3 |
| 9.2 Global SDF | 2 |
| 9.3 Cards and capture | 3 |
| 9.4 Card lighting and radiosity | 3 |
| 9.5 Tracing pipeline | 3 |
| 9.6 Final gather | 5 |
| 9.7 Reflections | 3 |
| 9.8 Translucency, fog, sky | 2 |
| 9.9 Scalability, tooling, Studio | 2 |
| 9.10 Gate | 1 |
| **Total** | **~28 weeks** (range 22–36) |

## References (appendix)
Public talks and documentation only; no Unreal source.
- Daniel Wright, Krzysztof Narkowicz, Patrick Kelly, "Lumen: Real-time Global Illumination in Unreal Engine 5" (SIGGRAPH 2022, Advances in Real-Time Rendering).
- Daniel Wright, "Radiance Caching for Real-Time Global Illumination" (SIGGRAPH 2021, Advances in Real-Time Rendering).
- Epic Games, public Lumen documentation ("Lumen Global Illumination and Reflections", "Lumen Technical Details"): feature expectations and tiers only.
- Daniel Wright, "Dynamic Occlusion with Signed Distance Fields" (SIGGRAPH 2015, Advances in Real-Time Rendering).
- John C. Hart, "Sphere Tracing: A Geometric Method for the Antialiased Ray Tracing of Implicit Surfaces" (The Visual Computer, 1996).
- Sebastian Aaltonen, "GPU-based clay simulation and ray-tracing tech in Claybook" (GDC 2018): SDF tracing at scale.
- Morgan McGuire, Michael Mara, "Efficient GPU Screen-Space Ray Tracing" (JCGT 2014).
- Majercik et al., "Dynamic Diffuse Global Illumination with Ray-Traced Irradiance Fields" (JCGT 2019) and "Scaling Probe-Based Real-Time Dynamic Global Illumination for Production" (JCGT 2021): the radiance cache's probe base (shared with M4).
- Schied et al., "Spatiotemporal Variance-Guided Filtering" (HPG 2017): denoising.
- Ouyang et al., "ReSTIR GI: Path Resampling for Real-Time Path Tracing" (HPG 2021): optional resampling in the final gather.
- Tomasz Stachowiak, "kajiya" renderer (Embark Studios, MIT/Apache-2.0): permissively licensed reference for a hybrid GI pipeline.
- Cigolle et al., "A Survey of Efficient Representations for Independent Unit Vectors" (JCGT 2014): octahedral probes.
- Apple, "Discover Metal ray tracing" (WWDC talks); Khronos `VK_KHR_ray_query`.
