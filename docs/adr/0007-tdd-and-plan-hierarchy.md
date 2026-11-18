# ADR-0007: TDD and the plan hierarchy
- Status: Accepted
- Date: 2026-11-17

## Decision
Design (`docs/design.md`) → milestone plans (`plans/`) → phases → code. Every phase lists its tests first; those tests are committed failing before the implementation. Every milestone has an executable gate test in `tests/gates/`, run with `just gate M<n>`.
