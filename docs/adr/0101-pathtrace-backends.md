# ADR-0101: The reference path tracer: GPU by default, CPU as the fallback, one core
- Status: Proposed (M4 4.1b2–4.1b4; author decisions 2026-11-24)
- Date: 2026-11-24
- Builds on: ADR-0040 (RHI), ADR-0068 (materials), ADR-0090 (quality methodology)

## Context
M4 needs a trustworthy oracle for lighting. The author wants it on the GPU by default (speed) and on the CPU as well, because games built with Concordia should eventually run on CPU-only machines (M14) and CI may lack a GPU. Two implementations of one algorithm can also check each other.

## Decision
1. `concordia-pathtrace` has a backend-neutral core: the `Scene` (built from a world plus its assets by `scene_from_world`, or by `SceneBuilder`), a binned-SAH BVH per mesh and one over instances, counter-based random numbers, and the light and sky models. Two backends render it: `render_cpu` (Rust, rows in parallel) and `GpuTracer` (Slang compute over the scene packed into `float4`/`uint4` rows). `Backend::auto` uses the GPU when the tracer builds on a device, else the CPU.
2. **Independence:** the crate never depends on `concordia-render` (a hygiene test). It shares *definitions*, not rendering code: `concordia-render-types` (components, light falloff, spot cone, equirect mapping, camera choice, tone mapping), `concordia-material` (the BSDF on the CPU, `material.slang` and the material rows on the GPU) and `concordia-assets` (`AssetSource`, texture decoding).
3. **One algorithm, step for step:** both backends use the same random-number streams and dimension order, traversal, sampling, MIS and light definitions. Every transport test runs on both, and the GPU must agree with the CPU statistically (and, for direct light, to float rounding).
4. **Determinism:** each random number is a hash of (seed, pixel, sample, dimension); the CPU is bit-identical at any thread count, the GPU on one device.
5. **For M14:** the core stays backend-neutral and the CPU backend first-class; GPU assumptions never enter the core.
6. **Textures** (4.1b4, author decision 2026-11-24): materials' base color (with alpha), emission, metallic-roughness and mix textures are sampled as the renderer samples them (bilinear, repeat, mip 0; the CPU's `SceneTexture` and the GPU's texel buffer use one rule) and applied as its `load_closure` applies them (`MaterialProgram::textured`; Slang `apply_texels`, shared with the renderer). Normal and occlusion maps are not traced: the tracer finds occlusion itself, and normal maps wait for a need.

## Consequences
- Every transport change is made twice (`cpu.rs`, `pathtrace.slang`); the shared tests catch drift.
- Mip 0 with many samples per pixel converges to the filtered texture a mipmapped renderer shows, so textures add little to the baseline.
- The tracer matches the engine's definitions where they differ from physics (light windows at `range`, unconcentrated spots), so it measures the renderer's error, not the definitions'.

## Alternatives considered
- GPU only: no oracle without a GPU, no cross-check, nothing for M14.
- CPU only: minutes per reference; no GPU path for M14 to learn from.
- Sharing the renderer's code: the oracle would inherit the bugs it should find.
