# concordia-command: agent guide

## Purpose
The single path for mutating an edit world. Every change is an `EditOp` (data, not closures) applied inside an attributed, atomic transaction with linear undo/redo, and recorded in an append-only JSON Lines journal that can be replayed and resumed.

## Public entry points
- `CommandLog` (`begin`, `apply`, `commit`, `abort`, `apply_one`, `undo`, `undo_to`, `redo`, `record_save`, `history`, `resume`, `attach_journal`)
- `EditOp` (`Spawn`, `Despawn`, `SetComponent`, `RemoveComponent`, `MigrateType`) and `AppliedOp`
- Applying, undoing, redoing and replaying take `&mut TypeRegistry` (2.6d): a `MigrateType` op registers the script type's new version, converts every stored value through reflect `migrate`, and its inverse (remove the component, return to the previous version, set every previous value) restores the exact previous version and values
- `Actor` (`<human|agent|system|tool|script>:<name>[#n]`, reflected as a string with `ACTOR_PATTERN`) and `Author` (actor + session)
- `Journal` (`open`, `load`, `append`), `JournalEntry`, `Event`, `replay_journal`
- `CommandError`, `History`, `HistoryEntry`

## Invariants
- Nothing outside `CommandLog` mutates an edit world (the Engine API, CLI and editor all go through it).
- Ops apply immediately (reads inside a transaction see its writes); each op is atomic, and its inverse is computed at apply time.
- A failing op rolls back its whole transaction. In an explicit transaction this records an `abort` with reason `op_failed`; an implicit single-op transaction that fails records nothing.
- A commit, undo or redo is journaled before it counts: if the journal write fails, the world is rolled back.
- Empty commits are not recorded.
- Undo and redo are rejected while a transaction is open; any actor may undo, and the undo is attributed to that actor. `undo_to` is a run of ordinary undos (one journal entry each).
- `HistoryEntry` carries the journal time and the entities a commit touched (undo/redo rows copy their target's); nothing new is journaled for them.
- The journal is append-only JSONL with `v: 1`; `seq` increases by 1 across restarts; `ts_ms` is audit-only and never read by replay.

## Journal format
Path: `<project>/.concordia/journal/<scene-id>.cmdlog.jsonl`. One object per line:
- common: `v`, `seq`, `kind`, `session?`, `ts_ms`
- `commit`: `txn`, `actor`, `label`, `ops: [{op, inverse}]`, `hash_before`, `hash_after`
- `undo` / `redo`: `target`, `actor`, `hash_after`
- `abort`: `txn`, `actor`, `label`, `reason`
- `save`: `actor`, `path`, `hash`

A truncated last line (crash mid-write) is reported as `JournalWarning::TruncatedTail` and cut on the next `Journal::open`; any other bad line is an error. `CommandLog::resume` restores the undo/redo stacks only when the journal's last recorded hash equals the loaded scene's hash; otherwise history is `Stale` and only new changes can be undone.

## Documents (M4 4.10c)
`EditOp::Document { path, before, after }` changes a project text file with full texts (byte-exact undo; refused if the file no longer holds `before`). The log needs `attach_documents` (the engine attaches the project folder); replay skips document ops.

## How to test
`cargo nextest run -p concordia-command`

## Common tasks
- **Add a new `EditOp`:** add the variant (doc-commented) in `src/op.rs`; implement it and its inverse in `src/apply.rs` so that a failure leaves the world unchanged; add a test that applies, undoes and redoes it, and extend the proptest in `tests/undo_redo.rs`; check `tests/replay.rs` still passes; update the journal snapshot if the format changes.
- **Record a new audit event:** add an `Event` variant, a `HistoryEntry` mapping in `summarize`, and its handling in `rebuild_stacks` and `replay_journal`.

## Don'ts
- Don't mutate a `World` directly from engine services; build an `EditOp`.
- Don't put closures or Rust-only types in ops; ops must round-trip through JSON.
- Don't read `ts_ms` in logic.

## References
- Gamma, Helm, Johnson & Vlissides, *Design Patterns*, "Command"; Robert Nystrom, *Game Programming Patterns*, "Command".
- Write-ahead logging concepts (database literature); JSON Lines (jsonlines.org).

## Status
M0 phase 0.4 complete; M2 phase 2.6d added `MigrateType` (ADR-0061).

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
