# concordia-render

Concordia's renderer: a render graph with automatic barriers and transient aliasing, clustered forward shading of Substrate-style materials, cascaded shadows, image-based lighting, GPU skinning of posed characters, representation LOD (far entities drawn as a cheaper mesh or not at all, chosen per camera), view modes, gizmos, debug layers from engine modules (such as the navmesh), headless capture and `capture.screenshot` (optionally with the posed skeletons or debug layers drawn on top).

See `AGENTS.md` for invariants, tests, performance notes and references.
