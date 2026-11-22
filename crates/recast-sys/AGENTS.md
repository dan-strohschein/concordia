# recast-sys: agent guide

## Purpose
Vendored Recast and Detour v1.6.0 (zlib) through Concordia's flat C shim (`shim/concordia_recast.{h,cpp}`), built with `cc` as C++17, **and the only safe API over them** (ADR-0077): no other navigation crate has `unsafe`.

## Public entry points
- `build_tile(&BuildConfig, &TileInput)` → `Option<TileData>` (one tile: rasterize, filter, compact, mark, erode, watershed regions, contours, polygons, detail, Detour data); `BuildConfig`, `TileInput`, `BoxMark`, `Area` (`BLOCKED`, `WALKABLE`, `cost(2..=62)`), `TileData` (`from_bytes`, `bytes`)
- `DtNavMesh` (`new(&NavMeshParams)`, `add_tile`, `tiles` → `TileInfo`, `poly`, `polys` → `Poly`, `links`); `PolyRef`
- `DtQuery` (`new(Arc<DtNavMesh>, max_nodes)`, `nearest`, `find_path` → `Corridor`, `straight_path`, `raycast` → `RayResult`); `AreaCosts`
- Crowd queries (2.5c.1): `DtQuery::move_along_surface` → `SurfaceMove` (`moveAlongSurface`: slides along walls; the visited polygons, the start's first), `poly_height` (`getPolyHeight`; `None` outside the polygon, the shim's `CR_EMPTY`), `raycast_path` → `RayPath` (`raycast` with the polygons it crossed)
- `RecastError` (`Input`, `Alloc`, `Build(BuildStage)`, `AddTile`, `NotFound`, `Query`), `RECAST_VERSION`, `MAX_VERTS_PER_POLY`, `live_allocations()`, `abi_mismatches()`

## Invariants
- recastnavigation is pinned at v1.6.0 (`third_party/recastnavigation/VENDOR.md`, with every file's SHA-256) and unmodified; only `Recast/` and `Detour/` are vendored. The build never downloads. Compiled with `DT_POLYREF64` (64-bit polygon references, checked by a `static_assert`), `NDEBUG` and `-ffp-contract=off`.
- Marking (the shim's own step): marks apply in order with `CR_AREA_BLOCKED` (1) as a temporary id; then blocked spans become `RC_NULL_AREA`, **before** erosion, so agents keep their radius from blocked cells. `rcMarkBoxArea` skips null spans and includes the cell its upper bound lands on (callers pass half-open boxes).
- Every entry point validates its input (null, counts, indices, finite numbers, ranges) and returns a status code or null instead of crashing; nothing calls back into Rust. A navmesh is only mutated through `&mut DtNavMesh` (adding tiles); `DtQuery` owns its node pool and keeps its navmesh alive through an `Arc`.
- Every Recast/Detour heap block goes through counted allocators (`live_allocations`); `ffi_handles_are_released` checks the count returns to its start after navmeshes, queries, failed builds and refused tiles are dropped.
- `src/ffi.rs` mirrors the header by hand. `tests/smoke.rs` checks the header's blake3 against its `// header blake3:` line, and every struct's size against the C compiler's (`cr_struct_size`).

## How to test
`cargo nextest run -p recast-sys`. The flat-plane values are derived in `tests/smoke.rs`' header (ledge filter, erosion, the one-cell-tall span rule); the crowd queries run on a 20 × 10 m plane cut into two tiles at x = 5 (derived above those tests). It builds at `opt-level = 2` in dev (the crowd's tests call Detour millions of times).

## Common tasks
- **Add a shim function:** declare it in the header, implement and validate it in the `.cpp`, mirror it in `src/ffi.rs`, update the hash line (the test prints it) and `cr_struct_size` for new structs, wrap it safely here, and test it in `tests/smoke.rs`.
- **Upgrade recastnavigation:** replace `third_party/recastnavigation/{Recast,Detour}`, update `VENDOR.md` (tag, commit, SHA-256s) and `RECAST_VERSION` (it is part of the navmesh cache key), and run the nav tests.

## Don'ts
- Don't expose Recast or Detour structs; flatten what Rust needs into `cr_*` structs.
- Don't vendor DetourCrowd: crowds are Terra processors (ADR-0077, N1). Add only the single queries they need.
- Don't add `unsafe` to other navigation crates; extend this one.

## References
- Recast & Detour (zlib), vendored: their headers' documentation and source, RecastDemo's `Sample_TileMesh` build steps (zlib); Mikko Mononen's talks and blog posts (digestingduck.blogspot.com). No Unreal or other non-open source was consulted.

## Status
M2 phase 2.5a (tile build, navmesh, nearest point, paths, surface raycast); 2.5c.1 (surface moves, polygon heights, raycasts with their polygons).

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
