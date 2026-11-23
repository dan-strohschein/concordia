# input-guest

A gameplay module (`inputg`) for `concordia-app`'s `tests/input_guest.rs` (M2 phase 2.10a). It shows a script reading player input: `inputg.walk` runs in `PrePhysics` after `input.actions`, reads the engine's `InputActions` (mirrored field for field, as a game's scripts would), writes `CharacterMotion` from the `camera_pan` axis times its `inputg.Walker` speed, and counts `spawn_box` presses. Built by `just wasm` as `input_guest.wasm` and by `just native-fixtures` (feature `dylib`) as a native library.
