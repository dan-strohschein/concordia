# Concordia editor

The Concordia editor: React panels (laid out like Godot) around a native Metal viewport, in a Tauri v2 shell. It is just another Engine API client; agents and the editor edit the same scene through the same API, live.

- `just editor`: open the native editor on `examples/m1-physics-stack`.
- `just editor-dev`: the UI in a browser, against `concordia headless --ws`.
- `just editor-test` / `just editor-shell-test`: UI, browser e2e and native shell tests.

See `AGENTS.md` and `docs/design/editor-ux.md`.
