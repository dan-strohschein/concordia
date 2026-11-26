# concordia-render

Concordia's renderer: a render graph with automatic barriers and transient aliasing, clustered forward shading of Substrate-style materials, cascaded shadows, image-based lighting, GPU skinning of posed characters, representation LOD (far entities drawn as a cheaper mesh or not at all, chosen per camera), view modes, gizmos, debug layers from engine modules (such as the navmesh), headless capture and `capture.screenshot` (optionally with the posed skeletons or debug layers drawn on top).

Static meshes with meshlets (ADR-0100) are drawn into the same forward pass with the same materials: through object and mesh shaders where the device has them (M4 4.3b), else through a compute pass and indirect draws (4.3a); `[render] meshlets = false` turns both off. On both, the GPU culls what can't be seen before drawing (M4 4.4): whole instances and single meshlets outside the camera's view or a shadow cascade's, and meshlets whose triangles all face away; and, in two passes around a depth pyramid built from what was visible last frame, meshlets hidden behind nearer surfaces (4.4b; the `occlusion` and `hzb` views show it). `[render] culling = false` turns that off. Every path writes motion vectors (M4 4.3b2) for temporal anti-aliasing, and the `motion` view shows them.

See `AGENTS.md` for invariants, tests, performance notes and references.
