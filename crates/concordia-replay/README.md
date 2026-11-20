# concordia-replay

Replay v0 (M2 phase 2.8, ADR-0072): the `replay` engine module, enabled by `[replay]` in `concordia.toml`. `replay.record` records a play session from the open scene into a canonical `.replay.ron` file: a header naming the engine, scene, seed and script builds, the action states that changed on each tick, and checkpoint hashes. `replay.run` replays it headless on a fresh play world and reports the first checkpoint that diverges, naming the parts (components, clock, RNG) that differ, or new hashes in `resimulate` mode. CLI: `concordia replay record|run`.

See `AGENTS.md` for invariants, tests and references.
