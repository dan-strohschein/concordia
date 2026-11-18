# concordia-api: agent guide

## Purpose
The one Engine API: typed `domain.verb` methods with generated schemas, JSON-RPC 2.0 over stdio and localhost WebSocket, sessions with default actors, transaction ownership, edit and play worlds, and agent-actionable errors. The MCP server and CLI are thin adapters over it.

## Public entry points
- `Engine` (`new`, `open_session`, `call`, `handle_jsonrpc`, `close_session`, `poll`, `spawn`, `scene_text`, `methods`, `world`, `play_state`) and `EngineConfig` (`modules`)
- `EngineHandle` (async `open_session`, `handle_jsonrpc`, `call`, `close_session`; `methods`)
- `EngineModule` and `PlaySetup` (ADR-0046): how other crates add types, methods, play systems and play-time state
- `methods()` → the core `MethodRegistry`; `Engine::methods()` is the full set. `MethodRegistry::build`, `params_schema`, `result_schema`, the `Method` trait
- `ApiError`, `ApiErrorKind`, `ErrorData`
- `transport::stdio::serve`, `transport::ws::{serve, WsConfig, WsServer, write_connection_file}`

## Invariants
- Every mutating method requires an actor (explicit `actor` or the session default) and changes the edit world only through `methods::edit` → `CommandLog`. A grep for direct world mutation finds only the play world's `SimRng`.
- Only the session holding an open transaction may mutate; others get `TxnConflict` with `holder`. Open transactions abort on session close and after `txn_timeout` (journal reasons `session_closed`, `timeout`).
- Simulation runs only on the play world (ADR-0032).
- Every error kind has a code, a default message and a suggestion (ADR-0031); mapping lives in `src/map.rs`.
- Method docs (the marker type's doc comment) and param/result field docs are agent-facing: they become tool descriptions and schemas. Params and results are `#[derive(Reflect)]`; nothing is hand-written.
- Scene paths are project-relative `.scene.ron` paths without `..`. WebSocket binds to loopback only, requires the token, and rejects non-local Origins.
- All engine state lives on one thread; transports only forward messages.
- Modules run in order: core first, then `EngineConfig::modules`. Module and method names are unique (`Engine::new` fails otherwise). `start_play` runs once per new play world; its state is dropped with the play world (`sim.stop`, `scene.open`, `scene.new`). Non-`Clone` runtime objects (physics, GPU) live in play state or in systems, never in `World` resources.
- `sim.*` accept an attribution-only `actor`; `sim.step` logs `sim stepped` (actor, ticks, world_hash). Nothing in `sim.*` is journaled.

## How to test
`cargo nextest run -p concordia-api` (dispatch, errors, sessions, sim, transports, log, schema snapshots).

## Common tasks
- **Add a method:** create a doc-commented marker struct, `Params` and `Result` types (`#[derive(Reflect)]`, doc comments on every field, `actor: Option<Actor>` if it mutates) in `src/methods/<domain>.rs`; implement `Method`; add it to that module's `methods()`; mutate only via `edit` or `CommandLog`; add tests; accept the `method_index_snapshot` change. MCP picks it up automatically.
- **Add a module:** implement `EngineModule` in your crate (types via `register_component`, methods via `MethodInfo::of`, play systems and state in `start_play`); read worlds with `Engine::world` and state with `Engine::play_state`; add it to the engine's `EngineConfig::modules`. Its methods become MCP tools automatically.
- **Add an error kind:** add it to `ApiErrorKind` and `ALL` with a code, name, message and suggestion; map the source error in `src/map.rs`; update `docs/guides/agents-engine-api.md`.

## Don'ts
- Don't write to stdout from this crate (stdio transports own it).
- Don't mutate `scene.world` outside `methods::edit` or `CommandLog`.
- Don't hand-write JSON Schemas for params or results.

## References
- JSON-RPC 2.0 specification; RFC 7396 (JSON Merge Patch); RFC 6901 (JSON Pointer); RFC 6455 (WebSocket).
- OWASP guidance on cross-site WebSocket hijacking (Origin checks, tokens).

## Status
M0 phase 0.6 complete. M1 phase 1.0: engine modules.

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
