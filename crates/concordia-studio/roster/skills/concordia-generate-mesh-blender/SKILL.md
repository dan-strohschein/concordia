---
name: concordia-generate-mesh-blender
description: "Creates meshes and animation clips (needs no API key): procedural meshes from gen_run, or a Blender Python script run headless through the Blender bridge, exported to glTF and imported with provenance."
when_to_use: "Use when a task needs a new 3D model, a variation of one, or a skeletal animation clip."
user-invocable: false
---
# Generating meshes (procedural or Blender)

Units: 1 unit = 1 meter, +Y up, pivot at the base center unless the task says otherwise.

## Procedural first
`mcp__concordia__gen_list {}` lists providers. For primitives, extrusions, lathes, heightfields and SDF shapes use the `mesh` provider:
`mcp__concordia__gen_run {"provider": "mesh", "request": {...}, "seed": 1, "path": "assets/models/pillar.glb"}`.
`gen_run` writes the file, imports it (the result has its `id` and `sub_assets`) and records provenance in its `.import.ron` sidecar, so it can be made again from text. `mcp__concordia__gen_list {}` gives each provider's request schema, e.g. `{"Lathe": {"profile": [[0.0, 0.0], [0.3, 0.0], [0.25, 3.0], [0.0, 3.0]], "segments": 16}}`.

## Blender for anything else
Blender runs on this machine as its own program (4.2 or newer; `[gen.blender] path` in `concordia.toml` or `BLENDER_PATH` if it isn't found). The engine never links it.
1. Write a script in `assets/blender/scripts/<name>.py` (inside your write paths) that builds the object in the empty scene it starts with: `bpy` and `SEED` (the run's seed, for variations) are in scope. No file I/O and no network; the bridge exports everything as glTF (Y-up) for you.
   ```python
   import bpy
   bpy.ops.mesh.primitive_cylinder_add(radius=0.3, depth=3.0, location=(0, 0, 1.5))
   bpy.context.object.name = "Pillar"
   ```
2. Run it: `mcp__concordia__gen_run {"provider": "blender", "request": {"script": "assets/blender/scripts/<name>.py", "timeout_s": 120}, "seed": 1, "path": "assets/models/<name>.glb"}`.
3. Read the result's `info`: `objects` (name, type, triangles, dimensions in meters), `actions` (each armature action is exported as an animation clip, `clip/<action>`), the Blender version and the script's hash. On failure the error has Blender's exit code and traceback; a script that never finishes is stopped at `timeout_s`, and one that makes nothing is refused. Fix the script and rerun; don't hand-edit the `.glb`.

## Check it
Place it in a preview and take `mcp__concordia__capture_screenshot` (no second `asset_import` is needed: `gen_run` imported it). Check the scale against a 1 m cube and the triangle budget in the task.

## Output contract
In the task comment: the asset path and id, the provider and seed or script, the triangle count and bounds, and the screenshot.
