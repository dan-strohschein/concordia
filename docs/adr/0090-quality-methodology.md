# ADR-0090: Quality methodology: a path-traced oracle, reference and regression goldens
- Status: Accepted (M4 4.1b4, 2026-11-24; the first three references approved by the author on the 4.1b4 review page)
- Date: 2026-11-24
- Builds on: ADR-0089 (perf testing), ADR-0101 (path-tracer backends), M4 refinement D2 and D3

## Context
Golden images catch change, not error: a blessed image of a wrong render keeps the bug forever. M4's lighting work (VSM, GI, TAA) needs a measure of how *right* an image is, and of whether a feature moves the renderer toward the truth.

## Decision
1. **Two kinds of golden.** *Regression* goldens are the renderer's own blessed output, compared tightly (about 0.01 mean FLIP) to catch drift. *Reference* goldens are path-traced renders of the same scene (`tests/golden/m4_reference/`), compared loosely to measure accuracy. Every M4 lighting feature has both. Tolerances live in each suite's `tolerances.ron` (D2).
2. **The oracle** is `concordia-pathtrace`: independent of the renderer's code, sharing only definitions (render types, the BSDF, asset loading). Its own tests (furnace, analytic lights, convergence, GPU against CPU) are what make it trustworthy.
3. **References carry their provenance** in `<scene>.reference.ron`: the inputs (project, scene, view, size, samples, seed, bounces), the backend and git hash, and two numbers: the *baseline* (the renderer's FLIP against the reference when it was blessed) and the *noise* (FLIP between two seeds of the reference). A reference is refused when its noise exceeds a fifth of its tolerance, so the comparison measures the renderer, not the reference's grain.
4. **"Beats the baseline" tests.** A feature meant to improve lighting asserts it: FLIP(with feature) ≤ k × baseline (k set per feature, e.g. 0.6 for GI in the M4 gate). A feature that makes images worse fails even if it passes its regression golden.
5. **Blessing** references is a deliberate act: `just bless-reference <scene>`, never in CI, reviewed by eye on a page that shows the reference, the renderer's image and the FLIP heat map.

## Consequences
- References are re-blessed when shared definitions change (lights, the BSDF, emissive units); the baseline moves with them and is recorded.
- Where the renderer and the oracle differ by design (textures, skinning, representation LOD, two-sided faces), reference scenes avoid those features or the difference is part of the recorded baseline.

## Alternatives considered
- Only regression goldens: measure drift, never accuracy.
- An external renderer as the oracle: different BSDF and light definitions would dominate the error.
