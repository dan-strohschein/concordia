# concordia-rhi-metal: agent guide

## Purpose
The Metal implementation of `concordia-rhi` (ADR-0040): `MetalDevice` over `objc2-metal` using the Metal 3 API, bindless argument-buffer tables and tracked hazards.

## Public entry points
- `new_headless()` → `Box<dyn Device>`; `MetalDevice::system_default()`, `MetalDevice::simulate_device_lost()` (test hook)
- `probe()` → `RhiCaps` (used by `concordia gpu-probe`); `probe_record()` → `record::ProbeRecord` (the raw facts, `concordia gpu-probe --record`), and `record::caps_from_record` (pure: the family table, tested from fixtures on every OS)
- `new_headless_masked(&CapsMask)` / `MetalDevice::system_default_masked`: a device with features hidden (testing fallback paths)
- `slots` (shader buffer slots: 0 buffers, 1 textures, 2 storage textures, 3 push constants), `BINDLESS_CAPACITY`

## Invariants
- Builds on every platform: Apple dependencies are `cfg(target_os = "macos")` only, and off macOS every entry point returns `RhiError::NoDevice`.
- Resources and heaps are hazard-tracked; bindless resources are declared with `useResource` on every encoder; an RHI barrier ends the current encoder. Push constants are re-applied whenever an encoder starts.
- `unsafe` stays in this crate; every block has a `// SAFETY:` comment. All mutable state is behind one Mutex.
- Unsupported on Metal 3 here: mesh pipelines and `R64Uint` (M4).
- Family table (`caps_from_record`): mesh and object shaders with `metal3` (Apple7+, Mac2), outputs ≤ 256 vertices and 512 primitives per threadgroup; the ray-tracing API from `supportsRaytracing`; hardware ray tracing from Apple9; MetalFX temporal scaling from `MTLFXTemporalScalerDescriptor.supportsDevice`; timestamps at stage boundaries when the timestamp counter set and that sampling point exist.
- Timer scopes (4.1a): a scope ends the current encoder at both edges; every encoder opened inside it is created from a pass descriptor with sample attachments (render: vertex and fragment start and end; compute and blit: encoder start and end) into one shared-storage counter sample buffer per timed submission (≤ 4096 samples). The completion handler resolves it; zero and error samples are skipped; ticks become ms through a `sampleTimestamps` calibration taken at device creation and again at resolve. Untimed submissions never split encoders.
- A command that fails while an encoder is open ends the encoder before the error returns (Metal aborts otherwise). Viewport, scissor and indirect-argument checks use the open pass's target size and the buffer's descriptor (`concordia_rhi::check`). (M4 4.3a)

## How to test
`CONCORDIA_REQUIRE_GPU=1 cargo nextest run -p concordia-rhi-metal` (conformance suite and goldens in `tests/golden/rhi/`). Without the variable, the tests skip when there is no device.

## Common tasks
- **Translate a new command:** add it to `encode.rs` (choose or open the right encoder) and run the conformance suite.
- **Bless goldens:** `just bless`, then check the PNGs by eye.

## Don'ts
- Don't use metal-rs (deprecated).
- Don't create untracked resources until the Metal 4 path adds explicit barriers.

## References
- Apple, "Sampling GPU data into counter sample buffers", "Creating a counter sample buffer to store a GPU's counter data", and MetalFX's `MTLFXTemporalScalerDescriptor` documentation; `objc2-metal-fx` (Zlib/Apache-2.0/MIT).
- Apple *Metal Feature Set Tables*, *Metal Shading Language Specification* 3.x, WWDC 2022 "Go bindless with Metal 3"; `objc2-metal` (MIT/Zlib/Apache-2.0).

## Status
M1 phase 1.2b (surfaces and presentation arrive with rendering in 1.5). M4 phase 4.1a: probe records and fixtures (`tests/fixtures/probes/`: apple9 recorded on the M3 Pro; apple7, apple8, apple10 hand-written from Apple's tables), masking, counter-sample-buffer timer scopes, the MetalFX probe.
 M4 4.3a: indirect draws, viewport, scissor and texture-to-texture blits.
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
