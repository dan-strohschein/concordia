# concordia-render: agent guide

## Purpose
The renderer (ADR-0040): render graph, forward PBR, cameras, headless capture, and `capture.screenshot` (via `RenderModule`).

## Public entry points
- `graph::RenderGraph`: `create_texture` (transient), `import_texture`, and `add_pass` / `add_pass_recorded` with `PassBuilder::uses(texture, Use)` and `side_effect()`. `compile(footprint)` returns a `CompiledGraph` with `pass_names`, `barriers_before`, `memory_plan`, `to_dot` and `execute`.
- Components: `Camera` (`Projection` perspective/orthographic, reverse-Z; `Msaa`; `exposure_ev100`; `Tonemap`; `priority`), `DirectionalLight`, and `PointLight` / `SpotLight` (lumens, range, cone half-angles).
- `tonemap::apply` gives CPU references for AgX (default), Khronos PBR Neutral, ACES fitted and Linear; the shader must match them (tested).
- `cluster::{ClusterGrid, ClusterSettings, ClusterCuller}`: the froxel grid (16×9×24; exponential slices for perspective, linear for ortho), the CPU reference `assign`, and the GPU culler (`cull` reads back). `Renderer::stats()` reports light counts and cluster overflow.
- `Environment` (HDR equirect `map`, `intensity`, `skybox`) and `ibl::{Equirect, sh9_irradiance, eval_sh9, prefilter, integrate_brdf, brdf_lut}`: image-based lighting computed on the CPU (deterministic, tested). The BRDF table and sampling come from `concordia-material` (re-exported, 2.3a). The renderer caches six prefiltered levels and SH9 per map, plus one BRDF table per renderer, built on first use. An `Environment` has no position: entities with one are extracted even without a `Transform` (`extract::placement` / `placements`, shared by full extraction and the scene cache).
- `shadow::{CascadeSettings, fit_cascades, split_distances}`: `DirectionalLight.shadows` enables cascaded shadows. Cascades are fitted on the CPU (practical splits, bounding spheres snapped to the texel grid, light depth covering all casters).
- `RenderWorld::extract(&World)` copies draws, lights and cameras with world transforms, in entity-id order. `camera(Option<EntityId>)` picks the requested camera, or else the highest priority.
- `Renderer::new(Arc<dyn Device>, Box<dyn ShaderCompiler>)` and `render(&RenderWorld, &mut dyn AssetSource, &RenderRequest) -> Image` (sRGB RGBA8).
- `AssetSource` is implemented for `AssetDb` and `MemoryAssets` (tests, procedural content, built-ins).
- `RenderModule::new(DeviceFactory)` provides `capture.screenshot` (result `image: EncodedImage`, which MCP returns as image content) and `render.stats` (the last frame's `FrameStats`, recorded by the render thread).
- `ViewMode` (`RenderRequest::view_mode`, `capture.screenshot` / `viewport.overlays` `view_mode`): lit, unlit, wireframe, normals, albedo, roughness_metallic, overdraw, shadow_cascades, light_clusters.
- Gizmos: `GizmoMode` (move, rotate, scale) on `GizmoOverlay`; `gizmo_handles` / `gizmo_rings`, `gizmo_hit` / `ring_hit`, `gizmo_pick(mode, ..)`; drag math `axis_delta`, `rotate_angle`, `scale_factor`.

## Invariants
- Frame: light clustering (compute) → one depth pass per shadow cascade (persistent maps imported into the graph) → forward pass (HDR `Rgba16Float`, depth `Depth32Float` cleared to 0 with `GreaterEqual`, optional 4× MSAA resolved into HDR) → tonemap (Khronos PBR Neutral or clamp) → sRGB output → readback. All of it is scheduled by the render graph, with transients placed in one heap per frame.
- Shader data read through pointers uses only `float4`/`uint4` rows (identical layout on every backend). Push constants are mirrored with `shader_struct!` and checked by `check_layout`.
- Unlit and emissive output are display-referred; exposure scales only light.
- IBL: diffuse from SH9 irradiance; specular = lerp of two prefiltered equirect levels (roughness × 5) × (F0·A + B) from the split-sum table; both scaled by intensity × exposure and by occlusion. The sky is drawn first in the forward pass (no depth test or write); its rays come from unprojecting each pixel at two depths, which works for ortho too. Equirect u = 0.5 at -Z, increasing toward +X; v = 0 straight up.
- Shadows: reverse-Z depth; filtered PCF (a box of bilinear compares, kernel radius 1 or 2); bias = constant + normal offset (texels) + slope-scaled ((radius + 1) texels × tan θ, capped at 10), so the whole PCF footprint clears the receiver. Stabilized cascades move in whole texels (the shimmer test pans one pixel per frame: 0 changed pixels when stabilized vs ~900 without).
- Punctual lights: intensity = lumens / 4π cd (spots are not concentrated by their cone); inverse-square falloff windowed to zero at `range`. Clusters hold at most `max_per_cluster` lights, in ascending index order; dropped assignments are counted (`RenderStats::cluster_overflow`) and logged, never silent.
- Transparency: `AlphaMode::Blend` draws after the opaque ones, back to front by the view depth of their bounds' center, with premultiplied alpha and no depth write; `Mask` discards below the cutoff.
- Rendering is deterministic on one device (same scene → same bytes); draws are ordered by pipeline, then entity id.
- View modes are one branch in `forward.slang`'s `fs` on `Frame.debug.x` (the `ViewMode` order). Debug data views (unlit, wireframe, normals, albedo, roughness/metallic, overdraw) skip the tonemap curve (`Linear`, clamped) and, except unlit, the sky. Wireframe draws barycentric edges from the corner index (vertices are pulled per triangle corner), so no fill-mode support is needed; it and overdraw use `Pipelines::debug` (no culling, no depth test; overdraw blends additively).
- `render.stats` GPU time comes from the device's completion handler (`Device::submit_timed` for captures, the present callback for viewport frames); it may arrive before the frame is recorded, and `stats::Last` keeps it until then.
- Gizmo rings are unit circles projected and scaled about the entity's screen position to the arrow length (60 px), so an edge-on ring is a line. Rotate angles are measured around the entity on screen (counter-clockwise is positive when the axis faces the eye); scale grows by one per arrow length dragged outward, never below 0.01.
- The device and shaders are created on the first capture, so engines start without a GPU. A missing device is `GpuUnavailable`.

## How to test
`cargo nextest run --workspace -E 'package(concordia-render)'`. The graph, camera and extraction tests are CPU-only. Render and capture tests skip without a GPU unless `CONCORDIA_REQUIRE_GPU=1`. glTF conformance goldens (`tests/gltf.rs`) need `just gltf-assets`. Goldens live in `tests/golden/render/`: inspect a new image by eye, then run `CONCORDIA_BLESS=1` (never in CI).

## Performance
- `cargo bench -p concordia-render --bench many_lights` (M3 Pro, 2026-11-18): 1,024 point lights at 1920×1080 ortho, median **6.5 ms per frame wall clock**. That includes CPU work, a synchronous submit and an 8 MB readback. GPU-only time needs timestamp queries, which the RHI doesn't have yet; the plan's target is < 4 ms GPU.

## Common tasks
- **Inspect a frame:** `compiled.to_dot(&graph)` gives Graphviz text; `memory_plan()` lists transient placements.
- **Add a pass:** declare its uses in `add_pass_recorded`. Record commands only, since the graph derives barriers. Look up bindless indices inside the record function (it may return an `RhiError`).
- **Change shader data:** edit `shaders/*.slang` and `frame.rs` together, keeping whole `float4`/`uint4` rows; push structs must pass the layout test.

## Don'ts
- Don't record barriers by hand inside passes.
- Don't make compilation or draw order depend on hash-map iteration.
- Don't sample with anything but the static samplers (`g_textures.samplers[SAMPLER_*]`).

## References
- Yuriy O'Donnell, "FrameGraph: Extensible Rendering Architecture in Frostbite" (GDC 2017); Hans-Kristian Arntzen, "Render graphs and Vulkan — a deep dive" (2017).
- glTF 2.0 specification, Appendix B (BRDF); Google Filament material documentation (Apache-2.0); Brian Karis, "Real Shading in Unreal Engine 4" (SIGGRAPH 2013 course notes; a public talk, no code consulted).
- Lagarde & de Rousiers, "Moving Frostbite to PBR" (SIGGRAPH 2014): EV100 exposure.
- Khronos PBR Neutral Tone Mapper specification; AgX after Troy Sobotka, using the minimal analytic fit (Benjamin Wrensch 2023; Godot 4.4, MIT); ACES fitted after Krzysztof Narkowicz (2015).
- Karis 2013 (split sum); Ramamoorthi & Hanrahan, "An Efficient Representation for Irradiance Environment Maps" (SIGGRAPH 2001); Colbert & Křivánek, "GPU-Based Importance Sampling" (GPU Gems 3, ch. 20).
- Zhang et al., "Parallel-Split Shadow Maps" (2006); Michal Valient, "Stable Rendering of Cascaded Shadow Maps" (ShaderX6); Holbert, "Saying goodbye to shadow acne" (GDC 2011 poster).
- Olsson, Billeter & Assarsson, "Clustered Deferred and Forward Shading" (HPG 2012); Emil Persson, "Practical Clustered Shading" (SIGGRAPH 2015).

## Status
M1 phase 1.6 complete: tonemappers, clustered lights, cascaded shadows, IBL, transparency and glTF goldens. Phase 1.16: view modes, `render.stats`, rotate and scale gizmos.

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
