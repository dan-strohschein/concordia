# concordia-graph-script: agent guide

## Purpose
Compiled gameplay graphs as script modules (M4 4.10b, J1): an Event `.graph.ron` loads as a `ScriptModule` and runs through the same `ScriptHost` as WASM and native scripts, with the same processor faults, fuel and reload.

## Public entry points
- `GraphBackend::new(Mode)`, `GraphBackend::load(&ModuleSource, &TypeRegistry)`, `GraphBackend::load_text(name, text, &TypeRegistry)`.
- `Mode::Wasm(WasmConfig)` (default: compiled, the scripts' Wasmtime settings and fuel) or `Mode::Interpret { budget }` (the IR interpreter).
- `GraphModule` (the `ScriptModule`).

## Invariants
- A graph compiles against the registry it loads with (component layouts, the node library); a graph that doesn't check or compile fails to load with every diagnostic in the error.
- The module declares one component, `graph.<name>.Vars` (the graph's variables); processors run only on entities that have it (attaching a graph = adding it).
- One call per processor per tick: the batch's columns go into the program's memory image as they are (arenas kept for written columns); each run gets a fresh instance.
- Compiled and interpreted runs give the same world (`interpreted_and_compiled_graphs_agree`).

## How to test
`cargo nextest run -p concordia-graph-script`

## References
ADR-0062 (the column protocol); the 4.10 plan's J1–J15. No engine source consulted.

## Status
M4 4.10b: Event graphs on their own entity; reload and the engine module come next (4.10b step 6).

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
