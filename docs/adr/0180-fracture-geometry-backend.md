# ADR-0180: Fracture geometry backend
- Status: Accepted (M8 8.1a, 2026-11-28; plan question Q2, decided under the author's M8 autonomy grant)
- Date: 2026-11-28
- Builds on: ADR-0041 (Jolt: the vendored `-sys` pattern), ADR-0012 (M8's scope)

## Context
Fracture cuts a closed mesh into pieces along a seeded pattern of convex cells. The cut has to be robust (no cracks, holes or flipped faces on any input that is closed and manifold), mark the faces it makes so they get the interior material, and give the same bytes for the same inputs on every platform, since `.breakable.ron` pieces are rebuilt from their recipe. The plan asked for a spike between binding a library and writing our own exact-predicate cutter. The author asked for a fast C or C++ library on the `jolt-sys` vendoring path.

## Decision
1. **Manifold v3.5.5 (Apache-2.0), core only,** vendored unmodified under `third_party/manifold/` (`VENDOR.md` lists what was copied). The cross-section module (and its Clipper2 dependency) and everything that isn't the library are left out.
2. **`manifold-sys`** builds it with `cc` (sorted sources, C++17, `-O2` always) plus a small C shim, `shim/concordia_manifold.{h,cpp}`, with one operation: clip a mesh by a convex set of half-spaces (`cm_clip_convex`), and accessors to copy the piece out. The bindings are hand-written (a few functions) and a test checks them against a blake3 of the shim header. A checked wrapper, `manifold_sys::safe::clip_convex`, validates input and copies results out, so callers need no `unsafe`.
3. **Determinism:** `MANIFOLD_PAR=-1` (sequential; no TBB or thread pool), `-ffp-contract=off` (no fused multiply-add, so ARM and x86 compute the same floats). The shim catches every C++ exception and returns a status; nothing calls back into Rust.
4. **Interior faces** are the triangles whose run doesn't come from the source mesh (`AsOriginal` gives the source an ID; faces from a trim plane have another).
5. **`concordia-fracture`** owns patterns and pieces. Patterns build cells with arithmetic, `sqrt` and `libm` only (no platform trigonometry), so the planes are the same everywhere. Neighbouring cells share each cutting plane exactly negated, so pieces meet without gaps.

## Consequences
- Pieces are closed and manifold by construction, the source volume is kept within 1e-4, and the same seed gives the same bytes (`concordia-fracture`'s tests).
- A Voronoi cell is clipped by the bisector against every other site: O(n²) planes for n cells, which is fine for authoring at today's counts (at most 1,024 cells). Pruning far sites is a later speed-up.
- Cross-platform byte equality rests on the build flags; Linux CI runs the same tests. If it ever drifts, the fallback is our own exact-predicate cutter (below).
- Upgrading Manifold means replacing `third_party/manifold`, updating `VENDOR.md` and running the fracture tests on macOS and Linux.

## Alternatives considered
- **Our own exact-predicate cutter** (convex cells clipped with Shewchuk-style predicates): fully ours and deterministic, but weeks of work to reach Manifold's robustness on concave and nearly degenerate inputs. Kept as the fallback.
- **CGAL's Nef polyhedra / corefinement:** robust, but GPL or commercial for these packages, and far heavier.
- **libigl / Cork:** libigl's booleans wrap CGAL; Cork isn't maintained and isn't robust on coplanar faces.
- **Manifold with TBB:** faster on big meshes, but parallel reductions can reorder floating-point sums. Fracture is an authoring step, so sequential is fast enough.
