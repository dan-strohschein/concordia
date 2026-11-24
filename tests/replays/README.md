Replay files (`.replay.ron`) used by determinism and playtest tests. From M2.

- `m1-physics.replay.ron` (M3 3.1b): 300 ticks of the M1 physics stack, recorded without stills. `committed_m1_physics_replay_verifies_on_its_platform` verifies it tick for tick on the platform in its header (at 1 and 8 threads) and replays it to the end elsewhere. After an intended change to physics, scheduling or the scene, re-bless it with `cargo nextest run -p concordia-tests --run-ignored only -E 'test(rebless_the_committed_m1_physics_replay)'` and review the diff. Never re-bless in CI.
