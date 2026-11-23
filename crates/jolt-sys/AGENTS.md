# jolt-sys: agent guide

## Purpose
Raw FFI to Jolt Physics through Concordia's C++ shim (`shim/concordia_jolt_ext.{h,cpp}`), built with CMake from the vendored sources. No safe API here; that is `concordia-physics`.

## Public entry points
- `init()` (idempotent; counted allocations), `jolt_version()`, `live_allocations()`
- The shim's C API (generated `src/bindings.rs`): `cj_world_*` (create, step, drain events, save/restore), `cj_body_*`, `cj_raycast`, `cj_character_*`, and the `Cj*` descriptor and state structs

## Invariants
- Jolt is pinned at v5.7.0 (`third_party/JoltPhysics/VENDOR.md`) and unmodified; the build never downloads.
- Build flags (`cmake/CMakeLists.txt`): `CROSS_PLATFORM_DETERMINISTIC=ON` (which implies `-ffp-contract=off`), single precision, `OBJECT_LAYER_BITS=16`, no GPU compute, ObjectStream, debug renderer or profiler, always CMake `Release`.
- Object layer = user layer × 2 + moving bit; 16 user layers with a symmetric collide matrix; static never collides with static.
- Contact events are recorded under a lock as one begin/end per body pair and drained after the step; callers sort them. Nothing calls back into Rust.
- Every shim entry point validates its input and returns null, `CJ_NO_BODY` or `false` instead of crashing.
- `src/bindings.rs` is generated and committed. A test checks it against a blake3 of the shim header; regenerate with `just jolt-bindgen` (needs libclang, e.g. Xcode's).

## How to test
`cargo nextest run -p jolt-sys`

## Common tasks
- **Add a shim function:** declare it in the header, implement and validate it in the `.cpp`, run `just jolt-bindgen`, then test it in `tests/smoke.rs` and wrap it in `concordia-physics`.
- **Upgrade Jolt:** replace `third_party/JoltPhysics`, update `VENDOR.md`, run the physics determinism tests on macOS and Linux, and record it in ADR-0041.

## Don'ts
- Don't edit `src/bindings.rs` by hand.
- Don't add callbacks from Jolt worker threads into Rust; batch in the shim instead.
- Don't enable Jolt's GPU compute options.

## References
- Jolt Physics (MIT), vendored; Jolt documentation and samples (jrouwe.github.io/JoltPhysics); Jorrit Rouwé, "Architecting Jolt Physics for Horizon Forbidden West" (GDC 2022).

## Status
M1 phase 1.7a (world, bodies, events, raycasts, state, characters exposed for 1.7b). M2 phase 2.10a: `CjCharacterDesc::user`; characters get an inner body for sensors, which the contact listener's `OnContactValidate` keeps away from non-sensors and raycasts and character moves skip (`NotInner`); bindings regenerated.

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
