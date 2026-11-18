# ADR-0045: Slang toolchain and shader ABI
- Status: Accepted
- Date: 2026-11-18

## Context
Every Concordia shader is written in Slang and compiled to MSL (Metal) or SPIR-V (Vulkan). There were two ways to drive the compiler: the `shader-slang` Rust crate (0.1.0, last released 2025-07, with reflection fixes only on git main), or the `slangc` executable with `-reflection-json`. Builds must stay reproducible and must not require a system install.

## Decision
- **Pinned prebuilt releases.** `tools/slang/slang.toml` pins Slang **v2026.22** with a SHA-256 per platform (macOS arm64, Linux x86_64). `just slang-fetch` downloads, verifies and unpacks it into `target/toolchain/slang/<version>/`; CI caches that directory. `SLANG_DIR` overrides the location. License: Apache-2.0 WITH LLVM-exception (a binary download, so it is outside cargo-deny).
- **`slangc` subprocess** behind the `ShaderCompiler` trait (`SlangcCompiler`). The `shader-slang` crate stays an option behind the same trait once a release covers reflection; `deny.toml` forbids git dependencies.
- **Shader ABI.** Shaders `import bindless;` (`crates/concordia-shaders/shaders/bindless.slang`), which declares the three bindless `ParameterBlock`s, and declare no other globals. Push constants are the entry point's single `uniform` parameter, named `push`. On Metal this lands on slots 0 (buffer pointers), 1 (sampled textures), 2 (storage textures) and 3 (push constants), matching `concordia-rhi-metal::slots`; a test checks it. Slot 1 also carries `samplers[4]`, indexed with the `SAMPLER_*` constants, which a test keeps in sync with `StaticSampler`. Global push declarations were rejected because Slang orders a file's own globals before imported ones.
- **Caching.** The cache key is blake3 of the compiler version, target, stage, entry, sorted defines, and the contents of the root file and every `import` / `#include` dependency. Entries are stored in the project DDC (`concordia_core::ddc::Ddc`, layout `<2 hex>/<64 hex>`) through the `ShaderCache` trait.
- **Layout checks.** `shader_struct!` (using `offset_of!`) describes `#[repr(C)]` Rust mirrors, and `check_layout` compares them with reflection. It catches, for example, Metal's 16-byte `float3`.
- **Hot reload.** `ShaderLibrary::on_files_changed` recompiles dependents and keeps the last good module when an edit fails; callers rebuild pipelines between frames. File watching is wired by the renderer (phase 1.5).

## Consequences
Shader compilation spawns a process (about 0.1–0.3 s per entry point), so the cache matters. Upgrading Slang means editing one pin file and rerunning the shader and conformance tests. The conformance suite's shaders are Slang, compiled per backend.

## Alternatives considered
- The `shader-slang` crate: in-process and faster, but stale releases and unclear reflection coverage.
- The LunarG Vulkan SDK's Slang: an extra SDK dependency on macOS.
- Hand-written MSL: no SPIR-V, and two shader languages to maintain.
