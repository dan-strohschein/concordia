# ufbx-sys: agent guide

## Purpose
Raw FFI to ufbx (FBX and OBJ loading) through Concordia's flat C shim (`shim/concordia_ufbx.{h,c}`), built with `cc` from the vendored sources. No safe API here; that is `concordia-import-fbx`'s `ffi` module.

## Public entry points
- `UFBX_VERSION`
- The shim's C API (`src/bindings.rs`, hand-written): `cu_load` / `cu_free`, `cu_settings_get`, `cu_node_*`, `cu_mesh_*` (triangulated corners per material part), `cu_skin_*` (clusters with geometry-to-bone matrices, per-vertex weights largest first), `cu_material_get` (ufbx's PBR values and texture file indices), `cu_texture_file_get`, `cu_stack_*` and `cu_bake_*` (stacks baked to linear keys), and the `cu_*` plain structs

## Invariants
- ufbx is pinned at v0.23.1 (`third_party/ufbx/VENDOR.md`) and unmodified; the build never downloads. Compiled as C11 (thread-safe scene refcounts).
- Every scene is loaded right-handed, Y-up, in meters with `MODIFY_GEOMETRY` space conversion and geometry transforms baked into meshes, so geometry, node transforms, skin matrices and keys all arrive in the engine's convention. External files are never opened (`load_external_files` off).
- Every entry point accepts null or out-of-range input and returns null, 0, `CU_NONE` or `false`; writers never write more than `cap` items; error messages are NUL-terminated within the caller's buffer. Nothing calls back into Rust.
- Strings and blobs the shim returns point into the scene and live until `cu_free`.
- `src/bindings.rs` mirrors the header by hand. `tests/smoke.rs` checks the header's blake3 against the `// header blake3:` line and every struct's size against the C compiler's (`cu_struct_size`).

## How to test
`cargo nextest run -p ufbx-sys` (uses `tests/fixtures/fbx`).

## Common tasks
- **Add a shim function:** declare it in the header, implement and bounds-check it in the `.c`, mirror it in `src/bindings.rs`, update the hash line (the test prints it) and `cu_struct_size` for new structs, test it in `tests/smoke.rs`, then wrap it in `concordia-import-fbx/src/ffi.rs`.
- **Upgrade ufbx:** replace `third_party/ufbx`, update `VENDOR.md` (tag, commit, SHA-256), and run the FBX import tests.

## Don'ts
- Don't expose ufbx's own structs through the bindings; flatten what Rust needs into `cu_*` structs.
- Don't enable `load_external_files`: textures are read by the importer through the asset database's sandboxed `read_relative`.

## References
- ufbx (MIT or Unlicense), vendored; its header documentation (ufbx.github.io). No Autodesk FBX SDK or other non-open source was consulted.

## Status
M2 phase 2.3d.

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
