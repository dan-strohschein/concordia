# concordia-perf: agent guide

## Purpose
Perf testing on reference hardware (ADR-0089): per-scene budgets as text, the reference machine, measured reports, verdicts that never claim a pass off the reference, and the runner that measures a perf scene through the Engine API. Per-pass timing itself lives in the RHI (timer scopes) and `concordia-render::perf` (`PassTimer`, perf mode).

## Public entry points
- `run(scene_file, &RunOptions, &dyn HostProbe) -> Run` (`Run::write(out)` writes `<out>/<name>.perf.ron`); `concordia perf run` and `just perf` call it.
- `PerfScene` (`tests/perf/scenes/<name>.perf.ron`: project, scene, size, warm-up and measured frames, `fov_y`, camera `path`; `pose(i, frames)`).
- `BudgetFile` / `Limit` (`tests/perf/budgets/<name>.budget.ron`; `read`, `to_text`, `validate`).
- `ReferenceMachine` (`tests/perf/reference.ron`; `mismatch(&HostInfo)`), `HostInfo`, `HostProbe`, `machine::SystemProbe`.
- `PerfReport` / `Summary` / `report::Status` (`record(&Verdict)`), `check`, `judge`, `Verdict`, `Violation`, `Stat`.

## Invariants
- Only the reference machine can `Pass`. Off it the verdict is `NotReference` (with the would-be violations), even with no budget file.
- A budget bound is a positive, finite number of ms, and each limit sets at least one bound; a budgeted pass that never ran is a violation.
- Summaries: median (mean of the middle two for even counts) and nearest-rank p95; any non-finite sample makes the series invalid.
- Every perf file is canonical RON through `concordia-scene` (`tests/repo_files.rs` checks the repository's).
- The runner turns on `[render] perf_mode` in memory only; it never edits the project.
- The perf folder layout is fixed: `reference.ron`, `scenes/<name>.perf.ron`, `budgets/<name>.budget.ron`.

## How to test
`cargo nextest run -p concordia-perf`. `tests/runner.rs` and `tests/host.rs` measure the M1 gate scene and read this host; they skip without a GPU unless `CONCORDIA_REQUIRE_GPU=1`. The real measurements: `just perf` (release; reports in `target/perf/`).

## Common tasks
- **Add a perf scene:** write `tests/perf/scenes/<name>.perf.ron` (canonical; `concordia_scene::write_document` of a `PerfScene`), run `just perf` on the reference Mac, then propose `tests/perf/budgets/<name>.budget.ron` for the author to approve.
- **Re-derive budgets** (new reference machine or macOS major version): update `reference.ron`, run `just perf` three times, and propose new budget files.

## Don'ts
- Don't compare GPU times across machines, or claim a pass off the reference.
- Don't put budget numbers in code.
- Don't measure with other GPU work running (tests, builds that compile shaders): the numbers drift.

## References
- Apple, "Sampling GPU data into counter sample buffers" (Metal documentation) and *Metal Feature Set Tables*; nearest-rank percentile (NIST/SEMATECH e-Handbook of Statistical Methods). No engine source consulted.

## Status
M4 phase 4.1a: budgets, the reference machine, reports, verdicts, `concordia perf run`; scenes `m1-gate` and `lamplight-night`.

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
