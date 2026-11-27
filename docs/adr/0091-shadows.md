# ADR-0091: Shadows: virtual shadow maps by default, cascades as the fallback
- Status: Proposed (M4 4.7a, 2026-11-26; the 4.7 plan's Q1–Q18, approved by the author; amended by 4.7b, caching, and 4.7c, local lights). Accepted when 4.7d completes Phase 4.7's done criterion.
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

### Amendment (M4 4.7b, 2026-11-26): caching
- **Residency is decided fresh every frame**, coarse levels first, exactly as without a cache; the cache only decides which resident pages are redrawn. Only resident pages are sampled, so a cached frame is bit-identical to an uncached one (pages are drawn relative to their own light-space corner, so a page drawn in any frame holds the same depth).
- **Pages persist** in a page store; unrequested pages age out after `[render.vsm] keep_frames` (60) and are evicted oldest first, ties by physical page, when new pages need room. Slots that wrap as a level scrolls are released.
- **Invalidation**: a ledger compares every caster (sphere, mesh, material, pose) with last frame's and invalidates the static pages under the spheres that changed; a sun turn, a change of levels, pool or depth range, a new view, a scene load or a technique switch clear the cache. Camera cuts keep it.
- **Static/dynamic split**: skinned and `Movable` casters, and `Auto` casters for 60 frames after they move, draw into a per-frame second page (a copy of the static page, `vsm.composite`); static pages never hold them. Without room for a second page they draw into the static page, which is redrawn the next frame. `MeshRenderer.mobility` (`Auto | Static | Movable`, default `Auto`) is a hint that changes only the cost.
- **One cached view at a time** (per-view caching is in `later-backlog.md` §12); `[render.vsm] cache = false` redraws everything every frame with every caster static, the oracle.

### Amendment (M4 4.7c, 2026-11-27): spot and point lights
- `shadows: Option<LocalShadowSettings>` on `PointLight` and `SpotLight` (`None` by default; spell lights stay unshadowed unless they opt in). `[render.vsm] max_local_lights` (64) lights are selected per frame by `shadow_select` (view coverage × candela, 10 % hysteresis, stable slots); the rest shade unshadowed and are counted.
- A spot light is one map (field of view twice its outer angle, up to 120°), a point light six cube faces (90° plus a 2-texel guard band); each map is 8192² texels in 64² pages with mips down to one page, reverse-Z perspective, in the same page table (after the sun's levels, sized to the lights present) and pool. One view index covers sun levels and local mips, so marking, residency, caching, invalidation, the static/dynamic split, binning and drawing are the sun's. Local casters are drawn as whole instances (no per-meshlet culling across hundreds of local views yet). VSM runs for shadowed local lights without a sun.
- Against the path tracer (`spell_arena_direct`): FLIP 0.0027 and 0.0032, a fifth of the unshadowed error; no cube seams (`point_seams`). Cost on `spell_arena_256` about 5.4 ms (above the provisional 2.5 ms; recorded for M4's recalibration).

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
