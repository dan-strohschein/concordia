# concordia-anim: agent guide

## Purpose
Animation (M2 phase 2.4, ADR-0074): skeletons built from imported skins, one hashed `Pose` per character, clips bound to skeletons by bone name and sampled per the glTF 2.0 specification, `AnimPlayer`s advanced at Terra's fixed step in `FrameEnd`, and linear blend skinning on the CPU, the reference for the renderer's GPU skinning (2.4.2).

## Public entry points
- `Skeleton` (`new` from `Joint`s, `from_skin(&SkinData, &PrefabDoc)`, `with_mesh` / `mesh` (the bind mesh's model matrix), `order`, `index_of`, `rest_pose`, `reset`, `model_space`, `joint_palette`, `palette(&Pose)`, `check`), `Joint` (`name`, `parent`, `offset`, `rest`, `inverse_bind`), `model_matrix(&PrefabDoc, EntityId)`, `bind_mesh(&PrefabDoc, &AssetRef)`
- `Pose` (component: `bones`, local transforms in skin joint order; `lerp` blends bone by bone)
- Skinning (2.4.2): `skin_vertices(&MeshData, palette)` → `SkinnedVertices` (positions, unit normals and tangents), `joint_bounds(&MeshData)` → `JointBounds`, `skinned_bounds(&JointBounds, palette)`
- `validate_clip(&ClipData)`, `BoundClip` (`bind`, `sample(time, &mut Pose)`, `duration`, `unmatched`, `skipped`)
- `AnimPlayer` (component: `clip`, `time`, `speed`, `looping`; `new`), `advance(time, delta, duration, looping)`
- `AnimLibrary` (`load(&mut AssetDb, &TypeRegistry, &mut World)` → `LoadReport`; `insert_skeleton`, `insert_clip`, `skeleton`, `shared_skeleton`, `clip`), `insert_rest_poses`, `add_processors` (`anim.pose`, `anim.sample`; `POSE`, `SAMPLE`, `CHUNK`)
- `AnimModule` (`[anim]`; needs `[assets]`), `register_types`, `AnimError`

## Invariants
- Joint indices are the skin's (vertex `joints` refer to them); `Skeleton::order` puts parents first. Names are unique and non-empty; a bad parent or a cycle is an error naming the joint.
- Model space is the prefab root's. A joint's matrix = parent joint's (or identity) × `offset` (the fixed non-joint nodes between them) × its local pose. At rest, `joint_palette` is the identity (tested on `SimpleSkin` and the FBX rig).
- Sampling: linear (slerp on the shortest path for rotations), step, cubic Hermite with tangents scaled by the segment's length; values hold outside the keys; rotations are normalized. Weight channels are skipped and counted, unknown bones listed, never errors.
- `anim.sample` runs in `FrameEnd`: time advances by `dt × speed` (dt already scaled by `SimClock::time_scale`), wraps when looping and clamps otherwise, then the pose is the rest pose plus the clip. It runs in parallel in `CHUNK`-entity pieces, so hashes match at every thread count.
- `Pose` and `AnimPlayer` are simulation state (hashed). Animation writes only the play world's `Pose`s and `AnimPlayer`s, never bone entities or the edit world.
- The library is fixed when play starts: every skin a `Skinned` entity uses, and every clip of each player's clip file, bound to that player's skeleton. Failures are logged (`concordia::anim`) and those players hold their pose. `AnimModule` also puts it in the play world as a resource (`Arc<AnimLibrary>`), where the renderer finds skeletons; resources other than the clock and RNG are not hashed.
- Palettes (2.4.2): `K_j = inverse(mesh) · jointModel_j · inverseBind_j` (glTF §3.7.3 with both globals relative to the prefab root), where `mesh` is the bind mesh's model matrix: the prefab's topmost entity whose `Skinned` names the skin (`bind_mesh`). At rest every `K_j` is the identity (tested on `SimpleSkin` and the FBX rig).
- Skinning: a vertex's matrix is the weighted sum of its four joints' (weights as stored; importers normalize them). Positions and tangents use the matrix, normals its cofactor (the inverse transpose up to scale, exact under non-uniform scale); both directions are normalized, and a zero length keeps the input. The bitangent sign is kept. Every joint index must be inside the palette, or skinning is an error naming the vertex.
- Bounds: `joint_bounds` keeps, per joint, the bind-space box of the vertices it influences with a positive weight; `skinned_bounds` is the union of each box moved by its matrix, which contains every skinned vertex (each is a weighted average of `K_j · v`).

## How to test
`just gltf-assets && cargo nextest run -p concordia-anim` (FBX fixtures in `tests/fixtures/fbx`, glTF samples fetched; `tests/skinning.rs` for 2.4.2). `concordia-app/tests/anim.rs` drives `[anim]` through the Engine API. Bench: `just bench -p concordia-anim` (`benches/BASELINE.md`).

## Common tasks
- **Change skinning:** change `skin.rs` and the renderer's `shaders/skin.slang` together; `cpu_skinning_matches_reference_vertices` holds hand-computed values, and `concordia-render`'s `gpu_skinning_matches_cpu_skinning` compares the two within 1e-4.
- **Play a clip on an imported character:** `prefab.instantiate` the model, then set `AnimPlayer { clip: "asset:<id>#clip/<name>" }` on the entity with `Skinned`; the clip may come from another file with the same bone names.
- **Add an interpolation or path:** extend `validate_clip`'s widths and `clip.rs`'s sampler together, with a test of hand-computed values.

## Don'ts
- Don't write bone entities' `Transform`s from animation (ADR-0074: the pose is the one source).
- Don't load assets from processors; extend `AnimLibrary::load`, which runs once when play starts.
- Don't derive chunk sizes from the thread count.

## References
- Khronos glTF 2.0 specification: §3.7 (skins, joint matrices, §3.7.3 skinned mesh attributes) and Appendix C (animation interpolation, cubic Hermite splines).
- Jason Gregory, *Game Engine Architecture* (3rd ed.), ch. 12 "Animation Systems": skeletons, poses as joint arrays, local and model space, the skinning matrix palette, linear blend skinning.
- Normals under skinning: the cofactor matrix as the inverse transpose up to scale (standard linear algebra; e.g. Eric Lengyel, *Mathematics for 3D Game Programming and Computer Graphics*, transforming normal vectors). Box bounds moved by a matrix: James Arvo, "Transforming Axis-Aligned Bounding Boxes" (Graphics Gems, 1990).
- Godot Engine documentation (MIT): `Skeleton3D` and `BoneAttachment3D` (one pose per skeleton; attachments follow bones). Concepts only, no code copied.
- glam (MIT/Apache) for quaternion slerp. No Unreal source consulted.

## Status
M2 phase 2.4.1 (skeletons, poses, sampling and players); 2.4.2 (CPU reference skinning, palettes, bounds, pose blending). Next: blend graphs (2.4.3), events, attachments, tweens and tools (2.4.4), the animator role (2.4.5).

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
