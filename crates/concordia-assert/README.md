# concordia-assert

The playtest assertion language: `within 600 ticks: distance(tag("player"), tag("door")) < 2m`. Temporal operators (`within`, `always`, `never`, `eventually`, `at`, `after`) over predicates on entity selectors (`tag`, `name`, `id`, `with`), distances, counts, events and component fields, with units (`m`, `s`, `ticks`, `deg`). Text parses with precise error spans and prints back canonically; the type checker resolves components and field paths against the reflection registry, checks units and turns seconds into ticks; a Rust builder makes the same trees for tests. `concordia-replay` evaluates assertions over a running or replayed world.

See `AGENTS.md` for invariants, tests and references.
