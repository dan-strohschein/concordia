Milestone gate tests: `m<n>_<name>.rs` (`M-1` uses prefix `m_minus1`), run with `just gate M<n>`. Each file is a module of the `integration` test binary: declare it in `tests/integration/main.rs` (`#[path = "../gates/<file>.rs"] mod <file>;`); its snapshots are `snapshots/integration__<file>__<name>.snap`.

Live gates (`just gate-live M<n>`, a real Claude Code session on a subscription) commit a small RON report per passing run under `live-reports/M<n>/` (no transcripts).
