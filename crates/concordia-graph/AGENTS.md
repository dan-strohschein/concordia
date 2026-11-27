# concordia-graph: agent guide

## Purpose
Text-first gameplay graphs (M4 4.9, D6): the `.graph.ron` format, the node library built from what the engine exposes, and the type checker. Compiling and running graphs is 4.10's.

## Public entry points
- Format: `Graph`, `GraphKind`, `Node`, `Input`, `Literal`, `ExecLink`, `Signature`, `PinDecl`, `Variable`; `read_graph`, `write_graph` (canonical through `concordia-scene`), `Graph::semantic_hash`, `Graph::normalized`.
- Library: `NodeLibrary::core()` (built-in nodes and `fns::graph_fns()`), `NodeLibrary::build(registry, is_component)` (plus `get.`/`set.` per component and field, `raise.`/`on.` per signal), `NodeDef`, `PinDef`, `PinType`, `GenericKind`.
- Checker: `check(graph, library) -> Report` (`diagnostics`, `pin_type`), `Diagnostic`, `Code`.
- Edits (4.10c): `GraphEdit`, `apply_edits` (all or nothing), `new_graph`, `fresh_id`. Traces: `runtime::Program::trace_job` → `JobTrace { executed, pins }` (the unoptimized function).
- Compiler (4.10a): `compile_function(graph, library) -> CompiledFunction { ir, wasm, params, results }`; `ir::{Function, Inst, Scalar, ScalarTy, UnOp, BinOp}`; `Function::eval` is the interpreter.

## Invariants
- Graph and node ids are UUIDv7 (checked on read and write).
- The printed text is canonical: maps and fields sorted, exec links sorted; `print(parse(print(g))) == print(g)`.
- `layout` is cosmetic: it never changes `semantic_hash`.
- Library ops are stable names; the core's are snapshotted (`library__core_library.snap`): a change there is an API change for every graph.
- Every diagnostic names a code and, where it can, the node and pin to fix; suggestions come from `concordia_core::suggest::did_you_mean`.
- No pin name repeats on a node (signal fields that share a fixed pin's name become `field_<name>`).
- The IR is scalar (vectors are components); `ir/eval.rs` defines every operation once (WASM semantics with NaN canonicalization: canonical NaN from arithmetic, `neg`/`abs` as bit operations, f32 min/max NaN-propagating with −0 < +0, integer ÷0 = 0 and MIN ÷ −1 = MIN, saturating float→int). The interpreter and constant folding use it; the emitter must match it bit for bit (`compiled_graph_matches_interpreter`).

## How to test
`cargo nextest run -p concordia-graph`; the engine-wide library check is `cargo nextest run -p concordia-tests --test integration graph_library`. Example graphs live in `tests/graphs/{ok,broken}`; `CONCORDIA_GRAPH_CANONICALIZE=1` rewrites the `ok` ones in canonical form after an intended format change.

## Common tasks
- **Offer a Rust function to graphs:** `#[concordia_reflect::graph_node(category = "Vector", pure)]` on a free function, then list `<fn>_graph_node()` in the crate's `graph_fns()`; its op is `<category lowercased>.<fn>`.
- **Make a type a signal:** `#[reflect(signal)]` on it (script signals are marked when their module registers).
- **Add a core node:** add it to `core_nodes()` in `src/library.rs` and review the core snapshot.
- **Add a diagnostic:** a `Code` variant, the pass that finds it (`src/check/`), and a broken example graph.

## References
Unreal's public Blueprint documentation for UX expectations only (no code). Type checking follows the usual bidirectional approach (annotated signatures and variables, inference inward). Cytron et al. 1991 (SSA form) for the IR; the WebAssembly core specification (numeric semantics) for the interpreter and emitter. No engine source consulted.

## Status
M4 4.9: format, library and checker done. 4.10a: IR, interpreter and the WASM emitter for pure Function graphs. Running graphs as script modules, hot reload and MCP authoring are 4.10b–c.

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
