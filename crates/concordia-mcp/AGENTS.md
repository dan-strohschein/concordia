# concordia-mcp: agent guide

## Purpose
The MCP server (`concordia-mcp`, also `concordia mcp`): a thin rmcp adapter that exposes every Engine API method except `session.open` as a tool with its generated schemas, plus schema, doc and scene resources (ADR-0033). It needs no API key.

## Public entry points
- `run(Args)`: logging to stderr, then serve MCP on stdio (shared by both binaries)
- `ConcordiaServer::start(Options)` and its rmcp `ServerHandler` impl (in-process tests serve it over a duplex pipe)
- `Options`, `Args`, `StartError`

## Invariants
- No hand-written schemas: tool `inputSchema`/`outputSchema` equal `params_schema`/`result_schema` byte for byte.
- Tools map 1:1 to API methods (`.` → `_`), minus `session.open`.
- stdout is protocol-only; logs go to stderr.
- rmcp is used only in `src/tools.rs` and `src/resources.rs`.
- Engine errors are tool errors; unknown tools are protocol errors.
- `human:*` actors are rejected (startup and per call).
- No API key or environment variable is required (tested with a cleared environment).

## How to test
`cargo nextest run -p concordia-mcp` (in-process tools and resources, plus binary stdout and empty-environment tests). End to end: `just gate M0`.

## Common tasks
- **A new API method:** nothing to do here; it appears as a tool automatically. Update the M0 gate's expected tool count if it is part of M0.
- **Upgrade rmcp:** bump the exact pin in the root `Cargo.toml`; fix compile errors in `src/tools.rs`, `src/resources.rs` and `tests/src/mcp_client.rs`; run `cargo nextest run -p concordia-mcp` and `just gate M0`; record notable protocol changes in ADR-0033.
- **Add a resource:** list it in `resources::list` (or `templates`), serve it in `resources::read`, add a test in `tests/resources.rs`.

## Don'ts
- Don't use rmcp's `#[tool]` macros or schemars derives.
- Don't print to stdout.
- Don't add engine logic here; put it in `concordia-api`.

## References
- Model Context Protocol specification (2025-11-25 and 2026-07-28); `rmcp` 3.11 documentation and source.

## Status
M0 phase 0.7 complete.

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
