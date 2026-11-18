# concordia-scene: agent guide

## Purpose
The text source of truth: canonical RON scenes and prefabs (ADR-0009) with stable ids, byte-stable round trips, agent-actionable read errors, built-in components, and conversion between documents and worlds.

## Public entry points
- `read_scene`, `read_prefab`, `write_scene`, `write_prefab`, `format`
- `SceneDoc`, `PrefabDoc`, `EntityDoc`, `SceneMeta`, `FORMAT_VERSION`
- `load_into_world`, `snapshot_world`, `instantiate_prefab` (returns `EditOp`s for `concordia-command`)
- `builtin::{Name, Transform, Parent, Velocity, register, integrate_velocity, headless_schedule}`
- `SceneError`

## Invariants
- Canonical rules (ADR-0009): entities sorted by id; component keys and struct fields sorted; 4-space indent; trailing commas; final newline; floats shortest round-trip for their type with a `.`; `-0.0` kept; non-finite rejected on save; minimal string escaping; `Some(x)`/`None`; enums `V`, `V(x)`, `V(a, b)`, `V(field: x)`; tuples inline when every item fits on one line; a regenerated header comment and no other comments.
- Every canonical file survives read → write byte for byte, and `format` is idempotent.
- Reading never panics; nesting depth is capped at 128.
- Every component value is validated by the registry before it reaches a world; errors carry line, column, JSON Pointer and the expected shape.
- Loading a scene builds a world directly and is not journaled; every later edit goes through `concordia-command`.
- Prefabs instantiate as flattened copies with fresh ids; any string equal to a prefab entity id is remapped (so `Parent` and other id references follow).

## How to test
`cargo nextest run -p concordia-scene` (round trips, canonical snapshots, errors, prefabs, property tests). Review snapshot changes with `cargo insta review`.

## Common tasks
- **Add a fixture:** write it by hand in canonical form under `tests/fixtures/<name>.scene.ron` (components must be registered in `tests/common/mod.rs`); `load_then_save_is_byte_identical_for_canonical_fixtures` picks it up. Non-canonical inputs go in files named `noncanonical*`.
- **Add a built-in component:** define it in `src/builtin.rs` with doc comments, register it in `builtin::register`, and cover it in `tests/fixtures/builtins.scene.ron`.
- **Bump the format version:** change `FORMAT_VERSION`, write an upgrade path from the old version (never silently reinterpret old files), update ADR-0009, the fixtures and the snapshots.

## Don'ts
- Don't hand-format scene text in code; always go through `write_scene`/`write_prefab`.
- Don't add serde attributes to built-ins that change their shape (the reflect derive rejects most).
- Don't mutate an edit world with `load_into_world` after it is open; use commands.

## References
- RON specification (ron-rs repository documentation); serde data model; RFC 6901 (JSON Pointer); RFC 9562 (UUIDv7).
- Ryū float formatting via the `ryu` crate (Ulf Adams, "Ryū: fast float-to-string conversion", PLDI 2018).

## Status
M0 phase 0.5 complete.

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
