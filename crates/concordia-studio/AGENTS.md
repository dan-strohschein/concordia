# concordia-studio: agent guide

## Purpose
Agent Studio's harness-neutral source of truth and its engine module: role files, the team, the task board, the Studio journal, and the `task.*` / `studio.*` methods. It never calls an LLM (ADR-0002).

## Public entry points
- `StudioModule`: the engine module (`[studio]` in `concordia.toml` turns it on via `concordia-app`).
- `RoleFile`, `RoleSpec`, `RoleId`: `studio/agents/<role>.md` (YAML frontmatter + Markdown body).
- `Team`, `Gate`, `GateParty`: `studio/team.toml`.
- `Roster::load` / `Roster::resolve`: team overrides applied, tool patterns expanded against a `ToolCatalog`.
- `TaskBoard`, `TaskStore`, `Task`: `studio/tasks/T-0001-<slug>.task.md`.
- `StudioJournal`: `.concordia/studio/journal.jsonl` (ADR-0080); hooks append to it directly.
- `default_roster`, `init`: the embedded default team (`roster/`).

## Invariants
- Text files are the source of truth; task history lives only in the Studio journal, never in task files.
- Every board operation holds `.concordia/studio/board.lock` and first syncs with the journal and the files. A file change no journal entry explains is a hand edit, logged as `human:local`.
- Journal writers lock the file, drop an incomplete last line, then append one line. `seq` is the line number.
- Role `mcp_tools` patterns must each match a tool the project offers; path patterns stay inside the project.
- Only gate parties may approve or reject; rejections past `max_attempts_per_task` block the task for a human.
- The default roster lives only in `roster/`. Its tool scopes name only tools that exist today (M3 refinement G10).

## How to test
`cargo nextest run -p concordia-studio` (role, team, journal, task board and method tests; `insta` snapshots).

## Common tasks
- **Add a role field:** add it to `RoleSpec` and `ROLE_KEYS`, then extend the round-trip property test.
- **Add a journal event:** add a `JournalEvent` variant (snake_case `kind`), and map it in `task()` and `kind()`.
- **Add a method:** define it in `methods.rs`, add it to `all()`, document it in `docs/guides/agents-engine-api.md` and update the schema snapshot.

## Don'ts
- Don't write task history into `.task.md` files, or Studio events into the per-scene command journal.
- Don't add YAML keys to role files without updating `ROLE_KEYS` (unknown keys are errors by design).
- Don't put personal paths or private game details in the default roster.

## References
- Claude Code subagent, hook and skill formats: code.claude.com/docs (checked against Claude Code 2.1.328; see the M3 plan's appendix).
- YAML via `serde_norway` 0.9 (MIT OR Apache-2.0), a maintained fork of `serde_yaml`; globs via `globset` (Unlicense OR MIT).

## Status
M3 phase 3.3 (config model, task board, Studio journal). Sync (3.4), enforcement (3.5) and the full prompts (3.6) come next.

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
