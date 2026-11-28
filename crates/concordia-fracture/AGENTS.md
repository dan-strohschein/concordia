# concordia-fracture: agent guide

## Purpose
Fracture authoring: a closed mesh cut into pieces by a seeded pattern of convex cells. The cutting is `manifold-sys`; this crate owns the patterns, the pieces, the `.breakable.ron` recipe and the `[fracture]` engine module.

## Public entry points
- `fracture(&Mesh, &Pattern, seed) -> Result<Fracture, FractureError>`
- `Pattern`: `Uniform`, `Clustered`, `Radial` (Voronoi), `Slices`, `Bricks`; `Pattern::cells(&Bounds, seed) -> Vec<Cell>`
- `Fracture { pieces }`, `Piece { cell, positions, triangles, materials, volume }`, `Slot { Surface, Interior }`, `Bounds`, `MAX_CELLS`
- `Piece::centre_of_mass()`, `Piece::collider()` (a `ColliderShape::ConvexHull` of its vertices, relative to the centre of mass), `Piece::interior_uvs(metres_per_tile)` (per triangle corner)
- `weld(&Mesh)`: vertices at bit-identical positions merged (imported meshes split them along seams)
- `Breakable { source, pattern, seed, metres_per_tile, interior_material }` (`.breakable.ron`, `EXTENSION`, `FRACTURE_VERSION`); `pieces(&Breakable, &mut AssetDb, &Ddc) -> Stored { fracture, key, cached }`, `cache_key`
- `FractureModule` (`[fracture]`, needs `[assets]`): `fracture.generate {mesh, pattern, seed, path, metres_per_tile, interior_material, actor}` → `{path, pieces, cache_key}`; `fracture.inspect {path}` → `{recipe, pieces, cache_key, cached}`

## Invariants
- The same mesh, pattern and seed give the same bytes. Patterns use `SimRng`, arithmetic, `sqrt` and `libm` only (no platform trigonometry).
- Pieces are closed, manifold and not empty, in cell order; their volumes sum to the source's.
- Neighbouring cells share each cutting plane exactly negated, so pieces meet without gaps.
- Every bad input (empty, open, non-finite mesh; zero or too many cells; out-of-range parameters) is a `FractureError`, never a panic.
- A piece's collider is in piece space: its body sits at the piece's centre of mass. Concave pieces get their convex hull (convex decomposition is later work).
- The recipe is the source of truth; the pieces are derived data in the DDC, keyed by the recipe's canonical text, a blake3 of the source mesh's encoded data and `FRACTURE_VERSION`, and rebuilt when missing. Bump `FRACTURE_VERSION` whenever the cutter's output changes.
- `fracture.generate` cuts before it writes, so a bad request writes nothing; the recipe is one `EditOp::Document` edit (undoable, by its actor), as `graph.apply`'s are. Like every mutating method it needs an actor and an open scene.
- Interior UVs are a world-space planar projection whose basis ignores which side of the face it is, so both sides of a cut line up. Surface faces get zeros until source attributes are carried through the cut.

## How to test
`cargo nextest run -p concordia-fracture`

## Common tasks
- **Add a pattern:** a `Pattern` variant with its validation and cells in `pattern.rs`, plus a test that it partitions the volume.

## Don'ts
- Don't use `sin`, `cos` or other platform maths in patterns.
- Don't call Manifold directly; extend `manifold-sys`'s shim and wrapper.

## References
- Voronoi fracture as convex-cell clipping: Müller, Chentanez and Kim, "Real Time Dynamic Fracture with Volumetric Approximate Convex Decompositions" (SIGGRAPH 2013); the generalised winding number used by the tests: Jacobson, Kavan and Sorkine-Hornung (SIGGRAPH 2013).

## Status
M8 8.1a: the five patterns and the cutter. 8.1b: centre of mass, hull colliders and interior UVs per piece. 8.1c: welding, `.breakable.ron`, the DDC store and `fracture.generate` / `fracture.inspect`. Studio roles get the fracture tools in 8.10.

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
