# concordia-render

Concordia's renderer: a render graph with automatic barriers and transient aliasing, clustered forward shading of Substrate-style materials, cascaded shadows, image-based lighting, GPU skinning of posed characters, representation LOD (far entities drawn as a cheaper mesh or not at all, chosen per camera), view modes, gizmos, debug layers from engine modules (such as the navmesh), headless capture and `capture.screenshot` (optionally with the posed skeletons or debug layers drawn on top).

Static meshes with meshlets (M4 4.3a, ADR-0100) are drawn through a compute pass and indirect draws into the same forward pass, shading with the same materials; `[render] meshlets = false` turns it off.

See `AGENTS.md` for invariants, tests, performance notes and references.
