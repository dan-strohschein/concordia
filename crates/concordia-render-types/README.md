# concordia-render-types

The render vocabulary shared by Concordia's GPU renderer and its reference path tracer: the reflected render components (`Camera`, lights, `Environment`), camera math, photometric units and the CPU tone-mapping references. It has no GPU dependencies, so CPU-only code can use it. `concordia-render` re-exports all of it under its old paths.

See `AGENTS.md` for invariants, tests and references.
