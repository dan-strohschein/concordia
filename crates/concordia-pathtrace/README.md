# concordia-pathtrace

Concordia's reference path tracer: a test oracle that renders a world the slow, correct way, so the real-time renderer's lighting can be measured against it. It reads the same scenes, light definitions and materials as the renderer (through `concordia-render-types`, `concordia-material` and `concordia-assets`) but shares none of its rendering code. The CPU backend is here; a GPU backend follows on the same backend-neutral core.

See `AGENTS.md` for invariants, tests and references.
