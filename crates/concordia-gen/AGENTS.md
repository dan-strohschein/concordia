# concordia-gen: agent guide

## Purpose
Generated content without APIs: the `ContentProvider` interface and registry, the `[gen]` engine module (`gen.list`, `gen.run`: write, import, record provenance in the sidecar), and the procedural providers. 3.7a: meshes as GLB.

## Public entry points
- `ContentProvider` (`id`, `kind`, `version`, `extensions`, `request_schema`, `procedural`, `generate(request, seed) -> Generated`), `GenKind`, `inputs_hash`
- `ProviderRegistry::procedural()` (the `mesh` provider), `with(provider)`
- `GenModule` (`[gen]`): `GenList` / `GenRun` (`GenRunParams { provider, request, seed, path, actor }`)
- `mesh::{MeshRequest, SdfShape, build, MeshProvider}`: box and sphere (shared with `concordia_assets::shapes`), capsule, cylinder, torus, extrude, lathe, heightfield, SDF surface nets
- `glb::write_glb(&MeshData)`

## Invariants
- Same provider, version, request and seed give the same bytes; `inputs_hash` (BLAKE3 of the four) is in the sidecar's `Provenance`, so a file can be made again.
- `gen.run` needs an actor, checks the path's extension and the request before writing anything, writes atomically, then imports through `asset.import` (the same log and events) and records provenance. Studio holds its `path` to the role's write paths (`concordia-mcp`).
- Closed shapes are watertight with outward winding (`Builder::triangle` turns each triangle to agree with its vertices' normals); no degenerate triangles on the axis.
- Requests are validated (positive sizes, at least 3 segments, simple polygons, SDF grids at most 256 points per axis); errors name the field.
- No `unsafe`; providers never panic on bad input.

## How to test
`cargo nextest run -p concordia-gen`

## Common tasks
- **Add a provider:** implement `ContentProvider`, register it in `ProviderRegistry::procedural()` (or a plugin seam in 3.7c), give its request a `Reflect` type for `gen.list`'s schema, and test determinism and validation.
- **Add a mesh shape:** a `MeshRequest` variant, a builder in `mesh/` using `Builder`, and a case in `generated_mesh_is_watertight_and_has_consistent_winding` with its hand-derived volume.

## Don'ts
- Don't link or call model APIs: model providers are opt-in plugins only (ADR-0002).
- Don't write outside `gen.run`'s checked path, or skip the import (provenance lives in the sidecar).

## References
- glTF 2.0 specification (Khronos, the GLB container and accessors).
- Surface nets: S. Gibson, "Constrained Elastic Surface Nets" (1998).
- Signed distance functions and the polynomial smooth minimum: Íñigo Quílez's published articles.
- Ear clipping: D. Eberly, "Triangulation by Ear Clipping" (Geometric Tools).
- Value noise with smoothstep interpolation (standard; integer hashing of lattice points).

## Status
M3 phase 3.7a (provider interface, `[gen]`, meshes). 3.7b adds sfx, music and textures; 3.7c the Blender bridge and the plugin seam.

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
