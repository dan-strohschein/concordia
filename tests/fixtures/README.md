Test fixtures. `wasm/<name>/` holds standalone WASM guest crates built by `just wasm`.

- `fbx/`: FBX import fixtures (see its README).
- `audio/`: `tone-440.ogg` (M2 2.7a.1): 1 s of a 440 Hz sine at amplitude 0.5, mono, 48 kHz, made by `tools/fixtures/make_audio.sh` (Python's stdlib writes the WAV, `oggenc` encodes it; the output is byte-identical run to run).
- `m3-studio-game/`: the M3 gate's fixture game (3.9b): a player tagged `player` at the origin, a door tagged `door` at (0, 0, 10), a WASM gameplay module `scripts/player_controller/` (a standalone crate with its own lock, built per copy by `concordia_tests::m3_fixture`) with a seeded bug (`speed * dt * dt`), its playtest, task T-0001 for the gameplay programmer, the default Studio roster (one of each role) and `fix.patch` (the known-good fix, used only by the scripted gate).
- `character/`: the original fixture character (M2 2.4.5): a 17-bone box mannequin with five clips, its sidecar with clip events, and a five-state `.animgraph.ron` (see its README).
