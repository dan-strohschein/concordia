# concordia-render-types: agent guide

## Purpose
The render vocabulary shared by the GPU renderer (`concordia-render`) and the reference path tracer (`concordia-pathtrace`, M4 4.1b): the reflected render components, their camera math, photometric units and the CPU tone-mapping references. Both renderers read the same definitions, so a reference image and a rendered one describe the same scene (ADR-0101).

## Public entry points
- Components: `Camera` (`exposure()`), `Projection` (`matrix(aspect)` → `ProjectionError`), `Msaa`, `Tonemap`, `DirectionalLight` (with `CascadeSettings`, `PcfKernel`; `CascadeSettings::validate`), `PointLight`, `SpotLight`, `Environment`.
- `view_matrix(world)`, `pick_camera(cameras, requested)` (highest priority, ties to the lowest id), `candela(lumens)`, `distance_window(d², range)` (the falloff windowed to zero at `range`), `SpotCone` (`attenuation`), `equirect::{dir_to_uv, uv_to_dir}`.
- `tonemap::{apply, srgb_encode}`: the CPU references `tonemap.slang` must match.

## Invariants
- **No GPU dependencies:** never `concordia-render`, `concordia-rhi*` or `concordia-shaders` (`repo_hygiene::gpu_free_crates_keep_their_dependencies`). This is what lets the path tracer and the future CPU-only path (M14) use it.
- `concordia-render` re-exports everything here under its old paths (`concordia_render::Camera`, `concordia_render::tonemap::apply`, …); reflected type names are unchanged, so scenes, schemas and snapshots are too.
- Errors here are plain values (`ProjectionError`, validation reasons as `String`); each renderer wraps them in its own error type.

## How to test
`cargo nextest run -p concordia-render-types`. Behavior is covered where it is used: `concordia-render`'s camera, shadow and tonemap tests (the tonemap shader is compared with `tonemap::apply`).

## Common tasks
- **Add a render component:** define it here (reflected, validated), re-export it from `concordia-render`, register it in `RenderModule`, and have each renderer read it.
- **Change a light's definition or a unit:** change it here, then both renderers' tests must agree with it.

## Don'ts
- Don't put GPU data layouts, shader indices or pipeline details here (`tonemap_shader_mode` stays in `concordia-render`).

## References
- Lagarde & de Rousiers, "Moving Frostbite to PBR" (SIGGRAPH 2014): EV100 exposure and photometric units.
- Khronos PBR Neutral Tone Mapper specification; AgX after Troy Sobotka (minimal analytic fit, Benjamin Wrensch 2023; Godot 4.4, MIT); ACES fitted after Krzysztof Narkowicz (2015).

## Status
M4 phase 4.1b2: the light falloff, the spot cone, the equirect mapping and the camera rule, shared with the path tracer. M4 phase 4.1b1: moved out of `concordia-render` (components, `CascadeSettings`, camera math, `candela`, the CPU tonemap).

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
