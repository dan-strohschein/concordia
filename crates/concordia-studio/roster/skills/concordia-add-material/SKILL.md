---
name: concordia-add-material
description: "Creates or edits a layered material (.material.ron) with material_get and material_set, assigns it to entities' MeshRenderer and checks the look and closure cost with a screenshot. Covers slabs, their features, the layering operators, value ranges and how to compare against a reference."
when_to_use: "Use when a task asks for a new material, a look change on existing meshes, or textures wired into a material."
user-invocable: false
---
# Adding a material

Materials are text: `assets/materials/<name>.material.ron`. A material is a set of named **nodes** and a **root**. A node is a `Slab` (one surface: an interface over a medium) or an operator that combines other nodes by name:
- `Vertical(top: "coat", bottom: "paint")`: `top` layered over `bottom` (clear coat, varnish, dust on top). Light reaches the bottom through the top's `transmittance`.
- `Horizontal(a: "metal", b: "rust", mix: 0.3, mix_texture: None)`: side by side; `mix` of the surface is `b` (a mask texture's R multiplies it).
- `Coverage(node: "dirt", coverage: 0.5)`: `node` covers only part of the surface.

Every node must be reachable from the root; names use letters, digits, `_` and `-`. `mcp__concordia__schema_get {"name": "Material"}` lists every field with its range.

## 1. Read, change, write
Read an existing material (M1's older flat files read as one slab named `surface`, with `migrated: true`; saving rewrites them):
`mcp__concordia__material_get {"path": "assets/materials/brushed_steel.material.ron"}`

Change the JSON it returns and send the whole material back. To create one, send it to a new path. For a red paint under a clear coat:
`mcp__concordia__material_set {"path": "assets/materials/red_car_paint.material.ron", "material": {"nodes": {"paint": {"Slab": {"base_color": [0.6, 0.05, 0.04, 1.0], "roughness": 0.45}}, "coat": {"Slab": {"base_color": [0.0, 0.0, 0.0, 1.0], "roughness": 0.08, "transmittance": [1.0, 1.0, 1.0]}}, "coated": {"Vertical": {"top": "coat", "bottom": "paint"}}}, "root": "coated"}}`

It validates the material, writes canonical text, imports it and returns `{"id": "<uuid>", "written": true}`; the renderer picks the change up on the next frame. Writing is not undoable (it is a file), so read before you overwrite. An invalid material is refused with the node and field to fix (e.g. `coat.base_color[0]: 1.5 is outside 0..=1`, or `node "coated" refers to "coat2", which doesn't exist`). Never hand-edit the file and the tool on the same material in one step.

## 2. Slab fields and features
Every field has a default, so send only what differs:
- `base_color` (linear RGBA, not sRGB; alpha is opacity), `metallic` (0 dielectric .. 1 metal), `roughness` (0 mirror .. 1 matte), `specular` (dielectric reflectance; 0.5 is the usual F0 0.04).
- Textures: `base_color_texture`, `metallic_roughness_texture`, `normal_texture` (+ `normal_scale`), `occlusion_texture` (+ `occlusion_strength`), `emissive_texture`, as `"asset:<uuid>"` from importing the image first (`mcp__concordia__asset_import {"path": "assets/textures/paint_normal.png"}`).
- Generated textures: `mcp__concordia__gen_run {"provider": "texture", "request": {"size": 256, "pattern": {"Noise": {"kind": "Worley", "cells": 8, "octaves": 3}}, "colors": [[40, 40, 44, 255], [180, 176, 168, 255]]}, "seed": 2, "path": "assets/textures/stone.png"}` writes a tileable PNG (noise, `Checker` or `Stripes`), imports it and records the recipe; use the returned `id` as `"asset:<id>"`. `mcp__concordia__gen_list {}` gives the full request schema.
- `emissive` (linear RGB) with `emissive_strength`: luminance in **nits**, exposed like any light (it dims with a darker camera and lights the world). Pick it from what it should look like at the scene camera's `exposure_ev100`: nits = brightness on screen × 1.2 × 2^EV (EV 0, a night scene: 1 shows as about 0.83; EV 15, daylight: a sign needs about 39,000). Unlit materials ignore exposure.
- `transmittance`: how much light passes the slab's medium (only matters for a slab on top of another; a clear coat is `[1, 1, 1]` with a black base color).
- Optional features (omit them or send `null` for none):
  - `haze`: `{"roughness": 0.6, "weight": 0.3}`, a second, blurrier highlight;
  - `fuzz`: `{"color": [0.5, 0.5, 0.5], "roughness": 0.8}`, sheen for cloth, velvet and dust;
  - `subsurface`: `{"mean_free_path": [0.01, 0.004, 0.002]}` (metres, at most 0.1), light bleeding under skin, wax or marble;
  - `thin_film`: `{"thickness_nm": 400, "ior": 1.4}`, rainbow interference (soap, oil, anodized metal).
- Material-wide: `alpha_mode` (`"Opaque"`, `{"Mask": {"cutoff": 0.5}}`, `"Blend"`), `double_sided`, `unlit`.

Physically plausible values: dielectrics have `metallic: 0.0`; metals `1.0` with a colored `base_color`; avoid pure black or pure white base colors (except a clear coat's black).

Materials inside imported models: a model (`.gltf`, `.glb` or `.fbx`, e.g. `mcp__concordia__asset_import {"path": "assets/models/crate.fbx"}`) brings each of its materials as a one-slab sub-asset `"asset:<uuid>#material/<name>"` and each image as `"asset:<uuid>#texture/<name>"`. They are rebuilt from the model file on every import, so `material_set` can't change them: write a `.material.ron` (start from the imported slab's values) and assign that instead.

## 3. Assign it
Find the entities (`mcp__concordia__query_run {"with": ["MeshRenderer", "Name"]}`), then in one transaction:
`mcp__concordia__txn_begin {"label": "red paint on crates"}`, for each entity `mcp__concordia__component_patch {"entity": "<id>", "component": "MeshRenderer", "patch": {"material": "asset:<uuid>"}}`, then `mcp__concordia__txn_commit`.

## 4. Verify the look and the cost
`mcp__concordia__capture_screenshot {"width": 1280, "height": 720}` from the scene camera, again with `"view_mode": "normals"` if you used a normal map, and `"view_mode": "closures"` for a layered material (one color per closure count). Feature views (`"haze"`, `"fuzz"`, `"subsurface"`, `"thin_film"`) show where each feature is. Compare with the reference the task gives.

Each slab use is one closure. The platform allows a budget of closures per pixel (`closure_budget` in `mcp__concordia__render_stats {}`; a project may lower it). A material over it is simplified automatically and listed in `downgraded_materials` with what was merged; if yours is, simplify it yourself or tell the director. `render_stats` also confirms draws and GPU ms didn't jump.

## Output contract
In the task comment: the material path and asset id, its nodes (e.g. "coat over paint, 2 closures"), the entities changed, and the screenshot you judged.
