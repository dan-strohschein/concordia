# ADR-0012: Advanced physics milestone (M8): destruction and GPU garments on top of Jolt
- Status: Accepted
- Date: 2026-11-18
- Builds on: ADR-0041 (Jolt physics), ADR-0032 (edit world vs play world), ADR-0046 (engine modules)

## Context
The author wants Chaos-class physics in Concordia, specifically a destruction system like Unreal's Chaos Destruction and a GPU-accelerated garment solver for characters, built on Jolt, which M1 adopted. No milestone planned either.

Jolt v5.7 already provides:
- rigid, compound and convex hull bodies;
- contact impulses;
- deterministic multithreaded stepping;
- CPU soft bodies with skinning constraints (v5.0);
- a GPU compute interface and work-in-progress strand hair (v5.6).

It has no fracture or connection-graph system.

## Decision
1. **A new milestone, M8 "Advanced physics"** (`plans/M8-advanced-physics.md`), is added to `docs/design.md`'s milestone list.
2. **Destruction is built on Jolt rigid bodies.**
   - Offline fracture: `concordia-fracture`, deterministic from a seed.
   - Clusters and a connection graph with strain thresholds.
   - Intact clusters are single Jolt compound bodies, split into connected components when edges break.
   - Fields, a debris budget and caches.
   - It runs on the CPU, deterministically, in the play world only.
3. **The garment solver is our own, on `concordia-rhi`** (Slang compute, XPBD with small steps; VBD evaluated in a spike), rather than Jolt's compute interface.
   - Its output feeds the skinned-mesh render path without leaving the GPU.
   - A CPU reference solver shares the math; Jolt's CPU soft body is the test oracle.
   - GPU cloth is cosmetic (not gameplay-authoritative) by default.
4. **Clean room:** Chaos is source-available, not open source. Only Epic's public documentation may be read, and only for feature expectations; algorithms come from published papers and permissively licensed libraries, recorded in each crate's `AGENTS.md`.
5. **Scheduling:**
   - Destruction phases need only M1, so they can start after M1.
   - Garment phases need M2's skeletal animation and skinning.
   - The exact slot in the order is an open question in the plan, for the author.

## Consequences
- Three new crates: `concordia-fracture`, `concordia-destruction`, `concordia-cloth`. Two new text formats: `.breakable.ron` and `.garment.ron`. M8 takes ADR block 0180–0199.
- Jolt stays the single rigid-body engine, so destruction pieces interact with everything else for free.
- Owning the GPU cloth solver costs more than wrapping Jolt's, but it avoids two GPU abstractions and copies between them. Jolt's GPU hair stays a backlog item to wrap once it's stable.
