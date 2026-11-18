# concordia-rhi: agent guide

## Purpose
Backend-neutral GPU vocabulary and the `Device` trait (ADR-0040): validated descriptors, typed handles, command lists as data, sync2-style barriers, bindless indices, capabilities, and the conformance suite every backend runs.

## Public entry points
- `Device` (object-safe), `Submit`, `QueueKind`
- `CommandList` (`push`, `validate`) and `Command`, `RenderPassDesc`, `ColorAttachment`, `DepthAttachment`, `LoadOp`, `StoreOp`, `ClearValue`
- Descriptors: `BufferDesc`, `TextureDesc`, `HeapDesc` (`validate`), `ComputePipelineDesc`, `GraphicsPipelineDesc`, `MeshPipelineDesc`, `ShaderSource`
- Handles: `Buffer`, `Texture`, `Pipeline`, `Heap`, `Timeline`, `BindlessIndex`; `Slots<T>` for backends
- `Access`, `BufferBarrier`, `TextureBarrier`, `Format`, `DeviceCaps`, `Requirement`, `RhiError`
- `conformance` (feature `conformance`): `conformance_tests!(make, golden, shaders)`, `Harness`, `GoldenCheck`, `ShaderProvider`, `shader_dir()` (Slang sources in `conformance/*.slang`)

## Invariants
- No graphics-API types here; backends translate.
- Every descriptor and command list is validated before a backend sees it; public APIs return `RhiError` and never panic on bad input.
- Handles are generation-checked: a destroyed resource's handle is `StaleHandle`, never another resource.
- Shaders see resources only through bindless indices (buffer table, texture table) and ≤ 128 bytes of push constants.
- `DeviceCaps::feature_sets` is sorted; `SampleCount` holds only 1/2/4/8.

## How to test
`cargo nextest run -p concordia-rhi` (GPU-free unit tests). The conformance cases run in each backend's `tests/conformance.rs`.

## Common tasks
- **Add a command:** add a `Command` variant, extend `CommandList::validate`, translate it in every backend, and add a conformance case.
- **Add a conformance case:** write `pub fn case(device, harness)` in `src/conformance/cases.rs` (shaders in `conformance/<name>.slang`), then add its name to `conformance_tests!`.

## Don'ts
- Don't add backend-specific enums; express them as capabilities.
- Don't make the conformance suite depend on testkit; goldens come in through `GoldenCheck`.

## References
- Vulkan synchronization2 chapter; Microsoft "Enhanced Barriers" (D3D12) for the barrier vocabulary; Apple WWDC 2022 "Go bindless with Metal 3"; Apple *Metal Feature Set Tables*.

## Status
M1 phase 1.2b; conformance shaders in Slang since 1.3.

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
