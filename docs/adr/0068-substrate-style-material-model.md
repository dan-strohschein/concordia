# ADR-0068: Substrate-style material model
- Status: Accepted (M2 phase 2.3a, 2026-11-20); the renderer (2.3b), migration and editor (2.3c) and FBX (2.3d) follow
- Date: 2026-11-20
- Builds on: ADR-0009 (canonical RON), ADR-0040 (renderer), ADR-0044 (assets), ADR-0067 decision D5 (materials follow Substrate, text first)

## Context
M1 has one metallic-roughness material (glTF 2.0 core). D5 replaces it with a model in the style of Unreal's Substrate: slabs with BSDF features, combined by layering, mixing and coverage operators, under a per-pixel closure budget. It is defined as text first; node-graph authoring waits for M4's graphs. Clean room: built only from Epic's public Substrate documentation and the SIGGRAPH 2023 Substrate course, plus the papers named per lobe, never Unreal source.

## Decision
1. **Format: a flat map of named nodes.** A `.material.ron` holds `nodes` (name → `Slab(..)`, `Vertical(top, bottom)`, `Horizontal(a, b, mix, mix_texture)` or `Coverage(node, coverage)`), a `root`, and `alpha_mode`, `double_sided`, `unlit`. Nodes refer to each other by name. The reflection system has no recursive types, and a named graph is what M4's node graphs will produce. Names are `[A-Za-z0-9_-]+`, references must exist, cycles are errors naming the path, every node must be reachable from the root, and a node may be used twice. The text is canonical through `concordia-scene`'s document writer.
2. **Slab.** It has M1's fields (base color with alpha as opacity, metallic, roughness, normal, occlusion and emission, with their textures), plus `specular` (dielectric F0 = 0.08 × specular; 0.5 is M1's 0.04), `transmittance` (light passing the medium at normal incidence; it matters only under a layer above) and four optional features:
   - **haze:** a second GGX lobe with its own roughness, mixed by weight, sharing the slab's F0;
   - **fuzz:** a Charlie sheen with the Estevez–Kulla Λ visibility;
   - **subsurface:** a per-channel mean free path (metres, ≤ 0.1). Forward-only approximation: energy-conserving wrapped diffuse, w = mfp / (mfp + 1 cm), so light reaches the surface from below its horizon. Screen-space diffusion waits for M4;
   - **thin film:** thickness in nm and film IOR. Airy reflectance of one dielectric film over a base whose IOR comes from F0 per channel, sampled at 630, 532 and 465 nm, replacing the slab's Fresnel.
3. **Energy conservation, exactly.**
   - Diffuse is scaled by 1 − E_spec(N·V), the directional albedo of the slab's own specular lobes from a table of the direct lobe (GGX with height-correlated Smith, the same terms the BSDF uses).
   - Sheen is divided by max(1, E_sheen) and scales the rest by 1 − max(color)·E_sheen. The sheen table goes in the albedo-table texture in 2.3b.
   - M1's coupling (diffuse × (1 − F(V·H))) gains energy at grazing views: up to 1.107 for a white, full-specular dielectric. The author chose exact conservation over pixel parity, so migrated M1 materials change slightly (about 1–2% of their diffuse head-on, more at silhouettes). Goldens that move beyond tolerance are reviewed and re-blessed in 2.3c.
4. **Operators** (what each passes up):
   - **Vertical:** color = top + T_top(v)·T_top(l)·bottom; emission = top + T_top(v)·bottom; alpha = 1 − (1 − a_top)(1 − a_bottom). A slab's transmittance toward a direction at cosine μ is (1 − E_spec(μ)) × transmittance^(1/μ) × (1 − sheen).
   - **Horizontal:** a linear mix of everything.
   - **Coverage:** like mixing with nothing (no light, full transmittance).
   - Slab alpha is coverage only in `Blend` materials (premultiplied results); otherwise it feeds only the mask test, as in M1.
5. **Closures and programs.** `concordia-material` compiles a material into a postfix program: one `Slab` op per closure (per slab use) and one op per operator, evaluated on a small stack. The GPU (2.3b) interprets the same program, so CPU and GPU share one definition.
6. **Closure budget: platform default, lowerable** (author's choice). Each platform has a maximum (1..=8; the default for the M3 Pro is set in 2.3b), and a project may lower it in `concordia.toml` but never raise it. A material over budget is downgraded by parameter blending (Substrate's documented fallback), deterministically:
   - first the first mergeable horizontal mix in post-order, then the first vertical layer;
   - a mix becomes the parameter blend of its two slabs at its mix;
   - a layer tints the bottom by the top's normal-incidence transmittance squared, blends toward the top by the top's opaque share, and keeps the top's highlight as haze (weight from the F0 ratio);
   - textures and features that cannot be kept are reported as `node.field`.
   `fit_budget` returns the fitted material and a `ClosureReport` (budget, before, after, merges); the renderer logs it and reports it in `render.stats` (2.3b).
7. **Migration.** `read_material` accepts M1's flat text and returns one slab named `surface` with the same values (`Material::from_pbr`). The material importer and renderer switch to `Material` in 2.3b, and `MaterialData` is retired in 2.3c.
8. **Crates.** The model and format live in `concordia-assets` (which owns `AssetRef` and `.material.ron`). Evaluation, the budget, the albedo tables and sampling live in the new `concordia-material`, which doesn't depend on the renderer, so M4's path tracer can use it as its BSDF. The split-sum IBL table moved there from `concordia-render` (which re-exports it).

## Alternatives
- **A recursive operator tree in the file.** It is natural to write, but `concordia-reflect` has no `Box`, and agents editing deep nesting make more mistakes than with named nodes.
- **Generating shader code per material.** This is what Substrate does with node graphs, but it means a pipeline per material and a compile on every edit. One interpreted program keeps M1's single pipeline. Code generation can come with M4's graphs.
- **Keeping M1's diffuse coupling** for exact golden parity: rejected by the author in favor of exact energy conservation.
- **A fixed closure budget, or a free project setting:** the author chose platform default plus lowering.
- **Neubelt's sheen visibility:** simpler, but its albedo reaches 1.7 at grazing views for low roughness. Even Λ needs the albedo table to stay ≤ 1.

## Consequences
- Every BSDF change is tested against independent reference values and white-furnace bounds (including a deterministic grid of bright extremes, because random cases rarely reach them), and checked by mutation.
- The renderer must mirror `eval_slab` and the program interpreter. 2.3b adds a test comparing shader output to the CPU reference per feature, besides the goldens.
- Subsurface is an approximation until M4's screen-space work. Thin film samples three wavelengths, so very thick films alias.
