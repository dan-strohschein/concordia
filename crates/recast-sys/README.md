# recast-sys

The vendored [recastnavigation](https://github.com/recastnavigation/recastnavigation) (Recast and Detour v1.6.0, zlib) behind a narrow C shim, with the safe API Concordia's navigation uses: build a navmesh tile from triangles and area marks, assemble a tiled Detour navmesh, and query it (nearest point, paths, surface raycasts). It is the only navigation crate with `unsafe` (ADR-0077).

See `AGENTS.md` for invariants, tests and references.
