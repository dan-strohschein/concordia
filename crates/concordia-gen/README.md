# concordia-gen

Generated content without API keys. Content providers turn a request and a seed into a file; `gen.run` writes it inside the project, imports it, and records its provenance (provider, version, request, seed and a hash of them) in the asset's sidecar so the same bytes can be made again. `gen.list` describes the providers and their request schemas. The first provider builds meshes as GLB: boxes, spheres, capsules, cylinders, tori, extrusions, lathes, seeded heightfields and signed-distance shapes meshed by surface nets. Enabled per project by `[gen]` in `concordia.toml`.

See `AGENTS.md` for invariants, tests and references.
