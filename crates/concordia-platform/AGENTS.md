# concordia-platform: agent guide

## Purpose
OS integration behind engine-owned types: one winit window reporting `WindowEvent`s, and a debounced, glob-filtered file watcher.

## Public entry points
- `run_window(&WindowDesc, RunOptions, &mut dyn WindowApp)`, `WindowApp`, `Flow`, `WindowEvent`, `WindowDesc` (reflected; `[window]` in `concordia.toml`)
- `FileWatcher` (`new(root, debounce, ignores)`, `recv_timeout`, `try_iter`), `FileChanged`, `ChangeKind`, `FileWatcher::DEFAULT_IGNORES`
- `key_from_winit`, `mouse_button_from_winit`

## Invariants
- On macOS the window event loop owns the main thread; call `run_window` from `main`'s thread only. Simulation and rendering run on other threads.
- winit types never leave this crate's API (except the conversion helpers).
- File changes are judged by modification time and size, not OS event kinds, so each OS gives the same result and replayed or spurious events produce nothing. Ignore globs like `target/**` also cover the directory itself.
- Key repeats are dropped; pointer positions are logical pixels.

## How to test
`cargo nextest run -p concordia-platform`

## Common tasks
- **Watch a new folder type:** pass it to `FileWatcher::new`; add ignore globs rather than filtering downstream.

## Don'ts
- Don't run `run_window` off the main thread (macOS aborts).
- Don't trust `notify` event kinds for create/modify/remove decisions.

## References
- winit 0.30 `ApplicationHandler` documentation; notify / notify-debouncer-full documentation.

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
