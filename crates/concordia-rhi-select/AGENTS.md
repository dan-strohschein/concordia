# concordia-rhi-select: agent guide

## Purpose
The one place that turns a backend choice into a GPU device (ADR-0088, M4 4.1v(b)). Everything that opens a device (the app's render and pathtrace modules, `concordia gpu-probe`, GPU tests) goes through it instead of naming a backend crate.

## Public entry points
- `BackendChoice::{Auto, Metal, Vulkan}` (serde and `FromStr` by lowercase name; `resolve`, `resolve_for(macos)`)
- `choose(env, project)` and `choose_from_env(project)` → `Backend`; `BACKEND_ENV` (`CONCORDIA_BACKEND`); `SelectError`
- `new_headless(Backend)`, `new_headless_masked(Backend, &CapsMask)`, `probe(Backend)`, `probe_record(Backend)` (JSON)

## Invariants
- Precedence: `CONCORDIA_BACKEND` (empty is unset), then the project's `[render] backend`, then `auto` (Metal on macOS, Vulkan elsewhere). An unknown env value is an error naming the variable, never a silent fallback.
- No `unsafe`, no backend logic: it only dispatches to `concordia-rhi-metal` and `concordia-rhi-vulkan`, both of which build (and report `NoDevice`) everywhere.

## How to test
- `cargo nextest run -p concordia-rhi-select`: parsing and precedence run everywhere; the device tests accept `NoDevice` unless that backend is required (`concordia_testkit::device_required`).

## Common tasks
- **Add a backend:** a `BackendChoice` variant, its `resolve_for` arm, and an arm in each of `new_headless_masked`, `probe` and `probe_record`; the backend crate must build (and report `NoDevice`) on every OS.
- **Open a device elsewhere:** call `choose_from_env(project setting)` and `new_headless(backend)`; never name a backend crate at a call site.

## Don'ts
- Don't fall back silently: an unknown `CONCORDIA_BACKEND` is an error.
- Don't put device or backend logic here; it belongs in the backend crates.

## References
- ADR-0088 (decision 14, backend selection). No external code consulted.

## Status
M4 phase 4.1v(b): the selector, `[render] backend` and `gpu-probe --backend`.

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
