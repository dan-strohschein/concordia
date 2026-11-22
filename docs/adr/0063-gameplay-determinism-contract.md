# ADR-0063: Gameplay determinism contract: phases, processors, presentation state and time
- Status: Accepted (M2 phase 2.0, 2026-11-19)
- Date: 2026-11-19
- Builds on: ADR-0030 (safe column storage), ADR-0032 (edit world vs play world), ADR-0067 (Terra's Mass core in M2)

## Context
M2 builds every gameplay subsystem as Terra processors (ADR-0067). M0's `Schedule` ran boxed systems in declared order, with parallelism only inside a system. It had no notion of simulation versus presentation, and no way to scale time. Unreal's Mass groups processors into processing phases and runs the ones that don't conflict in parallel. Concordia needs that same parallelism without giving up the engine's rule that the same inputs give the same world hash at any thread count.

## Decision
1. **Phases.** Every tick runs `PrePhysics`, `Physics`, `PostPhysics`, `FrameEnd` and `Presentation`, in that order.
   - Input sampling belongs in `PrePhysics`, the physics step in `Physics`, and most game logic in `PostPhysics`.
   - Exclusive systems (a closure with `&mut World`) still exist. `Schedule::add` puts one in `PostPhysics`, and `add_in` names its phase.
2. **Processors declare their access:** the components they read, write or write as presentation-only, plus `before`/`after` order constraints within their phase.
   - Their view holds only the declared columns, so an undeclared read or a write of a read-only component is an error, not a race.
   - The view is built from disjoint borrows of the archetype columns, with no `unsafe` (ADR-0030 stands).
3. **Deterministic waves.** Within a phase, the order is a topological sort of the explicit constraints, with ties broken by registration order. Every pair of processors that conflict (one writes what the other touches) is then ordered that way too.
   - Each processor runs in the first wave after all its predecessors.
   - The processors of a wave run in parallel on the job system. An exclusive system conflicts with everything, so it runs alone.
   - A parallel run therefore gives the same result as running everything one at a time.
   - Bookkeeping such as change ticks is applied at the wave barrier, in order.
4. **Presentation state is not simulation state.**
   - A component is presentation-only if a processor declares it with `writes_presentation`, or if `World::register_presentation` registers it.
   - Presentation-only components are left out of the world hash.
   - A `Presentation`-phase processor may write only presentation-only components.
   - Processors in the other phases may not read or write them, so simulation never depends on what isn't hashed.
   - Both rules are checked when a `SimLoop` is built.
5. **Time.**
   - `SimClock` holds a `time_scale` (finite, at least 0; 0 freezes simulated time for hit-stop) and `time` (simulated seconds, the sum of the scaled steps).
   - Each tick's `dt` is the fixed step times the scale. Ticks still count one by one.
   - `Timer` components count down scaled simulated time and fire on the tick they expire, several times in one tick if a step spans several periods. The built-in processor `terra.timers` advances them first in `PrePhysics`.
6. **Hashing.**
   - The clock's `time_scale` and `time` are hashed, because they change future steps.
   - Only play worlds have a `SimClock`. So edit-world hashes, which the command journal records and checks when a scene opens, are unchanged, and the hash domain stays `v1`.
   - Play-world hashes changed once. The M1 gate's pinned hash was updated deliberately; its golden image still matches.
7. **Errors.** A `SimLoop` refuses an invalid schedule (`ScheduleError`): cycles name their processors, as do unknown order targets, duplicate names, and presentation rule breaks. It also creates the world's `SimClock` at construction.

## Alternatives
- **An `UnsafeCell` world** that hands out column pointers by declared access. It's faster to build, but it reverses ADR-0030 for a gain that borrowing disjoint columns gets without `unsafe`.
- **Running in sequential order,** with parallelism only inside processors (M0's model). It's simpler, but leaves cores idle with many small processors, which is the case Mass is built for.
- **Hashing `time_scale` but not `time`.** Then timers and anything driven by simulated time could diverge without the hash noticing.
- **Bumping the hash domain** for the new clock fields. Every existing project's command journal would then look stale and lose its undo history, for no benefit, since edit worlds have no clock.

## Consequences
- `SimLoop::new` takes the world and returns a `Result`, and `SystemCtx` gains `dt_exact`.
- Engine modules place their systems in phases: input in `PrePhysics`, physics in `Physics`.
- Later phases build on this contract: command buffers and signals at phase boundaries (2.0b), templates (2.0c), scripted processors (2.6), and representation LOD in `Presentation` (2.4b).

## Update (M2 phase 2.8, ADR-0072)
- `SimLoop` runs read-only end-of-tick hooks (`Schedule::add_tick_end`) after the clock advances, so replay checkpoints equal `sim.step`'s hash.
- `World::state_digest` adds per-part hashes (each component, `@entities`, `@clock`, `@rng`, `@signals`) next to the unchanged world hash.
- Excluding presentation-only components (point 4) is all the "restrict the hash to simulation state" the old replay plan asked for.

## Update (M2 phase 2.4b.2, ADR-0076)
- Presentation-ness can belong to the type itself: `#[reflect(presentation)]` sets `TypeInfo::presentation`. Every world then treats the type as point 4 says (left out of the hash, off limits to simulation) from the first time it stores it, with no per-world registration. `Representation` is the first such type. `World::register_presentation` still works for others.
