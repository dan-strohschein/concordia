# concordia-testkit: agent guide

## Purpose
Dev-dependency-only test helpers: GPU test guards and golden images compared with NVIDIA FLIP.

## Public entry points
- `gpu_required()` / `gpu_required_from(value)` and `REQUIRE_GPU_ENV` (Metal)
- `vulkan_required()` / `vulkan_required_from(value)` and `REQUIRE_VULKAN_ENV` (`CONCORDIA_REQUIRE_VULKAN`; M4 4.1v(a))
- `golden::RhiGoldens(Backend)` (feature `rhi`): the RHI conformance suite's `GoldenCheck`, shared by every backend, with that backend's tolerance
- `device_required(Backend)` / `device_required_from`: Metal by `CONCORDIA_REQUIRE_GPU=1`, Vulkan by `CONCORDIA_REQUIRE_VULKAN=1` (M4 4.1v(b))
- `golden::{assert_golden, Golden, RgbaImage, Tolerance, Tolerances, FlipScore, flip_score, GoldenError}`; `Golden::tolerance(suite, name)` and `tolerance_on(backend, ..)`, `Golden::check_on(backend, ..)` and `assert_golden_on`, `Golden::with_keep`, `BackendTolerances`, `KeptScore`, `Golden::write_comparison` (actual, expected and FLIP heat map), `Golden::is_ci`
- `reference::{ReferenceFile, ReferenceLook, Blessed, SUITE}` (M4 4.1b4, ADR-0090): `<name>.reference.ron` in `tests/golden/m4_reference/` (inputs, and when blessed the backend, git hash, baseline FLIP and noise FLIP), canonical and validated; `Golden::reference(name)` and `Golden::bless_reference(name, file, image)`

## Invariants
- Only ever a `[dev-dependencies]` entry, or a dependency of the test-only `concordia-tests` crate (its reference-blessing harness); never of a crate that ships.
- Blessing a reference is refused in CI, without a `blessed` record, and when its noise FLIP mean exceeds a fifth of its tolerance's mean; nothing is written then.
- Goldens live at `tests/golden/<suite>/<name>.png`. Blessing needs `CONCORDIA_BLESS=1` and is refused when `CI=true`. Failures write `<name>.{actual,expected,flip}.png` to `target/golden-failures/<suite>/` (`<suite>/<backend>/` for a backend's check). `CONCORDIA_GOLDEN_KEEP=1` also writes them, plus `<name>.score.ron`, for checks that pass (review pages). Only Metal (or no backend) may bless: every backend compares against the same images (D4).
- FLIP is LDR (RGB, alpha ignored) at the default pixels-per-degree.
- Tolerances live in text (M4 4.1a, D2): `tests/golden/<suite>/tolerances.ron` (`Tolerances { default, goldens: {name: (mean, p99)} }`, canonical RON, values in [0, 1]). A golden's tolerance is its entry, else the file's default; an optional `backends: {"vulkan": (default: Some(..), goldens: {..})}` overrides per backend, most specific first (the backend's entry, the shared entry, the backend's default, the shared default; M4 4.1v(b), D4); only a suite without a file falls back to `Tolerance::default()` (mean ≤ 0.01, p99 ≤ 0.25), the one tolerance in code. `repo_hygiene::golden_tolerances_live_in_text` forbids tolerance literals outside this crate; `every_tolerance_entry_names_an_existing_golden` keeps the files honest.
- Golden names are `[A-Za-z0-9_@-]+` (no path escapes).

## How to test
`cargo nextest run -p concordia-testkit`

## Common tasks
- **Add a golden test:** render, then call `assert_golden(suite, name, &image)`; if it needs other than the suite's default, add its entry to `tests/golden/<suite>/tolerances.ron`. Run `just bless`, check the PNG by eye, and commit it.

## Don'ts
- Don't let a GPU test silently pass on a machine that should have a GPU: check `gpu_required()`.
- Don't commit goldens you have not looked at.

## References
- NVIDIA FLIP (BSD-3-Clause) through the `nv-flip` crate (MIT/Apache/Zlib); Andersson et al., "FLIP: A Difference Evaluator for Alternating Images" (HPG 2020).

## Status
M1 phase 1.2b (golden harness moved up from 1.5a). M4 phase 4.1a: tolerances per suite in `tolerances.ron` (D2).

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
