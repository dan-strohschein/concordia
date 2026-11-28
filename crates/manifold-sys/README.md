# manifold-sys

Raw FFI to Manifold (Apache-2.0), built with `cc` from the vendored core in `third_party/manifold`, through Concordia's small C shim (`shim/`). It exposes one operation, clipping a closed mesh by a convex cell, plus a checked wrapper (`safe::clip_convex`). Use `concordia-fracture` for fracture patterns and pieces.

See `AGENTS.md` for invariants, tests and references.
