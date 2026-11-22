# concordia-input: agent guide

## Purpose
Turns device events into per-tick action states deterministically, and serves `input.inject` so agents and tests drive the play world exactly as a player would.

## Public entry points
- `InputMap` (`parse`, `to_ron`, `validate`, `starter`): actions, `ActionKind` (`Button`, `Axis1d`, `Axis2d`, `Ray`), `Binding`, from `.input.ron`
- `InputSampler` (`push(tick, event)`, `sample(tick)`, `check`) and `tick_for_time`
- `InputEvent`, `ActionValue`, `ActionState`, `ActionStates` (play-world resource), `InputFrame`
- `KeyCode`, `MouseButton`, `Modifier`
- `pointer_ray(cursor, viewport, inv_view_proj)`
- `InputModule` (EngineModule) and the `input.inject` method types
- Replays (2.8, ADR-0072): `ActionStates::changed_since(earlier)` (the actions whose state differs) and `InputSampler::play_back(changes, until)` (recorded states replace sampling for ticks before `until`)

## Invariants
- Input acts only on the play world and is never journaled (ADR-0032); each injection is logged as `input injected` with the actor.
- Every event belongs to a tick: OS events at time t in (N, N+1] tick times apply at tick N+1; injected events at their `tick` or the next one. Events within a tick apply in push order. A press and release inside one tick still sets `just_pressed`.
- `input.inject` validates every event before queuing any (all or nothing).
- During playback each tick's states are the previous tick's plus that tick's recorded changes; device events queued for playback ticks are dropped. `play_back` checks every action name and value kind first.
- Action names match `[a-z][a-z0-9_]*` (≤ 64); bindings must fit the action kind. Action ids are sorted-name indices.
- Pointer rays assume reverse-Z, Y-up NDC (Metal); they work for orthographic and perspective cameras.
- Rays as input (2.5b.2a, ADR-0078): `InputEvent::PointerRay` and `Binding::PointerRay` (only on `Ray` actions); directions are normalized when they arrive, and non-finite or zero rays are refused (`InputError::Pointer`), as events and as action values. A ray action is idle until its first ray, then pressed. The simulation never sees pixels for clicks: the lens is `concordia-render`'s `camera_ray`.
- `InputMap::starter()` is the map `concordia new` writes: `move` (left click) and `pointer_ray`, `ClickToMove`'s defaults.

## How to test
`cargo nextest run -p concordia-input`

## Common tasks
- **Add a key:** add it to `KeyCode`, map it in `concordia-platform/src/keys.rs`, and extend the key test.
- **Add a binding type:** add a `Binding` variant, say which kinds it fits in `Binding::fits`, compute it in `sampler.rs`, and test it in `tests/sample.rs`.

## Don'ts
- Don't read wall-clock time in sampling; ticks are the only clock.
- Don't depend on winit here (the platform crate converts its events).

## References
- Input-only replays recorded after binding: GGPO's public documentation; Glenn Fiedler, "Deterministic Lockstep".
- Fixed-step input sampling practice (Glenn Fiedler, "Fix Your Timestep!", gafferongames.com); reverse-Z unprojection (standard projective geometry).

## Status
M1 phase 1.1; M2 phase 2.8 (playback for replays); M2 phase 2.5b.2a (the `Ray` kind, `PointerRay`, the starter map).

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
