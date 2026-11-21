# concordia-material

Substrate-style material evaluation (M2 phase 2.3a, ADR-0068). A material is a set of named slabs (each a specular interface over a diffuse medium, with optional haze, fuzz, subsurface and thin film) combined by vertical layering, horizontal mixing and coverage. This crate compiles one into a postfix closure program, evaluates it on the CPU as the reference BSDF the renderer must match, and fits it to a per-pixel closure budget by parameter blending, reporting every merge. The model and the `.material.ron` format are in `concordia-assets`.

See `AGENTS.md` for invariants, tests and references.
