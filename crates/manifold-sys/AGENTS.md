# manifold-sys: agent guide

## Purpose
Raw FFI to Manifold's core through Concordia's C shim (`shim/concordia_manifold.{h,cpp}`), built with `cc` from the vendored sources, plus a checked wrapper (`safe`) so callers need no `unsafe`. Patterns and pieces live in `concordia-fracture`.

## Public entry points
- `safe::clip_convex(&Mesh, &[Plane]) -> Result<Piece, ManifoldError>`: a closed mesh clipped by the convex cell where every `normal · x ≤ offset`; the piece's triangles, an interior flag per triangle (on a cut) and its volume
- `ffi`: the shim's C API (`cm_clip_convex`, `cm_piece_*`), `MANIFOLD_VERSION`

## Invariants
- Manifold is pinned at v3.5.5 (`third_party/manifold/VENDOR.md`) and unmodified; the build never downloads.
- Build flags (`build.rs`, ADR-0180): C++17, `-O2` in every profile, `MANIFOLD_PAR=-1` (sequential, no TBB), `-ffp-contract=off`, sources sorted. No cross sections (no Clipper2).
- The shim validates every input (counts, indices, finite values, non-zero normals), catches every C++ exception and returns a `CmStatus`. Nothing calls back into Rust.
- Interior faces are the triangles whose run doesn't come from the source (`AsOriginal`'s ID).
- `src/bindings.rs` is hand-written; a test checks it against a blake3 of the shim header. Change both together and update the hash.

## How to test
`cargo nextest run -p manifold-sys`

## Common tasks
- **Add a shim function:** declare it in the header, implement and validate it in the `.cpp`, add it to `src/bindings.rs`, update the header hash, then wrap it in `safe` and test it in `tests/`.
- **Upgrade Manifold:** replace `third_party/manifold`, update `VENDOR.md` and `MANIFOLD_VERSION`, run the fracture tests on macOS and Linux, and record it in ADR-0180.

## Don'ts
- Don't turn on TBB or `MANIFOLD_PAR`: parallel reductions can change the floats.
- Don't vendor the cross-section module.
- Don't let a C++ exception cross the shim.

## References
- Manifold (Apache-2.0), vendored; its documentation (manifoldcad.org) and Emmett Lalish's papers and talks on its robust booleans.

## Status
M8 8.1a: `cm_clip_convex` and the piece accessors; the checked wrapper.

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
