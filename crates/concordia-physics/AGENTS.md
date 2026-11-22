# concordia-physics: agent guide

## Purpose
Safe, deterministic rigid-body physics for Terra worlds over Jolt (via `jolt-sys`): authoring components, a per-play-session `PhysicsRuntime`, raycasts, characters, and the `physics` engine module with `physics.raycast`.

## Public entry points
- Components: `RigidBody` (`BodyKind`), `Collider` (`ColliderShape`, `PhysicsLayer`), `CharacterController`, `CharacterMotion`
- `PhysicsRuntime` (`new`, `snapshot`, `tick` → `TickReport`, `raycast` → `RayHit`, `state_hash`, `save_state`/`restore_state`, `body_count`)
- `PhysicsSettings` (gravity, `max_bodies`, `set_collides`), `LayerMask`
- `ContactBegan` / `ContactEnded { other, sensor }`: Terra signals raised on both entities by the physics step, delivered next tick in entity order (M2 phase 2.0b, ADR-0069)
- `PhysicsModule` (EngineModule) and `physics.raycast` (`RaycastParams` / `RaycastResult`)
- `static_solids(&World)` → `StaticSolid`s (entity, translation, rotation, collider): the static, non-sensor bodies the step accepts, by entity id, using the step's own validation; navigation builds navmeshes from them (M2 2.5a, ADR-0077)

## Invariants
- Determinism: entities are visited in `EntityId` order (`entity_ids_sorted`), bodies are created in that order, events are sorted, and nothing iterates a `HashMap`. Results are identical across runs, thread counts (1/2/8 tested) and scene entity order.
- Physics runs only in the play world and is never journaled (ADR-0032). The Jolt world is play-session state (`start_play`), not a `World` resource.
- Ownership per kind:
  - dynamic bodies own their `Transform` (rotation, translation) and `Velocity` after creation; Jolt writes them back each tick;
  - kinematic bodies follow their `Transform`;
  - static bodies are fixed, and changing their transform, body or collider rebuilds them.
- Bodies are root entities; `Transform.scale` does not scale colliders.
- `Velocity` is the M0 built-in. Dynamic bodies are overwritten after `integrate_velocity`, so nothing is integrated twice.
- Invalid bodies are skipped and reported in `TickReport::rejected` (the module logs each once); public APIs never panic.
- Only `src/jolt.rs` contains `unsafe`; each block has a `// SAFETY:` comment.

## How to test
`cargo nextest run -p concordia-physics`

## Common tasks
- **Add a shape:** add a `ColliderShape` variant, extend the shim (`jolt-sys`) and `body_desc`/`validate` in `runtime.rs`, then add behaviour and determinism tests.
- **Add a query:** add it to the shim, wrap it in `jolt.rs`, expose it on `PhysicsRuntime`, and (if agents need it) add an Engine API method in `module.rs`.

## Don'ts
- Don't store Jolt handles in the ECS.
- Don't read wall-clock time or use thread-dependent ordering.
- Don't loosen the determinism tests; if a platform differs, record per-platform snapshots in ADR-0041.

## References
- Jolt Physics documentation and samples (MIT); Jorrit Rouwé, "Architecting Jolt Physics for Horizon Forbidden West" (GDC 2022); Glenn Fiedler, "Fix Your Timestep!".

## Status
M2 phase 2.5a: `static_solids` for navigation. M1 phase 1.7b.

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
