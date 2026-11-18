# AGENTS.md: Concordia

Harness-neutral guide for AI agents (and humans) working on Concordia, an AI-first game engine written in Rust. Claude-specific notes are in `CLAUDE.md`.

## Rules
- **Subscription-first:** nothing may require an LLM API key. The engine is a tool provider: MCP server, CLI and generated agent files.
- **Plan hierarchy:** `docs/design.md` → `plans/M*.md` → phases → code. Read the current milestone plan and `plans/README.md` before you start.
- **Every world mutation is a transaction with an actor.** There is no editor-only logic.
- **Clean room:** never copy, port or translate Unreal Engine source (`docs/clean-room.md`).
- **Privacy:** never put personal paths or private game details in tracked files.

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

## Commands
| Command | Does |
|---|---|
| `just doctor` | Audit the toolchain |
| `just test` | Build WASM fixtures, run all tests and doc tests |
| `just lint` | rustfmt check + clippy (warnings are errors) |
| `just deny` | License and supply-chain checks |
| `just gate M<n>` | Full CI, then the milestone's gate tests |
| `just gate-tests M<n>` | Only the milestone's gate tests (what CI runs after its own checks) |
| `just gate-live M<n>` | Live-agent gate: a real `claude -p` session on the subscription, outcomes asserted |
| `just perf-smoke` | Release-only perf guards |
| `just bench` | Criterion benchmarks (baselines in `crates/*/benches/BASELINE.md`) |
| `just bless` | Accept new snapshots |
| `just new-crate <name>` | Create a crate from the template |

## Layout
`crates/` engine crates (`concordia-*`) · `tests/` cross-crate and gate tests · `wit/` WASM gameplay API · `editor/` editor app · `studio/` Agent Studio config · `tools/` scripts and templates · `third_party/` vendored C/C++ · `examples/` sample projects · `docs/` design and ADRs · `plans/` milestone plans.

## Where to start
1. `docs/design.md`: what we're building and why.
2. `plans/README.md`: conventions and milestone status.
3. The plan for the current milestone, then the `AGENTS.md` of each crate you touch.
