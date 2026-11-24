# concordia-pathtrace: agent guide

## Purpose
The reference path tracer (M4 4.1b, ADR-0101): a test oracle that renders a world "the slow, correct way", so the renderer's lighting can be measured against it. A backend-neutral core (scene, BVH, sampling, light definitions) with a CPU backend (4.1b2); the GPU backend (4.1b3) and CPU-only games (M14) build on the same core.

## Public entry points
- `scene_from_world(&World, &mut dyn AssetSource, &View)` → `Scene`: what the renderer would draw (world transforms, `MeshRenderer`s, point/spot/sun along each entity's −Z, the first `Environment`, the camera by `pick_camera`; `View::Look` keeps the chosen camera's exposure, tone mapping and clear color).
- `SceneBuilder` (`mesh`, `material`, `instance`, `point_light`, `spot_light`, `sun`, `environment` / `environment_with_background`, `camera`, `build`) for scenes made in code.
- `render(&Scene, &Settings, &Backend)` → `HdrImage` (exposed radiance, before tone mapping); `Backend::auto(Some((device, compiler)))` is the GPU when the tracer builds there, else `Backend::Cpu`; `render_cpu(&Scene, &Settings { width, height, spp, seed, max_bounces, threads })`.
- `GpuTracer::new(Arc<dyn Device>, &dyn ShaderCompiler)` (needs bindless buffers) and `GpuTracer::render`; `gpu::shader_dir()` holds `pathtrace.slang`.
- `PathTraceModule::new(Option<DeviceFactory>)`: the `capture.reference` Engine API method (registered by the app under `[render]`). It loads the world's assets first (`asset_refs` + `LoadedAssets`, so the world and the asset database aren't borrowed together), builds the GPU tracer on first use (auto falls back to the CPU), caps a call at `MAX_SAMPLES` (1280 × 720 × 64) and returns a PNG tone-mapped by `HdrImage::to_rgba8`. `trace(engine, params)` is the same without the cap (the reference-blessing harness in `concordia-tests` uses it).
- Textures (4.1b4): `SceneBuilder::texture(ref, &TextureData)`, `SceneTexture` (bilinear, repeat, texel centres, rows from the top: the renderer's linear-repeat sampler at mip 0), `Scene::uv` and `Scene::texel`. Shading applies `MaterialProgram::textured` per hit on the CPU and `apply_texels` (`material.slang`) on the GPU: base color with alpha, emission, metallic-roughness and mix textures. Normal and occlusion maps are not traced.
- Core pieces: `Triangle::intersect`, `Bvh` and the generic `bvh::Tree`, `Accel` / `Instance` (two levels), `SampleStream`, `EnvMap`, `Light`, `PathTraceError`.

## Invariants
- **Never depends on `concordia-render`** (`repo_hygiene::gpu_free_crates_keep_their_dependencies`). Shared definitions come from `concordia-render-types` (components, light falloff `distance_window`, `SpotCone`, `candela`, equirect mapping, `pick_camera`), `concordia-material` (the BSDF: `MaterialProgram::eval` is BSDF × N·L) and `concordia-assets` (`AssetSource`, `TextureData::linear_rgb`).
- **Determinism:** every random number is a hash of (seed, pixel, sample, dimension) (`SampleStream`, PCG); pixels are independent and their samples are summed in order, so images are bit-identical at any thread count.
- **The engine's light model, not a physical one where they differ:** point and spot falloff is windowed to zero at `range`; spots are not concentrated by their cone; emission and unlit surfaces are display-referred (radiance = value / exposure), as in the renderer; camera rays that miss show the skybox, or the camera's clear color when it is off.
- **Transport:** unidirectional; next-event estimation for every punctual light and the sun, one sky sample (luminance × solid angle) and one emitting triangle (by area), MIS by the power heuristic against BSDF sampling; BSDF directions from a mixture of cosine (half) and each slab's GGX lobe; Russian roulette from the third bounce; masked materials cut out below their cutoff, blended ones pass with probability 1 − alpha.
- Shadow rays start off the surface and are aimed at the target point itself, with a margin before it (a ray measured from the surface stopped at the emitter's own plane: the disk-light test found it).
- Instancing: rays move into object space (same `t`). World- and object-space tests may disagree only within rounding of a triangle edge (Möller–Trumbore isn't watertight across spaces).
- **The GPU backend** (`shaders/pathtrace.slang`, 4.1b3) is the CPU algorithm step for step: the same PCG streams and dimension order, traversal, sampling, MIS, light definitions, alpha handling and emission. It reads the scene from `float4`/`uint4` row buffers (`gpu.rs` `Packed`; the header's 12 rows are documented in `context`): both BVH levels (2 rows per node; the TLAS after the meshes' trees), 3 rows per triangle and per corner-normal set, 7 rows per instance, `concordia-material`'s rows plus a 6-row table per material (rows offset, lobes, alpha mode, unlit; cutoff and alpha; emission; unlit color; 8 lobe alphas), 4 rows per light, 3 per emitter, the sky and its cumulative weights, and the 32×32 albedo tables (bilinear like `dfg::bilinear`). Materials are walked like `MaterialProgram::eval` (a dynamic loop over the shared `closure_factors` / `finish_closure` / combinators). Samples run in batches of 16 per dispatch, each thread adding to its pixel's running sum; the CPU divides and exposes. Images are deterministic on one device; GPU and CPU agree statistically, not bit for bit (float operations differ).
- Known differences from the renderer, by design: every face is two-sided; meshes are drawn full (no representation LOD) and unskinned (bind pose); textures are not sampled (the CPU BSDF takes factors only, as `concordia-material`'s reference does).

## How to test
`cargo nextest run -p concordia-pathtrace`. The transport tests run on every backend (`tests/common` adds the GPU when a device exists; required with `CONCORDIA_REQUIRE_GPU=1`), plus `gpu_and_cpu_tracers_converge` (the GPU differs from the CPU no more than two CPU seeds do) and `gpu_and_cpu_agree_on_layered_materials` (noise-free, direct light only). Tests run optimized (`[profile.dev.package.concordia-pathtrace] opt-level = 3`). `tests/bvh.rs` and `tests/instances.rs` are proptests against brute force; `tests/transport.rs` has the furnace, the disk light, the punctual lights, convergence and determinism; `tests/world.rs` reads worlds.

## Common tasks
- **Add a light type:** define it in `concordia-render-types` (shared with the renderer), add a `Light` variant and its `arriving` case in `cpu.rs`, read it in `world.rs`, and test it exactly with `max_bounces: 1` like `punctual_lights_follow_the_engines_definitions`.
- **Change sampling or transport:** change `cpu.rs` and `pathtrace.slang` together (same steps, same random-number order); keep `sample_bsdf` and `bsdf_pdf` in agreement. The furnace, convergence and GPU-vs-CPU tests catch a mismatch.

## Don'ts
- Don't share rendering code with `concordia-render` (the oracle must stay independent); share definitions through `concordia-render-types` instead.
- Don't let thread order reach a random number.

## References
- Pharr, Jakob & Humphreys, *Physically Based Rendering*, 4th ed. (2023): path tracing, NEE, light and environment sampling, Russian roulette.
- Veach, *Robust Monte Carlo Methods for Light Transport Simulation* (PhD thesis, 1997): MIS and the power heuristic.
- Möller & Trumbore, "Fast, Minimum Storage Ray-Triangle Intersection" (JGT 1997).
- Wald, "On fast Construction of SAH-based Bounding Volume Hierarchies" (IEEE RT 2007): binned SAH.
- Ize, "Robust BVH Ray Traversal" (JCGT 2013): the conservative slab test.
- Jarzynski & Olano, "Hash Functions for GPU Rendering" (JCGT 2020): PCG hashing for counter-based sampling.
- Kulla & Conty, "Revisiting Physically Based Shading at Imageworks" (SIGGRAPH 2017 course): the energy terms the furnace test checks (through `concordia-material`).
No engine source consulted.

## Status
M4 phase 4.1b2: core and CPU backend. M4 phase 4.1b3: the GPU backend, `Backend`, `render`. M4 phase 4.1b4: `capture.reference`, textures, and the three blessed baselines (M1 gate, Lamplight town and street).

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
