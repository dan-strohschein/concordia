# ADR-0030: Own archetype ECS with safe column storage
- Status: Accepted
- Date: 2026-11-17

## Context
The engine needs an entity-component store with stable UUID ids, by-name (reflected) access for the Engine API and scenes, bit-identical results across thread counts, and a codebase small enough for agents to read and change safely.

## Decision
- Write our own archetype ECS (`concordia-ecs`) rather than depend on `bevy_ecs`, `hecs` or flecs bindings.
- Entities are addressed by `EntityId` (UUIDv7/v5); a generational `Entity` handle exists for fast internal lookups.
- Storage is **safe Rust**: each column is a `Vec<T>` behind a `Column` trait object. Archetype moves downcast and push; multi-column queries borrow columns with `slice::get_disjoint_mut`. Dynamic dispatch happens once per table, never per entity. The crate contains no `unsafe`.
- Systems run sequentially in declared order; parallelism is inside systems (`QueryState::par_for_each` with caller-chosen chunk sizes).
- `WorldHash` hashes entities by sorted id and components by sorted stable name, plus `SimClock` and `SimRng`, so it is independent of archetype layout, insertion history and thread count.

## Consequences
- Determinism, reflection-first dynamic access and stable ids are designed in rather than bolted on.
- No unsafe storage to audit, so the Miri job planned for M0 is not needed (the plan's Miri risk is retired). If profiling ever justifies raw blob columns, that change needs its own ADR and a Miri job.
- We maintain the ECS ourselves; features such as change detection, observers and parallel system scheduling come later and only when needed.
- Performance baseline (M0): `crates/concordia-ecs/benches/BASELINE.md`.

## Alternatives considered
- `bevy_ecs`: mature and fast, but large, tied to Bevy's scheduler and reflection, keyed by `Entity` rather than stable ids, and harder for agents to reason about.
- `hecs`: small, but no reflection or stable-id story; we would wrap most of it.
- flecs (C) bindings: excellent, but adds C FFI, a second language and its own reflection model.
- Type-erased blob columns with `unsafe` (the original M0 plan): marginally less indirection, but needs `unsafe` invariants, Miri and more review for no measured benefit at M0 scale.
