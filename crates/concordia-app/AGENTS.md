# concordia-app: agent guide

## Purpose
The composition root: loads a project's `concordia.toml` and builds the engine modules it uses, so the CLI, MCP server and editor never repeat the list (ADR-0046). It also decides whether a process attaches to the project's live engine or hosts one (ADR-0082).

## Public entry points
- `Project::load(root)`, `Project::engine_config()` (modules plus `[sim]` rate and seed and `[journal]` sync: every process that builds an engine from a project starts here, 2.8), `Project::modules()`, `Project::input_map()`
- `Manifest` and its sections (`[project]`, `[scene]`, `[sim]`, `[journal]`, `[window]`, `[input]`, `[physics]`, `[assets]`, `[scripts]` with `[[scripts.module]]` `kind`/`path`/`source`; the scripts module is added last, 2.6d; `[replay]`, no keys yet, adds the replay module before scripts, 2.8; `[anim]`, no keys yet, adds the animation module after assets, 2.4.1; `[nav]`, no keys yet, adds the navigation module after physics, 2.5a)
- `SharedEngine::attach_or_host[_on]`, `SharedEngine::attach_live`, `HostedEngine`: one engine per project (ADR-0082)
- `model_importers()`: the glTF and FBX importers, registered by the assets module and by `concordia import` (2.3d)
- `AppError`

## Invariants
- Unknown manifest keys are errors; a missing `concordia.toml` means all defaults.
- Paths in the manifest are project-relative and may not escape the project.
- One engine per project: attach-or-host runs under `.concordia/engine.lock`; a stale `headless.json` (dead pid, refused token) is replaced; a live engine of another version is never attached; a host removes its file on drop only if the file still names it.
- A module is included only if the manifest configures it (input needs `[input] map`; physics, assets and render need their sections), so M0 projects keep the M0 tool set.
- `[render] closure_budget` (1..=8) is checked when the project loads and can only lower the platform's closure budget (ADR-0068); `Project::closure_budget` passes it to `RenderModule::with_closure_budget`.

## How to test
`cargo nextest run -p concordia-app`

## Common tasks
- **Start an engine in a new process kind:** use `SharedEngine::attach_or_host` with a `HostKind`, never `Engine::new` plus `ws::serve` directly.
- **Add a module:** read its settings from a new manifest section, append it in `Project::modules`, and add a test in `tests/project.rs`.

## Don'ts
- Don't build module lists anywhere else.

## References
- TOML 1.0 specification.

## Status
M1 phase 1.1; M3 phase 3.5b (shared engine); M2 phase 2.6d (`[scripts]`); M2 phase 2.8 (`[replay]`, `engine_config`); M2 phase 2.4.1 (`[anim]`); M2 phase 2.5a (`[nav]`).

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
