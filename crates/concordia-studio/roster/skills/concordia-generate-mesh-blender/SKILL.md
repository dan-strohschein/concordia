---
name: concordia-generate-mesh-blender
description: "Creates meshes and animation clips (needs no API key): procedural meshes from gen_run, or a Blender Python script run headless through the Blender bridge, exported to glTF and imported with provenance."
when_to_use: "Use when a task needs a new 3D model, a variation of one, or a skeletal animation clip."
user-invocable: false
---
# Generating meshes (procedural or Blender)

> **Not available yet:** needs `gen_list` and `gen_run` (M3 phase 3.7). Until then, use built-in primitives through `entity_create` templates.

Units: 1 unit = 1 meter, +Y up, pivot at the base center unless the task says otherwise.

## Procedural first
`mcp__concordia__gen_list {}` lists providers. For primitives, extrusions, lathes, heightfields and SDF shapes use the `mesh` provider:
`mcp__concordia__gen_run {"provider": "mesh", "request": {...}, "seed": 1, "path": "assets/models/pillar.glb"}`.
The output gets an `.import.ron` sidecar with provenance, so it can be regenerated from text.

## Blender for anything else
1. Write a script in `assets/blender/scripts/<name>.py` that builds the object in a fresh scene (no file I/O, no network; the bridge exports for you).
2. Run it: `mcp__concordia__gen_run {"provider": "blender", "request": {"script": "assets/blender/scripts/<name>.py"}, "path": "assets/models/<name>.glb"}`.
3. Read the result: object names, triangle counts, bounds, actions, and Blender's output on failure. Fix the script and rerun; don't hand-edit the `.glb`.

## Check it
Import (`mcp__concordia__asset_import`), place it, `mcp__concordia__capture_screenshot`. Check the scale against a 1 m cube and the triangle budget in the task.

## Output contract
In the task comment: the asset path and id, the provider and seed or script, the triangle count and bounds, and the screenshot.
