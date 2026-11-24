# M14: CPU-only games
Status: Planned · Depends on: M4 (4.1b2's CPU path-tracing core, 4.1v's Vulkan backend), and every rendering milestone before it (M4, M9–M11) · Detail level: **Outline; needs refinement when it starts** · Estimated effort: to be set at refinement

> Added on 2026-11-24 by the author's decision during M4 4.1b planning: games built with Concordia should eventually run on machines with no GPU. It is deliberately the **last** milestone, so every GPU feature it must cover exists first. The author asked for care with the CPU architecture until then; the rules below are what earlier phases must keep true.

## Goal & context
Today every frame needs a GPU: the renderer records RHI command lists for Metal (and, from 4.1v, Vulkan). M4 4.1b adds a reference path tracer with two backends, GPU by default and CPU as a fallback, so reference renders and their tests already work without a GPU. M14 extends that idea from reference stills to playable games: a game exported from Concordia starts, plays and renders on a CPU-only machine (servers, thin clients, CI, old hardware), at reduced quality and frame rate.

## Scope (to be refined)
- A CPU rendering path for games: forward shading of Substrate materials, shadows, UI and post, at a resolution and frame rate set by measurement.
- Automatic fallback when no GPU device exists (`GpuUnavailable` today), with the same Engine API, captures and goldens (per-backend tolerances in `tests/golden/<suite>/tolerances.ron`).
- A perf scene and budgets for a CPU reference machine (ADR-0089).

## Options to weigh at refinement
1. **Vulkan on a CPU driver (lavapipe, MIT).** Runs the GPU renderer unchanged through 4.1v's backend. Cheapest; Linux-first; frame rate limited; shipping the driver with games needs review.
2. **Our own CPU rasterizer** behind the RHI or the renderer: a tile-based, SIMD, multithreaded rasterizer evaluating `concordia-material` on the CPU. Most control; several weeks.
3. **CPU path tracing at low sample counts with denoising**, growing from 4.1b2's core. Good for stills and slow scenes, not for action games.
4. A mix: rasterize on the CPU, reuse the path tracer's BVH for shadows and GI.

## Rules earlier phases keep (so M14 can grow from them)
- `concordia-pathtrace`'s scene, BVH, sampling and light code stay backend-neutral, behind traits, with the CPU backend a first-class implementation, never a test-only stub (ADR-0101, written in 4.1b).
- `concordia-material` stays the CPU reference for every BSDF feature; GPU shaders mirror it (its `AGENTS.md`).
- Render data types live in `concordia-render-types`, free of GPU dependencies.
- Nothing in the engine outside the RHI and the renderer may assume a GPU exists; a missing device is an error the caller can handle, never a panic.

## Open questions
1. Which platforms must run CPU-only (Linux only, or Windows and macOS too)?
2. The minimum playable target (resolution, frame rate, reference CPU).
3. Whether CPU-only games may drop features (GI, VSM, particles) or must approximate each.
