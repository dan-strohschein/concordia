# ADR-0013: Advanced lighting milestone (M9): Photon global illumination
- Status: Accepted
- Date: 2026-11-20
- Builds on: ADR-0040 (renderer), ADR-0068 (Substrate-style materials), M4's GI staging (DDGI-style probes first; stage B scaffolded behind `gi::Technique`)

## Context
The author wants fully dynamic global illumination and reflections in the class of Unreal Engine 5's Lumen, under the name **Photon**. M4 plans DDGI-style probes (stage A) and leaves a screen-probe radiance cache and surface cache (stage B) as an open question: an M4.5, or the backlog. Lumen is part of Unreal Engine, which is source-available, not open source, so its code may not be read or copied.

## Decision
1. **A new milestone, M9 "Advanced lighting"** (`plans/M9-advanced-lighting.md`), is added to `docs/design.md`'s milestone list. It is M4's stage B, answering M4's open question 4.
2. **Photon reimplements the publicly described Lumen design:**
   - mesh signed distance fields and a global distance-field clipmap;
   - a surface cache of material cards, lit directly and by radiosity;
   - a tracer chain: screen traces, then distance fields or hardware ray queries;
   - screen probes with a world-space radiance cache;
   - denoised reflections;
   - a translucency and fog volume.
   It implements M4's `gi::Technique`, and M4's DDGI stays as the low tier.
3. **Both tracing paths:** hardware ray queries where `RhiCaps` has them (the reference machine does), and software distance fields elsewhere.
4. **Crates (proposed):** `concordia-sdf` (mesh distance fields, independent of the renderer) and `concordia-photon` (the GI passes).
5. **Clean room:** only Epic's public talks (SIGGRAPH Advances 2015, 2021, 2022) and public documentation, the open research they build on, and permissively licensed code (e.g. kajiya, MIT/Apache) may be used. No Unreal source is read, ported or paraphrased, and contributors who have studied Lumen's source don't write Photon modules. Each module's `AGENTS.md` lists its references.
6. **Quality is measured, not asserted:** gate tests compare against M4's path tracer, and test response time, leaking, energy and per-tier budgets on the reference machine.
7. **ADR block:** M9 draws ADR numbers from 0200–0219.

## Alternatives
- **Fold stage B into M4 (an M4.5).** M4 already carries meshlets, VSM, GI stage A, graphs and AI; Photon alone is about 28 weeks.
- **Leave it in the backlog.** Rejected by the author, who wants Lumen-class GI as a planned milestone.
- **Hardware ray tracing only.** Simpler, but it would leave devices without hardware ray tracing on DDGI permanently. The software path is kept and its tier role is settled in phase 9.0.
- **Surfel GI** (EA SEED, SIGGRAPH 2021) as the scene representation. It is a credible alternative, kept as a reference; the card surface cache was chosen because it matches the target's documented behavior and reuses the material programs.

## Consequences
- M4 must keep the `gi::Technique` seam, the proxy BLAS and the probe volume general enough for Photon to build on (noted in M4's open questions).
- Two more proposed crates join the README crate table at M9's refinement.
