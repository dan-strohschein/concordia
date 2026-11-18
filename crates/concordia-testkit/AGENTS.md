# concordia-testkit: agent guide

## Purpose
Dev-dependency-only test helpers: GPU test guards and golden images compared with NVIDIA FLIP.

## Public entry points
- `gpu_required()` / `gpu_required_from(value)` and `REQUIRE_GPU_ENV`
- `golden::{assert_golden, Golden, RgbaImage, Tolerance, FlipScore, flip_score, GoldenError}`

## Invariants
- Only ever a `[dev-dependencies]` entry.
- Goldens live at `tests/golden/<suite>/<name>.png`. Blessing needs `CONCORDIA_BLESS=1` and is refused when `CI=true`. Failures write `<name>.{actual,expected,flip}.png` to `target/golden-failures/<suite>/`.
- FLIP is LDR (RGB, alpha ignored) at the default pixels-per-degree; the default tolerance is mean ≤ 0.01, p99 ≤ 0.25.
- Golden names are `[A-Za-z0-9_@-]+` (no path escapes).

## How to test
`cargo nextest run -p concordia-testkit`

## Common tasks
- **Add a golden test:** render, then call `assert_golden(suite, name, &image, Tolerance::mean(x))`; run `just bless`, check the PNG by eye, and commit it.

## Don'ts
- Don't let a GPU test silently pass on a machine that should have a GPU: check `gpu_required()`.
- Don't commit goldens you have not looked at.

## References
- NVIDIA FLIP (BSD-3-Clause) through the `nv-flip` crate (MIT/Apache/Zlib); Andersson et al., "FLIP: A Difference Evaluator for Alternating Images" (HPG 2020).

## Status
M1 phase 1.2b (golden harness moved up from 1.5a).

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
