# concordia-fracture

Fracture authoring for Concordia (M8): a closed mesh cut into pieces by a seeded pattern. The patterns are uniform, clustered and radial Voronoi cells, planar slices, and bricks. Pieces are closed and manifold, keep the source's volume, and mark cut faces with the interior material. The same inputs give the same bytes.

```rust
use concordia_fracture::{fracture, Pattern};

let pieces = fracture(&mesh, &Pattern::Uniform { cells: 16 }, 42)?;
```

Agents and tools fracture through the Engine API: with `[fracture]` in `concordia.toml`, `fracture.generate` writes a `.breakable.ron` recipe and caches its pieces in the DDC, and `fracture.inspect` summarises them.

See `AGENTS.md` for invariants, tests and references.
