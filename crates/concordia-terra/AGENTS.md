# concordia-terra: agent guide

## Purpose
**Terra**, Concordia's entity system (ADR-0010): our own archetype ECS (ADR-0030): entities keyed by stable `EntityId`, typed and by-name component access, typed queries (sequential and parallel), resources, sequential schedules, a deterministic fixed-step `SimLoop` and layout-independent world hashing. Since M2 (ADR-0067) it grows Mass-style features: processors with declared access in processing phases, run in deterministic parallel waves (2.0, ADR-0063); storage kinds (shared, chunk, tag), command buffers, signals, observers and resources in views (2.0b, ADR-0069); then templates (2.0c) and simulation LOD (2.4b.1, ADR-0076). M7 keeps the large scale.

## Public entry points
- `World` (`spawn`, `spawn_with_id`, `despawn`, `insert`, `remove`, `get`, `get_mut`, resources, `state_hash`, `Clone` for play worlds)
- `World::query` / `query_filtered` → `QueryState` (`for_each`, `par_for_each`) with `With` / `Without`
- `World::{get_dyn, get_json, set_json, insert_dyn, remove_dyn}` and `DynamicQuery` (by stable name, through the `TypeRegistry`)
- `register_component::<T>(&mut TypeRegistry)` and `ReflectComponent`
- `processor(name, phase)` → `ProcessorDecl` (`reads`, `writes`, `writes_presentation`, `after`, `before`, `run`), the `Processor` trait, `View` (`query` → `ViewQuery::for_each` / `par_for_each`), `Phase` (`PrePhysics`, `Physics`, `PostPhysics`, `FrameEnd`, `Presentation`)
- `Schedule` (`add` = exclusive system in `PostPhysics`, `add_in(phase, ..)`, `add_processor`, `append`, `plan`), `system(name, closure)`, `SystemCtx` (`dt`, `dt_exact`), `SimLoop::new(rate, schedule, &mut world) -> Result<_, ScheduleError>` (`step`, `advance`, `rate`, `set_rate`), `SimClock` (`time_scale`, `time`), `TimeScale`, `Timer`, `World::register_presentation`, `WorldHash` (hex `Display`/`FromStr`, serde)
- `component_from_json(&TypeRegistry, name, json)` (validate a component value without a world); `JsonComponent::of(registry, name)` → `set(world, id, json)` (2.4.4: a copyable codec, so play systems write components by name without the registry)
- Storage kinds (2.0b): `World::register_shared`, `storage_kind`, `storage_stats` (`StorageStats`), `chunk` / `set_chunk`, `CHUNK_ROWS`, `StorageKind` (`Dense`, `Shared`, `Tag`; zero-sized types are tags)
- Commands (2.0b): `View::commands` → `CommandBuffer` (`spawn` → `Pending`, `insert`, `remove`, `despawn`, `signal`, `append`, `apply` → `ApplyReport`), `ViewQuery::par_for_each_deferred`, `TerraStats` (skipped commands)
- Signals and observers (2.0b): `World::signal` / `signals` / `pending_signals` (2.4.4: raised, not yet delivered, sorted by entity), `ProcessorDecl::receives`, `View::signals`, `ViewQuery::for_each_of`; `observer::<T>(name, Observe::Add | Remove)`, `View::observed`
- Resources in views (2.0b): `ProcessorDecl::reads_resource` / `writes_resource`, `View::resource` / `resource_mut`
- Script types and named access (2.6a, ADR-0062): `register_dynamic_component`, `World::insert_dynamic`, `World::signal_dynamic`, `ReflectComponent::dynamic` / `signal`; `ProcessorDecl::reads_named` / `writes_named` / `receives_named`; `View::batch(&NamedQuery)` → `Batch` (ids + `WireColumn`s), `View::write_batch`, `View::signal_batch`, `View::signals_named`; `CommandBuffer::insert_reflect` / `remove_named` / `signal_reflect`; `ReflectComponent::from_wire` (decode one wire row as the component's value: `T` for Rust types, `DynamicValue` for script types; 2.6b); `World::migrate_dynamic` / `migrate_dynamic_all` (move stored script types to new versions, all or nothing; 2.6c, ADR-0061); `World::stores(name)` (2.6d)
- Replay support (2.8, ADR-0072): `World::state_digest` → `StateDigest` (the world hash plus one hash per component name and `@entities`, `@clock`, `@rng`, `@signals`, in one pass); `Schedule::add_tick_end` / `tick_end_names`, `tick_end(name, |world: &World| ..)`, the `TickEnd` trait: read-only hooks `SimLoop::step` runs after each tick, once the clock has advanced
- Simulation LOD (2.4b.1, ADR-0076): `LodViewer` (tag), `LodProfile` (shared: `distances`, `rates`, `hysteresis`; `new`, `validate`, `rate`, `level`), `SimLod` (hashed: `level`, `due`, `elapsed`, `last_time`), `lod_system(position)` (`terra.lod`, `LOD`; an exclusive system, first in `PrePhysics`) with `LodPosition`, `lod_phase`, `is_due`, `lod_stats` → `LodStats`, `register_lod`; `ProcessorDecl::lod` / `is_lod`, `ViewQuery::{for_each_due, par_for_each_due, par_for_each_deferred_due}` → `Due { dt, dt_exact, level }`
- Introspection (2.0c): `World::entity_storage` (`EntityStorage`: archetype, chunk, storage kinds, pending signals), `Schedule::waves` / `observers`, `SimLoop::schedule`, `ReflectComponent::register_shared` (type-erased shared registration for templates)

## Invariants
- No `unsafe` in this crate. Columns are `Vec<T>` behind the `Column` trait; keep it that way unless an ADR says otherwise.
- Every column of an archetype has exactly as many rows as its id list; each entity's recorded location matches its row (`World::validate_storage`).
- `WorldHash` is independent of archetype layout, insertion history, storage kind and thread count: entities by sorted id, components (dense, shared and tag alike) by sorted stable name, then `SimClock`, `SimRng` and pending signals (nothing when none are pending). Chunk components are derived data and never hashed.
- Shared and tag components are read-only in queries; a new value moves the entity to that value's archetype.
- Command buffers apply at the end of their phase in plan order (topological, then registration), never thread order; spawn ids come from a `DeterministicIdGen` seeded by (tick, phase). Commands on dead targets are skipped and counted.
- Signals raised in tick N (or between ticks) are delivered at the start of N+1, by entity then raise order; dead targets are dropped.
- Observers run after each simulation phase's flush on net adds and removals, in registration order, for at most 8 rounds; only observed types are logged.
- Change tracking keeps one bulk tick per archetype for write queries; an entity's change tick is the later of its own and its archetype's.
- Phases run in order. Within a phase the order is explicit constraints, then registration order; processors that conflict (one writes what the other touches) never share a wave, so parallel runs equal sequential ones. Exclusive systems run alone. Parallel queries chunk by a caller-chosen constant, never by thread count.
- A processor's `View` holds only its declared columns (disjoint borrows, no `unsafe`); undeclared or read-only-as-mutable queries are `QueryError`s.
- Presentation-only components are never hashed; `Presentation` processors write only them, and other phases never touch them (checked when a `SimLoop` is built).
- `state_digest().world == state_hash()` always; parts are hashed alongside, never instead. End-of-tick hooks see `&World` only and run after the clock advances, in registration order (`append` keeps them).
- Simulation LOD (ADR-0076):
  - Levels come only from hashed state: the nearest `LodViewer` by squared world distance (through the `LodPosition` the schedule was given), the entity's `LodProfile`, its previous `SimLod`, the clock and its id. Never from a camera, layout or thread count. No viewer, no position or an invalid profile means level 0.
  - Detail is lost at a boundary (d ≥ dᵢ) and regained only inside it by the hysteresis (d < dᵢ − h); `validate` keeps h below d₁.
  - Due: `(tick + lod_phase(id)) mod rate = 0`, or the level changed this tick (a new `SimLod` starts at level 0, so its first tick is always due). Due entities get `elapsed = end − last_time`, then `last_time = end`, where `end` is the clock's time plus this tick's scaled step.
  - A LOD-aware processor's queries skip rows that aren't due; pieces are still cut every `chunk` rows. Entities without `SimLod` are due every tick with the tick's `dt`. `View::signals` and `ViewQuery::for_each_of` ignore LOD (signals are never cleared unseen).
- `dt` is the fixed step times `SimClock::time_scale`; `SimClock::time` sums the scaled steps; both are hashed. Edit worlds have no clock, so their hashes (recorded in command journals) never depend on it.
- Simulation reads time only from `SimClock`/`SystemCtx` and randomness only from `SimRng`.
- A query mentions each component at most once (`QueryError::Conflict`).
- Components are identified by stable name (unique per world) in processor access, presentation sets, observers and views; `TypeId`s are only typed fast paths. A component declared by name conflicts exactly like the typed declaration of the same component.
- Dynamic (script) components store `DynamicValue`s of one type version per world (another version must be migrated first); types with no data are tags. They hash exactly like the Rust type of the same shape.
- Named batches visit archetypes then rows in the same order as typed queries. `write_batch` is atomic: every entity must be in the view with the component and every row must decode, or nothing is written.
- Migrating a script type converts every stored value, its tag value and its pending and delivered signals, staged across all requested types before anything commits; a failure leaves the world unchanged. A type can't move between dense and tag storage. Migrated archetypes get a new change tick.
- Archetype moves never drop component values; despawn and remove drop or return them exactly once.

## How to test
`cargo nextest run -p concordia-terra` (debug; the crate builds at `opt-level = 1` in dev so the 100k-entity tests stay fast; `tests/lod.rs` holds hand-computed due ticks, levels and elapsed times), `just perf-smoke` (release perf guard), `just bench -p concordia-terra` (criterion; update `benches/BASELINE.md` when numbers move meaningfully).

## Common tasks
- **Add a component:** derive `Clone, PartialEq, Serialize, Deserialize, Reflect` with doc comments; call `register_component::<T>(&mut registry)` so the API and scenes can use it by name.
- **Write a processor (preferred):** `schedule.add_processor(processor("name", Phase::PostPhysics).writes::<A>().reads::<B>().run(|view, ctx| view.query::<(&mut A, &B)>()?.par_for_each(ctx.jobs, CHUNK, ..)))`; declare exactly what it touches.
- **Change structure from a processor:** `view.commands()?.spawn()` / `.insert(target, value)` / `.despawn(id)`; in parallel loops use `par_for_each_deferred`. Changes appear at the end of the phase.
- **Raise and handle a signal:** `commands.signal(entity, MySignal { .. })` (or `world.signal` in a system); receive with `processor(..).receives::<MySignal>()` and `view.signals::<MySignal>()?`, then `query.for_each_of(&targets, ..)` for their components.
- **React to a component appearing or disappearing:** `schedule.add_processor(observer::<T>("name", Observe::Add).run(|view, ctx| for id in view.observed() { .. }))`.
- **Write an exclusive system** (whole world, runs alone): `schedule.add_in(Phase::PostPhysics, system("name", |world: &mut World, ctx: &SystemCtx| { ... }))`; use `world.resource_scope` to borrow a resource (e.g. `SimRng`) while querying.
- **Make a processor cheaper far away:** add `.lod()` to its declaration and use `par_for_each_due` (or `for_each_due`, `par_for_each_deferred_due`), advancing each entity by `due.dt` instead of `ctx.dt`. Entities need a `LodProfile`; the schedule needs `lod_system` first in `PrePhysics` (the core schedule has it, from `concordia_scene::builtin::lod_position`).
- **Add a determinism test:** run the same schedule with `JobSystem`s of 1 and 8 threads and compare `state_hash()` at several ticks (see `tests/determinism.rs`).

## Don'ts
- Don't mutate an edit world directly from engine services; edits go through `concordia-command`.
- Don't derive chunk sizes from `jobs.threads()`.
- Don't let simulation depend on chunk components unless they are rebuilt from hashed state (they aren't hashed).
- Don't use `ctx.dt` for an entity in a LOD-aware processor; use its `Due`. Don't write `SimLod` outside `terra.lod`.
- Don't read wall-clock time or OS randomness in systems.
- Don't iterate `HashMap`s where order affects results; use the sorted id list.

## References
- Sander Mertens, "Building an ECS" article series (flecs author).
- Michele Caini, "ECS back and forth" (EnTT author).
- Bevy ECS documentation and RFCs (design concepts only; no code copied).
- Unreal Engine's public MassEntity documentation (processors, processing phases, fragments and execution order), concepts only (clean room, ADR-0067); no Unreal source.
- Unreal Engine's public Mass documentation on shared fragments, chunk fragments, tags, deferred commands (command buffers flushed at phase ends), observer processors and the Mass signal subsystem (2.0b, ADR-0069); concepts only.
- Unreal Engine's public Mass documentation on processors querying fragments by type through `FMassEntityQuery` (2.6a named access); concepts only.
- Unreal Engine's public Mass documentation on LOD (LOD collectors and calculators from viewer distances with hysteresis, variable tick rates per LOD) and the Mass talks' descriptions of LOD-based ticking (2.4b.1, ADR-0076); concepts only, no Unreal source.
- Guy L. Steele, Doug Lea and Christine H. Flood, "Fast Splittable Pseudorandom Number Generators" (OOPSLA 2014): the SplitMix64 mixer behind `lod_phase`.
- flecs documentation on observers and deferred operations (Sander Mertens, MIT); Bevy documentation on commands and observers; concepts only, no code copied.
- Timothy Ford, "Overwatch Gameplay Architecture and Netcode", GDC 2017.
- Richard Fabian, *Data-Oriented Design*.
- Glenn Fiedler, "Fix Your Timestep!" and "Floating Point Determinism" (Gaffer on Games).

## Status
M0 phase 0.3 complete; renamed to Terra in M7 Phase 7.0 (2026-11-18); M2 phases 2.0 (processors, ADR-0063) and 2.0b (storage kinds, commands, signals, observers, ADR-0069) and 2.6a (script types and named access, ADR-0062) complete; 2.6b added `ReflectComponent::from_wire` for script commands; 2.6c added type-version migration; 2.8 added state digests and end-of-tick hooks (ADR-0072); 2.4.4 added `World::pending_signals` (read-only, for inspection) and `JsonComponent` (a registry-free component codec, which `World::set_json` uses); 2.4b.1 added simulation LOD (ADR-0076).

## Engineering rules
These apply to every change, by humans and agents alike.
1. **TDD on every phase.** Write the tests first, see them fail, implement, then refactor.
2. **SOLID.** Depend on abstractions at crate boundaries; extend through traits rather than by editing callers.
3. **No duplicate functionality.** Before writing any implementation, search for existing functionality (grep, `cargo doc`, crate `AGENTS.md` files). Extend or refactor what exists; never write a parallel version.
4. **Single responsibility at every level:** function, type, module and crate.
5. **Small functions** that do one thing.
6. **Validate parameters.** Every function checks its inputs:
   - Rust: encode invariants in types (newtypes, enums, `NonZero*`); validate at boundaries and return `Result` with a descriptive error; use `debug_assert!` for internal invariants. Public APIs never panic on bad input.
   - TypeScript: strict types, plus runtime validation at API and IPC boundaries.
7. **Short comments.** One-line doc comments on public items; no essays, and no comments that restate the code.
8. **Idiomatic code.**
   - Rust: follow the Rust API Guidelines and keep it clippy-clean; `thiserror` in libraries, `anyhow` only in binaries and tests.
   - Editor UI: idiomatic React and TypeScript, with function components, hooks, strict mode, ESLint and Prettier.
