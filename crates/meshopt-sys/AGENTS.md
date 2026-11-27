# meshopt-sys: agent guide

## Purpose
Raw FFI to meshoptimizer (meshlet building, meshlet optimization, cluster bounds) built with `cc` from the vendored sources, plus a small checked wrapper (`safe`) so that callers such as `concordia-cluster` need no `unsafe` (M4 4.2, R5, Q1).

## Public entry points
- `safe::build_meshlets`, `safe::optimize_meshlet`, `safe::meshlet_bounds`, `safe::simplify` (M4 4.8a, GI proxies; checks every position, since meshoptimizer scales by all of them); `safe::Meshlets`, `safe::Simplified`, `safe::MAX_VERTICES`, `safe::MAX_TRIANGLES`
- `MeshoptError`, `MESHOPTIMIZER_VERSION`
- `ffi`: the hand-written C API (`meshopt_buildMeshlets`, `meshopt_buildMeshletsBound`, `meshopt_optimizeMeshlet`, `meshopt_computeMeshletBounds`, `meshopt_simplify`, the `meshopt_Meshlet` and `meshopt_Bounds` structs) and the shim's `cm_struct_size` / `cm_version`

## Invariants
- meshoptimizer is pinned at v1.3 (`third_party/meshoptimizer/VENDOR.md`) and unmodified; the build never downloads. Compiled as C++11 with `-O2` in every profile, `-ffp-contract=off` (the same floats on x86 and ARM), no exceptions or RTTI.
- The checked wrapper validates every input meshoptimizer asserts on or would read out of bounds (limits, index ranges, triangle-list lengths, finite positions, cone weight) and returns `MeshoptError`; it never panics on bad input. Every `unsafe` block has a `// SAFETY:` comment.
- Output is trimmed to what meshoptimizer wrote; meshlets are packed with no padding between them. Degenerate triangles may be dropped by meshoptimizer.
- `src/bindings.rs` mirrors the header by hand: `tests/smoke.rs` checks the header's blake3 against the `// header blake3:` line and the structs' sizes against the C++ compiler's.
- Never a dependency of a wasm or native guest (`repo_hygiene::wasm_guests_never_depend_on_sys_crates`).

## How to test
`cargo nextest run -p meshopt-sys`.

## Common tasks
- **Bind another function:** mirror it in `src/bindings.rs`, add a checked wrapper in `safe.rs` that validates every documented precondition, and test the wrapper's refusals in `tests/smoke.rs`.
- **Upgrade meshoptimizer:** replace `third_party/meshoptimizer`, update `VENDOR.md` (tag, commit, SHA-256), the version constant and test, re-check the bindings against the header and update its hash line; bump `concordia-cluster`'s meshlet build version (its cache key includes it).

## Don'ts
- Don't call the vertex or index encoders from several threads: meshoptimizer documents them as not thread safe (not bound yet).
- Don't let a caller reach `ffi` with unchecked input; add a wrapper instead.

## References
- meshoptimizer (MIT), vendored: its header documentation and README (Kapoulkine). No non-open-source engine code was consulted.

## Status
M4 phase 4.2.

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
