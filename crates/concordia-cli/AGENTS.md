# concordia-cli: agent guide

## Purpose
The `concordia` command: `new` (project skeleton), `headless` (Engine API over stdio or localhost WebSocket), `mcp` (the MCP server), `schema` (print or export schemas) and `fmt` (canonical scene formatting). Every subcommand is a thin wrapper over `concordia-api`, `concordia-scene` or `concordia-mcp`.

## Public entry points
- The `concordia` binary (`src/main.rs`); subcommands in `src/cmd/`.

## Invariants
- No engine logic here; subcommands call library crates.
- `concordia new` writes `concordia.toml`, `scenes/main.scene.ron` (canonical), `.gitignore` (`.concordia/`) and `.mcp.json` registering the server as `concordia` (`<abs concordia> mcp --project .`). It refuses a non-empty directory without `--force` and never deletes files.
- `concordia replay record <path> --ticks N [--scene] [--inputs events.json] [--checkpoint-every]` and `concordia replay run <path> [--mode] [--force] [--threads]` run in process; `run` opens the replay's scene, prints the result JSON and exits 1 on a mismatch or divergence (2.8).
- Exit codes: 0 success; 1 usage errors, missing items and `fmt --check` failures; 2 files that cannot be parsed.
- `headless --ws` binds to 127.0.0.1 only and writes `.concordia/headless.json` (mode 0600).
- `fmt` skips hidden directories (such as `.concordia`) and `target`, and knows only the built-in components in M0.

## How to test
`cargo nextest run -p concordia-cli` (runs the real binary; `.mcp.json` snapshot with the binary path redacted).

## Common tasks
- **Add a subcommand:** add a variant to `Command` in `src/main.rs`, implement it in `src/cmd/<name>.rs` over a library crate, test it through the binary in `tests/cli_<name>.rs`.
- **Change the project template:** edit `src/cmd/new.rs`; update `tests/cli_new.rs` and accept the snapshot.

## Don'ts
- Don't print anything but results to stdout (`headless` stdio and `mcp` own it).
- Don't write absolute local paths into tracked repository files (only into generated user projects).

## References
- clap documentation.

## Status
M0 phase 0.7 complete; M2 phase 2.8 (`concordia replay`); M2 phase 2.4.5 (`concordia new` writes `[anim]`; `studio init` adds `[assets]` and `[anim]` when missing, for the animator); M2 phase 2.5a (`concordia new` writes `[nav]`; `studio init` adds it when missing, for the level designer); M2 phase 2.5b.2a (`concordia new` writes the starter input map `input/default.input.ron` and `[input] map`; `studio init` adds them when missing, for QA).

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
