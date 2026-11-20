# concordia-scripting

The scripts engine module (M2 phase 2.6d): a project's gameplay modules, listed under `[scripts]` in `concordia.toml`, are loaded when the engine starts (their component, signal and trait types registered by name), run as processors in every play session, and reloaded from disk by `script.reload` (MCP `script_reload`). A reload migrates changed component types in the edit world as one undoable transaction by `script:<module>`, and in the play world, all or nothing.

See `AGENTS.md` for invariants, tests and references.
