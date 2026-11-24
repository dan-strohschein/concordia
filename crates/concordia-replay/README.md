# concordia-replay

Replay (M2 phase 2.8 and M3 phase 3.1, ADR-0072): the `replay` engine module, enabled by `[replay]` in `concordia.toml`. `replay.record` records a play session from the open scene into a canonical `.replay.ron` file: a header naming the engine, platform, manifest, scene, seed, script builds, input map and templates, the action states that changed on each tick, checkpoint hashes and every tick's hash. `replay.run` replays it headless on a fresh play world and reports the exact first tick that diverges, naming the parts (components, clock, RNG) that differ, or new hashes in `resimulate` mode. CLI: `concordia replay record|run|migrate`.

See `AGENTS.md` for invariants, tests and references.
