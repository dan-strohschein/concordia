# ADR-0075: Play-world writes through component.set and component.patch
- Status: Accepted (M2 phase 2.4.3, 2026-11-21)
- Date: 2026-11-21
- Amends: ADR-0032 (edit world vs play world: "the play world cannot be edited through the API")
- Builds on: ADR-0063 (determinism contract), ADR-0072 (replay v0), ADR-0074 (animation runtime)

## Context
Animation graphs (2.4.3) are driven by parameters on an `AnimGraphPlayer`: a float for speed, a bool for "moving", a trigger for "attack", a travel request for a state machine. Gameplay sets them inside the simulation. Agents testing a character need to set them in a running play world too. Until now the only play-world writes were `input.inject` and `terra.spawn_batch`. `component.set` and `component.patch` changed only the edit world, through journaled transactions, so a running game couldn't be steered without a dedicated method per component. The author chose a general door over one per feature (M2 plan, decision B1, 2026-11-21).

## Decision
- `component.set` and `component.patch` take an optional `world: "edit" | "play"` (default `edit`, unchanged).
- With `"play"`, the value is validated against the component's schema, like any edit, and written straight into the play world. Without a play world, the call fails with `NotPlaying` (as reads do); it doesn't start one.
- Play writes aren't journaled or undoable, and never touch the scene. Each is logged at info (`concordia::api`, "play world changed") with the actor, method, entity and component, so `log.tail` attributes it.
- A write lands between ticks, so the next tick sees it. Inside the simulation, gameplay still sets components in its own phase (a `PostPhysics` parameter is seen by `FrameEnd` animation the same tick, ADR-0074).

## Consequences
- Agents can tune a running game (graph parameters, a light's color, a velocity) and read the result with `"world": "play"`. 2.4.4's planned `anim.set_param` and `anim.travel` become conveniences over this, or aren't needed.
- **Replay v0 doesn't record these writes.** It records input changes and checkpoints (ADR-0072), so a recording that uses play writes fails its first checkpoint after the write (`replay.run` reports the divergence). Recording them is a replay change for later; until then, drive recorded sessions with `input.inject` and gameplay.
- The MCP `component_set` and `component_patch` tools gain the `world` field. Their schemas are slightly larger, still under the role budgets (ADR-0073).
- Hashes stay deterministic: a play write is an input between ticks, like `input.inject`.

## Alternatives considered
- **One method per feature** (`anim.set_param`, `anim.travel`, …). They are small and discoverable, but every system that wants play-time tuning would need its own, and agents would learn many doors for one idea.
- **Edit the edit world and restart play.** The scene stays the only source, but every tweak would lose the play state being tested, and parameters aren't scene content.
- **Journal play writes.** Undo would cover them, but ADR-0032 keeps simulation out of the journal, and undoing a write the simulation has since built on has no meaning.
