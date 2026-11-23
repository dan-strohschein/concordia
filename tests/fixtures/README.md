Test fixtures. `wasm/<name>/` holds standalone WASM guest crates built by `just wasm`.

- `fbx/`: FBX import fixtures (see its README).
- `audio/`: `tone-440.ogg` (M2 2.7a.1): 1 s of a 440 Hz sine at amplitude 0.5, mono, 48 kHz, made by `tools/fixtures/make_audio.sh` (Python's stdlib writes the WAV, `oggenc` encodes it; the output is byte-identical run to run).
- `character/`: the original fixture character (M2 2.4.5): a 17-bone box mannequin with five clips, its sidecar with clip events, and a five-state `.animgraph.ron` (see its README).
