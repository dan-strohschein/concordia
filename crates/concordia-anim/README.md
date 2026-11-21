# concordia-anim

Concordia's animation runtime: skeletons built from imported skins (glTF, FBX), one `Pose` per character, clips bound to any skeleton by bone name and sampled per the glTF 2.0 specification, `AnimPlayer`s that advance deterministically at the simulation's fixed step, the CPU reference for skinning (the renderer skins on the GPU), and blend graphs and state machines (`.animgraph.ron`, played by `AnimGraphPlayer`). Enabled per project by `[anim]` in `concordia.toml`.

See `AGENTS.md` for invariants, tests and references.
