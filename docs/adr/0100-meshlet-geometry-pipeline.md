# ADR-0100: Meshlet geometry pipeline with GPU culling, drawn into Forward+
- Status: Proposed (author's direction, 2026-11-19)
- Date: 2026-11-19
- Builds on: ADR-0040 (RHI design), ADR-0045 (Slang toolchain); replaces M4's planned virtualized-geometry architecture (cluster DAG, visibility buffer, software raster, page streaming)

## Context
M4's plan called for Nanite-style virtualized geometry: an offline cluster LOD DAG, page streaming, a 64-bit visibility buffer, a software rasterizer for small triangles, and a material resolve with per-material bins. That is a second renderer next to M1's clustered Forward+. It also depends on 64-bit atomics, which Apple documents only from Apple9, so every older Mac needs two more raster fallbacks.

The reference machine and first backend are Metal (the author's M3 Pro). The goal is GPU-driven rendering of dense scenes that fits the renderer M1 already has, and that can grow LOD later.

## Decision
1. **Offline meshlets.** The asset pipeline splits each static mesh into meshlets of 64 or 128 triangles with meshoptimizer (MIT, vendored through `meshopt-sys`). Each meshlet gets bounds (a sphere) and a normal cone for backface culling. The build is CPU-only, deterministic, and cached in `.concordia/ddc`.
2. **Bindless geometry.** Meshlet vertices, indices and headers live in large bindless GPU buffers, plus an instance table (transform, previous transform, mesh, flags).
3. **Per-meshlet culling on the GPU:** frustum, backface (normal cone) and occlusion against a hierarchical Z-buffer (HZB).
   - **Mesh shaders first:** Metal 3 object and mesh shaders on the reference machine and first backend, then `VK_EXT_mesh_shader`, then D3D12 mesh shaders. The object stage culls and amplifies; the mesh stage emits the surviving meshlets.
   - **Compute fallback** wherever mesh shaders are missing: a compute pass culls and writes indirect draw arguments for the surviving meshlets. The path is chosen from `RhiCaps`, and cap masking tests both on one machine.
4. **The HZB comes from depth, with a two-pass re-test.**
   - Pass 1 draws the meshlets that were visible last frame, tested against last frame's HZB.
   - The HZB is rebuilt from pass 1's depth.
   - Pass 2 re-tests everything pass 1 culled against the new HZB, and draws what became visible.
   - The final HZB is kept for the next frame and for virtual shadow maps.
   - This gives no false negatives after camera cuts or behind moving occluders, and needs no separate depth prepass.
5. **The hardware rasterizer draws the survivors into the existing clustered Forward+ pass**, or into a G-buffer if M4 adds one. There is no software rasterizer, no visibility buffer and no material binning. Materials and lighting stay M1's.
6. **LOD starts with culling only.** Discrete per-meshlet LODs and HLODs come later, as their own phase. Virtualized geometry streaming from disk is out of scope at first, so there is no page format.
7. **Clean room.** Nanite is used for comparison only, as concepts from Epic's public talks. meshoptimizer is a dependency under its MIT license. Mesh-shader details come from Apple's, Khronos's and Microsoft's public API docs.

## Consequences
- `concordia-cluster` becomes meshlet building and per-meshlet bounds for GPU culling. It no longer builds a DAG or pages.
- `concordia-render` gains a `meshlet` module (scene buffers, culling, HZB, both draw paths). M1's Forward+ shading is reused unchanged.
- 64-bit atomics are no longer needed for geometry, so the Apple7/Apple8 software-raster fallbacks go away.
- Triangle cost scales with what survives culling, not with screen pixels, until LODs arrive. The gate's triangle count and budgets are set for that and are recalibrated when LODs land.
- Skinned meshes can use the same meshlet path later, because nothing depends on a static DAG.
- Virtual shadow maps reuse the per-view culling passes and the HZB.

## Alternatives considered
- **The Nanite-style plan it replaces** (cluster LOD DAG, page streaming, visibility buffer, software rasterizer, material resolve). It scales best to film-quality assets, but it is a second renderer with its own shading path. It needs 64-bit atomics that older Apple GPUs lack, and it carries the most clean-room risk. It can still come later, built on top of meshlets.
- **Compute-only culling everywhere** (no mesh shaders). One path is simpler, but it gives up amplification and per-meshlet output on the reference machine's best path. It is kept as the fallback instead.
- **CPU per-object culling only.** Too coarse for dense scenes, and it leaves the GPU idle.
