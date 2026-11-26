# concordia-gen

Generated content without API keys. Content providers turn a request and a seed into a file; `gen.run` writes it inside the project, imports it, and records its provenance (provider, version, request, seed and a hash of them) in the asset's sidecar so the same bytes can be made again; it also records the output's BLAKE3, and `gen.restore` remakes missing or changed generated files from their recipes, so large generated content can stay out of git (`examples/m4-canyon`). `canyon::Canyon::M4` is the M4 gate's canyon: its height function, meshes, layout and camera paths. `gen.list` describes the providers and their request schemas. Providers build meshes as GLB (boxes, spheres, capsules, cylinders, tori, extrusions, lathes, seeded heightfields, signed-distance shapes meshed by surface nets, seeded rocks and strips of the M4 canyon), sound effects from a small synthesizer with presets, music from a score of tracks and notes with loop points, and tileable noise and pattern textures as PNG. The `blender` provider runs a project script in the user's own Blender (4.2+, a separate program; ADR-0085) and returns the exported GLB with its objects and actions. `[[gen.plugin]]` adds opt-in providers that run as their own programs. Enabled per project by `[gen]` in `concordia.toml`:

```toml
[gen]
[gen.blender]
path = "/Applications/Blender.app/Contents/MacOS/Blender"   # optional

[[gen.plugin]]   # optional, never loaded unless listed
id = "my-images"
kind = "texture"
extensions = ["png"]
command = ["python3", "tools/my_images.py"]
```

See `AGENTS.md` for invariants, tests and references.
