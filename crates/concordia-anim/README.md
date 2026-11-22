# concordia-anim

Concordia's animation runtime: skeletons built from imported skins (glTF, FBX), one `Pose` per character, clips bound to any skeleton by bone name and sampled per the glTF 2.0 specification, `AnimPlayer`s that advance deterministically at the simulation's fixed step, the CPU reference for skinning (the renderer skins on the GPU), blend graphs and state machines (`.animgraph.ron`, played by `AnimGraphPlayer`), clip events raised as signals to gameplay, items attached to bones (`BoneAttachment`), property tweens with Godot's easing (`Tweens`), and `anim.inspect` for agents. Enabled per project by `[anim]` in `concordia.toml`; Agent Studio's animator works through it. `tests/fixtures/character/` holds an original 17-bone fixture character with five clips, clip events and a five-state graph.

See `AGENTS.md` for invariants, tests and references.
