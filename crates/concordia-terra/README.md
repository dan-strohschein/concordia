# concordia-terra

**Terra**, Concordia's entity system (ADR-0010): an archetype ECS with stable entity ids, typed and by-name queries, schedules, a deterministic fixed-step loop and world hashing. Safe Rust (ADR-0030). Since M2 it has Mass-style processors in phases (ADR-0063) and shared, chunk and tag components, command buffers, signals and observers (ADR-0069), and simulation LOD: far entities tick less often, staggered, with exact elapsed time (ADR-0076); large-scale simulation arrives in M7.

See `AGENTS.md` for invariants, tests and references.
