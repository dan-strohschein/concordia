# concordia-material: agent guide

## Purpose
Evaluates Substrate-style materials (ADR-0068): compiles a `concordia_assets::Material` into a closure program, fits it to a closure budget, and is the CPU reference BSDF the renderer's shaders must match. The model and `.material.ron` format live in `concordia-assets`.

## Public entry points
- `eval_slab(&Slab, n, v, l)` (BSDF × N·L) and `slab_transmittance(&Slab, cos)`; `thin_film_reflectance(f0, cos, &ThinFilm)`; `SSS_SCALE`, `WAVELENGTHS_NM`.
- `MaterialProgram::compile(&Material)` → `closures()`, `slabs()`, `ops()` (`Op`; `Horizontal` carries its `mix_texture` for the GPU), `blends()`, `eval(n, v, l)` → `Shaded { color, emissive, alpha }`, and `unlit()` (each slab's albedo and emission combined at normal incidence: unlit materials and the albedo view).
- `ClosureBudget::new(1..=8)`, `fit_budget(&Material, budget)` → `(Material, ClosureReport { budget, before, after, merges: [ClosureMerge { node, kind, slabs, dropped }] })`.
- `rows` (M4 4.1b3, moved from `concordia-render`): `material_rows(material, program, texture)` encodes a program as `uint4` rows (header, ops, eight rows per closure; `NO_TEXTURE`, `FLAG_*`, `OP_*`, `FEATURE_*`, `CLOSURE_ROWS`, `MAX_OPS`, `features`, `material_textures`), read by the renderer and the GPU path tracer.
- `shader_dir()` → `shaders/material.slang` (M4 4.1b1): the GPU mirror of this crate. It holds `Closure`, the slab BSDF (`eval_slab`, `eval_plain`, `fresnel`, `thin_film`, `specular_energy`, `transmittance`, sheen, wrap) and the program combinators (`Entry`, `nothing`, `layer`, `mix_entries`, `cover`); since 4.1b3 also `closure_factors(rows, features)` (a slab's factors from its rows) and `finish_closure(c, tables, n, v, layered)` (F0, alphas, the energy split, sheen, transmittance), which the renderer calls around its texture samples. Albedo tables come in through `interface IAlbedoTables` (`direct_albedo`, `sheen_albedo`), which each caller implements. `forward.slang` imports it, and so will the GPU path tracer.
- `dfg`: `integrate_brdf` / `brdf_lut(_cached)` (split-sum IBL table, moved from `concordia-render`), `direct_albedo` / `integrate_direct` (the direct lobe's albedo), `sheen_albedo` / `integrate_sheen` / `sheen_lut(_cached)`, `specular_albedo`. `sampling`: `hammersley`, `importance_ggx`.
- `MaterialProgram::textured(texel)` applies sampled texels to the factors exactly as the renderer's `load_closure` and Slang `apply_texels` do (base × rgb, alpha × a, emission × rgb, metallic × b, roughness × g, mix × r); `sampled_textures()` and `samples_textures()` list what it applies (M4 4.1b4).

## Invariants
- **Energy:** under uniform white light no slab or stack reflects more than it receives (furnace tests, ±2% integration error). Diffuse is scaled by 1 − E_spec(N·V) from the direct-lobe table (not M1's 1 − F(V·H), which gained up to 11% at grazing views); sheen is divided by max(1, E_sheen) and scales what is under it by 1 − max(color)·E_sheen; a layer passes (1 − E_spec(μ)) × transmittance^(1/μ) × (1 − sheen) of the light.
- **Programs:** postfix over a stack; one closure per slab *use* (a node used twice counts twice). Slab alpha is coverage only for `Blend` materials; otherwise it only feeds the mask test. Results are premultiplied when blending.
- **Budget:** merges are parameter blends, deterministic: the first mergeable horizontal mix in post-order, else the first vertical layer. A layer merge tints the bottom by the top's transmittance squared, blends toward the top by its opaque share, and keeps the top's highlight as haze. Dropped textures and features are reported as `node.field`; merged-away nodes are removed.
- Functions take any vector lengths and return zero (never panic or NaN) for zero or non-finite directions; table lookups clamp their inputs.
- The CPU reference uses factor values only; textures multiply them on the GPU exactly as in M1 (a mix texture's R multiplies `mix`).
- `shaders/material.slang` mirrors `eval_slab`, `slab_transmittance`, `thin_film_reflectance` and the program combinators line for line, and `concordia-render`'s `forward.slang` mirrors `unlit` and the interpreter (it binds textures, rows and the environment); change the Rust and the Slang together. `material.slang` never names renderer types (`tests/shader_module.rs` compiles it on its own for Metal and SPIR-V). Its `shader_matches_cpu_reference_per_feature` compares them to within 1.2 sRGB steps.

## How to test
`cargo nextest run -p concordia-material`. Reference values in `tests/bsdf.rs` and `tests/dfg.rs` come from an independent Python implementation of ADR-0068's formulas (converged midpoint grids), not from this crate.

## Common tasks
- **Add a slab feature:** add it to `Slab` in `concordia-assets` (reflected, ranged, validated in `check_ranges`), evaluate it in `bsdf.rs`, include it in `specular_energy` / `slab_transmittance` if it reflects light, blend it in `budget::blend`, then add a reference-value test, an extremes case in `tests/energy.rs` and a mutation check.
- **Add an operator:** a `MaterialNode` variant, its `children`, an `Op`, an `Entry` combinator, and a merge rule in `budget.rs`.

## Don'ts
- Don't depend on `concordia-render`: M4's path tracer uses this crate as its BSDF, on the CPU (Rust) and the GPU (`material.slang`).
- Don't compute expected test values with this crate's own functions.
- Don't use the split-sum IBL table for energy splits: its Smith term differs from the direct lobe's.

## References
- Epic Games, public Substrate documentation (Unreal Engine 5 "Substrate Materials" overview) and Hillaire & de Rousiers, "Authoring Materials That Matter: Substrate in Unreal Engine 5" (SIGGRAPH 2023 course): slabs, operators, closure budget, parameter-blend fallback. No Unreal source consulted.
- glTF 2.0 specification, Appendix B; Google Filament material documentation (Apache-2.0): GGX, height-correlated Smith, Schlick, sheen albedo scaling.
- Karis, "Real Shading in Unreal Engine 4" (SIGGRAPH 2013 course notes): split sum.
- Estevez & Kulla, "Production Friendly Microfacet Sheen BRDF" (SIGGRAPH 2017): Charlie sheen and the Λ visibility fit.
- Kulla & Conty, "Revisiting Physically Based Shading at Imageworks" (SIGGRAPH 2017 course): albedo scaling.
- Green, "Real-Time Approximations to Subsurface Scattering" (GPU Gems, ch. 16): wrapped diffuse.
- Born & Wolf, "Principles of Optics" (Airy thin-film summation); Belcour & Barla, "A Practical Extension to Microfacet Theory for the Modeling of Varying Iridescence" (SIGGRAPH 2017).

## Status
M2 phase 2.3a: model evaluation, budget and CPU reference. Phase 2.3b: `unlit`, mix textures in programs; the renderer evaluates the same programs on the GPU.

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
