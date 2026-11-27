# concordia-bvh: agent guide

## Purpose
Bounding-volume hierarchies for ray queries on the CPU: a binned-SAH tree over boxes, triangle meshes over it, and a two-level structure of instanced meshes, with refit. Shared by the path tracer (its own instances) and the renderer's GI (M4 4.8).

## Public entry points
- `Tree` (build over boxes, `refit`, `walk`), `Node` (the flattened layout the GPU reads).
- `Bvh` (triangles: `build`, `refit`, `intersect`, `occluded`).
- `Accel` / `Instance` / `InstanceHit` (two levels: `build`, `set_transforms`, `intersect`, `occluded`).
- `Triangle`, `Ray`, `Hit`, `BvhError`.
- GPU: `GpuBvh::pack` (the row layout) and `shaders/bvh.slang` (`bvh_from_rows`, `bvh_intersect`, `bvh_walk_mesh`, `bvh_triangle`; add `shader_dir()` to the include dirs). The path tracer and the renderer's GI both trace through it.

## Invariants
- Nodes are flattened depth-first: an inner node (`count == 0`) has its left child next and its right child at `first`; a leaf holds `count` primitives from `order[first..]`. The GPU traversal depends on this layout.
- Builds are deterministic (ties broken by primitive index).
- Refit keeps the tree's shape and gives the same hits as a rebuild (tested against brute force); it never changes the primitive count or order. Bad input returns `BvhError`, never panics.
- Depends only on `concordia-core` (no GPU, no engine crates); its GPU test uses the RHI as a dev-dependency.
- `bvh.slang` is the CPU traversal step for step; `gpu_bvh_matches_cpu_hits` holds them together. Instances are `INSTANCE_ROWS` (4) rows: world → object, then `[mesh, …]`; callers keep per-instance shading in their own buffer.

## How to test
`cargo nextest run -p concordia-bvh` (property tests against brute force; the GPU test runs on Metal, required with `CONCORDIA_REQUIRE_GPU=1`; the path tracer's transport suites cover it too).

## Common tasks
- Moving objects: `Accel::set_transforms` for the instance level; `Bvh::refit` for a deformed mesh with the same triangles.

## Don'ts
- Don't change the node layout without updating every Slang traversal that reads it.
- Don't make the oracle (`concordia-pathtrace`) share a scene instance with the renderer.

## References
- Wald, "On fast Construction of SAH-based Bounding Volume Hierarchies" (IEEE RT 2007): binned SAH.
- Ize, "Robust BVH Ray Traversal" (JCGT 2013): the conservative slab test.
- Möller & Trumbore, "Fast, Minimum Storage Ray-Triangle Intersection" (JGT 1997).
- Pharr, Jakob & Humphreys, *Physically Based Rendering*, 4th ed. (2023): instancing, refitting.
No engine source consulted.

## Status
M4 4.8a: moved from `concordia-pathtrace` (4.1b2), refit added.

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
