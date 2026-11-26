# ADR-0100: Meshlet geometry pipeline with GPU culling, drawn into Forward+
- Status: Accepted (2026-11-25, M4 4.3a: the compute + indirect path renders on Metal and on lavapipe with its goldens passing); proposed 2026-11-19 by the author's direction
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

## Amendment (2026-11-25, M4 4.2): the meshlet format and its build cache
Approved with 4.2's plan (Q1–Q5).

8. **The build** (`concordia-cluster`, safe Rust over `meshopt-sys`'s checked wrapper of vendored meshoptimizer v1.3):
   - One portable size, **128 triangles and 128 vertices**. It fits Metal (256 vertices, 512 primitives) and every `VK_EXT_mesh_shader` device (at least 256 and 256), and the compute fallback has no limit. `MeshletSize::for_limits` picks 64 only for a device that can't take 128; that build has its own cache key. The build needs no GPU.
   - A meshlet never spans two materials. Materials are built in ascending order; a material over 131,072 triangles is ordered by its triangles' centroid Morton codes and cut into fixed piles, built in parallel and joined in pile order. No step depends on the thread count, so the output is byte-identical for any number of threads.
   - meshoptimizer gives each meshlet a bounding sphere and an 8-bit normal cone whose cutoff (the sine of the normal cone's half-angle) is rounded up, so the sphere-form test `dot(c − eye, axis) ≥ cutoff·|c − eye| + r` never rejects a visible meshlet. For a meshlet whose triangles all have zero area, meshoptimizer returns zeroed bounds; the build replaces them with an enclosing sphere, and the zero cone never rejects.
   - meshoptimizer is compiled with `-ffp-contract=off`. A pinned hash test shows the same bytes on macOS arm64 and Linux x86_64.
9. **`MeshletBlob` v1**, little-endian and GPU-ready, so 4.3 uploads it without decoding:
   - a 48-byte header: magic `CMLT`, format version, max triangles (u16), max vertices (u16), cone weight (f32), the meshlet count, the vertex-reference count, the triangle bytes, source triangles, source vertices, and 12 zero bytes;
   - 32-byte meshlet records (16-byte aligned): center (3 × f32), radius (f32), cone axis (3 × i8) and cutoff (i8), vertex offset (u32), triangle offset (u32), vertex count (u8), triangle count (u8), material (u16);
   - vertex references (u32 into the mesh's vertex streams);
   - triangle runs (3 × u8 local corners per triangle), each meshlet's run 4-byte aligned with zero padding.
   The layout is canonical (consecutive offsets), and decoding validates every count, range, reference, sphere and padding byte, so a decoded blob re-encodes to the same bytes. A layout change bumps the format version.
10. **Storage and cache.**
    - The blob is stored in `MeshData::meshlets`, so the mesh artifact carries it.
    - The build is also cached in the DDC under its own key: blake3 of `"concordia-meshlets"`, the format version, the build version, meshoptimizer's version, the settings and only what the build reads (positions, indices, per-triangle materials). Changing an unrelated import setting or bumping an importer's version reuses the meshlets.
    - The platform is left out of the key because the output is the same on every OS.
    - A cache entry that doesn't decode is rebuilt.
11. **Import.** The model importers' sidecar setting `"meshlets": "auto" | true | false` (default `"auto"`):
    - `auto` builds meshlets for static meshes whose parts together have more than 8,192 triangles;
    - `true` builds them for every static mesh and is an import error on a skinned one;
    - `false` never builds them.
12. **Inspection.** `asset.meshlet_stats` (MCP `asset_meshlet_stats`, for the modeler and tech-artist roles) and `concordia cluster stats` report each mesh's meshlet count, triangles and vertices per meshlet, sphere radii, normal-cone coverage and encoded size, or why the mesh has none.

The vertex and index codecs (compression) are left out of v1; they would bump the format version when DDC size calls for them.

## Amendment (2026-11-25, M4 4.3a): indirect draws, the scene buffers and the compute + indirect path
Approved with 4.3a's plan (Q1–Q10), with the changes recorded in its As built.

13. **RHI (D5).** `DrawIndirect` and `DrawIndexedIndirect` draw one record each, in the layout Metal and Vulkan share; there are no count buffers or multi-draws yet, so the CPU decides how many draws and the GPU how large each is (GPU-decided counts are later work). `SetViewport` / `SetScissor` act inside a pass, which starts with its whole target; `CopyTexture` copies one region between single-sample textures of one format. Both backends run the same checks (`concordia_rhi::check`) and the same conformance cases.
14. **Scene buffers.** The per-frame instance table is shared by both paths; its spare word names the instance's mesh-table row. The mesh table (32 bytes a mesh: the blob and the vertex streams, the meshlet count, triangles per meshlet) is rebuilt each frame. `MeshletBlob` v1 is uploaded unchanged, once per mesh, beside the mesh's streams. The previous transform arrives with motion vectors (4.3b).
15. **The compute + indirect path.** Draws whose meshes have meshlets and are neither skinned nor blended take it (`[render] meshlets`, on by default). They are grouped as the M1 path batches them (pipeline and material), plus one shadow-caster bucket. `meshlet_expand` writes one entry (instance, meshlet) per meshlet in fixed slots, and the thread of each bucket's first entry writes its `DrawIndirect` record, with no atomics; 4.4's culling decides how entries are compacted. Each bucket is one indirect draw of 3 × max triangles vertices per meshlet instance; `meshlet_vs` pulls vertices through the blob and feeds Forward+'s fragment shader unchanged, and `meshlet_shadow_vs` draws the casters into every cascade. Buckets draw in the forward pass after the M1 opaque draws and before blended ones, sharing the depth buffer.
16. **Observability.** `render.stats` reports `raster_path` (`forward`, `compute_indirect`; `mesh_shader` in 4.3b) and `meshlets` (instances, meshlets, triangles, indirect draws); perf mode times `meshlet.expand`. The `meshlets` and `triangles` view modes come through `view_mode`; there is no separate `render.set_debug_view`.

## Amendment (2026-11-25, M4 4.3b): the mesh-shader path
Approved with 4.3b's plan (Q1–Q12), with the changes recorded in its As built.

17. **Slang on Metal.** Slang 2026.22 emits working `[[object]]` and `[[mesh]]` MSL (checked on the M3 Pro), so there is no hand-written MSL shim. Metal requires every mesh output vertex to be written as a whole struct. Per-primitive outputs are not used: Slang can't decorate the fragment input `PerPrimitiveEXT`, which Vulkan's interface matching needs.
18. **RHI.** `MeshPipelineDesc` (optional object stage with its payload size, mesh stage with its declared outputs, optional fragment stage, the graphics pipelines' fixed-function state) is checked against `RhiCaps` and the portable limits Metal and every `VK_EXT_mesh_shader` device accept (128 threads per stage group, 16 KB payloads, 65,535 groups per dimension and 2^22 in all); nothing is clamped. `DrawMeshTasks` is direct only: culling in the object stage still lets the CPU count the object groups, and an indirect variant comes when GPU instance culling needs it. Vulkan's caps come from the probe's `VK_EXT_mesh_shader` record; MoltenVK and masked devices refuse mesh pipelines.
19. **The path.** Chosen per frame: mesh shaders when the device has object and mesh shaders taking 128 vertices and primitives, else the compute + indirect path; one machine tests both by masking `mesh_shaders`. It reuses the work items, buckets and mesh table. The object stage (32 threads) finds each of its entries' work item by the same search as `meshlet_expand` and launches one mesh group per entry (4.4 culls and compacts here); the mesh stage emits the meshlet's ≤ 128 shared vertices and triangles; Forward+'s fragment shader is shared unchanged, and a depth-only mesh stage draws the casters into each cascade. There is no expand pass on this path. The `wireframe` and `triangles` views need per-triangle data that shared vertices can't carry, so they draw meshlet meshes through the compute path.
20. **Observability.** `render.stats` reports `raster_path: mesh_shader` and `meshlets.mesh_draws`. The compute fallback must match the mesh-shader image within a `paths` tolerance in the suite's `tolerances.ron` (mean FLIP 0.002 for `meshlets_vs_forward`).
21. **Motion vectors (4.3b2).** The instance table carries each instance's previous world matrix and previous joint-palette start (224-byte rows); `Frame` carries the previous view-projection. The renderer keeps last frame's transforms per entity, palettes per pose owner and the camera's matrix; anything not drawn last frame, a reset (camera cuts) and a full scene update have no motion. Every path (M1, compute, mesh shaders) and the sky write motion, in UV units with y down, to the forward pass's second target (`Rg16Float`, resolved like the color); blended, debug and grid draws mask it. History is per renderer until 4.6 makes it per view.

## Amendment (2026-11-25, M4 4.4a): GPU frustum and normal-cone culling
Approved with 4.4's plan (Q1–Q12); the HZB and two-pass occlusion follow in 4.4b's amendment.

22. **Instances are culled on the GPU.** Meshlet-path instances are no longer culled on the CPU: every one gets its bucket, and the GPU tests its box (eight corners against the view's planes, as the CPU's frustum test does) before its meshlets. The CPU frustum test stays for the draws the CPU makes (the M1 path).
23. **One cull test for both routes.** `meshlet.slang`'s `cull` decides each candidate (instance, meshlet) for its bucket's view: the instance's box, the meshlet's sphere, then the normal cone in the instance's local space (decision 8's sphere form). The cone test is skipped for double-sided materials, mirrored or non-uniformly scaled instances (beyond 0.1 %), orthographic views and a cutoff of 1. A masked material needs nothing special: a cut-out doesn't change which way a triangle faces. The compute route calls it in `meshlet.cull`; the mesh-shader route in its object stage.
24. **Views.** A per-frame view table holds the camera, then each shadow cascade. Cascades test their planes only, without the near plane (casters between the light and a cascade still cast). The shadow casters are laid out once per cascade, and not at all when the shadow maps are reused (the culling switches are part of the shadow cache key). VSM pages (4.7) and other cameras become more views.
25. **Deterministic compaction, no atomics.** Every candidate writes a code word (the cull code, and the meshlet's triangle count for the stats). The compute route packs survivors in slot order with a GPU exclusive prefix sum (`scan.slang`: per-group scans, the group totals scanned the same way, then the bases added back); the object stage ranks its survivors within its group. Culling on and off therefore draw the same triangles in the same order, and the images match exactly. Draw counts stay CPU-known (one draw per bucket); the GPU decides only each bucket's survivors, so GPU-decided draw counts and indirect mesh-task draws are not needed yet (`later-backlog.md` §12; wanted later).
26. **Observability.** `render.stats` `meshlets` reports the camera's candidates and what survives each stage (instance frustum, meshlet frustum, cone), the meshlets and triangles drawn, and the meshlets drawn into the cascades, read back from the codes (a viewport frame's when it retires). `Renderer::read_culling` returns every candidate's decision, and `CullView` is the CPU reference the tests hold the GPU to. `[render] culling` (on by default) turns it off for comparisons.

## Amendment (2026-11-25, M4 4.4b): two-pass HZB occlusion
Approved with 4.4's plan (Q7, Q8).

27. **The HZB lives in a buffer.** An `f32` mip chain laid out level after level, built every frame by one compute dispatch per level from the forward pass's single-sample depth: the depth itself without MSAA, else a min depth resolve (`RhiCaps::depth_resolve_min`; Metal's `.min` filter, Vulkan's `VK_RESOLVE_MODE_MIN_BIT`). With reverse-Z the conservative value is the farthest, the smallest: each texel is the min of its up to 2 × 2 parents (level 0 is half the depth rounding up). A device without min resolves skips occlusion when it draws with MSAA.
28. **Two passes, pass 1 without an HZB test.** Pass 1 draws the camera's meshlets that were visible last frame and pass the frustum and cone tests; the HZB is built from its depth (M1 opaque draws are occluders too); pass 2 tests every candidate's projected sphere (Mara & McGuire) against it at the level where the rectangle spans at most 2 × 2 texels, draws what pass 1 didn't, and records visibility for next frame. Testing pass 1 against last frame's HZB would need reprojection and can't add correctness, since pass 2 catches everything. Shadows are culled once, before both, without an HZB. The forward pass splits into `forward` and `forward_late` around `meshlet.hzb` (and `meshlet.cull_late` on the compute route) only when this runs; otherwise it stays one pass.
29. **Visibility is a word per meshlet.** Each meshlet-path instance (entity, mesh, occurrence) owns a stable run in a persistent buffer, its base in the instance row; runs of instances not drawn are pruned like the motion history. A word, not a bit, because nothing uses atomics. The first frame, a camera cut, a new entity or a grown buffer mean pass 1 draws little and pass 2 does the work: never lost geometry, so nothing resets it.
30. **Observability.** Cull codes gain `Occluded` and `DrawnLate`; `render.stats` `meshlets` gains `after_occlusion`, `drawn_early` and `drawn_late`; the `occlusion` view tints meshlets by the pass that drew them and the `hzb` view shows the finest level.
