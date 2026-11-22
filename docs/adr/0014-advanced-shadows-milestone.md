# ADR-0014: Advanced shadows milestone (M10): tiled shadow atlas
- Status: Proposed (author's direction, 2026-11-20)
- Date: 2026-11-20
- Builds on: ADR-0040 (renderer), M1's cascaded sun shadows and clustered punctual lights; relates to M4's virtual shadow maps (Phase 4.7) and ADR-0100 (meshlets)

## Context
M1 shadows only the sun, with stabilized cascades and filtered PCF. Point and spot lights are clustered and shaded but cast no shadows. The reference port is an isometric ARPG with many torches and spell lights, so it needs 20–50 shadowed local lights per frame.

M4 plans virtual shadow maps: hundreds of 128² pages per light that move every frame, page marking from the depth buffer, and GPU-driven culling per page. That depends on M4's GPU culling, and it is a large piece of architecture. The author wants most of its benefit sooner and more simply.

## Decision
1. **A new milestone, M10 "Advanced shadows"** (`plans/M10-advanced-shadows.md`), is added to `docs/design.md`'s milestone list.
2. **A tiled shadow atlas for local lights:**
   - one depth atlas, with one whole tile per light (for example 512² or 1024² for a spot light) and six tiles for a point light's cube faces;
   - tile sizes from the light's importance, packed by a deterministic power-of-two allocator, with hysteresis and eviction;
   - filtering and bias consistent with M1's sun PCF.
3. **Culling per tile, on the CPU:** a BVH over caster bounds, the light's range, the tile's frustum and receiver-aware culling. Culling is per light rather than per page, so a CPU culler handles 20–50 lights.
4. **Static shadow caching:** static and dynamic casters are kept in separate layers. A light that hasn't moved has its cached static depth copied into the active tile (or reused as is), and only moving casters inside its range are drawn. Invalidation rules are explicit and tested against an uncached render.
5. **No meshlet pipeline is required.** It works with M1's CPU-driven draws; an optional MultiDrawIndirect path serves GPU-driven pipelines, including M4's meshlets later.
6. **The sun stays on M1's cascades** in this milestone.
7. **Clean room:** shadow atlases and cached shadow maps come from public talks, papers and documentation, and from permissively licensed code (Godot, MIT). Epic's public VSM documentation is read for comparison only. No Unreal source is read.
8. **ADR block:** M10 draws ADR numbers from 0220–0239.

## Consequences
- The atlas and M4's VSM must share one light selection, one invalidation model, one set of stats and one BVH, not grow parallel versions. Their final relation (alternative, precursor or fallback tier) is open question 1 in the M10 plan; the recommendation is that the atlas ships first and stays the default local-light tier, with VSM kept for the sun and as an opt-in high tier where it measurably wins.
- The RHI gains viewport and scissor commands and a texture region copy (and indirect draws, if the MDI path is built).
- `PointLight` and `SpotLight` gain `shadows`, `MeshRenderer` gains `mobility`, and `render.stats` gains shadow-atlas counters. No new MCP tools are planned.
- A small proposed crate, `concordia-bvh`, joins the README crate table.

## Alternatives considered
- **Wait for M4's VSM.** It leaves local lights unshadowed until meshlets and GPU culling exist, and it is the heavier architecture.
- **One shadow map per light, separate textures.** Simpler, but each light needs its own texture and binding, and memory can't be balanced between lights.
- **Dual-paraboloid or tetrahedral point-light maps.** Fewer faces, but more distortion and harder filtering; the author chose six cube faces.
- **GPU-driven culling only.** It would tie the atlas to an indirect-draw pipeline; the CPU path keeps it usable on every renderer, and the MDI path is optional.
