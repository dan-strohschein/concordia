# CLAUDE.md: Concordia

**Concordia** is a game engine written from scratch in Rust. It is AI-first, open source and easy to work in, and aims for Unreal-class results. Start with `docs/design.md`. Harness-neutral rules and commands: see `AGENTS.md`.

## Rule zero: subscription-first
Nothing in this engine may *require* an LLM API key. The engine is a **tool provider**: an MCP server (`concordia-mcp`), the `concordia` CLI, and generated Claude Code files (`.claude/agents`, skills, hooks). The user's own agent harness does the thinking; Claude Code on a regular subscription is the primary target. Direct-API or local-model backends are opt-in plugins only.

## Plan hierarchy
1. `docs/design.md` is the top-level design. Change it only by deliberate decision, and record that decision as an ADR.
2. `plans/M*.md` holds one implementation plan per milestone, split into phases.
3. Executing a phase produces code, **test-first**.

Before working on a milestone, read its plan and `plans/README.md`, which defines the shared conventions: crate names, file formats, API naming and test rules. Update the plan's Status when a phase or gate completes. If reality diverges from the plan, update the plan in the same change.

## TDD is mandatory
- Write the phase's listed tests first, run them, and see them fail for the right reason.
- Implement the minimum needed to pass them, then refactor.
- Never weaken or delete a test to make it pass. If a test is wrong, say so and fix it deliberately.
- Every milestone ends with a gate test in `tests/gates/`, run with `just gate M<n>`.

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

## Code conventions
- Small crates under `crates/concordia-*`, each with `AGENTS.md` (purpose, invariants, how to test) and `README.md`.
- `cargo fmt`, `cargo clippy -D warnings` and `cargo nextest run` must be clean before a change is done.
- Every world mutation goes through `concordia-command` with an `Actor`. There is no editor-only logic; the editor is just an Engine API client.
- Text formats (RON, canonical formatting) are the source of truth. Binary data lives only in `.concordia/ddc`.

## Clean-room rule
Implement from papers, public talks, documentation and permissively licensed code. **Never** copy, port or translate code from Unreal Engine or any other source-available-but-not-open-source engine. See `docs/clean-room.md`. Record the references you used in the crate's `AGENTS.md`.

## Port target
The first real game is the author's Godot 4 isometric ARPG (the **reference port**). Its details are private: they live in the git-ignored `private/` folder (`private/port-target.md`) and are reached through `CONCORDIA_PORT_SRC`/`CONCORDIA_PORT_PROJECT`. **Never** copy game names, paths or content into tracked files. The game's headless test suite is the oracle for the port.
