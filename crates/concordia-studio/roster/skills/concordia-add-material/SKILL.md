---
name: concordia-add-material
description: "Creates a PBR material as a .material.ron file, imports it, assigns it to entities' MeshRenderer and checks the look with a screenshot. Covers the material fields, value ranges and how to compare against a reference."
when_to_use: "Use when a task asks for a new material, a look change on existing meshes, or textures wired into a material."
user-invocable: false
---
# Adding a material

Materials are text: `assets/materials/<name>.material.ron`, a metallic-roughness material (glTF 2.0 core). Every field has a default, so write only what differs.

## 1. Write the file
```ron
(
    base_color: (0.7, 0.08, 0.06, 1.0), // linear RGBA, not sRGB
    metallic: 0.0,                      // 0 dielectric .. 1 metal
    roughness: 0.6,                     // 0 mirror .. 1 matte
)
```
Other fields: `base_color_texture`, `metallic_roughness_texture`, `normal_texture` (+ `normal_scale`), `occlusion_texture` (+ `occlusion_strength`), `emissive` (linear RGB) with `emissive_strength`, `alpha_mode` (`Opaque`, `Mask(cutoff: 0.5)`, `Blend`), `double_sided`, `unlit`. Textures are asset references (`"asset:<uuid>"`) from importing the image first. `mcp__concordia__schema_get {"name": "MaterialData"}` lists them all with ranges.

Physically plausible values: dielectrics have `metallic: 0.0`; metals `1.0` with a colored `base_color`; avoid pure black or pure white base colors.

## 2. Import it
`mcp__concordia__asset_import {"path": "assets/materials/red_painted.material.ron"}` returns its asset id. Re-importing after an edit keeps the same id.

## 3. Assign it
Find the entities (`mcp__concordia__query_run {"with": ["MeshRenderer", "Name"]}`), then in one transaction:
`mcp__concordia__txn_begin {"label": "red paint on crates"}`, for each entity `mcp__concordia__component_patch {"entity": "<id>", "component": "MeshRenderer", "patch": {"material": "asset:<uuid>"}}`, then `mcp__concordia__txn_commit`.

## 4. Verify the look
`mcp__concordia__capture_screenshot {"width": 1280, "height": 720}` from the scene camera, and again with `"view_mode": "normals"` if you used a normal map. Compare with the reference the task gives. `mcp__concordia__render_stats {}` confirms the cost didn't jump (draws, GPU ms).

## Output contract
In the task comment: the material path and asset id, the entities changed, and the screenshot you judged.
