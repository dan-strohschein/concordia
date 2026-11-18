# concordia-testkit: agent guide

## Purpose
Dev-dependency-only test helpers: GPU test guards now, FLIP golden-image comparison and blessing from phase 1.5a.

## Public entry points
- `gpu_required()` / `gpu_required_from(value)` and `REQUIRE_GPU_ENV` (`CONCORDIA_REQUIRE_GPU`)

## Invariants
- Only ever a `[dev-dependencies]` entry; no engine crate depends on it at runtime.
- `CONCORDIA_REQUIRE_GPU=1` (exactly) makes GPU tests fail instead of skipping when no device exists.

## How to test
`cargo nextest run -p concordia-testkit`

## Common tasks
- **Write a GPU test:** probe the device; on `RhiError::NoDevice`, assert `!gpu_required()` and return, otherwise run the test.

## Don'ts
- Don't let a GPU test silently pass on a machine that should have a GPU: always check `gpu_required()`.

## References
- None yet (NVIDIA FLIP from phase 1.5a).

## Status
M1 phase 1.2a: GPU guard.

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
