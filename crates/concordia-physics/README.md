# concordia-physics

Deterministic rigid-body physics for Concordia over Jolt (through `jolt-sys`): bodies and colliders as components, a physics runtime per play session, raycasts, character controllers, and contact and sensor events as Terra signals. A character also carries an inner body that only sensors see, so sensors report characters like any body.

See `AGENTS.md` for invariants, tests and references.
