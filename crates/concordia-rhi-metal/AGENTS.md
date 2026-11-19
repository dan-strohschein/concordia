# concordia-rhi-metal: agent guide

## Purpose
The Metal implementation of `concordia-rhi` (ADR-0040): `MetalDevice` over `objc2-metal` using the Metal 3 API, bindless argument-buffer tables and tracked hazards.

## Public entry points
- `new_headless()` → `Box<dyn Device>`; `MetalDevice::system_default()`, `MetalDevice::simulate_device_lost()` (test hook)
- `probe()` → `DeviceCaps` (used by `concordia gpu-probe`)
- `slots` (shader buffer slots: 0 buffers, 1 textures, 2 storage textures, 3 push constants), `BINDLESS_CAPACITY`

## Invariants
- Builds on every platform: Apple dependencies are `cfg(target_os = "macos")` only, and off macOS every entry point returns `RhiError::NoDevice`.
- Resources and heaps are hazard-tracked; bindless resources are declared with `useResource` on every encoder; an RHI barrier ends the current encoder. Push constants are re-applied whenever an encoder starts.
- `unsafe` stays in this crate; every block has a `// SAFETY:` comment. All mutable state is behind one Mutex.
- Unsupported on Metal 3 here: mesh pipelines and `R64Uint` (M4).

## How to test
`CONCORDIA_REQUIRE_GPU=1 cargo nextest run -p concordia-rhi-metal` (conformance suite and goldens in `tests/golden/rhi/`). Without the variable, the tests skip when there is no device.

## Common tasks
- **Translate a new command:** add it to `encode.rs` (choose or open the right encoder) and run the conformance suite.
- **Bless goldens:** `just bless`, then check the PNGs by eye.

## Don'ts
- Don't use metal-rs (deprecated).
- Don't create untracked resources until the Metal 4 path adds explicit barriers.

## References
- Apple *Metal Feature Set Tables*, *Metal Shading Language Specification* 3.x, WWDC 2022 "Go bindless with Metal 3"; `objc2-metal` (MIT/Zlib/Apache-2.0).

## Status
M1 phase 1.2b (surfaces and presentation arrive with rendering in 1.5).

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
