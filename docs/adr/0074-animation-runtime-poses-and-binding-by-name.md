# ADR-0074: Animation runtime: one pose per character, clips bound by bone name, sampled in FrameEnd
- Status: Accepted (M2 phase 2.4.1, 2026-11-21)
- Date: 2026-11-21
- Builds on: ADR-0046 (engine modules), ADR-0063 (determinism contract: phases, presentation state, time scale), ADR-0067 (M2 refinement), ADR-0069 (command buffers at phase ends)

## Context
The glTF importer (M1) and the FBX importer (2.3d) already store skins (`SkinData`) and clips (`ClipData`), and meshes carry four joint weights per vertex. Nothing samples or plays them. Before skinning (2.4.2), blend graphs (2.4.3) and events (2.4.4) can build on a runtime, four questions must be settled:
- where a character's pose lives;
- how a clip finds the bones it drives, when `prefab.instantiate` gives every entity a fresh id and a clip may come from another file (a Mixamo-style "animation only" download);
- which phase animation runs in, given that gameplay depends on animation timing;
- what the importers must add.

The author decided the first and the last (M2 plan, decisions A1 and A2, 2026-11-21).

## Decision
1. **One `Pose` per character (A2).** A `Pose` component on the skinned entity holds every joint's local transform, in the skin's joint order. It is ordinary simulation state: it is hashed and saved like any component. The bone entities a model import creates stay in the prefab as rest-pose handles and name anchors. Animation never writes them. Items follow posed bones through `BoneAttachment` (2.4.4), and `anim.inspect` (2.4.4) reports live bone transforms. This is Godot's `Skeleton3D` model and the pose arrays of Gregory's *Game Engine Architecture*. It keeps crowds cheap, and it is what GPU skinning reads.
2. **Skeletons come from the skin and its own prefab,** and every instance shares them. A joint's parent is its nearest ancestor that is also a joint. The fixed transforms of non-joint nodes in between (an `Armature` node, Blender's ×100 export scale) become the joint's `offset`, so clips keep animating each node's own local transform. Model space is the prefab root's space.
3. **Clips bind by bone name.** Each channel carries its node's name (`Channel::target_name`, A1). A clip binds to any skeleton whose joints have those names, and channels with no matching bone are reported (`BoundClip::unmatched`), not errors. Morph-target weight channels are skipped and counted until morph targets exist.
4. **Sampling follows the glTF 2.0 specification, Appendix C:**
   - linear interpolation for translation and scale, slerp along the shortest path for rotation;
   - step holds the previous key;
   - the cubic Hermite spline scales tangents by each segment's length;
   - values hold before the first key and after the last;
   - rotation values are normalized at bind time.
   `validate_clip` refuses non-finite or non-increasing times, value counts that don't fit the path, zero rotations and bad durations, and each error names the clip, channel and bone.
5. **Playback runs in `FrameEnd` at the fixed step.** By then, gameplay in `PostPhysics` has set this tick's players (and, from 2.4.3, graph parameters), so changes take effect the same tick.
   - `anim.sample` advances `AnimPlayer::time` by `dt × speed`, where `dt` is already scaled by `SimClock::time_scale`, so hit-stop freezes animation. Looping clips wrap; others clamp.
   - It then writes the pose from the rest pose plus the clip, in parallel in fixed chunks of 64 entities, so the hash is the same at every thread count.
   - `anim.pose` first gives a rest pose to players spawned during play; the pose appears through the command buffer at the phase end, and sampling starts the next tick.
   - Events (2.4.4) will be Terra signals, delivered next tick.
   - The renderer interpolates between the last two poses (2.4.2), as it already does for transforms.
6. **The library is loaded once, when play starts.** The `[anim]` module (`concordia.toml`, needs `[assets]`) loads every skin a `Skinned` entity uses. For each `AnimPlayer`, it also loads every clip in the same file as its clip, bound to that player's skeleton. What fails to load is logged and skipped, and its players hold their pose. The loaded set depends only on the edit world, so behavior never depends on when loading happens.
7. **Importer changes (A1), one re-import:**
   - A renderer entity drawing a skinned mesh gets `Skinned { skin: "asset:<id>#skin/<index>" }`, and so do its extra parts.
   - Channels carry `target_name`.
   - `ClipData::frame_rate` is the authored rate for FBX, and `None` for glTF, which has none.
   - glTF goes to importer version 3 and FBX to version 2, so the derived data cache re-imports every model once.
   - The FBX start-time trim (stacks rebased to 0) is now tested with a hand-written text-format FBX fixture.
   - glTF clip times stay as authored: a first key after 0 holds until it is reached.

## Alternatives
- **Drive each bone entity's `Transform`.** Attaching an item would just be parenting, and `component.get` would show live bones. But each character would cost dozens of entity writes per tick, crowds would pay full price, GPU skinning would gather matrices from scattered entities, and the same state would live in two places once poses blend. The author chose the pose (A2).
- **Bind by entity id through the clip file's prefab.** This needs no importer change, but every bind would load the clip's source prefab, and a clip would not be usable without its file. The author chose names on channels (A1).
- **A separate `Sim` phase.** ADR-0063's phases already order gameplay before `FrameEnd`, and a new phase would change every module's schedule for no gain.
- **Load clips lazily when a player names a new one.** Whether a clip was ready would then depend on when `after_sim` ran relative to ticks, so `sim.step 10` and ten `sim.step 1` calls could differ. Loading at play start avoids that.

## Consequences
- `EngineModule::start_play_in` takes `&mut Engine` (a module may load assets when play starts), and `Engine::registry_and_state_mut` borrows the registry beside a module's state. `AssetDb::load_prefab_doc` and `prefab_doc` are now the one way prefab text is read: `prefab.instantiate` and `concordia_import_model::load_prefab_doc` use them.
- Every skinned model re-imports once. glTF and FBX models without skins or clips get new artifacts with the same content (the importer version is part of the cache key).
- A player whose clip isn't in its file's loaded set holds its pose. In 2.4.3, the graph asset names every clip it can play, so the library can load graphs the same way.
- Nothing renders a pose yet; 2.4.2 adds CPU and GPU skinning.
- **2.4.2 (2026-11-21):** the renderer draws poses as this ADR planned, with no new decision of its own:
  - The `[anim]` module puts its library in the play world as a resource (`Arc<AnimLibrary>`, not hashed), so the renderer can find skeletons.
  - A skinned draw takes its own pose or its nearest posed ancestor's with the same skin, and is placed at that entity's world matrix (glTF §3.7.3 ignores the skinned mesh node's transform).
  - The palette is `inverse(bindMesh) · jointModel · inverseBind`, built on the CPU from the pose interpolated between ticks.
  - Vertices are skinned in the forward and shadow vertex shaders.
  - Rendering never writes the world. See the M2 plan's 2.4.2 As built.
