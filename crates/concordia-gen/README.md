# concordia-gen

Generated content without API keys. Content providers turn a request and a seed into a file; `gen.run` writes it inside the project, imports it, and records its provenance (provider, version, request, seed and a hash of them) in the asset's sidecar so the same bytes can be made again. `gen.list` describes the providers and their request schemas. Providers build meshes as GLB (boxes, spheres, capsules, cylinders, tori, extrusions, lathes, seeded heightfields and signed-distance shapes meshed by surface nets), sound effects from a small synthesizer with presets, music from a score of tracks and notes with loop points, and tileable noise and pattern textures as PNG. The `blender` provider runs a project script in the user's own Blender (4.2+, a separate program; ADR-0085) and returns the exported GLB with its objects and actions. `[[gen.plugin]]` adds opt-in providers that run as their own programs. Enabled per project by `[gen]` in `concordia.toml`:

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
