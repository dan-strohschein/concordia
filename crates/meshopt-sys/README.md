# meshopt-sys

Raw FFI to the vendored [meshoptimizer](https://github.com/zeux/meshoptimizer) (v1.3, MIT) for meshlet building and cluster bounds, with a small checked wrapper (`safe`) that validates every limit so callers stay free of `unsafe`. Used by `concordia-cluster`.

See `AGENTS.md` for invariants, tests and references.
