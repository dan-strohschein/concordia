# M12: Rigging v1.0
Status: Planned (ADR-0016) · Depends on: M2 (animation 2.4, phases and presentation state 2.0, signals 2.0b, FBX skins 2.3d, scripts 2.6, replay 2.8); M1 (gizmos, overlays, picking, the inspector, `capture.screenshot`, goldens); M3 Studio (roster, `studio sync`, per-role budgets); M4 4.10 document transactions reused if it lands first · Detail level: **Outline; needs refinement after M2 2.4** · Estimated effort: ~22 weeks (range 17–28), author plus agents

> Outline plan with phase breakdowns and test lists. Refine in phase 12.0, once `concordia-anim` (M2 2.4) exists: reconcile with its skeleton, pose and clip types as built, measure rig evaluation on the reference machine, fix the gate budgets, and settle the open questions.

## Goal & context
The engine will play clips and blend them (M2 2.4), but it can't change a pose procedurally, and nothing can be animated without a round trip to Blender. M2 left IK and retargeting out on purpose (M2 non-goals; risks: "no IK/retargeting"). The backlog lists full-body IK "after M2, realistically after M4".

**Rigging v1.0** is a procedural rig runtime and an in-engine keyframing loop, in the style of Unreal's Control Rig, adapted to this engine. Control Rig is "a node-based rigging system that lets animators create procedural rigs and animate characters directly inside the engine without round-tripping to external 3D software." What stays:
- **Rigs** of named **units** (Control Rig's nodes): controls, get and set transform, IK solvers (two-bone, FABRIK, CCD), aim and look-at, constraints, and math.
- **Events:** a *setup* event that runs once when a rig binds to a skeleton, a *forward* solve (controls → bones) every evaluation, and an optional *backward* solve (bones → controls).
- **Controls** with shapes that the viewport draws and drags.
- **Animating in the engine:** keys and curves on controls, saved as clips, baked to skeleton clips, with a pose library.

**Where "node-based" goes.** In this engine a rig is a **named node graph in text**, not a visual graph:
- A `.rig.ron` file holds a flat map of named units. A unit's inputs name other units' outputs, controls, bones, parameters or literals. Those references are the graph's edges.
- The order of side effects is an ordered list of unit names per event, like Control Rig's execution chain. Pure units (math, get transform) are pulled by the units that use them.
- This follows ADR-0068 and M2's D5 ("node-graph authoring waits for M4's graphs"; a flat map of named nodes, because reflection has no recursive types and agents edit named nodes more reliably than nesting) and ADR-0015 (M11 dropped its graph editor for declarative definitions).
- A visual view comes only after M4's graph panel (4.11), as a view of the same `.rig.ron` file, never a second format (open question 1).

**Five architectural choices** (ADR-0016):
1. **Text first.** Rigs (`.rig.ron`), clips (`.clip.ron`) and pose libraries (`.poselib.ron`) are canonical RON, with schemas from `concordia-reflect`.
2. **A compiled rig program.** A rig binds to a skeleton once and compiles to a flat list of instructions over slots (bones, controls, values), like ADR-0068's `MaterialProgram`. Evaluation allocates nothing and has no hash maps in the hot loop.
3. **Two modes, one evaluator.** A *Simulation* rig runs in `PostPhysics`, writes the hashed pose, and is deterministic and replayed. A *Presentation* rig runs in `Presentation`, writes presentation-only state, and is excluded from the world hash (ADR-0063 rule 4).
4. **Agent-driven animation.** `rig.*` and `anim.key_*` methods edit rigs and clips. Each edit is an attributed, undoable transaction in `concordia-command`, which gains document operations for text assets (shared with M4 4.10's `graph.apply`).
5. **Baking is import.** A control clip imports to a skeleton clip in the DDC by baking its rig at every frame. Shipping games play baked clips with no rig cost. Live rigs remain for procedural work such as foot placement and aiming.

**Left out on purpose:** a visual rig editor, a timeline or dope-sheet panel (open question 12), full-body IK, retargeting (open question 13), physics-driven secondary motion, stateful units such as springs (open question 14), a cinematic sequencer (backlog), and GPU rig evaluation (open question 18).

**What the code gives us today:**
- `concordia-anim` **does not exist yet.** M2 2.4 plans it: skeletons, clips, sampling, CPU and GPU skinning, `.animgraph.ron` blend trees and state machines, events, `BoneAttachment`, tweens, and a skeleton debug view (D7). This plan depends on that plan and names its types provisionally. Phase 12.0 renames them to what 2.4 builds.
- `concordia-assets`: `SkinData { joint_names, joints, inverse_bind }`, `ClipData { name, duration, channels }`, `Channel { target: EntityId, path: ChannelPath, interpolation, times, values }`, `ChannelPath::{Translation, Rotation, Scale, Weights}`, `Interpolation::{Linear, Step, CubicSpline}`, and `AssetKind::{Skin, Clip}`. These are binary DDC artifacts. There is **no text clip format**, and channels target joint entity ids, not bone names.
- `concordia-import-gltf` `anim.rs` stores skins (`skin/<index>`) and clips (`clip/<name>`) "for M2's animation system". FBX skins and clips come in 2.3d through the same types.
- `concordia-command`: `EditOp::{Spawn, Despawn, SetComponent, RemoveComponent, MigrateType}`. Transactions change only the edit world's entities. There are **no document operations**; M4 4.10's `graph.apply` plans to add undoable file edits, and 2.3c's `material.set` is attributed but not undoable.
- `concordia-render`: `viewport.gizmo` (move, rotate and scale drags, each one transaction), `viewport.overlays` (selection, gizmo, grid, view mode), `viewport.pick`, `GizmoMode`, `GizmoAxis`, and `capture.screenshot { view_mode, overlays, world }`.
- Terra: phases `PrePhysics`, `Physics`, `PostPhysics`, `FrameEnd`, `Presentation`; processors with declared access; presentation-only components (ADR-0063); end-of-tick hooks and per-part hashes (ADR-0072); named component access for script modules (ADR-0062).
- Studio: the `animator` role is disabled ("animation arrives in M2"), with 10 tools and write paths `assets/anim/**` and `assets/blender/**`. M2 2.4 enables it (D8); M12 widens its scope.

**Clean room.** Control Rig is part of Unreal Engine, which is source-available, not open source. M12 uses only Epic's public Control Rig documentation and public talks, for concepts and vocabulary (rig units, controls, forward and backward solve, setup event, baking). Solvers come from open papers (see References). No Unreal source may be read, ported or paraphrased (`docs/clean-room.md`). `concordia-rig`'s `AGENTS.md` lists its references.

## Scope / Non-goals
**Scope**
- The `.rig.ron` format: controls, parameters, units and events, validated with errors an agent can act on.
- A rig compiler and a CPU evaluator: bind to a skeleton, schedule, evaluate in a fixed order, deterministic, allocation-free.
- Built-in units: controls, get and set transform, math, two-bone IK, FABRIK, CCD, aim, look-at, constraints (parent, position, rotation, scale, limit rotation, distribute rotation) and a ground probe.
- Setup, forward and backward events, with round-trip tests.
- Runtime integration: the `RigInstance` component, Simulation and Presentation modes, a written determinism and replay policy, gameplay driving controls by name.
- Custom units in Rust through a `RigUnit` trait (native modules and Evolve plugins).
- Document transactions in `concordia-command` for `.rig.ron`, `.clip.ron` and `.poselib.ron`.
- The `.clip.ron` format: keyed control curves and sampled bone tracks; keyframe methods; preview; baking at import; `rig.bake` to bones and to controls.
- A pose library with mirroring.
- Engine-drawn control gizmos, a rig overlay, inspector widgets, and auto-key on drag.
- The animator role's scope and skill.

**Non-goals (deferred; see `later-backlog.md`)**
- A visual rig graph editor, a timeline, dope sheet or curve editor panel.
- Full-body IK, motion matching, physics-based animation (backlog item 12).
- Retargeting between different skeletons, including humanoid retargeting (M5 open question 10).
- Stateful units: springs, verlet chains, secondary motion.
- A cinematic sequencer (multi-entity tracks, cameras, audio).
- GPU rig evaluation.
- Importing Control Rig assets (M5's non-goals keep them out) or exporting clips to glTF or FBX (the Blender bridge exports).
- Blend shapes driven by rig units (2.4's morph weights stay clip channels).

## Dependencies (what must already exist, from which milestone)
| Needed | From |
|---|---|
| Skeletons, poses, clip sampling, skinning, `.animgraph.ron`, the anim engine module, the skeleton debug view | M2 2.4 |
| Phases, processors, presentation-only components, `SimClock` (ADR-0063) | M2 2.0 |
| Signals and command buffers (ADR-0069) | M2 2.0b |
| Skins and clips from FBX (glTF already) | M2 2.3d (M1 for glTF) |
| Script modules that write components by name (ADR-0062) | M2 2.6 |
| Replay v0, checkpoints, module fingerprints (ADR-0072) | M2 2.8 |
| Inspector widgets for struct-variant enums and named-node maps | M2 2.3c |
| `viewport.gizmo`, `viewport.overlays`, `viewport.pick`, `capture.screenshot`, goldens with FLIP | M1 1.9, 1.16 |
| `physics.raycast` and Jolt queries (the ground probe) | M1 |
| Studio roster, `studio sync`, per-role budgets (ADR-0073) | M3 3.3–3.6 |
| Document operations in `concordia-command` | M4 4.10, if it has landed (else 12.8 adds them, and M4 reuses them) |
| Perf harness, `tests/perf/reference.ron`, budget files | M4 4.1, if it has landed (else M10 10.0's or M11 11.0's minimum) |
| Evolve extension points (custom units as plugins) | M6, optional |

## Crates, modules & directories (created / touched)
**Created**
- `crates/concordia-rig/` *(proposed; open question 4)*: no GPU code. Modules:
  - `format`: the rig model and `.rig.ron`;
  - `unit`: the `RigUnit` trait, unit descriptors and the built-in units;
  - `solve`: pure solver functions (two-bone, FABRIK, CCD, aim, look-at, swing-twist limits);
  - `program`: the compiler (bind, resolve, schedule, slots) and the evaluator;
  - `pose_library`: `.poselib.ron` and mirroring;
  - `bake`: baking forward (controls → bones) and backward (bones → controls);
  - `validate`: diagnostics;
  - `engine`: the `rig` engine module (`[rig]`): `RigInstance`, processors, `rig.*` methods, importers, fingerprints.
- `examples/m12-rig/`: original fixture characters and scenes (below), generated by code, CC0.
- `tests/gates/m12_rig.rs`, `tests/gates/m12_rig_perf.rs`, `tests/gates/m12_live_animator.rs`; `tests/golden/m12_rig/`.

**Touched**
- `concordia-anim` (built in M2 2.4): the `.clip.ron` text model with name-addressed channels; curve evaluation; the `PoseProcessor` seam after the animation graph; control channels; `anim.key_set`, `anim.key_get`, `anim.key_delete`, `anim.preview`.
- `concordia-command`: document operations (`EditOp::SetDocument`), shared with M4 4.10.
- `concordia-render`: control shapes and the rig overlay in `overlay.rs`; `viewport.gizmo` and `viewport.pick` learn control targets; the presentation pose input to skinning (if 2.4 hasn't added one).
- `concordia-assets`: no change to the crate; the `rig` module registers `RigImporter` and `PoseLibraryImporter`, and `concordia-anim` registers the clip importer that bakes.
- `concordia-cli`: `concordia rig validate <path>`; `concordia new` writes a commented `[rig]` table and `assets/anim/{rigs,clips,poses}/`.
- `concordia-studio`: the animator's scope, prompt and a new skill `concordia-rig-and-keyframe`; sync snapshots.
- `editor/ui`: regenerated schemas (`just editor-schemas`); the `RigInstance` inspector widget.
- `plans/M2-port-begins.md`, `M3-closed-loop-and-studio.md`, `M4-unreal-class-visuals.md`, `M5-bring-your-game.md`, `M8-advanced-physics.md`, `later-backlog.md`: cross-reference notes (written with this plan).

## Phases
### Phase 12.0: Refinement, spikes and measurement
- **Objective:** reconcile with `concordia-anim` as built, retire the technical risks, and fix the gate's numbers.
- **Deliverables:**
  - the updated plan, with 2.4's real type names (skeleton, pose, clip, the sim and render poses, the anim module's name) and the author's answers to the open questions;
  - a reconciliation note: whether bones are entities (as the glTF prefab makes them) or indices into a pose buffer, and how `.clip.ron` channels map onto 2.4's clip model;
  - spike 1: a hand-written two-bone IK rig as a 2.4 `PoseProcessor` on a skinned fixture, drawn in a capture;
  - spike 2: the cost of named access versus slot access for control values at 1,000 instances;
  - spike 3: whether M4 4.10's document operations exist; if not, a sketch of `EditOp::SetDocument` agreed with M4's plan;
  - measurements on the reference M3 Pro: two-bone IK, a 12-joint FABRIK chain at 10 iterations, a 12-joint CCD chain, and a 30-unit biped rig, single-threaded and on all cores;
  - the gate fixtures in `examples/m12-rig/` as files that parse;
  - ADR-0260 drafted.
- **Tests first:**
  - `m12_gate_scenes_load_headless`;
  - the gate test files exist and fail (`tests/gates/m12_*.rs`).
- **Done criteria:** budgets are written to `tests/perf/budgets/m12-*.budget.ron`, never to code; each spike has a result in the plan; the open questions are answered or deferred by name.

### Phase 12.1: Rig format and validation
- **Objective:** an agent writes a `.rig.ron` file and gets either a valid rig or errors that name the field and say how to fix it.
- **Deliverables:**
  - reflected types (`deny_unknown_fields`, one-line docs, ranges): `Rig { version, skeleton, params, controls, units, setup, forward, backward, symmetry }`, `Control { value, space, initial, limits, shape, animatable }`, `ControlValue::{Transform, Vec3, Rotation, F32, Bool}`, `Space::{World, Root, Bone(name), Control(name)}`, `Shape { kind: Circle | Box | Sphere | Diamond | Arrow, size, color }`, `Input::{Lit, Param, Control, Bone, Out}`, and `Unit` as a struct-variant enum, one variant per built-in unit (12.2–12.4) plus `Custom { unit, args }` (12.7);
  - an example, canonical through `concordia-scene`'s document writer (sorted keys; units are map keys; event lists are ordered):
    ```ron
    Rig(
      version: 1,
      skeleton: "asset:0193d1a0-…#skin/0",
      params: { "ik_weight": F32(1.0) },
      controls: {
        "foot_l_ik": Control(value: Transform, space: Bone("root"), shape: (kind: Box, size: 0.12, color: (0.1, 0.6, 1.0))),
        "knee_l_pole": Control(value: Vec3, space: Bone("root"), shape: (kind: Sphere, size: 0.05, color: (1.0, 0.8, 0.1))),
        "look_target": Control(value: Vec3, space: World, shape: (kind: Diamond, size: 0.08, color: (0.9, 0.2, 0.9))),
      },
      units: {
        "foot_l_from_bone": SetControlFromBone(control: "foot_l_ik", bone: "foot_l"),
        "leg_l": TwoBoneIk(
          root: "thigh_l", mid: "shin_l", tip: "foot_l",
          target: Control("foot_l_ik"), pole: Control("knee_l_pole"),
          max_stretch: Lit(F32(1.0)), weight: Param("ik_weight"),
        ),
        "head_look": LookAt(
          chain: ["spine_2", "neck", "head"], weights: [0.2, 0.3, 0.5],
          target: Control("look_target"), aim_axis: PosZ, up_axis: PosY,
          limit_deg: Lit(F32(70.0)),
        ),
      },
      setup: ["foot_l_from_bone"],
      forward: ["leg_l", "head_look"],
      backward: ["foot_l_from_bone"],
    )
    ```
  - validation, all errors collected, each a `RigDiagnostic { kind, path (JSON Pointer), message, suggestion }`: unknown unit, bone, control or parameter names (with the nearest name), input types against the unit's descriptor, pure units in an event list, a unit listed twice in one event (one unit may appear in several events), data cycles among pure units, controls of the wrong value kind, bones missing from the skeleton, and ranges;
  - names follow ADR-0068's rule (`[A-Za-z0-9_-]+`);
  - `RigImporter` (`.rig.ron` → validated rig artifact in the DDC), registered by the `rig` module.
- **Tests first:**
  - `rig_roundtrips_canonically` (`insta`, every example file);
  - `rig_text_is_canonical_after_any_edit` (`proptest`: parse, mutate, write, parse: same model, same bytes);
  - `unknown_fields_are_errors_with_their_path`;
  - `every_rig_error_is_reported_not_just_the_first`;
  - `unknown_bone_suggests_the_nearest_name`;
  - `input_type_mismatch_names_the_unit_input_and_expected_type`;
  - `pure_unit_in_an_event_list_is_an_error`;
  - `pure_data_cycle_is_an_error_naming_the_cycle`;
  - `schema_snapshot_of_rig_types` (`insta` over the JSON Schema `schema.get` serves);
  - `rig_import_is_deterministic_and_cached`.
- **Implementation notes:** reflection has no recursive types, which the flat map suits. `Input::Out(unit, output)` is the only edge. Run `just editor-schemas` when the reflected types change.
- **Done criteria:** every gate rig parses and validates against its fixture skeleton.

### Phase 12.2: Rig compiler and evaluator core
- **Objective:** bind a rig to a skeleton once, then evaluate it with no allocation and in a fixed order.
- **Deliverables:**
  - `program::compile(rig, skeleton, units) -> Result<RigProgram, RigError>`: resolves names to slots (bone indices, control slots, value registers), topologically orders each event's pure dependencies before their first use (ties by name), and stores one instruction list per event;
  - `RigProgram::evaluate(event, &mut RigFrame)`, where `RigFrame` holds the input pose, control values, parameters and scratch registers; pure results are computed once per evaluation;
  - pose spaces: local (parent-relative), component (relative to the skeleton root) and world; global transforms are recomputed lazily per dirty chain and are exact for unchanged bones;
  - core units: `GetBone`, `SetBone` (space, weight, propagate to children or not), `GetControl`, `SetControl`, `SetControlFromBone`, `GetParam`, and math (`Add`, `Sub`, `Mul`, `Lerp`, `Clamp`, `Remap`, `Normalize`, `Cross`, `Dot`, `QuatFromEuler`, `QuatFromAxisAngle`, `BlendTransforms`, `Compose`, `Inverse`, `ProjectOnPlane`);
  - a `RigStats` result per evaluation (units run, IK iterations, reach error per solver), read by `rig.evaluate`.
- **Tests first:**
  - `compiled_order_is_deterministic_and_independent_of_map_order` (`proptest`);
  - `pure_units_run_once_per_evaluation_even_with_many_users`;
  - `set_bone_in_component_space_matches_hand_computed_transforms`;
  - `untouched_bones_are_bit_identical_to_the_input_pose`;
  - `same_inputs_give_bit_identical_pose` (`proptest` over controls and poses);
  - `evaluation_allocates_nothing` (a counting allocator in the test);
  - `binding_to_a_skeleton_without_a_named_bone_is_an_error`;
  - `nan_or_infinite_control_values_are_rejected_at_the_boundary`.
- **Implementation notes:** `f32` with glam; no fused multiply-add assumptions; no `HashMap` iteration anywhere. Determinism is per binary and machine, as ADR-0072 states.
- **Done criteria:** a rig of only core units drives a fixture skeleton and matches hand-computed poses.

### Phase 12.3: IK solvers
- **Objective:** three IK solvers as pure functions, each tested against analytic values or proven properties.
- **Deliverables:**
  - `solve::two_bone(root, mid, tip, target, pole, max_stretch) -> TwoBoneResult`: law of cosines in the plane given by the pole vector; full extension beyond reach; optional stretch up to `max_stretch`; defined fallbacks for a target at the root and a pole on the chain's line (no NaN);
  - `solve::fabrik(chain, target, tolerance, max_iterations, limits) -> ChainResult` (Aristidou and Lasenby): forward and backward reaching passes; the root stays fixed; joint limits applied per iteration;
  - `solve::ccd(chain, target, tolerance, max_iterations, limits) -> ChainResult`: tip-to-root joint rotations, with an optional per-joint damping angle;
  - units `TwoBoneIk`, `FabrikIk`, `CcdIk`, each with `weight`, writing rotations only (positions follow from bone lengths, except two-bone stretch);
  - both iterative solvers stop at `tolerance` or `max_iterations`, whichever comes first; both are deterministic for equal inputs.
- **Tests first:**
  - `two_bone_matches_analytic_reference_values` (a table of chains, targets and poles; expected angles from an independent Python script, committed beside the test, as 2.3a did);
  - `two_bone_right_angle_case` (bones 0.5 and 0.5, target at distance √0.5: the mid joint bends 90°);
  - `two_bone_beyond_reach_is_fully_extended_toward_the_target`;
  - `two_bone_pole_sets_the_bend_plane`;
  - `two_bone_degenerate_inputs_never_produce_nan` (`proptest`, including zero lengths and coincident points);
  - `fabrik_preserves_bone_lengths` (`proptest`, every iteration);
  - `fabrik_keeps_the_root_fixed`;
  - `fabrik_reaches_reachable_targets_within_tolerance` (`proptest`, unconstrained chains, within `max_iterations`);
  - `fabrik_unreachable_target_straightens_the_chain_toward_it`;
  - `ccd_distance_to_target_never_increases_per_joint_step` (`proptest`);
  - `ccd_respects_joint_limits_after_every_iteration`;
  - `solver_stops_at_tolerance_or_max_iterations` (iteration count reported in `RigStats`);
  - `ik_weight_zero_leaves_the_pose_unchanged` and `ik_weight_one_reaches_the_target`.
- **Implementation notes:** references: Aristidou and Lasenby, "FABRIK: A fast, iterative solver for the Inverse Kinematics problem" (Graphical Models, 2011); Wang and Chen, "A combined optimization method for solving the inverse kinematics problem of mechanical manipulators" (IEEE Trans. Robotics and Automation, 1991) for CCD; Lander, "Making Kine More Flexible" (Game Developer, 1998) and the law-of-cosines treatment in standard robotics texts for two-bone.
- **Done criteria:** the solver tests pass; the crane fixture reaches every gate target.

### Phase 12.4: Aim, look-at and constraints
- **Objective:** the units that orient bones and tie them to each other.
- **Deliverables:**
  - `Aim { bone, target, aim_axis, up_axis, up_target, weight }`: rotate one bone so its axis points at the target, with a secondary up axis;
  - `LookAt { chain, weights, target, aim_axis, up_axis, limit_deg }`: aim distributed along a chain by weights (spine, neck, head), clamped by a cone limit;
  - constraints, each with `weight` and `maintain_offset`: `ParentConstraint`, `PositionConstraint`, `RotationConstraint`, `ScaleConstraint`;
  - `LimitRotation { bone, swing_deg, twist_deg }` by swing-twist decomposition;
  - `DistributeRotation { chain, rotation, weights }`;
  - `GroundProbe { from, direction, max_distance }` → hit position and normal, through a `RigWorld` trait; the `physics` module implements it with `physics.raycast`'s query. Without physics it returns no hit.
- **Tests first:**
  - `aim_points_the_axis_at_the_target` (angle error ≤ 1e-5 rad);
  - `aim_up_axis_resolves_the_roll`;
  - `look_at_distributes_rotation_by_weights` (weights sum check; each bone's share);
  - `look_at_clamps_to_the_cone_limit`;
  - `parent_constraint_with_maintain_offset_keeps_the_initial_offset`;
  - `constraint_weight_blends_linearly_in_translation_and_by_slerp_in_rotation`;
  - `swing_twist_limit_matches_reference_decomposition` (reference values from the Python script);
  - `ground_probe_hits_the_physics_floor_and_misses_without_physics`.
- **Implementation notes:** swing-twist decomposition from Dobrowolski, "Swing-twist decomposition in Clifford algebra" (2015). Aim and look-at are closed form: no iteration.
- **Done criteria:** the mannequin's head follows a target within its limit; the strider's feet find the stepped floor.

### Phase 12.5: Setup and backward solve
- **Objective:** a rig that sets itself up from the skeleton, and that can turn a bone pose back into control values, so existing clips can be edited on controls.
- **Deliverables:**
  - the setup event: runs at bind and when the rig or skeleton changes; typically places controls at their bones in the rest pose and stores offsets as control initial values;
  - the backward event: computes control values from an input pose (for example an IK control from the tip bone's transform, a pole from the mid joint's position);
  - an `invertible` flag in each unit's descriptor; validation warns when a control used in `forward` is never written in `backward`;
  - `RigProgram::backward(pose) -> Controls` and `RigProgram::forward(controls) -> Pose`.
- **Tests first:**
  - `setup_places_controls_at_their_bones_in_the_rest_pose`;
  - `forward_of_backward_reproduces_reachable_poses` (`proptest` over poses the rig can produce; per-bone error ≤ 1e-4 m and 1e-4 rad);
  - `backward_of_forward_reproduces_controls_in_range` (`proptest`; poles away from the degenerate line, no stretch);
  - `controls_never_written_by_backward_are_reported`;
  - `setup_reruns_when_the_rig_changes_on_hot_reload`.
- **Done criteria:** an imported fixture walk clip round-trips through the mannequin's controls within tolerance.

### Phase 12.6: Runtime integration and determinism
- **Objective:** rigs run in Terra's phases on any number of entities, and the world hash stays honest about which rigs gameplay depends on.
- **Deliverables:**
  - `RigInstance` (reflected component): `rig: AssetRef`, `mode: Presentation | Simulation`, `controls: BTreeMap<String, ControlValue>` (current values; authored in the edit world, written by gameplay and clips in the play world), `params: BTreeMap<String, ParamValue>`, `weight: f32`;
  - the seam: `concordia-anim` exposes a `PoseProcessor` trait run after an entity's animation graph; `concordia-rig` implements it; `concordia-anim` never depends on `concordia-rig`;
  - **Simulation mode:** processor `rig.solve` in `PostPhysics`, `after` the anim graph's processor; it reads the graph's output pose and `RigInstance`, and writes the simulation pose (hashed). `BoneAttachment` and gameplay may read the result;
  - **Presentation mode:** processor `rig.present` in `Presentation`; it writes the presentation pose the renderer skins (2.4's render pose if it has one, else a new presentation-only `PresentedPose`), excluded from the world hash;
  - **the schedule rule:** a simulation processor that reads bones a Presentation rig writes sees only the pre-rig pose; a `BoneAttachment` on a bone written by a Presentation rig follows the presented pose only in rendering, and this is documented;
  - **edit-world display:** the renderer's extraction evaluates rigs for edit-world views (no tick, no hash), so a drag of a control shows the solved pose at once;
  - gameplay and scripts drive rigs by writing `RigInstance.controls` and `params` by name (ADR-0062 named access; no WIT change);
  - **replay:** the `rig` module's `EngineModule::fingerprints` reports the BLAKE3 hash of each Simulation-mode rig definition and of each custom unit's code, so a changed rig refuses a replay with `ReplayMismatch` (ADR-0072);
  - parallel evaluation: one job per batch of entities; each entity's evaluation is independent, so results don't depend on thread count.
- **Tests first:**
  - `simulation_rig_changes_the_world_hash_presentation_rig_does_not`;
  - `presentation_rig_state_is_unreadable_from_simulation_processors` (a `ScheduleError` naming both);
  - `pose_hash_identical_for_1_and_8_threads` (1,000 strider instances, 600 ticks);
  - `rig_evaluation_order_is_stable_across_runs`;
  - `script_sets_a_control_by_name_and_the_rig_follows` (a 2.6 conformance-style script);
  - `bone_attachment_follows_a_simulation_rig`;
  - `replay_of_a_simulation_rig_verifies` (600 ticks, checkpoints unchanged; ADR-0072);
  - `changing_a_simulation_rig_refuses_the_replay_with_its_fingerprint`;
  - `presentation_rig_does_not_change_replay_checkpoints`;
  - `edit_world_drag_shows_the_solved_pose_without_a_play_world`;
  - `rig_instance_roundtrips_in_scene_text` (`insta`).
- **Implementation notes:** if 12.0's spike shows named control access too slow, keep slots in a runtime column built at play start and keep `RigInstance.controls` as the named interface.
- **Done criteria:** the strider walks the stepped floor in Simulation mode with replay verified; the reed tracks its target in Presentation mode with the world hash unchanged.

### Phase 12.7: Custom units
- **Objective:** a project adds its own rig units in Rust, without editing the engine's unit list.
- **Deliverables:**
  - the `RigUnit` trait: `descriptor()` (inputs, outputs, types, pure or mutating, invertible, doc) and `evaluate(&self, inputs, outputs, &mut RigFrame)`;
  - registration through `EngineModule` (a native script module or an Evolve plugin); rigs use them as `Custom { unit: "<module>/<name>", args }`;
  - the built-in units implement the same trait, so the compiler has one path;
  - custom units obey the native tier's rules (ADR-0061: no mutable statics, checked by `check_module_source`).
- **Tests first:**
  - `custom_unit_plugs_in_without_compiler_changes` (a test-only unit);
  - `custom_unit_descriptor_is_validated_like_a_builtin`;
  - `custom_unit_is_fingerprinted_for_replay`;
  - `every_builtin_unit_has_a_descriptor_with_docs` (for agents);
  - `unit_catalogue_resource_lists_every_unit` (`concordia://rig/units`, generated from descriptors).
- **Implementation notes:** WASM rig units are deferred (open question 9). The unit catalogue is an MCP resource, not a tool, so it costs no role budget.
- **Done criteria:** the gate's crane uses one custom unit (a project-defined joint coupling).

### Phase 12.8: Document transactions (shared with M4 4.10)
- **Objective:** edits to `.rig.ron`, `.clip.ron` and `.poselib.ron` are attributed, undoable transactions, like entity edits.
- **Deliverables:**
  - if M4 4.10 has landed: reuse its document operations; add nothing but the three file kinds;
  - otherwise: `EditOp::SetDocument { path, before: Blake3, after: String }` in `concordia-command`, where `after` is canonical text. Undo restores the previous text. The op is journaled in the open scene's journal;
  - documents and entity edits share transactions, so "add a `RigInstance` and create its rig file" is one undo step;
  - a file changed outside the engine since the op makes undo refuse with `TxnConflict` (−32005), naming the file and both hashes; no new error code;
  - the asset watcher re-imports a document after commit, undo and redo.
- **Tests first:**
  - `document_edit_is_journaled_with_its_actor`;
  - `undo_restores_the_document_byte_identical`;
  - `redo_reapplies_the_document_edit`;
  - `entity_and_document_edits_undo_together`;
  - `undo_after_an_external_change_is_refused_naming_the_file`;
  - `aborted_transaction_leaves_no_file_written`;
  - `journal_replay_on_open_restores_document_history`.
- **Implementation notes:** an ADR addendum: M4's graph ADR records the op if M4 writes it first; otherwise ADR-0263 records it and M4 reuses it. 2.3c's `material.set` stays not undoable; adopting document ops is its owner's choice.
- **Done criteria:** a rig file edit undoes and redoes byte-identically through `txn.undo` and `txn.redo`.

### Phase 12.9: Rig methods
- **Objective:** agents build and check rigs entirely through the Engine API.
- **Deliverables:**
  - **`rig.validate`** (MCP `rig_validate`), read-only, no GPU: `{ path } | { rig: AssetRef }` → `{ ok, diagnostics, units, controls }`. An invalid rig is a result, not an error;
  - **`rig.apply`** (MCP `rig_apply`), mutating, takes an `actor`: a structured patch on one rig file (add, remove or replace a unit; set one input; add, remove or change a control; set an event's order; set a parameter), applied as one document transaction; the result carries the new diagnostics. Whole-file text edits stay valid, and hot reload picks them up, as M4 plans for graphs;
  - **`rig.evaluate`** (MCP `rig_evaluate`), read-only: `{ entity, world?, controls?, event? }` → bone transforms (local and world), control values and `RigStats`. It is how an agent checks a pose with numbers, not pixels;
  - CLI: `concordia rig validate <path>` prints `file:path: message` and exits non-zero on errors;
  - reload and failure log lines under `rig` (visible in `log.tail`); a broken rig keeps the last good program running.
- **Tests first:**
  - `rig_apply_is_attributed_and_undoable` (actor in the journal; undo byte-identical);
  - `rig_apply_with_an_invalid_patch_changes_nothing` (diagnostics returned, file untouched);
  - `rig_validate_works_without_a_gpu`;
  - `rig_evaluate_matches_the_runtime_pose` (same tick, same entity);
  - `broken_rig_keeps_the_last_good_program_running`;
  - `cli_validate_exit_code_and_format`;
  - `method_schemas_snapshot` (`insta`; generated from reflection);
  - `rig_methods_exist_only_with_the_rig_module` (projects without `[rig]` keep their tool set).
- **Done criteria:** a test client builds the mannequin's leg IK from an empty rig over MCP.

### Phase 12.10: The clip format and keyframing
- **Objective:** an agent keys controls over time, and sees and measures the result at any frame.
- **Deliverables:**
  - `.clip.ron` in `concordia-anim`: `Clip { version, rate, frames, looping, rig, curves, bones, events }`:
    - `curves`: control channels by name (`"<control>.<field>[.<axis>]"`, e.g. `foot_l_ik.translation.y`, `foot_l_ik.rotation.x`), each a list of keys `(frame, value, interp: Constant | Linear | Cubic, tangent: Auto | Flat | User(in, out))`;
    - `bones`: sampled bone tracks by bone name (from baking or import), one sample per frame;
    - `events`: 2.4's animation events by frame;
  - key times are integer frames at `rate`, so text never holds drifting float times (open question 6); rotations on controls are Euler degrees per axis in the control's declared order (open question 7);
  - cubic curves are Hermite with clamped auto tangents, so a curve never overshoots between keys of equal value;
  - an example:
    ```ron
    Clip(
      version: 1, rate: 30, frames: 48, looping: true,
      rig: Some("asset:0193d1a2-…"),
      curves: {
        "foot_l_ik.translation.y": [
          (frame: 0, value: 0.0, interp: Cubic, tangent: Auto),
          (frame: 12, value: 0.25, interp: Cubic, tangent: Auto),
          (frame: 24, value: 0.0, interp: Linear, tangent: Flat),
        ],
      },
      bones: {},
      events: {},
    )
    ```
  - **`anim.key_set`** (MCP `anim_key_set`), mutating: one or many keys on one clip (`channel`, `frame`, `value`, `interp`, `tangent`), creating the clip if absent; one document transaction per call;
  - **`anim.key_delete`** (MCP `anim_key_delete`), mutating: keys by channel and frame range;
  - **`anim.key_get`** (MCP `anim_key_get`), read-only: keys in a range, or values sampled at given frames;
  - **`anim.preview`** (MCP `anim_preview`): sets the session's preview of one entity to a clip and frame, for edit-world views and captures. It is view state, like `viewport.overlays`: never saved, journaled or hashed;
  - live playback: 2.4's clip node samples control channels into `RigInstance.controls`, so a control clip plays through a live rig (for layering, and in Simulation mode deterministically).
- **Tests first:**
  - `clip_roundtrips_canonically` (`insta`) and `clip_text_is_canonical_after_any_edit` (`proptest`);
  - `cubic_auto_tangents_never_overshoot_between_equal_keys` (`proptest`);
  - `constant_linear_and_cubic_sample_to_reference_values`;
  - `key_set_is_attributed_and_undoable_as_one_transaction` (many keys, one undo step);
  - `key_delete_range_removes_only_that_range`;
  - `key_get_samples_match_runtime_sampling`;
  - `preview_changes_the_capture_but_not_the_scene_or_journal`;
  - `looping_clip_samples_frame_0_at_the_end`;
  - `unknown_channel_suggests_the_nearest_control`;
  - `control_clip_drives_a_live_rig_deterministically` (hash across thread counts in Simulation mode).
- **Done criteria:** an agent keys the mannequin's wave from MCP alone, previews it at six frames and reads the hand's height with `rig.evaluate`.

### Phase 12.11: Baking
- **Objective:** a control clip becomes a skeleton clip that plays with no rig, and a skeleton clip becomes control curves an animator can edit.
- **Deliverables:**
  - **bake at import:** the clip importer evaluates the clip's rig at every frame and stores a skeleton clip artifact in the DDC (2.4's runtime clip type), keyed by BLAKE3 of the clip text, the rig, the skeleton and the unit fingerprints. Games play the artifact through the anim graph like any clip;
  - **`rig.bake`** (MCP `rig_bake`), mutating, one document transaction:
    - `to: Bones`: writes a new `.clip.ron` with sampled bone tracks (freezing a rig layer over an existing clip, for example foot IK over an imported walk);
    - `to: Controls`: runs the backward solve at every frame of a skeleton clip and writes control curves, reduced to keys within a tolerance (default 1e-4 in value units);
  - a reduction step that drops keys a cubic curve reproduces within tolerance.
- **Tests first:**
  - `baked_clip_equals_live_evaluation_at_every_frame` (bit-identical bone transforms at frame times);
  - `baked_clip_between_frames_is_within_interpolation_tolerance`;
  - `bake_is_cached_and_deterministic` (same artifact key twice; byte-identical);
  - `changing_the_rig_rebakes_its_clips`;
  - `bake_to_controls_then_forward_reproduces_the_source_clip` (within 12.5's tolerance);
  - `key_reduction_stays_within_tolerance` (`proptest`);
  - `baked_clip_plays_without_a_rig_instance`.
- **Done criteria:** the mannequin's wave plays from its baked artifact on 1,000 instances without any `RigInstance`.

### Phase 12.12: Pose library
- **Objective:** named poses an agent can save, apply, blend and mirror.
- **Deliverables:**
  - `.poselib.ron`: `PoseLibrary { rig, poses: { name: { control: ControlValue } } }`;
  - **`rig.pose_save`** (MCP `rig_pose_save`): stores an entity's current control values (all, or listed controls) as a named pose;
  - **`rig.pose_apply`** (MCP `rig_pose_apply`): applies a pose with `weight` and optional controls, either to `RigInstance.controls` (an entity edit) or as keys at a frame of a clip (a document edit); `mirror: true` swaps sides by the rig's `symmetry` (name pattern such as `_l`/`_r`, and a mirror axis).
- **Tests first:**
  - `pose_library_roundtrips_canonically`;
  - `pose_save_then_apply_restores_the_controls`;
  - `pose_apply_at_half_weight_blends_by_lerp_and_slerp`;
  - `mirrored_pose_applied_twice_is_the_original`;
  - `pose_apply_to_a_clip_keys_one_frame_in_one_transaction`.
- **Done criteria:** the mannequin's hand poses (open, fist, point) apply and mirror.

### Phase 12.13: Control gizmos, the rig overlay and the inspector
- **Objective:** humans see and drag controls in the viewport, through the same API agents use (D7: debug views and inspector widgets, no new panel).
- **Deliverables:**
  - `Overlays` gains `rig: Option<RigOverlay { entities, show: controls | chains | targets }>`, set through `viewport.overlays` and drawn by `capture.screenshot { overlays: true }`; control shapes are engine-drawn lines, like gizmo rings;
  - `viewport.pick` reports control hits; `viewport.gizmo` accepts a control target (`{ entity, control }`) and drags it in the control's space with the existing move, rotate and scale math;
  - a drag is one transaction: it patches `RigInstance.controls`, or, when the session previews a clip and auto-key is on (`viewport.overlays { auto_key: true }`), it sets keys at the preview frame for the changed channels;
  - a `rig` debug view: IK chains, pole vectors, targets and reach errors, on top of 2.4's skeleton view;
  - the inspector: a `RigInstance` widget (controls grouped by the rig's declared groups, from reflection), and `.rig.ron` shown in the Inspector dock when selected in the File System dock, as 2.3c does for materials, edited through `rig.apply`.
- **Tests first:**
  - `control_shapes_golden` (`tests/golden/m12_rig/controls_*`; mean FLIP ≤ 0.01);
  - `pick_hits_a_control_before_the_mesh_behind_it`;
  - `control_drag_is_one_undo_step_with_the_human_actor`;
  - `auto_key_drag_writes_keys_at_the_preview_frame`;
  - `rig_debug_view_golden`;
  - `rig_instance_inspector_edits_send_component_patch` (vitest with a mocked client);
  - `no_rig_overlay_renders_byte_identical_to_before` (the existing goldens are unchanged).
- **Done criteria:** a human poses the mannequin by dragging its controls, and undo walks back each drag.

### Phase 12.14: Studio enablement (the animator)
- **Objective:** the default team's animator builds rigs and keys clips, within its tool budget.
- **Deliverables:**
  - `roster/agents/animator.md`: scope adds `rig_validate`, `rig_apply`, `rig_evaluate`, `rig_bake`, `rig_pose_save`, `rig_pose_apply`, `anim_key_set`, `anim_key_get`, `anim_key_delete`, `anim_preview`, and what its prompt now uses to see and undo its work (`scene_open`, `viewport_overlays`, `txn_undo`); write paths add `assets/anim/rigs/**`, `assets/anim/clips/**` and `assets/anim/poses/**` (already under `assets/anim/**`);
  - a new skill `concordia-rig-and-keyframe`: the rig format, the unit catalogue resource, the loop (edit → `rig_validate` → `anim_key_set` → `anim_preview` → `capture_screenshot` with overlays → `rig_evaluate` for numbers), when to bake, and when to use a pose;
  - the prompt keeps the Blender bridge for clips authored outside the engine, and uses `rig_bake` with `to: Controls` to edit them on controls;
  - the tech artist keeps materials and shaders; the gameplay programmer attaches `RigInstance` to gameplay entities and drives controls from scripts.
- **Tests first:**
  - `animator_scope_names_only_existing_tools` (3.3's rule);
  - `animator_prompt_and_skills_use_every_scoped_tool` and the reverse (ADR-0073 rule 3);
  - `every_enabled_default_role_stays_under_its_tool_budget` passes with the widened animator (22k; about 23 tools, estimated ≈17k);
  - `studio_sync_snapshots` updated (`insta`);
  - `animator_can_rig_and_key_over_mcp` (deterministic agent-gate step: scripted MCP calls from the role's scope build a leg IK, key a step and bake it).
- **Done criteria:** `studio sync` on a new project yields the animator with the skill, under budget.

### Phase 12.15: Gate
Write the gate tests at the start of the milestone (12.0); this phase makes them pass on the reference machine and updates the plan's Status.

## Milestone gate
`just gate M12` runs `tests/gates/m12_rig.rs` and `m12_rig_perf.rs` on original fixtures in `examples/m12-rig/`. The characters are our own, generated by a Rust fixture builder in `tests/src/` (capsule segments skinned rigidly to bones, procedural textures, CC0), committed as small `.glb` files; no recreation of any existing game's characters:
- **mannequin:** a 22-bone biped with leg and arm two-bone IK, a three-bone look-at, finger controls, and a pose library (open, fist, point);
- **strider:** a four-legged walker with FABRIK legs and ground probes, in Simulation mode, walking a scripted path over a stepped floor;
- **reed:** a 12-segment stalk that tracks a drifting target with FABRIK, in Presentation mode;
- **crane:** a 4-joint arm reaching targets with CCD, joint limits and one custom unit;
- **crowd:** 1,000 mannequins with foot IK and look-at (the throughput scene).

It asserts:
1. **Format:** every rig, clip and pose library round-trips canonically and validates with no diagnostics.
2. **Solvers:** the analytic two-bone table, and the FABRIK and CCD property tests, pass on the fixture chains.
3. **Round trips:** forward of backward and backward of forward stay within 12.5's tolerances on the mannequin.
4. **Determinism:** the strider's 600-tick play-world hashes are identical at 1 and 8 threads and under replay; the reed changes no hash; a changed strider rig refuses the replay by fingerprint.
5. **Authoring:** a scripted MCP session builds the mannequin's leg IK with `rig.apply`, keys a 48-frame wave with `anim.key_set`, applies and mirrors a hand pose, and every step is in the journal with its actor; undoing all of it restores every file byte-identical.
6. **Baking:** the baked wave equals live evaluation at every frame, bit for bit, and plays on instances without a `RigInstance`.
7. **Images:** each scene at fixed ticks or frames matches its goldens within mean FLIP 0.01 (`tests/golden/m12_rig/`).
8. **Scale on reference** (provisional, set in 12.0): the mannequin rig evaluates at ≥ 2,000 instances per ms on all cores and ≥ 300 per ms on one thread; a 12-joint FABRIK solve at 10 iterations takes ≤ 2 µs; the crowd scene's rig work is ≤ 1.0 ms CPU per tick. Off-reference runs report `NotReference`.
9. **No silent failure:** unreachable targets, iteration caps and degenerate poles are reported in `RigStats` and `rig.evaluate`, never as NaN.

`just gate-live M12` runs `m12_live_animator_keys_a_wave`: a real `claude -p` animator, on the subscription, takes a task ("a two-second loopable wave on the mannequin's right arm; the hand never passes through the head"), and the test asserts outcomes: the clip validates, its first and last frames give the same pose, `rig.evaluate` at every frame keeps the hand outside the head's capsule, and the edits are journaled with the agent's actor.

## Risks & mitigations
| Risk | Mitigation |
|---|---|
| `concordia-anim` lands with types or seams this plan didn't assume | Phase 12.0 reconciles first; cross-reference notes in M2 2.4 ask for the `PoseProcessor` seam and name-addressed channels now. |
| Rigs grow into a general programming language | Fixed unit set plus Rust custom units; no loops or branches in rig files; scripts drive controls from outside. |
| A Presentation rig's pose leaks into gameplay (attachments, hit tests) | ADR-0063's schedule checks; a test that simulation can't read presentation state; the default mode is documented. |
| Iterative solvers jitter or differ run to run | Fixed tolerance and iteration caps, no time-based exits, per-entity independence; thread-count hash tests. |
| Undo of file edits races with editors outside the engine | Hash-checked undo that refuses with `TxnConflict`, naming the file. |
| Two document-transaction mechanisms (M4 and M12) | One op in `concordia-command`, recorded in one ADR, whichever milestone lands first. |
| Euler control curves hit gimbal lock | Per-control rotation order; bones baked as quaternions; open question 7. |
| The animator's scope exceeds 22k tokens | Estimated ≈17k; the budget test decides; fallback: a separate rigger role (open question 11). |
| Baked text clips grow large | Bake to the DDC at import by default; sampled text only on explicit `rig.bake`; key reduction for control curves. |
| Clean-room contamination (Control Rig is an Unreal feature) | Concepts from Epic's public docs and talks only; open papers for every solver; references in `AGENTS.md`. |

## Open questions
Each is for the author, with a recommendation.
1. **When does a visual rig graph arrive?** *Recommendation:* not in M12. After M4's graph panel (4.11) exists and M12's gate passes, the panel gets a rig view over `.rig.ron`: units as nodes, `Out` inputs as edges, event lists as an execution chain, positions in a separate layout block, edits through `rig.apply`. There is no `.graph.ron` rig kind and no second format.
2. **Execution model:** ordered event lists for mutating units with pulled pure units (Control Rig's model), or a pure topological order of everything? *Recommendation:* ordered lists. Side-effect order is then explicit text an agent can read and reorder, and two units writing one bone never depend on a sort's tie-break.
3. **File names:** `.rig.ron`, `.clip.ron` and `.poselib.ron`, or `.anim.ron` for clips? *Recommendation:* `.clip.ron`, matching `AssetKind::Clip`, and distinct from `.animgraph.ron`.
4. **Crate split:** a separate `concordia-rig`, or rigs inside `concordia-anim`? *Recommendation:* separate. `concordia-anim` owns clips, curves and the `PoseProcessor` seam; `concordia-rig` depends on it, never the reverse.
5. **Default mode:** Presentation or Simulation? *Recommendation:* Presentation by default, Simulation opt-in for rigs gameplay reads (attachments, foot placement that affects movement).
6. **Key times:** integer frames at the clip's rate, or seconds? *Recommendation:* frames. Canonical text stays exact, and baking samples exactly at keys.
7. **Control rotations:** Euler curves per axis, or quaternion keys? *Recommendation:* Euler degrees per axis with a declared order, as animators edit them; bones are baked as quaternions.
8. **Undo for documents:** extend `concordia-command` with document ops (shared with M4 4.10), or follow `material.set` (attributed, not undoable)? *Recommendation:* extend. Animators iterate key by key, and M4 already plans undoable `graph.apply`. Journal them in the open scene's journal; with no scene open, the methods return `NoSceneOpen`.
9. **Custom units in WASM?** *Recommendation:* Rust units only in M12 (native modules and Evolve plugins). Gameplay already drives controls by name from either tier. Add a script-protocol unit binding later if a project needs sandboxed units, with one batched call per unit per rig definition.
10. **Live control playback, or bake only?** *Recommendation:* both. Bake at import is the shipping path; live playback serves layering (aim offsets over a run) and previews.
11. **Which role?** *Recommendation:* the animator owns rigs and keys. Split a `rigger` role (the `rig_*` tools) only if the budget test fails.
12. **A timeline or dope-sheet panel?** *Recommendation:* not in M12 (D7). Agents use `anim.key_get`, `anim.preview` and captures; humans use preview frames and auto-key drags. Revisit after the gate, with M4's graph panel.
13. **Retargeting and full-body IK?** *Recommendation:* out of M12; both stay in the backlog and M5's open question 10. A same-topology retarget can already be written as a rig of constraints.
14. **Stateful units (springs, secondary motion)?** *Recommendation:* defer. They need per-instance state (hashed in Simulation mode) and a fixed-dt rule; the gate needs none.
15. **Port mappings (M5):** should the Godot importer map Godot 4's skeleton modifiers (`SkeletonIK3D`, `LookAtModifier3D` and similar) to rig units? *Recommendation:* yes, a mapped subset, with unmapped settings in the port ledger; Godot is MIT, so its docs and source may be read.
16. **Garments (M8):** which pose do garments follow? *Recommendation:* the final presented pose (after both rig modes), because garments are presentation; body proxies follow the same pose.
17. **When does M12 run?** *Recommendation:* after M2 2.4 and the Studio track, independent of M4. Phases 12.1–12.5 need no GPU and may start once 2.4's skeleton and pose types exist.
18. **GPU rig evaluation?** *Recommendation:* no. CPU evaluation in parallel covers the gate's crowd; M7's 100k-agent scale uses representation LOD (2.4b) and baked clips.

## ADRs to write
- **ADR-0016** (design block, written, Proposed): the Rigging v1.0 milestone (M12).
- **ADR-0260:** the rig format, units, events and the execution model (after open questions 1–4).
- **ADR-0261:** solver contracts: tolerances, iteration caps, degenerate-input fallbacks and reported stats.
- **ADR-0262:** runtime integration and determinism: modes, phases, hashing, the `PoseProcessor` seam and replay fingerprints.
- **ADR-0263:** document transactions in `concordia-command` (only if M4 4.10 hasn't recorded them).
- **ADR-0264:** the clip format, curves, keyframing methods, preview and baking.

## Effort estimate
| Phase | Weeks |
|---|---|
| 12.0 Refinement, spikes, measurement | 1 |
| 12.1 Rig format and validation | 1.5 |
| 12.2 Compiler and evaluator core | 2 |
| 12.3 IK solvers | 2 |
| 12.4 Aim, look-at, constraints | 1.5 |
| 12.5 Setup and backward solve | 1.5 |
| 12.6 Runtime integration and determinism | 2 |
| 12.7 Custom units | 1 |
| 12.8 Document transactions (0.5 if reused) | 1.5 |
| 12.9 Rig methods | 1 |
| 12.10 Clip format and keyframing | 2 |
| 12.11 Baking | 1 |
| 12.12 Pose library | 0.5 |
| 12.13 Gizmos, overlay, inspector | 1.5 |
| 12.14 Studio enablement | 0.5 |
| 12.15 Gate | 1 |
| **Total** | **~22 weeks** (range 17–28) |

## References (appendix)
Public papers, talks, documentation and permissively licensed code only; no Unreal source.
- Epic Games, Control Rig public documentation (overview, rig units, controls, forwards and backwards solve, setup event, animating and baking in Sequencer): concepts and vocabulary only.
- Epic Games' public GDC and Unreal Fest talks on Control Rig and in-engine animation authoring: architecture at the level presented in the talks only.
- Andreas Aristidou and Joan Lasenby, "FABRIK: A fast, iterative solver for the Inverse Kinematics problem" (Graphical Models 73(5), 2011).
- Li-Chun Tommy Wang and Chih Cheng Chen, "A combined optimization method for solving the inverse kinematics problem of mechanical manipulators" (IEEE Transactions on Robotics and Automation, 1991): cyclic coordinate descent.
- Jeff Lander, "Making Kine More Flexible" and "Oh My God, I Inverted Kine!" (Game Developer, 1998): CCD and analytic two-bone IK for games.
- Przemysław Dobrowolski, "Swing-twist decomposition in Clifford algebra" (arXiv:1506.05481, 2015).
- Doris Kochanek and Richard Bartels, "Interpolating splines with local tension, continuity, and bias control" (SIGGRAPH 1984): Hermite tangents.
- Ken Shoemake, "Animating rotation with quaternion curves" (SIGGRAPH 1985).
- Jason Gregory, *Game Engine Architecture*, animation chapters (skeletons, poses, blending).
- glTF 2.0 specification: skins, animation channels and samplers.
- Godot Engine 4 (MIT): skeleton modifier documentation, for M5's mapping (open question 15).
