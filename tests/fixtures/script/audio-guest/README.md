# audio-guest

A gameplay module (`aud`) for `concordia-app`'s `tests/audio.rs` (M2 phase 2.7a.2). It shows a WASM guest playing a cue: `aud.chime` raises the engine's `PlayCue` signal on each entity with an `aud.Chime` every `every` ticks. `PlayCue` is mirrored field for field, as a game's scripts would. Built by `just wasm` as `audio_guest.wasm`.
