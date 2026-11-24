# concordia-pathtrace

Concordia's reference path tracer: a test oracle that renders a world the slow, correct way, so the real-time renderer's lighting can be measured against it. It reads the same scenes, light definitions and materials as the renderer (through `concordia-render-types`, `concordia-material` and `concordia-assets`) but shares none of its rendering code. It has two backends on one backend-neutral core: the GPU by default and the CPU as a fallback, checked against each other. It traces materials with their color, emission, metallic-roughness and mix textures.

Agents preview it with `capture.reference` (capped); full-quality references in `tests/golden/m4_reference/` are blessed with `just bless-reference <name>` (ADR-0090).

See `AGENTS.md` for invariants, tests and references.
