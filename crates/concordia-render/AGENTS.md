# concordia-render: agent guide

## Purpose
The renderer (ADR-0040): render graph, forward PBR, cameras, headless capture, and `capture.screenshot` (via `RenderModule`).

## Public entry points
- `graph::RenderGraph`: `create_texture` (transient), `import_texture`, and `add_pass` / `add_pass_recorded` with `PassBuilder::uses(texture, Use)` and `side_effect()`. `compile(footprint)` returns a `CompiledGraph` with `pass_names`, `barriers_before`, `memory_plan`, `to_dot` and `execute`.
- Components: `Camera` (`Projection` perspective/orthographic, reverse-Z; `Msaa`; `exposure_ev100`; `Tonemap`; `priority`) and `DirectionalLight`.
- `RenderWorld::extract(&World)` copies draws, lights and cameras with world transforms, in entity-id order. `camera(Option<EntityId>)` picks the requested camera, or else the highest priority.
- `Renderer::new(Arc<dyn Device>, Box<dyn ShaderCompiler>)` and `render(&RenderWorld, &mut dyn AssetSource, &RenderRequest) -> Image` (sRGB RGBA8).
- `AssetSource` is implemented for `AssetDb` and `MemoryAssets` (tests, procedural content, built-ins).
- `RenderModule::new(DeviceFactory)` provides `capture.screenshot` (result `image: EncodedImage`, which MCP returns as image content).

## Invariants
- Frame: forward pass (HDR `Rgba16Float`, depth `Depth32Float` cleared to 0 with `GreaterEqual`, optional 4× MSAA resolved into HDR) → tonemap (Khronos PBR Neutral or clamp) → sRGB output → readback. All of it is scheduled by the render graph, with transients placed in one heap per frame.
- Shader data read through pointers uses only `float4`/`uint4` rows (identical layout on every backend). Push constants are mirrored with `shader_struct!` and checked by `check_layout`.
- Unlit and emissive output are display-referred; exposure scales only light.
- Rendering is deterministic on one device (same scene → same bytes); draws are ordered by pipeline, then entity id.
- The device and shaders are created on the first capture, so engines start without a GPU. A missing device is `GpuUnavailable`.

## How to test
`cargo nextest run --workspace -E 'package(concordia-render)'`. The graph, camera and extraction tests are CPU-only. Render and capture tests skip without a GPU unless `CONCORDIA_REQUIRE_GPU=1`. Goldens live in `tests/golden/render/`: inspect a new image by eye, then run `CONCORDIA_BLESS=1` (never in CI).

## Common tasks
- **Inspect a frame:** `compiled.to_dot(&graph)` gives Graphviz text; `memory_plan()` lists transient placements.
- **Add a pass:** declare its uses in `add_pass_recorded`. Record commands only, since the graph derives barriers. Look up bindless indices inside the record function (it may return an `RhiError`).
- **Change shader data:** edit `shaders/*.slang` and `frame.rs` together, keeping whole `float4`/`uint4` rows; push structs must pass the layout test.

## Don'ts
- Don't record barriers by hand inside passes.
- Don't make compilation or draw order depend on hash-map iteration.
- Don't sample with anything but the static samplers (`g_textures.samplers[SAMPLER_*]`).

## References
- Yuriy O'Donnell, "FrameGraph: Extensible Rendering Architecture in Frostbite" (GDC 2017); Hans-Kristian Arntzen, "Render graphs and Vulkan — a deep dive" (2017).
- glTF 2.0 specification, Appendix B (BRDF); Google Filament material documentation (Apache-2.0); Brian Karis, "Real Shading in Unreal Engine 4" (SIGGRAPH 2013 course notes; a public talk, no code consulted).
- Lagarde & de Rousiers, "Moving Frostbite to PBR" (SIGGRAPH 2014): EV100 exposure.
- Khronos PBR Neutral Tone Mapper specification.

## Status
M1 phase 1.5 complete. Phase 1.6 adds clustered lights, cascaded shadows, IBL and more tonemappers.

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
