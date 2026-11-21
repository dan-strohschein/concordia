# concordia-anim: agent guide

## Purpose
Animation (M2 phase 2.4, ADR-0074): skeletons built from imported skins, one hashed `Pose` per character, clips bound to skeletons by bone name and sampled per the glTF 2.0 specification, `AnimPlayer`s advanced at Terra's fixed step in `FrameEnd`, linear blend skinning on the CPU, the reference for the renderer's GPU skinning (2.4.2), and blend graphs and state machines from `.animgraph.ron` files with Godot `AnimationTree` semantics (2.4.3).

## Public entry points
- `Skeleton` (`new` from `Joint`s, `from_skin(&SkinData, &PrefabDoc)`, `with_mesh` / `mesh` (the bind mesh's model matrix), `order`, `index_of`, `rest_pose`, `reset`, `model_space`, `joint_palette`, `palette(&Pose)`, `check`), `Joint` (`name`, `parent`, `offset`, `rest`, `inverse_bind`), `model_matrix(&PrefabDoc, EntityId)`, `bind_mesh(&PrefabDoc, &AssetRef)`
- `Pose` (component: `bones`, local transforms in skin joint order; `lerp` blends bone by bone)
- Skinning (2.4.2): `skin_vertices(&MeshData, palette)` → `SkinnedVertices` (positions, unit normals and tangents), `joint_bounds(&MeshData)` → `JointBounds`, `skinned_bounds(&JointBounds, palette)`
- `validate_clip(&ClipData)`, `BoundClip` (`bind`, `sample(time, &mut Pose)`, `duration`, `unmatched`, `skipped`)
- `AnimPlayer` (component: `clip`, `time`, `speed`, `looping`; `new`), `advance(time, delta, duration, looping)`
- Graphs (2.4.3): `BoundGraph` (`bind(&AnimGraph, &Skeleton, clips)`, `initial_state`, `evaluate(dt, &mut AnimGraphPlayer, &mut AnimGraphState, &Skeleton, &mut Pose)`, `active_state`, `unmatched`); `AnimGraphPlayer` (component: `graph`, `floats`, `ints`, `bools`, `triggers`, `travel`; `new`); `AnimGraphState` / `NodeState` (component: per-node playback, in node name order). The format (`AnimGraph`, `read_animgraph`, `write_animgraph`, validation) is `concordia-assets`'.
- `AnimLibrary` (`load(&mut AssetDb, &TypeRegistry, &mut World)` → `LoadReport`; `insert_skeleton`, `insert_clip`, `insert_graph`, `skeleton`, `shared_skeleton`, `clip`, `graph`), `insert_rest_poses` (also graph states), `add_processors` (`anim.pose`, `anim.sample`; `POSE`, `SAMPLE`, `CHUNK`)
- `AnimModule` (`[anim]`; needs `[assets]`), `register_types`, `AnimError`

## Invariants
- Joint indices are the skin's (vertex `joints` refer to them); `Skeleton::order` puts parents first. Names are unique and non-empty; a bad parent or a cycle is an error naming the joint.
- Model space is the prefab root's. A joint's matrix = parent joint's (or identity) × `offset` (the fixed non-joint nodes between them) × its local pose. At rest, `joint_palette` is the identity (tested on `SimpleSkin` and the FBX rig).
- Sampling: linear (slerp on the shortest path for rotations), step, cubic Hermite with tangents scaled by the segment's length; values hold outside the keys; rotations are normalized. Weight channels are skipped and counted, unknown bones listed, never errors.
- `anim.sample` runs in `FrameEnd`: time advances by `dt × speed` (dt already scaled by `SimClock::time_scale`), wraps when looping and clamps otherwise, then the pose is the rest pose plus the clip. It runs in parallel in `CHUNK`-entity pieces, so hashes match at every thread count.
- `Pose`, `AnimPlayer`, `AnimGraphPlayer` and `AnimGraphState` are simulation state (hashed). Animation writes only the play world's copies, never bone entities or the edit world.
- Graphs (2.4.3), `graph.rs` and `graph_eval.rs`:
  - Nodes are indexed in name order, and each has one parent (validated), so per-node state is unambiguous.
  - Every evaluation writes a full pose (clips reset to rest, then sample). Blends are local-space: lerp, shortest-path slerp, lerp. `BlendN` uses the normalized weighted sum with rotations flipped into the first input's hemisphere.
  - Every blend input advances every tick; inactive transition inputs and states don't. A fade drops its source on the tick it completes, without advancing it.
  - A one-shot's weight is the curve of min(elapsed / fade_in, remaining / fade_out). It ends when the shot ends or first wraps.
  - A state machine resolves travel first (a breadth-first search over transitions that aren't disabled, edges in (priority, file order); teleport when unreachable), then takes at most one transition (the travel step, else the first `Auto` whose condition holds). `AtEnd` waits until the current state ended last tick, `Sync` seeks to its position, `reset` restarts, and a transition taken mid-fade drops the old fade's source (Godot 4). The new state is evaluated the same tick.
  - `evaluate` clears `triggers` and `travel` after every run, and resets a state of the wrong size or with out-of-range indices.
- `anim.sample` evaluates graph players first, then clip players that have no graph (a sorted id set; views have no filtered queries). Both run in `CHUNK` pieces.
- The library is fixed when play starts: every skin a `Skinned` entity uses, every clip of each player's clip file, and each graph player's graph and the clips it names, bound to that entity's skeleton. An entity with both players is reported, and its graph plays. Graph files edited during play apply at the next play. Failures are logged (`concordia::anim`) and those players hold their pose. `AnimModule` also puts it in the play world as a resource (`Arc<AnimLibrary>`), where the renderer finds skeletons; resources other than the clock and RNG are not hashed.
- Palettes (2.4.2): `K_j = inverse(mesh) · jointModel_j · inverseBind_j` (glTF §3.7.3 with both globals relative to the prefab root), where `mesh` is the bind mesh's model matrix: the prefab's topmost entity whose `Skinned` names the skin (`bind_mesh`). At rest every `K_j` is the identity (tested on `SimpleSkin` and the FBX rig).
- Skinning: a vertex's matrix is the weighted sum of its four joints' (weights as stored; importers normalize them). Positions and tangents use the matrix, normals its cofactor (the inverse transpose up to scale, exact under non-uniform scale); both directions are normalized, and a zero length keeps the input. The bitangent sign is kept. Every joint index must be inside the palette, or skinning is an error naming the vertex.
- Bounds: `joint_bounds` keeps, per joint, the bind-space box of the vertices it influences with a positive weight; `skinned_bounds` is the union of each box moved by its matrix, which contains every skinned vertex (each is a weighted average of `K_j · v`).

## How to test
`just gltf-assets && cargo nextest run -p concordia-anim` (FBX fixtures in `tests/fixtures/fbx`, glTF samples fetched; `tests/skinning.rs` for 2.4.2). `tests/graph.rs` (hand-built two-bone skeleton; every expected pose hand-computed) and `tests/graph_play.rs` (the FBX rig through `SimLoop`) cover 2.4.3. `concordia-app/tests/anim.rs` drives `[anim]` through the Engine API, including graphs and play-world patches. Bench: `just bench -p concordia-anim` (`benches/BASELINE.md`).

## Common tasks
- **Change skinning:** change `skin.rs` and the renderer's `shaders/skin.slang` together; `cpu_skinning_matches_reference_vertices` holds hand-computed values, and `concordia-render`'s `gpu_skinning_matches_cpu_skinning` compares the two within 1e-4.
- **Play a clip on an imported character:** `prefab.instantiate` the model, then set `AnimPlayer { clip: "asset:<id>#clip/<name>" }` on the entity with `Skinned`; the clip may come from another file with the same bone names.
- **Drive a character with a graph:** write an `.animgraph.ron` (clips as asset references), import it, and set `AnimGraphPlayer { graph: "asset:<id>" }` on the `Skinned` entity. Steer a running game with `component.patch` and `world: "play"` (ADR-0075).
- **Add a graph node:** add the variant to `concordia_assets::AnimNode` (with `children` and validation), bind it in `graph.rs`, evaluate it in `graph_eval.rs` (return its `Playback`), and add hand-computed tests in `tests/graph.rs`.
- **Add an interpolation or path:** extend `validate_clip`'s widths and `clip.rs`'s sampler together, with a test of hand-computed values.

## Don'ts
- Don't write bone entities' `Transform`s from animation (ADR-0074: the pose is the one source).
- Don't load assets from processors; extend `AnimLibrary::load`, which runs once when play starts.
- Don't derive chunk sizes from the thread count.
- Don't iterate hash maps in evaluation; parameters, states and nodes are `BTreeMap`s or sorted indices.

## References
- Khronos glTF 2.0 specification: §3.7 (skins, joint matrices, §3.7.3 skinned mesh attributes) and Appendix C (animation interpolation, cubic Hermite splines).
- Jason Gregory, *Game Engine Architecture* (3rd ed.), ch. 12 "Animation Systems": skeletons, poses as joint arrays, local and model space, the skinning matrix palette, linear blend skinning.
- Normals under skinning: the cofactor matrix as the inverse transpose up to scale (standard linear algebra; e.g. Eric Lengyel, *Mathematics for 3D Game Programming and Computer Graphics*, transforming normal vectors). Box bounds moved by a matrix: James Arvo, "Transforming Axis-Aligned Bounding Boxes" (Graphics Gems, 1990).
- Godot Engine documentation (MIT): `Skeleton3D` and `BoneAttachment3D` (one pose per skeleton; attachments follow bones). Concepts only, no code copied.
- Graphs (2.4.3): Gregory, *Game Engine Architecture* (3rd ed.), ch. 12: LERP blending, cross-fades, partial-skeleton (masked) blends, animation state machines and transitions. Godot Engine documentation (MIT), for semantics only, no code copied: `AnimationTree`, `AnimationNodeStateMachine`, `AnimationNodeStateMachineTransition` (switch modes immediate/sync/at end, advance modes disabled/enabled/auto, advance conditions, `reset`, `priority`, `xfade_time`), `AnimationNodeStateMachinePlayback` (`travel` along the shortest path, teleport when unreachable), `AnimationNodeOneShot` (fades ending at the shot's end, `request` cleared after processing), `AnimationNodeTransition` (per-input reset, cross-fade), `AnimationNodeBlend2` (filters; playback from the heavier input) and `AnimationNodeTimeScale`. Godot's A* weighting by editor positions was not adopted (our text has no positions). Normalized weighted quaternion sums with hemisphere alignment: standard practice (e.g. Gregory's quaternion blending).
- glam (MIT/Apache) for quaternion slerp. No Unreal source consulted.

## Status
M2 phase 2.4.1 (skeletons, poses, sampling and players); 2.4.2 (CPU reference skinning, palettes, bounds, pose blending); 2.4.3 (blend graphs and state machines, graph players and state). Next: events, attachments, tweens and tools (2.4.4), the animator role (2.4.5).

## Engineering rules
These apply to every change, by humans and agents alike.
1. **TDD on every phase.** Write the tests first, see them fail, implement, then refactor.
2. **SOLID.** Depend on abstractions at crate boundaries; extend through traits rather than by editing callers.
3. **No duplicate functionality.** Before writing any implementation, search for existing functionality (grep, `cargo doc`, crate `AGENTS.md` files). Extend or refactor what exists; never write a parallel version.
4. **Single responsibility at every level:** function, type, module and crate.
5. **Small functions** that do one thing.
6. **Validate parameters.** Every function checks its inputs:
   - Rust: encode invariants in types (newtypes, enums, `NonZero*`); validate at boundaries and return `Result` with a descriptive error; use `debug_assert!` for internal invariants. Public APIs never panic on bad input.
   - TypeScript: strict types, plus runtime validation at API and IPC boundaries.
7. **Short comments.** One-line doc comments on public items; no essays, and no comments that restate the code.
8. **Idiomatic code.**
   - Rust: follow the Rust API Guidelines and keep it clippy-clean; `thiserror` in libraries, `anyhow` only in binaries and tests.
   - Editor UI: idiomatic React and TypeScript, with function components, hooks, strict mode, ESLint and Prettier.
