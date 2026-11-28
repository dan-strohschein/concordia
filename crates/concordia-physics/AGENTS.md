# concordia-physics: agent guide

## Purpose
Safe, deterministic rigid-body physics for Terra worlds over Jolt (via `jolt-sys`): authoring components, a per-play-session `PhysicsRuntime`, raycasts, characters, and the `physics` engine module with `physics.raycast`.

## Public entry points
- Components: `RigidBody` (`BodyKind`), `Collider` (`ColliderShape`, `PhysicsLayer`), `CharacterController`, `CharacterMotion`
- `PhysicsRuntime` (`new`, `snapshot`, `tick` → `TickReport`, `raycast` → `RayHit`, `state_hash`, `save_state`/`restore_state`, `body_count`)
- `PhysicsSettings` (gravity, `max_bodies`, `set_collides`), `LayerMask`
- `ContactBegan` / `ContactEnded { other, sensor }`: Terra signals raised on both entities by the physics step, delivered next tick in entity order (M2 phase 2.0b, ADR-0069); registered by the module so gameplay scripts receive them by name (2.10c.1)
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
- Characters (2.10a, ADR-0041 update): each has Jolt's kinematic inner body (its capsule, its moving layer, its own user id mapped to the entity), so sensors report `ContactBegan`/`ContactEnded` naming the character. The shim rejects every other pair with an inner body (`OnContactValidate`), and raycasts and character moves skip inner bodies, so solids, dynamics, rays and other characters see characters as before.
- `Velocity` is the M0 built-in. Dynamic bodies are overwritten after `integrate_velocity`, so nothing is integrated twice.
- Invalid bodies are skipped and reported in `TickReport::rejected` (the module logs each once); public APIs never panic.
- Only `src/jolt.rs` contains `unsafe`; each block has a `// SAFETY:` comment.

- Raycasts skip sensors (trigger volumes, not surfaces) and characters' inner bodies (M4 4.13a; the shim's query body filter).

- `ColliderShape::ConvexHull` and `::Compound` (M8 8.0a) are Jolt hulls with its default convex radius. Fewer than 4 finite points, or no volume, makes the body rejected (reported, never a panic). `hull_triangles` gives the same hull to anyone who needs its faces (navigation).
- `ContactImpact { other, strength, point }` (M8 8.0b) goes only to entities with `ContactReports { min_strength }`. Its strength is the closing speed along the normal times the pair's reduced mass (N·s), measured by the shim when the contact is added; never for sensors. Other entities' signals, and so every existing hash, are unchanged.
- Bodies follow their entities every tick, in sorted id order: a spawned entity gets a body with its `Velocity`; a despawned one, or one whose collider changed, loses its body. M8's splits rely on this (8.0c's tests pin it).
- Pushing live bodies (M8 8.4b). The runtime reads `Velocity` only when it makes a body, so these exist:
  - `Impulse { linear, angular }` is applied to dynamic bodies at the start of the next step, then removed;
  - `Sleep {}` deactivates the body, then is removed;
  - `PhysicsDisabled {}` keeps the entity out of the Jolt world while present, and the body returns with its `Velocity` when it goes.

  They are handled in entity order after the body sync (`cj_body_add_impulse` and `cj_body_set_active` in the shim).
- `SoftReference` and `SoftSheet` (M8 8.0d): Jolt's soft body in its own single-threaded world, the reference the cloth solver (8.8) is tested against. Not for gameplay.

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
M2 phase 2.10a: characters' inner bodies for sensors; `tests/sensors.rs` (static and kinematic sensors with moving kinematic bodies and characters; no solid contacts) and `raycasts_ignore_a_characters_inner_body`; characters still pass through each other. M2 phase 2.5a: `static_solids` for navigation. M2 phase 2.5b.2a: `PhysicsQueries`, a play resource processors raycast through (bodies as of the last step; nothing on tick 0; not hashed). M1 phase 1.7b.

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
