# ADR-0071: Fast automatic CI, manual regression suite
- Status: Accepted (2026-11-20; author's decision)
- Date: 2026-11-20
- Amends: ADR-0022 (CI platforms)

## Context
ADR-0022 ran everything on every PR and every push to main, on both macOS and Linux: lint, tests, doc tests, `cargo deny` and the gates, plus the editor's macOS end-to-end and shell jobs. By M2, the macOS jobs took 10–25 minutes. The tests themselves take about 80 seconds.
- **Compiling dominates.** GitHub's macOS runners are slow, and `rust-cache` caches only dependencies. A change to a low-level crate such as `concordia-reflect` recompiles almost the whole workspace and every test binary.
- **The cache thrashed.** The repository's caches (about 11.7 GB) exceeded GitHub's 10 GB limit, and PR caches evicted main's. Runs often started cold.

The author wants the automatic build to take at most 5 minutes, and to run the regression suite by hand. Every phase already runs the whole suite locally before it is committed (the commit checklist in `plans/README.md` and the agent rules).

## Decision
1. **`ci`** runs automatically on PRs and pushes to main, on Linux only. Its jobs run in parallel:
   - `lint`: rustfmt and clippy for the workspace and the guest fixture, then the WASM fixture build;
   - `supply chain`: `cargo deny` for the workspace, the guest and the editor shell.
2. **`editor`** keeps its automatic Linux UI job: lint, unit tests and the generated-types drift check.
3. **`regression`** is run by hand (`workflow_dispatch`, with a `suite` of `all`, `engine` or `editor`). It holds everything else:
   - the test suite on macOS (GPU required) and Linux;
   - doc tests;
   - the gates M-1, M0, M1 and M3S;
   - the editor's macOS end-to-end tests and native shell tests.
4. **Rust caches are saved only from main.** PR runs restore them but never write, so they can't evict main's.
5. **The repository hygiene test pins this split.** `ci.yml` must contain the fast checks and must not run tests, gates or macOS jobs. `regression.yml` must contain the full suite.

## Alternatives
- **sccache or a remote build cache.** It speeds compiling up, but the macOS jobs would still be far over 5 minutes, and it needs a storage backend.
- **A self-hosted macOS runner (the author's Mac).** It's fast and has a real GPU. It's still planned for the private-content gates, but it shouldn't block every PR.
- **Running tests on Linux only, automatically.** About 5½ minutes warm, over budget, and it still misses the GPU tests.
- **Nightly scheduled runs.** These spend runner minutes when nothing changed. Manual runs fit the author's cadence of a few merges a day.

## Consequences
- A PR can merge with a regression that the local checklist missed. Mitigations: the local checklist runs everything before each commit, and the author can run `regression` on a branch or on main at any time.
- "A milestone is done only when its gate passes in CI" now means a green manual `regression` run on main.
