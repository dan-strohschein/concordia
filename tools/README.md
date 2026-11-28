Developer scripts (`doctor.sh`), crate templates, and exporter plugins (M5).

## Disk (2026-11-27)

- `just disk`: free space and the size of the build folders. `just prune-builds`: removes old
  incremental snapshots, the release build, golden review output and perf reports, then build
  units Cargo has not written for 3 days (`disk/sweep.py`: `target/<profile>/build/<crate>/<hash>/`,
  each source change leaves one behind; `--dry-run` lists, `--days N`; tests in
  `python3 -I tools/disk/sweep_test.py`). The next debug build recompiles only what changed.
  `just clean-builds` only when pruning is not enough.
- **Building on an external drive:** `target/` may be a symlink to a folder on another disk. Every recipe and test path (`target/wasm-fixtures`, `target/golden-review`, …) keeps working. Move it with Cargo idle: `mv target /Volumes/<drive>/concordia-target && ln -s /Volumes/<drive>/concordia-target target`. Use APFS (not exFAT). `guard.sh` then measures that drive and stops builds while it's unplugged.
- `disk/guard.sh`: a Claude Code `PreToolUse` hook (register it in your git-ignored
  `.claude/settings.local.json` under `Bash`, command `"$CLAUDE_PROJECT_DIR"/tools/disk/guard.sh`).
  Below 30 GB free (`DISK_STOP_GB`) it blocks cargo and `just` build commands and says why; below
  40 GB (`DISK_WARN_GB`), or with `target/` over 45 GB (`DISK_TARGET_WARN_GB`), it asks the agent to
  prune first. `du` over `target/` takes seconds, so its size is cached in
  `target/.disk-guard-cache` and refreshed in the background every 5 minutes
  (`DISK_TARGET_CACHE_SECONDS`); the hook itself stays under 50 ms. Pruning and listing recipes
  always pass.
- Fewer test binaries (2026-11-27): each links the whole engine (about 200 MB), so
  `concordia-tests`, `concordia-app` and `concordia-mcp` build one `integration` binary each
  (`tests/integration/main.rs`, `tests/main.rs`); a file's tests are `<file>::<test>`
  (`--test integration m4_lighting_quality::`). Only `bless_reference` keeps its own release-only binary.
  `repo_hygiene` fails if a test file is in no binary.
- Leaks: the M3 gates' fixture builds (`target/m3-fixture/<sources hash>/`) drop other sources
  versions' builds unused for an hour; `just mutants` removes its worktrees and their target
  folders when the run ends, however it ends.
- Debug builds carry line tables only, and dependencies no debug info (root `Cargo.toml`); use
  `CARGO_PROFILE_DEV_DEBUG=true` for a build you step through in a debugger.

## Checks (2026-11-25)

- Per phase (author decision 2026-11-25, later): only the phase's new tests and the few existing
  tests its code directly affects, plus fmt and clippy on the changed crates; no `just check`,
  mutants or Vulkan runs (`plans/README.md`). Everything below is for a milestone's end, or when
  asked.
- `just check` (`check.sh`, `affected.py`): the affected crates' tests and their dependents'. Only the
  tests of the crates the branch changes and of their dependents run, without the milestone gates
  (`tests/gates/m<n>_*`) unless a gate changed; clippy on the changed crates; the editor's tests,
  the Tauri clippy and deny only when their files changed (the editor's end-to-end tests only for
  more than regenerated types). A changed root `Cargo.toml` or `Cargo.lock` runs the full checks.
  `python3 -I tools/affected_test.py` tests the selection (`just check` runs it first).
- `just check full`: a milestone's end. Everything, with independent steps overlapped and never
  two builds at once (memory-bound). Run the on-demand `vulkan` workflow at the same point.
- `just mutants <file.json> [workers]` (`mutants/run.py`; a milestone's end only): mutation checks in parallel git
  worktrees under `target/mutants/` (2 workers by default, sharing `CARGO_BUILD_JOBS`); each test
  selection runs unmutated first. A phase mutates only the code it changed; a milestone's end runs
  the full sweep. `just clean-builds` (only when free disk is under 25 GB) removes the worktrees'
  builds with the rest.
