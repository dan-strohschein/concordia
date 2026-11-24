# concordia-replay

Replay (M2 phase 2.8 and M3 phase 3.1, ADR-0072): the `replay` engine module, enabled by `[replay]` in `concordia.toml`. `replay.record` records a play session from the open scene into a canonical `.replay.ron` file: a header naming the engine, platform, manifest, scene, seed, script builds, input map and templates, the action states that changed on each tick, checkpoint hashes and every tick's hash. `replay.run` replays it headless on a fresh play world and reports the exact first tick that diverges, naming the parts (components, clock, RNG) that differ, with the values that differ (from stills kept in the derived-data cache), or new hashes in `resimulate` mode, which can also rewrite the file. An `ab` run replays a file on two builds of the game side by side and names the system after which they first differ. CLI: `concordia replay record|run|migrate|diff`.

See `AGENTS.md` for invariants, tests and references.
