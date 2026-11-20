# concordia-script-protocol: agent guide

## Purpose
The script protocol's data types (ADR-0062), shared by the engine side and the guest SDK. Pure data plus structural validation and the packed-id form; no engine dependencies, so guests build small. Every backend (WASM, native, in-process) is a binding of these types; the WIT world `wit/concordia-game` is the WASM one.

## Public entry points
- `ModuleDesc` (`validate`), `ProcessorDesc`, `TraitDesc`, `Phase`
- `TickContext`, `Batch` (`column`, `signal`), `Column`, `SignalColumn`
- `RunOutput`, `Command` (`Spawn`, `SpawnTemplate`, `Insert`, `Remove`, `Despawn`, `Signal`), `Target` (`Entity`, `Pending`), `Value`
- `ids_to_bytes` / `ids_from_bytes` (16 big-endian UUID bytes per id), `ProtocolError`, `PROTOCOL_VERSION`

## Invariants
- Component data is always wire-encoded (`concordia_reflect::wire`): a `Column` is `rows × stride` bytes then the arena; a `Value` is a one-row column.
- A batch's columns are the processor's reads then writes, one row per id; signal columns are per received type, by entity then raise order.
- `Target::Pending(n)` names the n-th spawn (`Spawn` or `SpawnTemplate`) of the same run output.
- `validate` checks names (non-empty, unique per kind), that a processor names a component at most once across reads, writes, with and without, and that traits add at least one component, each once. Type shapes are checked when registered.
- Packed ids must be valid entity ids (UUID v7 or v5); anything else is an error, never a panic.

## How to test
`cargo nextest run -p concordia-script-protocol`. The conformance suite (`concordia-script`) exercises the types end to end.

## Common tasks
- **Add a command:** add the variant here, to the WIT `command` variant, to both bindings' conversions (`concordia-script-guest/src/wasm.rs`, `concordia-script-wasm/src/convert.rs`), to the SDK's `RunCtx`, and to `ScriptProcessor::stage_commands`; extend the conformance module and suite.

## Don'ts
- Don't add engine dependencies (Terra, scene): guests compile this crate.
- Don't change a type's meaning without bumping `PROTOCOL_VERSION` and the WIT package version.

## References
- WebAssembly Component Model and WIT documentation (component-model.bytecodealliance.org) for the binding's shape; the protocol itself is our own (ADR-0062).
- Unreal Engine's public Mass documentation on processors, queries and deferred commands, concepts only (clean room, ADR-0067).

## Status
M2 phase 2.6b complete (2026-11-20).

## Engineering rules
These apply to every change, by humans and agents alike.
1. **TDD on every phase.** Write the tests first, see them fail, implement, then refactor.
2. **SOLID.** Depend on abstractions at crate boundaries; extend through traits rather than by editing callers.
3. **No duplicate functionality.** Before writing any implementation, search for existing functionality (grep, `cargo doc`, crate `AGENTS.md` files). Extend or refactor what exists; never write a parallel version.
4. **Single responsibility at every level:** function, type, module and crate.
5. **Small functions** that do one thing.
6. **Validate parameters.** Every function checks its inputs:
   - Rust: encode invariants in types (newtypes, enums, `NonZero*`); validate at boundaries and return `Result` with a descriptive error; use `debug_assert!` for internal invariants. Public APIs never panic on bad input.
   - TypeScript: strict types, plus runtime validation at API and IPC boundaries.
7. **Short comments.** One-line doc comments on public items; no essays, and no comments that restate the code.
8. **Idiomatic code.**
   - Rust: follow the Rust API Guidelines and keep it clippy-clean; `thiserror` in libraries, `anyhow` only in binaries and tests.
   - Editor UI: idiomatic React and TypeScript, with function components, hooks, strict mode, ESLint and Prettier.
