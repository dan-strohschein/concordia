# concordia-cluster: agent guide

## Purpose
Offline meshlet building for GPU culling (M4 4.2, ADR-0100): splits a static triangle mesh into meshlets of at most 64 or 128 triangles, each with a bounding sphere and a conservative normal cone, never spanning two materials. Pure CPU, no GPU, no `unsafe` (`#![forbid(unsafe_code)]`; meshoptimizer is reached through `meshopt-sys`'s checked wrapper).

## Public entry points
- `build(&ClusterInput, &MeshletBuildSettings, &JobSystem) -> MeshletMesh`
- `ClusterInput::new(positions, indices, materials)` (validated)
- `MeshletBuildSettings` (`new`, `for_size`, `default`: 128 triangles, 128 vertices, cone weight 0.25), `MeshletSize::for_limits` (the per-device size from mesh-output limits)
- `MeshletMesh`, `Meshlet`, `Sphere`, `NormalCone` (`rejects`, `can_cull`)
- `ClusterError`

## Invariants
- Every source triangle appears in exactly one meshlet with its winding kept (corners may rotate); degenerate triangles included.
- A meshlet's triangles share one material; meshlets are grouped by ascending material.
- Limits: at most `size` triangles and `max_vertices` vertices per meshlet.
- Bounds contain every vertex of their meshlet, including meshlets of zero-area triangles (meshoptimizer zeroes those; we compute an enclosing sphere).
- `NormalCone` is meshoptimizer's 8-bit SNORM cone with its cutoff rounded up, so `rejects` (the sphere form: `dot(c - eye, axis) >= cutoff·|c - eye| + r`) never rejects a meshlet with a front-facing triangle.

## How to test
`cargo nextest run -p concordia-cluster` (proptests; `PROPTEST_CASES=5000` with `--release` for a deeper run).

## Common tasks
- **Change the build:** it changes output, so bump the meshlet build version (cache key) and re-run the determinism and pinned-hash tests.

## Don'ts
- Don't add `unsafe` here; extend `meshopt-sys::safe`.
- Don't derive anything from the thread count.

## References
- meshoptimizer (MIT): README and header docs on meshlets and cluster bounds (Kapoulkine); its cone test formulas, with the derivation in Akenine-Möller et al., *Real-Time Rendering* 4th ed., §19.3.
- Kubisch, "Introduction to Turing Mesh Shaders" (NVIDIA 2018); Apple, "Transform your geometry with Metal mesh shaders" (WWDC22); Wihlidal, "Optimizing the Graphics Pipeline with Compute" (GDC 2016).
- No Unreal Engine (Nanite) or other non-open-source engine code was consulted.

## Status
M4 phase 4.2 (in progress).

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
