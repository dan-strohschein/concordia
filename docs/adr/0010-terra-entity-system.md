# ADR-0010: Terra, the entity system
- Status: Accepted
- Date: 2026-11-17

## Context
Unreal's Mass framework (an archetype entity store plus a large-scale simulation layer) is a key reason ambitious crowd, traffic and ecosystem games choose that engine. Concordia's M0 archetype ECS (`concordia-ecs`) already provides the store, with stable ids, reflection-first access and deterministic hashing (ADR-0030). The author wants Concordia's equivalent named **Terra**, as one entity system rather than a separate layer with its own name.

## Decision
- The entity system is called **Terra**. The M0 crate `concordia-ecs` is renamed `concordia-terra` right after the M0 PR merges (M7 Phase 7.0), before M1, while only four crates depend on it.
- Terra grows Mass-style capabilities in milestone **M7** (`plans/M7-terra.md`): processors with access declarations and deterministic parallel scheduling, processing phases, shared/chunk components and tags, deferred command buffers, text entity templates and traits (`.template.ron`), simulation and representation LOD, signals and observers, spawners, representation switching, and crowds/traffic. New crates: `concordia-terra-sim`, `concordia-terra-crowd`.
- Clean room: designed from public documentation and talks about Mass concepts and the ECS literature; no Unreal source.
- Determinism is non-negotiable: simulation LOD depends only on simulation-state inputs, and parallel scheduling never changes results.

## Consequences
One name for one system; M1–M5 plans refer to `concordia-terra` after the rename. The parallel system scheduling and change detection that M0 deferred land in M7 (or earlier, if a milestone needs them, in the same crate). M7 depends on M1, M2 and M4 for rendering, crowds and behavior.

## Alternatives considered
- A separate `concordia-terra` layer on top of an unchanged `concordia-ecs`: cleaner layering, but two names for what users see as one entity system.
- Adopting `bevy_ecs` or flecs for the large-scale layer: rejected for the same reasons as ADR-0030.
