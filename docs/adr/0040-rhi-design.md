# ADR-0040: RHI design
- Status: Accepted
- Date: 2026-11-18

## Context
Concordia renders through its own thin RHI (design doc), first on Metal, then Vulkan, and D3D12 later. It must express what M4 needs (bindless resources, explicit synchronization, transient aliasing, mesh shaders, 64-bit atomics) without leaking backend types. It must also stay testable without a GPU: the render graph (phase 1.5) and agents should be able to inspect the work a frame records.

## Decision
- **Handles and descriptors.** Resources are typed, generation-checked handles (`Buffer`, `Texture`, `Pipeline`, `Heap`, `Timeline`) owned by the device. Descriptors are plain data and validated before any backend call, so public APIs return `RhiError` and never panic.
- **Commands are data.** Work is recorded into a backend-neutral `CommandList`: barriers, copies, render passes, pipeline binds, push constants, draws, dispatches and indirect dispatches. `CommandList::validate` checks its structure, and `Device::submit` translates it. The render graph emits command lists that a mock device can assert on.
- **Bindless first.** Each device has one buffer table and one texture table (16,384 entries each). Shaders receive only `BindlessIndex`es through push constants (≤ 128 bytes) or buffers. On Metal 3 the tables are argument buffers holding `gpuAddress` / `gpuResourceID` values, at fixed slots: buffers 0, sampled textures 1, storage textures 2 (the same table), push constants 3. Encoders declare bindless resources with `useResource`. A fixed set of static samplers (`StaticSampler`: linear/nearest × repeat/clamp) follows the texture slots in the texture table, so sampling needs no extra slot (added in phase 1.5).
- **Barriers in sync2 style.** Each barrier records stages, access flags and layout before and after, plus explicit aliasing barriers. **On Metal 3, M1 relies on Metal's hazard tracking:** resources and heaps are created tracked, and an RHI barrier only ends the current encoder. Hand-placed `MTLFence`s on untracked resources are subtle and error-prone on Metal 3. Explicit barriers become authoritative on Vulkan and on a future Metal 4 path, where the API requires them.
- **Sync.** `Timeline` maps to `MTLSharedEvent` (and to Vulkan timeline semaphores). A submission waits on and signals (timeline, value) pairs.
- **Future-proofing.** `DeviceCaps` reports mesh shaders, ray tracing, bindless and MSAA counts. The API already includes `MeshPipelineDesc`, `R64Uint` and indirect dispatch. On Metal, M1 returns `Unsupported` for mesh pipelines and for `R64Uint` textures until M4 uses them.
- **Conformance.** `concordia_rhi::conformance` is the backend-agnostic test suite (feature `conformance`). Each backend instantiates it with one macro; goldens go through `concordia-testkit` (FLIP).
- **Backends.** Metal through `objc2-metal` (Metal 3 API, macOS 14+; Metal 4 evaluated in M2/M4). Vulkan through `ash` later (phase 1.10 stretch or M2). D3D12 later still. WebGPU is an optional browser backend.

## Consequences
Rendering code and tests never see Metal or Vulkan types. Recording into a `Vec<Command>` costs an allocation per frame, which is acceptable in M1; arenas can come later. Relying on tracked hazards on Metal costs some CPU, which is acceptable at M1 scale, and the explicit barriers are already recorded for backends that need them.

## Alternatives considered
- **wgpu:** a portable, mature WebGPU implementation. Its binding model (bind groups, no true bindless or 64-bit atomics on all paths) and its abstraction level would limit M4's virtualized geometry and GPU-driven rendering.
- **Trait objects per command buffer (immediate-mode encoding):** less allocation, but command lists can no longer be inspected and the backend surface grows.
- **Untracked Metal resources with fences now:** the plan's first choice. Deferred to the Metal 4 path, where explicit barriers are native.
- **OpenGL: rejected.** macOS freezes it at 4.1 with no compute; it has no portable bindless or explicit barriers, no mesh shaders, and no 64-bit atomics; and driver variance would weaken golden images. Its one gain, legacy hardware, is not a goal: Vulkan covers 2016+ GPUs, and lavapipe covers software rendering in CI. Unreal Engine 5 made the same choice (D3D12, Vulkan, Metal; no desktop OpenGL).
