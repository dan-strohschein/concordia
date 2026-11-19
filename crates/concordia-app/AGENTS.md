# concordia-app: agent guide

## Purpose
The composition root: loads a project's `concordia.toml` and builds the engine modules it uses, so the CLI, MCP server and editor never repeat the list (ADR-0046).

## Public entry points
- `Project::load(root)`, `Project::modules()`, `Project::input_map()`
- `Manifest` and its sections (`[project]`, `[scene]`, `[sim]`, `[journal]`, `[window]`, `[input]`, `[physics]`, `[assets]`)
- `AppError`

## Invariants
- Unknown manifest keys are errors; a missing `concordia.toml` means all defaults.
- Paths in the manifest are project-relative and may not escape the project.
- A module is included only if the manifest configures it (input needs `[input] map`; physics, assets and render need their sections), so M0 projects keep the M0 tool set.

## How to test
`cargo nextest run -p concordia-app`

## Common tasks
- **Add a module:** read its settings from a new manifest section, append it in `Project::modules`, and add a test in `tests/project.rs`.

## Don'ts
- Don't build module lists anywhere else.

## References
- TOML 1.0 specification.

## Status
M1 phase 1.1.

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
