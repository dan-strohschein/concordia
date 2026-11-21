# concordia-render: agent guide

## Purpose
The renderer (ADR-0040): render graph, forward shading of Substrate-style materials (ADR-0068), cameras, headless capture, and `capture.screenshot` (via `RenderModule`).

## Public entry points
- `graph::RenderGraph`: `create_texture` (transient), `import_texture`, and `add_pass` / `add_pass_recorded` with `PassBuilder::uses(texture, Use)` and `side_effect()`. `compile(footprint)` returns a `CompiledGraph` with `pass_names`, `barriers_before`, `memory_plan`, `to_dot` and `execute`.
- Components: `Camera` (`Projection` perspective/orthographic, reverse-Z; `Msaa`; `exposure_ev100`; `Tonemap`; `priority`), `DirectionalLight`, and `PointLight` / `SpotLight` (lumens, range, cone half-angles).
- `tonemap::apply` gives CPU references for AgX (default), Khronos PBR Neutral, ACES fitted and Linear; the shader must match them (tested).
- `cluster::{ClusterGrid, ClusterSettings, ClusterCuller}`: the froxel grid (16×9×24; exponential slices for perspective, linear for ortho), the CPU reference `assign`, and the GPU culler (`cull` reads back). `Renderer::stats()` reports light counts and cluster overflow.
- `Environment` (HDR equirect `map`, `intensity`, `skybox`) and `ibl::{Equirect, sh9_irradiance, eval_sh9, prefilter, integrate_brdf, brdf_lut}`: image-based lighting computed on the CPU (deterministic, tested). The BRDF table and sampling come from `concordia-material` (re-exported, 2.3a). The renderer caches six prefiltered levels and SH9 per map, plus one BRDF table per renderer, built on first use. An `Environment` has no position: entities with one are extracted even without a `Transform` (`extract::placement` / `placements`, shared by full extraction and the scene cache).
- `shadow::{CascadeSettings, fit_cascades, split_distances}`: `DirectionalLight.shadows` enables cascaded shadows. Cascades are fitted on the CPU (practical splits, bounding spheres snapped to the texel grid, light depth covering all casters).
- `RenderWorld::extract(&World)` copies draws, lights and cameras with world transforms, in entity-id order. `camera(Option<EntityId>)` picks the requested camera, or else the highest priority.
- `Renderer::new(Arc<dyn Device>, Box<dyn ShaderCompiler>)` and `render(&RenderWorld, &mut dyn AssetSource, &RenderRequest) -> Image` (sRGB RGBA8).
- `AssetSource` (meshes, layered `Material`s, textures) is implemented for `AssetDb` and `MemoryAssets` (tests, procedural content, built-ins).
- Closure budget (ADR-0068): `platform_closure_budget(&DeviceCaps)` (8 on Metal), `effective_closure_budget(platform, project)`, `Renderer::set_closure_budget` / `closure_budget` / `closure_reports` (the last frame's downgraded materials). `ClosureBudget` and `ClosureReport` are re-exported from `concordia-material`.
- `RenderModule::new(DeviceFactory)` (`.with_closure_budget(..)` for a project's `[render] closure_budget`) provides `capture.screenshot` (result `image: EncodedImage`, which MCP returns as image content) and `render.stats` (the last frame's `FrameStats`, recorded by the render thread, with `closure_budget` and `downgraded_materials`: `MaterialDowngrade` / `ClosureMergeStats` / `MergedOperator`).
- `ibl::albedo_table()`: the split-sum, direct-lobe and sheen albedo tables as one 64×32 RGBA16F texture.
- `ViewMode` (`RenderRequest::view_mode`, `capture.screenshot` / `viewport.overlays` `view_mode`): lit, unlit, wireframe, normals, albedo, roughness_metallic, overdraw, shadow_cascades, light_clusters, closures, haze, fuzz, subsurface, thin_film.
- Gizmos: `GizmoMode` (move, rotate, scale) on `GizmoOverlay`; `gizmo_handles` / `gizmo_rings`, `gizmo_hit` / `ring_hit`, `gizmo_pick(mode, ..)`; drag math `axis_delta`, `rotate_angle`, `scale_factor`.

## Invariants
- Frame: light clustering (compute) → one depth pass per shadow cascade (persistent maps imported into the graph) → forward pass (HDR `Rgba16Float`, depth `Depth32Float` cleared to 0 with `GreaterEqual`, optional 4× MSAA resolved into HDR) → tonemap (Khronos PBR Neutral or clamp) → sRGB output → readback. All of it is scheduled by the render graph, with transients placed in one heap per frame.
- Shader data read through pointers uses only `float4`/`uint4` rows (identical layout on every backend). Push constants are mirrored with `shader_struct!` and checked by `check_layout`.
- Materials (ADR-0068): at upload each material is fitted to the closure budget (`fit_budget`) and compiled (`MaterialProgram`), and kept until its content or the budget changes (`programs.rs`); a downgrade is logged once (`material simplified to fit the closure budget`, fields `material`, `closures`, `budget`, `merged`, `dropped`) and reported in every frame's `render.stats` that draws it. The material buffer holds, per material, a header row, one row per op and eight rows per closure (`material_rows.rs`, mirrored in `forward.slang`); `DrawPush.material` is the first row. At most 8 closures and 64 ops.
- `forward.slang` first runs the mask test from the program's alpha alone (`program_alpha`), then loads each closure (textures applied, and its view-only terms: N·V, the diffuse share 1 − E_spec(N·V), the sheen albedo, and the view-side transmittance for layered programs) into the thread-private `g_closures`, and runs the program on a stack: once per light (sun, then each clustered punctual light), once for image-based lighting (each closure lit by the environment, transmittance toward the mirror direction = toward the view), and once for the albedo (unlit, albedo view). A lit one-slab program skips `g_closures` and the stack entirely (registers only), and a slab with no optional features uses `eval_plain`; light-side transmittance is computed only when the header's `FLAG_LAYERED` says there is a vertical layer.
- **Metal miscompile (Slang 2026.22, M3 Pro):** discarding after the closures are written to `g_closures` (or an `inout` array) dropped every masked fragment though the compared values were right; passing the array by value works but is ~40× slower. Keep the mask test before the closure loads; `masked_materials_keep_fragments_at_or_above_the_cutoff` guards it. It mirrors `concordia-material` exactly; `shader_matches_cpu_reference_per_feature` checks every feature and operator against `MaterialProgram::eval` (≤ 1.2 sRGB steps), and mutations of each term are caught by it.
- Energy (ADR-0068): diffuse, direct and ambient, is scaled by 1 − E_spec(N·V) from the direct-lobe table, not M1's 1 − F(V·H). The albedo tables are uploaded on the first frame, with or without an environment.
- Unlit and emissive output are display-referred; exposure scales only light.
- IBL per closure: diffuse from SH9 irradiance (× 1 − E_spec); specular = lerp of two prefiltered equirect levels (roughness × 5) × (F0·A + B) from the split-sum table, per lobe (haze blended by weight; thin film replaces F0 at N·V); sheen = fuzz × E_sheen × the prefiltered level at the sheen roughness, over the rest dimmed by 1 − max(fuzz)·E_sheen; all scaled by intensity × exposure and by occlusion. The sky is drawn first in the forward pass (no depth test or write); its rays come from unprojecting each pixel at two depths, which works for ortho too. Equirect u = 0.5 at -Z, increasing toward +X; v = 0 straight up.
- Shadows: reverse-Z depth; filtered PCF (a box of bilinear compares, kernel radius 1 or 2); bias = constant + normal offset (texels) + slope-scaled ((radius + 1) texels × tan θ, capped at 10), so the whole PCF footprint clears the receiver. Stabilized cascades move in whole texels (the shimmer test pans one pixel per frame: 0 changed pixels when stabilized vs ~900 without).
- Punctual lights: intensity = lumens / 4π cd (spots are not concentrated by their cone); inverse-square falloff windowed to zero at `range`. Clusters hold at most `max_per_cluster` lights, in ascending index order; dropped assignments are counted (`RenderStats::cluster_overflow`) and logged, never silent.
- Transparency: `AlphaMode::Blend` draws after the opaque ones, back to front by the view depth of their bounds' center, with premultiplied alpha and no depth write; `Mask` discards below the cutoff.
- Rendering is deterministic on one device (same scene → same bytes); draws are ordered by pipeline, then entity id.
- View modes are one branch in `forward.slang`'s `fs` on `Frame.debug.x` (the `ViewMode` order). Debug data views (unlit, wireframe, normals, albedo, roughness/metallic, overdraw, closures and the four feature views) skip the tonemap curve (`Linear`, clamped) and, except unlit, the sky. Albedo and unlit show the program's combined albedo (`MaterialProgram::unlit`); normals and roughness/metallic show the first closure; `closures` colors the closure count (blue 1, green 4, red 8); haze (weight), fuzz (color), subsurface (wrap per channel) and thin film (thickness / 1000 nm) combine each closure's value through the operators, black where absent. Wireframe draws barycentric edges from the corner index (vertices are pulled per triangle corner), so no fill-mode support is needed; it and overdraw use `Pipelines::debug` (no culling, no depth test; overdraw blends additively).
- `render.stats` GPU time comes from the device's completion handler (`Device::submit_timed` for captures, the present callback for viewport frames); it may arrive before the frame is recorded, and `stats::Last` keeps it until then.
- Gizmo rings are unit circles projected and scaled about the entity's screen position to the arrow length (60 px), so an edge-on ring is a line. Rotate angles are measured around the entity on screen (counter-clockwise is positive when the axis faces the eye); scale grows by one per arrow length dragged outward, never below 0.01.
- The device and shaders are created on the first capture, so engines start without a GPU. A missing device is `GpuUnavailable`.

## How to test
`cargo nextest run --workspace -E 'package(concordia-render)'`. The graph, camera and extraction tests are CPU-only. Render and capture tests skip without a GPU unless `CONCORDIA_REQUIRE_GPU=1`. glTF conformance goldens (`tests/gltf.rs`) need `just gltf-assets`. Goldens live in `tests/golden/render/`: inspect a new image by eye, then run `CONCORDIA_BLESS=1` (never in CI).

## Performance
- `cargo bench -p concordia-render --bench many_lights` (M3 Pro, 2026-11-18): 1,024 point lights at 1920×1080 ortho, median **6.5 ms per frame wall clock**. Re-measured 2026-11-20 on the same Mac: main (M1 shading) 2.5–3.2 ms median, best 2.46 ms; 2.3b's closure interpreter 2.9–3.2 ms median, best 2.90 ms (one-slab materials, about 15–18% slower). A first version that re-ran view-only terms and copied the closure array per light took 130 ms. Lit one-slab materials keep their closure in registers and featureless slabs take `eval_plain`; four feature branches per light cost about 0.7 ms. That includes CPU work, a synchronous submit and an 8 MB readback. GPU-only time needs timestamp queries, which the RHI doesn't have yet; the plan's target is < 4 ms GPU.

## Common tasks
- **Inspect a frame:** `compiled.to_dot(&graph)` gives Graphviz text; `memory_plan()` lists transient placements.
- **Add a pass:** declare its uses in `add_pass_recorded`. Record commands only, since the graph derives barriers. Look up bindless indices inside the record function (it may return an `RhiError`).
- **Change shader data:** edit `shaders/*.slang` and `frame.rs` (or `material_rows.rs`) together, keeping whole `float4`/`uint4` rows; push structs must pass the layout test.
- **Change a BSDF term:** change `concordia-material` first (with its reference values), then the same lines in `forward.slang`; `shader_matches_cpu_reference_per_feature` must stay within 1.2 steps, and add a case if the term is not yet exercised (mutate it to check).

## Don'ts
- Don't record barriers by hand inside passes.
- Don't make compilation or draw order depend on hash-map iteration.
- Don't sample with anything but the static samplers (`g_textures.samplers[SAMPLER_*]`).
- Don't change a shading formula in `forward.slang` alone: it must stay the mirror of `concordia-material`.

## References
- Yuriy O'Donnell, "FrameGraph: Extensible Rendering Architecture in Frostbite" (GDC 2017); Hans-Kristian Arntzen, "Render graphs and Vulkan — a deep dive" (2017).
- glTF 2.0 specification, Appendix B (BRDF); Google Filament material documentation (Apache-2.0; sheen IBL); Brian Karis, "Real Shading in Unreal Engine 4" (SIGGRAPH 2013 course notes; a public talk, no code consulted).
- Materials: Epic Games' public Substrate documentation (Unreal Engine 5 "Substrate Materials" overview: closures per pixel, parameter-blend downgrade) and Hillaire & de Rousiers, "Authoring Materials That Matter: Substrate in Unreal Engine 5" (SIGGRAPH 2023 course). No Unreal source consulted. The BSDF and its papers are listed in `concordia-material`.
- Lagarde & de Rousiers, "Moving Frostbite to PBR" (SIGGRAPH 2014): EV100 exposure.
- Khronos PBR Neutral Tone Mapper specification; AgX after Troy Sobotka, using the minimal analytic fit (Benjamin Wrensch 2023; Godot 4.4, MIT); ACES fitted after Krzysztof Narkowicz (2015).
- Karis 2013 (split sum); Ramamoorthi & Hanrahan, "An Efficient Representation for Irradiance Environment Maps" (SIGGRAPH 2001); Colbert & Křivánek, "GPU-Based Importance Sampling" (GPU Gems 3, ch. 20).
- Zhang et al., "Parallel-Split Shadow Maps" (2006); Michal Valient, "Stable Rendering of Cascaded Shadow Maps" (ShaderX6); Holbert, "Saying goodbye to shadow acne" (GDC 2011 poster).
- Olsson, Billeter & Assarsson, "Clustered Deferred and Forward Shading" (HPG 2012); Emil Persson, "Practical Clustered Shading" (SIGGRAPH 2015).

## Status
M1 phase 1.6 complete: tonemappers, clustered lights, cascaded shadows, IBL, transparency and glTF goldens. Phase 1.16: view modes, `render.stats`, rotate and scale gizmos. M2 phase 2.3b: Substrate-style materials (closure programs on the GPU, the closure budget, material view modes).

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
