# concordia-rhi-vulkan: agent guide

## Purpose
The Vulkan implementation of `concordia-rhi` (ADR-0088): `VulkanDevice` over `ash` on Vulkan 1.3 core (dynamic rendering, synchronization2, timeline semaphores, descriptor indexing, buffer device addresses, `shaderDrawParameters` for Slang vertex shaders), with `gpu-allocator` memory and backend-tracked hazards like Metal's.

## Public entry points
- `new_headless()`, `new_headless_masked(&CapsMask)`, `new_headless_with(&Options)` → `Box<dyn Device>`; `VulkanDevice::new(&Options)`
- `Options { validation: Validation::{Off, IfAvailable, Required}, mask, log, strict }`, `Options::from_env()` (`VALIDATION_ENV`, `CONCORDIA_VULKAN_VALIDATION=off|auto|on|strict`); `ValidationLog` (`errors`, `take`); `VulkanDevice::validation_enabled`, `validation_log`, `report_validation_error_for_tests` (test hook)
- `check_spirv_module(&[u32])`: the module-wide SPIR-V rules pipeline creation applies (render tests run it on every entry point on any OS)
- Surfaces through the RHI surface API: `SurfaceTarget::{Headless, Xlib, Xcb, Wayland, MetalLayer}` (MoltenVK's `VK_EXT_metal_surface`: wired, untested)
- `probe()` → `RhiCaps`; `probe_record()` → `record::ProbeRecord`; `record::caps_from_record` and `record::choose` (pure, tested from fixtures on every OS); `BINDLESS_CAPACITY`

## Invariants
- Builds on every platform: the loader is opened at run time (`ash` `loaded`). Without a loader, a 1.3 driver or the required features, every entry point is `RhiError::NoDevice` with the reason.
- `unsafe` stays in this crate (R4); every block has a `// SAFETY:` comment (`#![deny(clippy::undocumented_unsafe_blocks)]`). `convert`, `spirv`, `record`, `track` and `timing` are safe and pure.
- **Bindless (ADR-0088):** set 0 binding 0 is a storage buffer of `BINDLESS_CAPACITY` device addresses; set 1 holds sampled images (binding 0) and the immutable static samplers (binding 1); set 2 holds storage images. Sets 1 and 2 are update-after-bind and partially bound, indexed non-uniformly (`shader{Sampled,Storage}ImageArrayNonUniformIndexing`). One pipeline layout carries 128 bytes of push constants for vertex, fragment and compute. Slots are reused only after the GPU finishes with them.
- **Empty texture slots show a fallback texture** (4.1v(c)): a 1×1 texture made at device start (cleared to zero, in `General`) fills every empty slot of both texture sets, the unused set of a texture with one usage, and freed slots, so reading one returns zero, as Metal's nil entries do. Freed buffer slots hold address 0. Shaders must keep indices inside the table, and should not pick a different texture per pixel where they can avoid it: lavapipe's non-uniform sampling faulted on that (the renderer's environment levels; `forward.slang`'s `env_sample` loops with a uniform index).
- **Hazards:** every texture rests in a home layout from its usage (storage → `General`, sampled → `ShaderRead`, attachment-only → its attachment layout, copy-only → `General`). New textures move there at the start of the next submission. Each step (render pass, dispatch, copy) moves the textures it names out and back, and a global memory barrier separates steps (and opens every submission). Every list ends with a barrier to host reads. RHI barriers only matter for aliasing (the texture's next use discards from `Undefined`).
- Clip-space Y is up (negative-height viewport) and `CullMode::Back` keeps counter-clockwise triangles, as on Metal (`clip_space_y_is_up_and_back_faces_cull`).
- SPIR-V is checked (header, framing, the named entry point for the stage, and the pointer rule: a function variable holding a physical-storage-buffer pointer must be `AliasedPointer` or `RestrictPointer`, which Slang omits when it spills such a field) before the driver sees it; failures are `PipelineCompile` with the reason. MSL is `Invalid`. The checks know the `TaskEXT` and `MeshEXT` models (D4's spike; mesh pipelines are 4.3b).
- One universal queue runs every `QueueKind`. An internal timeline counts submissions. A **retire thread** polls it (every millisecond while work is in flight, under the state lock; it makes no Vulkan call outside the lock) and retires finished submissions as Metal's completion handlers do: firing `submit_timed` and presentation callbacks, recycling command buffers and freeing destroyed objects. Submit, waits, reads and destroys retire too. The thread is joined on drop.
- **Surfaces (4.1v(b)):** the instance enables `VK_KHR_surface` and every platform surface extension the loader has; the device enables `VK_KHR_swapchain` (and `VK_KHR_swapchain_mutable_format` when present). A pure planner (`swapchain::plan`) picks 2 images when allowed, checks the extent and usage, and uses FIFO. Acquire rebuilds out-of-date or resized swapchains; images rest in `PRESENT_SRC`. Surfaces that offer only UNORM formats (Mesa's headless) present sRGB through mutable-format views. `on_presented` fires when the frame's GPU work retires (an approximation until `VK_KHR_present_wait`).
- **Strict validation:** with `Options::strict`, any error-severity validation message makes the device's next submit, present, wait or read fail with the messages, so tests that never see the device still fail on them.
- Timer scopes: a query pool per timed submission (query 0 opens the list, two per scope, the last closes it), `draw_boundary` granularity when the queue has timestamp bits; masked timestamps leave only the total.
- Not built yet: mesh pipelines (4.3b) and ray queries (4.8); they are `Unsupported` and their caps are false. D4's spike (`src/spike`, test-only) shows lavapipe runs Slang's mesh, amplification and ray-query SPIR-V under validation.
- All mutable state is behind one Mutex; the queue is used only while it is held.
- M4 4.3a: viewports are flipped one by one (`flipped_viewport`) so Y stays up inside any rectangle; `CopyTexture` moves both textures to copy layouts and back (`General` within one texture); copy-only textures get no image views (`has_views`); a failed command closes an open render pass (`Recorder::abandon`) so the command buffer can end. The global barrier (`ALL_COMMANDS`, `MEMORY_*`) already covers indirect-argument reads.

## How to test
- `cargo nextest run -p concordia-rhi-vulkan`: the pure tests run everywhere; the device and conformance tests skip without Vulkan.
- `CONCORDIA_REQUIRE_VULKAN=1` (the on-demand `vulkan` workflow: `mesa-vulkan-drivers`, `vulkan-validationlayers`) makes a missing device or validation layer a failure, and any validation error fails a test. `CONCORDIA_REQUIRE_GPU` means Metal and does not affect these tests.
- The device code was written before its tests (author decision 2026-11-24). They pass on lavapipe in CI with no validation errors; real GPUs and MoltenVK are re-tested later. The `vulkan` workflow runs only on demand (Actions → vulkan → Run workflow, or `gh workflow run vulkan --ref <branch>`); inputs: `filter` (this crate's tests), `render_filter` (the renderer's), and `gdb` (`file:test` runs one renderer test under gdb with GPU-assisted validation, strict and off, printing registers and code, and skips the rest).
- `spike::*` (D4, M4 4.1v(c)): the spike shaders' SPIR-V passes the checks on any OS with the Slang toolchain; the device cases open their own small device with `VK_EXT_mesh_shader` or `VK_KHR_ray_query`/`VK_KHR_acceleration_structure` and fail on any validation error.
- A crash inside lavapipe's JIT'd shader code (a `llvmpipe-N` thread, no symbols) is usually a bad descriptor: run the test with the `gdb` input and read the code around the fault. In the non-uniform sampling loop (`kmovd`/`tzcnt` over lanes, a load from the lane's descriptor, an indirect call) a null pointer can also come from lavapipe itself when lanes read different textures; experiments on a throwaway branch (make one site's index uniform at a time) find the site faster than reading the code.

## Common tasks
- **Translate a new command:** add it to `track::plan` (is it a step? which layouts?) and `encode.rs`, then a conformance case.
- **Re-record the lavapipe fixture:** see `tests/fixtures/probes/README.md`.

## Don'ts
- Don't call `ash` outside `instance`, `device`, `resources`, `bindless`, `pipeline`, `encode` and `validation`.
- Don't hand malformed SPIR-V to the driver; extend `spirv::check` instead.
- Don't reuse a bindless slot before retirement.

## References
- Khronos *Vulkan 1.3 specification* (synchronization2, dynamic rendering, descriptor indexing, buffer device address, timeline semaphores, timestamp queries; the facing rule in "Basic Polygon Rasterization"; WSI: `VK_KHR_surface`, `VK_KHR_swapchain`, `VK_KHR_swapchain_mutable_format`, `VK_EXT_headless_surface`, the Xlib, XCB, Wayland and Metal surface extensions; `VK_EXT_robustness2`'s null descriptors; `VK_EXT_mesh_shader`, `VK_KHR_acceleration_structure` and `VK_KHR_ray_query` for the spike).
- The Khronos Vulkan-Samples' `mesh_shading` and `ray_queries` samples (Apache-2.0), read for call order only; the Slang user guide on mesh shaders and `RayQuery`.
- The `ash` 0.38 and `gpu-allocator` 0.28 sources and docs (both MIT OR Apache-2.0), read for their APIs.
- The lavapipe fixture was recorded on GitHub's ubuntu-24.04 runner by the CI job.
- No Unreal or other source-available engine code was consulted.

## Status
M4 phase 4.1v(a): device, memory, buffers, textures, samplers, bindless, compute and graphics pipelines, readback and timestamps. 4.1v(b): surfaces and swapchains, strict validation, the retire thread, non-uniform indexing, the SPIR-V pointer rule; the M1 renderer runs on lavapipe. 4.1v(c): the fallback texture in empty slots, the mesh/task SPIR-V models, and D4's spike.
 M4 4.3a: indirect draws, viewports, scissors and image copies; tested on lavapipe through the `vulkan` workflow.
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
