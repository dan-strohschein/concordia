# ADR-0069: Terra storage kinds, command buffers, signals and observers
- Status: Accepted (M2 phase 2.0b, 2026-11-19; author's decisions)
- Date: 2026-11-19
- Builds on: ADR-0030 (safe column storage), ADR-0063 (determinism contract), ADR-0067 (Terra's Mass core in M2)

## Context
Phase 2.0 gave Terra processors in phases, run in deterministic parallel waves. Processors still couldn't change structure (spawn, despawn, add or remove components), react to each other, or read resources. Mass solves these with shared and chunk fragments, tags, deferred commands, signals and observers. Every one of them has to keep ADR-0063's rule: the same inputs give the same world hash at any thread count. Command journals also check edit-world hashes when a scene opens, so those must not change.

## Decision
1. **Storage kinds.** A component type is stored one of three ways. Every kind hashes exactly as a dense component would (by stable name, then canonical value), so the storage choice never changes a world hash.
   - **Dense** (default): one value per entity in a column.
   - **Tag:** any zero-sized type, detected automatically. It is part of the archetype's key and has no column.
   - **Shared:** registered with `World::register_shared::<T>()` before first use. Values are interned by canonical hash. An archetype is keyed by its dense components *and* its shared values, so each value is stored once per archetype. Queries read shared components and never write them (`QueryError::SharedReadOnly`); `insert` of a new value moves the entity. Scenes and the Engine API still see per-entity values.
2. **Chunk components** are logical for now. An archetype table is split into chunks of `CHUNK_ROWS` (1024) rows, and a chunk component holds one optional value per chunk. The value belongs to the chunk: it stays when entities move in or out, and it is dropped when the chunk empties. Chunk values are **not hashed**. They are derived data, such as LOD or culling caches, rebuilt from hashed state, because row layout depends on history and the hash promises layout independence. Processor access to chunk components comes with their first consumer (2.4b's LOD).
3. **Command buffers.** Each processor's view has a `CommandBuffer` (spawn, despawn, insert, remove, signal). `par_for_each_deferred` gives each piece its own buffer, joined in piece order.
   - Buffers apply **at the end of their phase**, in the phase's plan order (the topological order, with ties broken by registration order), never in thread-finish order. Within a buffer, commands keep the order they were recorded in.
   - Spawns get ids when the buffer applies, from a `DeterministicIdGen` seeded by (tick, phase). No new state is hashed for this.
   - A command whose target is gone is skipped and counted in `TerraStats` (an unhashed resource).
   - `Presentation` processors get no buffer (`QueryError::CommandsInPresentation`).
4. **Signals** are typed: any `Reflect` type, named by its stable name, so scripts and the API can raise them by name later.
   - A signal targets one entity and is held in a world-level queue, not on the entity, so raising one never moves an archetype.
   - Signals raised during tick N, or between ticks, are delivered at the start of tick N+1, sorted by entity and then by raise order. Signals for despawned targets are dropped.
   - Receivers declare `receives::<S>()` and read `View::signals`. Exclusive systems use `World::signal` and `World::signals`.
   - Pending signals are hashed in play worlds. A world with none adds nothing to the hash, so quiet worlds, including every edit world, hash as before.
5. **Observers** (`observer::<T>(name, Observe::Add | Remove)`) run after each simulation phase's flush, on the net changes since the last pass, in registration order.
   - An entity that gained `T` fires `Add` once. Replacing a value never fires. Adding and removing within one batch cancels out. Despawning counts as a removal.
   - Changes made directly by exclusive systems count too.
   - Commands from observers flush in up to 8 rounds per phase. Anything later waits for the next phase end.
   - Only observed types are logged, so unobserved types cost nothing.
6. **Resources in views.** Processors declare `reads_resource` and `writes_resource`, and resources join the conflict graph. A wave's declared resources are taken out of the world and lent to its views (shared for readers, exclusive to the one writer), then put back. No `unsafe` is needed. `Presentation` processors may not write resources.
7. **Change tracking** records one change tick per archetype for each write query, instead of a tick per entity. The M1 1.9e version did a `BTreeMap` lookup for every entity a write query visited, which made every write query 40–200× slower. Entering an archetype always touches the entity, so the semantics are unchanged.

## Alternatives
- **Named string signals without payloads (Mass's model).** Payloads would then live in components, which costs a structural change per raise.
- **Per-entity signal inboxes.** Every raise would move an archetype.
- **Real chunked tables now.** This is the most faithful to Mass, but it rewrites every query path and risks the M0 benchmarks. It is planned as a later performance update (below).
- **Hashing chunk values.** That would catch divergence in chunk data, but break the hash's layout independence.
- **Flushing commands at every wave barrier.** Results would then also depend on how processors group into waves, and it costs more flushes.
- **A separate tag storage path.** Treating tags as shared values of a zero-sized type reuses one mechanism.

## Consequences
- **Hashes.** Edit-world hashes are unchanged (pinned in `storage_kinds_never_change_the_world_hash`). A play world's hash changes only while signals are pending, for example contacts raised by the physics step.
- **Physics.** `CollisionEvents` is replaced by the `ContactBegan` and `ContactEnded` signals, raised on both entities.
- **Future performance update:** real chunked tables, with fixed-size memory chunks per archetype as in Mass, replace the logical chunks. Chunk components then sit in each chunk next to its rows, and processors may iterate by chunk. This keeps the API and semantics here, and gets its own ADR and benchmarks.
- Templates (2.0c) build on shared components and tags. LOD (2.4b) builds on chunk components and signals.
