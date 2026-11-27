# ADR-0091: Shadows: virtual shadow maps by default, cascades as the fallback
- Status: Proposed (M4 4.7a, 2026-11-26; the 4.7 plan's Q1–Q18, approved by the author). Accepted when 4.7d completes Phase 4.7's done criterion.
- Date: 2026-11-26
- Builds on: ADR-0014 (M10's tiled shadow atlas), ADR-0040 (render graph), ADR-0088 (Vulkan backend), ADR-0090 (quality methodology), ADR-0100 (meshlets and GPU culling), ADR-0103 (temporal upscaling)

## Context
M1 shadows the sun with three or four stabilized cascades of 2048². Near the camera a cascade texel spans several screen pixels, so thin casters (slats, lattices, bench legs) and contact shadows blur, and M1 has no shadows for lamps. ADR-0014 planned a tiled atlas for local lights in M10, ahead of M4's virtual shadow maps, but M4 arrived first. The reference port and the M4 gate need sharp sun shadows over a large canyon, and many shadowed spell lights later.

## Decision
1. **Technique:** `[render] shadows = "csm" | "vsm"`, `render.set_shadows` for a session. `csm` stays the default during Phase 4.7; the default moves to `vsm` at M4's end, with temporal and GI, when the goldens are re-blessed anyway.
2. **Virtual shadow maps for the sun:** a clipmap of levels (default 16; level 0 is 8 m wide, each level twice the last), each a 16 384² virtual map of 128² pages, centred on the camera and snapped to whole pages, addressed toroidally, so drawn pages keep their place as the camera moves.
3. **Only needed pages exist:** a depth-only camera prepass gives this frame's receivers; each marks the page of its level (chosen so a texel is about half a pixel across, `resolution_bias = −1`) and of a coarse floor level. Pages are allocated on the GPU, coarse levels first, from a deterministic scan; a starved request reads a coarser level, never a wrong one.
4. **The physical pool:** `[render.vsm] pool_pages` (default 4096, 256 MB) pages of `u32` reverse-Z depth in bindless buffers of 1024 pages, written by atomic max from the casters' fragments (order-independent, so deterministic). No sparse or tiled resources, so Metal, Vulkan and MoltenVK behave alike.
5. **Device requirements:** bindless resources, fragment-shader storage atomics (`RhiCaps::fragment_atomics`) and 16 384-pixel attachment-less render passes (`RhiCaps::max_render_extent`). Without them `vsm` runs CSM, logged once, and `render.stats` `shadows` says why.
6. **Local lights (4.7c):** virtual shadow maps also cover spot and point lights where the device runs them. M10's tiled atlas becomes their fallback tier, sharing the light selection, the invalidation model and the stats; ADR-0014 and M10's open question 1 are amended to say so.
7. **Clean room:** built from public papers and talks (adaptive and resolution-matched shadow maps, clipmaps, sparse virtual textures) and Epic's public VSM documentation, read for ideas only; no engine source.

## Consequences
- The frame under `vsm` gains a depth prepass (it also feeds the HZB), page marking, allocation and page rendering; CSM frames are unchanged.
- Sharper shadows: against `sun_courtyard`'s path-traced direct-only references, VSM's FLIP is 0.50× and 0.70× CSM's.
- **Casters are drawn per page** (4.7a2): binned on the GPU to the pages due under them and drawn one page per pair, so the cost follows the casters over needed pages, not the levels' virtual area (the courtyard's page pass fell from 29.7 ms to about 1 ms). Uncached, a dense scene still redraws every caster under its pages each frame (the canyon: 90k–330k meshlets, 11–37 ms); caching (4.7b) is required before VSM can be the default there, and a mesh-shader page path and meshlet LODs are further levers.
- Memory: 256 MB by default for the pool, about 1 MB of page tables.

## Alternatives considered
- **Better cascades** (more, larger, SDSM-fitted): cheaper and simpler, but texel density still falls with distance from the camera's split planes, and nothing serves many local lights.
- **Sparse/tiled textures:** the hardware page table, but not on MoltenVK, partial on lavapipe, and no help with atomic writes.
- **Rendering pages into a depth atlas with one viewport per page:** hardware depth testing, but one draw per (caster, page) pair from the CPU.
- **M10's atlas first, for every light:** simpler for lamps, but leaves the sun on cascades.
