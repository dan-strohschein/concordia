# concordia-script-guest

The guest SDK for Concordia gameplay modules (ADR-0062). Implement `Module` (`describe` + `run`), declare components and signals as ordinary `#[derive(Reflect)]` types, read and write whole wire columns through `RunCtx`, and record commands. The same code builds natively (in-process and native backends) and for `wasm32-wasip2`, where `export_module!` binds it to the WIT world `concordia:game/gameplay`.

See `AGENTS.md` for invariants, tests and references.
