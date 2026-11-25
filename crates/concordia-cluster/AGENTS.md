# concordia-cluster: agent guide

## Purpose
Offline meshlet building for GPU culling (M4 4.2, ADR-0100): splits a static triangle mesh into meshlets of at most 64 or 128 triangles, each with a bounding sphere and a conservative normal cone, never spanning two materials. Pure CPU, no GPU, no `unsafe` (`#![forbid(unsafe_code)]`; meshoptimizer is reached through `meshopt-sys`'s checked wrapper).

## Public entry points
- `build(&ClusterInput, &MeshletBuildSettings, &JobSystem) -> MeshletMesh`; `piles(&ClusterInput)` (the units of parallel work, in output order) and `PILE_TRIANGLES` (131,072)
- `ClusterInput::new(positions, indices, materials)` (validated)
- `MeshletBuildSettings` (`new`, `for_size`, `default`: 128 triangles, 128 vertices, cone weight 0.25), `MeshletSize::for_limits` (the per-device size from mesh-output limits), `AUTO_MIN_TRIANGLES` (import's `auto` threshold, 8,192)
- `MeshletMesh` (`encode` / `decode`: `MeshletBlob` v1, `FORMAT_VERSION`), `Meshlet`, `Sphere`, `NormalCone` (`rejects`, `can_cull`)
- `cache_key(&ClusterInput, &MeshletBuildSettings)`: the DDC key of a build
- `MeshletStats::of(&MeshletMesh, encoded_bytes)`, `Spread`
- `ClusterError`

## Invariants
- Every source triangle appears in exactly one meshlet with its winding kept (corners may rotate), degenerate triangles included.
- A meshlet's triangles share one material; meshlets are grouped by ascending material.
- Limits: at most `size` triangles and `max_vertices` vertices per meshlet.
- Bounds contain every vertex of their meshlet, including meshlets of zero-area triangles (meshoptimizer zeroes those; we compute an enclosing sphere and keep the zero cone, which never rejects).
- `NormalCone` is meshoptimizer's 8-bit cone: `cutoff` is the sine of the normal cone's half-angle, rounded up, so `rejects` (the sphere form `dot(c − eye, axis) ≥ cutoff·|c − eye| + r`) never rejects a meshlet with a front-facing triangle.
- Determinism: a material over `PILE_TRIANGLES` is sorted by centroid Morton code (ties by triangle number) and cut into fixed piles; piles build in parallel into their own slots and join in order. Nothing depends on the thread count. The output is the same on macOS arm64 and Linux x86_64 (`output_hash_is_pinned`; meshoptimizer is built with `-ffp-contract=off`).
- `MeshletBlob` v1 is canonical (consecutive offsets, 4-byte aligned triangle runs with zero padding); `decode` validates everything and never panics, and a valid blob re-encodes to the same bytes.
- `cache_key` covers only what the build reads (positions, indices, per-triangle materials with none meaning 0), the settings and the format, build and meshoptimizer versions; no platform.

## How to test
`cargo nextest run -p concordia-cluster` (proptests; `PROPTEST_CASES=5000` with `--release` for a deeper run). The 10M-triangle throughput test is ignored: `cargo nextest run -p concordia-cluster --release --run-ignored only` (judged only on the reference Mac).

## Common tasks
- **Change the build or the format:** bump `BUILD_VERSION` (output changes) or `FORMAT_VERSION` (layout changes) in `blob.rs`, update ADR-0100, and re-pin `output_hash_is_pinned` deliberately.

## Don'ts
- Don't add `unsafe` here; extend `meshopt-sys::safe`.
- Don't derive anything from the thread count.
- Don't use libm functions (`sin`, `cos`, …) on build data: their last bit may differ between OSes.

## References
- meshoptimizer (MIT): README, header docs and source on meshlets and cluster bounds (Kapoulkine); its cone test formulas, with the derivation in Akenine-Möller et al., *Real-Time Rendering* 4th ed., §19.3.
- Morton codes for spatial ordering: Karras, "Maximizing Parallelism in the Construction of BVHs, Octrees, and k-d Trees" (HPG 2012).
- Kubisch, "Introduction to Turing Mesh Shaders" (NVIDIA 2018); Apple, "Transform your geometry with Metal mesh shaders" (WWDC22); Wihlidal, "Optimizing the Graphics Pipeline with Compute" (GDC 2016).
- No Unreal Engine (Nanite) or other non-open-source engine code was consulted.

## Status
M4 phase 4.2 (done): offline meshlets. GPU use is 4.3, culling 4.4, meshlet LODs 4.5.

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
