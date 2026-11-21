# ADR-0016: Rigging milestone (M12): procedural rigs and in-engine keyframing
- Status: Proposed (author's direction, 2026-11-20)
- Date: 2026-11-20
- Builds on: ADR-0009 (canonical RON), ADR-0032 (edit world vs play world), ADR-0061 (native tier), ADR-0062 (script protocol, named access), ADR-0063 (determinism and presentation state), ADR-0068 (flat named-node models), ADR-0072 (replay), ADR-0073 (per-role tool budgets); relates to ADR-0015 (declarative definitions instead of a graph editor)

## Context
M2 2.4 plans skeletons, clips, blending and state machines, and leaves IK and retargeting out. Nothing can change a pose procedurally, and every clip must be authored outside the engine (the Blender bridge, M3 3.7). The backlog lists full-body IK "after M2".

Unreal's Control Rig is the reference: "a node-based rigging system that lets animators create procedural rigs and animate characters directly inside the engine without round-tripping to external 3D software". Its rigs are graphs of rig units with controls, forward and backward solves and a setup event, and its controls are keyed in Sequencer and baked to animation. Concordia's users work mostly through AI agents. Agents edit named nodes in text more reliably than visual graphs or nested trees, and reflection has no recursive types (ADR-0068). M11 (ADR-0015) set the precedent of keeping a system's model and dropping its graph editor.

## Decision
1. **A new milestone, M12 "Rigging v1.0"** (`plans/M12-rigging.md`), is added to `docs/design.md`'s milestone list.
2. **Keep Control Rig's model, express it as text.** A rig (`.rig.ron`, canonical RON, schema from `concordia-reflect`) is a flat map of named units with controls and parameters. Inputs name other units' outputs, controls, bones, parameters or literals; those references are the graph. Setup, forward and backward events are ordered lists of mutating units; pure units are pulled by their users.
3. **No visual rig editor in M12.** A view of `.rig.ron` in M4's graph panel may follow M12's gate; it edits the same file through `rig.apply`. There is no `.graph.ron` rig kind.
4. **A compiled program, evaluated on the CPU.** A rig binds to a skeleton once and compiles to an instruction list over slots. Evaluation allocates nothing, iterates in a fixed order and is independent per entity, so results don't depend on thread count. Solvers have fixed tolerances and iteration caps and never return NaN.
5. **Built-in units:** controls, get and set transform, math, two-bone IK, FABRIK, CCD, aim, look-at, constraints and a ground probe. Custom units are Rust types implementing `RigUnit`, registered by engine modules or Evolve plugins. WASM units are deferred.
6. **Two modes.** A Simulation rig runs in `PostPhysics`, writes the hashed pose, is fingerprinted for replays and may feed gameplay. A Presentation rig runs in `Presentation`, writes presentation-only state and is excluded from the world hash (ADR-0063 rule 4). Presentation is the default. The edit world evaluates rigs only for display.
7. **Animating in the engine.** Clips (`.clip.ron`) hold keyed control curves at integer frames and sampled bone tracks. `anim.key_set`, `anim.key_get`, `anim.key_delete` and `anim.preview` key and inspect them; `rig.bake` bakes to bones or, by the backward solve, to controls; a pose library (`.poselib.ron`) saves, applies and mirrors poses.
8. **Edits are transactions.** `concordia-command` gains document operations, so rig, clip and pose-library edits are attributed and undoable, and share transactions with entity edits. M4 4.10's `graph.apply` needs the same operations; whichever milestone lands first adds them.
9. **Baking is import.** A control clip imports to a skeleton clip in the DDC by evaluating its rig at every frame. Shipping games play baked clips with no rig cost.
10. **Viewport, not panels.** Controls are engine-drawn shapes, picked and dragged through `viewport.pick` and `viewport.gizmo`, each drag one transaction, with auto-key at the preview frame. The inspector gains a `RigInstance` widget. There is no timeline panel in M12.
11. **Studio:** the animator role gets the `rig_*` and `anim_key_*` tools in an explicit scope under the 22k budget (ADR-0073), and a `concordia-rig-and-keyframe` skill.
12. **Clean room:** Control Rig concepts come only from Epic's public documentation and talks; solvers come from open papers (FABRIK, CCD, analytic two-bone, swing-twist). No Unreal source is read.
13. **ADR block:** M12 draws ADR numbers from 0260–0279.

## Consequences
- A proposed GPU-free crate, `concordia-rig`, with the `rig` engine module (`[rig]`). `concordia-anim` gains the `.clip.ron` text model, curves and a `PoseProcessor` seam, and never depends on `concordia-rig`.
- New Engine API methods `rig.validate`, `rig.apply`, `rig.evaluate`, `rig.bake`, `rig.pose_save`, `rig.pose_apply`, and `anim.key_set`, `anim.key_get`, `anim.key_delete`, `anim.preview`. Projects without the modules keep their tool set.
- M2 2.4 should leave the seam (`PoseProcessor`) and name-addressed channels; M4 4.10 and M12 share one document operation; M5 may map Godot's skeleton modifiers to rig units; M8 garments follow the final presented pose.
- Full-body IK, retargeting, stateful units, a cinematic sequencer and GPU evaluation stay deferred.

## Alternatives considered
- **A visual node-graph editor first, as in Unreal.** The most familiar to humans, the most costly, and the least useful to agents, which write text directly.
- **Rigs as an M4 `.graph.ron` kind compiled to WASM.** It ties rigging to M4's compiler and schedule, and a WASM call per unit per bone costs more than a compiled slot program.
- **Rigs as nested trees.** Reflection has no recursive types, and agents edit flat named nodes more reliably (ADR-0068).
- **Rig logic as gameplay scripts.** Scripts can already drive controls by name; writing solvers in scripts would duplicate them per project and lose the shared tests.
- **Keyframe edits as plain file writes, as `material.set` does.** Simpler, but animation is edited key by key, and without undo every mistake is a manual repair.
