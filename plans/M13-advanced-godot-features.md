# M13: Advanced Godot features
Status: Planned (ADR-0017) · Depends on: M2 (animation graphs 2.4.3, `anim.*` tools 2.4.4, the animator role 2.4.5, `SimRng` and replay 2.8); M5's AnimationTree import is the main consumer · Detail level: **Outline; needs refinement after M2 2.4** · Estimated effort: ~5 weeks (range 4–7), author plus agents

> Outline plan with phase breakdowns and test lists. Refine in phase 13.0, once 2.4.4 and 2.4.5 have landed: reconcile with `concordia-anim` and the `.animgraph.ron` format as built, settle the open questions, and set the gate's fixtures.

## Goal & context
M2 2.4.3 built native blend graphs and state machines (`.animgraph.ron`) with Godot 4's `AnimationTree` semantics as the compatibility target, and deliberately left out the Godot features that nothing in M2 needed (M2 plan, 2.4.3 As built):
- `AnimationNodeOneShot`: the **abort** and **fade-out** requests, **autorestart** (with its delay and random delay), and the **additive** mix mode;
- `AnimationNodeTransition`: per-input **auto-advance**, **transition to self**, and per-input **break loop at end**;
- `AnimationNodeStateMachineTransition`: **advance expressions** and **break loop at end** (2.4.3 treats every wrap of a looping state as its end);
- **`Curve` resources** for fades and cross-fades (2.4.3 has `Linear` and `Smooth` only).

M13 adds them, so a Godot project's animation trees (M5's AnimationTree import, and the reference port's characters) map onto `.animgraph.ron` without "unmapped" reports for these features. Agents author them as text, as they do the rest of the graph.

**Clean room.** Godot is MIT-licensed open source: its documentation and source may be read for semantics (ADR-0006, `docs/clean-room.md`). We still implement from the semantics and write our own code; each crate's `AGENTS.md` records the Godot classes and files read. Running a Godot 4 binary headless to produce oracle values (phase 13.0) is allowed.

## Scope / Non-goals
**Scope**
- The features above, with Godot 4's documented behavior, in `.animgraph.ron` (format version 2) and `concordia-anim`'s evaluator.
- Validation for every new field, errors naming the node (and transition index or expression column).
- Determinism: everything at the fixed step in `FrameEnd`; random restart delays derived from hashed state, never from thread order.
- Docs: the agent guide, the animator skill (`concordia-animation-graphs`, 2.4.5), crate `AGENTS.md`, editor schemas.

**Non-goals**
- Godot's A* travel weighted by editor graph positions (our text has no positions; 2.4.3 chose fewest transitions with deterministic ties).
- Godot nodes 2.4.3 never had (blend spaces, `Add2`/`Add3`/`Sub2`, `TimeSeek`, `Sync`, end states, nested and grouped machine types, `start`/`stop`/`next` playback calls): see open question 2.
- A general scripting language in advance expressions (ADR-0280 fixes a small grammar).
- The Godot importer itself (M5).

## Dependencies (what must already exist, from which milestone)
- M2 2.4.3: `AnimGraph`, `read_animgraph` / `write_animgraph`, `AnimGraphImporter` v1, `BoundGraph`, `AnimGraphPlayer`, `AnimGraphState`, `anim.sample` in `FrameEnd`, play-world writes (ADR-0075).
- M2 2.4.4: `anim.inspect` (to report read-only one-shot outputs such as `active`), `anim.set_param` / `anim.travel`.
- M2 2.4.5: the animator role and its graph skill, which this milestone extends.
- M2 2.0 / 2.8: `SimRng` and replay checkpoints (the autorestart delay's randomness must replay).

## Crates, modules & directories (created / touched)
- `crates/concordia-assets`: `anim_graph.rs` (new fields, `Curve`, the expression AST and its parser, validation), `AnimGraphImporter` version 2.
- `crates/concordia-anim`: `graph.rs` and `graph_eval.rs` (the new behavior), `graph_player.rs` (`NodeState` fields: restart countdown, transition-to-self request, loop counts), tests.
- `tools/godot-oracle/` (new, optional): a Godot 4 headless project that plays fixture graphs and writes sampled values to committed JSON; CI never needs Godot.
- `tests/fixtures/anim/`, `tests/gates/m13_*.rs`, `docs/guides/agents-engine-api.md`, the Studio animator skill, `editor/ui` generated schemas.

## Phases
### Phase 13.0: Refinement and the Godot oracle
- **Objective:** reconcile with 2.4.3–2.4.5 as built, settle the open questions, and pin Godot's behavior where its documentation is ambiguous.
- **Deliverables:** the refinement in this plan; `tools/godot-oracle` (a Godot 4.x headless project and a `just godot-oracle` recipe) producing `tests/fixtures/anim/oracle/*.json` for each feature's fixture graph; the test 2.4.3 left open for playback pass-through (`Blend2` reports `b`'s playback above 0.5, `BlendN` its heaviest input's).
- **Tests first:** `blend_playback_passes_through_like_godot` (an `AtEnd` transition over a `Blend2` fires when the heavier input ends).
- **Done criteria:** each later phase has oracle values or hand-computed values for its tests; open questions answered or deferred in writing.

### Phase 13.1: Unit curves
- **Objective:** Godot `Curve` resources for one-shot fades, transition-node cross-fades and state machine cross-fades.
- **Deliverables:** `FadeCurve::Curve(points)` (each point: `x`, `y`, left and right tangents) evaluated as Godot's cubic Hermite between points, holding the end values outside them; validation (x in 0..=1 and strictly increasing, finite values, at least one point); `Linear` and `Smooth` unchanged.
- **Tests first:** `unit_curve_matches_hermite_between_points` (hand-computed values at four progress points, unequal spacing), `curve_holds_outside_its_points`, `curve_fades_match_godot_oracle`, `bad_curves_are_errors_naming_the_node`, `animgraph_v2_roundtrips_canonical` (insta, reviewed).
- **Implementation notes:** curves are inline in the node in M13 (open question 3: shared `.curve.ron` assets).

### Phase 13.2: One-shot requests and autorestart
- **Objective:** Godot's full one-shot request set and its autorestart.
- **Deliverables:** optional `abort` and `fade_out_request` trigger parameters on `OneShot` (abort stops the shot now; fade-out fades it over `fade_out` from now); `autorestart`, `autorestart_delay` and `autorestart_random_delay` (the shot replays after the delay plus a random extra; abort cancels the pending restart, a later fire restarts it); `NodeState` keeps the countdown.
- **Tests first:** `abort_stops_the_shot_at_once`, `fade_out_request_fades_from_now_over_fade_out`, `autorestart_replays_after_its_delay`, `abort_cancels_a_pending_restart_but_fire_resumes_it`, `random_restart_delay_is_deterministic_across_thread_counts_and_replays`.
- **Implementation notes:** the random extra is a hash of (the play world's `SimRng` seed, entity id, node index, fire count) mapped into 0..=`autorestart_random_delay`, so it never depends on evaluation order and replays exactly (ADR-0063).

### Phase 13.3: Additive mix mode
- **Objective:** `OneShot`'s `mix: Add` (Godot's `MIX_MODE_ADD`, "similar to AnimationNodeAdd2").
- **Deliverables:** an additive blend in `concordia-anim`: the shot's difference from the rest pose (translation offset, rotation `rest⁻¹ · shot`, scale ratio), weighted and applied on top of the base pose, respecting the filter.
- **Tests first:** `additive_one_shot_adds_the_shots_offset_to_the_base` (hand-computed translation, rotation and scale), `additive_at_zero_weight_is_the_base`, `additive_matches_godot_oracle`.
- **Implementation notes:** Godot computes additive animation against the animation's own reset pose; 13.0 confirms whether the rest pose is the right reference for imported clips.

### Phase 13.4: Transition node extras
- **Objective:** Godot `AnimationNodeTransition`'s per-input auto-advance and break-loop, and transition to self.
- **Deliverables:** `TransitionInput::auto_advance` (after the input plays once, move to the next, wrapping after the last) and `break_loop_at_end`; `allow_transition_to_self` with an optional `request` trigger (an int parameter can't signal "the same value again"; firing the trigger restarts the current input when it resets).
- **Tests first:** `auto_advance_moves_to_the_next_input_after_one_play_and_wraps`, `break_loop_at_end_ends_a_looping_input_for_auto_advance`, `transition_to_self_restarts_only_when_allowed`.

### Phase 13.5: State machine extras
- **Objective:** advance expressions and break-loop-at-end on state machine transitions.
- **Deliverables:**
  - `StateTransition::expression`: a condition over graph parameters in the small grammar ADR-0280 fixes (literals, parameter names, `+ - * /`, comparisons, `&& || !`, parentheses), parsed and type-checked at import, with errors naming the node, the transition and the column. An `Auto` transition needs its `condition` and its `expression` (when present) to hold, as in Godot.
  - `StateTransition::break_loop_at_end`: an `AtEnd` transition from a looping state fires at the end of a loop only when set (Godot's behavior; see open question 4 for 2.4.3 files).
- **Tests first:** `advance_expression_gates_an_auto_transition`, `expressions_are_type_checked_at_import` (unknown parameter, bool-number mismatch, a syntax error with its column), `expression_evaluation_is_exact_and_deterministic` (integer and float rules fixed), `break_loop_at_end_waits_for_the_loop_end_only_when_set`.

### Phase 13.6: Docs, Studio and the gate
- **Objective:** agents can use every new feature, and the gate proves it.
- **Deliverables:** the agent guide's graph section, the animator skill's examples, crate `AGENTS.md` (references: the Godot classes and files read), editor schemas, the gate tests and fixtures.
- **Tests first:** the gate (below); the animator skill snapshot (reviewed).

## Milestone gate
`just gate M13` runs `tests/gates/m13_godot_graphs.rs` on original fixtures in `tests/fixtures/anim/m13/` (our own rig from the Blender fixture script, CC0; no existing game's characters or graphs). It asserts:
1. **Format:** every fixture graph round-trips canonically at version 2, and every 2.4.3 (version 1) file reads with unchanged behavior.
2. **Semantics:** for each feature's fixture, sampled poses at fixed ticks match the Godot oracle JSON (or hand-computed values) within 1e-4.
3. **Determinism:** a 100-character crowd using autorestart with random delays, expressions and auto-advance has identical hashes at 1, 3 and 8 threads, and `replay.run` of its recording passes.
4. **Errors:** each invalid fixture's import error names its node (and transition and column for expressions).
5. **Agents:** an MCP session imports a graph using every new feature, plays it with `sim.step`, and reads the expected states through `anim.inspect`.

## Risks & mitigations
- **Godot's documentation leaves behavior open** (fade timing on abort, additive reference pose, loop ends). *Mitigation:* the headless oracle in 13.0, and its values committed.
- **The expression grammar grows into a scripting language.** *Mitigation:* ADR-0280 fixes the grammar; anything more belongs in gameplay scripts that set parameters.
- **Changed defaults alter existing graphs.** *Mitigation:* version 1 files keep 2.4.3 behavior (open question 4), and the gate checks it.
- **Randomness breaks replays.** *Mitigation:* derived from hashed state, never from evaluation order; tested across thread counts and replays.

## Open questions
1. **Ordering.** M13 is numbered after M12 but is small, and M5's AnimationTree import needs it to avoid unmapped features. *Recommendation:* run it before M5's AnimationTree phase.
2. **Wider scope.** Godot nodes 2.4.3 never had: blend spaces (1D, 2D), `Add2` / `Add3` / `Sub2`, `TimeSeek`, `Sync`, end states, nested and grouped machine types, and `start` / `stop` / `next` on playback. Should M13 take any of them? *Recommendation:* decide in 13.0 from the reference port's trees: take what they use, backlog the rest.
3. **Shared curves.** Inline curves only, or also `.curve.ron` assets shared between graphs? *Recommendation:* inline in M13; assets only if fixtures repeat curves.
4. **Loop ends in version 1 files.** 2.4.3 ends a looping state at every wrap, which is Godot's `break_loop_at_end = true`. *Recommendation:* version 1 files read with `break_loop_at_end: true` so they keep their behavior, version 2 files default to Godot's `false`, and the M5 importer writes the field explicitly.

## ADRs to write
- ADR-0017: the milestone (written with this plan, Proposed).
- ADR-0280: the advance-expression grammar, its types and evaluation rules (phase 13.5).
- ADR-0281: `.animgraph.ron` version 2 and how version 1 files keep their behavior (phase 13.1, if open question 4 changes ADR-0074's contract).

## Effort estimate
| Phase | Estimate |
|---|---|
| 13.0 Refinement and oracle | 0.5–1 week |
| 13.1 Unit curves | 0.5 week |
| 13.2 One-shot requests and autorestart | 1 week |
| 13.3 Additive mix mode | 0.5–1 week |
| 13.4 Transition node extras | 0.5 week |
| 13.5 State machine extras | 1–1.5 weeks |
| 13.6 Docs, Studio and the gate | 0.5–1 week |
| **Total** | **~5 weeks (range 4–7)** |

## References (appendix)
- Godot Engine documentation (MIT): `AnimationTree`, `AnimationNodeOneShot`, `AnimationNodeTransition`, `AnimationNodeStateMachineTransition`, `AnimationNodeStateMachinePlayback`, `AnimationNodeAdd2`, `Curve`, `Expression`.
- Godot Engine source (MIT), for semantics where the docs are silent: `scene/animation/animation_blend_tree.cpp`, `animation_node_state_machine.cpp`, `scene/resources/curve.cpp`; each file read is recorded in the crate's `AGENTS.md`.
- Jason Gregory, *Game Engine Architecture* (3rd ed.), ch. 12: additive blending.
