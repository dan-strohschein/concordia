# concordia-perf

Perf testing on reference hardware (ADR-0089): measure a perf scene's GPU time per render-graph pass along a scripted camera path, write `target/perf/<scene>.perf.ron`, and judge it against `tests/perf/budgets/<scene>.budget.ron`. Only the reference machine (`tests/perf/reference.ron`, the author's M3 Pro) can pass; elsewhere a run reports `not_reference`. Run it with `concordia perf run tests/perf/scenes/<scene>.perf.ron` or `just perf`.

See `AGENTS.md` for invariants, tests and references.
