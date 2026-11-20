# ADR-0032: Edit world vs play world
- Status: Accepted
- Date: 2026-11-17

## Context
Scenes are the source of truth and every change to them is an attributed, undoable transaction. Simulation (physics, scripts, AI) changes state every tick; recording that in the command journal would bury edits in noise and make the saved scene depend on how long someone pressed play.

## Decision
- The **edit world** is the open scene. Only `concordia-command` transactions change it, and they are journaled.
- The **play world** is a clone of the edit world, made on demand by `sim.play` or `sim.step`, and discarded by `sim.stop` or by opening or creating a scene. Simulation runs only there, on the fixed-step `SimLoop` with a seeded `SimRng`.
- Read methods take `world: "edit" | "play"` (default `edit`); reading `play` without a play world is `NotPlaying`.
- In M0 the play world cannot be edited through the API (that arrives with `input.inject` and replays).
- `sim.step` returns the play world's state hash, identical across thread counts on the same machine and binary.

## Consequences
Saving never captures simulation state; "press play, experiment, stop" leaves the scene exactly as it was. Restarting play always starts from the current edit world. Cloning costs memory proportional to the scene, which is fine at M0 scale; copy-on-write can come later if needed.

## Alternatives considered
- Simulate in place and snapshot/restore on stop: easy to leak sim state into saves, and undo history interleaves with simulation.
- Journal simulation changes: enormous journals, and undo becomes meaningless.

## Update (M2 phase 2.8, ADR-0072)
Injected input and play outcomes are now recorded by `concordia-replay` (`replay.record`, `replay.run`), never the command journal. Both start a fresh play world from the edit world, which they never change.
