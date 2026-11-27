# concordia-graph: agent guide

## Purpose
Text-first gameplay graphs (M4 4.9, D6): the `.graph.ron` format, the node library built from what the engine exposes, and the type checker. Compiling and running graphs is 4.10's.

## Public entry points
- `Graph`, `GraphKind`, `Node`, `Input`, `Literal`, `ExecLink`, `Signature`, `PinDecl`, `Variable`.
- `read_graph`, `write_graph` (canonical through `concordia-scene`), `Graph::semantic_hash`, `Graph::normalized`.

## Invariants
- Graph and node ids are UUIDv7 (checked on read and write).
- The printed text is canonical: maps and fields sorted, exec links sorted; `print(parse(print(g))) == print(g)`.
- `layout` is cosmetic: it never changes `semantic_hash`.

## How to test
`cargo nextest run -p concordia-graph`

## References
Unreal's public Blueprint documentation for UX expectations only (no code). No engine source consulted.

## Status
M4 4.9 in progress: the format.

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
