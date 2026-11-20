# concordia-script-protocol

The script protocol's types (ADR-0062): what a gameplay module declares (`ModuleDesc` with component, signal and helper `TypeDesc`s, traits and processors) and what it exchanges with the engine each tick (`TickContext`, `Batch` of wire columns, `RunOutput` of column writes and commands). Shared by the host (`concordia-script`) and the guest SDK (`concordia-script-guest`); small enough to build for `wasm32-wasip2`.

See `AGENTS.md` for invariants, tests and references.
