# concordia-terra: agent guide

## Purpose
**Terra**, Concordia's entity system (ADR-0010): our own archetype ECS (ADR-0030): entities keyed by stable `EntityId`, typed and by-name component access, typed queries (sequential and parallel), resources, sequential schedules, a deterministic fixed-step `SimLoop` and layout-independent world hashing. Since M2 (ADR-0067) it grows Mass-style features: processors with declared access in processing phases, run in deterministic parallel waves (2.0, ADR-0063), then command buffers, signals, templates and LOD (2.0b onward). M7 keeps the large scale.

## Public entry points
- `World` (`spawn`, `spawn_with_id`, `despawn`, `insert`, `remove`, `get`, `get_mut`, resources, `state_hash`, `Clone` for play worlds)
- `World::query` / `query_filtered` → `QueryState` (`for_each`, `par_for_each`) with `With` / `Without`
- `World::{get_dyn, get_json, set_json, insert_dyn, remove_dyn}` and `DynamicQuery` (by stable name, through the `TypeRegistry`)
- `register_component::<T>(&mut TypeRegistry)` and `ReflectComponent`
- `processor(name, phase)` → `ProcessorDecl` (`reads`, `writes`, `writes_presentation`, `after`, `before`, `run`), the `Processor` trait, `View` (`query` → `ViewQuery::for_each` / `par_for_each`), `Phase` (`PrePhysics`, `Physics`, `PostPhysics`, `FrameEnd`, `Presentation`)
- `Schedule` (`add` = exclusive system in `PostPhysics`, `add_in(phase, ..)`, `add_processor`, `append`, `plan`), `system(name, closure)`, `SystemCtx` (`dt`, `dt_exact`), `SimLoop::new(rate, schedule, &mut world) -> Result<_, ScheduleError>` (`step`, `advance`, `rate`, `set_rate`), `SimClock` (`time_scale`, `time`), `TimeScale`, `Timer`, `World::register_presentation`, `WorldHash` (hex `Display`/`FromStr`, serde)
- `component_from_json(&TypeRegistry, name, json)` (validate a component value without a world)

## Invariants
- No `unsafe` in this crate. Columns are `Vec<T>` behind the `Column` trait; keep it that way unless an ADR says otherwise.
- Every column of an archetype has exactly as many rows as its id list; each entity's recorded location matches its row (`World::validate_storage`).
- `WorldHash` is independent of archetype layout, insertion history and thread count: entities by sorted id, components by sorted stable name, then `SimClock` and `SimRng`.
- Phases run in order. Within a phase the order is explicit constraints, then registration order; processors that conflict (one writes what the other touches) never share a wave, so parallel runs equal sequential ones. Exclusive systems run alone. Parallel queries chunk by a caller-chosen constant, never by thread count.
- A processor's `View` holds only its declared columns (disjoint borrows, no `unsafe`); undeclared or read-only-as-mutable queries are `QueryError`s.
- Presentation-only components are never hashed; `Presentation` processors write only them, and other phases never touch them (checked when a `SimLoop` is built).
- `dt` is the fixed step times `SimClock::time_scale`; `SimClock::time` sums the scaled steps; both are hashed. Edit worlds have no clock, so their hashes (recorded in command journals) never depend on it.
- Simulation reads time only from `SimClock`/`SystemCtx` and randomness only from `SimRng`.
- A query mentions each component at most once (`QueryError::Conflict`).
- Archetype moves never drop component values; despawn and remove drop or return them exactly once.

## How to test
`cargo nextest run -p concordia-terra` (debug), `just perf-smoke` (release perf guard), `just bench -p concordia-terra` (criterion; update `benches/BASELINE.md` when numbers move meaningfully).

## Common tasks
- **Add a component:** derive `Clone, PartialEq, Serialize, Deserialize, Reflect` with doc comments; call `register_component::<T>(&mut registry)` so the API and scenes can use it by name.
- **Write a processor (preferred):** `schedule.add_processor(processor("name", Phase::PostPhysics).writes::<A>().reads::<B>().run(|view, ctx| view.query::<(&mut A, &B)>()?.par_for_each(ctx.jobs, CHUNK, ..)))`; declare exactly what it touches.
- **Write an exclusive system** (whole world, runs alone; for structural changes until command buffers land in 2.0b): `schedule.add_in(Phase::PostPhysics, system("name", |world: &mut World, ctx: &SystemCtx| { ... }))`; use `world.resource_scope` to borrow a resource (e.g. `SimRng`) while querying.
- **Add a determinism test:** run the same schedule with `JobSystem`s of 1 and 8 threads and compare `state_hash()` at several ticks (see `tests/determinism.rs`).

## Don'ts
- Don't mutate an edit world directly from engine services; edits go through `concordia-command`.
- Don't derive chunk sizes from `jobs.threads()`.
- Don't read wall-clock time or OS randomness in systems.
- Don't iterate `HashMap`s where order affects results; use the sorted id list.

## References
- Sander Mertens, "Building an ECS" article series (flecs author).
- Michele Caini, "ECS back and forth" (EnTT author).
- Bevy ECS documentation and RFCs (design concepts only; no code copied).
- Unreal Engine's public MassEntity documentation (processors, processing phases, fragments and execution order), concepts only (clean room, ADR-0067); no Unreal source.
- Timothy Ford, "Overwatch Gameplay Architecture and Netcode", GDC 2017.
- Richard Fabian, *Data-Oriented Design*.
- Glenn Fiedler, "Fix Your Timestep!" and "Floating Point Determinism" (Gaffer on Games).

## Status
M0 phase 0.3 complete; renamed to Terra in M7 Phase 7.0 (2026-11-18).

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
