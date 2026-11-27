Developer scripts (`doctor.sh`), crate templates, and exporter plugins (M5).

## Disk (2026-11-27)

- `just disk`: free space and the size of the build folders. `just prune-builds`: removes old
  incremental snapshots, the release build, golden review output and perf reports (the next debug
  build recompiles only what changed). `just clean-builds` only when pruning is not enough.
- `disk/guard.sh`: a Claude Code `PreToolUse` hook (register it in your git-ignored
  `.claude/settings.local.json` under `Bash`, command `"$CLAUDE_PROJECT_DIR"/tools/disk/guard.sh`).
  Below 30 GB free (`DISK_STOP_GB`) it blocks cargo and `just` build commands and says why; below
  40 GB (`DISK_WARN_GB`) it asks the agent to prune first. Pruning and listing recipes always pass.
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
