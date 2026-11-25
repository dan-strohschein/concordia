# concordia-rhi-vulkan: agent guide

## Purpose
The Vulkan implementation of `concordia-rhi` (ADR-0088): `VulkanDevice` over `ash` on Vulkan 1.3 core (dynamic rendering, synchronization2, timeline semaphores, descriptor indexing, buffer device addresses, `shaderDrawParameters` for Slang vertex shaders), with `gpu-allocator` memory and backend-tracked hazards like Metal's.

## Public entry points
- `new_headless()`, `new_headless_masked(&CapsMask)`, `new_headless_with(&Options)` → `Box<dyn Device>`; `VulkanDevice::new(&Options)`
- `Options { validation: Validation::{Off, IfAvailable, Required}, mask, log }`; `ValidationLog` (`errors`, `take`); `VulkanDevice::validation_enabled`, `validation_log`, `report_validation_error_for_tests` (test hook)
- `probe()` → `RhiCaps`; `probe_record()` → `record::ProbeRecord`; `record::caps_from_record` and `record::choose` (pure, tested from fixtures on every OS); `BINDLESS_CAPACITY`

## Invariants
- Builds on every platform: the loader is opened at run time (`ash` `loaded`). Without a loader, a 1.3 driver or the required features, every entry point is `RhiError::NoDevice` with the reason.
- `unsafe` stays in this crate (R4); every block has a `// SAFETY:` comment (`#![deny(clippy::undocumented_unsafe_blocks)]`). `convert`, `spirv`, `record`, `track` and `timing` are safe and pure.
- **Bindless (ADR-0088):** set 0 binding 0 is a storage buffer of `BINDLESS_CAPACITY` device addresses; set 1 holds sampled images (binding 0) and the immutable static samplers (binding 1); set 2 holds storage images. Sets 1 and 2 are update-after-bind and partially bound. One pipeline layout carries 128 bytes of push constants for vertex, fragment and compute. Slots are reused only after the GPU finishes with them.
- **Hazards:** every texture rests in a home layout from its usage (storage → `General`, sampled → `ShaderRead`, attachment-only → its attachment layout, copy-only → `General`). New textures move there at the start of the next submission. Each step (render pass, dispatch, copy) moves the textures it names out and back, and a global memory barrier separates steps (and opens every submission). Every list ends with a barrier to host reads. RHI barriers only matter for aliasing (the texture's next use discards from `Undefined`).
- Clip-space Y is up (negative-height viewport) and `CullMode::Back` keeps counter-clockwise triangles, as on Metal (`clip_space_y_is_up_and_back_faces_cull`).
- SPIR-V is checked (header, framing, the named entry point for the stage) before the driver sees it; failures are `PipelineCompile` with the reason. MSL is `Invalid`.
- One universal queue runs every `QueueKind`. An internal timeline counts submissions; finished ones retire lazily (on submit, waits, timeline reads and destroys), firing `submit_timed` callbacks, recycling command buffers and freeing destroyed objects.
- Timer scopes: a query pool per timed submission (query 0 opens the list, two per scope, the last closes it), `draw_boundary` granularity when the queue has timestamp bits; masked timestamps leave only the total.
- Not in 4.1v(a): surfaces and present (4.1v(b)), mesh pipelines (4.3b), ray queries (4.8); they are `Unsupported` and their caps are false.
- All mutable state is behind one Mutex; the queue is used only while it is held.

## How to test
- `cargo nextest run -p concordia-rhi-vulkan`: the pure tests run everywhere; the device and conformance tests skip without Vulkan.
- `CONCORDIA_REQUIRE_VULKAN=1` (the on-demand `vulkan` workflow: `mesa-vulkan-drivers`, `vulkan-validationlayers`) makes a missing device or validation layer a failure, and any validation error fails a test. `CONCORDIA_REQUIRE_GPU` means Metal and does not affect these tests.
- The device code was written before its tests (author decision 2026-11-24). They pass on lavapipe in CI (64/64, no validation errors); real GPUs and MoltenVK are re-tested later. The `vulkan` workflow runs only on demand (Actions → vulkan → Run workflow, or `gh workflow run vulkan --ref <branch>`, optional `filter`).

## Common tasks
- **Translate a new command:** add it to `track::plan` (is it a step? which layouts?) and `encode.rs`, then a conformance case.
- **Re-record the lavapipe fixture:** see `tests/fixtures/probes/README.md`.

## Don'ts
- Don't call `ash` outside `instance`, `device`, `resources`, `bindless`, `pipeline`, `encode` and `validation`.
- Don't hand malformed SPIR-V to the driver; extend `spirv::check` instead.
- Don't reuse a bindless slot before retirement.

## References
- Khronos *Vulkan 1.3 specification* (synchronization2, dynamic rendering, descriptor indexing, buffer device address, timeline semaphores, timestamp queries; the facing rule in "Basic Polygon Rasterization").
- The `ash` 0.38 and `gpu-allocator` 0.28 sources and docs (both MIT OR Apache-2.0), read for their APIs.
- The lavapipe fixture was recorded on GitHub's ubuntu-24.04 runner by the CI job.
- No Unreal or other source-available engine code was consulted.

## Status
M4 phase 4.1v(a): device, memory, buffers, textures, samplers, bindless, compute and graphics pipelines, readback and timestamps.

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
