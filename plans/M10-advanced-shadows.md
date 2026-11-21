# M10: Advanced shadows (tiled shadow atlas)
Status: Planned (ADR-0014) · Depends on: M1 (render graph, CSM, clustered punctual lights, `render.stats`, `ViewMode`s, goldens); M4 4.1 (perf harness) and 4.3 (indirect draws) only if they land first · Detail level: **Outline; needs refinement after M2** · Estimated effort: ~11 weeks without the MDI path, ~13 with it (range 9–16), author plus agents

> Outline plan with phase breakdowns and test lists. Refine in phase 10.0, once M2 has brought skinned meshes and the port's first levels (the content to measure against): measure the atlas's memory and tile-render cost on the reference machine, fix the gate budgets, and settle the open questions, above all how the atlas relates to M4's virtual shadow maps (open question 1).

## Goal & context
M1 shadows only the sun. Point and spot lights are clustered and shaded, but they cast no shadows. The reference port is an isometric ARPG full of torches, lanterns and spell lights, so it needs many shadowed local lights per frame.

M10 adds a **tiled shadow atlas** for local lights:
- one large depth atlas, with one whole tile per light: 512² or 1024² for a spot light, and 6 tiles for a point light's cube faces;
- frustum and occlusion culling per tile, on the CPU;
- **static shadow caching**: static casters are drawn once and kept, and only moving casters inside a light's range are drawn each frame.

The aim is about 80% of virtual shadow maps' benefits at 20% of their architectural complexity. VSM (M4 4.7) shifts hundreds of 128² pages every frame and needs page marking from the depth buffer and GPU-driven page culling. The atlas culls per light instead, so a plain CPU frustum culler over a BVH handles 20–50 shadowed lights per frame. And a light that hasn't moved costs nothing for its static casters: the CPU never submits them.

**No meshlet pipeline is required.** The atlas works with M1's CPU-driven draws. An optional MultiDrawIndirect (MDI) path (10.7) serves GPU-driven renderers, including M4's meshlet path once it exists.

**What M1 leaves us** (in `crates/concordia-render`):
- `shadow.rs`: `CascadeSettings`, `PcfKernel`, `fit_cascades`, `split_distances` (stabilized cascades).
- `shadow_pass.rs`: `ShadowMaps` (persistent depth textures), `ShadowPush`, `ShadowDraw`, `ShadowJob`, `add_shadow_passes`, `world_bounds`.
- `renderer.rs`: `prepare_shadows`, and the whole-map cache `shadow_key` behind `RenderStats::shadows_cached` (reuse only when no caster, light or cascade changed).
- `forward.slang`: `sun_shadow` (filtered PCF; bias = constant + normal offset + slope-scaled, capped).
- `cull.rs`: `Frustum::new(view_proj)` and `Frustum::sees(world, lo, hi)`, used for camera culling.
- `lighting.rs`: `punctual_lights` packs four `float4` rows per light; `cluster.rs` assigns them to clusters.
- `extract.rs`: `DrawItem { cast_shadows, .. }` and `PunctualLight`; `scene_cache.rs`: `SceneCache`, which already knows which entities changed.
- `stats.rs`: `FrameStats` behind `render.stats`; `view_mode.rs`: `ViewMode` (including `shadow_cascades`).

**What the RHI lacks** (`concordia-rhi` `Command`): a viewport/scissor rect inside a render pass, a texture-to-texture region copy, and indirect draws (only `DispatchIndirect` exists). M10 adds the first two, and 10.7 adds the third.

**Clean room.** Shadow atlases and cached shadow maps are long-established techniques with public descriptions (see References). Epic's public VSM documentation is read for comparison only. No Unreal source is read, ported or paraphrased (`docs/clean-room.md`).

## Scope / Non-goals
**Scope**
- Shadows for spot and point lights, in one atlas, with tile sizes chosen per light by importance.
- Per-tile CPU culling: light range, tile frustum, and receiver-aware culling.
- Static and dynamic casters in separate layers, with explicit invalidation rules.
- Filtering consistent with the sun's PCF and bias model.
- The CPU draw path on every backend; an optional MDI path behind `RhiCaps`.
- `render.stats` counters and debug view modes for agents and the editor.
- Perf on the reference machine for 20–50 shadowed lights.

**Non-goals (deferred; see `later-backlog.md`)**
- The sun. It keeps M1's cascades; its static/dynamic split comes with M4's VSM clipmap (open question 6).
- Meshlets and GPU page marking (M4 4.3, 4.4, 4.7).
- Soft shadows beyond PCF: PCSS, shadow-map ray marching (M4 4.7 stage 2) and ray-traced shadows.
- Translucent and colored shadows, and alpha-masked casters beyond a cutout test (decided in 10.4).
- Dual-paraboloid or tetrahedral point-light maps. The author chose six cube faces.
- Skinned meshes as *static* casters. They are always dynamic.

## Dependencies (what must already exist, from which milestone)
| Needed | From |
|---|---|
| Render graph, imported persistent textures, depth-only pipelines | M1 1.5–1.6 |
| CSM: `CascadeSettings`, `PcfKernel`, the bias model, `shadow_key` caching | M1 1.6 |
| Clustered punctual lights (`lighting.rs`, `cluster.rs`), `PointLight`, `SpotLight` | M1 1.6 |
| `render.stats` (`FrameStats`), `ViewMode`, `capture.screenshot` | M1 1.16 |
| Golden images with FLIP (`concordia-testkit`), the self-hosted GPU runner | M1 |
| Incremental extraction (`SceneCache`) for change detection | M1 |
| Skinned meshes (always dynamic casters) | M2 |
| Per-pass GPU timing (`PassTimer`), `tests/perf/reference.ron`, budget files | M4 4.1, if it has landed (else 10.0 builds the minimum and 4.1 extends it) |
| Indirect draws in the RHI | M4 4.3, if it has landed (else 10.7 adds them and 4.3 reuses them) |

## Crates, modules & directories (created / touched)
**Created**
- `crates/concordia-bvh/` *(proposed; open question 8)*: CPU AABB trees with frustum and sphere queries, refit and deterministic builds. It has no renderer dependency. M4's shared ray `bvh` module (4.8) should grow from it, not beside it.
- `concordia-render::shadow_atlas` (module): `settings.rs`, `select.rs` (importance), `alloc.rs` (the atlas allocator), `cull.rs` (per-tile caster lists), `cache.rs` (layers and invalidation), `pass.rs` (graph passes), `mdi.rs` (10.7).
- `examples/m10-shadows/`: original gate scenes (below), procedural, CC0.
- `tests/gates/m10_shadow_atlas.rs`, `tests/gates/m10_shadow_perf.rs`; `tests/golden/m10_shadows/`.

**Touched**
- `concordia-rhi` (+ Metal and Vulkan backends): `Command::SetViewport` and `Command::SetScissor`, `Command::CopyTexture` (region copy, depth formats), and in 10.7 `Command::DrawIndexedIndirect` with a draw count.
- `concordia-render`: `PointLight` and `SpotLight` gain `shadows: Option<LocalShadowSettings>`; `cull.rs` gains world-space AABB and sphere tests; `lighting.rs` packs a shadow index; `forward.slang` gains `local_shadow`; `FrameStats` and `RenderStats` gain `ShadowAtlasStats`; `ViewMode` gains two modes; `AGENTS.md` invariants and references.
- `concordia-assets`: `MeshRenderer` gains `mobility: Mobility` (open question 3).
- `concordia-cli`: `concordia new` writes a commented `[render.shadow_atlas]` table.
- `editor/ui`: regenerated schemas (`just editor-schemas`).
- `concordia-studio`: the tech artist's skill text mentions the new view modes; no new MCP tools (open question 9).

## Phases
### Phase 10.0: Refinement and measurement
- **Objective:** reconcile with the renderer as built and with M4's state, and fix the gate's numbers.
- **Deliverables:**
  - an updated plan, with the author's answers to the open questions;
  - measurements on the reference M3 Pro: tile-render cost per 1,000 caster draws, atlas memory at 4096² and 8192² in `Depth32Float` (and `Depth16Unorm`), a region copy's cost, and the cost of one render pass over a large atlas with scissored tiles (TBDR load and store);
  - the gate scenes in `examples/m10-shadows/`, loading headless;
  - perf plumbing: reuse M4 4.1's harness if it exists; otherwise the minimum it would build first (`tests/perf/reference.ron`, `NotReference`, budget files), so 4.1 extends it rather than duplicating it;
  - ADR-0220 (atlas architecture) drafted.
- **Tests first:**
  - `m10_gate_scenes_load_headless`;
  - the gate test files exist and fail (`tests/gates/m10_*.rs`).
- **Done criteria:** budgets are written to `tests/perf/budgets/m10-*.budget.ron`, never to code; the open questions are answered or deferred by name.

### Phase 10.1: Local-light shadow components and atlas settings
- **Objective:** let a scene ask for shadows on a point or spot light, and let a project size the atlas.
- **Deliverables:**
  - `LocalShadowSettings` (reflected, `deny_unknown_fields`): `resolution: TileResolution` (`Auto` or a fixed power of two, 128–2048), `depth_bias`, `normal_bias`, `pcf: PcfKernel` (reused from `shadow.rs`), `near` (clip distance), `priority` (importance weight);
  - `PointLight.shadows` and `SpotLight.shadows`: `Option<LocalShadowSettings>`, off when absent, like `DirectionalLight.shadows`;
  - `ShadowAtlasSettings` in `concordia.toml` `[render.shadow_atlas]`: `size` (default 4096, set in 10.0), `min_tile`, `max_tile`, `max_shadowed_lights` (default 64, M4's figure), `static_cache` on or off;
  - `Mobility` (`Static`, `Movable`) on `MeshRenderer`, carried into `DrawItem`;
  - extraction carries the settings into `PunctualLight`.
- **Tests first:**
  - `local_shadow_settings_reject_out_of_range_values` (each bound; a descriptive `RenderError`, no panic);
  - `tile_resolution_rejects_non_powers_of_two`;
  - `lights_without_shadows_extract_exactly_as_before` (`PunctualLight` equality with M1);
  - `shadow_atlas_settings_parse_from_concordia_toml` and `unknown_atlas_keys_are_errors`;
  - `mobility_defaults_and_roundtrips_in_scene_text` (`insta`);
  - `schema_snapshots_include_local_shadow_settings` (`insta`).
- **Implementation notes:** validation mirrors `CascadeSettings::validate`. Bias lives in the settings but is applied at sampling (as for the sun), so bias changes never invalidate the cache (10.6). Run `just editor-schemas` after the reflected types change.
- **Done criteria:** a scene with shadowed lights saves, loads and renders exactly as M1 did (no shadows drawn yet).

### Phase 10.2: Light selection and the atlas allocator (CPU)
- **Objective:** each frame, decide which lights get shadows and where their tiles go, deterministically and stably.
- **Deliverables:**
  - `select::importance(light, camera)`: projected screen size of the light's influence sphere × luminous intensity × `priority`, zero when the sphere is outside the camera frustum;
  - `select::choose(lights, settings) -> Selection`: at most `max_shadowed_lights`, by importance, ties broken by entity id;
  - tile size from importance (`Auto`) or the fixed resolution, clamped to `min_tile..=max_tile`, with hysteresis so sizes change only when importance moves past a threshold by a margin;
  - `alloc::AtlasAllocator`: a power-of-two quadtree (buddy) allocator over the atlas; a point light's six faces share one size; lights keep their tile while their size is unchanged; eviction of tiles unused for N frames; when demand exceeds the atlas, the least important lights step down one size first, then drop to unshadowed;
  - `ShadowAtlasStats` begins: `lights_shadowed`, `lights_dropped`, `tiles`, `atlas_used` (fraction), `reallocations`.
- **Tests first:**
  - `allocated_tiles_never_overlap` (`proptest`, random light sets and frame sequences);
  - `tiles_stay_inside_the_atlas` (`proptest`);
  - `tile_sizes_are_powers_of_two_within_bounds` (`proptest`);
  - `point_light_faces_share_one_size`;
  - `more_important_light_never_gets_a_smaller_tile` (`proptest`);
  - `atlas_layout_is_deterministic_for_the_same_inputs` (two runs; and input order shuffled);
  - `stable_lights_keep_their_tiles_across_frames` (the cache depends on it);
  - `hysteresis_stops_size_flapping` (importance oscillating ±5% around a threshold keeps one size);
  - `over_budget_downsizes_least_important_first`;
  - `lights_that_do_not_fit_are_counted_never_silent` (`lights_dropped`, plus a log line, like `cluster_overflow`);
  - `freeing_everything_returns_the_whole_atlas` (no fragmentation leak after churn);
  - `off_screen_lights_get_no_tile`.
- **Implementation notes:** M4 4.7 lists `many_local_lights_select_top_64_by_importance`. This phase writes that selection once, as `shadow_atlas::select`, and VSM reuses it. References: Knowlton's buddy allocator; Jylänki's rectangle-packing survey; Godot 4's positional shadow atlas (MIT, documented quadrant subdivision).
- **Done criteria:** all tests green; allocation of 64 lights takes ≤ 50 µs (criterion bench, `crates/concordia-render/benches/BASELINE.md`).

### Phase 10.3: Caster index and per-tile culling (CPU)
- **Objective:** for each tile, list exactly the casters that can affect it, fast enough for 50 lights over 10,000 casters.
- **Deliverables:**
  - `concordia-bvh`: an AABB tree (binned SAH build, refit, deterministic leaf order) with frustum and sphere queries;
  - two indexes in the renderer: a static BVH (rebuilt or refit only when a static caster changes, driven by `SceneCache` changes) and a small per-frame dynamic list (or BVH) for `Movable` casters;
  - `cull.rs` grows world-space `Frustum::sees_aabb(lo, hi)` and a sphere test; the corner transform moves to the index, so no allocation per test;
  - per tile: range sphere query → tile frustum (spot, or each cube face) → **receiver-aware culling**: a caster is kept only if its shadow, extruded away from the light to the range, can reach the camera frustum;
  - per-tile caster lists split by layer (static, dynamic), sorted by pipeline then entity id (the M1 draw-order rule);
  - `ShadowAtlasStats` gains `casters_tested`, `casters_culled`.
- **Tests first:**
  - `bvh_frustum_query_matches_brute_force` (`proptest`);
  - `bvh_sphere_query_matches_brute_force` (`proptest`);
  - `bvh_refit_after_a_move_matches_a_rebuild` (same query results);
  - `bvh_build_is_deterministic`;
  - `casters_outside_light_range_are_never_listed`;
  - `spot_tile_culls_casters_outside_the_cone`;
  - `cube_face_lists_only_casters_in_its_frustum` (a caster on a face seam is in both);
  - `receiver_aware_culling_has_no_false_negatives` (`proptest`: every caster whose shadow volume meets the view frustum is kept);
  - `cast_shadows_false_is_never_listed`;
  - `tile_caster_lists_are_deterministic_and_ordered`;
  - `cull_50_lights_over_10k_casters_within_budget` (criterion; ≤ 0.5 ms on reference, provisional; also in `just perf-smoke`).
- **Implementation notes:** occlusion culling on the CPU path is receiver-aware culling only. True occlusion (casters hidden behind other casters from the light) comes in the MDI path, against the cached static depth (10.7); masked software occlusion is a measured option (open question 7). References: Ericson, *Real-Time Collision Detection* (AABB trees, frustum tests); Wald 2007 (binned SAH); Assarsson & Möller 2000 (frustum-box tests).
- **Done criteria:** all tests green; the bench numbers are recorded in `BASELINE.md`.

### Phase 10.4: Spot shadows in atlas tiles
- **Objective:** the first shadowed local light on screen, sampled with the sun's filter and bias model.
- **Deliverables:**
  - RHI: `Command::SetViewport` and `Command::SetScissor` on Metal and Vulkan (and the recording test backend);
  - a persistent atlas texture (`ShadowAtlas`, like `ShadowMaps`), imported into the graph;
  - spot projection: perspective, reverse-Z, field of view = 2 × outer angle plus a guard band of PCF-radius texels, `near` to `range`;
  - one depth pass per frame over the atlas (`LoadOp::Load`, tiles cleared by a scissored clear or a depth-writing quad), drawing each tile's casters with a viewport per tile; `ShadowPush` gains a view-projection index instead of `cascade`;
  - shading: `lighting.rs` packs a shadow index in the light's fourth row; a shadow buffer holds each light's matrices and tile rects; `forward.slang`'s `local_shadow` samples with `PcfKernel` and the same bias terms as `sun_shadow`, with the texel size taken at the receiver's distance; taps are clamped to the tile's inner rect.
- **Tests first:**
  - `set_viewport_and_scissor_limit_rasterization` (RHI conformance, every backend);
  - `spot_shadow_footprint_matches_analytic_projection` (a box over a plane: the shadow mask's IoU against the CPU-projected silhouette ≥ 0.98);
  - `pcf_taps_never_read_a_neighbor_tile` (neighbor tiles filled with a near-depth pattern: the image is unchanged);
  - `spot_shadow_has_no_acne_or_peter_panning` (a lit plane is fully lit; a box's contact shadow starts within one texel);
  - `local_bias_terms_match_the_sun_formula` (CPU reference of the shader's bias, against `sun_shadow`'s);
  - `x5_kernel_is_softer_than_x3` (penumbra width);
  - `unshadowed_lights_render_byte_identical_to_m1` (the M1 light goldens, unchanged);
  - `spot_shadow_regression_goldens` (`tests/golden/m10_shadows/spot_*.png`, mean FLIP ≤ 0.01);
  - `atlas_texture_is_reused_across_frames` (`RenderStats::allocations` = 0 once warm).
- **Implementation notes:** keep one render pass over the atlas per frame if 10.0 shows that a scissored pass over a large attachment is cheap on TBDR; otherwise group tiles by region. Alpha-masked casters use a cutout pipeline only if 10.0 finds them in the port's content.
- **Done criteria:** goldens blessed by eye and passing; the spot-light test scene is in the editor's viewport.

### Phase 10.5: Point shadows (six cube faces)
- **Objective:** omnidirectional shadows from six tiles, with no seams.
- **Deliverables:**
  - six face projections (90° plus the guard band), one tile each, all one size;
  - face selection in the shader by the major axis of light-to-receiver; taps stay inside the chosen face's tile;
  - faces whose frustum cannot reach the camera's view are skipped (no draws; the tile still exists so its cache survives).
- **Tests first:**
  - `point_shadow_covers_all_six_directions` (a box on each axis: each casts);
  - `cube_face_seams_are_invisible` (receivers across a face boundary: no discontinuity above a threshold; a golden with a seam mask);
  - `face_selection_matches_cpu_reference` (`proptest` over directions);
  - `faces_outside_the_view_are_not_drawn` (counter);
  - `point_shadow_regression_goldens` (mean FLIP ≤ 0.01).
- **Implementation notes:** references: Gerasimov, "Omnidirectional Shadow Mapping" (GPU Gems, ch. 12); King & Newhall, "Efficient Omnidirectional Shadow Maps" (ShaderX3).
- **Done criteria:** goldens pass; a torch-lit test room looks right by eye.

### Phase 10.6: Static caching and invalidation
- **Objective:** a light that hasn't moved costs nothing for its static casters: the CPU never submits them.
- **Deliverables:**
  - two layers per tile: the **static** layer holds only `Static` casters and persists; the **active** tile is what the forward pass samples;
  - a tile with no dynamic casters in range is its own cache: when nothing changed, it is reused as is (zero draws), like M1's `shadow_key` reuse;
  - a tile with dynamic casters: copy its static layer into the active tile (`Command::CopyTexture`, or a depth-writing quad; 10.0 picks), then draw only the dynamic casters;
  - static copies live in a smaller static-copy pool (open question 4), allocated by the same `AtlasAllocator`;
  - a cache key per tile: light pose, range, angles, near, tile rect and size, and a hash of the static casters listed for it;
  - invalidation rules (all of them; nothing else invalidates):
    1. the light moved, rotated, or changed range, angle or `near`;
    2. a static caster was added, removed, moved, changed mesh or toggled `cast_shadows`, and its old **or** new bounds overlap the light's range;
    3. the tile was reallocated, resized or evicted, or the atlas was resized;
    4. a mesh asset used by a listed static caster was hot-reloaded;
  - not invalidating: bias and PCF changes (applied at sampling), color and intensity, camera motion;
  - a moving light draws everything straight into its active tile and keeps no static layer until it has been still for one frame;
  - a dynamic caster that leaves a light's range triggers one more copy, so its shadow is erased;
  - a `Static` caster that moves on many consecutive frames is logged once (it should be `Movable`), and counted;
  - `ShadowAtlasStats` gains `tiles_drawn`, `tiles_reused`, `static_hits`, `static_draws`, `static_draws_skipped`, `dynamic_draws`, `static_churn`.
- **Tests first:**
  - `static_draws_skipped_when_no_light_moved` (frames 2..N of a static-light scene with walkers: `static_draws` = 0 and `static_draws_skipped` = the static casters in range, every frame);
  - `cached_frames_match_uncached_frames` (a scripted 300-frame sequence with lights and casters moving: images byte-identical with `static_cache` on and off; the key oracle);
  - `cache_invalidation_matches_a_reference_model` (`proptest`: random operations on lights and casters; the set of redrawn tiles equals a brute-force model of the rules above, and no tile is stale);
  - `moving_a_light_redraws_only_its_tiles`;
  - `moving_a_static_caster_invalidates_only_lights_it_overlaps` (old and new bounds);
  - `adding_or_removing_a_static_caster_invalidates_overlapping_lights`;
  - `toggling_cast_shadows_invalidates`;
  - `mesh_hot_reload_invalidates_its_users`;
  - `tile_reallocation_invalidates_its_cache`;
  - `bias_change_does_not_invalidate_the_cache`;
  - `dynamic_caster_redraws_its_tile_without_static_draws`;
  - `dynamic_caster_leaving_a_light_clears_its_shadow_next_frame`;
  - `static_caster_that_keeps_moving_is_reported_once`.
- **Implementation notes:** change detection comes from `SceneCache` (what changed per entity), never from a per-frame scan of every caster. References: Sousa & Geffroy, "The Devil is in the Details: idTech 666" (SIGGRAPH 2016 Advances; a shadow atlas with cached static shadows); Unity HDRP's public documentation on cached and mixed cached shadows; Godot 4 (MIT).
- **Done criteria:** all tests green; in the lantern hall scene, a still frame submits no static draws.

### Phase 10.7: The MDI path (optional, behind caps)
- **Objective:** the same shadows from GPU-written draw lists, for GPU-driven renderers, with occlusion culling of dynamic casters.
- **Deliverables:**
  - RHI: `Command::DrawIndexedIndirect { buffer, offset, count }` (multi-draw; a loop of single draws where the backend lacks a count), shared with M4 4.3's compute + indirect fallback;
  - per-instance data in one buffer (the 4.3 layout if it exists) and a compute pass that culls instances per tile (range, frustum) and writes indirect arguments;
  - **occlusion of dynamic casters against the static layer:** a max-depth pyramid built from each tile's cached static depth; a dynamic caster entirely behind static depth from the light's view adds no shadow and is culled;
  - path choice: `ShadowDrawPath::{Cpu, Indirect}` from `RhiCaps`, overridable in `[render.shadow_atlas]`; cap masking in tests.
- **Tests first:**
  - `indirect_path_matches_cpu_path` (byte-identical images on the gate scenes, or FLIP ≤ 0.001 if draw order differs);
  - `indirect_path_falls_back_to_cpu_when_masked`;
  - `gpu_tile_culling_matches_cpu_lists` (readback of the per-tile instance lists vs 10.3's);
  - `dynamic_casters_hidden_behind_static_depth_are_culled` (counter);
  - `static_depth_occlusion_has_no_false_negatives` (images identical with occlusion on and off);
  - `indirect_draw_calls_are_bounded_by_tiles` (one indirect call per tile and layer).
- **Implementation notes:** references: Haar & Aaltonen, "GPU-Driven Rendering Pipelines" (SIGGRAPH 2015); Wihlidal, "Optimizing the Graphics Pipeline with Compute" (GDC 2016); Greene et al., "Hierarchical Z-Buffer Visibility" (SIGGRAPH 1993).
- **Done criteria:** tests green; the gate runs on both paths.

### Phase 10.8: Tooling: stats, view modes, agents
- **Objective:** agents and the editor can see what the atlas does and why.
- **Deliverables:**
  - `FrameStats.shadow_atlas: Option<ShadowAtlasStats>` in `render.stats` (null when no light is shadowed);
  - view modes, appended to `ViewMode` (the shader index order stays stable):
    - `shadow_tiles`: the lit image tinted by the tile size of the light shadowing each pixel (a `forward.slang` branch, like `shadow_cascades`);
    - `shadow_atlas`: the atlas itself, tiles outlined and tinted by light, static and dynamic layers marked (a separate debug pass; `AGENTS.md`'s view-mode invariant is updated);
  - the tech artist's skill text: how to read the counters and when to mark meshes `Movable`;
  - no new MCP tool: `render_stats` and `capture_screenshot` (with `view_mode`) are already in the tech artist's scope (open question 9).
- **Tests first:**
  - `render_stats_reports_shadow_atlas_counters` (schema snapshot, `insta`; values from a known scene);
  - `shadow_tiles_view_mode_golden`;
  - `shadow_atlas_view_mode_golden`;
  - `view_mode_order_is_stable` (the existing modes keep their numbers);
  - the ADR-0073 role-budget test passes unchanged;
  - editor schemas are fresh (`just editor-schemas` leaves no diff).
- **Done criteria:** an agent can explain a frame's shadow cost from `render.stats` alone.

### Phase 10.9: Gate
Write the gate tests at the start of the milestone (10.0); this phase makes them pass on the reference machine and updates the plan's Status.

## Milestone gate
`just gate M10` runs `tests/gates/m10_shadow_atlas.rs` and `m10_shadow_perf.rs` on original scenes in `examples/m10-shadows/` (our own content, procedural):
- **lantern hall:** 20 static point lights, about 2,000 static casters, 32 walking capsule characters;
- **spell arena:** 48 shadowed lights (24 spot, 24 point: 168 tiles), 16 of them moving, 64 moving characters, isometric camera;
- **analytic bench:** single spot and point lights over boxes and planes, for footprints and seams.

It asserts:
1. **Correctness:** over a scripted 300-frame sequence in each scene, images are byte-identical with the static cache on and off, on both draw paths (CPU, and MDI where caps allow).
2. **Caching:** in the lantern hall's still phase, `static_draws` = 0 every frame, and `dynamic_draws` equals the dynamic casters in range; when one light moves, only its tiles are redrawn.
3. **Quality:** spot footprint IoU ≥ 0.98; no visible cube seams; regression goldens within mean FLIP 0.01.
4. **Scale on reference** (provisional, set in 10.0; 1920×1080): in the spell arena, shadow GPU work ≤ 3.0 ms median in the steady state and ≤ 6.0 ms p95 on a frame where every light moves; CPU shadow preparation (selection, allocation, culling, draw lists) ≤ 1.0 ms for 50 lights. Off-reference runs report `NotReference`.
5. **No silent loss:** lights beyond the atlas or `max_shadowed_lights` are counted in `lights_dropped` and logged.

If M4 4.1's path tracer exists by then, the analytic bench also gets direct-only reference goldens against `concordia-pathtrace`.

## Risks & mitigations
| Risk | Mitigation |
|---|---|
| Atlas memory: two 8192² `Depth32Float` layers are 512 MB | Default 4096²; static copies only for tiles that also have dynamic casters (open question 4); 10.0 measures `Depth16Unorm`. |
| Tile sizes flapping as the camera moves, which empties the cache | Hysteresis and the `stable_lights_keep_their_tiles` test; `reallocations` in `render.stats`. |
| Meshes marked `Static` that actually move, so caches thrash | Invalidation stays correct regardless; `static_churn` and a one-time log tell the author which entity to mark `Movable`. |
| One render pass over a large atlas is slow on Apple's tile-based GPUs | Measured in 10.0; fall back to grouped passes by region. |
| Cube-face seams and PCF reading neighbor tiles | Guard bands, tap clamping, and dedicated seam and neighbor tests. |
| Duplicating M4's VSM work (selection, invalidation, stats, BVH) | One `shadow_atlas::select`, one invalidation model and one `concordia-bvh`; M4 4.7 reuses them (see M4's note). |
| CPU culling cost grows with casters | BVH queries, receiver-aware culling, static caching (statics aren't even listed for unchanged tiles), and the MDI path. |
| Clean-room contamination (VSM is an Unreal feature) | Atlases are older public techniques; Epic's public VSM docs are read for comparison only; every module's `AGENTS.md` lists its references. |

## Open questions
Each is for the author, with a recommendation.
1. **How does the atlas relate to M4's VSM?** Options: an *alternative* (VSM is dropped for local lights), a *precursor* (the atlas ships first and VSM replaces it), or a *fallback tier* (VSM is the default and the atlas serves devices without it). *Recommendation:* a precursor that stays as the default local-light tier. The atlas becomes the local-light shadow path for every project, and the only one without meshlets. M4 4.7 keeps the sun's VSM clipmap, and VSM for local lights becomes an opt-in high tier, kept only where it measurably beats the atlas on the gate scenes. Both share the light selection, the invalidation model and the stats. M4's "spot and point lights in VSM" and "VSM default, CSM fallback" ADR would be updated to say so.
2. **When does M10 run?** *Recommendation:* the CPU phases (10.1–10.3) may start any time after M1; the whole milestone runs before M4 4.7, because the port needs local shadows first and VSM can then build on it.
3. **How is `Mobility` decided?** *Recommendation:* an explicit field on `MeshRenderer`, default `Static`; skinned meshes and bodies with a dynamic or kinematic `RigidBody` are always treated as `Movable`. The Godot importer (M5) maps the source's flags.
4. **Static-cache layout:** a full second atlas copied into the active one, two layers sampled with a min in the shader, or a smaller static-copy pool only for tiles that also have dynamic casters? *Recommendation:* the pool (half the atlas by default). Tiles without dynamic casters are their own cache, so most tiles need no copy, and the shader samples once per tap.
5. **Depth format:** `Depth32Float` (shared with the sun) or `Depth16Unorm` (half the memory)? *Recommendation:* `Depth32Float` unless 10.0 shows D16 passes the goldens with reverse-Z.
6. **The sun:** should its cascades move into the atlas, or gain a static/dynamic split now? *Recommendation:* no. Keep M1's cascades and their whole-map cache; the split arrives with M4's VSM clipmap.
7. **Occlusion culling on the CPU path:** receiver-aware culling only, or masked software occlusion (Hasselgren et al., HPG 2016) too? *Recommendation:* receiver-aware only, plus static-depth occlusion in the MDI path; add software occlusion only if 10.0's measurements on the port's levels show many hidden casters.
8. **Where does the BVH live?** *Recommendation:* a small proposed crate, `concordia-bvh`, with no renderer dependency, so M4's ray `bvh` module and M7's spatial queries grow from it instead of writing a second one.
9. **MCP exposure:** is `render.stats` plus the view modes enough, or should there be a `render.shadow_report` (per-light tiles, cache state, reasons for drops)? *Recommendation:* no new tool in M10. If one is needed later, it goes only into the tech artist's scope and must keep that role under its 22k budget (ADR-0073).
10. **Atlas settings location:** `[render.shadow_atlas]` in `concordia.toml`, or a scene-level component? *Recommendation:* `concordia.toml`, because the atlas is a project-wide device resource, like M4's `render.shadows`.

## ADRs to write
- **ADR-0014** (design block, written, Proposed): the Advanced shadows milestone (M10, tiled shadow atlas).
- **ADR-0220:** shadow atlas architecture: tile sizes, the allocator and eviction, the layers, the shading data, and its relation to VSM (after open question 1).
- **ADR-0221:** static shadow caching: `Mobility`, the invalidation rules, and the static-copy pool.
- **ADR-0222:** indirect shadow draws: the RHI indirect command and its sharing with M4 4.3 (only if 10.7 is built).

## Effort estimate
| Phase | Weeks |
|---|---|
| 10.0 Refinement and measurement | 0.5 |
| 10.1 Components and settings | 0.5 |
| 10.2 Selection and allocator | 1.5 |
| 10.3 Caster index and culling | 1.5 |
| 10.4 Spot shadows in tiles | 2 |
| 10.5 Point shadows | 1 |
| 10.6 Static caching | 2 |
| 10.7 MDI path (optional) | 2 |
| 10.8 Tooling | 1 |
| 10.9 Gate | 1 |
| **Total** | **~11 weeks** without 10.7, **~13** with it (range 9–16) |

## References (appendix)
Public papers, talks, documentation and permissively licensed code only; no Unreal source.
- Lance Williams, "Casting Curved Shadows on Curved Surfaces" (SIGGRAPH 1978).
- Reeves, Salesin, Cook, "Rendering Antialiased Shadows with Depth Maps" (SIGGRAPH 1987): percentage-closer filtering.
- Holbert, "Saying goodbye to shadow acne" (GDC 2011 poster); Michal Valient, "Stable Rendering of Cascaded Shadow Maps" (ShaderX6): the M1 bias and stability model this milestone keeps.
- Philipp Gerasimov, "Omnidirectional Shadow Mapping" (GPU Gems, 2004, ch. 12); King & Newhall, "Efficient Omnidirectional Shadow Maps" (ShaderX3, 2004).
- Olsson, Billeter, Sintorn, Kämpe, Assarsson, "Efficient Virtual Shadow Maps for Many Lights" (I3D 2014) and "More Efficient Virtual Shadow Maps for Many Lights" (IEEE TVCG 2015): many shadowed lights with clustered shading.
- Tiago Sousa, Jean Geffroy, "The Devil is in the Details: idTech 666" (SIGGRAPH 2016, Advances in Real-Time Rendering): a shadow atlas with cached static shadows.
- Unity, High Definition Render Pipeline documentation, "Shadows" (shadow atlases, cached and mixed cached shadows): public documentation, feature expectations only.
- Godot Engine 4 (MIT): the positional shadow atlas and its documentation; permissively licensed.
- Kenneth Knowlton, "A Fast Storage Allocator" (CACM 1965): the buddy allocator; Jukka Jylänki, "A Thousand Ways to Pack the Bin" (2010).
- Christer Ericson, *Real-Time Collision Detection* (2004): AABB trees and frustum tests; Ingo Wald, "On fast Construction of SAH-based Bounding Volume Hierarchies" (IEEE RT 2007); Assarsson & Möller, "Optimized View Frustum Culling Algorithms for Bounding Boxes" (JGT 2000).
- Hasselgren, Andersson, Akenine-Möller, "Masked Software Occlusion Culling" (HPG 2016).
- Greene, Kass, Miller, "Hierarchical Z-Buffer Visibility" (SIGGRAPH 1993); Haar & Aaltonen, "GPU-Driven Rendering Pipelines" (SIGGRAPH 2015); Wihlidal, "Optimizing the Graphics Pipeline with Compute" (GDC 2016).
- Epic Games, "Virtual Shadow Maps" (public Unreal Engine 5 documentation): for comparison only.
