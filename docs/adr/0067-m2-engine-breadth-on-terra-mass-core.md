# ADR-0067: M2 is engine breadth on Terra's Mass core, with Substrate-style materials
- Status: Accepted (author's decisions, M2 refinement, 2026-11-19)
- Date: 2026-11-19
- Builds on: ADR-0010 (Terra), ADR-0048 (engine breadth before the port), ADR-0049 (agent-first, Studio early); amends ADR-0010's timing

## Context
ADR-0048 renamed M2 "Engine breadth" and moved the reference port to M5, but left M2's restructuring to its refinement. The refinement (against M0, M1 and the M3 Studio track as built) found these problems:
- M2 had no gate, because its gate was the private port slice.
- Phases 2.3–2.7 held Godot mappings that sit on the importer that moved.
- Terra lacks the processing phases, events, timers and time scale that every M2 subsystem needs.
- The author wants gameplay built on a framework as close to Unreal's Mass as possible, and materials like Unreal's Substrate, but no equivalent of Unreal's Gameplay Ability System.

## Decision
1. **Terra's Mass-style core moves from M7 into M2**, as M2's first phases (2.0, 2.0b, 2.0c and later 2.4b). It covers:
   - processors with access declarations and processing phases, scheduled in parallel and deterministically;
   - shared and chunk components, tags and command buffers;
   - templates and traits (`.template.ron`);
   - simulation and representation LOD;
   - signals and observers;
   - spawners and representation switching.

   Crowds and traffic join M2's navigation phase. M7 keeps large-scale performance, AI-driven behavior (after M4) and its gate.
2. **M2's subsystems are Terra processors:** scripting, animation, navigation, audio and in-game UI. Gameplay scripts (native tier 1, WASM tier 2, ADR-0003) run as processors in Terra's phases. A Gameplay Ability System (abilities, effects and attribute sets) is a non-goal.
3. **Materials follow a Substrate-style model:** slabs with BSDF features, vertical layering, horizontal mixing and coverage operators, and a per-pixel closure budget that downgrades gracefully. They are defined as text (`.material.ron`) first, and graph authoring waits for M4. The model replaces M1's PBR material, with a migration. Its details are ADR-0068, written in phase 2.3.
4. **Godot-specific work moves to M5:** the gdshader translator, Godot import and material mapping, AnimationTree import, node mappings, `Control` import and sound banks. So do `concordia-interchange` v0 and ADRs 0064–0065.
5. **M2's gate is an original sample game** in `examples/` that uses every M2 subsystem. It runs headless in public CI, with an optional live Studio gate.
6. **Phase order unblocks dependents first:** Terra core, then scripting, replay, materials and FBX, animation, LOD and representation, navigation, audio and UI, and finally the sample game.
7. **Clean room:** Mass and Substrate are implemented from Epic's public documentation and talks only (the Mass docs, the Substrate docs and the SIGGRAPH 2023 Substrate course material), never from Unreal source (`docs/clean-room.md`). The references are recorded in the crates' `AGENTS.md`.

## Alternatives
- **Keep Mass in M7 and build only Mass-shaped foundations in M2.** It's less work now, but gameplay subsystems would be built twice: once on simple systems, and again on processors and LOD.
- **Fold all of M7 into M2.** That would make M2 the largest milestone, and M7's AI-driven crowds need M4 anyway.
- **Build Substrate-style materials in M4.** It sits beside GI and the path-traced oracle, but every M2 asset would be authored for a material model that is about to change.
- **Per-subsystem gates instead of a sample game.** Nothing would prove that the subsystems work together.

## Consequences
- M2 grows to about 29–36 phase-weeks, and M7 shrinks to about 4–6 weeks.
- `concordia-terra-sim` and `concordia-terra-crowd` become M2 crates. `concordia-interchange` and `concordia-import-godot` become M5 crates.
- M1 material files migrate once, in 2.3.
- The disabled Studio roles become enabled phase by phase, as their features land.
