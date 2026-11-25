# concordia-rhi: agent guide

## Purpose
Backend-neutral GPU vocabulary and the `Device` trait (ADR-0040): validated descriptors, typed handles, command lists as data, sync2-style barriers, bindless indices, capabilities, and the conformance suite every backend runs.

## Public entry points
- `Device` (object-safe), `Submit`, `QueueKind`
- `CommandList` (`push`, `validate`) and `Command`, `RenderPassDesc`, `ColorAttachment`, `DepthAttachment`, `LoadOp`, `StoreOp`, `ClearValue`
- Descriptors: `BufferDesc`, `TextureDesc`, `HeapDesc` (`validate`), `ComputePipelineDesc`, `GraphicsPipelineDesc`, `MeshPipelineDesc`, `ShaderSource`
- Handles: `Buffer`, `Texture`, `Pipeline`, `Heap`, `Timeline`, `BindlessIndex`; `Slots<T>` for backends
- `Access`, `BufferBarrier`, `TextureBarrier`, `Format`, `Requirement`, `RhiError`
- Capabilities (M4 4.1a, D10): `RhiCaps` (was `DeviceCaps`; adds `object_shaders`, `hw_ray_tracing`, `metalfx_temporal`, `timestamp_granularity`, `max_mesh_output_vertices` / `_primitives`), `TimestampGranularity` (`times_passes`), `CapsMask` and `RhiCaps::masked`
- Timing (4.1a, ADR-0089): `Command::BeginTimer(TimerScope)` / `EndTimer`; `GpuTiming { total_ms, scopes }` and `ScopeTime` through `GpuTimeCallback` (`submit_timed`, `Present::on_gpu_time`)
- `conformance` (feature `conformance`): `conformance_tests!(make, golden, shaders)`, `Harness`, `GoldenCheck`, `ShaderProvider`, `shader_dir()` (Slang sources in `conformance/*.slang`); `surface_conformance_tests!(make, surface)` with `TestSurface` (M4 4.1v(b): each backend supplies a test surface: a `CAMetalLayer`, or Vulkan's `Headless`)
- `SurfaceTarget::{MetalLayer, Xlib, Xcb, Wayland, Headless}` (M4 4.1v(b)); `Backend::{ALL, as_str}`, `Display`, `FromStr` (`UnknownBackend`)
- M4 4.3a (D5): `Command::{DrawIndirect, DrawIndexedIndirect}` (one draw each; `DRAW_INDIRECT_BYTES` 16, `DRAW_INDEXED_INDIRECT_BYTES` 20, the layout Metal and Vulkan share), `SetViewport(Viewport)` and `SetScissor(Rect)` (inside passes; each pass starts with the whole target), `CopyTexture` (one region, `TextureOrigin`, outside passes); `check::{indirect_args, texture_copy (CopySide), viewport_in, scissor_in}`, the checks every backend runs against its resources.
- M4 4.3b: `MeshPipelineDesc` (`ObjectStage`, `MeshStage`, optional fragment, samples, cull, blend) with `validate(&RhiCaps)`: `Unsupported` without mesh (or, with an object stage, object) shaders; `Invalid` for output counts outside `1..=` the caps, stage groups outside `1..=MAX_MESH_STAGE_THREADS` (128) or payloads outside `1..=MAX_MESH_PAYLOAD_BYTES` (16 KB), the portable limits Metal and every `VK_EXT_mesh_shader` device take; never clamped. `Command::DrawMeshTasks { groups }` inside passes, each count `1..=MAX_MESH_TASK_GROUPS` (65,535) and at most `MAX_MESH_TASK_GROUPS_TOTAL` (2^22) in all; a mesh draw needs a mesh pipeline bound and vertex draws a vertex pipeline (checked by the backends). Conformance: `mesh.rs` (5 cases); devices without mesh shaders must refuse every mesh pipeline.

## Invariants
- No graphics-API types here; backends translate.
- Every descriptor and command list is validated before a backend sees it; public APIs return `RhiError` and never panic on bad input.
- Handles are generation-checked: a destroyed resource's handle is `StaleHandle`, never another resource.
- Shaders see resources only through bindless indices (buffer table, texture table) and ≤ 128 bytes of push constants.
- `RhiCaps::feature_sets` is sorted; `SampleCount` holds only 1/2/4/8.
- `masked` only ever removes capabilities: mesh shaders take object shaders and the mesh output limits with them, the ray-tracing API takes hardware ray tracing, and masked timestamps leave at most `command_buffer`.
- Timer scopes are paired, not nested, unique per list and outside render passes (`CommandList::validate`). A backend whose caps don't `times_passes()` ignores them and reports no scopes; one that does reports each scope once, in command order, in ms from the submission's first sample.
- Indirect draws read one record; GPU-decided draw counts (count buffers, indirect command buffers) are later work (`later-backlog.md` §12). Our shaders take their bases from push constants and keep the records' first vertex and instance at 0. Viewports and scissors must lie inside the pass's targets; copies are single-sample, same format, not depth, inside both mips, and never overlap within one mip.

## How to test
`cargo nextest run -p concordia-rhi` (GPU-free unit tests). The conformance cases run in each backend's `tests/conformance.rs`.

## Common tasks
- **Add a command:** add a `Command` variant, extend `CommandList::validate`, translate it in every backend, and add a conformance case.
- **Add a conformance case:** write `pub fn case(device, harness)` in `src/conformance/cases.rs` (shaders in `conformance/<name>.slang`), then add its name to `conformance_tests!`.

## Don'ts
- Don't add backend-specific enums; express them as capabilities.
- Don't make the conformance suite depend on testkit; goldens come in through `GoldenCheck`.

## References
- Apple, "Sampling GPU data into counter sample buffers" (timer scopes at stage boundaries).
- Vulkan synchronization2 chapter; Microsoft "Enhanced Barriers" (D3D12) for the barrier vocabulary; Apple WWDC 2022 "Go bindless with Metal 3"; Apple *Metal Feature Set Tables*.

## Status
M1 phase 1.2b; conformance shaders in Slang since 1.3. M4 phase 4.1a: `RhiCaps` and masking, timer scopes (`timer_scopes_report_each_scope_once_in_order`).
 M4 4.3a: indirect draws, viewport and scissor, texture copies, `check`, and 11 conformance cases (`indirect.rs`, `regions.rs`). M4 4.3b: mesh pipelines and mesh-task draws (`mesh.rs`, 5 cases).
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
