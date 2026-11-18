# ADR-0046: Engine modules
- Status: Accepted
- Date: 2026-11-18

## Context
M0 wired the engine statically. The method registry was a process-wide list of the M0 methods, `Engine::new` registered only the built-in components, and the play world always ran `headless_schedule()`. M1 adds crates (input, physics, render, assets) that must contribute components, simulation systems and Engine API methods. Some of them also own runtime objects, such as a Jolt physics system or a GPU device. Those objects are not `Clone`, so they cannot be `World` resources, because the play world is a clone of the edit world (ADR-0032). Making `concordia-api` depend on every feature crate would pull Metal and Jolt into the API crate, and M6 would have to undo it.

## Decision
- An `EngineModule` trait in `concordia-api`, with four hooks: `name`, `register_types`, `methods` and `start_play(&mut World) -> PlaySetup { schedule, state }` (it may prepare the new play world, e.g. insert resources). Every hook except `name` has a no-op default.
- The M0 built-ins are the first module (`core`). `EngineConfig::modules` appends further modules in order.
- Module names and method names must be unique. `Engine::new` returns an error naming both modules.
- Each engine has its own `MethodRegistry` (`Engine::methods`, `EngineHandle::methods`). It drives dispatch, `schema.get`, and the MCP tool and resource lists. `concordia_api::methods()` remains the core set.
- `start_play` runs once per new play world. The schedules are appended in module order, so core systems run first. The returned state lives beside the play world and is dropped with it. Methods reach it through `Engine::play_state::<T>()`. A tick-rate change keeps the session (`SimLoop::set_rate`).
- Modules may also own engine-lifetime state (`engine_state`, read through `Engine::state` / `state_mut`), e.g. the asset database behind `asset.import`.
- Modules read worlds through `Engine::world(WorldSel)` and write the play world through `Engine::play_world_mut()`, which creates a paused play world if there is none. Edit-world mutation still goes only through the command log.

## Consequences
New feature crates plug in without editing the API crate's callers, and their methods become MCP tools with generated schemas automatically. M6 Evolve generalizes this trait (versioning, manifests, WASM) rather than replacing it. A composition root that lists the default modules arrives with the first real module (M1 phase 1.1).

## Alternatives considered
- `concordia-api` depends directly on the feature crates: simplest now, but the API crate would grow Metal, Jolt and windowing dependencies, and the static method list would remain.
- Runtime state as `World` resources: impossible without `Clone`, and it would leak runtime handles into world hashing and play-world cloning.
- Waiting for M6's Evolve extension points: M1 cannot wait, and the M1-sized seam is the subset Evolve needs anyway.
