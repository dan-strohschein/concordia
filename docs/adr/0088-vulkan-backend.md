# ADR-0088: The Vulkan backend
- Status: Accepted (M4 phase 4.1v(a), 2026-11-24; plan approved by the author on 2026-11-24. The conformance suite passes on lavapipe in CI; real GPUs and MoltenVK are re-tested later)
- Date: 2026-11-24
- Builds on: ADR-0040 (RHI), ADR-0042 (GPU testing in CI), ADR-0045 (Slang), ADR-0089 (timer scopes); M4 refinement R3 and R4

## Context
Until M4 the engine rendered only on Metal. R3 brings Vulkan into M4, so every rendering feature from 4.3 on lands on both backends, and Linux CI can finally run the GPU tests that it has skipped since M1 (ADR-0042). The RHI was designed for this: command lists as data, sync2-style barriers, bindless indices, `Backend::Vulkan` and SPIR-V shader sources. The Metal backend relies on two things Vulkan does not give for free: tracked hazards, and argument buffers that hold 16 384 device pointers.

## Decision
1. **A new crate, `concordia-rhi-vulkan`, on `ash`** (MIT OR Apache-2.0), loaded at run time (`ash`'s `loaded` feature). It builds on every OS. Without a Vulkan loader or a suitable device, every entry point returns `RhiError::NoDevice`.
2. **The `unsafe` exception (R4).** Raw Vulkan calls are `unsafe`, and they stay inside this crate, as Metal's stay inside `concordia-rhi-metal`. Every block has a `// SAFETY:` comment, enforced by `#![deny(clippy::undocumented_unsafe_blocks)]`. Conversion, SPIR-V checks, caps and hazard planning are safe code.
3. **Vulkan 1.3 core is the floor:** dynamic rendering, synchronization2, timeline semaphores, descriptor indexing and buffer device addresses. lavapipe, MoltenVK and current desktop drivers meet it. A device below it is `NoDevice`, with the missing features named.
4. **Bindless on Vulkan:**
   - Set 0 binding 0 is a storage buffer of buffer device addresses. Under `CONCORDIA_SPIRV`, which `concordia-shaders` defines for SPIR-V, `bindless.slang` declares it as `StructuredBuffer<uint*>`. A std140 uniform block of 16 384 pointers would be 256 KiB, over common uniform-buffer limits. Shaders are unchanged, and so is the Metal output.
   - Set 1 holds sampled images (binding 0) and the four immutable static samplers (binding 1); set 2 holds storage images.
   - The sets are update-after-bind and partially bound. One pipeline layout per device carries 128 bytes of push constants for every stage.
5. **Memory through `gpu-allocator`** (MIT OR Apache-2.0). A heap is one dedicated allocation, and placed textures bind at an offset in it.
6. **Hazards are the backend's job**, as Metal's tracked mode makes them on Metal:
   - Every texture has a home layout chosen from its usage, and each step (render pass, dispatch, copy) moves what it uses out of that layout and back.
   - A global memory barrier separates consecutive steps. New textures move to their home layouts at the start of the next submission.
   - RHI barriers are still translated (aliasing discards from `Undefined`).
   - Making the render graph's barriers authoritative, and dropping the global barriers, is a later performance change.
7. **Metal's conventions:**
   - Clip-space Y points up (a negative-height viewport), and `CullMode::Back` keeps counter-clockwise triangles.
   - Every `QueueKind` runs on one universal queue for now.
8. **Timer scopes (ADR-0089)** use timestamp queries, one pool per timed submission. Granularity is `draw_boundary` when the queue has timestamp bits.
9. **Tests:**
   - The RHI conformance suite runs unchanged, plus Vulkan-only cases.
   - Linux CI runs them on Mesa's lavapipe with the Khronos validation layers in a `vulkan (lavapipe)` job of its own workflow (`vulkan.yml`, keeping `ci.yml` fast per ADR-0071), informational until the author's first re-test on another machine. Any error-severity validation message fails the test.
   - `CONCORDIA_REQUIRE_VULKAN=1` turns a missing device or layer into a failure; `CONCORDIA_REQUIRE_GPU` keeps meaning Metal.
   - Slang's SPIR-V keeps source entry names (`-fvk-use-entrypoint-name`), and Slang vertex shaders need `shaderDrawParameters`, which the backend requires.
   - RHI goldens are shared across backends, with one tolerance file.

## Consequences
- Linux CI exercises GPU code for the first time, in software. lavapipe is slow, so the Vulkan suites run small images.
- Real GPUs (the author's Linux machine, MoltenVK on the Mac) are tested later. A driver difference that lavapipe hides may surface then.
- Global barriers between steps cost GPU time on real hardware; they buy correctness now, and the render graph's barriers can replace them.
- `bindless.slang` has one target switch. Any new global table must be checked on both targets (`spirv_bindless_tables_use_sets_0_1_2`).

## Alternatives considered
- **wgpu:** another abstraction under ours, without bindless arrays of the size we need or the barrier control later phases want. Rejected.
- **vulkano:** safe wrappers, but its synchronisation model would fight the RHI's command lists. Rejected.
- **Our own allocator:** less code to depend on, but sub-allocation, alignment and memory-type selection are easy to get subtly wrong. `gpu-allocator` is widely used and permissively licensed.
- **Requiring explicit barriers everywhere:** Metal ignores them, so nothing would test them until a real Vulkan renderer existed. Tracking in the backend keeps the conformance suite unchanged.
- **A scalar-layout uniform buffer for the table** (`-fvk-use-scalar-layout`): 128 KiB, still over the 64 KiB limit on some drivers. Rejected.
